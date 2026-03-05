#[cfg(test)]
fn item_status_chip(status: hunk_codex::state::ItemStatus) -> &'static str {
    match status {
        hunk_codex::state::ItemStatus::Started => "started",
        hunk_codex::state::ItemStatus::Streaming => "streaming",
        hunk_codex::state::ItemStatus::Completed => "completed",
    }
}

#[cfg(test)]
mod ai_tests {
    use super::ai_drop_image_attachment_status_message;
    use super::bundled_codex_executable_candidates;
    use super::codex_runtime_binary_name;
    use super::codex_runtime_platform_dir;
    use super::item_status_chip;
    use super::is_supported_ai_image_path;
    use super::is_command_name_without_path;
    use super::normalized_thread_session_state;
    use super::normalized_user_input_answers;
    use super::resolve_bundled_codex_executable_from_exe;
    use super::should_follow_timeline_output;
    use super::should_scroll_timeline_to_bottom_on_new_activity;
    use super::sorted_threads;
    use super::should_scroll_timeline_to_bottom_on_selection_change;
    use super::should_sync_selected_thread_from_active_thread;
    use super::thread_latest_timeline_sequence;
    use super::workspace_include_hidden_models;
    use super::workspace_mad_max_mode;
    use crate::app::ai_runtime::AiPendingUserInputQuestion;
    use crate::app::ai_runtime::AiPendingUserInputQuestionOption;
    use crate::app::ai_runtime::AiPendingUserInputRequest;
    use hunk_codex::state::AiState;
    use hunk_codex::state::ItemStatus;
    use hunk_codex::state::ThreadLifecycleStatus;
    use hunk_codex::state::ThreadSummary;
    use hunk_domain::state::AiThreadSessionState;
    use hunk_domain::state::AppState;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn thread_selection_change_triggers_timeline_scroll() {
        assert!(should_scroll_timeline_to_bottom_on_selection_change(
            Some("thread-a"),
            Some("thread-b"),
        ));
        assert!(should_scroll_timeline_to_bottom_on_selection_change(
            None,
            Some("thread-b"),
        ));
    }

    #[test]
    fn unchanged_or_missing_selection_does_not_trigger_scroll() {
        assert!(!should_scroll_timeline_to_bottom_on_selection_change(
            Some("thread-a"),
            Some("thread-a"),
        ));
        assert!(!should_scroll_timeline_to_bottom_on_selection_change(
            Some("thread-a"),
            None,
        ));
        assert!(!should_scroll_timeline_to_bottom_on_selection_change(None, None));
    }

    #[test]
    fn new_activity_scroll_requires_follow_mode() {
        assert!(should_scroll_timeline_to_bottom_on_new_activity(12, 11, true));
        assert!(!should_scroll_timeline_to_bottom_on_new_activity(12, 12, true));
        assert!(!should_scroll_timeline_to_bottom_on_new_activity(12, 11, false));
    }

    #[test]
    fn follow_mode_only_active_at_bottom() {
        assert!(should_follow_timeline_output(0, 0.0, 0.0));
        assert!(should_follow_timeline_output(5, -120.0, 120.0));
        assert!(!should_follow_timeline_output(5, -118.0, 120.0));
    }

    #[test]
    fn active_thread_change_updates_selection_when_current_selection_is_valid() {
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

        assert!(should_sync_selected_thread_from_active_thread(
            Some("thread-old"),
            Some("thread-new"),
            Some("thread-old"),
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
            Some("thread-a"),
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
            Some("thread-a"),
            &state,
        ));
    }

