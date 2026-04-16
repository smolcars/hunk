    #[test]
    fn sorted_threads_ignores_activity_updates_when_created_at_differs() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-early".to_string(),
            ThreadSummary {
                id: "thread-early".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                created_at: 5,
                updated_at: 1000,
                last_sequence: 999,
            },
        );
        state.threads.insert(
            "thread-late".to_string(),
            ThreadSummary {
                id: "thread-late".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                created_at: 10,
                updated_at: 1,
                last_sequence: 1,
            },
        );

        let sorted = sorted_threads(&state);
        assert_eq!(sorted[0].id, "thread-late");
        assert_eq!(sorted[1].id, "thread-early");
    }

    #[test]
    fn ai_branch_name_for_thread_falls_back_to_thread_title() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-1".to_string(),
            ThreadSummary {
                id: "thread-1".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Improve PR dropdown behavior".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );

        assert_eq!(
            ai_branch_name_for_thread(&state, "thread-1", "main", false),
            "ai/local/improve-pr-dropdown-behavior"
        );
    }

    #[test]
    fn active_thread_change_does_not_override_valid_local_selection() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-old".to_string(),
            ThreadSummary {
                id: "thread-old".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );
        state.threads.insert(
            "thread-new".to_string(),
            ThreadSummary {
                id: "thread-new".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                created_at: 2,
                updated_at: 2,
                last_sequence: 2,
            },
        );

        assert!(!should_sync_selected_thread_from_active_thread(
            Some("thread-old"),
            Some("thread-new"),
            false,
            &state,
        ));
    }

    #[test]
    fn unchanged_active_thread_does_not_override_local_selection() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );
        state.threads.insert(
            "thread-b".to_string(),
            ThreadSummary {
                id: "thread-b".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                created_at: 2,
                updated_at: 2,
                last_sequence: 2,
            },
        );

        assert!(!should_sync_selected_thread_from_active_thread(
            Some("thread-b"),
            Some("thread-a"),
            false,
            &state,
        ));
    }

    #[test]
    fn missing_selection_follows_active_thread() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );

        assert!(should_sync_selected_thread_from_active_thread(
            None,
            Some("thread-a"),
            false,
            &state,
        ));
    }

    #[test]
    fn snapshot_thread_change_detection_ignores_item_only_updates() {
        let mut previous = AiState::default();
        previous.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Thread".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 2,
                last_sequence: 3,
            },
        );

        let mut next = previous.clone();
        next.threads.get_mut("thread-a").expect("thread").updated_at = 4;
        next.threads.get_mut("thread-a").expect("thread").last_sequence = 7;

        assert!(!ai_snapshot_threads_changed(&previous, &next));
        assert!(!ai_snapshot_removed_thread_ids(&previous, &next));
        assert!(!ai_snapshot_removed_retainable_terminal_threads(
            &previous, &next
        ));
    }

    #[test]
    fn snapshot_thread_change_detection_flags_thread_metadata_changes() {
        let mut previous = AiState::default();
        previous.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Thread".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 2,
                last_sequence: 3,
            },
        );

        let mut next = previous.clone();
        next.threads.get_mut("thread-a").expect("thread").title = Some("Renamed".to_string());
        next.threads.get_mut("thread-a").expect("thread").updated_at = 4;
        next.threads.get_mut("thread-a").expect("thread").last_sequence = 7;

        assert!(ai_snapshot_threads_changed(&previous, &next));
    }

    #[test]
    fn snapshot_thread_change_detection_flags_removed_threads() {
        let mut previous = AiState::default();
        previous.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Thread".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 2,
                last_sequence: 3,
            },
        );

        let next = AiState::default();

        assert!(ai_snapshot_threads_changed(&previous, &next));
        assert!(ai_snapshot_removed_thread_ids(&previous, &next));
        assert!(ai_snapshot_removed_retainable_terminal_threads(
            &previous, &next
        ));
    }

    #[test]
    fn snapshot_terminal_retainable_detection_flags_archived_threads() {
        let mut previous = AiState::default();
        previous.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Thread".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 2,
                last_sequence: 3,
            },
        );

        let mut next = previous.clone();
        next.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Thread".to_string()),
                status: ThreadLifecycleStatus::Archived,
                created_at: 1,
                updated_at: 4,
                last_sequence: 5,
            },
        );

        assert!(ai_snapshot_threads_changed(&previous, &next));
        assert!(!ai_snapshot_removed_thread_ids(&previous, &next));
        assert!(ai_snapshot_removed_retainable_terminal_threads(
            &previous, &next
        ));
    }

    #[test]
    fn workspace_draft_preserves_empty_selection_even_with_active_thread() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );

        assert!(!should_sync_selected_thread_from_active_thread(
            None,
            Some("thread-a"),
            true,
            &state,
        ));
    }

    #[test]
    fn thread_latest_timeline_sequence_uses_turn_item_and_plan_sequences() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 1,
                last_sequence: 3,
            },
        );
        state.turns.insert(
            "turn-a".to_string(),
            hunk_codex::state::TurnSummary {
                id: "turn-a".to_string(),
                thread_id: "thread-a".to_string(),
                collaboration_mode: None,
                status: hunk_codex::state::TurnStatus::InProgress,
                last_sequence: 7,
            },
        );
        state.items.insert(
            "item-a".to_string(),
            hunk_codex::state::ItemSummary {
                id: "item-a".to_string(),
                thread_id: "thread-a".to_string(),
                turn_id: "turn-a".to_string(),
                kind: "agentMessage".to_string(),
                status: ItemStatus::Streaming,
                content: "chunk".to_string(),
                display_metadata: None,
                last_sequence: 11,
            },
        );
        state.turn_plans.insert(
            hunk_codex::state::turn_storage_key("thread-a", "turn-a"),
            hunk_codex::state::TurnPlanSummary {
                thread_id: "thread-a".to_string(),
                turn_id: "turn-a".to_string(),
                explanation: Some("Updated plan".to_string()),
                steps: vec![hunk_codex::state::TurnPlanStepSummary {
                    step: "Fix ordering".to_string(),
                    status: hunk_codex::state::TurnPlanStepStatus::InProgress,
                }],
                created_sequence: 5,
                last_sequence: 13,
            },
        );

        assert_eq!(thread_latest_timeline_sequence(&state, "thread-a"), 13);
        assert_eq!(thread_latest_timeline_sequence(&state, "missing"), 0);
    }

    #[test]
    fn untitled_thread_with_turns_produces_refresh_key() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );
        state.turns.insert(
            "turn-a".to_string(),
            hunk_codex::state::TurnSummary {
                id: "turn-a".to_string(),
                thread_id: "thread-a".to_string(),
                collaboration_mode: None,
                status: hunk_codex::state::TurnStatus::InProgress,
                last_sequence: 7,
            },
        );

        assert_eq!(
            thread_metadata_refresh_key(&state, "thread-a").as_deref(),
            Some("turn-a:in-progress")
        );

        state
            .turns
            .get_mut("turn-a")
            .expect("turn should exist")
            .status = hunk_codex::state::TurnStatus::Completed;
        assert_eq!(
            thread_metadata_refresh_key(&state, "thread-a").as_deref(),
            Some("turn-a:completed")
        );
    }

    #[test]
    fn titled_or_empty_thread_has_no_refresh_key() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Named".to_string()),
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );
        state.turns.insert(
            "turn-a".to_string(),
            hunk_codex::state::TurnSummary {
                id: "turn-a".to_string(),
                thread_id: "thread-a".to_string(),
                collaboration_mode: None,
                status: hunk_codex::state::TurnStatus::InProgress,
                last_sequence: 7,
            },
        );

        assert_eq!(thread_metadata_refresh_key(&state, "thread-a"), None);

        state
            .threads
            .get_mut("thread-a")
            .expect("thread should exist")
            .title = None;
        state.turns.clear();
        assert_eq!(thread_metadata_refresh_key(&state, "thread-a"), None);
    }

    #[test]
    fn prompt_send_waits_while_ai_is_connecting() {
        assert!(ai_prompt_send_waiting_on_connection(
            AiConnectionState::Connecting,
            false,
        ));
        assert!(ai_prompt_send_waiting_on_connection(
            AiConnectionState::Ready,
            true,
        ));
        assert!(ai_prompt_send_waiting_on_connection(
            AiConnectionState::Reconnecting,
            false,
        ));
        assert!(!ai_prompt_send_waiting_on_connection(
            AiConnectionState::Ready,
            false,
        ));
    }

    #[test]
    fn thread_metadata_refresh_attempts_are_rate_limited_and_bounded() {
        let mut state = AiState::default();
        state.threads.insert(
            "thread-a".to_string(),
            ThreadSummary {
                id: "thread-a".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );
        state.turns.insert(
            "turn-a".to_string(),
            hunk_codex::state::TurnSummary {
                id: "turn-a".to_string(),
                thread_id: "thread-a".to_string(),
                collaboration_mode: None,
                status: hunk_codex::state::TurnStatus::InProgress,
                last_sequence: 7,
            },
        );

        let mut refresh_state_by_thread = BTreeMap::new();
        let now = Instant::now();
        assert_eq!(
            next_thread_metadata_refresh_attempt(
                &mut refresh_state_by_thread,
                &state,
                "thread-a",
                now,
            ),
            Some(("turn-a:in-progress".to_string(), 1))
        );

        refresh_state_by_thread.insert(
            "thread-a".to_string(),
            AiThreadTitleRefreshState {
                key: "turn-a:in-progress".to_string(),
                attempts: 1,
                in_flight: true,
                last_attempt_at: now,
            },
        );
        assert_eq!(
            next_thread_metadata_refresh_attempt(
                &mut refresh_state_by_thread,
                &state,
                "thread-a",
                now + Duration::from_millis(100),
            ),
            None
        );
        assert!(
            !refresh_state_by_thread
                .get("thread-a")
                .expect("state should exist")
                .in_flight
        );
        assert_eq!(
            next_thread_metadata_refresh_attempt(
                &mut refresh_state_by_thread,
                &state,
                "thread-a",
                now + Duration::from_millis(500),
            ),
            None
        );
        assert_eq!(
            next_thread_metadata_refresh_attempt(
                &mut refresh_state_by_thread,
                &state,
                "thread-a",
                now + super::AI_THREAD_TITLE_REFRESH_RETRY_INTERVAL,
            ),
            Some(("turn-a:in-progress".to_string(), 2))
        );

        refresh_state_by_thread.insert(
            "thread-a".to_string(),
            AiThreadTitleRefreshState {
                key: "turn-a:in-progress".to_string(),
                attempts: super::AI_THREAD_TITLE_REFRESH_MAX_ATTEMPTS,
                in_flight: false,
                last_attempt_at: now,
            },
        );
        assert_eq!(
            next_thread_metadata_refresh_attempt(
                &mut refresh_state_by_thread,
                &state,
                "thread-a",
                now + super::AI_THREAD_TITLE_REFRESH_RETRY_INTERVAL,
            ),
            None
        );
    }

    #[test]
    fn item_status_chip_labels_are_stable() {
        assert_eq!(item_status_chip(ItemStatus::Started), "started");
        assert_eq!(item_status_chip(ItemStatus::Streaming), "streaming");
        assert_eq!(item_status_chip(ItemStatus::Completed), "completed");
    }
