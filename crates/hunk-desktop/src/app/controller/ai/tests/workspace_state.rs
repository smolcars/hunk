    #[test]
    fn sorted_threads_orders_by_created_at_descending() {
        let mut state = AiState::default();
        state.threads.insert(
            "t-older".to_string(),
            ThreadSummary {
                id: "t-older".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                created_at: 10,
                updated_at: 10,
                last_sequence: 2,
            },
        );
        state.threads.insert(
            "t-newer".to_string(),
            ThreadSummary {
                id: "t-newer".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                created_at: 20,
                updated_at: 20,
                last_sequence: 1,
            },
        );
        let sorted = sorted_threads(&state);
        assert_eq!(sorted[0].id, "t-newer");
        assert_eq!(sorted[1].id, "t-older");
    }

    #[test]
    fn sorted_threads_breaks_created_at_ties_in_descending_id_order() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                created_at: 7,
                updated_at: 7,
                last_sequence: 7,
            },
        );
        state.threads.insert(
            "thread-z".to_string(),
            ThreadSummary {
                id: "thread-z".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                created_at: 7,
                updated_at: 7,
                last_sequence: 7,
            },
        );

        let sorted = sorted_threads(&state);
        assert_eq!(sorted[0].id, "thread-z");
        assert_eq!(sorted[1].id, "thread-a");
    }

    #[test]
    fn ai_branch_name_for_thread_prefers_first_user_message() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-1".to_string(),
            ThreadSummary {
                id: "thread-1".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Thread title".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 2,
                last_sequence: 2,
            },
        );
        state.items.insert(
            "thread-1::turn-1::item-1".to_string(),
            timeline_tool_item(
                "item-1",
                "thread-1",
                "turn-1",
                "userMessage",
                ItemStatus::Completed,
                "Fix the login spinner overflow issue",
                "{}",
                1,
            ),
        );
        state.items.insert(
            "thread-1::turn-2::item-2".to_string(),
            timeline_tool_item(
                "item-2",
                "thread-1",
                "turn-2",
                "userMessage",
                ItemStatus::Completed,
                "This later prompt should not rename the branch",
                "{}",
                2,
            ),
        );

        assert_eq!(
            ai_branch_name_for_thread(&state, "thread-1", "main", false),
            "ai/local/fix-login-spinner-overflow-issue"
        );
    }

    #[test]
    fn workspace_target_summary_for_root_checks_other_open_projects() {
        let current_workspace_targets = vec![workspace_target(
            "repo-a",
            WorkspaceTargetKind::PrimaryCheckout,
            "/repo-a",
            "Repo A",
        )];
        let mut other_project_targets = vec![workspace_target(
            "repo-b",
            WorkspaceTargetKind::PrimaryCheckout,
            "/repo-b",
            "Repo B",
        )];
        other_project_targets[0].branch_name = "feature/project-b".to_string();

        let resolved = workspace_target_summary_for_root(
            std::path::Path::new("/repo-b"),
            &current_workspace_targets,
            [other_project_targets.as_slice()],
        )
        .expect("cross-project workspace target should resolve");

        assert_eq!(resolved.display_name, "Repo B");
        assert_eq!(resolved.branch_name, "feature/project-b");
    }

    #[test]
    fn cached_workspace_branch_name_for_root_uses_attached_worktree_root() {
        let workflow_cache = [(
            "/repo-b".to_string(),
            CachedWorkflowState {
                root: Some(PathBuf::from("/repo-b")),
                branch_name: "main".to_string(),
                branches: vec![CachedLocalBranchState {
                    name: "feature/project-b".to_string(),
                    is_current: false,
                    is_remote_tracking: false,
                    tip_unix_time: None,
                    attached_workspace_target_id: Some("worktree:task-1".to_string()),
                    attached_workspace_target_root: Some(PathBuf::from("/repo-b/worktrees/task-1")),
                    attached_workspace_target_label: Some("task-1".to_string()),
                }],
                ..CachedWorkflowState::default()
            },
        )]
        .into_iter()
        .collect::<BTreeMap<_, _>>();

        assert_eq!(
            cached_workspace_branch_name_for_root(
                std::path::Path::new("/repo-b/worktrees/task-1"),
                &workflow_cache,
            )
            .as_deref(),
            Some("feature/project-b")
        );
    }

    #[test]
    fn workspace_branch_name_for_root_checks_other_open_projects() {
        let current_workspace_targets = vec![workspace_target(
            "repo-a",
            WorkspaceTargetKind::PrimaryCheckout,
            "/repo-a",
            "Repo A",
        )];
        let mut other_project_targets = vec![workspace_target(
            "repo-b",
            WorkspaceTargetKind::PrimaryCheckout,
            "/repo-b",
            "Repo B",
        )];
        other_project_targets[0].branch_name = "feature/project-b".to_string();

        assert_eq!(
            workspace_branch_name_for_root(
                std::path::Path::new("/repo-b"),
                &current_workspace_targets,
                [other_project_targets.as_slice()],
                &BTreeMap::default(),
            )
            .as_deref(),
            Some("feature/project-b")
        );
    }

    #[test]
    fn workspace_branch_name_for_root_falls_back_to_cached_workflow_state() {
        let workflow_cache = [(
            "/repo-b".to_string(),
            CachedWorkflowState {
                root: Some(PathBuf::from("/repo-b")),
                branch_name: "main".to_string(),
                branches: vec![CachedLocalBranchState {
                    name: "feature/project-b".to_string(),
                    is_current: false,
                    is_remote_tracking: false,
                    tip_unix_time: None,
                    attached_workspace_target_id: Some("worktree:task-1".to_string()),
                    attached_workspace_target_root: Some(PathBuf::from("/repo-b/worktrees/task-1")),
                    attached_workspace_target_label: Some("task-1".to_string()),
                }],
                ..CachedWorkflowState::default()
            },
        )]
        .into_iter()
        .collect::<BTreeMap<_, _>>();

        assert_eq!(
            workspace_branch_name_for_root(
                std::path::Path::new("/repo-b/worktrees/task-1"),
                &[],
                std::iter::empty::<&[WorkspaceTargetSummary]>(),
                &workflow_cache,
            )
            .as_deref(),
            Some("feature/project-b")
        );
    }

    #[test]
    fn apply_ai_snapshot_to_workspace_state_selects_active_thread_for_pending_draft() {
        let mut workspace_state = AiWorkspaceState {
            new_thread_draft_active: true,
            pending_new_thread_selection: true,
            ..AiWorkspaceState::default()
        };
        let mut snapshot_state = AiState::default();
        snapshot_state.threads.insert(
            "thread-1".to_string(),
            ThreadSummary {
                id: "thread-1".to_string(),
                cwd: "/repo/worktrees/task-1".to_string(),
                title: Some("Task 1".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 20,
                updated_at: 20,
                last_sequence: 3,
            },
        );

        DiffViewer::apply_ai_snapshot_to_workspace_state(
            &mut workspace_state,
            AiSnapshot {
                state: snapshot_state,
                active_thread_id: Some("thread-1".to_string()),
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
        assert_eq!(workspace_state.selected_thread_id.as_deref(), Some("thread-1"));
        assert!(!workspace_state.new_thread_draft_active);
        assert!(!workspace_state.pending_new_thread_selection);
        assert!(workspace_state.error_message.is_none());
    }

    #[test]
    fn seeded_ai_workspace_state_for_new_thread_workspace_preserves_pending_draft_state() {
        let mut current_snapshot = AiState::default();
        current_snapshot.threads.insert(
            "thread-1".to_string(),
            ThreadSummary {
                id: "thread-1".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Existing".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );

        let current_state = AiWorkspaceState {
            connection_state: AiConnectionState::Ready,
            state_snapshot: current_snapshot,
            selected_thread_id: Some("thread-1".to_string()),
            new_thread_draft_active: true,
            new_thread_start_mode: AiNewThreadStartMode::Worktree,
            worktree_base_branch_name: Some("main".to_string()),
            pending_thread_start: Some(AiPendingThreadStart {
                workspace_key: "/repo".to_string(),
                prompt: "Follow up on the failing startup flow".to_string(),
                local_images: Vec::new(),
                skill_bindings: Vec::new(),
                started_at: Instant::now(),
                start_mode: AiNewThreadStartMode::Worktree,
                thread_id: None,
            }),
            queued_messages: vec![AiQueuedUserMessage {
                thread_id: "thread-1".to_string(),
                prompt: "queue this follow-up".to_string(),
                local_images: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                queued_at: Instant::now(),
                status: AiQueuedUserMessageStatus::Queued,
            }],
            interrupt_restore_queued_thread_ids: ["thread-1".to_string()]
                .into_iter()
                .collect(),
            timeline_follow_output: false,
            thread_title_refresh_state_by_thread: [(
                "thread-1".to_string(),
                AiThreadTitleRefreshState {
                    key: "refresh-thread-1".to_string(),
                    attempts: 1,
                    in_flight: true,
                    last_attempt_at: Instant::now(),
                },
            )]
            .into_iter()
            .collect(),
            expanded_timeline_row_ids: ["row-1".to_string()].into_iter().collect(),
            models: vec![ai_model(
                "gpt-5",
                "GPT-5",
                true,
                &[ReasoningEffort::High],
                ReasoningEffort::High,
            )],
            selected_model: Some("gpt-5".to_string()),
            selected_effort: Some("high".to_string()),
            selected_collaboration_mode: AiCollaborationModeSelection::Plan,
            selected_service_tier: AiServiceTierSelection::Fast,
            mad_max_mode: true,
            ..AiWorkspaceState::default()
        };

        let seeded =
            DiffViewer::seeded_ai_workspace_state_for_new_thread_workspace(&current_state);

        assert_eq!(seeded.connection_state, AiConnectionState::Disconnected);
        assert!(!seeded.bootstrap_loading);
        assert!(seeded.status_message.is_none());
        assert!(seeded.error_message.is_none());
        assert!(seeded.state_snapshot.threads.is_empty());
        assert_eq!(seeded.selected_thread_id, None);
        assert!(seeded.new_thread_draft_active);
        assert_eq!(seeded.new_thread_start_mode, AiNewThreadStartMode::Worktree);
        assert_eq!(
            seeded.worktree_base_branch_name.as_deref(),
            Some("main")
        );
        assert_eq!(
            seeded
                .pending_thread_start
                .as_ref()
                .map(|pending| pending.prompt.as_str()),
            Some("Follow up on the failing startup flow")
        );
        assert!(seeded.queued_messages.is_empty());
        assert!(seeded.interrupt_restore_queued_thread_ids.is_empty());
        assert!(!seeded.timeline_follow_output);
        assert!(seeded.thread_title_refresh_state_by_thread.is_empty());
        assert!(seeded.expanded_timeline_row_ids.is_empty());
        assert!(seeded.inline_review_mode_by_thread.is_empty());
        assert_eq!(seeded.models.len(), 1);
        assert_eq!(seeded.selected_model.as_deref(), Some("gpt-5"));
        assert_eq!(seeded.selected_effort.as_deref(), Some("high"));
        assert_eq!(
            seeded.selected_collaboration_mode,
            AiCollaborationModeSelection::Plan
        );
        assert_eq!(seeded.selected_service_tier, AiServiceTierSelection::Fast);
        assert!(seeded.mad_max_mode);
    }

    #[test]
    fn current_visible_thread_id_from_snapshot_prefers_live_snapshot_threads_only() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-active".to_string(),
            ThreadSummary {
                id: "thread-active".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Active".to_string()),
                status: ThreadLifecycleStatus::Idle,
                created_at: 20,
                updated_at: 20,
                last_sequence: 2,
            },
        );
        state
            .active_thread_by_cwd
            .insert("/repo".to_string(), "thread-active".to_string());

        assert_eq!(
            current_visible_thread_id_from_snapshot(
                &state,
                Some("thread-stale"),
                Some("/repo"),
                false,
            )
            .as_deref(),
            Some("thread-active")
        );
    }

    #[test]
    fn current_visible_thread_fallback_workspace_key_prefers_visible_workspace_over_stale_draft() {
        assert_eq!(
            current_visible_thread_fallback_workspace_key(
                Some("/repo"),
                None,
                Some("/repo/worktrees/task-1"),
            )
            .as_deref(),
            Some("/repo")
        );
    }

    #[test]
    fn current_visible_thread_fallback_workspace_key_prefers_selected_thread_workspace_over_stale_draft() {
        assert_eq!(
            current_visible_thread_fallback_workspace_key(
                None,
                Some(std::path::Path::new("/repo")),
                Some("/repo/worktrees/task-1"),
            )
            .as_deref(),
            Some("/repo")
        );
    }

    #[test]
    fn current_visible_thread_id_from_snapshot_uses_visible_workspace_before_stale_draft_workspace() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-local".to_string(),
            ThreadSummary {
                id: "thread-local".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Local".to_string()),
                status: ThreadLifecycleStatus::Idle,
                created_at: 10,
                updated_at: 10,
                last_sequence: 1,
            },
        );
        state.threads.insert(
            "thread-worktree".to_string(),
            ThreadSummary {
                id: "thread-worktree".to_string(),
                cwd: "/repo/worktrees/task-1".to_string(),
                title: Some("Worktree".to_string()),
                status: ThreadLifecycleStatus::Idle,
                created_at: 20,
                updated_at: 20,
                last_sequence: 2,
            },
        );
        state
            .active_thread_by_cwd
            .insert("/repo".to_string(), "thread-local".to_string());
        state.active_thread_by_cwd.insert(
            "/repo/worktrees/task-1".to_string(),
            "thread-worktree".to_string(),
        );

        let workspace_key = current_visible_thread_fallback_workspace_key(
            Some("/repo"),
            None,
            Some("/repo/worktrees/task-1"),
        );

        assert_eq!(
            current_visible_thread_id_from_snapshot(
                &state,
                Some("missing-thread"),
                workspace_key.as_deref(),
                false,
            )
            .as_deref(),
            Some("thread-local")
        );
    }

    #[test]
    fn restore_ai_workspace_state_after_failure_reopens_pending_new_thread_draft() {
        let mut workspace_state = AiWorkspaceState {
            new_thread_draft_active: false,
            pending_new_thread_selection: true,
            pending_thread_start: Some(AiPendingThreadStart {
                workspace_key: "/repo/worktrees/task-1".to_string(),
                prompt: "pending".to_string(),
                local_images: Vec::new(),
                skill_bindings: Vec::new(),
                started_at: Instant::now(),
                start_mode: AiNewThreadStartMode::Local,
                thread_id: Some("thread-1".to_string()),
            }),
            ..AiWorkspaceState::default()
        };

        DiffViewer::restore_ai_workspace_state_after_failure_for_state(&mut workspace_state);

        assert!(workspace_state.new_thread_draft_active);
        assert!(!workspace_state.pending_new_thread_selection);
        assert_eq!(
            workspace_state
                .pending_thread_start
                .as_ref()
                .and_then(|pending| pending.thread_id.as_deref()),
            None
        );
    }

    #[test]
    fn apply_ai_snapshot_to_workspace_state_tracks_pending_thread_start_until_user_item_arrives() {
        let mut workspace_state = AiWorkspaceState {
            new_thread_draft_active: true,
            pending_new_thread_selection: true,
            pending_thread_start: Some(AiPendingThreadStart {
                workspace_key: "/repo/worktrees/task-1".to_string(),
                prompt: "Implement timeline update".to_string(),
                local_images: Vec::new(),
                skill_bindings: Vec::new(),
                started_at: Instant::now(),
                start_mode: AiNewThreadStartMode::Worktree,
                thread_id: Some("thread-1".to_string()),
            }),
            ..AiWorkspaceState::default()
        };
        let mut first_snapshot_state = AiState::default();
        first_snapshot_state.threads.insert(
            "thread-1".to_string(),
            ThreadSummary {
                id: "thread-1".to_string(),
                cwd: "/repo/worktrees/task-1".to_string(),
                title: Some("Task 1".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 20,
                updated_at: 20,
                last_sequence: 3,
            },
        );

        DiffViewer::apply_ai_snapshot_to_workspace_state(
            &mut workspace_state,
            AiSnapshot {
                state: first_snapshot_state,
                active_thread_id: Some("thread-1".to_string()),
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

        let pending = workspace_state
            .pending_thread_start
            .as_ref()
            .expect("pending thread start should persist until user item appears");
        assert_eq!(pending.thread_id.as_deref(), Some("thread-1"));

        let mut second_snapshot_state = workspace_state.state_snapshot.clone();
        second_snapshot_state.turns.insert(
            "thread-1::turn-1".to_string(),
            hunk_codex::state::TurnSummary {
                id: "turn-1".to_string(),
                thread_id: "thread-1".to_string(),
                status: hunk_codex::state::TurnStatus::Completed,
                last_sequence: 5,
            },
        );
        second_snapshot_state.items.insert(
            "thread-1::turn-1::item-user".to_string(),
            hunk_codex::state::ItemSummary {
                id: "item-user".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                kind: "userMessage".to_string(),
                status: ItemStatus::Completed,
                content: "Implement timeline update".to_string(),
                display_metadata: None,
                last_sequence: 5,
            },
        );

        DiffViewer::apply_ai_snapshot_to_workspace_state(
            &mut workspace_state,
            AiSnapshot {
                state: second_snapshot_state,
                active_thread_id: Some("thread-1".to_string()),
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

        assert!(workspace_state.pending_thread_start.is_none());
    }

    #[test]
    fn apply_ai_snapshot_to_workspace_state_waits_for_explicit_pending_thread_id() {
        let mut workspace_state = AiWorkspaceState {
            new_thread_draft_active: true,
            pending_new_thread_selection: true,
            pending_thread_start: Some(AiPendingThreadStart {
                workspace_key: "/repo/worktrees/task-1".to_string(),
                prompt: "Implement timeline update".to_string(),
                local_images: Vec::new(),
                skill_bindings: Vec::new(),
                started_at: Instant::now(),
                start_mode: AiNewThreadStartMode::Worktree,
                thread_id: None,
            }),
            ..AiWorkspaceState::default()
        };
        let mut snapshot_state = AiState::default();
        snapshot_state.threads.insert(
            "thread-old".to_string(),
            ThreadSummary {
                id: "thread-old".to_string(),
                cwd: "/repo/worktrees/task-1".to_string(),
                title: Some("Existing worktree thread".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 10,
                updated_at: 10,
                last_sequence: 1,
            },
        );

        DiffViewer::apply_ai_snapshot_to_workspace_state(
            &mut workspace_state,
            AiSnapshot {
                state: snapshot_state,
                active_thread_id: Some("thread-old".to_string()),
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

        assert!(workspace_state.new_thread_draft_active);
        assert!(workspace_state.pending_new_thread_selection);
        assert_eq!(workspace_state.selected_thread_id, None);
        assert_eq!(
            workspace_state
                .pending_thread_start
                .as_ref()
                .and_then(|pending| pending.thread_id.as_deref()),
            None
        );
    }

    #[test]
    fn apply_ai_snapshot_to_workspace_state_prefers_explicit_pending_thread_id_over_old_active_thread() {
        let mut workspace_state = AiWorkspaceState {
            new_thread_draft_active: true,
            pending_new_thread_selection: true,
            pending_thread_start: Some(AiPendingThreadStart {
                workspace_key: "/repo/worktrees/task-1".to_string(),
                prompt: "Implement timeline update".to_string(),
                local_images: Vec::new(),
                skill_bindings: Vec::new(),
                started_at: Instant::now(),
                start_mode: AiNewThreadStartMode::Worktree,
                thread_id: Some("thread-new".to_string()),
            }),
            ..AiWorkspaceState::default()
        };
        let mut snapshot_state = AiState::default();
        snapshot_state.threads.insert(
            "thread-old".to_string(),
            ThreadSummary {
                id: "thread-old".to_string(),
                cwd: "/repo/worktrees/task-1".to_string(),
                title: Some("Existing worktree thread".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 10,
                updated_at: 10,
                last_sequence: 1,
            },
        );
        snapshot_state.threads.insert(
            "thread-new".to_string(),
            ThreadSummary {
                id: "thread-new".to_string(),
                cwd: "/repo/worktrees/task-1".to_string(),
                title: Some("New worktree thread".to_string()),
                status: ThreadLifecycleStatus::Idle,
                created_at: 20,
                updated_at: 20,
                last_sequence: 2,
            },
        );

        DiffViewer::apply_ai_snapshot_to_workspace_state(
            &mut workspace_state,
            AiSnapshot {
                state: snapshot_state,
                active_thread_id: Some("thread-old".to_string()),
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

        assert!(!workspace_state.new_thread_draft_active);
        assert!(!workspace_state.pending_new_thread_selection);
        assert_eq!(
            workspace_state.selected_thread_id.as_deref(),
            Some("thread-new")
        );
    }

    #[test]
    fn ai_composer_draft_key_prefers_thread_over_workspace() {
        assert_eq!(
            ai_composer_draft_key(Some("thread-a"), Some("/repo")),
            Some(AiComposerDraftKey::Thread("thread-a".to_string()))
        );
        assert_eq!(
            ai_composer_draft_key(None, Some("/repo")),
            Some(AiComposerDraftKey::Workspace("/repo".to_string()))
        );
        assert_eq!(ai_composer_draft_key(None, None), None);
    }

    #[test]
    fn ai_composer_prompt_for_target_is_scoped_to_each_thread() {
        let drafts = BTreeMap::from([
            (
                AiComposerDraftKey::Thread("thread-a".to_string()),
                AiComposerDraft {
                    prompt: "draft-a".to_string(),
                    local_images: vec![PathBuf::from("/tmp/a.png")],
                    skill_bindings: Vec::new(),
                },
            ),
            (
                AiComposerDraftKey::Thread("thread-b".to_string()),
                AiComposerDraft {
                    prompt: "draft-b".to_string(),
                    local_images: vec![PathBuf::from("/tmp/b.png")],
                    skill_bindings: Vec::new(),
                },
            ),
            (
                AiComposerDraftKey::Workspace("/repo".to_string()),
                AiComposerDraft {
                    prompt: "workspace-draft".to_string(),
                    local_images: Vec::new(),
                    skill_bindings: Vec::new(),
                },
            ),
        ]);

        let thread_a = ai_composer_draft_key(Some("thread-a"), Some("/repo")).expect("thread key");
        let thread_b = ai_composer_draft_key(Some("thread-b"), Some("/repo")).expect("thread key");
        let workspace = ai_composer_draft_key(None, Some("/repo")).expect("workspace key");

        assert_eq!(
            ai_composer_prompt_for_target(&drafts, Some(&thread_a)),
            "draft-a"
        );
        assert_eq!(
            ai_composer_prompt_for_target(&drafts, Some(&thread_b)),
            "draft-b"
        );
        assert_eq!(
            ai_composer_prompt_for_target(&drafts, Some(&workspace)),
            "workspace-draft"
        );
    }

    #[test]
    fn thread_start_mode_for_workspace_uses_matching_target_kind() {
        let workspace_targets = vec![
            workspace_target(
                "primary",
                WorkspaceTargetKind::PrimaryCheckout,
                "/repo",
                "Primary Checkout",
            ),
            workspace_target(
                "worktree:worktree-3",
                WorkspaceTargetKind::LinkedWorktree,
                "/tmp/hunk/worktrees/repo/worktree-3",
                "worktree-3",
            ),
        ];

        assert_eq!(
            ai_thread_start_mode_for_workspace(
                Some(std::path::Path::new("/repo")),
                workspace_targets.as_slice(),
                std::path::Path::new("/repo"),
            ),
            Some(AiNewThreadStartMode::Local),
        );
        assert_eq!(
            ai_thread_start_mode_for_workspace(
                Some(std::path::Path::new("/repo")),
                workspace_targets.as_slice(),
                std::path::Path::new("/tmp/hunk/worktrees/repo/worktree-3"),
            ),
            Some(AiNewThreadStartMode::Worktree),
        );
    }

    #[test]
    fn thread_start_mode_for_workspace_falls_back_to_repo_root_when_catalog_is_missing() {
        assert_eq!(
            ai_thread_start_mode_for_workspace(
                Some(std::path::Path::new("/repo")),
                &[],
                std::path::Path::new("/repo"),
            ),
            Some(AiNewThreadStartMode::Local),
        );
        assert_eq!(
            ai_thread_start_mode_for_workspace(
                Some(std::path::Path::new("/repo")),
                &[],
                std::path::Path::new("/repo-worktree"),
            ),
            Some(AiNewThreadStartMode::Worktree),
        );
    }

    #[test]
    fn thread_catalog_workspace_roots_skip_visible_primary_checkout() {
        with_temp_hunk_home("catalog-roots-primary", |temp_home| {
            let chats_root = resolve_ai_chats_root_path().expect("chats root should resolve");
            let chat_a = chats_root.join("chat-a");
            let chat_b = chats_root.join("chat-b");
            std::fs::create_dir_all(&chat_a).expect("chat-a should exist");
            std::fs::create_dir_all(&chat_b).expect("chat-b should exist");

            let workspace_targets = vec![
                workspace_target(
                    "primary",
                    WorkspaceTargetKind::PrimaryCheckout,
                    "/repo",
                    "Primary Checkout",
                ),
                workspace_target(
                    "worktree:task-1",
                    WorkspaceTargetKind::LinkedWorktree,
                    "/repo/worktrees/task-1",
                    "task-1",
                ),
                workspace_target(
                    "worktree:task-1-duplicate",
                    WorkspaceTargetKind::LinkedWorktree,
                    "/repo/worktrees/task-1",
                    "task-1",
                ),
                workspace_target(
                    "worktree:task-2",
                    WorkspaceTargetKind::LinkedWorktree,
                    "/repo/worktrees/task-2",
                    "task-2",
                ),
            ];

            let roots =
                ai_thread_catalog_workspace_roots(workspace_targets.as_slice(), Some("/repo"));
            assert_eq!(
                roots,
                vec![
                    PathBuf::from("/repo/worktrees/task-1"),
                    PathBuf::from("/repo/worktrees/task-2"),
                    chats_root,
                    chat_a,
                    chat_b,
                ]
            );
            let _ = std::fs::remove_dir_all(temp_home);
        });
    }

    #[test]
    fn thread_catalog_workspace_roots_still_skip_visible_worktree() {
        with_temp_hunk_home("catalog-roots-worktree", |_| {
            let chats_root = resolve_ai_chats_root_path().expect("chats root should resolve");
            let chat_a = chats_root.join("chat-a");
            std::fs::create_dir_all(&chat_a).expect("chat-a should exist");
            let workspace_targets = vec![
                workspace_target(
                    "primary",
                    WorkspaceTargetKind::PrimaryCheckout,
                    "/repo",
                    "Primary Checkout",
                ),
                workspace_target(
                    "worktree:task-1",
                    WorkspaceTargetKind::LinkedWorktree,
                    "/repo/worktrees/task-1",
                    "task-1",
                ),
                workspace_target(
                    "worktree:task-2",
                    WorkspaceTargetKind::LinkedWorktree,
                    "/repo/worktrees/task-2",
                    "task-2",
                ),
            ];

            let roots = ai_thread_catalog_workspace_roots(
                workspace_targets.as_slice(),
                Some("/repo/worktrees/task-1"),
            );
            assert_eq!(
                roots,
                vec![PathBuf::from("/repo"), PathBuf::from("/repo/worktrees/task-2"), chats_root, chat_a]
            );
        });
    }

    #[test]
    fn workspace_catalog_inputs_include_all_projects_and_skip_visible_workspace_root() {
        with_temp_hunk_home("catalog-inputs-projects", |_| {
            let chats_root = resolve_ai_chats_root_path().expect("chats root should resolve");
            let chat_a = chats_root.join("chat-a");
            std::fs::create_dir_all(&chat_a).expect("chat-a should exist");
            let repo_a_targets = vec![
                workspace_target(
                    "primary",
                    WorkspaceTargetKind::PrimaryCheckout,
                    "/repo-a",
                    "Primary Checkout",
                ),
                workspace_target(
                    "worktree:task-a",
                    WorkspaceTargetKind::LinkedWorktree,
                    "/repo-a/worktrees/task-a",
                    "task-a",
                ),
            ];
            let repo_b_targets = vec![
                workspace_target(
                    "primary",
                    WorkspaceTargetKind::PrimaryCheckout,
                    "/repo-b",
                    "Primary Checkout",
                ),
                workspace_target(
                    "worktree:task-b",
                    WorkspaceTargetKind::LinkedWorktree,
                    "/repo-b/worktrees/task-b",
                    "task-b",
                ),
            ];

            let inputs = ai_workspace_catalog_inputs_from_target_sets(
                &[repo_a_targets, repo_b_targets],
                &[],
                Some("/repo-b/worktrees/task-b"),
            );

            assert_eq!(
                inputs.known_workspace_keys,
                BTreeSet::from([
                    chats_root.to_string_lossy().to_string(),
                    chat_a.to_string_lossy().to_string(),
                    "/repo-a".to_string(),
                    "/repo-a/worktrees/task-a".to_string(),
                    "/repo-b".to_string(),
                    "/repo-b/worktrees/task-b".to_string(),
                ])
            );
            assert_eq!(
                inputs.workspace_roots,
                vec![
                    PathBuf::from("/repo-a"),
                    PathBuf::from("/repo-a/worktrees/task-a"),
                    PathBuf::from("/repo-b"),
                    chats_root,
                    chat_a,
                ]
            );
        });
    }

    #[test]
    fn workspace_catalog_inputs_keep_fallback_project_roots_for_projects_without_targets() {
        with_temp_hunk_home("catalog-inputs-fallback", |_| {
            let chats_root = resolve_ai_chats_root_path().expect("chats root should resolve");
            let chat_a = chats_root.join("chat-a");
            std::fs::create_dir_all(&chat_a).expect("chat-a should exist");
            let repo_a_targets = vec![workspace_target(
                "primary",
                WorkspaceTargetKind::PrimaryCheckout,
                "/repo-a",
                "Primary Checkout",
            )];

            let inputs = ai_workspace_catalog_inputs_from_target_sets(
                &[repo_a_targets],
                &[PathBuf::from("/repo-b")],
                Some("/repo-a"),
            );

            assert_eq!(
                inputs.known_workspace_keys,
                BTreeSet::from([
                    chats_root.to_string_lossy().to_string(),
                    chat_a.to_string_lossy().to_string(),
                    "/repo-a".to_string(),
                    "/repo-b".to_string(),
                ])
            );
            assert_eq!(
                inputs.workspace_roots,
                vec![PathBuf::from("/repo-b"), chats_root, chat_a]
            );
        });
    }

    #[test]
    fn thread_catalog_state_replaces_snapshot_and_selects_active_thread() {
        let mut workspace_state = AiWorkspaceState {
            connection_state: AiConnectionState::Failed,
            error_message: Some("boom".to_string()),
            selected_thread_id: Some("missing-thread".to_string()),
            pending_approvals: vec![crate::app::ai_runtime::AiPendingApproval {
                request_id: "request-1".to_string(),
                thread_id: "missing-thread".to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "item-1".to_string(),
                kind: crate::app::ai_runtime::AiApprovalKind::CommandExecution,
                reason: None,
                command: None,
                cwd: None,
                grant_root: None,
            }],
            pending_user_inputs: vec![AiPendingUserInputRequest {
                request_id: "request-2".to_string(),
                thread_id: "missing-thread".to_string(),
                turn_id: "turn-2".to_string(),
                item_id: "item-2".to_string(),
                questions: Vec::new(),
            }],
            ..AiWorkspaceState::default()
        };
        let mut state_snapshot = AiState::default();
        state_snapshot.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo/worktrees/task-1".to_string(),
                title: Some("Task 1".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 10,
                updated_at: 10,
                last_sequence: 1,
            },
        );
        state_snapshot.threads.insert(
            "thread-b".to_string(),
            ThreadSummary {
                id: "thread-b".to_string(),
                cwd: "/repo/worktrees/task-1".to_string(),
                title: Some("Task 2".to_string()),
                status: ThreadLifecycleStatus::Idle,
                created_at: 20,
                updated_at: 20,
                last_sequence: 2,
            },
        );

        apply_ai_thread_catalog_to_workspace_state(
            &mut workspace_state,
            AiWorkspaceThreadCatalog {
                workspace_key: "/repo/worktrees/task-1".to_string(),
                state_snapshot,
                active_thread_id: Some("thread-a".to_string()),
            },
        );

        assert_eq!(workspace_state.connection_state, AiConnectionState::Disconnected);
        assert!(workspace_state.error_message.is_none());
        assert!(workspace_state.pending_approvals.is_empty());
        assert!(workspace_state.pending_user_inputs.is_empty());
        assert_eq!(workspace_state.selected_thread_id.as_deref(), Some("thread-a"));
        assert!(workspace_state.state_snapshot.threads.contains_key("thread-a"));
        assert!(workspace_state.state_snapshot.threads.contains_key("thread-b"));
    }

    #[test]
    fn thread_mode_picker_state_is_editable_only_for_pre_send_draft() {
        assert_eq!(
            resolved_ai_thread_mode_picker_state(
                Some(AiNewThreadStartMode::Worktree),
                AiNewThreadStartMode::Local,
                true,
                false,
            ),
            (AiNewThreadStartMode::Local, true),
        );
        assert_eq!(
            resolved_ai_thread_mode_picker_state(
                Some(AiNewThreadStartMode::Local),
                AiNewThreadStartMode::Worktree,
                true,
                true,
            ),
            (AiNewThreadStartMode::Worktree, false),
        );
    }

    #[test]
    fn thread_mode_picker_state_follows_selected_thread_when_not_drafting() {
        assert_eq!(
            resolved_ai_thread_mode_picker_state(
                Some(AiNewThreadStartMode::Worktree),
                AiNewThreadStartMode::Local,
                false,
                false,
            ),
            (AiNewThreadStartMode::Worktree, false),
        );
        assert_eq!(
            resolved_ai_thread_mode_picker_state(
                None,
                AiNewThreadStartMode::Local,
                false,
                false,
            ),
            (AiNewThreadStartMode::Local, false),
        );
    }

    #[test]
    fn preferred_ai_worktree_base_branch_name_prefers_explicit_default_branch() {
        let branches = vec![
            local_branch("feature/current", true),
            local_branch("main", false),
            local_branch("release", false),
        ];

        assert_eq!(
            preferred_ai_worktree_base_branch_name(
                branches.as_slice(),
                Some("release"),
                Some("feature/current"),
            ),
            Some("release".to_string())
        );
    }

    #[test]
    fn preferred_ai_worktree_base_branch_name_falls_back_to_main_then_current_then_first() {
        let branches = vec![
            local_branch("feature/current", true),
            local_branch("main", false),
            local_branch("release", false),
        ];
        assert_eq!(
            preferred_ai_worktree_base_branch_name(
                branches.as_slice(),
                Some("missing"),
                Some("feature/current"),
            ),
            Some("main".to_string())
        );

        let branches = vec![local_branch("feature/current", true), local_branch("release", false)];
        assert_eq!(
            preferred_ai_worktree_base_branch_name(
                branches.as_slice(),
                Some("missing"),
                Some("feature/current"),
            ),
            Some("feature/current".to_string())
        );

        let branches = vec![local_branch("release", false), local_branch("topic", false)];
        assert_eq!(
            preferred_ai_worktree_base_branch_name(branches.as_slice(), None, Some("missing")),
            Some("release".to_string())
        );
    }

    #[test]
    fn ai_pending_steer_seed_content_formats_text_and_images() {
        assert_eq!(
            ai_pending_steer_seed_content("Continue from the last failure", &[]).as_deref(),
            Some("Continue from the last failure")
        );
        assert_eq!(
            ai_pending_steer_seed_content("", &[PathBuf::from("/tmp/screenshot.png")]).as_deref(),
            Some("[image] screenshot.png")
        );
        assert_eq!(
            ai_pending_steer_seed_content(
                "Check the attached regression",
                &[
                    PathBuf::from("/tmp/first.png"),
                    PathBuf::from("/tmp/second.png"),
                ],
            )
            .as_deref(),
            Some("Check the attached regression\n[images] first.png, second.png")
        );
    }

    #[test]
    fn ai_composer_retained_thread_ids_include_hidden_workspace_threads() {
        let mut visible_state = AiState::default();
        visible_state.threads.insert(
            "thread-visible".to_string(),
            ThreadSummary {
                id: "thread-visible".to_string(),
                cwd: "/repo-a".to_string(),
                title: Some("Visible".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );

        let mut hidden_workspace_state = AiWorkspaceState::default();
        hidden_workspace_state.state_snapshot.threads.insert(
            "thread-hidden".to_string(),
            ThreadSummary {
                id: "thread-hidden".to_string(),
                cwd: "/repo-b".to_string(),
                title: Some("Hidden".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 2,
                updated_at: 2,
                last_sequence: 2,
            },
        );

        let thread_ids = ai_composer_retained_thread_ids(
            &visible_state,
            &BTreeMap::from([("/repo-b".to_string(), hidden_workspace_state)]),
        );

        assert!(thread_ids.contains("thread-visible"));
        assert!(thread_ids.contains("thread-hidden"));
    }

    #[test]
    fn reconcile_ai_pending_steers_matches_committed_user_messages_in_order() {
        let mut pending_steers = vec![
            AiPendingSteer {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                prompt: "First follow-up".to_string(),
                local_images: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                accepted_after_sequence: 5,
                started_at: Instant::now(),
            },
            AiPendingSteer {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                prompt: "Second follow-up".to_string(),
                local_images: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                accepted_after_sequence: 5,
                started_at: Instant::now(),
            },
        ];
        let mut state = AiState::default();
        state.items.insert(
            "thread-1::turn-1::item-1".to_string(),
            hunk_codex::state::ItemSummary {
                id: "item-1".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                kind: "userMessage".to_string(),
                status: ItemStatus::Completed,
                content: "First follow-up".to_string(),
                display_metadata: None,
                last_sequence: 6,
            },
        );
        state.items.insert(
            "thread-1::turn-1::item-2".to_string(),
            hunk_codex::state::ItemSummary {
                id: "item-2".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                kind: "userMessage".to_string(),
                status: ItemStatus::Completed,
                content: "Second follow-up".to_string(),
                display_metadata: None,
                last_sequence: 7,
            },
        );

        reconcile_ai_pending_steers(&mut pending_steers, &state);

        assert!(pending_steers.is_empty());
    }

    #[test]
    fn reconcile_ai_pending_steers_matches_attachment_messages_despite_whitespace_differences() {
        let mut pending_steers = vec![AiPendingSteer {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            prompt: "Check the attached regression".to_string(),
            local_images: vec![PathBuf::from("/tmp/screenshot.png")],
            selected_skills: Vec::new(),
            skill_bindings: Vec::new(),
            accepted_after_sequence: 5,
            started_at: Instant::now(),
        }];
        let mut state = AiState::default();
        state.items.insert(
            "thread-1::turn-1::item-1".to_string(),
            hunk_codex::state::ItemSummary {
                id: "item-1".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                kind: "userMessage".to_string(),
                status: ItemStatus::Completed,
                content: "Check the attached regression\n\n[image] screenshot.png\n".to_string(),
                display_metadata: None,
                last_sequence: 6,
            },
        );

        reconcile_ai_pending_steers(&mut pending_steers, &state);

        assert!(pending_steers.is_empty());
    }

    #[test]
    fn reconcile_ai_pending_steers_matches_attachment_messages_with_inline_image_roundtrip() {
        let mut pending_steers = vec![AiPendingSteer {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            prompt: "Check the attached regression".to_string(),
            local_images: vec![PathBuf::from("/tmp/screenshot.png")],
            selected_skills: Vec::new(),
            skill_bindings: Vec::new(),
            accepted_after_sequence: 5,
            started_at: Instant::now(),
        }];
        let mut state = AiState::default();
        state.items.insert(
            "thread-1::turn-1::item-1".to_string(),
            timeline_tool_item(
                "item-1",
                "thread-1",
                "turn-1",
                "userMessage",
                ItemStatus::Completed,
                "Check the attached regression\n[image]",
                "{}",
                6,
            ),
        );

        reconcile_ai_pending_steers(&mut pending_steers, &state);

        assert!(pending_steers.is_empty());
    }

    #[test]
    fn reconcile_ai_pending_steers_preserves_commas_in_image_names() {
        let mut pending_steers = vec![AiPendingSteer {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            prompt: "Check the attached regression".to_string(),
            local_images: vec![PathBuf::from("/tmp/foo,1.png")],
            selected_skills: Vec::new(),
            skill_bindings: Vec::new(),
            accepted_after_sequence: 5,
            started_at: Instant::now(),
        }];
        let mut state = AiState::default();
        state.items.insert(
            "thread-1::turn-1::item-1".to_string(),
            timeline_tool_item(
                "item-1",
                "thread-1",
                "turn-1",
                "userMessage",
                ItemStatus::Completed,
                "Check the attached regression\n[image] foo,1.png",
                "{}",
                6,
            ),
        );

        reconcile_ai_pending_steers(&mut pending_steers, &state);

        assert!(pending_steers.is_empty());
    }

    #[test]
    fn reconcile_ai_pending_steers_blocks_later_match_until_earlier_message_commits() {
        let mut pending_steers = vec![
            AiPendingSteer {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                prompt: "First follow-up".to_string(),
                local_images: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                accepted_after_sequence: 5,
                started_at: Instant::now(),
            },
            AiPendingSteer {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                prompt: "Second follow-up".to_string(),
                local_images: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                accepted_after_sequence: 5,
                started_at: Instant::now(),
            },
        ];
        let mut state = AiState::default();
        state.items.insert(
            "thread-1::turn-1::item-2".to_string(),
            hunk_codex::state::ItemSummary {
                id: "item-2".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                kind: "userMessage".to_string(),
                status: ItemStatus::Completed,
                content: "Second follow-up".to_string(),
                display_metadata: None,
                last_sequence: 7,
            },
        );

        reconcile_ai_pending_steers(&mut pending_steers, &state);

        assert_eq!(pending_steers.len(), 2);
        assert_eq!(pending_steers[0].prompt, "First follow-up");
        assert_eq!(pending_steers[1].prompt, "Second follow-up");
    }

    #[test]
    fn apply_ai_snapshot_to_workspace_state_restores_pending_steer_after_turn_completes() {
        let mut workspace_state = AiWorkspaceState {
            pending_steers: vec![AiPendingSteer {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                prompt: "Continue after the interrupt".to_string(),
                local_images: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                accepted_after_sequence: 3,
                started_at: Instant::now(),
            }],
            ..AiWorkspaceState::default()
        };
        let mut snapshot_state = AiState::default();
        snapshot_state.threads.insert(
            "thread-1".to_string(),
            ThreadSummary {
                id: "thread-1".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Thread".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 1,
                last_sequence: 4,
            },
        );
        snapshot_state.turns.insert(
            hunk_codex::state::turn_storage_key("thread-1", "turn-1"),
            hunk_codex::state::TurnSummary {
                id: "turn-1".to_string(),
                thread_id: "thread-1".to_string(),
                status: hunk_codex::state::TurnStatus::Completed,
                last_sequence: 4,
            },
        );

        let restored = DiffViewer::apply_ai_snapshot_to_workspace_state(
            &mut workspace_state,
            AiSnapshot {
                state: snapshot_state,
                active_thread_id: Some("thread-1".to_string()),
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

        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].prompt, "Continue after the interrupt");
        assert!(workspace_state.pending_steers.is_empty());
    }

    #[test]
    fn take_restorable_ai_pending_steers_keeps_in_progress_turns() {
        let mut pending_steers = vec![
            AiPendingSteer {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-active".to_string(),
                prompt: "Keep waiting".to_string(),
                local_images: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                accepted_after_sequence: 1,
                started_at: Instant::now(),
            },
            AiPendingSteer {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-finished".to_string(),
                prompt: "Restore me".to_string(),
                local_images: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                accepted_after_sequence: 1,
                started_at: Instant::now(),
            },
        ];
        let mut state = AiState::default();
        state.turns.insert(
            hunk_codex::state::turn_storage_key("thread-1", "turn-active"),
            hunk_codex::state::TurnSummary {
                id: "turn-active".to_string(),
                thread_id: "thread-1".to_string(),
                status: hunk_codex::state::TurnStatus::InProgress,
                last_sequence: 2,
            },
        );

        let restored = take_restorable_ai_pending_steers(&mut pending_steers, &state);

        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].prompt, "Restore me");
        assert_eq!(pending_steers.len(), 1);
        assert_eq!(pending_steers[0].prompt, "Keep waiting");
    }

    #[test]
    fn auth_required_message_requires_sign_in_when_account_missing() {
        assert_eq!(
            ai_auth_required_message(None, true, None),
            Some(AI_AUTH_REQUIRED_MESSAGE.to_string())
        );
        assert_eq!(ai_auth_required_message(None, true, Some("login-1")), None);
        assert_eq!(
            ai_auth_required_message(None, false, None),
            None
        );
    }

    #[test]
    fn prominent_worker_status_error_recognizes_auth_failures() {
        assert_eq!(
            ai_prominent_worker_status_error(
                "Unable to read account state: json-rpc server error 401: unauthorized"
            ),
            Some(AI_AUTH_REQUIRED_MESSAGE.to_string())
        );
        assert_eq!(
            ai_prominent_worker_status_error("ChatGPT login failed: token expired"),
            Some("ChatGPT login failed: token expired".to_string())
        );
        assert_eq!(
            ai_prominent_worker_status_error("Connected over websocket."),
            None
        );
    }

    #[test]
    fn apply_ai_snapshot_to_workspace_state_sets_auth_error_when_sign_in_required() {
        let mut workspace_state = AiWorkspaceState::default();

        DiffViewer::apply_ai_snapshot_to_workspace_state(
            &mut workspace_state,
            AiSnapshot {
                state: AiState::default(),
                active_thread_id: None,
                pending_approvals: Vec::new(),
                pending_user_inputs: Vec::new(),
                account: None,
                requires_openai_auth: true,
                pending_chatgpt_login_id: None,
                pending_chatgpt_auth_url: None,
                rate_limits: None,
                models: Vec::new(),
                experimental_features: Vec::new(),
                collaboration_modes: Vec::new(),
                skills: Vec::new(),
                include_hidden_models: false,
                mad_max_mode: false,
            },
        );

        assert_eq!(
            workspace_state.error_message.as_deref(),
            Some(AI_AUTH_REQUIRED_MESSAGE)
        );
        assert_eq!(workspace_state.connection_state, AiConnectionState::Ready);
    }
