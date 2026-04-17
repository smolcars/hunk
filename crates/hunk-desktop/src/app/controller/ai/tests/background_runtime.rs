#[test]
fn background_workspace_disconnect_clears_transient_state_and_restores_draft() {
    let mut workspace_state = AiWorkspaceState {
        connection_state: AiConnectionState::Reconnecting,
        bootstrap_loading: true,
        new_thread_draft_active: false,
        pending_new_thread_selection: true,
        pending_thread_start: Some(AiPendingThreadStart {
            workspace_key: "/repo-a".to_string(),
            prompt: "Continue after reconnect".to_string(),
            local_images: Vec::new(),
            skill_bindings: Vec::new(),
            started_at: Instant::now(),
            start_mode: AiNewThreadStartMode::Local,
            thread_id: Some("thread-live".to_string()),
        }),
        pending_steers: vec![AiPendingSteer {
            thread_id: "thread-live".to_string(),
            turn_id: "turn-live".to_string(),
            prompt: "Resume the last step".to_string(),
            local_images: Vec::new(),
            selected_skills: Vec::new(),
            skill_bindings: Vec::new(),
            accepted_after_sequence: 7,
            started_at: Instant::now(),
        }],
        queued_messages: vec![AiQueuedUserMessage {
            thread_id: "thread-live".to_string(),
            prompt: "Queued follow-up".to_string(),
            local_images: Vec::new(),
            selected_skills: Vec::new(),
            skill_bindings: Vec::new(),
            queued_at: Instant::now(),
            status: AiQueuedUserMessageStatus::Queued,
        }],
        interrupt_restore_queued_thread_ids: BTreeSet::from(["thread-live".to_string()]),
        pending_approvals: vec![crate::app::ai_runtime::AiPendingApproval {
            request_id: "approval-1".to_string(),
            thread_id: "thread-live".to_string(),
            turn_id: "turn-live".to_string(),
            item_id: "item-live".to_string(),
            kind: crate::app::ai_runtime::AiApprovalKind::CommandExecution,
            reason: Some("Need confirmation".to_string()),
            command: Some("git status".to_string()),
            cwd: Some(PathBuf::from("/repo-a")),
            grant_root: None,
        }],
        pending_user_inputs: vec![AiPendingUserInputRequest {
            request_id: "input-1".to_string(),
            thread_id: "thread-live".to_string(),
            turn_id: "turn-live".to_string(),
            item_id: "item-input".to_string(),
            questions: vec![AiPendingUserInputQuestion {
                id: "approval_mode".to_string(),
                header: "Mode".to_string(),
                question: "Apply now?".to_string(),
                is_other: false,
                is_secret: false,
                options: vec![AiPendingUserInputQuestionOption {
                    label: "Apply".to_string(),
                    description: "Apply the change now".to_string(),
                }],
            }],
        }],
        pending_user_input_answers: BTreeMap::from([(
            "input-1".to_string(),
            BTreeMap::from([("approval_mode".to_string(), vec!["Apply".to_string()])]),
        )]),
        status_message: Some("Reconnecting...".to_string()),
        ..AiWorkspaceState::default()
    };

    DiffViewer::apply_background_ai_workspace_disconnect(&mut workspace_state);

    assert_eq!(workspace_state.connection_state, AiConnectionState::Failed);
    assert!(!workspace_state.bootstrap_loading);
    assert!(workspace_state.new_thread_draft_active);
    assert!(!workspace_state.pending_new_thread_selection);
    assert_eq!(
        workspace_state
            .pending_thread_start
            .as_ref()
            .and_then(|pending| pending.thread_id.as_deref()),
        None
    );
    assert!(workspace_state.pending_steers.is_empty());
    assert!(workspace_state.queued_messages.is_empty());
    assert!(workspace_state.interrupt_restore_queued_thread_ids.is_empty());
    assert!(workspace_state.pending_approvals.is_empty());
    assert!(workspace_state.pending_user_inputs.is_empty());
    assert!(workspace_state.pending_user_input_answers.is_empty());
    assert_eq!(
        workspace_state.status_message.as_deref(),
        Some("Codex integration failed")
    );
    assert_eq!(
        workspace_state.error_message.as_deref(),
        Some("Codex worker disconnected.")
    );
}

#[test]
fn background_workspace_fatal_overrides_message_and_clears_transient_state() {
    let mut workspace_state = AiWorkspaceState {
        connection_state: AiConnectionState::Ready,
        bootstrap_loading: true,
        status_message: Some("Connected".to_string()),
        error_message: Some("Old error".to_string()),
        pending_steers: vec![AiPendingSteer {
            thread_id: "thread-live".to_string(),
            turn_id: "turn-live".to_string(),
            prompt: "Resume the last step".to_string(),
            local_images: Vec::new(),
            selected_skills: Vec::new(),
            skill_bindings: Vec::new(),
            accepted_after_sequence: 7,
            started_at: Instant::now(),
        }],
        queued_messages: vec![AiQueuedUserMessage {
            thread_id: "thread-live".to_string(),
            prompt: "Queued follow-up".to_string(),
            local_images: Vec::new(),
            selected_skills: Vec::new(),
            skill_bindings: Vec::new(),
            queued_at: Instant::now(),
            status: AiQueuedUserMessageStatus::Queued,
        }],
        ..AiWorkspaceState::default()
    };

    DiffViewer::apply_background_ai_workspace_fatal(
        &mut workspace_state,
        "helper runtime crashed".to_string(),
    );

    assert_eq!(workspace_state.connection_state, AiConnectionState::Failed);
    assert!(!workspace_state.bootstrap_loading);
    assert!(workspace_state.pending_steers.is_empty());
    assert!(workspace_state.queued_messages.is_empty());
    assert_eq!(
        workspace_state.status_message.as_deref(),
        Some("Codex integration failed")
    );
    assert_eq!(
        workspace_state.error_message.as_deref(),
        Some("helper runtime crashed")
    );
}
