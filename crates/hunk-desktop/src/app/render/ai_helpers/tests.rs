#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod ai_helper_tests {
    use super::ai_account_summary;
    use super::ai_chat_markdown_text;
    use super::ai_command_execution_display_details;
    use super::ai_composer_status_tone;
    use super::ai_thread_status_text;
    use super::ai_item_display_label;
    use super::ai_reasoning_effort_label;
    use super::ai_rate_limit_summary;
    use super::ai_tool_header_label;
    use super::ai_timeline_item_is_renderable;
    use super::ai_truncate_multiline_content;
    use hunk_codex::state::ItemDisplayMetadata;
    use hunk_codex::state::ItemStatus;
    use hunk_codex::state::ItemSummary;
    use hunk_codex::state::ThreadLifecycleStatus;
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
    fn thread_status_text_maps_lifecycle_states() {
        assert_eq!(ai_thread_status_text(ThreadLifecycleStatus::Active), "active");
        assert_eq!(ai_thread_status_text(ThreadLifecycleStatus::Idle), "idle");
        assert_eq!(
            ai_thread_status_text(ThreadLifecycleStatus::NotLoaded),
            "not loaded"
        );
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
            ai_chat_markdown_text(spans),
            "That is now in timeline_rows.rs, and wired."
        );
    }
}
