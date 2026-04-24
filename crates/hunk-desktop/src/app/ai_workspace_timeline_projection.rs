pub(crate) const AI_WORKSPACE_CONTENT_LANE_MAX_WIDTH_PX: usize = 960;
pub(crate) const AI_WORKSPACE_USER_CONTENT_LANE_MAX_WIDTH_PX: usize = 1104;
pub(crate) const AI_WORKSPACE_COMMAND_PREVIEW_MAX_OUTPUT_LINES: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceCommandExecutionDisplayDetails {
    pub(crate) command: String,
    pub(crate) cwd: String,
    pub(crate) process_id: Option<String>,
    pub(crate) status: String,
    pub(crate) action_summaries: Vec<String>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) duration_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceDiffFileSummary {
    pub(crate) path: String,
    pub(crate) added: usize,
    pub(crate) removed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceDiffSummary {
    pub(crate) files: Vec<AiWorkspaceDiffFileSummary>,
    pub(crate) total_added: usize,
    pub(crate) total_removed: usize,
}

pub(crate) fn ai_workspace_item_display_label(kind: &str) -> &str {
    match kind {
        "userMessage" => "User",
        "agentMessage" => "Agent",
        "commandExecution" => "Command",
        "fileChange" => "File Change",
        "plan" => "Plan",
        "reasoning" => "Reasoning",
        "mcpToolCall" => "MCP Tool Call",
        "dynamicToolCall" => "Tool Call",
        "collabAgentToolCall" => "Collab Tool Call",
        "webSearch" => "Web Search",
        "imageView" => "Image View",
        "enteredReviewMode" => "Review Mode Entered",
        "exitedReviewMode" => "Review Mode Exited",
        "contextCompaction" => "Context Compaction",
        _ => kind,
    }
}

fn ai_workspace_browser_tool_action_label(tool: &str) -> Option<&'static str> {
    match tool {
        hunk_codex::browser_tools::BROWSER_NAVIGATE_TOOL => Some("Navigate"),
        hunk_codex::browser_tools::BROWSER_RELOAD_TOOL => Some("Reload"),
        hunk_codex::browser_tools::BROWSER_STOP_TOOL => Some("Stop"),
        hunk_codex::browser_tools::BROWSER_BACK_TOOL => Some("Back"),
        hunk_codex::browser_tools::BROWSER_FORWARD_TOOL => Some("Forward"),
        hunk_codex::browser_tools::BROWSER_SNAPSHOT_TOOL => Some("Snapshot"),
        hunk_codex::browser_tools::BROWSER_CLICK_TOOL => Some("Click"),
        hunk_codex::browser_tools::BROWSER_TYPE_TOOL => Some("Type"),
        hunk_codex::browser_tools::BROWSER_PRESS_TOOL => Some("Press"),
        hunk_codex::browser_tools::BROWSER_SCROLL_TOOL => Some("Scroll"),
        hunk_codex::browser_tools::BROWSER_SCREENSHOT_TOOL => Some("Screenshot"),
        _ => None,
    }
}

pub(crate) fn ai_workspace_item_status_label(
    status: hunk_codex::state::ItemStatus,
) -> &'static str {
    match status {
        hunk_codex::state::ItemStatus::Started => "started",
        hunk_codex::state::ItemStatus::Streaming => "streaming",
        hunk_codex::state::ItemStatus::Completed => "completed",
    }
}

pub(crate) fn ai_workspace_format_header_line(
    title: &str,
    summary: Option<&str>,
    status: Option<&str>,
) -> String {
    let mut sections = Vec::with_capacity(3);
    let title = title.trim();
    if !title.is_empty() {
        sections.push(title.to_string());
    }
    if let Some(summary) = summary.map(str::trim).filter(|value| !value.is_empty()) {
        sections.push(summary.to_string());
    }
    if let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) {
        sections.push(status.to_string());
    }
    sections.join("  ")
}

