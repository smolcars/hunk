use hunk_codex::protocol::{
    DynamicToolCallOutputContentItem, DynamicToolCallParams, DynamicToolCallResponse,
};
use hunk_terminal::{TerminalCellSnapshot, TerminalScreenSnapshot};
use serde_json::json;

pub(crate) use super::ai_terminal_safety::{
    SensitiveTerminalAction, TerminalToolSafetyMode, classify_terminal_request,
    redact_terminal_tool_text, terminal_dynamic_tool_confirmation,
};
use super::{AiTerminalSessionState, AiTerminalSessionStatus, TerminalTabId, TerminalTabState};

const TERMINAL_CELL_RESPONSE_LIMIT: usize = 2_000;

pub(crate) fn terminal_unavailable_response(
    params: &DynamicToolCallParams,
    message: &str,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": "terminalUnavailable",
        "message": message,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
    }))
}

pub(super) fn terminal_invalid_arguments_response(
    params: &DynamicToolCallParams,
    message: &str,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": "invalidTerminalToolArguments",
        "message": message,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
    }))
}

pub(super) fn terminal_action_rejected_response(
    params: &DynamicToolCallParams,
    message: &str,
) -> DynamicToolCallResponse {
    terminal_error_response(params, "terminalActionRejected", message)
}

pub(super) fn terminal_tab_not_found_response(
    params: &DynamicToolCallParams,
    tab_id: TerminalTabId,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": "terminalTabNotFound",
        "message": format!("Terminal tab {tab_id} does not exist."),
        "tabId": tab_id,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
    }))
}

pub(super) fn terminal_no_active_thread_response(
    params: &DynamicToolCallParams,
) -> DynamicToolCallResponse {
    terminal_error_response(
        params,
        "terminalNoActiveThread",
        "Select an AI thread before using the terminal.",
    )
}

pub(super) fn terminal_no_workspace_response(
    params: &DynamicToolCallParams,
) -> DynamicToolCallResponse {
    terminal_error_response(
        params,
        "terminalNoWorkspace",
        "Open a project workspace before using the terminal.",
    )
}

pub(super) fn terminal_no_shell_session_response(
    params: &DynamicToolCallParams,
) -> DynamicToolCallResponse {
    terminal_error_response(
        params,
        "terminalNoShellSession",
        "No running terminal session is available for this action.",
    )
}

pub(super) fn terminal_confirmation_required_response(
    params: &DynamicToolCallParams,
    kind: SensitiveTerminalAction,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": "terminalConfirmationRequired",
        "message": "This terminal action requires user confirmation before it can run.",
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "sensitiveAction": format!("{kind:?}"),
    }))
}

pub(super) fn terminal_confirmation_declined_response(
    params: &DynamicToolCallParams,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": "terminalConfirmationDeclined",
        "message": "The user declined this terminal action.",
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
    }))
}

pub(super) fn terminal_action_response(
    params: &DynamicToolCallParams,
    active_tab_id: TerminalTabId,
    tabs: &[TerminalTabState],
    action: &str,
    message: &str,
) -> DynamicToolCallResponse {
    json_success_response(json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "action": action,
        "activeTabId": active_tab_id,
        "tabs": terminal_tabs_value(tabs),
        "message": message,
    }))
}

pub(super) fn terminal_tabs_response(
    params: &DynamicToolCallParams,
    active_tab_id: TerminalTabId,
    tabs: &[TerminalTabState],
    message: &str,
) -> DynamicToolCallResponse {
    json_success_response(json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "activeTabId": active_tab_id,
        "tabs": terminal_tabs_value(tabs),
        "message": message,
    }))
}

