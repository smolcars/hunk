#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiTimelineGroupKind {
    Exploration,
    Collaboration,
    CommandBatch,
    FileChangeBatch,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct AiExplorationGroupSummary {
    files: usize,
    searches: usize,
    listings: usize,
}

impl AiExplorationGroupSummary {
    fn total(&self) -> usize {
        self.files + self.searches + self.listings
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct AiCollaborationGroupSummary {
    spawned: usize,
    sent_inputs: usize,
    resumed: usize,
    waits: usize,
    closed: usize,
    receiver_thread_ids: BTreeSet<String>,
}

impl AiCollaborationGroupSummary {
    fn target_count(&self) -> usize {
        self.receiver_thread_ids.len()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct AiCommandBatchGroupSummary {
    count: usize,
    preview_commands: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct AiFileChangeBatchGroupSummary {
    operation_count: usize,
    total_files: usize,
    preview_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AiTimelineGroupSummary {
    Exploration(AiExplorationGroupSummary),
    Collaboration(AiCollaborationGroupSummary),
    CommandBatch(AiCommandBatchGroupSummary),
    FileChangeBatch(AiFileChangeBatchGroupSummary),
}

impl AiTimelineGroupSummary {
    fn kind(&self) -> AiTimelineGroupKind {
        match self {
            Self::Exploration(_) => AiTimelineGroupKind::Exploration,
            Self::Collaboration(_) => AiTimelineGroupKind::Collaboration,
            Self::CommandBatch(_) => AiTimelineGroupKind::CommandBatch,
            Self::FileChangeBatch(_) => AiTimelineGroupKind::FileChangeBatch,
        }
    }

    fn merge(&mut self, next: Self) {
        match (self, next) {
            (Self::Exploration(current), Self::Exploration(next)) => {
                current.files += next.files;
                current.searches += next.searches;
                current.listings += next.listings;
            }
            (Self::Collaboration(current), Self::Collaboration(next)) => {
                current.spawned += next.spawned;
                current.sent_inputs += next.sent_inputs;
                current.resumed += next.resumed;
                current.waits += next.waits;
                current.closed += next.closed;
                current
                    .receiver_thread_ids
                    .extend(next.receiver_thread_ids);
            }
            (Self::CommandBatch(current), Self::CommandBatch(next)) => {
                current.count += next.count;
                current.preview_commands.extend(next.preview_commands);
            }
            (Self::FileChangeBatch(current), Self::FileChangeBatch(next)) => {
                current.operation_count += next.operation_count;
                current.total_files += next.total_files;
                current.preview_paths.extend(next.preview_paths);
            }
            _ => {}
        }
    }
}

impl AiTimelineGroupKind {
    fn as_storage_key(self) -> &'static str {
        match self {
            Self::Exploration => "exploration",
            Self::Collaboration => "collaboration",
            Self::CommandBatch => "command_batch",
            Self::FileChangeBatch => "file_change_batch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingAiTimelineGroup {
    thread_id: String,
    turn_id: String,
    last_sequence: u64,
    status: hunk_codex::state::ItemStatus,
    child_row_ids: Vec<String>,
    summary: AiTimelineGroupSummary,
}

fn group_ai_timeline_rows_for_thread(
    state: &hunk_codex::state::AiState,
    row_ids: &[String],
    rows_by_id: &BTreeMap<String, AiTimelineRow>,
) -> (Vec<String>, Vec<AiTimelineGroup>, BTreeMap<String, String>) {
    let mut grouped_row_ids = Vec::with_capacity(row_ids.len());
    let mut groups = Vec::new();
    let mut parent_by_child_row_id = BTreeMap::new();
    let mut pending_group: Option<PendingAiTimelineGroup> = None;

    for row_id in row_ids {
        let Some(row) = rows_by_id.get(row_id.as_str()) else {
            continue;
        };
        if !ai_timeline_row_is_renderable_for_layout(state, row) {
            continue;
        }
        let candidate = ai_timeline_group_summary_for_row(state, row);
        let can_extend_pending = pending_group
            .as_ref()
            .zip(candidate.as_ref())
            .is_some_and(|(pending, candidate)| {
                pending.turn_id == row.turn_id && pending.summary.kind() == candidate.kind()
            });

        if can_extend_pending {
            if let (Some(pending), Some(candidate)) = (pending_group.as_mut(), candidate) {
                pending.last_sequence = row.last_sequence;
                pending.status = ai_merge_group_item_status(pending.status, ai_timeline_row_status(state, row));
                pending.child_row_ids.push(row_id.clone());
                pending.summary.merge(candidate);
            }
            continue;
        }

        flush_pending_ai_timeline_group(
            &mut pending_group,
            &mut grouped_row_ids,
            &mut groups,
            &mut parent_by_child_row_id,
        );

        if let Some(candidate) = candidate {
            pending_group = Some(PendingAiTimelineGroup {
                thread_id: row.thread_id.clone(),
                turn_id: row.turn_id.clone(),
                last_sequence: row.last_sequence,
                status: ai_timeline_row_status(state, row),
                child_row_ids: vec![row_id.clone()],
                summary: candidate,
            });
        } else {
            grouped_row_ids.push(row_id.clone());
        }
    }

    flush_pending_ai_timeline_group(
        &mut pending_group,
        &mut grouped_row_ids,
        &mut groups,
        &mut parent_by_child_row_id,
    );

    (grouped_row_ids, groups, parent_by_child_row_id)
}

fn flush_pending_ai_timeline_group(
    pending_group: &mut Option<PendingAiTimelineGroup>,
    grouped_row_ids: &mut Vec<String>,
    groups: &mut Vec<AiTimelineGroup>,
    parent_by_child_row_id: &mut BTreeMap<String, String>,
) {
    let Some(pending) = pending_group.take() else {
        return;
    };

    if pending.child_row_ids.len() < 2 {
        grouped_row_ids.extend(pending.child_row_ids);
        return;
    }

    let Some(first_child_row_id) = pending.child_row_ids.first() else {
        return;
    };
    let group_id = format!("group:{first_child_row_id}");
    let (title, summary) =
        ai_timeline_group_title_and_summary(&pending.summary, pending.child_row_ids.len());
    for child_row_id in &pending.child_row_ids {
        parent_by_child_row_id.insert(child_row_id.clone(), group_id.clone());
    }
    grouped_row_ids.push(group_id.clone());
    groups.push(AiTimelineGroup {
        id: group_id,
        thread_id: pending.thread_id,
        turn_id: pending.turn_id,
        last_sequence: pending.last_sequence,
        kind: pending.summary.kind().as_storage_key().to_string(),
        status: pending.status,
        title,
        summary,
        child_row_ids: pending.child_row_ids,
    });
}

fn ai_timeline_group_summary_for_row(
    state: &hunk_codex::state::AiState,
    row: &AiTimelineRow,
) -> Option<AiTimelineGroupSummary> {
    let AiTimelineRowSource::Item { item_key } = &row.source else {
        return None;
    };
    let item = state.items.get(item_key.as_str())?;
    ai_timeline_group_summary_for_item(item)
}

fn ai_timeline_group_summary_for_item(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiTimelineGroupSummary> {
    ai_exploration_group_summary_for_item(item)
        .map(AiTimelineGroupSummary::Exploration)
        .or_else(|| {
            ai_collaboration_group_summary_for_item(item)
                .map(AiTimelineGroupSummary::Collaboration)
        })
        .or_else(|| ai_command_batch_group_summary_for_item(item).map(AiTimelineGroupSummary::CommandBatch))
        .or_else(|| {
            ai_file_change_batch_group_summary_for_item(item).map(AiTimelineGroupSummary::FileChangeBatch)
        })
}

fn ai_exploration_group_summary_for_item(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiExplorationGroupSummary> {
    if item.kind == "commandExecution" {
        return ai_exploration_group_summary_for_command_execution(item);
    }

    let tool_name = ai_exploration_tool_name_for_item(item)?;
    ai_exploration_group_summary_for_tool_name(tool_name.as_str())
}

fn ai_exploration_group_summary_for_command_execution(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiExplorationGroupSummary> {
    let details = ai_timeline_item_details_value(item)?;
    if let Some(summary) = ai_exploration_group_summary_from_action_summaries(&details) {
        return Some(summary);
    }
    let command = details
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    ai_exploration_group_summary_for_shell_command(command)
}

fn ai_exploration_group_summary_from_action_summaries(
    details: &serde_json::Value,
) -> Option<AiExplorationGroupSummary> {
    let action_summaries = details
        .get("actionSummaries")
        .and_then(|value| value.as_array())?;
    let mut summary = AiExplorationGroupSummary::default();
    for action in action_summaries {
        let action = action.as_str()?.trim();
        if action.starts_with("Read ") {
            summary.files += 1;
        } else if action.starts_with("Search ") {
            summary.searches += 1;
        } else if action.starts_with("List files") {
            summary.listings += 1;
        } else {
            return None;
        }
    }

    (summary.total() > 0).then_some(summary)
}

fn ai_exploration_group_summary_for_shell_command(
    command: &str,
) -> Option<AiExplorationGroupSummary> {
    let normalized = ai_timeline_grouping_shell_command(command).to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    let mut summary = AiExplorationGroupSummary::default();
    if normalized.contains("rg --files")
        || normalized.starts_with("find ")
        || normalized.starts_with("ls ")
    {
        summary.listings = 1;
    } else if normalized.starts_with("rg ")
        || normalized.contains(" rg ")
        || normalized.starts_with("grep ")
        || normalized.contains(" grep ")
    {
        summary.searches = 1;
    } else if normalized.starts_with("cat ")
        || normalized.contains(" cat ")
        || normalized.starts_with("wc ")
        || normalized.contains(" wc ")
        || normalized.contains("sed -n")
        || normalized.contains("nl -ba")
        || normalized.starts_with("head ")
        || normalized.contains(" head ")
        || normalized.starts_with("tail ")
        || normalized.contains(" tail ")
        || normalized.starts_with("bat ")
        || normalized.contains(" bat ")
    {
        summary.files = 1;
    }

    (summary.total() > 0).then_some(summary)
}

fn ai_timeline_grouping_shell_command(command: &str) -> &str {
    const SHELL_WRAPPER_PREFIXES: &[&str] = &[
        "/usr/bin/env zsh -lc ",
        "env zsh -lc ",
        "/bin/zsh -lc ",
        "zsh -lc ",
        "/usr/bin/env bash -lc ",
        "env bash -lc ",
        "/bin/bash -lc ",
        "bash -lc ",
        "/usr/bin/env sh -lc ",
        "env sh -lc ",
        "/bin/sh -lc ",
        "sh -lc ",
        "/usr/bin/env bash -c ",
        "env bash -c ",
        "/bin/bash -c ",
        "bash -c ",
        "/usr/bin/env sh -c ",
        "env sh -c ",
        "/bin/sh -c ",
        "sh -c ",
    ];

    let trimmed = command.trim();
    let stripped = SHELL_WRAPPER_PREFIXES
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix))
        .unwrap_or(trimmed)
        .trim();
    ai_strip_matching_outer_quotes(stripped)
}

fn ai_strip_matching_outer_quotes(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0];
        let last = bytes[trimmed.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

fn ai_exploration_tool_name_for_item(item: &hunk_codex::state::ItemSummary) -> Option<String> {
    let thread_item = ai_timeline_item_thread_item(item)?;
    match thread_item {
        codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. }
        | codex_app_server_protocol::ThreadItem::McpToolCall { tool, .. } => Some(tool),
        _ => None,
    }
}

fn ai_exploration_group_summary_for_tool_name(tool_name: &str) -> Option<AiExplorationGroupSummary> {
    let normalized = tool_name.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    let mut summary = AiExplorationGroupSummary::default();
    if matches!(
        normalized.as_str(),
        "read" | "read_file" | "get_file_contents"
    ) || normalized.contains("read_file")
        || normalized.contains("get_file_contents")
    {
        summary.files = 1;
    } else if matches!(
        normalized.as_str(),
        "search" | "search_code" | "find_in_files"
    ) || normalized.contains("search_code")
        || normalized.contains("find_in_files")
    {
        summary.searches = 1;
    } else if normalized == "list_files" || normalized.contains("list_files") {
        summary.listings = 1;
    }

    (summary.total() > 0).then_some(summary)
}

fn ai_collaboration_group_summary_for_item(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiCollaborationGroupSummary> {
    let thread_item = ai_timeline_item_thread_item(item)?;
    let codex_app_server_protocol::ThreadItem::CollabAgentToolCall {
        tool,
        receiver_thread_ids,
        ..
    } = thread_item
    else {
        return None;
    };

    let mut summary = AiCollaborationGroupSummary::default();
    match tool {
        codex_app_server_protocol::CollabAgentTool::SpawnAgent => summary.spawned = 1,
        codex_app_server_protocol::CollabAgentTool::SendInput => summary.sent_inputs = 1,
        codex_app_server_protocol::CollabAgentTool::ResumeAgent => summary.resumed = 1,
        codex_app_server_protocol::CollabAgentTool::Wait => summary.waits = 1,
        codex_app_server_protocol::CollabAgentTool::CloseAgent => summary.closed = 1,
    }
    summary
        .receiver_thread_ids
        .extend(receiver_thread_ids);
    Some(summary)
}

fn ai_command_batch_group_summary_for_item(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiCommandBatchGroupSummary> {
    if item.kind != "commandExecution" {
        return None;
    }

    let details = ai_timeline_item_details_value(item)?;
    let command = details
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Some(AiCommandBatchGroupSummary {
        count: 1,
        preview_commands: command.into_iter().collect(),
    })
}

fn ai_file_change_batch_group_summary_for_item(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiFileChangeBatchGroupSummary> {
    if item.kind != "fileChange" {
        return None;
    }

    let thread_item = ai_timeline_item_thread_item(item)?;
    let codex_app_server_protocol::ThreadItem::FileChange { changes, .. } = thread_item else {
        return None;
    };
    let preview_paths = changes
        .iter()
        .filter_map(|change| {
            let path = change.path.trim();
            (!path.is_empty()).then(|| path.to_string())
        })
        .collect::<Vec<_>>();
    Some(AiFileChangeBatchGroupSummary {
        operation_count: 1,
        total_files: preview_paths.len(),
        preview_paths,
    })
}

fn ai_timeline_item_details_value(
    item: &hunk_codex::state::ItemSummary,
) -> Option<serde_json::Value> {
    let details_json = item
        .display_metadata
        .as_ref()
        .and_then(|metadata| metadata.details_json.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    serde_json::from_str::<serde_json::Value>(details_json).ok()
}

fn ai_timeline_item_thread_item(
    item: &hunk_codex::state::ItemSummary,
) -> Option<codex_app_server_protocol::ThreadItem> {
    let details_json = item
        .display_metadata
        .as_ref()
        .and_then(|metadata| metadata.details_json.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    serde_json::from_str::<codex_app_server_protocol::ThreadItem>(details_json).ok()
}

fn ai_merge_group_item_status(
    current: hunk_codex::state::ItemStatus,
    next: hunk_codex::state::ItemStatus,
) -> hunk_codex::state::ItemStatus {
    use hunk_codex::state::ItemStatus;

    match (current, next) {
        (ItemStatus::Streaming, _) | (_, ItemStatus::Streaming) => ItemStatus::Streaming,
        (ItemStatus::Started, _) | (_, ItemStatus::Started) => ItemStatus::Started,
        _ => ItemStatus::Completed,
    }
}

fn ai_timeline_row_status(
    state: &hunk_codex::state::AiState,
    row: &AiTimelineRow,
) -> hunk_codex::state::ItemStatus {
    let AiTimelineRowSource::Item { item_key } = &row.source else {
        return hunk_codex::state::ItemStatus::Completed;
    };
    state.items
        .get(item_key.as_str())
        .map(|item| item.status)
        .unwrap_or(hunk_codex::state::ItemStatus::Completed)
}

fn ai_timeline_group_title_and_summary(
    summary: &AiTimelineGroupSummary,
    child_count: usize,
) -> (String, Option<String>) {
    match summary {
        AiTimelineGroupSummary::Exploration(summary) => (
            ai_exploration_group_title(summary),
            ai_exploration_group_summary(summary),
        ),
        AiTimelineGroupSummary::Collaboration(summary) => {
            ai_collaboration_group_title_and_summary(summary, child_count)
        }
        AiTimelineGroupSummary::CommandBatch(summary) => {
            ai_command_batch_group_title_and_summary(summary, child_count)
        }
        AiTimelineGroupSummary::FileChangeBatch(summary) => {
            ai_file_change_batch_title_and_summary(summary, child_count)
        }
    }
}

fn ai_exploration_group_title(summary: &AiExplorationGroupSummary) -> String {
    let mut parts = Vec::new();
    if summary.files > 0 {
        parts.push(ai_count_noun(summary.files, "file", "files"));
    }
    if summary.searches > 0 {
        parts.push(ai_count_noun(summary.searches, "search", "searches"));
    }
    if summary.listings > 0 {
        parts.push(ai_count_noun(summary.listings, "listing", "listings"));
    }

    if parts.is_empty() {
        "Explored".to_string()
    } else {
        format!("Explored {}", parts.join(", "))
    }
}

fn ai_exploration_group_summary(summary: &AiExplorationGroupSummary) -> Option<String> {
    let mut parts = Vec::new();
    if summary.files > 0 {
        parts.push(ai_count_noun(summary.files, "read", "reads"));
    }
    if summary.searches > 0 {
        parts.push(ai_count_noun(summary.searches, "search", "searches"));
    }
    if summary.listings > 0 {
        parts.push(ai_count_noun(summary.listings, "listing", "listings"));
    }
    (!parts.is_empty()).then(|| parts.join(" • "))
}

fn ai_collaboration_group_title_and_summary(
    summary: &AiCollaborationGroupSummary,
    child_count: usize,
) -> (String, Option<String>) {
    let target_count = summary.target_count().max(child_count);
    let action_breakdown = ai_collaboration_group_breakdown(summary);
    let nonzero_action_kinds = action_breakdown.len();
    if nonzero_action_kinds == 1 {
        let title = if summary.spawned > 0 {
            format!("Launched {}", ai_count_noun(target_count, "sub-agent", "sub-agents"))
        } else if summary.sent_inputs > 0 {
            format!("Messaged {}", ai_count_noun(target_count, "sub-agent", "sub-agents"))
        } else if summary.resumed > 0 {
            format!("Resumed {}", ai_count_noun(target_count, "sub-agent", "sub-agents"))
        } else if summary.waits > 0 {
            format!("Waited on {}", ai_count_noun(target_count, "sub-agent", "sub-agents"))
        } else {
            format!("Closed {}", ai_count_noun(target_count, "sub-agent", "sub-agents"))
        };
        return (title, None);
    }

    let title = format!(
        "Worked with {}",
        ai_count_noun(target_count, "sub-agent", "sub-agents")
    );
    let summary = (!action_breakdown.is_empty()).then(|| action_breakdown.join(" • "));
    (title, summary)
}

fn ai_collaboration_group_breakdown(summary: &AiCollaborationGroupSummary) -> Vec<String> {
    let mut parts = Vec::new();
    if summary.spawned > 0 {
        parts.push(ai_count_noun(summary.spawned, "launch", "launches"));
    }
    if summary.sent_inputs > 0 {
        parts.push(ai_count_noun(summary.sent_inputs, "message", "messages"));
    }
    if summary.resumed > 0 {
        parts.push(ai_count_noun(summary.resumed, "resume", "resumes"));
    }
    if summary.waits > 0 {
        parts.push(ai_count_noun(summary.waits, "wait", "waits"));
    }
    if summary.closed > 0 {
        parts.push(ai_count_noun(summary.closed, "close", "closes"));
    }
    parts
}

fn ai_command_batch_group_title_and_summary(
    summary: &AiCommandBatchGroupSummary,
    child_count: usize,
) -> (String, Option<String>) {
    let count = summary.count.max(child_count);
    let title = format!("Ran {}", ai_count_noun(count, "command", "commands"));
    let summary = ai_command_batch_preview(summary.preview_commands.as_slice());
    (title, summary)
}

fn ai_command_batch_preview(commands: &[String]) -> Option<String> {
    let previews = commands
        .iter()
        .map(|command| command.trim())
        .filter(|command| !command.is_empty())
        .take(2)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if previews.is_empty() {
        return None;
    }
    let mut summary = previews.join(" • ");
    let remaining = commands
        .iter()
        .map(|command| command.trim())
        .filter(|command| !command.is_empty())
        .count()
        .saturating_sub(previews.len());
    if remaining > 0 {
        summary.push_str(&format!(" • +{remaining} more"));
    }
    Some(summary)
}

fn ai_file_change_batch_title_and_summary(
    summary: &AiFileChangeBatchGroupSummary,
    child_count: usize,
) -> (String, Option<String>) {
    let count = summary.operation_count.max(child_count);
    let title = format!("Applied {}", ai_count_noun(count, "file change", "file changes"));
    let summary = ai_file_change_batch_preview(summary);
    (title, summary)
}

fn ai_file_change_batch_preview(summary: &AiFileChangeBatchGroupSummary) -> Option<String> {
    let first_path = summary.preview_paths.first()?.trim();
    if first_path.is_empty() {
        return None;
    }
    if summary.total_files <= 1 {
        Some(first_path.to_string())
    } else {
        Some(format!(
            "{first_path} (+{} more files)",
            summary.total_files.saturating_sub(1)
        ))
    }
}

fn ai_count_noun(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

#[cfg(test)]
mod timeline_group_tests {
    use super::group_ai_timeline_rows_for_thread;
    use crate::app::AiTimelineRow;
    use crate::app::AiTimelineRowSource;
    use hunk_codex::state::AiState;
    use hunk_codex::state::ItemDisplayMetadata;
    use hunk_codex::state::ItemStatus;
    use std::collections::BTreeMap;

    #[allow(clippy::too_many_arguments)]
    fn timeline_tool_item(
        item_id: &str,
        thread_id: &str,
        turn_id: &str,
        kind: &str,
        status: ItemStatus,
        content: &str,
        details_json: &str,
        last_sequence: u64,
    ) -> hunk_codex::state::ItemSummary {
        hunk_codex::state::ItemSummary {
            id: item_id.to_string(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: kind.to_string(),
            status,
            content: content.to_string(),
            display_metadata: Some(ItemDisplayMetadata {
                summary: Some(kind.to_string()),
                details_json: Some(details_json.to_string()),
            }),
            last_sequence,
        }
    }

    fn timeline_item_row(
        row_id: &str,
        thread_id: &str,
        turn_id: &str,
        last_sequence: u64,
        item_key: &str,
    ) -> AiTimelineRow {
        AiTimelineRow {
            id: row_id.to_string(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            last_sequence,
            source: AiTimelineRowSource::Item {
                item_key: item_key.to_string(),
            },
        }
    }

    #[test]
    fn timeline_grouping_ignores_hidden_reasoning_boundaries() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let first_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-1");
        let hidden_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-2");
        let second_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-3");
        let first_row_id = format!("item:{first_item_key}");
        let hidden_row_id = format!("item:{hidden_item_key}");
        let second_row_id = format!("item:{second_item_key}");

        let mut state = AiState::default();
        state.items.insert(
            first_item_key.clone(),
            timeline_tool_item(
                "item-1",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "commandExecution",
                    "command": "cargo check --workspace",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Run cargo check --workspace"]
                }"#,
                1,
            ),
        );
        state.items.insert(
            hidden_item_key.clone(),
            hunk_codex::state::ItemSummary {
                id: "item-2".to_string(),
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                kind: "reasoning".to_string(),
                status: ItemStatus::Completed,
                content: String::new(),
                display_metadata: None,
                last_sequence: 2,
            },
        );
        state.items.insert(
            second_item_key.clone(),
            timeline_tool_item(
                "item-3",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "commandExecution",
                    "command": "cargo clippy --workspace --all-targets -- -D warnings",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Run cargo clippy --workspace --all-targets -- -D warnings"]
                }"#,
                3,
            ),
        );

        let row_ids = vec![
            first_row_id.clone(),
            hidden_row_id.clone(),
            second_row_id.clone(),
        ];
        let rows_by_id = BTreeMap::from([
            (
                first_row_id.clone(),
                timeline_item_row(first_row_id.as_str(), thread_id, turn_id, 1, first_item_key.as_str()),
            ),
            (
                hidden_row_id.clone(),
                timeline_item_row(hidden_row_id.as_str(), thread_id, turn_id, 2, hidden_item_key.as_str()),
            ),
            (
                second_row_id.clone(),
                timeline_item_row(
                    second_row_id.as_str(),
                    thread_id,
                    turn_id,
                    3,
                    second_item_key.as_str(),
                ),
            ),
        ]);

        let (grouped_row_ids, groups, _) =
            group_ai_timeline_rows_for_thread(&state, row_ids.as_slice(), &rows_by_id);

        assert_eq!(grouped_row_ids, vec![format!("group:{first_row_id}")]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, "command_batch");
        assert_eq!(groups[0].title, "Ran 2 commands");
    }

    #[test]
    fn timeline_grouping_recognizes_shell_wrapped_exploration_commands() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let first_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-1");
        let second_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-2");
        let first_row_id = format!("item:{first_item_key}");
        let second_row_id = format!("item:{second_item_key}");

        let mut state = AiState::default();
        state.items.insert(
            first_item_key.clone(),
            timeline_tool_item(
                "item-1",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "commandExecution",
                    "command": "/bin/zsh -lc \"rg -n update_window crates/hunk-desktop/src/app.rs\"",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Run /bin/zsh -lc \\\"rg -n update_window crates/hunk-desktop/src/app.rs\\\""]
                }"#,
                1,
            ),
        );
        state.items.insert(
            second_item_key.clone(),
            timeline_tool_item(
                "item-2",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "commandExecution",
                    "command": "/bin/zsh -lc \"wc -l crates/hunk-desktop/src/app.rs\"",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Run /bin/zsh -lc \\\"wc -l crates/hunk-desktop/src/app.rs\\\""]
                }"#,
                2,
            ),
        );

        let row_ids = vec![first_row_id.clone(), second_row_id.clone()];
        let rows_by_id = BTreeMap::from([
            (
                first_row_id.clone(),
                timeline_item_row(first_row_id.as_str(), thread_id, turn_id, 1, first_item_key.as_str()),
            ),
            (
                second_row_id.clone(),
                timeline_item_row(
                    second_row_id.as_str(),
                    thread_id,
                    turn_id,
                    2,
                    second_item_key.as_str(),
                ),
            ),
        ]);

        let (grouped_row_ids, groups, _) =
            group_ai_timeline_rows_for_thread(&state, row_ids.as_slice(), &rows_by_id);

        assert_eq!(grouped_row_ids, vec![format!("group:{first_row_id}")]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, "exploration");
        assert_eq!(groups[0].title, "Explored 1 file, 1 search");
        assert_eq!(groups[0].summary.as_deref(), Some("1 read • 1 search"));
    }
}
