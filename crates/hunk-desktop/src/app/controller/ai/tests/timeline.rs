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
    fn timeline_row_ids_with_height_changes_tracks_turn_plan_updates() {
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
        previous.turn_plans.insert(
            hunk_codex::state::turn_storage_key("thread-1", "turn-1"),
            hunk_codex::state::TurnPlanSummary {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                explanation: Some("Inspect the reducer".to_string()),
                steps: vec![hunk_codex::state::TurnPlanStepSummary {
                    step: "Inspect notifications".to_string(),
                    status: hunk_codex::state::TurnPlanStepStatus::InProgress,
                }],
                created_sequence: 2,
                last_sequence: 2,
            },
        );

        let mut next = previous.clone();
        next.turn_plans.insert(
            hunk_codex::state::turn_storage_key("thread-1", "turn-1"),
            hunk_codex::state::TurnPlanSummary {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                explanation: Some("Render the checklist".to_string()),
                steps: vec![hunk_codex::state::TurnPlanStepSummary {
                    step: "Inspect notifications".to_string(),
                    status: hunk_codex::state::TurnPlanStepStatus::Completed,
                }],
                created_sequence: 2,
                last_sequence: 3,
            },
        );

        let changed_row_ids =
            timeline_row_ids_with_height_changes(&previous, &next, "thread-1");
        assert_eq!(
            changed_row_ids,
            BTreeSet::from([format!(
                "turn-plan:{}",
                hunk_codex::state::turn_storage_key("thread-1", "turn-1")
            ),]),
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
    fn timeline_grouping_merges_contiguous_exploration_rows() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let first_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-1");
        let second_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-2");
        let first_row_id = format!("item:{first_item_key}");
        let second_row_id = format!("item:{second_item_key}");

        let mut state = AiState::default();
        state.items.insert(
            first_item_key.clone(),
            timeline_tool_item(
                "item-1",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "commandExecution",
                    "command": "sed -n '1,40p' core.rs",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Read core.rs"]
                }"#,
                1,
            ),
        );
        state.items.insert(
            second_item_key.clone(),
            timeline_tool_item(
                "item-2",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "commandExecution",
                    "command": "rg -n update_window",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Search update_window in app"]
                }"#,
                2,
            ),
        );

        let row_ids = vec![first_row_id.clone(), second_row_id.clone()];
        let rows_by_id = BTreeMap::from([
            (
                first_row_id.clone(),
                timeline_item_row(first_row_id.as_str(), thread_id, turn_id, 1, first_item_key.as_str()),
            ),
            (
                second_row_id.clone(),
                timeline_item_row(
                    second_row_id.as_str(),
                    thread_id,
                    turn_id,
                    2,
                    second_item_key.as_str(),
                ),
            ),
        ]);

        let (grouped_row_ids, groups, parent_by_child) =
            group_ai_timeline_rows_for_thread(&state, row_ids.as_slice(), &rows_by_id);

        let expected_group_id = format!("group:{first_row_id}");
        assert_eq!(grouped_row_ids, vec![expected_group_id.clone()]);
        assert_eq!(parent_by_child.get(first_row_id.as_str()), Some(&expected_group_id));
        assert_eq!(parent_by_child.get(second_row_id.as_str()), Some(&expected_group_id));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Explored 1 file, 1 search");
        assert_eq!(groups[0].summary.as_deref(), Some("1 read • 1 search"));
        assert_eq!(groups[0].child_row_ids, row_ids);
    }

    #[test]
    fn timeline_grouping_recognizes_shell_read_and_search_commands() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let first_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-1");
        let second_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-2");
        let first_row_id = format!("item:{first_item_key}");
        let second_row_id = format!("item:{second_item_key}");

        let mut state = AiState::default();
        state.items.insert(
            first_item_key.clone(),
            timeline_tool_item(
                "item-1",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "10: line",
                r#"{
                    "kind": "commandExecution",
                    "command": "nl -ba crates/hunk-desktop/src/app.rs | sed -n '256,286p'",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Run nl -ba crates/hunk-desktop/src/app.rs | sed -n '256,286p'"]
                }"#,
                1,
            ),
        );
        state.items.insert(
            second_item_key.clone(),
            timeline_tool_item(
                "item-2",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "crates/hunk-desktop/src/app.rs:256",
                r#"{
                    "kind": "commandExecution",
                    "command": "rg -n \"AiTimelineRowSource\" crates/hunk-desktop/src/app.rs",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Run rg -n \"AiTimelineRowSource\" crates/hunk-desktop/src/app.rs"]
                }"#,
                2,
            ),
        );

        let row_ids = vec![first_row_id.clone(), second_row_id.clone()];
        let rows_by_id = BTreeMap::from([
            (
                first_row_id.clone(),
                timeline_item_row(
                    first_row_id.as_str(),
                    thread_id,
                    turn_id,
                    1,
                    first_item_key.as_str(),
                ),
            ),
            (
                second_row_id.clone(),
                timeline_item_row(
                    second_row_id.as_str(),
                    thread_id,
                    turn_id,
                    2,
                    second_item_key.as_str(),
                ),
            ),
        ]);

        let (grouped_row_ids, groups, _) =
            group_ai_timeline_rows_for_thread(&state, row_ids.as_slice(), &rows_by_id);

        assert_eq!(grouped_row_ids, vec![format!("group:{first_row_id}")]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Explored 1 file, 1 search");
    }

    #[test]
    fn timeline_grouping_merges_generic_command_batches() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let first_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-1");
        let second_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-2");
        let first_row_id = format!("item:{first_item_key}");
        let second_row_id = format!("item:{second_item_key}");

        let mut state = AiState::default();
        state.items.insert(
            first_item_key.clone(),
            timeline_tool_item(
                "item-1",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "check output",
                r#"{
                    "kind": "commandExecution",
                    "command": "cargo check --workspace",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Run cargo check --workspace"]
                }"#,
                1,
            ),
        );
        state.items.insert(
            second_item_key.clone(),
            timeline_tool_item(
                "item-2",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "clippy output",
                r#"{
                    "kind": "commandExecution",
                    "command": "cargo clippy --workspace --all-targets -- -D warnings",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Run cargo clippy --workspace --all-targets -- -D warnings"]
                }"#,
                2,
            ),
        );

        let row_ids = vec![first_row_id.clone(), second_row_id.clone()];
        let rows_by_id = BTreeMap::from([
            (
                first_row_id.clone(),
                timeline_item_row(
                    first_row_id.as_str(),
                    thread_id,
                    turn_id,
                    1,
                    first_item_key.as_str(),
                ),
            ),
            (
                second_row_id.clone(),
                timeline_item_row(
                    second_row_id.as_str(),
                    thread_id,
                    turn_id,
                    2,
                    second_item_key.as_str(),
                ),
            ),
        ]);

        let (grouped_row_ids, groups, _) =
            group_ai_timeline_rows_for_thread(&state, row_ids.as_slice(), &rows_by_id);

        assert_eq!(grouped_row_ids, vec![format!("group:{first_row_id}")]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, "command_batch");
        assert_eq!(groups[0].title, "Ran 2 commands");
        assert_eq!(
            groups[0].summary.as_deref(),
            Some(
                "cargo check --workspace • cargo clippy --workspace --all-targets -- -D warnings"
            )
        );
    }

    #[test]
    fn timeline_grouping_merges_file_change_batches() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let first_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-1");
        let second_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-2");
        let first_row_id = format!("item:{first_item_key}");
        let second_row_id = format!("item:{second_item_key}");

        let mut state = AiState::default();
        state.items.insert(
            first_item_key.clone(),
            timeline_tool_item(
                "item-1",
                thread_id,
                turn_id,
                "fileChange",
                ItemStatus::Completed,
                "",
                r#"{
                    "type": "fileChange",
                    "id": "item-1",
                    "changes": [
                        {
                            "path": "/repo/src/first.rs",
                            "kind": { "type": "update", "movePath": null },
                            "diff": "@@ -1 +1 @@"
                        }
                    ],
                    "status": "completed"
                }"#,
                1,
            ),
        );
        state.items.insert(
            second_item_key.clone(),
            timeline_tool_item(
                "item-2",
                thread_id,
                turn_id,
                "fileChange",
                ItemStatus::Completed,
                "",
                r#"{
                    "type": "fileChange",
                    "id": "item-2",
                    "changes": [
                        {
                            "path": "/repo/src/second.rs",
                            "kind": { "type": "update", "movePath": null },
                            "diff": "@@ -1 +1 @@"
                        }
                    ],
                    "status": "completed"
                }"#,
                2,
            ),
        );

        let row_ids = vec![first_row_id.clone(), second_row_id.clone()];
        let rows_by_id = BTreeMap::from([
            (
                first_row_id.clone(),
                timeline_item_row(
                    first_row_id.as_str(),
                    thread_id,
                    turn_id,
                    1,
                    first_item_key.as_str(),
                ),
            ),
            (
                second_row_id.clone(),
                timeline_item_row(
                    second_row_id.as_str(),
                    thread_id,
                    turn_id,
                    2,
                    second_item_key.as_str(),
                ),
            ),
        ]);

        let (grouped_row_ids, groups, _) =
            group_ai_timeline_rows_for_thread(&state, row_ids.as_slice(), &rows_by_id);

        assert_eq!(grouped_row_ids, vec![format!("group:{first_row_id}")]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, "file_change_batch");
        assert_eq!(groups[0].title, "Applied 2 file changes");
        assert_eq!(
            groups[0].summary.as_deref(),
            Some("/repo/src/first.rs (+1 more files)")
        );
    }

    #[test]
    fn timeline_grouping_merges_compact_file_change_batches() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let first_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-1");
        let second_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-2");
        let first_row_id = format!("item:{first_item_key}");
        let second_row_id = format!("item:{second_item_key}");

        let mut state = AiState::default();
        state.items.insert(
            first_item_key.clone(),
            timeline_tool_item(
                "item-1",
                thread_id,
                turn_id,
                "fileChange",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "fileChangeSummary",
                    "changes": [
                        { "path": "/repo/src/first.rs", "added": 2, "removed": 1 },
                        { "path": "/repo/src/second.rs", "added": 1, "removed": 0 }
                    ],
                    "truncatedCount": 0
                }"#,
                1,
            ),
        );
        state.items.insert(
            second_item_key.clone(),
            timeline_tool_item(
                "item-2",
                thread_id,
                turn_id,
                "fileChange",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "fileChangeSummary",
                    "changes": [
                        { "path": "/repo/src/third.rs", "added": 1, "removed": 1 }
                    ],
                    "truncatedCount": 2
                }"#,
                2,
            ),
        );

        let row_ids = vec![first_row_id.clone(), second_row_id.clone()];
        let rows_by_id = BTreeMap::from([
            (
                first_row_id.clone(),
                timeline_item_row(
                    first_row_id.as_str(),
                    thread_id,
                    turn_id,
                    1,
                    first_item_key.as_str(),
                ),
            ),
            (
                second_row_id.clone(),
                timeline_item_row(
                    second_row_id.as_str(),
                    thread_id,
                    turn_id,
                    2,
                    second_item_key.as_str(),
                ),
            ),
        ]);

        let (grouped_row_ids, groups, _) =
            group_ai_timeline_rows_for_thread(&state, row_ids.as_slice(), &rows_by_id);

        assert_eq!(grouped_row_ids, vec![format!("group:{first_row_id}")]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, "file_change_batch");
        assert_eq!(groups[0].title, "Applied 5 file changes");
        assert_eq!(
            groups[0].summary.as_deref(),
            Some("/repo/src/first.rs (+4 more files)")
        );
    }

    #[test]
    fn turn_file_change_detection_tracks_turn_keys() {
        let mut state = AiState::default();
        state.items.insert(
            hunk_codex::state::item_storage_key("thread-1", "turn-1", "item-1"),
            hunk_codex::state::ItemSummary {
                id: "item-1".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                kind: "fileChange".to_string(),
                status: ItemStatus::Completed,
                content: String::new(),
                display_metadata: None,
                last_sequence: 1,
            },
        );
        state.items.insert(
            hunk_codex::state::item_storage_key("thread-1", "turn-2", "item-2"),
            hunk_codex::state::ItemSummary {
                id: "item-2".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-2".to_string(),
                kind: "commandExecution".to_string(),
                status: ItemStatus::Completed,
                content: String::new(),
                display_metadata: None,
                last_sequence: 2,
            },
        );

        let turn_keys = ai_turn_keys_with_file_change_items(&state);
        assert!(turn_keys.contains(hunk_codex::state::turn_storage_key("thread-1", "turn-1").as_str()));
        assert!(!turn_keys.contains(hunk_codex::state::turn_storage_key("thread-1", "turn-2").as_str()));
        assert!(!turn_keys.contains(hunk_codex::state::turn_storage_key("thread-2", "turn-1").as_str()));
    }

    #[test]
    fn timeline_grouping_respects_non_tool_boundaries() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let first_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-1");
        let second_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-2");
        let third_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-3");
        let first_row_id = format!("item:{first_item_key}");
        let second_row_id = format!("item:{second_item_key}");
        let third_row_id = format!("item:{third_item_key}");

        let mut state = AiState::default();
        state.items.insert(
            first_item_key.clone(),
            timeline_tool_item(
                "item-1",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "commandExecution",
                    "command": "sed -n '1,40p' core.rs",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Read core.rs"]
                }"#,
                1,
            ),
        );
        state.items.insert(
            second_item_key.clone(),
            hunk_codex::state::ItemSummary {
                id: "item-2".to_string(),
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                kind: "agentMessage".to_string(),
                status: ItemStatus::Completed,
                content: "Planning".to_string(),
                display_metadata: None,
                last_sequence: 2,
            },
        );
        state.items.insert(
            third_item_key.clone(),
            timeline_tool_item(
                "item-3",
                thread_id,
                turn_id,
                "commandExecution",
                ItemStatus::Completed,
                "",
                r#"{
                    "kind": "commandExecution",
                    "command": "rg -n update_window",
                    "cwd": "/repo",
                    "status": "completed",
                    "actionSummaries": ["Search update_window in app"]
                }"#,
                3,
            ),
        );

        let row_ids = vec![
            first_row_id.clone(),
            second_row_id.clone(),
            third_row_id.clone(),
        ];
        let rows_by_id = BTreeMap::from([
            (
                first_row_id.clone(),
                timeline_item_row(first_row_id.as_str(), thread_id, turn_id, 1, first_item_key.as_str()),
            ),
            (
                second_row_id.clone(),
                timeline_item_row(
                    second_row_id.as_str(),
                    thread_id,
                    turn_id,
                    2,
                    second_item_key.as_str(),
                ),
            ),
            (
                third_row_id.clone(),
                timeline_item_row(third_row_id.as_str(), thread_id, turn_id, 3, third_item_key.as_str()),
            ),
        ]);

        let (grouped_row_ids, groups, parent_by_child) =
            group_ai_timeline_rows_for_thread(&state, row_ids.as_slice(), &rows_by_id);

        assert_eq!(grouped_row_ids, row_ids);
        assert!(groups.is_empty());
        assert!(parent_by_child.is_empty());
    }

    #[test]
    fn timeline_grouping_merges_contiguous_collaboration_rows() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let first_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-1");
        let second_item_key = hunk_codex::state::item_storage_key(thread_id, turn_id, "item-2");
        let first_row_id = format!("item:{first_item_key}");
        let second_row_id = format!("item:{second_item_key}");

        let mut state = AiState::default();
        state.items.insert(
            first_item_key.clone(),
            timeline_tool_item(
                "item-1",
                thread_id,
                turn_id,
                "collabAgentToolCall",
                ItemStatus::Completed,
                "",
                r#"{
                    "type": "collabAgentToolCall",
                    "id": "item-1",
                    "tool": "spawnAgent",
                    "status": "completed",
                    "senderThreadId": "thread-1",
                    "receiverThreadIds": ["agent-a"],
                    "prompt": null,
                    "agentsStates": {}
                }"#,
                1,
            ),
        );
        state.items.insert(
            second_item_key.clone(),
            timeline_tool_item(
                "item-2",
                thread_id,
                turn_id,
                "collabAgentToolCall",
                ItemStatus::Completed,
                "",
                r#"{
                    "type": "collabAgentToolCall",
                    "id": "item-2",
                    "tool": "wait",
                    "status": "completed",
                    "senderThreadId": "thread-1",
                    "receiverThreadIds": ["agent-a", "agent-b"],
                    "prompt": null,
                    "agentsStates": {}
                }"#,
                2,
            ),
        );

        let row_ids = vec![first_row_id.clone(), second_row_id.clone()];
        let rows_by_id = BTreeMap::from([
            (
                first_row_id.clone(),
                timeline_item_row(first_row_id.as_str(), thread_id, turn_id, 1, first_item_key.as_str()),
            ),
            (
                second_row_id.clone(),
                timeline_item_row(
                    second_row_id.as_str(),
                    thread_id,
                    turn_id,
                    2,
                    second_item_key.as_str(),
                ),
            ),
        ]);

        let (grouped_row_ids, groups, parent_by_child) =
            group_ai_timeline_rows_for_thread(&state, row_ids.as_slice(), &rows_by_id);

        let expected_group_id = format!("group:{first_row_id}");
        assert_eq!(grouped_row_ids, vec![expected_group_id.clone()]);
        assert_eq!(parent_by_child.get(first_row_id.as_str()), Some(&expected_group_id));
        assert_eq!(parent_by_child.get(second_row_id.as_str()), Some(&expected_group_id));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Worked with 2 sub-agents");
        assert_eq!(groups[0].summary.as_deref(), Some("1 launch • 1 wait"));
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