pub(super) fn terminal_snapshot_response(
    params: &DynamicToolCallParams,
    active_tab_id: TerminalTabId,
    tabs: &[TerminalTabState],
    tab: &TerminalTabState,
    include_cells: bool,
) -> DynamicToolCallResponse {
    let screen = tab.session.screen.as_ref();
    json_success_response(json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "tabId": tab.id,
        "activeTabId": active_tab_id,
        "tabs": terminal_tabs_value(tabs),
        "cwd": tab.session.cwd.as_ref().map(|cwd| cwd.display().to_string()),
        "status": terminal_status_label(tab.session.status),
        "exitCode": tab.session.exit_code,
        "statusMessage": tab.session.status_message.as_deref(),
        "size": screen.map(|screen| json!({
            "rows": screen.rows,
            "cols": screen.cols,
        })),
        "cursor": screen.map(|screen| terminal_cursor_value(screen)),
        "mode": screen.map(|screen| terminal_mode_value(screen)),
        "displayOffset": screen.map(|screen| screen.display_offset),
        "visibleText": screen
            .map(|screen| redact_terminal_tool_text(terminal_visible_text(screen).join("\n").as_str()))
            .unwrap_or_default(),
        "cells": include_cells.then(|| screen.map(|screen| terminal_cells_value(screen))).flatten(),
        "cellsTruncated": include_cells.then(|| {
            screen
                .map(|screen| screen.cells.len() > TERMINAL_CELL_RESPONSE_LIMIT)
                .unwrap_or(false)
        }),
        "message": "Terminal screen snapshot was read.",
    }))
}

pub(super) fn terminal_logs_response(
    params: &DynamicToolCallParams,
    active_tab_id: TerminalTabId,
    tabs: &[TerminalTabState],
    tab: &TerminalTabState,
    since_sequence: Option<u64>,
    limit: usize,
) -> DynamicToolCallResponse {
    let entries = terminal_log_entries(&tab.session, since_sequence, limit);
    let latest_sequence = tab.session.transcript.lines().count() as u64;
    let first_returned_sequence = entries
        .first()
        .and_then(|entry| entry.get("sequence"))
        .and_then(|value| value.as_u64());
    let truncated = first_returned_sequence.is_some_and(|sequence| {
        let first_available = since_sequence.unwrap_or(0).saturating_add(1);
        sequence > first_available
    });

    json_success_response(json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "tabId": tab.id,
        "activeTabId": active_tab_id,
        "tabs": terminal_tabs_value(tabs),
        "entries": entries,
        "latestSequence": latest_sequence,
        "truncated": truncated,
        "status": terminal_status_label(tab.session.status),
        "exitCode": tab.session.exit_code,
        "message": "Terminal transcript logs were read.",
    }))
}

fn json_success_response(value: serde_json::Value) -> DynamicToolCallResponse {
    json_response(value, true)
}

fn terminal_error_response(
    params: &DynamicToolCallParams,
    code: &str,
    message: &str,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": code,
        "message": message,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
    }))
}

fn json_error_response(value: serde_json::Value) -> DynamicToolCallResponse {
    json_response(value, false)
}

fn json_response(value: serde_json::Value, success: bool) -> DynamicToolCallResponse {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|error| {
        format!("{{\"error\":\"terminalResponseSerializationFailed\",\"message\":\"{error}\"}}")
    });
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText { text }],
        success,
    }
}

fn terminal_tabs_value(tabs: &[TerminalTabState]) -> serde_json::Value {
    json!(
        tabs.iter()
            .map(|tab| {
                json!({
                    "tabId": tab.id,
                    "title": tab.title.as_str(),
                    "followOutput": tab.follow_output,
                    "cwd": tab.session.cwd.as_ref().map(|cwd| cwd.display().to_string()),
                    "status": terminal_status_label(tab.session.status),
                    "exitCode": tab.session.exit_code,
                    "statusMessage": tab.session.status_message.as_deref(),
                    "hasScreen": tab.session.screen.is_some(),
                    "transcriptBytes": tab.session.transcript.len(),
                    "pendingInput": tab.pending_input.is_some(),
                    "lastCommand": tab
                        .session
                        .last_command
                        .as_deref()
                        .map(redact_terminal_tool_text),
                })
            })
            .collect::<Vec<_>>()
    )
}