pub(crate) fn ai_workspace_timeline_item_details_json(
    item: &hunk_codex::state::ItemSummary,
) -> Option<&str> {
    item.display_metadata
        .as_ref()
        .and_then(|metadata| metadata.details_json.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn ai_workspace_timeline_item_details_value(
    item: &hunk_codex::state::ItemSummary,
) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(ai_workspace_timeline_item_details_json(item)?).ok()
}

fn ai_workspace_timeline_item_thread_item(
    item: &hunk_codex::state::ItemSummary,
) -> Option<hunk_codex::protocol::ThreadItem> {
    serde_json::from_str::<hunk_codex::protocol::ThreadItem>(
        ai_workspace_timeline_item_details_json(item)?,
    )
    .ok()
}

pub(crate) fn ai_workspace_command_execution_display_details(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiWorkspaceCommandExecutionDisplayDetails> {
    let details_json = ai_workspace_timeline_item_details_json(item)?;
    let details = serde_json::from_str::<serde_json::Value>(details_json).ok()?;
    let object = details.as_object()?;
    if object.get("kind").and_then(|value| value.as_str()) != Some("commandExecution") {
        return None;
    }

    Some(AiWorkspaceCommandExecutionDisplayDetails {
        command: object
            .get("command")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        cwd: object
            .get("cwd")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        process_id: object
            .get("processId")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        status: object
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        action_summaries: object
            .get("actionSummaries")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|value| value.as_str().map(str::trim))
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        exit_code: object
            .get("exitCode")
            .and_then(|value| value.as_i64())
            .and_then(|value| i32::try_from(value).ok()),
        duration_ms: object.get("durationMs").and_then(|value| value.as_i64()),
    })
}

fn ai_workspace_tool_compact_preview_text(
    item: &hunk_codex::state::ItemSummary,
    content_text: &str,
) -> Option<String> {
    if let Some(details) = ai_workspace_command_execution_display_details(item) {
        return Some(details.command);
    }
    if let Some(summary) = ai_workspace_file_change_summary(item) {
        let first_path = summary.files.first()?.path.clone();
        if summary.files.len() == 1 {
            return Some(first_path);
        }
        return Some(format!(
            "{first_path} (+{} more files)",
            summary.files.len() - 1
        ));
    }

    match ai_workspace_timeline_item_thread_item(item) {
        Some(hunk_codex::protocol::ThreadItem::McpToolCall { server, tool, .. }) => {
            Some(format!("{server} :: {tool}"))
        }
        Some(hunk_codex::protocol::ThreadItem::DynamicToolCall {
            tool,
            arguments,
            content_items,
            ..
        }) => Some(
            ai_workspace_browser_tool_compact_summary(
                tool.as_str(),
                &arguments,
                content_items.as_deref(),
            )
            .unwrap_or(tool),
        ),
        Some(hunk_codex::protocol::ThreadItem::CollabAgentToolCall {
            tool,
            receiver_thread_ids,
            ..
        }) => {
            let receiver_summary = match receiver_thread_ids.len() {
                0 => "no targets".to_string(),
                1 => receiver_thread_ids[0].clone(),
                count => format!("{count} targets"),
            };
            Some(format!("{tool:?} -> {receiver_summary}"))
        }
        _ => content_text
            .lines()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    }
}

fn ai_workspace_tool_summary_is_placeholder(summary: &str) -> bool {
    let trimmed = summary.trim();
    trimmed.is_empty() || !trimmed.chars().any(|ch| ch.is_alphanumeric())
}

pub(crate) fn ai_workspace_tool_header_title(item: &hunk_codex::state::ItemSummary) -> String {
    if let Some(hunk_codex::protocol::ThreadItem::DynamicToolCall { tool, .. }) =
        ai_workspace_timeline_item_thread_item(item)
        && hunk_codex::browser_tools::is_browser_dynamic_tool(tool.as_str())
    {
        return "Browser".to_string();
    }

    item.display_metadata
        .as_ref()
        .and_then(|metadata| metadata.summary.as_deref())
        .filter(|value| !ai_workspace_tool_summary_is_placeholder(value))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| ai_workspace_item_display_label(item.kind.as_str()).to_string())
}

