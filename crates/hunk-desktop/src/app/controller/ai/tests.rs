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
    use super::ai_completion_reload_workspace_root;
    use super::ai_composer_draft_key;
    use super::ai_composer_prompt_for_target;
    use super::ai_composer_retained_thread_ids;
    use super::ai_prompt_send_waiting_on_connection;
    use super::ai_pending_steer_seed_content;
    use super::resolved_ai_thread_mode_picker_state;
    use super::ai_attachment_status_message;
    use super::ai_branch_name_for_thread;
    use super::ai_composer_shortcut_for_keystroke;
    use super::AiComposerShortcut;
    use super::background_branch_name_for_new_thread;
    use super::ai_thread_catalog_workspace_roots;
    use super::ai_thread_workspace_matches_current_project;
    use super::ai_thread_start_mode_for_workspace;
    use super::apply_ai_thread_catalog_to_workspace_state;
    use super::bundled_codex_executable_candidates;
    use super::codex_runtime_binary_name;
    use super::codex_runtime_platform_dir;
    use super::current_visible_thread_id_from_snapshot;
    use super::current_visible_thread_fallback_workspace_key;
    use super::drain_ai_worker_events;
    use super::group_ai_timeline_rows_for_thread;
    use super::item_status_chip;
    use super::is_supported_ai_image_path;
    use super::is_command_name_without_path;
    use super::normalized_thread_session_state;
    use super::normalized_user_input_answers;
    use super::preferred_ai_worktree_base_branch_name;
    use super::review_compare_selection_ids_for_workspace_root;
    use super::requested_branch_name_for_new_thread;
    use super::running_from_packaged_bundle;
    use super::resolve_bundled_codex_executable_from_exe;
    use super::resolved_ai_workspace_cwd;
    use super::seed_ai_workspace_preferences;
    #[cfg(target_os = "windows")]
    use super::resolve_windows_command_path;
    #[cfg(target_os = "windows")]
    use super::resolve_windows_command_path_from_env;
    use super::should_follow_timeline_output;
    use super::next_thread_metadata_refresh_attempt;
    use super::normalized_ai_session_selection;
    use super::should_reset_ai_timeline_measurements;
    use super::should_scroll_timeline_to_bottom_on_new_activity;
    use super::sorted_threads;
    use super::reconcile_ai_queued_messages_after_snapshot;
    use super::ready_ai_queued_message_thread_ids;
    use super::take_last_editable_ai_queued_message_for_thread;
    use super::thread_metadata_refresh_key;
    use super::timeline_turn_ids_by_thread;
    use super::timeline_row_ids_with_height_changes;
    use super::timeline_visible_row_ids_for_turns;
    use super::timeline_visible_turn_ids;
    use super::should_scroll_timeline_to_bottom_on_selection_change;
    use super::should_sync_selected_thread_from_active_thread;
    use super::thread_latest_timeline_sequence;
    use super::workspace_include_hidden_models;
    use super::workspace_mad_max_mode;
    use super::reconcile_ai_pending_steers;
    use super::take_restorable_ai_pending_steers;
    use crate::app::AiComposerDraft;
    use crate::app::AiComposerDraftKey;
    use crate::app::AiNewThreadStartMode;
    use crate::app::AiQueuedUserMessage;
    use crate::app::AiQueuedUserMessageStatus;
    use crate::app::AiPendingSteer;
    use crate::app::AiPendingThreadStart;
    use crate::app::AiThreadTitleRefreshState;
    use crate::app::AiTextSelection;
    use crate::app::AiTextSelectionSurfaceSpec;
    use crate::app::AiTimelineRow;
    use crate::app::AiTimelineRowSource;
    use crate::app::AiWorkspaceState;
    use crate::app::DiffViewer;
    use crate::app::ai_runtime::AiWorkspaceThreadCatalog;
    use crate::app::ai_runtime::AiPendingUserInputQuestion;
    use crate::app::ai_runtime::AiPendingUserInputQuestionOption;
    use crate::app::ai_runtime::AiPendingUserInputRequest;
    use crate::app::ai_runtime::AiConnectionState;
    use crate::app::ai_runtime::AiSnapshot;
    use crate::app::review_compare_picker::ReviewCompareSourceOption;
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
    use hunk_git::git::LocalBranch;
    use hunk_git::worktree::WorkspaceTargetKind;
    use hunk_git::worktree::WorkspaceTargetSummary;
    use codex_app_server_protocol::Model;
    use codex_app_server_protocol::ReasoningEffortOption;
    use codex_protocol::openai_models::ReasoningEffort;
    use std::collections::{BTreeMap, BTreeSet};
    use std::env;
    #[cfg(target_os = "windows")]
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    fn with_locked_env<T>(f: impl FnOnce() -> T) -> T {
        let _guard = ENV_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env mutex should not be poisoned");
        f()
    }

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
            is_default,
        }
    }

    include!("tests/workspace_state.rs");
    include!("tests/queued_messages.rs");
    include!("tests/timeline.rs");
    include!("tests/selection_and_refresh.rs");
    include!("tests/runtime_path_and_session.rs");
    include!("tests/text_selection.rs");
}