fn terminal_visible_text(screen: &TerminalScreenSnapshot) -> Vec<String> {
    terminal_screen_grid(screen)
        .into_iter()
        .map(|line| line.into_iter().collect::<String>().trim_end().to_string())
        .collect()
}

fn terminal_screen_grid(screen: &TerminalScreenSnapshot) -> Vec<Vec<char>> {
    let rows = usize::from(screen.rows.max(1));
    let cols = usize::from(screen.cols.max(1));
    let first_visible_line = screen
        .cells
        .iter()
        .map(|cell| cell.line)
        .min()
        .unwrap_or(screen.cursor.line.max(0));
    let mut grid = vec![vec![' '; cols]; rows];

    for cell in &screen.cells {
        let relative_line = cell.line - first_visible_line;
        if relative_line < 0 {
            continue;
        }
        let Ok(row_index) = usize::try_from(relative_line) else {
            continue;
        };
        if row_index >= rows || cell.column >= cols || terminal_cell_is_wide_spacer(cell.flags) {
            continue;
        }
        grid[row_index][cell.column] = terminal_render_character(cell.character);
    }

    grid
}

fn terminal_cursor_value(screen: &TerminalScreenSnapshot) -> serde_json::Value {
    json!({
        "line": screen.cursor.line,
        "column": screen.cursor.column,
        "shape": format!("{:?}", screen.cursor.shape),
        "visible": screen.mode.show_cursor,
    })
}

fn terminal_mode_value(screen: &TerminalScreenSnapshot) -> serde_json::Value {
    json!({
        "altScreen": screen.mode.alt_screen,
        "appCursor": screen.mode.app_cursor,
        "appKeypad": screen.mode.app_keypad,
        "showCursor": screen.mode.show_cursor,
        "lineWrap": screen.mode.line_wrap,
        "bracketedPaste": screen.mode.bracketed_paste,
        "focusInOut": screen.mode.focus_in_out,
        "mouseMode": screen.mode.mouse_mode,
        "mouseMotion": screen.mode.mouse_motion,
        "mouseDrag": screen.mode.mouse_drag,
        "sgrMouse": screen.mode.sgr_mouse,
        "utf8Mouse": screen.mode.utf8_mouse,
        "alternateScroll": screen.mode.alternate_scroll,
    })
}

fn terminal_cells_value(screen: &TerminalScreenSnapshot) -> serde_json::Value {
    json!(
        screen
            .cells
            .iter()
            .take(TERMINAL_CELL_RESPONSE_LIMIT)
            .map(terminal_cell_value)
            .collect::<Vec<_>>()
    )
}

fn terminal_cell_value(cell: &TerminalCellSnapshot) -> serde_json::Value {
    json!({
        "line": cell.line,
        "column": cell.column,
        "character": terminal_render_character(cell.character).to_string(),
        "flags": cell.flags,
        "zerowidth": cell.zerowidth.iter().collect::<String>(),
    })
}

fn terminal_log_entries(
    session: &AiTerminalSessionState,
    since_sequence: Option<u64>,
    limit: usize,
) -> Vec<serde_json::Value> {
    let since_sequence = since_sequence.unwrap_or(0);
    let mut entries = session
        .transcript
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let sequence = index as u64 + 1;
            (sequence > since_sequence).then(|| {
                json!({
                    "sequence": sequence,
                    "text": redact_terminal_tool_text(line),
                })
            })
        })
        .collect::<Vec<_>>();
    if entries.len() > limit {
        entries = entries.split_off(entries.len() - limit);
    }
    entries
}

fn terminal_status_label(status: AiTerminalSessionStatus) -> &'static str {
    match status {
        AiTerminalSessionStatus::Idle => "idle",
        AiTerminalSessionStatus::Running => "running",
        AiTerminalSessionStatus::Completed => "completed",
        AiTerminalSessionStatus::Failed => "failed",
        AiTerminalSessionStatus::Stopped => "stopped",
    }
}

fn terminal_render_character(character: char) -> char {
    if character == '\0' || character.is_control() {
        ' '
    } else {
        character
    }
}

