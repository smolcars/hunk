fn lifecycle_status_from_thread_status(status: &ThreadStatus) -> ThreadLifecycleStatus {
    match status {
        ThreadStatus::Active { .. } => ThreadLifecycleStatus::Active,
        ThreadStatus::Idle | ThreadStatus::SystemError => ThreadLifecycleStatus::Idle,
        ThreadStatus::NotLoaded => ThreadLifecycleStatus::NotLoaded,
    }
}

fn thread_item_kind(item: &ThreadItem) -> &'static str {
    match item {
        ThreadItem::UserMessage { .. } => "userMessage",
        ThreadItem::AgentMessage { .. } => "agentMessage",
        ThreadItem::Plan { .. } => "plan",
        ThreadItem::Reasoning { .. } => "reasoning",
        ThreadItem::CommandExecution { .. } => "commandExecution",
        ThreadItem::FileChange { .. } => "fileChange",
        ThreadItem::McpToolCall { .. } => "mcpToolCall",
        ThreadItem::DynamicToolCall { .. } => "dynamicToolCall",
        ThreadItem::CollabAgentToolCall { .. } => "collabAgentToolCall",
        ThreadItem::WebSearch { .. } => "webSearch",
        ThreadItem::ImageView { .. } => "imageView",
        ThreadItem::ImageGeneration { .. } => "imageGeneration",
        ThreadItem::EnteredReviewMode { .. } => "enteredReviewMode",
        ThreadItem::ExitedReviewMode { .. } => "exitedReviewMode",
        ThreadItem::ContextCompaction { .. } => "contextCompaction",
    }
}

const MAX_ITEM_DETAILS_JSON_BYTES: usize = 16 * 1024;
const MAX_COMMAND_DISPLAY_BYTES: usize = 2 * 1024;
const MAX_COMMAND_ACTION_SUMMARY_BYTES: usize = 512;
const MAX_COMMAND_CWD_DISPLAY_BYTES: usize = 1024;

fn thread_item_display_metadata(item: &ThreadItem) -> Option<crate::state::ItemDisplayMetadata> {
    if !thread_item_supports_display_metadata(item) {
        return None;
    }

    let summary = thread_item_display_summary(item).map(ToOwned::to_owned);
    let details_json = thread_item_display_details_json(item);

    if summary.is_none() && details_json.is_none() {
        return None;
    }

    Some(crate::state::ItemDisplayMetadata {
        summary,
        details_json,
    })
}

fn thread_item_display_details_json(item: &ThreadItem) -> Option<String> {
    match item {
        ThreadItem::CommandExecution {
            command,
            cwd,
            process_id,
            status,
            command_actions,
            exit_code,
            duration_ms,
            ..
        } => serde_json::to_string_pretty(&serde_json::json!({
            "kind": "commandExecution",
            "command": truncate_utf8_inline_for_display(
                command.clone(),
                MAX_COMMAND_DISPLAY_BYTES,
            ),
            "cwd": truncate_utf8_inline_for_display(
                cwd.display().to_string(),
                MAX_COMMAND_CWD_DISPLAY_BYTES,
            ),
            "processId": process_id
                .as_ref()
                .map(|value| truncate_utf8_inline_for_display(value.clone(), 128)),
            "status": command_execution_status_text(status),
            "actionSummaries": command_actions
                .iter()
                .map(command_action_summary)
                .collect::<Vec<_>>(),
            "exitCode": exit_code,
            "durationMs": duration_ms,
        }))
        .ok(),
        ThreadItem::FileChange { changes, .. } => {
            let mut change_summaries = changes
                .iter()
                .map(|change| {
                    let path = change.path.trim();
                    let (added, removed) = file_change_diff_line_counts(change.diff.as_str());
                    serde_json::json!({
                        "path": if path.is_empty() { "changes" } else { path },
                        "added": added,
                        "removed": removed,
                    })
                })
                .collect::<Vec<_>>();
            let mut truncated_count = 0usize;

            loop {
                let value = serde_json::json!({
                    "kind": "fileChangeSummary",
                    "changes": &change_summaries,
                    "truncatedCount": truncated_count,
                });
                let json = serde_json::to_string_pretty(&value).ok()?;
                if json.len() <= MAX_ITEM_DETAILS_JSON_BYTES || change_summaries.is_empty() {
                    return Some(json);
                }
                change_summaries.pop();
                truncated_count = truncated_count.saturating_add(1);
            }
        }
        _ => serde_json::to_string_pretty(item)
            .ok()
            .map(|json| truncate_utf8_for_display(json, MAX_ITEM_DETAILS_JSON_BYTES)),
    }
}

