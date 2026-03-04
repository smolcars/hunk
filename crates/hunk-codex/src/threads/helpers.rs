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
        ThreadItem::EnteredReviewMode { .. } => "enteredReviewMode",
        ThreadItem::ExitedReviewMode { .. } => "exitedReviewMode",
        ThreadItem::ContextCompaction { .. } => "contextCompaction",
    }
}

fn thread_item_seed_content(item: &ThreadItem) -> Option<String> {
    match item {
        ThreadItem::UserMessage { content, .. } => {
            let text = content
                .iter()
                .filter_map(user_input_text_content)
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
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
        ThreadItem::DynamicToolCall { .. }
        | ThreadItem::CollabAgentToolCall { .. }
        | ThreadItem::WebSearch { .. }
        | ThreadItem::ImageView { .. }
        | ThreadItem::ContextCompaction { .. } => None,
    }
}

fn user_input_text_content(input: &UserInput) -> Option<&str> {
    match input {
        UserInput::Text { text, .. } => Some(text.as_str()),
        _ => None,
    }
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
        _ => false,
    }
}

fn request_id_key(request_id: &RequestId) -> String {
    match request_id {
        RequestId::Integer(value) => format!("int:{value}"),
        RequestId::String(value) => format!("str:{value}"),
    }
}
