use gpui::{
    FontStyle, FontWeight, HighlightStyle, StrikethroughStyle, StyledText, UnderlineStyle,
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiTimelineItemRole {
    User,
    Assistant,
    Tool,
}

const AI_TIMELINE_CONTENT_LANE_MAX_WIDTH: f32 = 960.0;
const AI_TIMELINE_USER_CONTENT_LANE_MAX_WIDTH: f32 = 1104.0;

struct AiTimelineRowRenderMetrics {
    window_started_at: std::time::Instant,
    row_render_count: usize,
    message_row_count: usize,
    tool_row_count: usize,
    group_row_count: usize,
    diff_row_count: usize,
    pending_row_count: usize,
    animated_row_count: usize,
    total_row_render_micros: u128,
}

impl AiTimelineRowRenderMetrics {
    fn new() -> Self {
        Self {
            window_started_at: std::time::Instant::now(),
            row_render_count: 0,
            message_row_count: 0,
            tool_row_count: 0,
            group_row_count: 0,
            diff_row_count: 0,
            pending_row_count: 0,
            animated_row_count: 0,
            total_row_render_micros: 0,
        }
    }
}

static AI_TIMELINE_ROW_RENDER_METRICS:
    std::sync::LazyLock<std::sync::Mutex<AiTimelineRowRenderMetrics>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(AiTimelineRowRenderMetrics::new()));

fn record_ai_timeline_row_render(
    elapsed: std::time::Duration,
    row_kind: &'static str,
    animated: bool,
) {
    let Ok(mut metrics) = AI_TIMELINE_ROW_RENDER_METRICS.lock() else {
        return;
    };
    metrics.row_render_count = metrics.row_render_count.saturating_add(1);
    metrics.total_row_render_micros = metrics
        .total_row_render_micros
        .saturating_add(elapsed.as_micros());
    match row_kind {
        "message" => metrics.message_row_count = metrics.message_row_count.saturating_add(1),
        "tool" => metrics.tool_row_count = metrics.tool_row_count.saturating_add(1),
        "group" => metrics.group_row_count = metrics.group_row_count.saturating_add(1),
        "diff" => metrics.diff_row_count = metrics.diff_row_count.saturating_add(1),
        "pending" => metrics.pending_row_count = metrics.pending_row_count.saturating_add(1),
        _ => {}
    }
    if animated {
        metrics.animated_row_count = metrics.animated_row_count.saturating_add(1);
    }
    if metrics.window_started_at.elapsed() < std::time::Duration::from_secs(1) {
        return;
    }

    let average_row_render_micros = if metrics.row_render_count == 0 {
        0
    } else {
        metrics.total_row_render_micros / metrics.row_render_count as u128
    };
    tracing::debug!(
        "ai timeline row renders/sec={} avg_row_us={} messages={} tools={} groups={} diffs={} pending={} animated={}",
        metrics.row_render_count,
        average_row_render_micros,
        metrics.message_row_count,
        metrics.tool_row_count,
        metrics.group_row_count,
        metrics.diff_row_count,
        metrics.pending_row_count,
        metrics.animated_row_count
    );
    *metrics = AiTimelineRowRenderMetrics::new();
}