pub(crate) fn ai_workspace_tool_compact_summary(
    item: &hunk_codex::state::ItemSummary,
    content_text: &str,
) -> Option<String> {
    let summary = ai_workspace_tool_compact_preview_text(item, content_text)?;
    let summary = summary.trim();
    if summary.is_empty() {
        return None;
    }

    let title = ai_workspace_tool_header_title(item);
    (summary != title).then(|| summary.to_string())
}

fn ai_workspace_browser_tool_compact_summary(
    tool: &str,
    arguments: &serde_json::Value,
    content_items: Option<&[hunk_codex::protocol::DynamicToolCallOutputContentItem]>,
) -> Option<String> {
    let action = ai_workspace_browser_tool_action_label(tool)?;
    if let Some(confirmation) = ai_workspace_browser_confirmation_summary(content_items) {
        return Some(confirmation);
    }

    let summary = match tool {
        hunk_codex::browser_tools::BROWSER_NAVIGATE_TOOL => arguments
            .get("url")
            .and_then(|value| value.as_str())
            .map(|url| format!("{action} {url}"))
            .unwrap_or_else(|| action.to_string()),
        hunk_codex::browser_tools::BROWSER_RELOAD_TOOL
        | hunk_codex::browser_tools::BROWSER_STOP_TOOL
        | hunk_codex::browser_tools::BROWSER_BACK_TOOL
        | hunk_codex::browser_tools::BROWSER_FORWARD_TOOL => action.to_string(),
        hunk_codex::browser_tools::BROWSER_CLICK_TOOL => arguments
            .get("index")
            .and_then(|value| value.as_u64())
            .map(|index| format!("{action} element #{index}"))
            .unwrap_or_else(|| action.to_string()),
        hunk_codex::browser_tools::BROWSER_TYPE_TOOL => arguments
            .get("index")
            .and_then(|value| value.as_u64())
            .map(|index| format!("{action} into element #{index}"))
            .unwrap_or_else(|| action.to_string()),
        hunk_codex::browser_tools::BROWSER_PRESS_TOOL => arguments
            .get("keys")
            .and_then(|value| value.as_str())
            .map(|keys| format!("{action} {keys}"))
            .unwrap_or_else(|| action.to_string()),
        hunk_codex::browser_tools::BROWSER_SCROLL_TOOL => {
            let direction = if arguments
                .get("down")
                .and_then(|value| value.as_bool())
                .unwrap_or(true)
            {
                "down"
            } else {
                "up"
            };
            let pages = arguments
                .get("pages")
                .and_then(|value| value.as_f64())
                .unwrap_or(1.0);
            format!("{action} {direction} {pages} pages")
        }
        hunk_codex::browser_tools::BROWSER_SNAPSHOT_TOOL
        | hunk_codex::browser_tools::BROWSER_SCREENSHOT_TOOL => action.to_string(),
        _ => return None,
    };
    Some(summary)
}

fn ai_workspace_browser_confirmation_summary(
    content_items: Option<&[hunk_codex::protocol::DynamicToolCallOutputContentItem]>,
) -> Option<String> {
    let items = content_items?;
    items.iter().find_map(|item| {
        let hunk_codex::protocol::DynamicToolCallOutputContentItem::InputText { text } = item
        else {
            return None;
        };
        let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
        if value.get("error").and_then(|value| value.as_str())
            != Some("browserConfirmationRequired")
        {
            return None;
        }
        let sensitive_action = value
            .get("sensitiveAction")
            .and_then(|value| value.as_str())
            .unwrap_or("action");
        Some(format!("Confirmation required: {sensitive_action}"))
    })
}

