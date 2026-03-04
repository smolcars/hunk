fn ai_connection_label(
    state: AiConnectionState,
    cx: &mut Context<DiffViewer>,
) -> (&'static str, Hsla) {
    match state {
        AiConnectionState::Disconnected => ("Disconnected", cx.theme().muted_foreground),
        AiConnectionState::Connecting => ("Connecting", cx.theme().warning),
        AiConnectionState::Ready => ("Connected", cx.theme().success),
        AiConnectionState::Failed => ("Failed", cx.theme().danger),
    }
}

fn ai_thread_status_label(
    status: ThreadLifecycleStatus,
    cx: &mut Context<DiffViewer>,
) -> (&'static str, Hsla) {
    match status {
        ThreadLifecycleStatus::Active => ("active", cx.theme().success),
        ThreadLifecycleStatus::Archived => ("archived", cx.theme().warning),
        ThreadLifecycleStatus::Closed => ("closed", cx.theme().muted_foreground),
    }
}

fn ai_turn_status_label(status: TurnStatus) -> &'static str {
    match status {
        TurnStatus::InProgress => "in-progress",
        TurnStatus::Completed => "completed",
    }
}

fn ai_item_status_label(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Started => "started",
        ItemStatus::Streaming => "streaming",
        ItemStatus::Completed => "completed",
    }
}

fn ai_item_status_color(status: ItemStatus, cx: &mut Context<DiffViewer>) -> Hsla {
    match status {
        ItemStatus::Started => cx.theme().muted_foreground,
        ItemStatus::Streaming => cx.theme().accent,
        ItemStatus::Completed => cx.theme().success,
    }
}

fn ai_approval_kind_label(kind: AiApprovalKind) -> &'static str {
    match kind {
        AiApprovalKind::CommandExecution => "Command Execution Approval",
        AiApprovalKind::FileChange => "File Change Approval",
    }
}

fn ai_approval_description(approval: &AiPendingApproval) -> String {
    match approval.kind {
        AiApprovalKind::CommandExecution => {
            if let Some(command) = approval.command.as_ref() {
                return format!("Command: {command}");
            }
            if let Some(cwd) = approval.cwd.as_ref() {
                return format!("Requested in {}", cwd.display());
            }
            "Command execution request".to_string()
        }
        AiApprovalKind::FileChange => {
            if let Some(grant_root) = approval.grant_root.as_ref() {
                return format!("Grant write access under {}", grant_root.display());
            }
            "File change request".to_string()
        }
    }
}