    #[test]
    fn thread_latest_timeline_sequence_uses_turn_and_item_sequences() {
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
                last_sequence: 11,
            },
        );

        assert_eq!(thread_latest_timeline_sequence(&state, "thread-a"), 11);
        assert_eq!(thread_latest_timeline_sequence(&state, "missing"), 0);
    }

    #[test]
    fn item_status_chip_labels_are_stable() {
        assert_eq!(item_status_chip(ItemStatus::Started), "started");
        assert_eq!(item_status_chip(ItemStatus::Streaming), "streaming");
        assert_eq!(item_status_chip(ItemStatus::Completed), "completed");
    }

    #[test]
    fn workspace_mad_max_mode_defaults_to_false_when_missing() {
        let state = AppState::default();
        assert!(!workspace_mad_max_mode(&state, Some("/repo")));
        assert!(!workspace_mad_max_mode(&state, None));
    }

    #[test]
    fn workspace_mad_max_mode_reads_per_workspace_flags() {
        let state = AppState {
            last_project_path: None,
            ai_workspace_mad_max: [
                ("/repo-a".to_string(), true),
                ("/repo-b".to_string(), false),
            ]
            .into_iter()
            .collect(),
            ai_workspace_include_hidden_models: Default::default(),
            ai_workspace_session_overrides: Default::default(),
        };
        assert!(workspace_mad_max_mode(&state, Some("/repo-a")));
        assert!(!workspace_mad_max_mode(&state, Some("/repo-b")));
        assert!(!workspace_mad_max_mode(&state, Some("/repo-c")));
    }

    #[test]
    fn workspace_include_hidden_models_defaults_to_false_when_missing() {
        let state = AppState::default();
        assert!(!workspace_include_hidden_models(&state, Some("/repo")));
        assert!(!workspace_include_hidden_models(&state, None));
    }

    #[test]
    fn workspace_include_hidden_models_reads_per_workspace_flags() {
        let state = AppState {
            last_project_path: None,
            ai_workspace_mad_max: Default::default(),
            ai_workspace_include_hidden_models: [
                ("/repo-a".to_string(), true),
                ("/repo-b".to_string(), false),
            ]
            .into_iter()
            .collect(),
            ai_workspace_session_overrides: Default::default(),
        };
        assert!(workspace_include_hidden_models(&state, Some("/repo-a")));
        assert!(!workspace_include_hidden_models(&state, Some("/repo-b")));
        assert!(!workspace_include_hidden_models(&state, Some("/repo-c")));
    }

    #[test]
    fn normalized_thread_session_state_drops_empty_entries() {
        assert_eq!(
            normalized_thread_session_state(AiThreadSessionState::default()),
            None
        );
    }

    #[test]
    fn normalized_thread_session_state_preserves_selected_overrides() {
        let session = AiThreadSessionState {
            model: Some("gpt-5-codex".to_string()),
            effort: Some("high".to_string()),
            collaboration_mode: Some("Plan".to_string()),
        };
        assert_eq!(
            normalized_thread_session_state(session.clone()),
            Some(session),
        );
    }

    #[test]
    fn command_name_without_path_detection_is_stable() {
        assert!(is_command_name_without_path(std::path::Path::new("codex")));
        assert!(!is_command_name_without_path(std::path::Path::new("./codex")));
        assert!(!is_command_name_without_path(std::path::Path::new("/usr/bin/codex")));
    }

    #[test]
    fn bundled_codex_resolution_picks_existing_runtime_candidate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-runtime-{unique}"));
        let exe_dir = root.join("bin");
        std::fs::create_dir_all(&exe_dir).expect("exe dir should be created");
        let exe_path = exe_dir.join("hunk");
        std::fs::write(&exe_path, "").expect("fake exe should be written");

        let runtime_path = exe_dir
            .join("codex-runtime")
            .join(codex_runtime_platform_dir())
            .join(codex_runtime_binary_name());
        std::fs::create_dir_all(
            runtime_path
                .parent()
                .expect("runtime parent should exist"),
        )
        .expect("runtime dir should be created");
        std::fs::write(&runtime_path, "").expect("runtime binary should be written");

        let resolved = resolve_bundled_codex_executable_from_exe(exe_path.as_path());
        assert_eq!(resolved, Some(runtime_path));

        let candidates = bundled_codex_executable_candidates(exe_path.as_path());
        assert!(candidates.iter().any(|candidate| candidate.ends_with(PathBuf::from("codex-runtime").join(codex_runtime_platform_dir()).join(codex_runtime_binary_name()))));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn normalized_user_input_answers_defaults_to_first_option_or_blank() {
        let request = AiPendingUserInputRequest {
            request_id: "req-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: "item-1".to_string(),
            questions: vec![
                AiPendingUserInputQuestion {
                    id: "q-option".to_string(),
                    header: "Header".to_string(),
                    question: "Pick one".to_string(),
                    is_other: false,
                    is_secret: false,
                    options: vec![
                        AiPendingUserInputQuestionOption {
                            label: "first".to_string(),
                            description: "first option".to_string(),
                        },
                        AiPendingUserInputQuestionOption {
                            label: "second".to_string(),
                            description: "second option".to_string(),
                        },
                    ],
                },
                AiPendingUserInputQuestion {
                    id: "q-empty".to_string(),
                    header: "Free text".to_string(),
                    question: "Enter value".to_string(),
                    is_other: true,
                    is_secret: false,
                    options: Vec::new(),
                },
            ],
        };

        let answers = normalized_user_input_answers(&request, None);
        assert_eq!(
            answers.get("q-option"),
            Some(&vec!["first".to_string()])
        );
        assert_eq!(answers.get("q-empty"), Some(&vec![String::new()]));
    }

    #[test]
    fn normalized_user_input_answers_preserves_existing_answers() {
        let request = AiPendingUserInputRequest {
            request_id: "req-2".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: "item-2".to_string(),
            questions: vec![AiPendingUserInputQuestion {
                id: "q-option".to_string(),
                header: "Header".to_string(),
                question: "Pick one".to_string(),
                is_other: false,
                is_secret: false,
                options: vec![AiPendingUserInputQuestionOption {
                    label: "default".to_string(),
                    description: "default option".to_string(),
                }],
            }],
        };
        let previous = [("q-option".to_string(), vec!["custom".to_string()])]
            .into_iter()
            .collect();

        let answers = normalized_user_input_answers(&request, Some(&previous));
        assert_eq!(
            answers.get("q-option"),
            Some(&vec!["custom".to_string()])
        );
    }

    #[test]
    fn supported_image_path_check_is_case_insensitive() {
        assert!(is_supported_ai_image_path(std::path::Path::new("image.PNG")));
        assert!(is_supported_ai_image_path(std::path::Path::new("shot.JpEg")));
        assert!(is_supported_ai_image_path(std::path::Path::new("anim.GIF")));
    }

    #[test]
    fn unsupported_image_path_without_extension_is_rejected() {
        assert!(!is_supported_ai_image_path(std::path::Path::new("image")));
        assert!(!is_supported_ai_image_path(std::path::Path::new("archive.zip")));
    }

    #[test]
    fn drop_image_attachment_status_message_reports_expected_outcomes() {
        assert_eq!(
            ai_drop_image_attachment_status_message(1, 1),
            "Attached 1 image."
        );
        assert_eq!(
            ai_drop_image_attachment_status_message(3, 3),
            "Attached 3 images."
        );
        assert_eq!(
            ai_drop_image_attachment_status_message(3, 1),
            "Attached 1 image. Skipped 2 unsupported or duplicate files."
        );
        assert_eq!(
            ai_drop_image_attachment_status_message(1, 0),
            "Dropped file is not a supported image or is already attached."
        );
        assert_eq!(
            ai_drop_image_attachment_status_message(2, 0),
            "No dropped files were supported images or were already attached."
        );
    }
}