fn ai_workspace_diff_summary_push_file(
    summary: &mut AiWorkspaceDiffSummary,
    path: String,
    added: usize,
    removed: usize,
) {
    if path.trim().is_empty() {
        return;
    }

    if let Some(existing) = summary.files.iter_mut().find(|file| file.path == path) {
        existing.added = existing.added.saturating_add(added);
        existing.removed = existing.removed.saturating_add(removed);
    } else {
        summary.files.push(AiWorkspaceDiffFileSummary {
            path,
            added,
            removed,
        });
    }
    summary.total_added = summary.total_added.saturating_add(added);
    summary.total_removed = summary.total_removed.saturating_add(removed);
}

fn ai_workspace_file_change_summary_from_details_value(
    details: &serde_json::Value,
) -> Option<AiWorkspaceDiffSummary> {
    if details.get("kind").and_then(|value| value.as_str()) != Some("fileChangeSummary") {
        return None;
    }

    let mut summary = AiWorkspaceDiffSummary {
        files: Vec::new(),
        total_added: 0,
        total_removed: 0,
    };
    let changes = details.get("changes")?.as_array()?;
    for change in changes {
        let path = change
            .get("path")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("changes")
            .to_string();
        let added = change
            .get("added")
            .and_then(|value| value.as_u64())
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0);
        let removed = change
            .get("removed")
            .and_then(|value| value.as_u64())
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0);
        ai_workspace_diff_summary_push_file(&mut summary, path, added, removed);
    }

    (!summary.files.is_empty()).then_some(summary)
}

pub(crate) fn ai_workspace_file_change_summary(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiWorkspaceDiffSummary> {
    if let Some(details) = ai_workspace_timeline_item_details_value(item)
        && let Some(summary) = ai_workspace_file_change_summary_from_details_value(&details)
    {
        return Some(summary);
    }

    let hunk_codex::protocol::ThreadItem::FileChange { changes, .. } =
        ai_workspace_timeline_item_thread_item(item)?
    else {
        return None;
    };

    let mut summary = AiWorkspaceDiffSummary {
        files: Vec::new(),
        total_added: 0,
        total_removed: 0,
    };
    for change in changes {
        let path = change.path.trim();
        let resolved_path = if path.is_empty() {
            "changes".to_string()
        } else {
            path.to_string()
        };
        let (added, removed) = hunk_codex::diff_stats::file_update_change_line_counts(&change);
        ai_workspace_diff_summary_push_file(&mut summary, resolved_path, added, removed);
    }

    (!summary.files.is_empty()).then_some(summary)
}

fn ai_workspace_turn_diff_file_header_paths(line: &str) -> Option<(String, String)> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("diff"), Some("--git"), Some(old_path), Some(new_path)) => {
            Some((old_path.to_string(), new_path.to_string()))
        }
        _ => None,
    }
}

fn ai_workspace_turn_diff_display_path(old_path: &str, new_path: &str) -> String {
    let normalized_new = new_path.strip_prefix("b/").unwrap_or(new_path);
    if normalized_new != "/dev/null" {
        return normalized_new.to_string();
    }

    let normalized_old = old_path.strip_prefix("a/").unwrap_or(old_path);
    if normalized_old != "/dev/null" {
        return normalized_old.to_string();
    }

    "changes".to_string()
}

fn ai_workspace_turn_diff_fallback_file(
    files: &mut Vec<AiWorkspaceDiffFileSummary>,
) -> &mut AiWorkspaceDiffFileSummary {
    if files.is_empty() {
        files.push(AiWorkspaceDiffFileSummary {
            path: "changes".to_string(),
            added: 0,
            removed: 0,
        });
    }

    files
        .last_mut()
        .expect("fallback diff file must exist after initialization")
}