fn file_change_diff_line_counts(diff_text: &str) -> (u64, u64) {
    let mut added = 0u64;
    let mut removed = 0u64;

    for line in diff_text.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added = added.saturating_add(1);
            continue;
        }
        if line.starts_with('-') {
            removed = removed.saturating_add(1);
        }
    }

    (added, removed)
}

fn thread_item_supports_display_metadata(item: &ThreadItem) -> bool {
    matches!(
        item,
        ThreadItem::CommandExecution { .. }
            | ThreadItem::FileChange { .. }
            | ThreadItem::McpToolCall { .. }
            | ThreadItem::DynamicToolCall { .. }
            | ThreadItem::CollabAgentToolCall { .. }
    )
}

fn thread_item_display_summary(item: &ThreadItem) -> Option<&'static str> {
    match item {
        ThreadItem::CommandExecution { .. } => Some("Ran command"),
        ThreadItem::FileChange { .. } => Some("Applied file changes"),
        ThreadItem::McpToolCall { .. } => Some("Called MCP tool"),
        ThreadItem::DynamicToolCall { .. } => Some("Called tool"),
        ThreadItem::CollabAgentToolCall { .. } => Some("Delegated to collaborator"),
        _ => None,
    }
}

fn truncate_utf8_for_display(input: String, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input;
    }

    let mut cutoff = max_bytes.min(input.len());
    while cutoff > 0 && !input.is_char_boundary(cutoff) {
        cutoff = cutoff.saturating_sub(1);
    }

    let mut truncated = input[..cutoff].to_string();
    truncated.push_str("\n... [truncated]");
    truncated
}

fn truncate_utf8_inline_for_display(input: String, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input;
    }

    let mut cutoff = max_bytes.min(input.len());
    while cutoff > 0 && !input.is_char_boundary(cutoff) {
        cutoff = cutoff.saturating_sub(1);
    }

    let mut truncated = input[..cutoff].to_string();
    truncated.push_str("...");
    truncated
}

fn command_execution_status_text(
    status: &codex_app_server_protocol::CommandExecutionStatus,
) -> &'static str {
    match status {
        codex_app_server_protocol::CommandExecutionStatus::InProgress => "inProgress",
        codex_app_server_protocol::CommandExecutionStatus::Completed => "completed",
        codex_app_server_protocol::CommandExecutionStatus::Failed => "failed",
        codex_app_server_protocol::CommandExecutionStatus::Declined => "declined",
    }
}

fn command_action_summary(action: &codex_app_server_protocol::CommandAction) -> String {
    let summary = match action {
        codex_app_server_protocol::CommandAction::Read { name, path, .. } => {
            format!("Read {name} from {}", path.display())
        }
        codex_app_server_protocol::CommandAction::ListFiles { path, .. } => {
            let scope = path.as_deref().unwrap_or(".");
            format!("List files in {scope}")
        }
        codex_app_server_protocol::CommandAction::Search { query, path, .. } => {
            let query = query.as_deref().unwrap_or("<query>");
            let scope = path.as_deref().unwrap_or(".");
            format!("Search {query} in {scope}")
        }
        codex_app_server_protocol::CommandAction::Unknown { command } => {
            format!("Run {command}")
        }
    };

    truncate_utf8_inline_for_display(summary, MAX_COMMAND_ACTION_SUMMARY_BYTES)
}

