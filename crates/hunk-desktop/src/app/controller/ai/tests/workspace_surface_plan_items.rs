#[test]
fn ai_workspace_message_block_config_treats_plan_items_as_assistant_messages() {
    assert_eq!(
        DiffViewer::ai_workspace_message_block_config("plan"),
        Some((
            ai_workspace_session::AiWorkspaceBlockRole::Assistant,
            "Proposed Plan",
        ))
    );
}

#[test]
fn ai_workspace_message_block_config_keeps_existing_message_kinds() {
    assert_eq!(
        DiffViewer::ai_workspace_message_block_config("userMessage"),
        Some((ai_workspace_session::AiWorkspaceBlockRole::User, "You"))
    );
    assert_eq!(
        DiffViewer::ai_workspace_message_block_config("agentMessage"),
        Some((ai_workspace_session::AiWorkspaceBlockRole::Assistant, "Assistant"))
    );
    assert_eq!(
        DiffViewer::ai_workspace_message_block_config("commandExecution"),
        None
    );
}