struct AiCommandExecutionDisplayDetails {
    command: String,
    cwd: String,
    process_id: Option<String>,
    status: String,
    action_summaries: Vec<String>,
    exit_code: Option<i32>,
    duration_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTurnDiffFileSummary {
    path: String,
    added: usize,
    removed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTurnDiffSummary {
    files: Vec<AiTurnDiffFileSummary>,
    total_added: usize,
    total_removed: usize,
}

fn ai_timeline_item_role(kind: &str) -> AiTimelineItemRole {
    match kind {
        "userMessage" => AiTimelineItemRole::User,
        "agentMessage" | "plan" => AiTimelineItemRole::Assistant,
        _ => AiTimelineItemRole::Tool,
    }
}

fn ai_timeline_item_is_renderable(item: &hunk_codex::state::ItemSummary) -> bool {
    if matches!(item.kind.as_str(), "reasoning" | "webSearch") {
        let has_content = !item.content.trim().is_empty();
        let has_metadata = item.display_metadata.as_ref().is_some_and(|metadata| {
            metadata
                .summary
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                || metadata
                    .details_json
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
        });
        return has_content || has_metadata;
    }

    true
}

fn ai_timeline_row_is_renderable(this: &DiffViewer, row: &AiTimelineRow) -> bool {
    match &row.source {
        AiTimelineRowSource::Item { item_key } => this
            .ai_state_snapshot
            .items
            .get(item_key.as_str())
            .is_some_and(ai_timeline_item_is_renderable),
        AiTimelineRowSource::Group { group_id } => this
            .ai_timeline_group(group_id.as_str())
            .is_some_and(|group| !group.child_row_ids.is_empty()),
        AiTimelineRowSource::TurnDiff { turn_key } => this
            .ai_state_snapshot
            .turn_diffs
            .get(turn_key.as_str())
            .is_some_and(|diff| !diff.trim().is_empty()),
    }
}

fn ai_timeline_item_details_json(item: &hunk_codex::state::ItemSummary) -> Option<&str> {
    item.display_metadata
        .as_ref()
        .and_then(|metadata| metadata.details_json.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn ai_timeline_item_details_value(
    item: &hunk_codex::state::ItemSummary,
) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(ai_timeline_item_details_json(item)?).ok()
}

fn ai_timeline_item_thread_item(
    item: &hunk_codex::state::ItemSummary,
) -> Option<codex_app_server_protocol::ThreadItem> {
    serde_json::from_str::<codex_app_server_protocol::ThreadItem>(
        ai_timeline_item_details_json(item)?,
    )
    .ok()
}

fn ai_command_execution_display_details(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiCommandExecutionDisplayDetails> {
    let details_json = ai_timeline_item_details_json(item)?;
    let details = serde_json::from_str::<serde_json::Value>(details_json).ok()?;
    let object = details.as_object()?;
    if object.get("kind").and_then(|value| value.as_str()) != Some("commandExecution") {
        return None;
    }

    Some(AiCommandExecutionDisplayDetails {
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

fn ai_tool_compact_preview_text(
    item: &hunk_codex::state::ItemSummary,
    content_text: &str,
) -> Option<String> {
    if let Some(details) = ai_command_execution_display_details(item) {
        return Some(details.command);
    }
    if let Some(summary) = ai_file_change_summary(item) {
        let first_path = summary.files.first()?.path.clone();
        if summary.files.len() == 1 {
            return Some(first_path);
        }
        return Some(format!("{first_path} (+{} more files)", summary.files.len() - 1));
    }

    match ai_timeline_item_thread_item(item) {
        Some(codex_app_server_protocol::ThreadItem::McpToolCall { server, tool, .. }) => {
            Some(format!("{server} :: {tool}"))
        }
        Some(codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. }) => Some(tool),
        Some(codex_app_server_protocol::ThreadItem::CollabAgentToolCall {
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

fn ai_tool_summary_is_placeholder(summary: &str) -> bool {
    let trimmed = summary.trim();
    trimmed.is_empty() || !trimmed.chars().any(|ch| ch.is_alphanumeric())
}

fn ai_tool_header_title(item: &hunk_codex::state::ItemSummary) -> String {
    item.display_metadata
        .as_ref()
        .and_then(|metadata| metadata.summary.as_deref())
        .filter(|value| !ai_tool_summary_is_placeholder(value))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| ai_item_display_label(item.kind.as_str()).to_string())
}

fn ai_tool_compact_summary(
    item: &hunk_codex::state::ItemSummary,
    content_text: &str,
) -> Option<String> {
    let summary = ai_tool_compact_preview_text(item, content_text)?;
    let summary = summary.trim();
    if summary.is_empty() {
        return None;
    }

    let title = ai_tool_header_title(item);
    (summary != title).then(|| summary.to_string())
}

#[cfg(test)]
fn ai_tool_header_label(item: &hunk_codex::state::ItemSummary, content_text: &str) -> String {
    let title = ai_tool_header_title(item);
    if title != ai_item_display_label(item.kind.as_str()) {
        return title;
    }

    if let Some(preview_line) = ai_tool_compact_summary(item, content_text) {
        return preview_line;
    }

    title
}

fn ai_duration_ms_label(duration_ms: Option<i64>) -> Option<String> {
    let duration_ms = duration_ms?;
    let millis = u64::try_from(duration_ms).ok()?;
    Some(ai_activity_elapsed_label(std::time::Duration::from_millis(
        millis,
    )))
}

fn ai_turn_diff_file_header_paths(line: &str) -> Option<(String, String)> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("diff"), Some("--git"), Some(old_path), Some(new_path)) => {
            Some((old_path.to_string(), new_path.to_string()))
        }
        _ => None,
    }
}

fn ai_turn_diff_display_path(old_path: &str, new_path: &str) -> String {
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

fn ai_turn_diff_fallback_file(files: &mut Vec<AiTurnDiffFileSummary>) -> &mut AiTurnDiffFileSummary {
    if files.is_empty() {
        files.push(AiTurnDiffFileSummary {
            path: "changes".to_string(),
            added: 0,
            removed: 0,
        });
    }

    files
        .last_mut()
        .expect("fallback diff file must exist after initialization")
}

fn ai_turn_diff_summary(diff_text: &str) -> AiTurnDiffSummary {
    let mut files = Vec::new();
    let mut total_added = 0usize;
    let mut total_removed = 0usize;

    for line in diff_text.lines() {
        if let Some((old_path, new_path)) = ai_turn_diff_file_header_paths(line) {
            files.push(AiTurnDiffFileSummary {
                path: ai_turn_diff_display_path(old_path.as_str(), new_path.as_str()),
                added: 0,
                removed: 0,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("+++ ") {
            let path = path.strip_prefix("b/").unwrap_or(path);
            let file = ai_turn_diff_fallback_file(&mut files);
            if file.path == "changes" && path != "/dev/null" {
                file.path = path.to_string();
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("--- ") {
            let path = path.strip_prefix("a/").unwrap_or(path);
            let file = ai_turn_diff_fallback_file(&mut files);
            if file.path == "changes" && path != "/dev/null" {
                file.path = path.to_string();
            }
            continue;
        }

        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }

        if line.starts_with('+') {
            let file = ai_turn_diff_fallback_file(&mut files);
            file.added = file.added.saturating_add(1);
            total_added = total_added.saturating_add(1);
            continue;
        }

        if line.starts_with('-') {
            let file = ai_turn_diff_fallback_file(&mut files);
            file.removed = file.removed.saturating_add(1);
            total_removed = total_removed.saturating_add(1);
        }
    }

    if files.is_empty() && !diff_text.trim().is_empty() {
        files.push(AiTurnDiffFileSummary {
            path: "changes".to_string(),
            added: total_added,
            removed: total_removed,
        });
    }

    AiTurnDiffSummary {
        files,
        total_added,
        total_removed,
    }
}

fn ai_diff_summary_push_file(
    summary: &mut AiTurnDiffSummary,
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
        summary.files.push(AiTurnDiffFileSummary {
            path,
            added,
            removed,
        });
    }
    summary.total_added = summary.total_added.saturating_add(added);
    summary.total_removed = summary.total_removed.saturating_add(removed);
}

fn ai_file_change_summary_from_details_value(
    details: &serde_json::Value,
) -> Option<AiTurnDiffSummary> {
    if details.get("kind").and_then(|value| value.as_str()) != Some("fileChangeSummary") {
        return None;
    }

    let mut summary = AiTurnDiffSummary {
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
        ai_diff_summary_push_file(&mut summary, path, added, removed);
    }

    (!summary.files.is_empty()).then_some(summary)
}

fn ai_file_change_summary(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiTurnDiffSummary> {
    if let Some(details) = ai_timeline_item_details_value(item)
        && let Some(summary) = ai_file_change_summary_from_details_value(&details)
    {
        return Some(summary);
    }

    let codex_app_server_protocol::ThreadItem::FileChange { changes, .. } =
        ai_timeline_item_thread_item(item)?
    else {
        return None;
    };

    let mut summary = AiTurnDiffSummary {
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
        ai_diff_summary_push_file(&mut summary, resolved_path, added, removed);
    }

    (!summary.files.is_empty()).then_some(summary)
}

fn ai_file_change_group_summary(
    this: &DiffViewer,
    group: &AiTimelineGroup,
) -> Option<AiTurnDiffSummary> {
    let mut summary = AiTurnDiffSummary {
        files: Vec::new(),
        total_added: 0,
        total_removed: 0,
    };

    for child_row_id in &group.child_row_ids {
        let row = this.ai_timeline_row(child_row_id.as_str())?;
        let AiTimelineRowSource::Item { item_key } = &row.source else {
            continue;
        };
        let item = this.ai_state_snapshot.items.get(item_key.as_str())?;
        let Some(item_summary) = ai_file_change_summary(item) else {
            continue;
        };
        for file in item_summary.files {
            ai_diff_summary_push_file(&mut summary, file.path, file.added, file.removed);
        }
    }

    (!summary.files.is_empty()).then_some(summary)
}

#[allow(clippy::too_many_arguments)]
fn ai_tool_detail_section(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    surface_id: impl Into<String>,
    selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
    title: &str,
    content: String,
    mono: bool,
    max_height: Option<gpui::Pixels>,
    scroll_x: bool,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let surface_id = surface_id.into();
    let scroll_region_id = format!("{surface_id}-scroll");
    let line_count = content.lines().count().max(1);
    let needs_vertical_scroll = max_height.is_some_and(|max_height| {
        !scroll_x && px((line_count as f32 * 17.0) + 12.0) > max_height
    });

    let container = div()
        .w_full()
        .max_w_full()
        .min_w_0()
        .rounded(px(8.0))
        .border_1()
        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.85, 0.68))
        .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.10, 0.14))
        .overflow_hidden()
        .px_2()
        .py_1p5();

    let mut text = div()
        .w_full()
        .max_w_full()
        .min_w_full()
        .min_w_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .whitespace_normal();
    if mono {
        text = text.font_family(cx.theme().mono_font_family.clone());
    }
    if scroll_x {
        text = text.whitespace_nowrap();
    }
    text = text.child(
        div()
            .w_full()
            .max_w_full()
            .min_w_full()
            .min_w_0()
                .child(ai_render_selectable_styled_text(
                    this,
                    view,
                    row_id,
                    surface_id,
                    selection_surfaces,
                    ai_text_link_ranges(Vec::new()),
                    StyledText::new(content),
                    hunk_text_selection_background(cx.theme(), is_dark),
                )),
    );

    let content = match (max_height, scroll_x, needs_vertical_scroll) {
        (Some(_), false, false) => text.into_any_element(),
        (Some(max_height), true, _) => h_flex()
            .id(scroll_region_id.clone())
            .w_full()
            .max_w_full()
            .min_w_0()
            .max_h(max_height)
            .items_stretch()
            .overflow_scroll()
            .occlude()
            .child(
                div()
                    .w_full()
                    .max_w_full()
                    .min_w_0()
                    .min_h_full()
                    .flex_1()
                    .child(text),
            )
            .into_any_element(),
        (Some(max_height), false, true) => div()
            .w_full()
            .max_w_full()
            .min_w_0()
            .h(max_height)
            .child(
                div()
                    .id(scroll_region_id.clone())
                    .size_full()
                    .overflow_y_scroll()
                    .overflow_x_hidden()
                    .occlude()
                    .child(
                        div()
                            .w_full()
                            .max_w_full()
                            .min_w_full()
                            .min_w_0()
                            .min_h_full()
                            .child(text),
                    ),
            )
            .into_any_element(),
        (None, true, _) => div()
            .id(scroll_region_id)
            .w_full()
            .max_w_full()
            .min_w_0()
            .overflow_x_scroll()
            .occlude()
            .child(text)
            .into_any_element(),
        (None, false, _) => text.into_any_element(),
    };
    let container = container.child(content);

    v_flex()
        .w_full()
        .max_w_full()
        .min_w_0()
        .items_stretch()
        .gap_1()
        .child(
            div()
                .w_full()
                .max_w_full()
                .min_w_0()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .whitespace_nowrap()
                .child(title.to_string()),
        )
        .child(container)
        .into_any_element()
}

const AI_COMMAND_PREVIEW_MAX_OUTPUT_LINES: usize = 40;
const AI_COMMAND_EXECUTION_CARD_MAX_WIDTH: f32 = 940.0;
const AI_COMMAND_EXECUTION_TRANSCRIPT_MIN_WIDTH: f32 = 860.0;
const AI_COMMAND_EXECUTION_TRANSCRIPT_MAX_WIDTH: f32 = 4096.0;
const AI_COMMAND_EXECUTION_MONO_CHAR_WIDTH: f32 = 8.0;

fn ai_command_execution_status_color(
    details: &AiCommandExecutionDisplayDetails,
    cx: &mut Context<DiffViewer>,
) -> Hsla {
    match details.exit_code {
        Some(0) => cx.theme().success,
        Some(_) => cx.theme().danger,
        None => match details.status.as_str() {
            "completed" => cx.theme().success,
            "started" | "running" | "streaming" => cx.theme().accent,
            _ => cx.theme().muted_foreground,
        },
    }
}

fn ai_command_execution_transcript_width(content: &str) -> gpui::Pixels {
    let longest_line_chars = content
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0) as f32;
    let estimated_width =
        (longest_line_chars * AI_COMMAND_EXECUTION_MONO_CHAR_WIDTH) + 24.0;
    px(
        estimated_width
            .clamp(
                AI_COMMAND_EXECUTION_TRANSCRIPT_MIN_WIDTH,
                AI_COMMAND_EXECUTION_TRANSCRIPT_MAX_WIDTH,
            ),
    )
}

fn ai_command_execution_terminal_text(
    details: &AiCommandExecutionDisplayDetails,
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
    if let Some(duration) = ai_duration_ms_label(details.duration_ms) {
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
        let visible_line_limit = max_output_lines.unwrap_or(AI_COMMAND_PREVIEW_MAX_OUTPUT_LINES);
        sections.push(String::new());
        sections.push(format!(
            "... output truncated to the first {visible_line_limit} lines ..."
        ));
    }

    (sections.join("\n"), truncated)
}

fn render_ai_command_execution_details(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    details: &AiCommandExecutionDisplayDetails,
    output: &str,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let terminal_surface_id = ai_timeline_text_surface_id(row_id, "tool-terminal", 0);
    let (preview_text, _truncated) = ai_command_execution_terminal_text(
        details,
        output,
        Some(AI_COMMAND_PREVIEW_MAX_OUTPUT_LINES),
    );
    let (full_text, _) = ai_command_execution_terminal_text(details, output, None);
    let selection_surfaces = ai_text_selection_surfaces(vec![AiTextSelectionSurfaceSpec::new(
        terminal_surface_id.clone(),
        preview_text.clone(),
    )]);
    let rerun_button_id = format!("ai-rerun-command-exec-{}", row_id.replace('\u{1f}', "--"));
    let copy_button_id = format!("ai-copy-command-exec-{}", row_id.replace('\u{1f}', "--"));
    let status_color = ai_command_execution_status_color(details, cx);
    let status_text = details.status.replace('_', " ");
    let transcript_width = ai_command_execution_transcript_width(preview_text.as_str());
    let command_to_rerun = details.command.trim().to_string();
    let command_cwd = (!details.cwd.trim().is_empty()).then(|| std::path::PathBuf::from(details.cwd.clone()));

    div()
        .w_full()
        .min_w_0()
        .max_w(px(AI_COMMAND_EXECUTION_CARD_MAX_WIDTH))
        .rounded(px(10.0))
        .border_1()
        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.88, 0.72))
        .bg(hunk_blend(
            cx.theme().background,
            cx.theme().secondary,
            is_dark,
            0.24,
            0.16,
        ))
        .p_2()
        .child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_xs()
                                .font_semibold()
                                .child("Shell"),
                        )
                        .child(
                            h_flex()
                                .flex_none()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .flex_none()
                                        .rounded(px(999.0))
                                        .border_1()
                                        .border_color(hunk_opacity(status_color, is_dark, 0.84, 0.72))
                                        .bg(hunk_opacity(status_color, is_dark, 0.12, 0.08))
                                        .px_1p5()
                                        .py_0p5()
                                        .text_xs()
                                        .text_color(status_color)
                                        .child(status_text),
                                )
                                .child(
                                    Button::new(rerun_button_id)
                                        .ghost()
                                        .compact()
                                        .rounded(px(7.0))
                                        .icon(Icon::new(IconName::SquareTerminal).size(px(13.0)))
                                        .text_color(cx.theme().muted_foreground)
                                        .min_w(px(22.0))
                                        .h(px(20.0))
                                        .tooltip("Run in terminal")
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.ai_run_command_in_terminal(
                                                        command_cwd.clone(),
                                                        command_to_rerun.clone(),
                                                        cx,
                                                    );
                                                });
                                            }
                                        }),
                                )
                                .child(
                                    Button::new(copy_button_id)
                                        .ghost()
                                        .compact()
                                        .rounded(px(7.0))
                                        .icon(Icon::new(IconName::Copy).size(px(12.0)))
                                        .text_color(cx.theme().muted_foreground)
                                        .min_w(px(22.0))
                                        .h(px(20.0))
                                        .tooltip("Copy command transcript")
                                        .on_click({
                                            let view = view.clone();
                                            move |_, window, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.ai_copy_text_action(
                                                        full_text.clone(),
                                                        "Copied command transcript.",
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }
                                        }),
                                ),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.82, 0.66))
                        .bg(hunk_blend(
                            cx.theme().background,
                            cx.theme().secondary,
                            is_dark,
                            0.38,
                            0.24,
                        ))
                        .p_2()
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .overflow_hidden()
                                .overflow_x_scrollbar()
                                .overflow_y_hidden()
                                .child(
                                    div()
                                        .w(transcript_width)
                                        .min_w(transcript_width)
                                        .text_xs()
                                        .font_family(cx.theme().mono_font_family.clone())
                                        .text_color(cx.theme().foreground)
                                        .child(ai_render_selectable_styled_text(
                                            this,
                                            view,
                                            row_id,
                                            terminal_surface_id,
                                            selection_surfaces,
                                            ai_text_link_ranges(Vec::new()),
                                            StyledText::new(preview_text),
                                            hunk_text_selection_background(cx.theme(), is_dark),
                                        )),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

fn render_ai_compact_diff_summary_row(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    summary: &AiTurnDiffSummary,
    theme: &gpui_component::Theme,
    nested: bool,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    const AI_TURN_DIFF_VISIBLE_FILE_LIMIT: usize = 4;

    let disclosure_colors = hunk_disclosure_row(theme, is_dark);
    let line_stats_colors = hunk_line_stats(theme, is_dark);
    let row_id_string = row_id.to_string();
    let file_count_label = if summary.files.len() == 1 {
        "1 file changed".to_string()
    } else {
        format!("{} files changed", summary.files.len())
    };
    let visible_files = summary
        .files
        .iter()
        .take(AI_TURN_DIFF_VISIBLE_FILE_LIMIT)
        .map(|file| {
            let (file_name, directory) = ai_display_path_parts(file.path.as_str());

            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Edited"),
                )
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .items_baseline()
                        .gap_1p5()
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().accent)
                                .child(file_name),
                        )
                        .when_some(directory, |this, directory| {
                            this.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(directory),
                            )
                        }),
                )
                .child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .gap_1p5()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_xs()
                        .child(
                            div()
                                .text_color(line_stats_colors.added)
                                .child(format!("+{}", file.added)),
                        )
                        .child(
                            div()
                                .text_color(line_stats_colors.removed)
                                .child(format!("-{}", file.removed)),
                        ),
                )
                .into_any_element()
        })
        .collect::<Vec<_>>();
    let hidden_file_count = summary.files.len().saturating_sub(AI_TURN_DIFF_VISIBLE_FILE_LIMIT);

    let row_element = h_flex()
        .w_full()
        .min_w_0()
        .justify_start()
        .child(
            v_flex()
                .w_full()
                .min_w_0()
                .max_w(px(940.0))
                .gap_1()
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .items_stretch()
                        .gap_0p5()
                        .px_2()
                        .py_1p5()
                        .rounded(px(8.0))
                        .hover(move |style| {
                            style
                                .bg(disclosure_colors.hover_background)
                                .cursor_pointer()
                        })
                        .on_mouse_down(MouseButton::Left, {
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.ai_open_review_tab(cx);
                                });
                            }
                        })
                        .children(visible_files)
                        .when(hidden_file_count > 0, |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("+{hidden_file_count} more files")),
                            )
                        })
                        .child(
                            h_flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    h_flex()
                                        .items_center()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(disclosure_colors.title)
                                                .child(file_count_label.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .text_color(line_stats_colors.added)
                                                .child(format!("+{}", summary.total_added)),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .text_color(line_stats_colors.removed)
                                                .child(format!("-{}", summary.total_removed)),
                                        ),
                                )
                                .child(
                                    Icon::new(IconName::ChevronRight)
                                        .size(px(12.0))
                                        .text_color(disclosure_colors.chevron),
                                ),
                        ),
                ),
        );

    let wrapped_row = h_flex()
        .w_full()
        .min_w_0()
        .justify_start()
        .child(
            div()
                .w_full()
                .min_w_0()
                .when(nested, |this| this.pl_4())
                .child(row_element),
        );

    if nested {
        wrapped_row.into_any_element()
    } else {
        ai_timeline_row_with_animation(this, row_id_string.as_str(), wrapped_row)
    }
}

