#[test]
fn apply_ai_snapshot_to_workspace_state_preserves_explicit_selection_while_other_thread_streams() {
    let mut workspace_state = AiWorkspaceState {
        selected_thread_id: Some("thread-selected".to_string()),
        ..AiWorkspaceState::default()
    };
    let mut snapshot_state = AiState::default();
    snapshot_state.threads.insert(
        "thread-selected".to_string(),
        ThreadSummary {
            id: "thread-selected".to_string(),
            cwd: "/repo".to_string(),
            title: Some("Selected".to_string()),
            status: ThreadLifecycleStatus::Idle,
            created_at: 1,
            updated_at: 1,
            last_sequence: 1,
        },
    );
    snapshot_state.threads.insert(
        "thread-streaming".to_string(),
        ThreadSummary {
            id: "thread-streaming".to_string(),
            cwd: "/repo".to_string(),
            title: Some("Streaming".to_string()),
            status: ThreadLifecycleStatus::Active,
            created_at: 2,
            updated_at: 5,
            last_sequence: 5,
        },
    );
    snapshot_state.turns.insert(
        hunk_codex::state::turn_storage_key("thread-streaming", "turn-streaming"),
        hunk_codex::state::TurnSummary {
            id: "turn-streaming".to_string(),
            thread_id: "thread-streaming".to_string(),
            collaboration_mode: None,
            status: hunk_codex::state::TurnStatus::InProgress,
            last_sequence: 5,
        },
    );
    snapshot_state.items.insert(
        hunk_codex::state::item_storage_key("thread-streaming", "turn-streaming", "item-1"),
        timeline_tool_item(
            "item-1",
            "thread-streaming",
            "turn-streaming",
            "agentMessage",
            ItemStatus::Streaming,
            "still working",
            "{}",
            5,
        ),
    );

    DiffViewer::apply_ai_snapshot_to_workspace_state(
        &mut workspace_state,
        AiSnapshot {
            state: snapshot_state,
            active_thread_id: Some("thread-streaming".to_string()),
            pending_approvals: Vec::new(),
            pending_user_inputs: Vec::new(),
            account: None,
            requires_openai_auth: false,
            pending_chatgpt_login_id: None,
            pending_chatgpt_auth_url: None,
            rate_limits: None,
            models: Vec::new(),
            experimental_features: Vec::new(),
            collaboration_modes: Vec::new(),
            skills: Vec::new(),
            include_hidden_models: true,
            mad_max_mode: false,
        },
    );

    assert_eq!(workspace_state.connection_state, AiConnectionState::Ready);
    assert_eq!(
        workspace_state.selected_thread_id.as_deref(),
        Some("thread-selected")
    );
    assert_eq!(
        workspace_state.state_snapshot.threads.get("thread-streaming"),
        Some(&ThreadSummary {
            id: "thread-streaming".to_string(),
            cwd: "/repo".to_string(),
            title: Some("Streaming".to_string()),
            status: ThreadLifecycleStatus::Active,
            created_at: 2,
            updated_at: 5,
            last_sequence: 5,
        })
    );
}

#[test]
fn apply_ai_snapshot_to_workspace_state_switches_to_streaming_thread_when_selection_disappears() {
    let mut workspace_state = AiWorkspaceState {
        selected_thread_id: Some("thread-gone".to_string()),
        ..AiWorkspaceState::default()
    };
    let mut snapshot_state = AiState::default();
    snapshot_state.threads.insert(
        "thread-streaming".to_string(),
        ThreadSummary {
            id: "thread-streaming".to_string(),
            cwd: "/repo".to_string(),
            title: Some("Streaming".to_string()),
            status: ThreadLifecycleStatus::Active,
            created_at: 2,
            updated_at: 5,
            last_sequence: 5,
        },
    );
    snapshot_state.turns.insert(
        hunk_codex::state::turn_storage_key("thread-streaming", "turn-streaming"),
        hunk_codex::state::TurnSummary {
            id: "turn-streaming".to_string(),
            thread_id: "thread-streaming".to_string(),
            collaboration_mode: None,
            status: hunk_codex::state::TurnStatus::InProgress,
            last_sequence: 5,
        },
    );

    DiffViewer::apply_ai_snapshot_to_workspace_state(
        &mut workspace_state,
        AiSnapshot {
            state: snapshot_state,
            active_thread_id: Some("thread-streaming".to_string()),
            pending_approvals: Vec::new(),
            pending_user_inputs: Vec::new(),
            account: None,
            requires_openai_auth: false,
            pending_chatgpt_login_id: None,
            pending_chatgpt_auth_url: None,
            rate_limits: None,
            models: Vec::new(),
            experimental_features: Vec::new(),
            collaboration_modes: Vec::new(),
            skills: Vec::new(),
            include_hidden_models: true,
            mad_max_mode: false,
        },
    );

    assert_eq!(
        workspace_state.selected_thread_id.as_deref(),
        Some("thread-streaming")
    );
}
