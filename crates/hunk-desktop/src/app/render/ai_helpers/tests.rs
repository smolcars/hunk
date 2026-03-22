#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod ai_helper_tests {
    use super::ai_account_summary;
    use super::ai_markdown_code_block_text;
    use super::ai_markdown_code_block_text_and_highlights;
    use super::ai_terminal_screen_grid;
    use super::ai_command_execution_display_details;
    use super::ai_command_execution_terminal_text;
    use super::ai_composer_status_tone;
    use super::ai_collaboration_picker_label;
    use super::ai_display_path_parts;
    use super::ai_file_change_summary;
    use super::ai_should_show_no_turns_empty_state;
    use super::ai_tool_compact_summary;
    use super::ai_thread_display_title;
    use super::ai_thread_status_text;
    use super::ai_tool_header_title;
    use super::ai_item_display_label;
    use super::ai_reasoning_effort_label;
    use super::ai_rate_limit_summary;
    use super::ai_terminal_selection_surfaces;
    use super::ai_terminal_selection_columns;
    use super::ai_terminal_link_ranges;
    use super::ai_turn_diff_summary;
    use super::ai_tool_header_label;
    use super::ai_timeline_item_is_renderable;
    use super::ai_truncate_multiline_content;
    use super::ai_terminal_supports_text_selection;
    use crate::app::terminal_cursor::ai_terminal_effective_cursor_shape;
    use crate::app::terminal_cursor::ai_terminal_cursor_shape_blinks;
    use crate::app::terminal_cursor::ai_terminal_cursor_visible_for_paint;
    use super::AiCommandExecutionDisplayDetails;
    use crate::app::markdown_links::markdown_inline_text_and_link_ranges;
    use hunk_terminal::TerminalCellSnapshot;
    use hunk_terminal::TerminalColorSnapshot;
    use hunk_terminal::TerminalCursorShapeSnapshot;
    use hunk_terminal::TerminalCursorSnapshot;
    use hunk_terminal::TerminalDamageSnapshot;
    use hunk_terminal::TerminalModeSnapshot;
    use hunk_terminal::TerminalNamedColorSnapshot;
    use hunk_terminal::TerminalScreenSnapshot;
    use hunk_codex::state::ItemDisplayMetadata;
    use hunk_codex::state::ItemStatus;
    use hunk_codex::state::ItemSummary;
    use hunk_codex::state::ThreadSummary;
    use hunk_codex::state::ThreadLifecycleStatus;
    use hunk_domain::markdown_preview::MarkdownCodeSpan;
    use hunk_domain::markdown_preview::MarkdownCodeTokenKind;
    use hunk_domain::state::AiCollaborationModeSelection;
    use hunk_domain::markdown_preview::MarkdownPreviewBlock;

    fn rate_limit_window(
        used_percent: i32,
        window_duration_mins: Option<i64>,
        resets_at: Option<i64>,
    ) -> codex_app_server_protocol::RateLimitWindow {
        codex_app_server_protocol::RateLimitWindow {
            used_percent,
            window_duration_mins,
            resets_at,
        }
    }

    fn rate_limit_snapshot(
        primary: Option<codex_app_server_protocol::RateLimitWindow>,
        secondary: Option<codex_app_server_protocol::RateLimitWindow>,
    ) -> codex_app_server_protocol::RateLimitSnapshot {
        codex_app_server_protocol::RateLimitSnapshot {
            limit_id: Some("codex".to_string()),
            limit_name: Some("Codex".to_string()),
            primary,
            secondary,
            credits: None,
            plan_type: None,
        }
    }

    #[test]
    fn rate_limit_summary_reports_five_hour_and_weekly_windows() {
        let snapshot = rate_limit_snapshot(
            Some(rate_limit_window(42, Some(300), Some(1_700_000_000))),
            Some(rate_limit_window(19, Some(10_080), Some(1_700_300_000))),
        );

        let (five_hour, weekly) = ai_rate_limit_summary(Some(&snapshot), false);
        assert!(five_hour.contains("5h: 42% used"));
        assert!(weekly.contains("weekly: 19% used"));
        assert!(!five_hour.contains("1700000000"));
        assert!(!weekly.contains("1700300000"));
        assert!(five_hour.contains("UTC"));
        assert!(weekly.contains("UTC"));
    }

    #[test]
    fn rate_limit_summary_falls_back_to_unavailable_when_missing() {
        let (five_hour, weekly) = ai_rate_limit_summary(None, false);
        assert_eq!(five_hour, "5h: unavailable");
        assert_eq!(weekly, "weekly: unavailable");
    }

    #[test]
    fn rate_limit_summary_reports_loading_during_bootstrap() {
        let (five_hour, weekly) = ai_rate_limit_summary(None, true);
        assert_eq!(five_hour, "5h: loading");
        assert_eq!(weekly, "weekly: loading");
    }

    #[test]
    fn account_summary_reports_loading_while_bootstrapping() {
        let summary = ai_account_summary(None, false, true);
        assert_eq!(summary, "Loading account...");
    }

    #[test]
    fn rate_limit_summary_uses_primary_and_secondary_when_durations_are_unknown() {
        let snapshot = rate_limit_snapshot(
            Some(rate_limit_window(11, Some(60), Some(1_700_000_000))),
            Some(rate_limit_window(27, Some(120), Some(1_700_100_000))),
        );

        let (five_hour, weekly) = ai_rate_limit_summary(Some(&snapshot), false);
        assert!(five_hour.contains("5h: 11% used"));
        assert!(weekly.contains("weekly: 27% used"));
    }

    #[test]
    fn truncate_multiline_content_only_marks_overflow_when_needed() {
        let (single, single_truncated) = ai_truncate_multiline_content("line 1\nline 2", 3);
        assert_eq!(single, "line 1\nline 2");
        assert!(!single_truncated);

        let (truncated, is_truncated) =
            ai_truncate_multiline_content("line 1\nline 2\nline 3\nline 4", 3);
        assert_eq!(truncated, "line 1\nline 2\nline 3\n...");
        assert!(is_truncated);
    }

    #[test]
    fn item_display_label_maps_user_and_agent_labels() {
        assert_eq!(ai_item_display_label("userMessage"), "User");
        assert_eq!(ai_item_display_label("agentMessage"), "Agent");
        assert_eq!(ai_item_display_label("unknownKind"), "unknownKind");
    }

    #[test]
    fn no_turns_empty_state_is_hidden_while_first_prompt_is_pending() {
        assert!(ai_should_show_no_turns_empty_state(0, false));
        assert!(!ai_should_show_no_turns_empty_state(0, true));
        assert!(!ai_should_show_no_turns_empty_state(1, false));
    }

    #[test]
    fn collaboration_picker_defaults_to_default_mode() {
        assert_eq!(
            ai_collaboration_picker_label(AiCollaborationModeSelection::Default),
            "Default"
        );
    }

    #[test]
    fn collaboration_picker_uses_plan_label() {
        assert_eq!(
            ai_collaboration_picker_label(AiCollaborationModeSelection::Plan),
            "Plan"
        );
    }

    #[test]
    fn terminal_screen_grid_places_cells_and_cursor_in_visible_rows() {
        let screen = TerminalScreenSnapshot {
            rows: 2,
            cols: 4,
            display_offset: 0,
            cursor: TerminalCursorSnapshot {
                line: 1,
                column: 2,
                shape: TerminalCursorShapeSnapshot::Block,
            },
            mode: TerminalModeSnapshot {
                show_cursor: true,
                ..TerminalModeSnapshot::default()
            },
            damage: TerminalDamageSnapshot::Full,
            cells: vec![
                TerminalCellSnapshot {
                    line: 0,
                    column: 0,
                    character: 'h',
                    fg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Foreground),
                    bg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Background),
                    flags: 0,
                    zerowidth: Vec::new(),
                },
                TerminalCellSnapshot {
                    line: 1,
                    column: 1,
                    character: 'i',
                    fg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Foreground),
                    bg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Background),
                    flags: 0,
                    zerowidth: Vec::new(),
                },
            ],
        };

        let grid = ai_terminal_screen_grid(&screen);
        assert_eq!(grid.len(), 2);
        assert_eq!(grid[0].len(), 4);
        assert_eq!(grid[0][0].character, 'h');
        assert_eq!(grid[1][1].character, 'i');
        assert!(grid[1][2].cursor);
    }

    #[test]
    fn terminal_screen_grid_preserves_zero_width_marks() {
        let screen = TerminalScreenSnapshot {
            rows: 1,
            cols: 2,
            display_offset: 0,
            cursor: TerminalCursorSnapshot {
                line: 0,
                column: 0,
                shape: TerminalCursorShapeSnapshot::Block,
            },
            mode: TerminalModeSnapshot::default(),
            damage: TerminalDamageSnapshot::Full,
            cells: vec![TerminalCellSnapshot {
                line: 0,
                column: 0,
                character: 'e',
                fg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Foreground),
                bg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Background),
                flags: 0,
                zerowidth: vec!['\u{301}'],
            }],
        };

        let grid = ai_terminal_screen_grid(&screen);
        assert_eq!(grid[0][0].character, 'e');
        assert_eq!(grid[0][0].zerowidth, "\u{301}");
    }

    #[test]
    fn terminal_selection_surfaces_insert_newline_separators_between_rows() {
        let surfaces = ai_terminal_selection_surfaces(
            &[
                super::AiTerminalPaintLine {
                    surface_id: "surface-a".into(),
                    text: "hello".into(),
                    column_byte_offsets: vec![0, 1, 2, 3, 4, 5].into(),
                    link_ranges: Vec::new().into(),
                    background_rects: Vec::<super::AiTerminalBackgroundRect>::new().into(),
                    cursor_overlays: Vec::<super::AiTerminalCursorOverlay>::new().into(),
                    text_runs: Vec::<gpui::TextRun>::new().into(),
                    selection_range: None,
                },
                super::AiTerminalPaintLine {
                    surface_id: "surface-b".into(),
                    text: "world".into(),
                    column_byte_offsets: vec![0, 1, 2, 3, 4, 5].into(),
                    link_ranges: Vec::new().into(),
                    background_rects: Vec::<super::AiTerminalBackgroundRect>::new().into(),
                    cursor_overlays: Vec::<super::AiTerminalCursorOverlay>::new().into(),
                    text_runs: Vec::<gpui::TextRun>::new().into(),
                    selection_range: None,
                },
            ],
        );

        assert_eq!(surfaces.len(), 2);
        assert_eq!(surfaces[0].separator_before, "");
        assert_eq!(surfaces[1].separator_before, "\n");
        assert_eq!(surfaces[0].text, "hello");
        assert_eq!(surfaces[1].text, "world");
    }

    #[test]
    fn terminal_text_selection_is_disabled_only_for_alt_screen() {
        let mut screen = TerminalScreenSnapshot {
            rows: 1,
            cols: 1,
            display_offset: 0,
            cursor: TerminalCursorSnapshot {
                line: 0,
                column: 0,
                shape: TerminalCursorShapeSnapshot::Block,
            },
            mode: TerminalModeSnapshot::default(),
            damage: TerminalDamageSnapshot::Full,
            cells: Vec::new(),
        };

        assert!(ai_terminal_supports_text_selection(&screen));

        screen.mode.alt_screen = true;
        assert!(!ai_terminal_supports_text_selection(&screen));

        screen.mode.alt_screen = false;
        screen.mode.mouse_mode = true;
        assert!(ai_terminal_supports_text_selection(&screen));
    }

    #[test]
    fn terminal_selection_columns_follow_byte_boundaries() {
        assert_eq!(
            ai_terminal_selection_columns(&[0, 1, 5, 6], &(1..5)),
            Some((1, 2))
        );
        assert_eq!(
            ai_terminal_selection_columns(&[0, 1, 5, 6], &(0..6)),
            Some((0, 3))
        );
        assert_eq!(ai_terminal_selection_columns(&[0, 1, 5, 6], &(5..5)), None);
    }

    #[test]
    fn terminal_link_ranges_detect_urls_and_file_style_paths() {
        let ranges = ai_terminal_link_ranges(
            "open https://example.com and src/main.rs:12 plus /tmp/log.txt.",
        );

        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].raw_target, "https://example.com");
        assert_eq!(ranges[1].raw_target, "src/main.rs:12");
        assert_eq!(ranges[2].raw_target, "/tmp/log.txt");
    }

    #[test]
    fn terminal_cursor_blink_visibility_depends_on_focus_and_shape() {
        assert!(ai_terminal_cursor_shape_blinks(
            TerminalCursorShapeSnapshot::Block
        ));
        assert!(!ai_terminal_cursor_shape_blinks(
            TerminalCursorShapeSnapshot::Hidden
        ));

        assert!(ai_terminal_cursor_visible_for_paint(
            TerminalCursorShapeSnapshot::Block,
            false,
            false,
            false,
        ));
        assert!(ai_terminal_cursor_visible_for_paint(
            TerminalCursorShapeSnapshot::Underline,
            true,
            true,
            false,
        ));
        assert!(!ai_terminal_cursor_visible_for_paint(
            TerminalCursorShapeSnapshot::Beam,
            true,
            false,
            false,
        ));
        assert!(!ai_terminal_cursor_visible_for_paint(
            TerminalCursorShapeSnapshot::Beam,
            true,
            true,
            true,
        ));
        assert_eq!(
            ai_terminal_effective_cursor_shape(
                TerminalCursorShapeSnapshot::Block,
                true,
                false,
            ),
            TerminalCursorShapeSnapshot::Beam,
        );
    }

    #[test]
    fn thread_status_text_maps_lifecycle_states() {
        assert_eq!(ai_thread_status_text(ThreadLifecycleStatus::Active), "active");
        assert_eq!(ai_thread_status_text(ThreadLifecycleStatus::Idle), "idle");
        assert_eq!(
            ai_thread_status_text(ThreadLifecycleStatus::NotLoaded),
            "not loaded"
        );
    }

    #[test]
    fn thread_display_title_avoids_exposing_thread_id_when_title_missing() {
        let thread = ThreadSummary {
            id: "019ccb4e-165a-75e1-a9ac-ddbc307ec84a".to_string(),
            cwd: "/repo".to_string(),
            title: None,
            status: ThreadLifecycleStatus::Idle,
            created_at: 0,
            updated_at: 0,
            last_sequence: 0,
        };

        assert_eq!(ai_thread_display_title(&thread), "Untitled thread");
    }

    #[test]
    fn thread_display_title_collapses_multiline_whitespace() {
        let thread = ThreadSummary {
            id: "thread-1".to_string(),
            cwd: "/repo".to_string(),
            title: Some("  crates/hunk-desktop\n\nok please\tthink carefully  ".to_string()),
            status: ThreadLifecycleStatus::Idle,
            created_at: 0,
            updated_at: 0,
            last_sequence: 0,
        };

        assert_eq!(
            ai_thread_display_title(&thread),
            "crates/hunk-desktop ok please think carefully"
        );
    }

    #[test]
    fn thread_display_title_uses_fallback_when_title_is_only_whitespace() {
        let thread = ThreadSummary {
            id: "thread-1".to_string(),
            cwd: "/repo".to_string(),
            title: Some(" \n\t ".to_string()),
            status: ThreadLifecycleStatus::Idle,
            created_at: 0,
            updated_at: 0,
            last_sequence: 0,
        };

        assert_eq!(ai_thread_display_title(&thread), "Untitled thread");
    }

    #[test]
    fn timeline_item_renderability_hides_empty_reasoning_without_metadata() {
        let reasoning = ItemSummary {
            id: "item-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            kind: "reasoning".to_string(),
            status: ItemStatus::Completed,
            content: "   ".to_string(),
            display_metadata: None,
            last_sequence: 1,
        };
        assert!(!ai_timeline_item_is_renderable(&reasoning));

        let reasoning_with_metadata = ItemSummary {
            display_metadata: Some(ItemDisplayMetadata {
                summary: Some("Thinking".to_string()),
                details_json: None,
            }),
            ..reasoning
        };
        assert!(ai_timeline_item_is_renderable(&reasoning_with_metadata));
    }

    #[test]
    fn command_display_details_parse_compact_metadata_shape() {
        let item = ItemSummary {
            id: "item-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            kind: "commandExecution".to_string(),
            status: ItemStatus::Completed,
            content: "Finished test suite".to_string(),
            display_metadata: Some(ItemDisplayMetadata {
                summary: Some("Ran command".to_string()),
                details_json: Some(
                    r#"{
                        "kind": "commandExecution",
                        "command": "cargo test -p hunk-desktop",
                        "cwd": "/repo",
                        "processId": "123",
                        "status": "completed",
                        "actionSummaries": ["Run cargo test"],
                        "exitCode": 0,
                        "durationMs": 1250
                    }"#
                        .to_string(),
                ),
            }),
            last_sequence: 1,
        };

        let details =
            ai_command_execution_display_details(&item).expect("command details should parse");
        assert_eq!(details.command, "cargo test -p hunk-desktop");
        assert_eq!(details.cwd, "/repo");
        assert_eq!(details.process_id.as_deref(), Some("123"));
        assert_eq!(details.status, "completed");
        assert_eq!(details.action_summaries, vec!["Run cargo test".to_string()]);
        assert_eq!(details.exit_code, Some(0));
        assert_eq!(details.duration_ms, Some(1250));
    }

    #[test]
    fn command_execution_terminal_text_formats_metadata_and_command_output() {
        let details = AiCommandExecutionDisplayDetails {
            command: "cargo test -p hunk-desktop".to_string(),
            cwd: "/repo".to_string(),
            process_id: Some("123".to_string()),
            status: "completed".to_string(),
            action_summaries: vec!["Run cargo test".to_string()],
            exit_code: Some(0),
            duration_ms: Some(1250),
        };

        let (text, truncated) =
            ai_command_execution_terminal_text(&details, "line 1\nline 2\n", Some(10));

        assert!(!truncated);
        assert!(text.contains("# cwd: /repo"));
        assert!(text.contains("pid: 123"));
        assert!(text.contains("exit: 0"));
        assert!(text.contains("duration:"));
        assert!(text.contains("# Run cargo test"));
        assert!(text.contains("$ cargo test -p hunk-desktop"));
        assert!(text.contains("line 1\nline 2"));
    }

    #[test]
    fn command_execution_terminal_text_truncates_output_preview() {
        let details = AiCommandExecutionDisplayDetails {
            command: "cargo clippy".to_string(),
            cwd: "/repo".to_string(),
            process_id: None,
            status: "completed".to_string(),
            action_summaries: Vec::new(),
            exit_code: Some(0),
            duration_ms: None,
        };

        let output = "one\ntwo\nthree\nfour\n";
        let (text, truncated) = ai_command_execution_terminal_text(&details, output, Some(2));

        assert!(truncated);
        assert!(text.contains("one\ntwo"));
        assert!(!text.contains("three\nfour"));
        assert!(text.contains("... output truncated to the first 2 lines ..."));
    }

    #[test]
    fn turn_diff_summary_groups_line_counts_by_file() {
        let diff = "\
diff --git a/crates/hunk-desktop/src/app/render/ai_composer.rs b/crates/hunk-desktop/src/app/render/ai_composer.rs
--- a/crates/hunk-desktop/src/app/render/ai_composer.rs
+++ b/crates/hunk-desktop/src/app/render/ai_composer.rs
@@ -1,2 +1,3 @@
-old
+new
+newer
 keep
diff --git a/crates/hunk-desktop/src/app/render/ai.rs b/crates/hunk-desktop/src/app/render/ai.rs
--- a/crates/hunk-desktop/src/app/render/ai.rs
+++ b/crates/hunk-desktop/src/app/render/ai.rs
@@ -10,1 +10,0 @@
-gone";

        let summary = ai_turn_diff_summary(diff);

        assert_eq!(summary.total_added, 2);
        assert_eq!(summary.total_removed, 2);
        assert_eq!(summary.files.len(), 2);
        assert_eq!(
            summary.files[0],
            super::AiTurnDiffFileSummary {
                path: "crates/hunk-desktop/src/app/render/ai_composer.rs".to_string(),
                added: 2,
                removed: 1,
            }
        );
        assert_eq!(
            summary.files[1],
            super::AiTurnDiffFileSummary {
                path: "crates/hunk-desktop/src/app/render/ai.rs".to_string(),
                added: 0,
                removed: 1,
            }
        );
    }

    #[test]
    fn turn_diff_summary_uses_fallback_file_for_headerless_patch() {
        let summary = ai_turn_diff_summary(
            "@@ -1 +1 @@\n-old line\n+new line\n+second line",
        );

        assert_eq!(summary.total_added, 2);
        assert_eq!(summary.total_removed, 1);
        assert_eq!(summary.files.len(), 1);
        assert_eq!(summary.files[0].path, "changes");
        assert_eq!(summary.files[0].added, 2);
        assert_eq!(summary.files[0].removed, 1);
    }

    #[test]
    fn file_change_summary_uses_compact_persisted_metadata() {
        let item = ItemSummary {
            id: "item-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            kind: "fileChange".to_string(),
            status: ItemStatus::Completed,
            content: String::new(),
            display_metadata: Some(ItemDisplayMetadata {
                summary: Some("Applied file changes".to_string()),
                details_json: Some(
                    r#"{
                        "kind": "fileChangeSummary",
                        "changes": [
                            {
                                "path": "docs/alpha.md",
                                "added": 2,
                                "removed": 1
                            },
                            {
                                "path": "docs/beta.md",
                                "added": 1,
                                "removed": 1
                            }
                        ],
                        "truncatedCount": 0
                    }"#
                        .to_string(),
                ),
            }),
            last_sequence: 1,
        };

        let summary = ai_file_change_summary(&item).expect("file change summary should parse");
        assert_eq!(summary.total_added, 3);
        assert_eq!(summary.total_removed, 2);
        assert_eq!(summary.files.len(), 2);
        assert_eq!(summary.files[0].path, "docs/alpha.md");
        assert_eq!(summary.files[0].added, 2);
        assert_eq!(summary.files[0].removed, 1);
        assert_eq!(summary.files[1].path, "docs/beta.md");
        assert_eq!(summary.files[1].added, 1);
        assert_eq!(summary.files[1].removed, 1);
    }

    #[test]
    fn display_path_parts_split_windows_paths() {
        let (file_name, directory) =
            ai_display_path_parts(r"C:\Users\nites\Documents\hunk\src\main.rs");

        assert_eq!(file_name, "main.rs");
        assert_eq!(
            directory.as_deref(),
            Some(r"C:\Users\nites\Documents\hunk\src")
        );
    }

    #[test]
    fn tool_header_label_falls_back_to_preview_when_summary_is_placeholder() {
        let item = ItemSummary {
            id: "item-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            kind: "commandExecution".to_string(),
            status: ItemStatus::Completed,
            content: "Finished test suite".to_string(),
            display_metadata: Some(ItemDisplayMetadata {
                summary: Some("...".to_string()),
                details_json: Some(
                    r#"{
                        "kind": "commandExecution",
                        "command": "sed -n '1,40p' crates/hunk-desktop/src/app/render/ai.rs",
                        "cwd": "/repo",
                        "status": "completed"
                    }"#
                        .to_string(),
                ),
            }),
            last_sequence: 1,
        };

        let label = ai_tool_header_label(&item, item.content.trim());
        assert_eq!(label, "sed -n '1,40p' crates/hunk-desktop/src/app/render/ai.rs");
    }

    #[test]
    fn tool_header_title_prefers_non_placeholder_summary() {
        let item = ItemSummary {
            id: "item-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            kind: "commandExecution".to_string(),
            status: ItemStatus::Completed,
            content: "Finished test suite".to_string(),
            display_metadata: Some(ItemDisplayMetadata {
                summary: Some("Ran command".to_string()),
                details_json: None,
            }),
            last_sequence: 1,
        };

        assert_eq!(ai_tool_header_title(&item), "Ran command");
    }

    #[test]
    fn tool_compact_summary_uses_command_preview_when_summary_exists() {
        let item = ItemSummary {
            id: "item-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            kind: "commandExecution".to_string(),
            status: ItemStatus::Completed,
            content: "Finished test suite".to_string(),
            display_metadata: Some(ItemDisplayMetadata {
                summary: Some("Ran command".to_string()),
                details_json: Some(
                    r#"{
                        "kind": "commandExecution",
                        "command": "cargo clippy --workspace --all-targets -- -D warnings",
                        "cwd": "/repo",
                        "status": "completed"
                    }"#
                        .to_string(),
                ),
            }),
            last_sequence: 1,
        };

        assert_eq!(
            ai_tool_compact_summary(&item, item.content.trim()).as_deref(),
            Some("cargo clippy --workspace --all-targets -- -D warnings")
        );
    }

    #[test]
    fn reasoning_effort_labels_are_compact_and_human_readable() {
        assert_eq!(ai_reasoning_effort_label("high"), "High");
        assert_eq!(ai_reasoning_effort_label("extra_high"), "Extra High");
        assert_eq!(ai_reasoning_effort_label("extra-high"), "Extra High");
        assert_eq!(ai_reasoning_effort_label("medium"), "Medium");
    }

    #[test]
    fn composer_status_tone_hides_routine_transport_and_attachment_messages() {
        assert!(ai_composer_status_tone("Codex App Server connected over WebSocket").is_none());
        assert!(ai_composer_status_tone("Starting Codex App Server...").is_none());
        assert!(ai_composer_status_tone("Attached 2 images.").is_none());
        assert!(ai_composer_status_tone("Interrupted").is_some());
        assert!(ai_composer_status_tone("Prompt cannot be empty.").is_some());
    }

    #[test]
    fn chat_markdown_parses_inline_code_and_file_links() {
        let blocks = hunk_domain::markdown_preview::parse_markdown_preview(
            "Run `cargo fmt --all` in [ai.rs](/tmp/ai.rs#L72).",
        );

        let MarkdownPreviewBlock::Paragraph(spans) = &blocks[0] else {
            panic!("expected paragraph block");
        };

        assert!(spans
            .iter()
            .any(|span| span.style.code && span.text == "cargo fmt --all"));
        assert!(spans.iter().any(|span| {
            span.style.link.as_deref() == Some("/tmp/ai.rs#L72") && span.text == "ai.rs"
        }));
    }

    #[test]
    fn chat_markdown_text_keeps_link_text_inline() {
        let blocks = hunk_domain::markdown_preview::parse_markdown_preview(
            "That is now in [timeline_rows.rs](/tmp/timeline_rows.rs#L387), and wired.",
        );
        let MarkdownPreviewBlock::Paragraph(spans) = &blocks[0] else {
            panic!("expected paragraph block");
        };

        assert_eq!(
            markdown_inline_text_and_link_ranges(spans).0,
            "That is now in timeline_rows.rs, and wired."
        );
    }

    #[test]
    fn markdown_code_block_text_preserves_line_breaks() {
        let lines = vec![
            vec![
                MarkdownCodeSpan {
                    text: "cargo ".to_string(),
                    token: MarkdownCodeTokenKind::Plain,
                },
                MarkdownCodeSpan {
                    text: "test".to_string(),
                    token: MarkdownCodeTokenKind::Keyword,
                },
            ],
            vec![MarkdownCodeSpan {
                text: "--workspace".to_string(),
                token: MarkdownCodeTokenKind::Plain,
            }],
        ];

        assert_eq!(ai_markdown_code_block_text(&lines), "cargo test\n--workspace");
    }

    #[test]
    fn markdown_code_block_highlights_keep_plain_text_intact() {
        let lines = vec![vec![
            MarkdownCodeSpan {
                text: "fn".to_string(),
                token: MarkdownCodeTokenKind::Keyword,
            },
            MarkdownCodeSpan {
                text: " main".to_string(),
                token: MarkdownCodeTokenKind::Plain,
            },
            MarkdownCodeSpan {
                text: "()".to_string(),
                token: MarkdownCodeTokenKind::Operator,
            },
        ]];

        let default_color = gpui::transparent_black();
        let theme = gpui_component::Theme::default();
        let (text, highlights) =
            ai_markdown_code_block_text_and_highlights(&lines, &theme, default_color);

        assert_eq!(text.as_ref(), "fn main()");
        assert!(!highlights.is_empty());
    }
}