fn ai_display_path_parts(path: &str) -> (String, Option<String>) {
    let normalized = path.trim().trim_end_matches(['/', '\\']);
    if normalized.is_empty() {
        return ("changes".to_string(), None);
    }

    let Some(separator_ix) = normalized.rfind(['/', '\\']) else {
        return (normalized.to_string(), None);
    };
    let file_name = normalized[separator_ix + 1..].trim();
    if file_name.is_empty() {
        return (normalized.to_string(), None);
    }

    let directory = normalized[..separator_ix]
        .trim()
        .to_string();
    (
        file_name.to_string(),
        (!directory.is_empty()).then_some(directory),
    )
}

fn render_ai_turn_diff_row(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row: &AiTimelineRow,
    diff_text: &str,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let summary = ai_turn_diff_summary(diff_text);
    let theme = cx.theme().clone();
    render_ai_compact_diff_summary_row(
        this,
        view,
        row.id.as_str(),
        &summary,
        &theme,
        false,
        is_dark,
        cx,
    )
}

fn render_ai_chat_timeline_row_for_view(
    this: &DiffViewer,
    row_id: &str,
    view: Entity<DiffViewer>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let started_at = std::time::Instant::now();
    if let Some(pending) = this.ai_pending_steer_for_row_id(row_id) {
        let element = render_ai_pending_steer(&pending, is_dark, cx);
        record_ai_timeline_row_render(started_at.elapsed(), "pending", false);
        return element;
    }
    if let Some(queued) = this.ai_queued_message_for_row_id(row_id) {
        let element = render_ai_queued_message(&queued, is_dark, cx);
        record_ai_timeline_row_render(started_at.elapsed(), "pending", false);
        return element;
    }

    let Some(row) = this.ai_timeline_row(row_id) else {
        let element = div().w_full().h(px(0.0)).into_any_element();
        record_ai_timeline_row_render(started_at.elapsed(), "empty", false);
        return element;
    };
    if !ai_timeline_row_is_renderable(this, row) {
        let element = div().w_full().h(px(0.0)).into_any_element();
        record_ai_timeline_row_render(started_at.elapsed(), "empty", false);
        return element;
    }

    let (element, row_kind, animated) = match &row.source {
        AiTimelineRowSource::Item { item_key } => {
            let Some(item) = this.ai_state_snapshot.items.get(item_key.as_str()) else {
                let element = div().w_full().h(px(0.0)).into_any_element();
                record_ai_timeline_row_render(started_at.elapsed(), "empty", false);
                return element;
            };
            let role = ai_timeline_item_role(item.kind.as_str());
            match role {
                AiTimelineItemRole::User | AiTimelineItemRole::Assistant => {
                    let is_user = role == AiTimelineItemRole::User;
                    let role_label = if is_user {
                        "You"
                    } else if item.kind == "plan" {
                        "Plan"
                    } else {
                        "Assistant"
                    };
                    let bubble_max_width = if is_user { px(680.0) } else { px(700.0) };
                    let text_content = item.content.trim();
                    let fallback_summary = item
                        .display_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.summary.as_deref())
                        .unwrap_or_default();
                    let bubble_text = if text_content.is_empty() {
                        fallback_summary
                    } else {
                        text_content
                    };
                    let message_hover_group =
                        format!("ai-message-hover-{}", row.id.replace('\u{1f}', "--"));
                    let copy_message_id =
                        format!("ai-copy-message-{}", row.id.replace('\u{1f}', "--"));
                    let copy_message_text = bubble_text.to_string();

                    let row_element = h_flex()
                        .w_full()
                        .min_w_0()
                        .when(is_user, |this| this.justify_end())
                        .when(!is_user, |this| this.justify_start())
                        .child(
                            v_flex()
                                .group(message_hover_group.clone())
                                .w_full()
                                .max_w(bubble_max_width)
                                .min_w_0()
                                .gap_1p5()
                                .child(
                                    h_flex()
                                        .w_full()
                                        .min_w_0()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            div()
                                                .flex_none()
                                                .whitespace_nowrap()
                                                .text_xs()
                                                .font_semibold()
                                                .child(role_label),
                                        )
                                        .when(!bubble_text.is_empty(), |header| {
                                            let hover_group = message_hover_group.clone();
                                            let view = view.clone();
                                            let message = copy_message_text.clone();
                                            header.child(
                                                div()
                                                    .flex_none()
                                                    .invisible()
                                                    .group_hover(hover_group, |this| this.visible())
                                                    .child(
                                                        Button::new(copy_message_id.clone())
                                                            .ghost()
                                                            .compact()
                                                            .rounded(px(7.0))
                                                            .icon(Icon::new(IconName::Copy).size(px(12.0)))
                                                            .text_color(theme.muted_foreground)
                                                            .min_w(px(22.0))
                                                            .h(px(20.0))
                                                            .tooltip("Copy message")
                                                            .on_click(move |_, window, cx| {
                                                                view.update(cx, |this, cx| {
                                                                    this.ai_copy_message_action(
                                                                        message.clone(),
                                                                        window,
                                                                        cx,
                                                                    );
                                                                });
                                                            }),
                                                    ),
                                            )
                                        })
                                )
                                .when(!bubble_text.is_empty(), |container| {
                                    container.child(ai_render_chat_markdown_message(
                                        this,
                                        view.clone(),
                                        row.id.as_str(),
                                        bubble_text,
                                        &theme,
                                        is_dark,
                                    ))
                                }),
                        );
                    let element = if is_user {
                        ai_timeline_row_with_animation_in_lane(
                            this,
                            row.id.as_str(),
                            row_element,
                            AI_TIMELINE_USER_CONTENT_LANE_MAX_WIDTH,
                        )
                    } else {
                        ai_timeline_row_with_animation(this, row.id.as_str(), row_element)
                    };
                    (element, "message", false)
                }
                AiTimelineItemRole::Tool => {
                    (
                        render_ai_tool_item_row(
                        this,
                        view,
                        row.id.as_str(),
                        item,
                        &theme,
                        is_dark,
                        false,
                        cx,
                        ),
                        "tool",
                        false,
                    )
                }
            }
        }
        AiTimelineRowSource::Group { group_id } => {
            let Some(group) = this.ai_timeline_group(group_id.as_str()) else {
                let element = div().w_full().h(px(0.0)).into_any_element();
                record_ai_timeline_row_render(started_at.elapsed(), "empty", false);
                return element;
            };
            (
                render_ai_timeline_group_row(this, view, row, group, &theme, is_dark, cx),
                "group",
                false,
            )
        }
        AiTimelineRowSource::TurnDiff { turn_key } => {
            let Some(diff) = this.ai_state_snapshot.turn_diffs.get(turn_key.as_str()) else {
                let element = div().w_full().h(px(0.0)).into_any_element();
                record_ai_timeline_row_render(started_at.elapsed(), "empty", false);
                return element;
            };
            let diff_text = diff.trim();
            if diff_text.is_empty() {
                let element = div().w_full().h(px(0.0)).into_any_element();
                record_ai_timeline_row_render(started_at.elapsed(), "empty", false);
                return element;
            }
            (
                render_ai_turn_diff_row(this, view, row, diff_text, is_dark, cx),
                "diff",
                false,
            )
        }
    };
    record_ai_timeline_row_render(started_at.elapsed(), row_kind, animated);
    element
}

fn ai_timeline_row_with_animation(
    this: &DiffViewer,
    row_id: &str,
    row: gpui::Div,
) -> AnyElement {
    ai_timeline_row_with_animation_in_lane(this, row_id, row, AI_TIMELINE_CONTENT_LANE_MAX_WIDTH)
}

fn ai_timeline_row_with_animation_in_lane(
    this: &DiffViewer,
    row_id: &str,
    row: gpui::Div,
    lane_max_width: f32,
) -> AnyElement {
    let _ = (this, row_id);
    let row = h_flex()
        .w_full()
        .min_w_0()
        .justify_center()
        .child(
            div()
                .w_full()
                .max_w(px(lane_max_width))
                .min_w_0()
                .px_1()
                .py_1p5()
                .child(row),
        );
    row.into_any_element()
}