fn thread_item_seed_content(item: &ThreadItem) -> Option<String> {
    match item {
        ThreadItem::UserMessage { content, .. } => user_message_seed_content(content.as_slice()),
        ThreadItem::AgentMessage { text, .. } | ThreadItem::Plan { text, .. } => {
            (!text.is_empty()).then(|| text.clone())
        }
        ThreadItem::Reasoning {
            summary, content, ..
        } => {
            let mut parts = String::new();
            if !summary.is_empty() {
                parts.push_str(&summary.join(""));
            }
            if !content.is_empty() {
                parts.push_str(&content.join(""));
            }
            (!parts.is_empty()).then_some(parts)
        }
        ThreadItem::CommandExecution {
            aggregated_output, ..
        } => aggregated_output.clone().filter(|value| !value.is_empty()),
        ThreadItem::FileChange { changes, .. } => {
            let joined = changes
                .iter()
                .map(|change| change.diff.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            (!joined.is_empty()).then_some(joined)
        }
        ThreadItem::McpToolCall { error, .. } => error.as_ref().map(|value| value.message.clone()),
        ThreadItem::EnteredReviewMode { review, .. }
        | ThreadItem::ExitedReviewMode { review, .. } => {
            (!review.is_empty()).then(|| review.clone())
        }
        ThreadItem::WebSearch { query, action, .. } => {
            let detail = web_search_detail(action.as_ref(), query.as_str());
            (!detail.is_empty()).then(|| format!("Searched {detail}"))
        }
        ThreadItem::ImageGeneration {
            status,
            revised_prompt,
            result,
            ..
        } => {
            let detail = revised_prompt
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or(result.as_str());
            let detail = detail.trim();
            if detail.is_empty() && status.trim().is_empty() {
                None
            } else if detail.is_empty() {
                Some(format!("Generated image ({status})"))
            } else if status.trim().is_empty() {
                Some(format!("Generated image: {detail}"))
            } else {
                Some(format!("Generated image ({status}): {detail}"))
            }
        }
        ThreadItem::DynamicToolCall { .. }
        | ThreadItem::CollabAgentToolCall { .. }
        | ThreadItem::ImageView { .. }
        | ThreadItem::ContextCompaction { .. } => None,
    }
}

fn web_search_action_detail(action: &codex_app_server_protocol::WebSearchAction) -> String {
    match action {
        codex_app_server_protocol::WebSearchAction::Search { query, queries } => {
            query.clone().filter(|value| !value.is_empty()).unwrap_or_else(|| {
                let first = queries
                    .as_ref()
                    .and_then(|items| items.first())
                    .cloned()
                    .unwrap_or_default();
                if queries.as_ref().is_some_and(|items| items.len() > 1) && !first.is_empty() {
                    format!("{first} ...")
                } else {
                    first
                }
            })
        }
        codex_app_server_protocol::WebSearchAction::OpenPage { url } => {
            url.clone().unwrap_or_default()
        }
        codex_app_server_protocol::WebSearchAction::FindInPage { url, pattern } => {
            match (pattern, url) {
                (Some(pattern), Some(url)) => format!("'{pattern}' in {url}"),
                (Some(pattern), None) => format!("'{pattern}'"),
                (None, Some(url)) => url.clone(),
                (None, None) => String::new(),
            }
        }
        codex_app_server_protocol::WebSearchAction::Other => String::new(),
    }
}

fn web_search_detail(
    action: Option<&codex_app_server_protocol::WebSearchAction>,
    query: &str,
) -> String {
    let detail = action.map(web_search_action_detail).unwrap_or_default();
    if detail.is_empty() {
        query.to_string()
    } else {
        detail
    }
}

fn user_message_seed_content(content: &[UserInput]) -> Option<String> {
    let text = content
        .iter()
        .filter_map(user_input_text_content)
        .collect::<Vec<_>>()
        .join("");
    let images = content
        .iter()
        .filter_map(user_input_local_image_name)
        .collect::<Vec<_>>();

    if text.is_empty() && images.is_empty() {
        return None;
    }

    if images.is_empty() {
        return Some(text);
    }

    let image_prefix = if images.len() == 1 {
        "[image] "
    } else {
        "[images] "
    };
    let image_summary = format!("{image_prefix}{}", images.join(", "));
    if text.is_empty() {
        Some(image_summary)
    } else {
        Some(format!("{text}\n{image_summary}"))
    }
}

fn user_input_text_content(input: &UserInput) -> Option<&str> {
    match input {
        UserInput::Text { text, .. } => Some(text.as_str()),
        _ => None,
    }
}

fn user_input_local_image_name(input: &UserInput) -> Option<String> {
    match input {
        UserInput::LocalImage { path } => Some(local_image_display_name(path.as_path())),
        _ => None,
    }
}

fn local_image_display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn thread_item_is_complete(item: &ThreadItem) -> bool {
    match item {
        ThreadItem::CommandExecution { status, .. } => {
            !matches!(status, CommandExecutionStatus::InProgress)
        }
        ThreadItem::FileChange { status, .. } => !matches!(status, PatchApplyStatus::InProgress),
        ThreadItem::McpToolCall { status, .. } => !matches!(status, McpToolCallStatus::InProgress),
        ThreadItem::DynamicToolCall { status, .. } => {
            !matches!(status, DynamicToolCallStatus::InProgress)
        }
        ThreadItem::CollabAgentToolCall { status, .. } => {
            !matches!(status, CollabAgentToolCallStatus::InProgress)
        }
        ThreadItem::ImageGeneration { status, .. } => !status.trim().is_empty(),
        _ => false,
    }
}

fn request_id_key(request_id: &RequestId) -> String {
    match request_id {
        RequestId::Integer(value) => format!("int:{value}"),
        RequestId::String(value) => format!("str:{value}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_search_seed_content_prefers_action_query() {
        let item = ThreadItem::WebSearch {
            id: "ws_1".to_string(),
            query: "fallback".to_string(),
            action: Some(codex_app_server_protocol::WebSearchAction::Search {
                query: Some("weather: 30009".to_string()),
                queries: None,
            }),
        };

        assert_eq!(
            thread_item_seed_content(&item).as_deref(),
            Some("Searched weather: 30009")
        );
    }

    #[test]
    fn web_search_seed_content_uses_fallback_query_when_action_empty() {
        let item = ThreadItem::WebSearch {
            id: "ws_2".to_string(),
            query: "weather: New York, NY".to_string(),
            action: None,
        };

        assert_eq!(
            thread_item_seed_content(&item).as_deref(),
            Some("Searched weather: New York, NY")
        );
    }

    #[test]
    fn web_search_seed_content_formats_find_in_page() {
        let item = ThreadItem::WebSearch {
            id: "ws_3".to_string(),
            query: "fallback".to_string(),
            action: Some(codex_app_server_protocol::WebSearchAction::FindInPage {
                pattern: Some("rain".to_string()),
                url: Some("https://example.com/weather".to_string()),
            }),
        };

        assert_eq!(
            thread_item_seed_content(&item).as_deref(),
            Some("Searched 'rain' in https://example.com/weather")
        );
    }

    #[test]
    fn truncate_utf8_for_display_keeps_utf8_boundaries() {
        let value = "tool ✅ output".to_string();
        let truncated = truncate_utf8_for_display(value, 7);
        assert!(truncated.starts_with("tool "));
        assert!(!truncated.starts_with("tool ✅"));
        assert!(!truncated.contains('\u{fffd}'));
        assert!(truncated.contains("... [truncated]"));
    }

    #[test]
    fn file_change_display_details_json_stays_valid_when_truncated() {
        let item = serde_json::from_value::<ThreadItem>(serde_json::json!({
            "type": "fileChange",
            "id": "item-1",
            "changes": (0..400).map(|index| {
                serde_json::json!({
                    "path": format!("docs/file-{index}.md"),
                    "kind": { "type": "update", "movePath": null },
                    "diff": "@@ -1 +1,2 @@\n-old\n+new\n+extra"
                })
            }).collect::<Vec<_>>(),
            "status": "completed"
        }))
        .expect("file change item should deserialize");

        let details_json = thread_item_display_details_json(&item).expect("details json");
        assert!(details_json.len() <= MAX_ITEM_DETAILS_JSON_BYTES);

        let value =
            serde_json::from_str::<serde_json::Value>(details_json.as_str()).expect("valid json");
        assert_eq!(
            value.get("kind").and_then(|value| value.as_str()),
            Some("fileChangeSummary")
        );
        assert!(value
            .get("truncatedCount")
            .and_then(|value| value.as_u64())
            .is_some());
    }
}