pub(crate) fn ai_workspace_turn_diff_summary(diff_text: &str) -> AiWorkspaceDiffSummary {
    let mut files = Vec::new();
    let mut total_added = 0usize;
    let mut total_removed = 0usize;

    for line in diff_text.lines() {
        if let Some((old_path, new_path)) = ai_workspace_turn_diff_file_header_paths(line) {
            files.push(AiWorkspaceDiffFileSummary {
                path: ai_workspace_turn_diff_display_path(old_path.as_str(), new_path.as_str()),
                added: 0,
                removed: 0,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("+++ ") {
            let path = path.strip_prefix("b/").unwrap_or(path);
            let file = ai_workspace_turn_diff_fallback_file(&mut files);
            if file.path == "changes" && path != "/dev/null" {
                file.path = path.to_string();
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("--- ") {
            let path = path.strip_prefix("a/").unwrap_or(path);
            let file = ai_workspace_turn_diff_fallback_file(&mut files);
            if file.path == "changes" && path != "/dev/null" {
                file.path = path.to_string();
            }
            continue;
        }

        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }

        if line.starts_with('+') {
            let file = ai_workspace_turn_diff_fallback_file(&mut files);
            file.added = file.added.saturating_add(1);
            total_added = total_added.saturating_add(1);
            continue;
        }

        if line.starts_with('-') {
            let file = ai_workspace_turn_diff_fallback_file(&mut files);
            file.removed = file.removed.saturating_add(1);
            total_removed = total_removed.saturating_add(1);
        }
    }

    if files.is_empty() && !diff_text.trim().is_empty() {
        files.push(AiWorkspaceDiffFileSummary {
            path: "changes".to_string(),
            added: total_added,
            removed: total_removed,
        });
    }

    AiWorkspaceDiffSummary {
        files,
        total_added,
        total_removed,
    }
}

pub(crate) fn ai_workspace_diff_summary_preview(summary: &AiWorkspaceDiffSummary) -> String {
    const AI_WORKSPACE_DIFF_VISIBLE_FILE_LIMIT: usize = 4;

    if summary.files.is_empty() {
        return String::new();
    }

    let mut lines = summary
        .files
        .iter()
        .take(AI_WORKSPACE_DIFF_VISIBLE_FILE_LIMIT)
        .map(|file| format!("Edited {}  +{} -{}", file.path, file.added, file.removed))
        .collect::<Vec<_>>();
    let hidden_file_count = summary
        .files
        .len()
        .saturating_sub(AI_WORKSPACE_DIFF_VISIBLE_FILE_LIMIT);
    if hidden_file_count > 0 {
        lines.push(format!("+{hidden_file_count} more files"));
    }
    let file_count_label = if summary.files.len() == 1 {
        "1 file changed".to_string()
    } else {
        format!("{} files changed", summary.files.len())
    };
    lines.push(format!(
        "{file_count_label}, +{} -{}",
        summary.total_added, summary.total_removed
    ));
    lines.join("\n")
}

fn ai_workspace_activity_elapsed_label(duration: std::time::Duration) -> String {
    if duration.as_secs() >= 60 {
        let minutes = duration.as_secs() / 60;
        if minutes >= 60 {
            format!("{}h {}m", minutes / 60, minutes % 60)
        } else {
            format!("{minutes}m")
        }
    } else if duration.as_millis() >= 1000 {
        format!("{:.1}s", duration.as_secs_f32())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn ai_workspace_duration_ms_label(duration_ms: Option<i64>) -> Option<String> {
    let duration_ms = duration_ms?;
    let millis = u64::try_from(duration_ms).ok()?;
    Some(ai_workspace_activity_elapsed_label(
        std::time::Duration::from_millis(millis),
    ))
}

pub(crate) fn ai_workspace_command_execution_terminal_text(
    details: &AiWorkspaceCommandExecutionDisplayDetails,
    output: &str,
    max_output_lines: Option<usize>,
) -> (String, bool) {
    let mut sections = Vec::<String>::new();
    sections.push(format!("# cwd: {}", details.cwd));

    let mut meta = Vec::<String>::new();
    if let Some(process_id) = details.process_id.as_ref() {
        meta.push(format!("pid: {process_id}"));
    }
    if let Some(exit_code) = details.exit_code {
        meta.push(format!("exit: {exit_code}"));
    }
    if let Some(duration) = ai_workspace_duration_ms_label(details.duration_ms) {
        meta.push(format!("duration: {duration}"));
    }
    if !meta.is_empty() {
        sections.push(format!("# {}", meta.join(" | ")));
    }
    for summary in &details.action_summaries {
        sections.push(format!("# {summary}"));
    }

    if !sections.is_empty() {
        sections.push(String::new());
    }

    let mut command_lines = details.command.lines();
    if let Some(first_line) = command_lines.next() {
        sections.push(format!("$ {first_line}"));
        for line in command_lines {
            sections.push(format!("> {line}"));
        }
    }

    let trimmed_output = output.trim_end_matches('\n');
    if trimmed_output.is_empty() {
        return (sections.join("\n"), false);
    }

    sections.push(String::new());
    let output_lines = trimmed_output.lines().collect::<Vec<_>>();
    let truncated = max_output_lines.is_some_and(|max_lines| output_lines.len() > max_lines);
    let preview_lines = output_lines
        .iter()
        .take(max_output_lines.unwrap_or(usize::MAX))
        .copied()
        .collect::<Vec<_>>();
    sections.push(preview_lines.join("\n"));
    if truncated {
        let visible_line_limit =
            max_output_lines.unwrap_or(AI_WORKSPACE_COMMAND_PREVIEW_MAX_OUTPUT_LINES);
        sections.push(String::new());
        sections.push(format!(
            "... output truncated to the first {visible_line_limit} lines ..."
        ));
    }

    (sections.join("\n"), truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser_tool_item(
        tool: &str,
        arguments: serde_json::Value,
        content_items: Option<Vec<hunk_codex::protocol::DynamicToolCallOutputContentItem>>,
    ) -> hunk_codex::state::ItemSummary {
        let thread_item = hunk_codex::protocol::ThreadItem::DynamicToolCall {
            id: "call-1".to_string(),
            namespace: None,
            tool: tool.to_string(),
            arguments,
            status: hunk_codex::protocol::DynamicToolCallStatus::Completed,
            content_items,
            success: Some(true),
            duration_ms: Some(12),
        };

        hunk_codex::state::ItemSummary {
            id: "item-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            kind: "dynamicToolCall".to_string(),
            status: hunk_codex::state::ItemStatus::Completed,
            content: String::new(),
            display_metadata: Some(hunk_codex::state::ItemDisplayMetadata {
                summary: Some("Called tool".to_string()),
                details_json: serde_json::to_string(&thread_item).ok(),
            }),
            last_sequence: 1,
        }
    }

    #[test]
    fn browser_dynamic_tool_rows_use_browser_title() {
        let item = browser_tool_item(
            hunk_codex::browser_tools::BROWSER_SNAPSHOT_TOOL,
            serde_json::json!({}),
            None,
        );

        assert_eq!(ai_workspace_tool_header_title(&item), "Browser");
    }

    #[test]
    fn browser_navigation_tool_rows_summarize_url() {
        let item = browser_tool_item(
            hunk_codex::browser_tools::BROWSER_NAVIGATE_TOOL,
            serde_json::json!({ "url": "https://example.com" }),
            None,
        );

        assert_eq!(
            ai_workspace_tool_compact_summary(&item, ""),
            Some("Navigate https://example.com".to_string())
        );
    }

    #[test]
    fn browser_confirmation_tool_rows_summarize_required_confirmation() {
        let item = browser_tool_item(
            hunk_codex::browser_tools::BROWSER_NAVIGATE_TOOL,
            serde_json::json!({ "url": "mailto:support@example.com" }),
            Some(vec![
                hunk_codex::protocol::DynamicToolCallOutputContentItem::InputText {
                    text: serde_json::json!({
                        "error": "browserConfirmationRequired",
                        "sensitiveAction": "ExternalProtocol"
                    })
                    .to_string(),
                },
            ]),
        );

        assert_eq!(
            ai_workspace_tool_compact_summary(&item, ""),
            Some("Confirmation required: ExternalProtocol".to_string())
        );
    }
}
