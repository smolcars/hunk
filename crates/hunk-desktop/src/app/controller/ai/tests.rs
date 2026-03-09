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
    use super::ai_composer_draft_key;
    use super::ai_composer_prompt_for_target;
    use super::ai_prompt_send_waiting_on_connection;
    use super::resolved_ai_thread_mode_picker_state;
    use super::ai_attachment_status_message;
    use super::ai_branch_name_for_thread;
    use super::ai_thread_catalog_workspace_roots;
    use super::ai_thread_start_mode_for_workspace;
    use super::apply_ai_thread_catalog_to_workspace_state;
    use super::bundled_codex_executable_candidates;
    use super::codex_runtime_binary_name;
    use super::codex_runtime_platform_dir;
    use super::current_visible_thread_id_from_snapshot;
    use super::drain_ai_worker_events;
    use super::group_ai_timeline_rows_for_thread;
    use super::item_status_chip;
    use super::is_supported_ai_image_path;
    use super::is_command_name_without_path;
    use super::normalized_thread_session_state;
    use super::normalized_user_input_answers;
    use super::preferred_ai_worktree_base_branch_name;
    use super::resolve_bundled_codex_executable_from_exe;
    use super::resolved_ai_workspace_cwd;
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
    use crate::app::AiComposerDraft;
    use crate::app::AiComposerDraftKey;
    use crate::app::AiNewThreadStartMode;
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
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[cfg(target_os = "windows")]
    fn write_fake_windows_pe(path: &std::path::Path) {
        std::fs::write(path, b"MZfake-pe").expect("fake PE should be written");
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
    fn restore_ai_workspace_state_after_failure_reopens_pending_new_thread_draft() {
        let mut workspace_state = AiWorkspaceState {
            new_thread_draft_active: false,
            pending_new_thread_selection: true,
            pending_thread_start: Some(AiPendingThreadStart {
                workspace_key: "/repo/worktrees/task-1".to_string(),
                prompt: "pending".to_string(),
                local_images: Vec::new(),
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
                started_at: Instant::now(),
                start_mode: AiNewThreadStartMode::Worktree,
                thread_id: None,
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
                include_hidden_models: true,
                mad_max_mode: false,
            },
        );

        assert!(workspace_state.pending_thread_start.is_none());
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
                },
            ),
            (
                AiComposerDraftKey::Thread("thread-b".to_string()),
                AiComposerDraft {
                    prompt: "draft-b".to_string(),
                    local_images: vec![PathBuf::from("/tmp/b.png")],
                },
            ),
            (
                AiComposerDraftKey::Workspace("/repo".to_string()),
                AiComposerDraft {
                    prompt: "workspace-draft".to_string(),
                    local_images: Vec::new(),
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
    fn thread_catalog_workspace_roots_skip_visible_workspace_and_dedupe() {
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

        let roots = ai_thread_catalog_workspace_roots(workspace_targets.as_slice(), Some("/repo"));
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/repo/worktrees/task-1"),
                PathBuf::from("/repo/worktrees/task-2"),
            ]
        );
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
            last_workspace_target_by_repo: Default::default(),
            review_compare_selection_by_repo: Default::default(),
            ai_workspace_mad_max: [
                ("/repo-a".to_string(), true),
                ("/repo-b".to_string(), false),
            ]
            .into_iter()
            .collect(),
            ai_workspace_include_hidden_models: Default::default(),
            ai_workspace_session_overrides: Default::default(),
            git_workflow_cache: None,
            git_recent_commits_cache: None,
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
            last_workspace_target_by_repo: Default::default(),
            review_compare_selection_by_repo: Default::default(),
            ai_workspace_mad_max: Default::default(),
            ai_workspace_include_hidden_models: [
                ("/repo-a".to_string(), true),
                ("/repo-b".to_string(), false),
            ]
            .into_iter()
            .collect(),
            ai_workspace_session_overrides: Default::default(),
            git_workflow_cache: None,
            git_recent_commits_cache: None,
        };
        assert!(workspace_include_hidden_models(&state, Some("/repo-a")));
        assert!(!workspace_include_hidden_models(&state, Some("/repo-b")));
        assert!(workspace_include_hidden_models(&state, Some("/repo-c")));
    }

    #[test]
    fn resolved_ai_workspace_cwd_prefers_repo_root_when_paths_are_related() {
        let project = PathBuf::from("/repo/subdir");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(
            resolved_ai_workspace_cwd(Some(project.as_path()), Some(repo_root.as_path())),
            Some(repo_root),
        );
    }

    #[test]
    fn resolved_ai_workspace_cwd_prefers_selected_project_when_repo_root_is_stale() {
        let project = PathBuf::from("/repo-b");
        let stale_repo_root = PathBuf::from("/repo-a");
        assert_eq!(
            resolved_ai_workspace_cwd(Some(project.as_path()), Some(stale_repo_root.as_path())),
            Some(project),
        );
    }

    #[test]
    fn drain_ai_worker_events_preserves_final_fatal_before_disconnect() {
        let (event_tx, event_rx) = mpsc::channel();
        event_tx
            .send(crate::app::ai_runtime::AiWorkerEvent {
                workspace_key: "/repo-a".to_string(),
                payload: crate::app::ai_runtime::AiWorkerEventPayload::Fatal("boom".to_string()),
            })
            .expect("fatal event should send");
        drop(event_tx);

        let (events, disconnected) = drain_ai_worker_events(&event_rx);
        assert!(disconnected);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.first(),
            Some(crate::app::ai_runtime::AiWorkerEvent { workspace_key, payload: crate::app::ai_runtime::AiWorkerEventPayload::Fatal(message) })
                if workspace_key == "/repo-a" && message == "boom"
        ));
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
            collaboration_mode: AiCollaborationModeSelection::Plan,
            service_tier: Some(AiServiceTierSelection::Fast),
        };
        assert_eq!(
            normalized_thread_session_state(session.clone()),
            Some(session),
        );
    }

    #[test]
    fn normalized_thread_session_state_drops_standard_service_tier_only() {
        let session = AiThreadSessionState {
            model: None,
            effort: None,
            collaboration_mode: AiCollaborationModeSelection::Default,
            service_tier: Some(AiServiceTierSelection::Standard),
        };
        assert_eq!(normalized_thread_session_state(session), None);
    }

    #[test]
    fn normalized_thread_session_state_strips_standard_service_tier_from_overrides() {
        let session = AiThreadSessionState {
            model: Some("gpt-5-codex".to_string()),
            effort: None,
            collaboration_mode: AiCollaborationModeSelection::Default,
            service_tier: Some(AiServiceTierSelection::Standard),
        };
        assert_eq!(
            normalized_thread_session_state(session),
            Some(AiThreadSessionState {
                model: Some("gpt-5-codex".to_string()),
                effort: None,
                collaboration_mode: AiCollaborationModeSelection::Default,
                service_tier: None,
            })
        );
    }

    #[test]
    fn normalized_thread_session_state_drops_default_collaboration_mode() {
        let session = AiThreadSessionState {
            model: None,
            effort: None,
            collaboration_mode: AiCollaborationModeSelection::Default,
            service_tier: None,
        };
        assert_eq!(normalized_thread_session_state(session), None);
    }

    #[test]
    fn normalized_ai_session_selection_preserves_server_default_when_unset() {
        let models = vec![ai_model(
            "gpt-5.3-codex",
            "5.3 Codex",
            true,
            &[ReasoningEffort::Low, ReasoningEffort::Medium],
            ReasoningEffort::Medium,
        )];

        assert_eq!(
            normalized_ai_session_selection(models.as_slice(), None, None),
            (None, None),
        );
    }

    #[test]
    fn normalized_ai_session_selection_preserves_model_default_effort_when_unset() {
        let models = vec![ai_model(
            "gpt-5.3-codex",
            "5.3 Codex",
            true,
            &[ReasoningEffort::Low, ReasoningEffort::Medium],
            ReasoningEffort::Medium,
        )];

        assert_eq!(
            normalized_ai_session_selection(
                models.as_slice(),
                Some("gpt-5.3-codex".to_string()),
                None,
            ),
            (Some("gpt-5.3-codex".to_string()), None),
        );
    }

    #[test]
    fn command_name_without_path_detection_is_stable() {
        assert!(is_command_name_without_path(std::path::Path::new("codex")));
        assert!(!is_command_name_without_path(std::path::Path::new("./codex")));
        assert!(!is_command_name_without_path(std::path::Path::new("/usr/bin/codex")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_command_resolution_prefers_spawnable_launcher_on_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-cmd-{unique}"));
        let bin_dir = root.join("bin");
        std::fs::create_dir_all(&bin_dir).expect("bin dir should be created");
        std::fs::write(bin_dir.join("codex"), "#!/bin/sh\n").expect("unix shim should be written");
        let launcher_path = bin_dir.join("codex.cmd");
        std::fs::write(&launcher_path, "@echo off\r\n").expect("fake launcher should be written");

        let resolved = resolve_windows_command_path_from_env(
            std::path::Path::new("codex"),
            Some(std::env::join_paths([bin_dir.as_path()]).expect("path should join")),
            Some(OsString::from(".COM;.EXE;.BAT;.CMD")),
        );

        assert_eq!(
            resolved.map(|path| path.to_string_lossy().to_ascii_lowercase()),
            Some(launcher_path.to_string_lossy().to_ascii_lowercase()),
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_explicit_command_path_prefers_adjacent_launcher_over_unix_shim() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-explicit-{unique}"));
        std::fs::create_dir_all(&root).expect("root dir should be created");
        let unix_shim_path = root.join("codex");
        std::fs::write(&unix_shim_path, "#!/bin/sh\n").expect("unix shim should be written");
        let launcher_path = root.join("codex.cmd");
        std::fs::write(&launcher_path, "@echo off\r\n").expect("fake launcher should be written");

        let resolved = resolve_windows_command_path(unix_shim_path.as_path());

        assert_eq!(
            resolved.map(|path| path.to_string_lossy().to_ascii_lowercase()),
            Some(launcher_path.to_string_lossy().to_ascii_lowercase()),
        );
        let _ = std::fs::remove_dir_all(root);
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
        #[cfg(target_os = "windows")]
        write_fake_windows_pe(runtime_path.as_path());
        #[cfg(not(target_os = "windows"))]
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

    #[test]
    fn ai_text_selection_tracks_forward_ranges() {
        let mut selection = AiTextSelection::new(
            "row".to_string(),
            ai_selection_surfaces([("surface", "hello world", "")]).as_slice(),
            "surface",
            0,
        );
        selection.set_head_for_surface("surface", 5);

        assert_eq!(selection.range(), 0..5);
        assert_eq!(selection.selected_text().as_deref(), Some("hello"));
        assert_eq!(selection.range_for_surface("surface"), Some(0..5));
    }

    #[test]
    fn ai_text_selection_tracks_reverse_ranges() {
        let mut selection = AiTextSelection::new(
            "row".to_string(),
            ai_selection_surfaces([("surface", "hello world", "")]).as_slice(),
            "surface",
            8,
        );
        selection.set_head_for_surface("surface", 2);

        assert_eq!(selection.range(), 2..8);
        assert_eq!(selection.selected_text().as_deref(), Some("llo wo"));
    }

    #[test]
    fn ai_text_selection_select_all_covers_full_surface() {
        let mut selection = AiTextSelection::new(
            "row".to_string(),
            ai_selection_surfaces([("surface", "entire message", "")]).as_slice(),
            "surface",
            4,
        );
        selection.select_all();

        assert_eq!(selection.range(), 0.."entire message".len());
        assert_eq!(
            selection.selected_text().as_deref(),
            Some("entire message")
        );
        assert!(!selection.dragging);
    }

    #[test]
    fn ai_text_selection_spans_multiple_surfaces_in_same_row() {
        let surfaces = ai_selection_surfaces([
            ("surface-a", "hello", ""),
            ("surface-b", "world", "\n\n"),
        ]);
        let mut selection = AiTextSelection::new("row".to_string(), surfaces.as_slice(), "surface-a", 2);
        selection.set_head_for_surface("surface-b", 3);

        assert_eq!(selection.selected_text().as_deref(), Some("llo\n\nwor"));
        assert_eq!(selection.range_for_surface("surface-a"), Some(2..5));
        assert_eq!(selection.range_for_surface("surface-b"), Some(0..3));
    }

    #[test]
    fn ai_text_selection_returns_none_for_non_overlapping_surface() {
        let surfaces = ai_selection_surfaces([
            ("surface-a", "hello", ""),
            ("surface-b", "world", "\n\n"),
        ]);
        let mut selection =
            AiTextSelection::new("row".to_string(), surfaces.as_slice(), "surface-a", 1);
        selection.set_head_for_surface("surface-a", 4);

        assert_eq!(selection.selected_text().as_deref(), Some("ell"));
        assert_eq!(selection.range_for_surface("surface-a"), Some(1..4));
        assert_eq!(selection.range_for_surface("surface-b"), None);
    }
}