fn terminal_cell_is_wide_spacer(flags: u16) -> bool {
    const WIDE_CHAR_SPACER_FLAG: u16 = 0b0000_0000_0100_0000;
    const LEADING_WIDE_CHAR_SPACER_FLAG: u16 = 0b0000_0100_0000_0000;
    flags & (WIDE_CHAR_SPACER_FLAG | LEADING_WIDE_CHAR_SPACER_FLAG) != 0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hunk_codex::protocol::{DynamicToolCallOutputContentItem, DynamicToolCallParams};
    use hunk_terminal::{
        TerminalCellSnapshot, TerminalColorSnapshot, TerminalCursorShapeSnapshot,
        TerminalCursorSnapshot, TerminalDamageSnapshot, TerminalModeSnapshot,
        TerminalNamedColorSnapshot, TerminalScreenSnapshot,
    };

    use super::*;

    #[test]
    fn terminal_logs_response_redacts_secret_tokens() {
        let tab = TerminalTabState {
            id: 1,
            title: "Shell 1".to_string(),
            follow_output: true,
            session: AiTerminalSessionState {
                transcript: "token=abc123 visible\nnormal output\n".to_string(),
                status: AiTerminalSessionStatus::Running,
                ..Default::default()
            },
            pending_input: None,
        };
        let response = terminal_logs_response(
            &terminal_params("logs"),
            1,
            std::slice::from_ref(&tab),
            &tab,
            None,
            100,
        );

        let text = response_text(&response);
        assert!(
            !text.contains("token=abc123"),
            "unexpected response: {text}"
        );
        assert!(text.contains("[redacted]"), "unexpected response: {text}");
        assert!(
            text.contains("normal output"),
            "unexpected response: {text}"
        );
    }

    #[test]
    fn terminal_snapshot_response_redacts_visible_text() {
        let tab = TerminalTabState {
            id: 1,
            title: "Shell 1".to_string(),
            follow_output: true,
            session: AiTerminalSessionState {
                screen: Some(Arc::new(screen_with_text("api_key=abc"))),
                status: AiTerminalSessionStatus::Running,
                ..Default::default()
            },
            pending_input: None,
        };
        let response = terminal_snapshot_response(
            &terminal_params("snapshot"),
            1,
            std::slice::from_ref(&tab),
            &tab,
            false,
        );

        let text = response_text(&response);
        assert!(!text.contains("api_key=abc"), "unexpected response: {text}");
        assert!(text.contains("[redacted]"), "unexpected response: {text}");
    }

    fn terminal_params(tool: &str) -> DynamicToolCallParams {
        DynamicToolCallParams {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            call_id: "call-1".to_string(),
            namespace: Some(hunk_codex::terminal_tools::TERMINAL_TOOL_NAMESPACE.to_string()),
            tool: tool.to_string(),
            arguments: serde_json::json!({}),
        }
    }

    fn screen_with_text(text: &str) -> TerminalScreenSnapshot {
        TerminalScreenSnapshot {
            rows: 1,
            cols: text.chars().count().max(1) as u16,
            display_offset: 0,
            cursor: TerminalCursorSnapshot {
                line: 0,
                column: 0,
                shape: TerminalCursorShapeSnapshot::Block,
            },
            mode: TerminalModeSnapshot {
                show_cursor: true,
                ..Default::default()
            },
            damage: TerminalDamageSnapshot::Full,
            cells: text
                .chars()
                .enumerate()
                .map(|(column, character)| TerminalCellSnapshot {
                    line: 0,
                    column,
                    character,
                    fg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Foreground),
                    bg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Background),
                    flags: 0,
                    zerowidth: Vec::new(),
                })
                .collect(),
        }
    }

    fn response_text(response: &hunk_codex::protocol::DynamicToolCallResponse) -> String {
        response
            .content_items
            .iter()
            .find_map(|item| match item {
                DynamicToolCallOutputContentItem::InputText { text } => Some(text.clone()),
                DynamicToolCallOutputContentItem::InputImage { .. } => None,
            })
            .expect("response should include text")
    }
}
