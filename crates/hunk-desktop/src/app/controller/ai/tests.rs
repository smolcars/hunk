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
    use super::ai_auth_required_message;
    use super::ai_branch_name_for_thread;
    use super::ai_completion_reload_workspace_root;
    use super::ai_cycle_composer_mode_target;
    use super::ai_followup_prompt_for_thread;
    use super::ai_followup_prompt_action_for_keystroke;
    use super::ai_visible_followup_prompt_for_selected_thread;
    use super::ai_historical_turn_diff_key_for_row;
    use super::ai_historical_inline_review_loaded_state;
    use super::ai_inline_review_toggle_target_mode;
    use super::ai_inline_review_uses_review_compare_session_for_surface;
    use super::ai_latest_supported_inline_review_row_id_for_visible_rows;
    use super::ai_prominent_worker_status_error;
    use super::ai_resolved_inline_review_row_id_for_visible_rows;
    use super::ai_timeline_row_supports_inline_review;
    use super::AI_AUTH_REQUIRED_MESSAGE;
    use super::ai_workspace_catalog_inputs_from_target_sets;
    use super::ai_visible_thread_sections;
    use super::ai_composer_draft_key;
    use super::ai_composer_prompt_for_target;
    use super::ai_composer_retained_thread_ids;
    use super::ai_composer_shortcut_for_keystroke;
    use super::ai_pending_steer_seed_content;
    use super::ai_prompt_send_waiting_on_connection;
    use super::ai_thread_catalog_workspace_roots;
    use super::ai_thread_start_mode_for_workspace;
    use super::ai_thread_workspace_matches_current_project;
    use super::ai_turn_keys_with_file_change_items;
    use super::apply_ai_thread_catalog_to_workspace_state;
    use super::background_branch_name_for_new_thread;
    use super::bundled_codex_executable_candidates;
    use super::cached_workspace_branch_name_for_root;
    use super::composer_status_message_for_visible_target;
    use super::codex_runtime_binary_name;
    use super::codex_runtime_platform_dir;
    use super::current_visible_thread_fallback_workspace_key;
    use super::current_visible_thread_id_from_snapshot;
    use super::drain_ai_worker_events;
    use super::group_ai_timeline_rows_for_thread;
    use super::is_command_name_without_path;
    use super::item_status_chip;
    use super::merge_restored_ai_prompt;
    use super::next_thread_metadata_refresh_attempt;
    use super::normalized_ai_session_selection;
    use super::normalized_thread_session_state;
    use super::normalized_user_input_answers;
    use super::preferred_ai_worktree_base_branch_name;
    use super::ready_ai_queued_message_thread_ids;
    use super::reconcile_ai_pending_steers;
    use super::reconcile_ai_queued_messages_after_snapshot;
    use super::requested_branch_name_for_new_thread;
    use super::resolve_bundled_codex_executable_from_exe;
    #[cfg(target_os = "windows")]
    use super::resolve_windows_command_path;
    #[cfg(target_os = "windows")]
    use super::resolve_windows_command_path_from_env;
    use super::resolve_workspace_codex_executable_from_exe;
    use super::resolved_ai_turn_session_overrides;
    use super::resolved_ai_thread_session_state;
    use super::resolved_ai_thread_mode_picker_state;
    use super::resolved_ai_workspace_cwd;
    use super::review_compare_selection_ids_for_workspace_root;
    use super::review_mode_selected_path;
    use super::seed_ai_workspace_preferences;
    use super::selected_git_workspace_review_compare_selection_ids;
    use super::update_persisted_review_compare_selection;
    use super::ai_workspace_selection_surfaces;
    use super::ai_snapshot_removed_retainable_terminal_threads;
    use super::ai_snapshot_removed_thread_ids;
    use super::ai_snapshot_threads_changed;
    use super::should_follow_timeline_output;
    use super::should_scroll_timeline_to_bottom_on_new_activity;
    use super::should_scroll_timeline_to_bottom_on_selection_change;
    use super::should_sync_selected_thread_from_active_thread;
    use super::sorted_threads;
    use super::sync_ai_followup_prompt_ui_state;
    use super::sync_ai_review_mode_threads_after_snapshot;
    use super::take_last_editable_ai_queued_message_for_thread;
    use super::take_restorable_ai_pending_steers;
    use super::thread_latest_timeline_sequence;
    use super::thread_metadata_refresh_key;
    use super::timeline_row_ids_with_height_changes;
    use super::timeline_turn_ids_by_thread;
    use super::timeline_visible_row_ids_for_turns;
    use super::timeline_visible_turn_ids;
    use super::workspace_target_summary_for_root;
    use super::workspace_branch_name_for_root;
    use super::workspace_include_hidden_models;
    use super::workspace_mad_max_mode;
    use super::AiComposerModeTarget;
    use super::AiComposerShortcut;
    use crate::app::ai_composer_completion::merge_rebased_ai_composer_skill_bindings;
    use crate::app::ai_paths::resolve_ai_chats_root_path;
    use crate::app::ai_runtime::AiConnectionState;
    use crate::app::ai_workspace_session;
    use crate::app::ai_workspace_surface::ai_workspace_selectable_text_context_menu_target;
    use crate::app::AiTurnSessionOverrides;
    use crate::app::ai_runtime::AiPendingUserInputQuestion;
    use crate::app::ai_runtime::AiPendingUserInputQuestionOption;
    use crate::app::ai_runtime::AiPendingUserInputRequest;
    use crate::app::ai_runtime::AiSnapshot;
    use crate::app::ai_runtime::AiWorkspaceThreadCatalog;
    use crate::app::review_compare_picker::ReviewCompareSourceOption;
    use crate::app::AiComposerDraft;
    use crate::app::AiComposerDraftKey;
    use crate::app::AiComposerSkillBinding;
    use crate::app::AiFollowupPrompt;
    use crate::app::AiFollowupPromptAction;
    use crate::app::AiFollowupPromptKind;
    use crate::app::AiInlineReviewMode;
    use crate::app::AiNewThreadStartMode;
    use crate::app::AiPendingSteer;
    use crate::app::AiPendingThreadStart;
    use crate::app::AiPromptSkillReference;
    use crate::app::AiQueuedUserMessage;
    use crate::app::AiQueuedUserMessageStatus;
    use crate::app::AiTextSelection;
    use crate::app::AiTextSelectionSurfaceSpec;
    use crate::app::AiThreadFollowupPromptState;
    use crate::app::AiThreadTitleRefreshState;
    use crate::app::AiTimelineRow;
    use crate::app::AiTimelineRowSource;
    use crate::app::AiWorkspaceKind;
    use crate::app::AiWorkspaceState;
    use crate::app::DiffViewer;
    use crate::app::WorkspaceViewMode;
    use codex_app_server_protocol::Model;
    use codex_app_server_protocol::ReasoningEffortOption;
    use codex_protocol::openai_models::ReasoningEffort;
    use gpui::Keystroke;
    use hunk_codex::state::AiState;
    use hunk_codex::state::ItemDisplayMetadata;
    use hunk_codex::state::ItemStatus;
    use hunk_codex::state::ThreadLifecycleStatus;
    use hunk_codex::state::ThreadSummary;
    use hunk_domain::state::AiCollaborationModeSelection;
    use hunk_domain::state::AiServiceTierSelection;
    use hunk_domain::state::AiThreadSessionState;
    use hunk_domain::state::AppState;
    use hunk_domain::state::CachedLocalBranchState;
    use hunk_domain::state::CachedWorkflowState;
    use hunk_domain::state::ReviewCompareSelectionState;
    use hunk_git::git::LocalBranch;
    use hunk_git::worktree::WorkspaceTargetKind;
    use hunk_git::worktree::WorkspaceTargetSummary;
    use std::collections::{BTreeMap, BTreeSet};
    use std::env;
    #[cfg(target_os = "windows")]
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::sync::mpsc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[cfg(target_os = "windows")]
    fn write_fake_windows_pe(path: &std::path::Path) {
        std::fs::write(path, b"MZfake-pe").expect("fake PE should be written");
    }

    #[cfg(target_os = "windows")]
    fn write_fake_codex_launcher(path: &std::path::Path) {
        std::fs::write(
            path,
            "@echo off\r\nif /I \"%~1\"==\"app-server\" if /I \"%~2\"==\"--help\" exit /b 0\r\nexit /b 1\r\n",
        )
        .expect("fake launcher should be written");
    }

    #[cfg(not(target_os = "windows"))]
    fn write_fake_codex_launcher(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;

        std::fs::write(
            path,
            "#!/bin/sh\nif [ \"$1\" = \"app-server\" ] && [ \"$2\" = \"--help\" ]; then\n  exit 0\nfi\nexit 1\n",
        )
        .expect("fake launcher should be written");
        let mut permissions = std::fs::metadata(path)
            .expect("launcher metadata should be readable")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("launcher should be executable");
    }

    #[allow(clippy::too_many_arguments)]
    fn timeline_tool_item(
        item_id: &str,
        thread_id: &str,
        turn_id: &str,
        kind: &str,
        status: ItemStatus,
        content: &str,
        details_json: &str,
        last_sequence: u64,
    ) -> hunk_codex::state::ItemSummary {
        hunk_codex::state::ItemSummary {
            id: item_id.to_string(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: kind.to_string(),
            status,
            content: content.to_string(),
            display_metadata: Some(ItemDisplayMetadata {
                summary: Some(kind.to_string()),
                details_json: Some(details_json.to_string()),
            }),
            last_sequence,
        }
    }

    fn timeline_item_row(
        row_id: &str,
        thread_id: &str,
        turn_id: &str,
        last_sequence: u64,
        item_key: &str,
    ) -> AiTimelineRow {
        AiTimelineRow {
            id: row_id.to_string(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            last_sequence,
            source: AiTimelineRowSource::Item {
                item_key: item_key.to_string(),
            },
        }
    }

    fn ai_selection_surfaces(
        surfaces: impl IntoIterator<Item = (&'static str, &'static str, &'static str)>,
    ) -> Vec<AiTextSelectionSurfaceSpec> {
        surfaces
            .into_iter()
            .map(|(surface_id, text, separator_before)| {
                AiTextSelectionSurfaceSpec::new(surface_id, text)
                    .with_separator_before(separator_before)
            })
            .collect()
    }

    fn workspace_target(
        id: &str,
        kind: WorkspaceTargetKind,
        root: &str,
        display_name: &str,
    ) -> WorkspaceTargetSummary {
        WorkspaceTargetSummary {
            id: id.to_string(),
            kind,
            root: PathBuf::from(root),
            name: display_name.to_string(),
            display_name: display_name.to_string(),
            branch_name: "main".to_string(),
            managed: matches!(kind, WorkspaceTargetKind::LinkedWorktree),
            is_active: false,
        }
    }

    fn local_branch(name: &str, is_current: bool) -> LocalBranch {
        LocalBranch {
            name: name.to_string(),
            is_current,
            is_remote_tracking: false,
            tip_unix_time: None,
            attached_workspace_target_id: None,
            attached_workspace_target_root: None,
            attached_workspace_target_label: None,
        }
    }

    #[test]
    fn ai_composer_shortcut_for_keystroke_matches_queue_and_edit_shortcuts() {
        assert!(matches!(
            ai_composer_shortcut_for_keystroke(&Keystroke::parse("tab").expect("valid keystroke")),
            Some(AiComposerShortcut::QueuePrompt)
        ));
        assert!(matches!(
            ai_composer_shortcut_for_keystroke(
                &Keystroke::parse("ctrl-shift-up").expect("valid keystroke")
            ),
            Some(AiComposerShortcut::EditLastQueuedPrompt)
        ));
        assert!(ai_composer_shortcut_for_keystroke(
            &Keystroke::parse("ctrl-up").expect("valid keystroke")
        )
        .is_none());
        assert!(ai_composer_shortcut_for_keystroke(
            &Keystroke::parse("shift-tab").expect("valid keystroke")
        )
        .is_none());
        assert!(ai_composer_shortcut_for_keystroke(
            &Keystroke::parse("up").expect("valid keystroke")
        )
        .is_none());
    }

    fn ai_model(
        id: &str,
        display_name: &str,
        is_default: bool,
        supported_reasoning_efforts: &[ReasoningEffort],
        default_reasoning_effort: ReasoningEffort,
    ) -> Model {
        Model {
            id: id.to_string(),
            model: id.to_string(),
            upgrade: None,
            upgrade_info: None,
            availability_nux: None,
            display_name: display_name.to_string(),
            description: String::new(),
            hidden: false,
            supported_reasoning_efforts: supported_reasoning_efforts
                .iter()
                .cloned()
                .map(|reasoning_effort| ReasoningEffortOption {
                    reasoning_effort,
                    description: String::new(),
                })
                .collect(),
            default_reasoning_effort,
            input_modalities: Vec::new(),
            supports_personality: false,
            additional_speed_tiers: Vec::new(),
            is_default,
        }
    }

    fn ai_test_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_temp_hunk_home<T>(test_name: &str, f: impl FnOnce(PathBuf) -> T) -> T {
        let _guard = ai_test_env_lock()
            .lock()
            .expect("ai test env lock should be available");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!("hunk-ai-test-{test_name}-{unique}"));
        let previous = std::env::var_os(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR);
        unsafe { std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, &temp_home) };
        let _ = std::fs::remove_dir_all(&temp_home);
        std::fs::create_dir_all(&temp_home).expect("temp hunk home should be created");

        let result = f(temp_home.clone());

        match previous {
            Some(value) => unsafe {
                std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, value)
            },
            None => unsafe { std::env::remove_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR) },
        }
        let _ = std::fs::remove_dir_all(&temp_home);

        result
    }

    include!("tests/workspace_state.rs");
    include!("tests/queued_messages.rs");
    include!("tests/timeline.rs");
    include!("tests/selection_and_refresh.rs");
    include!("tests/runtime_path_and_session.rs");
    include!("tests/followup_prompts.rs");
    include!("tests/composer_status_scope.rs");
    include!("tests/workspace_surface_plan_items.rs");
    include!("tests/text_selection.rs");
}
