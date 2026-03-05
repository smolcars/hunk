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
    use super::ai_attachment_status_message;
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
    use super::should_reset_ai_timeline_measurements;
    use super::should_scroll_timeline_to_bottom_on_new_activity;
    use super::sorted_threads;
    use super::timeline_turn_ids_by_thread;
    use super::timeline_row_ids_with_height_changes;
    use super::timeline_visible_row_ids_for_turns;
    use super::timeline_visible_turn_ids;
    use super::should_scroll_timeline_to_bottom_on_selection_change;
    use super::should_sync_selected_thread_from_active_thread;
    use super::thread_latest_timeline_sequence;
    use super::workspace_include_hidden_models;
    use super::workspace_mad_max_mode;
    use crate::app::AiTimelineRow;
    use crate::app::AiTimelineRowSource;
    use crate::app::ai_runtime::AiPendingUserInputQuestion;
    use crate::app::ai_runtime::AiPendingUserInputQuestionOption;
    use crate::app::ai_runtime::AiPendingUserInputRequest;
    use hunk_codex::state::AiState;
    use hunk_codex::state::ItemStatus;
    use hunk_codex::state::ThreadLifecycleStatus;
    use hunk_codex::state::ThreadSummary;
    use hunk_domain::state::AiThreadSessionState;
    use hunk_domain::state::AppState;
    use std::collections::{BTreeMap, BTreeSet};
    use std::env;
    use std::path::PathBuf;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
    fn timeline_visible_turn_ids_paginates_from_newest_turns() {
        let turn_ids = vec![
            "turn-1".to_string(),
            "turn-2".to_string(),
            "turn-3".to_string(),
            "turn-4".to_string(),
        ];

        let (total, visible, hidden, visible_turn_ids) =
            timeline_visible_turn_ids(turn_ids.as_slice(), 2);
        assert_eq!(total, 4);
        assert_eq!(visible, 2);
        assert_eq!(hidden, 2);
        assert_eq!(visible_turn_ids, vec!["turn-3".to_string(), "turn-4".to_string()]);
    }

    #[test]
    fn timeline_turn_ids_by_thread_uses_plain_turn_ids_instead_of_storage_keys() {
        let mut state = AiState::default();
        state.turns.insert(
            hunk_codex::state::turn_storage_key("thread-1", "turn-2"),
            hunk_codex::state::TurnSummary {
                id: "turn-2".to_string(),
                thread_id: "thread-1".to_string(),
                status: hunk_codex::state::TurnStatus::Completed,
                last_sequence: 2,
            },
        );
        state.turns.insert(
            hunk_codex::state::turn_storage_key("thread-1", "turn-1"),
            hunk_codex::state::TurnSummary {
                id: "turn-1".to_string(),
                thread_id: "thread-1".to_string(),
                status: hunk_codex::state::TurnStatus::Completed,
                last_sequence: 1,
            },
        );

        let turn_ids_by_thread = timeline_turn_ids_by_thread(&state);
        assert_eq!(
            turn_ids_by_thread.get("thread-1"),
            Some(&vec!["turn-1".to_string(), "turn-2".to_string()]),
        );
    }

    #[test]
    fn timeline_visible_row_ids_filter_by_visible_turns_and_preserve_row_order() {
        let row_ids = vec![
            "item:1".to_string(),
            "item:2".to_string(),
            "turn-diff:2".to_string(),
            "item:3".to_string(),
            "item:missing".to_string(),
        ];
        let rows_by_id = BTreeMap::from([
            (
                "item:1".to_string(),
                AiTimelineRow {
                    id: "item:1".to_string(),
                    thread_id: "thread-1".to_string(),
                    turn_id: "turn-1".to_string(),
                    last_sequence: 1,
                    source: AiTimelineRowSource::Item {
                        item_key: "item-1".to_string(),
                    },
                },
            ),
            (
                "item:2".to_string(),
                AiTimelineRow {
                    id: "item:2".to_string(),
                    thread_id: "thread-1".to_string(),
                    turn_id: "turn-2".to_string(),
                    last_sequence: 2,
                    source: AiTimelineRowSource::Item {
                        item_key: "item-2".to_string(),
                    },
                },
            ),
            (
                "turn-diff:2".to_string(),
                AiTimelineRow {
                    id: "turn-diff:2".to_string(),
                    thread_id: "thread-1".to_string(),
                    turn_id: "turn-2".to_string(),
                    last_sequence: 2,
                    source: AiTimelineRowSource::TurnDiff {
                        turn_key: "turn-2".to_string(),
                    },
                },
            ),
            (
                "item:3".to_string(),
                AiTimelineRow {
                    id: "item:3".to_string(),
                    thread_id: "thread-1".to_string(),
                    turn_id: "turn-3".to_string(),
                    last_sequence: 3,
                    source: AiTimelineRowSource::Item {
                        item_key: "item-3".to_string(),
                    },
                },
            ),
        ]);
        let visible_turn_ids = vec!["turn-2".to_string(), "turn-3".to_string()];

        let visible_rows = timeline_visible_row_ids_for_turns(
            row_ids.as_slice(),
            &rows_by_id,
            visible_turn_ids.as_slice(),
        );
        assert_eq!(
            visible_rows,
            vec![
                "item:2".to_string(),
                "turn-diff:2".to_string(),
                "item:3".to_string(),
            ]
        );
    }

    #[test]
    fn timeline_row_ids_with_height_changes_tracks_streamed_item_and_diff_updates() {
        let mut previous = AiState::default();
        previous.turns.insert(
            hunk_codex::state::turn_storage_key("thread-1", "turn-1"),
            hunk_codex::state::TurnSummary {
                id: "turn-1".to_string(),
                thread_id: "thread-1".to_string(),
                status: hunk_codex::state::TurnStatus::InProgress,
                last_sequence: 1,
            },
        );
        previous.items.insert(
            hunk_codex::state::item_storage_key("thread-1", "turn-1", "item-1"),
            hunk_codex::state::ItemSummary {
                id: "item-1".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                kind: "agentMessage".to_string(),
                status: ItemStatus::Streaming,
                content: "hello".to_string(),
                display_metadata: None,
                last_sequence: 2,
            },
        );
        previous.turn_diffs.insert(
            hunk_codex::state::turn_storage_key("thread-1", "turn-1"),
            "@@ -1 +1 @@\n-old\n+new".to_string(),
        );

        let mut next = previous.clone();
        next.items.insert(
            hunk_codex::state::item_storage_key("thread-1", "turn-1", "item-1"),
            hunk_codex::state::ItemSummary {
                id: "item-1".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                kind: "agentMessage".to_string(),
                status: ItemStatus::Completed,
                content: "hello world".to_string(),
                display_metadata: None,
                last_sequence: 3,
            },
        );
        next.turn_diffs.insert(
            hunk_codex::state::turn_storage_key("thread-1", "turn-1"),
            "@@ -1 +1 @@\n-old line\n+new line".to_string(),
        );

        let changed_row_ids =
            timeline_row_ids_with_height_changes(&previous, &next, "thread-1");
        assert_eq!(
            changed_row_ids,
            BTreeSet::from([
                format!(
                    "item:{}",
                    hunk_codex::state::item_storage_key("thread-1", "turn-1", "item-1")
                ),
                format!(
                    "turn-diff:{}",
                    hunk_codex::state::turn_storage_key("thread-1", "turn-1")
                ),
            ]),
        );
    }

    #[test]
    fn timeline_measurements_reset_when_thread_or_visible_rows_change() {
        let row_ids = vec!["row-1".to_string(), "row-2".to_string()];
        assert!(should_reset_ai_timeline_measurements(
            Some("thread-1"),
            Some("thread-2"),
            row_ids.as_slice(),
            row_ids.as_slice(),
            row_ids.len(),
        ));
        assert!(should_reset_ai_timeline_measurements(
            Some("thread-1"),
            Some("thread-1"),
            row_ids.as_slice(),
            ["row-3".to_string(), "row-4".to_string()].as_slice(),
            row_ids.len(),
        ));
        assert!(should_reset_ai_timeline_measurements(
            Some("thread-1"),
            Some("thread-1"),
            row_ids.as_slice(),
            row_ids.as_slice(),
            0,
        ));
        assert!(!should_reset_ai_timeline_measurements(
            Some("thread-1"),
            Some("thread-1"),
            row_ids.as_slice(),
            row_ids.as_slice(),
            row_ids.len(),
        ));
    }

    #[test]
    #[ignore = "Runs an AI timeline row-index benchmark and optionally enforces thresholds."]
    fn ai_timeline_visible_row_index_perf_harness() {
        let turn_count = env::var("HUNK_AI_TIMELINE_PERF_TURNS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(2_000)
            .max(1);
        let items_per_turn = env::var("HUNK_AI_TIMELINE_PERF_ITEMS_PER_TURN")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(6)
            .max(1);
        let visible_turn_limit = env::var("HUNK_AI_TIMELINE_PERF_VISIBLE_TURNS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(64)
            .max(1);
        let iterations = env::var("HUNK_AI_TIMELINE_PERF_ITERATIONS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(250)
            .max(1);
        let enforce_thresholds = env::var("HUNK_AI_TIMELINE_PERF_ENFORCE")
            .ok()
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let max_avg_us = env::var("HUNK_AI_TIMELINE_PERF_MAX_AVG_US")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(15_000.0);

        let mut turn_ids = Vec::with_capacity(turn_count);
        let mut row_ids = Vec::with_capacity(turn_count.saturating_mul(items_per_turn + 1));
        let mut rows_by_id = BTreeMap::new();

        for turn_ix in 0..turn_count {
            let turn_id = format!("turn-{turn_ix}");
            turn_ids.push(turn_id.clone());
            for item_ix in 0..items_per_turn {
                let row_id = format!("item:{turn_ix}:{item_ix}");
                row_ids.push(row_id.clone());
                rows_by_id.insert(
                    row_id.clone(),
                    AiTimelineRow {
                        id: row_id,
                        thread_id: "thread-perf".to_string(),
                        turn_id: turn_id.clone(),
                        last_sequence: ((turn_ix * items_per_turn) + item_ix) as u64,
                        source: AiTimelineRowSource::Item {
                            item_key: format!("item-key:{turn_ix}:{item_ix}"),
                        },
                    },
                );
            }
            let diff_row_id = format!("turn-diff:{turn_ix}");
            row_ids.push(diff_row_id.clone());
            rows_by_id.insert(
                diff_row_id.clone(),
                AiTimelineRow {
                    id: diff_row_id,
                    thread_id: "thread-perf".to_string(),
                    turn_id: turn_id.clone(),
                    last_sequence: ((turn_ix * items_per_turn) + items_per_turn) as u64,
                    source: AiTimelineRowSource::TurnDiff {
                        turn_key: turn_id,
                    },
                },
            );
        }

        let started = Instant::now();
        let mut visible_row_total = 0usize;
        for _ in 0..iterations {
            let (_, _, _, visible_turn_ids) =
                timeline_visible_turn_ids(turn_ids.as_slice(), visible_turn_limit);
            let visible_rows =
                timeline_visible_row_ids_for_turns(row_ids.as_slice(), &rows_by_id, visible_turn_ids.as_slice());
            visible_row_total = visible_row_total.saturating_add(visible_rows.len());
        }
        let elapsed = started.elapsed();
        let average_us = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;

        println!("PERF_METRIC ai_timeline_turn_count={turn_count}");
        println!("PERF_METRIC ai_timeline_items_per_turn={items_per_turn}");
        println!("PERF_METRIC ai_timeline_iterations={iterations}");
        println!("PERF_METRIC ai_timeline_avg_us={average_us:.2}");
        println!("PERF_METRIC ai_timeline_visible_rows_total={visible_row_total}");

        assert!(visible_row_total > 0);
        if enforce_thresholds {
            assert!(
                average_us <= max_avg_us,
                "AI timeline row-index average {:.2}us exceeded threshold {:.2}us",
                average_us,
                max_avg_us
            );
        }
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
                display_metadata: None,
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
            git_workflow_cache: None,
        };
        assert!(workspace_mad_max_mode(&state, Some("/repo-a")));
        assert!(!workspace_mad_max_mode(&state, Some("/repo-b")));
        assert!(!workspace_mad_max_mode(&state, Some("/repo-c")));
    }

    #[test]
    fn workspace_include_hidden_models_defaults_to_true_when_missing() {
        let state = AppState::default();
        assert!(workspace_include_hidden_models(&state, Some("/repo")));
        assert!(workspace_include_hidden_models(&state, None));
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
            git_workflow_cache: None,
        };
        assert!(workspace_include_hidden_models(&state, Some("/repo-a")));
        assert!(!workspace_include_hidden_models(&state, Some("/repo-b")));
        assert!(workspace_include_hidden_models(&state, Some("/repo-c")));
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
    fn attachment_status_message_reports_only_problem_cases() {
        assert_eq!(ai_attachment_status_message(1, 1), None);
        assert_eq!(ai_attachment_status_message(3, 3), None);
        assert_eq!(
            ai_attachment_status_message(3, 1),
            Some("Attached 1 image. Skipped 2 unsupported or duplicate files.".to_string())
        );
        assert_eq!(
            ai_attachment_status_message(1, 0),
            Some("File is not a supported image or is already attached.".to_string())
        );
        assert_eq!(
            ai_attachment_status_message(2, 0),
            Some("No files were supported images or were already attached.".to_string())
        );
    }
}
