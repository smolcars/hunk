impl DiffViewer {
    const AUTO_REFRESH_MAX_INTERVAL_MS: u64 = 60_000;
    const AUTO_REFRESH_QUICK_PROBE_MS: u64 = 3_000;
    const AUTO_REFRESH_BACKOFF_STEPS: u32 = 6;
    const REPO_WATCH_DEBOUNCE: Duration = Duration::from_millis(150);
    const LINE_STATS_BACKGROUND_DEBOUNCE: Duration = Duration::from_millis(350);

    fn load_app_config() -> (Option<ConfigStore>, AppConfig) {
        let store = match ConfigStore::new() {
            Ok(store) => store,
            Err(err) => {
                error!("failed to initialize config path: {err:#}");
                return (None, AppConfig::default());
            }
        };

        match store.load_or_create_default() {
            Ok(config) => (Some(store), config),
            Err(err) => {
                error!(
                    "failed to load app config from {}: {err:#}",
                    store.path().display()
                );
                (Some(store), AppConfig::default())
            }
        }
    }

    fn load_app_state() -> (Option<AppStateStore>, AppState) {
        let store = match AppStateStore::new() {
            Ok(store) => store,
            Err(err) => {
                error!("failed to initialize app state path: {err:#}");
                return (None, AppState::default());
            }
        };

        match store.load_or_default() {
            Ok(state) => (Some(store), state),
            Err(err) => {
                error!(
                    "failed to load app state from {}: {err:#}",
                    store.path().display()
                );
                (Some(store), AppState::default())
            }
        }
    }

    fn apply_theme_preference(&self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = match self.config.theme {
            ThemePreference::System => ThemeMode::from(window.appearance()),
            ThemePreference::Light => ThemeMode::Light,
            ThemePreference::Dark => ThemeMode::Dark,
        };
        Theme::change(mode, Some(window), cx);
    }

    fn persist_config(&self) {
        let Some(store) = &self.config_store else {
            return;
        };

        if let Err(err) = store.save(&self.config) {
            error!(
                "failed to save app config to {}: {err:#}",
                store.path().display()
            );
        }
    }

    fn persist_state(&self) {
        let Some(store) = &self.state_store else {
            return;
        };

        if let Err(err) = store.save(&self.state) {
            error!(
                "failed to save app state to {}: {err:#}",
                store.path().display()
            );
        }
    }

    fn set_active_workspace_project_path(&mut self, project_path: Option<PathBuf>) {
        let changed = match project_path {
            Some(project_path) => self.state.activate_workspace_project(project_path),
            None => {
                let previous_active = self.state.active_workspace_project_path.clone();
                self.state.active_workspace_project_path = None;
                self.state.normalize_workspace_state();
                self.state.active_workspace_project_path != previous_active
            }
        };
        if !changed {
            return;
        }
        self.persist_state();
    }

    fn current_workspace_project_key(&self) -> Option<String> {
        self.project_path
            .as_ref()
            .or(self.repo_root.as_ref())
            .map(|path| path.to_string_lossy().to_string())
    }

    fn workflow_cache_unix_time() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
    }

    fn file_status_from_cache_tag(status_tag: &str) -> FileStatus {
        match status_tag {
            "A" => FileStatus::Added,
            "M" => FileStatus::Modified,
            "D" => FileStatus::Deleted,
            "R" => FileStatus::Renamed,
            "U" => FileStatus::Untracked,
            "T" => FileStatus::TypeChange,
            "!" => FileStatus::Conflicted,
            _ => FileStatus::Unknown,
        }
    }

    fn hydrate_workflow_cache_if_available(&mut self, cx: &mut Context<Self>) {
        let Some(expected_root) = self
            .project_path
            .clone()
            .or_else(|| self.state.active_project_path().cloned())
        else {
            return;
        };
        let cache_key = expected_root.to_string_lossy().to_string();
        let Some(cache) = self
            .state
            .git_workflow_cache_by_repo
            .get(cache_key.as_str())
            .cloned()
        else {
            return;
        };
        let Some(root) = cache.root.clone() else {
            return;
        };
        let cached_project_root =
            hunk_git::worktree::primary_repo_root(root.as_path()).unwrap_or_else(|_| root.clone());
        if cached_project_root != expected_root {
            return;
        }

        let previous_ai_workspace_key = self
            .ai_worker_workspace_key
            .clone()
            .or_else(|| self.ai_workspace_key());
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.project_path = Some(cached_project_root);
        self.repo_root = Some(root.clone());
        self.active_workspace_target_id = self.persisted_workspace_target_id();
        self.ai_handle_workspace_change(previous_ai_workspace_key, cx);
        self.branch_name = if cache.branch_name.is_empty() {
            "unknown".to_string()
        } else {
            cache.branch_name
        };
        self.branch_has_upstream = cache.branch_has_upstream;
        self.branch_ahead_count = cache.branch_ahead_count;
        self.branch_behind_count = cache.branch_behind_count;
        self.branches = cache
            .branches
            .into_iter()
            .map(|branch| LocalBranch {
                name: branch.name,
                is_current: branch.is_current,
                is_remote_tracking: branch.is_remote_tracking,
                tip_unix_time: branch.tip_unix_time,
                attached_workspace_target_id: branch.attached_workspace_target_id,
                attached_workspace_target_root: branch.attached_workspace_target_root,
                attached_workspace_target_label: branch.attached_workspace_target_label,
            })
            .collect();
        self.files = cache
            .files
            .into_iter()
            .map(|file| ChangedFile {
                path: file.path,
                status: Self::file_status_from_cache_tag(file.status_tag.as_str()),
                staged: file.staged,
                unstaged: file.unstaged,
                untracked: file.untracked,
            })
            .collect();
        self.file_status_by_path = self
            .files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect();
        self.last_commit_subject = cache.last_commit_subject;
        self.selected_path = self
            .selected_path
            .clone()
            .filter(|selected| self.files.iter().any(|file| &file.path == selected))
            .or_else(|| self.files.first().map(|file| file.path.clone()));
        self.selected_status = self
            .selected_path
            .as_deref()
            .and_then(|selected| self.status_for_path(selected));
        self.sync_ai_worktree_base_branch_from_repo();
        self.sync_branch_picker_state(cx);
        self.sync_ai_worktree_base_branch_picker_state(cx);
        self.refresh_workspace_targets_from_git_state(cx);
        self.sync_git_workspace_with_primary_state();
        self.repo_discovery_failed = false;
        self.error_message = None;
        debug!(
            "hydrated git workflow cache for {} (files={} branches={})",
            root.display(),
            self.files.len(),
            self.branches.len(),
        );
        cx.notify();
    }

    fn persist_workflow_cache(&mut self) {
        let Some(root) = self.repo_root.clone() else {
            return;
        };
        let Some(cache_key) = self.current_workspace_project_key() else {
            return;
        };

        let mut cache = CachedWorkflowState {
            root: Some(root),
            branch_name: self.branch_name.clone(),
            branch_has_upstream: self.branch_has_upstream,
            branch_ahead_count: self.branch_ahead_count,
            branch_behind_count: self.branch_behind_count,
            branches: self
                .branches
                .iter()
                .map(|branch| CachedLocalBranchState {
                    name: branch.name.clone(),
                    is_current: branch.is_current,
                    is_remote_tracking: branch.is_remote_tracking,
                    tip_unix_time: branch.tip_unix_time,
                    attached_workspace_target_id: branch.attached_workspace_target_id.clone(),
                    attached_workspace_target_root: branch.attached_workspace_target_root.clone(),
                    attached_workspace_target_label: branch.attached_workspace_target_label.clone(),
                })
                .collect(),
            files: self
                .files
                .iter()
                .map(|file| CachedChangedFileState {
                    path: file.path.clone(),
                    status_tag: file.status.tag().to_string(),
                    staged: file.staged,
                    unstaged: file.unstaged,
                    untracked: file.untracked,
                })
                .collect(),
            last_commit_subject: self.last_commit_subject.clone(),
            cached_unix_time: 0,
        };

        if let Some(previous) = self
            .state
            .git_workflow_cache_by_repo
            .get(cache_key.as_str())
        {
            let mut previous_without_time = previous.clone();
            previous_without_time.cached_unix_time = 0;
            if previous_without_time == cache {
                return;
            }
        }

        cache.cached_unix_time = Self::workflow_cache_unix_time();
        self.state
            .git_workflow_cache_by_repo
            .insert(cache_key, cache);
        self.persist_state();
    }

    fn sync_theme_with_system_if_needed(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.config.theme != ThemePreference::System {
            return;
        }
        self.apply_theme_preference(window, cx);
    }

    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (config_store, config) = Self::load_app_config();
        let (state_store, mut state) = Self::load_app_state();
        let preferred_ai_session = hunk_domain::state::AiThreadSessionState::preferred_defaults();
        let database_store = Self::load_database_store();
        let update_install_source = hunk_updater::detect_install_source();
        let update_status = match &update_install_source {
            InstallSource::SelfManaged => UpdateStatus::Idle,
            InstallSource::PackageManaged { explanation } => {
                UpdateStatus::DisabledByInstallSource {
                    explanation: explanation.clone(),
                }
            }
        };
        state.normalize_workspace_state();
        let initial_project_path = state.active_project_path().cloned();
        let initial_ai_workspace_key = initial_project_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string());
        let initial_ai_mad_max_mode = initial_ai_workspace_key
            .as_ref()
            .and_then(|workspace| state.ai_workspace_mad_max.get(workspace))
            .copied()
            .unwrap_or(false);
        let initial_ai_include_hidden_models = initial_ai_workspace_key
            .as_ref()
            .and_then(|workspace| state.ai_workspace_include_hidden_models.get(workspace))
            .copied()
            .unwrap_or(true);
        let branch_picker_state = cx.new(|cx| {
            HunkPickerState::new(
                BranchPickerDelegate::default(),
                None,
                "Find a branch",
                window,
                cx,
            )
        });
        let ai_worktree_base_branch_picker_state = cx.new(|cx| {
            HunkPickerState::new(
                BranchPickerDelegate::default(),
                None,
                "Choose a base branch",
                window,
                cx,
            )
        });
        let project_picker_state = cx.new(|cx| {
            HunkPickerState::new(
                ProjectPickerDelegate::default(),
                None,
                "Find a project",
                window,
                cx,
            )
        });
        let workspace_target_picker_state = cx.new(|cx| {
            HunkPickerState::new(
                WorkspaceTargetPickerDelegate::default(),
                None,
                "Find a branch or project",
                window,
                cx,
            )
        });
        let review_left_picker_state = cx.new(|cx| {
            HunkPickerState::new(
                ReviewComparePickerDelegate::default(),
                None,
                "Find a branch or worktree",
                window,
                cx,
            )
        });
        let review_right_picker_state = cx.new(|cx| {
            HunkPickerState::new(
                ReviewComparePickerDelegate::default(),
                None,
                "Find a branch or worktree",
                window,
                cx,
            )
        });
        let branch_input_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("Create or activate branch"));
        let commit_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(4)
                .placeholder("Commit message")
        });
        let files_editor = Rc::new(RefCell::new(
            crate::app::native_files_editor::FilesEditor::new(),
        ));
        let repo_file_search_provider = Rc::new(RepoFileSearchProvider::new());
        let comment_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(3)
                .placeholder("Add comment for this diff row")
        });
        let ai_composer_file_completion_provider =
            Rc::new(AiComposerFileCompletionProvider::default());
        let ai_composer_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(4)
                .placeholder("Ask Codex anything, @ to add files, / for commands, $ for skills")
        });
        let ai_terminal_input_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("Run a command in this workspace"));
        let file_quick_open_input_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("Type a file name or path"));
        let editor_search_input_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("Find in file"));
        let editor_replace_input_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("Replace in file"));
        let in_app_menu_bar = (!cfg!(target_os = "macos")).then(|| AppMenuBar::new(cx));

        let mut view = Self {
            config_store,
            config,
            settings_draft: None,
            update_install_source,
            update_status,
            ready_update: None,
            update_check_task: Task::ready(()),
            update_apply_task: Task::ready(()),
            update_poll_task: Task::ready(()),
            state_store,
            state,
            database_store,
            window_handle: window.window_handle(),
            comments_cache: Vec::new(),
            comments_preview_open: false,
            comments_show_non_open: false,
            comment_miss_streaks: BTreeMap::new(),
            comment_row_matches: BTreeMap::new(),
            comment_open_row_counts: Vec::new(),
            hovered_comment_row: None,
            active_comment_editor_row: None,
            comment_input_state,
            comment_status_message: None,
            project_path: initial_project_path,
            repo_root: None,
            workspace_targets: Vec::new(),
            active_workspace_target_id: None,
            git_workspace: GitWorkspaceState::default(),
            review_compare_sources: Vec::new(),
            review_default_left_source_id: None,
            review_default_right_source_id: None,
            review_left_source_id: None,
            review_right_source_id: None,
            review_loaded_left_source_id: None,
            review_loaded_right_source_id: None,
            review_loaded_collapsed_files: BTreeSet::new(),
            branch_name: "unknown".to_string(),
            branch_has_upstream: false,
            branch_ahead_count: 0,
            branch_behind_count: 0,
            working_copy_commit_id: None,
            branches: Vec::new(),
            git_working_tree_scroll_handle: ScrollHandle::default(),
            recent_commits_scroll_handle: ScrollHandle::default(),
            workspace_view_mode: WorkspaceViewMode::GitWorkspace,
            ai_connection_state: AiConnectionState::Disconnected,
            ai_bootstrap_loading: false,
            ai_status_message: None,
            ai_error_message: None,
            ai_state_snapshot: hunk_codex::state::AiState::default(),
            ai_selected_thread_id: None,
            ai_new_thread_draft_active: false,
            ai_new_thread_start_mode: AiNewThreadStartMode::Local,
            ai_worktree_base_branch_name: None,
            ai_pending_new_thread_selection: false,
            ai_pending_thread_start: None,
            ai_pending_steers: Vec::new(),
            ai_queued_messages: Vec::new(),
            ai_interrupt_restore_queued_thread_ids: BTreeSet::new(),
            ai_scroll_timeline_to_bottom: false,
            ai_timeline_follow_output: true,
            ai_inline_review_selected_row_id_by_thread: BTreeMap::new(),
            ai_inline_review_mode_by_thread: BTreeMap::new(),
            ai_inline_review_session: None,
            ai_inline_review_loaded_state: None,
            ai_inline_review_error: None,
            ai_inline_review_status_message: None,
            ai_git_progress: None,
            ai_thread_title_refresh_state_by_thread: BTreeMap::new(),
            ai_expanded_thread_sidebar_project_roots: BTreeSet::new(),
            ai_visible_frame_state: None,
            ai_thread_sidebar_sections: Vec::new(),
            ai_thread_sidebar_rows: Vec::new(),
            ai_thread_sidebar_list_state: ListState::new(0, ListAlignment::Top, px(40.0)),
            ai_thread_sidebar_row_count: 0,
            ai_workspace_session: None,
            ai_workspace_surface_scroll_handle: ScrollHandle::default(),
            ai_workspace_surface_last_scroll_offset: None,
            ai_inline_review_surface: AiInlineReviewSurfaceState::new(),
            ai_hovered_workspace_block_id: None,
            ai_workspace_selection: None,
            ai_timeline_visible_turn_limit_by_thread: BTreeMap::new(),
            ai_timeline_turn_ids_by_thread: BTreeMap::new(),
            ai_timeline_row_ids_by_thread: BTreeMap::new(),
            ai_timeline_rows_by_id: BTreeMap::new(),
            ai_timeline_groups_by_id: BTreeMap::new(),
            ai_timeline_group_parent_by_child_row_id: BTreeMap::new(),
            ai_in_progress_turn_started_at: BTreeMap::new(),
            ai_composer_activity_elapsed_second: None,
            ai_expanded_timeline_row_ids: BTreeSet::new(),
            ai_pressed_markdown_link: None,
            ai_text_selection: None,
            ai_text_selection_drag_pointer: None,
            ai_text_selection_auto_scroll_task: Task::ready(()),
            ai_pending_approvals: Vec::new(),
            ai_pending_user_inputs: Vec::new(),
            ai_pending_user_input_answers: BTreeMap::new(),
            ai_account: None,
            ai_requires_openai_auth: false,
            ai_pending_chatgpt_login_id: None,
            ai_pending_chatgpt_auth_url: None,
            ai_rate_limits: None,
            ai_models: Vec::new(),
            ai_experimental_features: Vec::new(),
            ai_collaboration_modes: Vec::new(),
            ai_skills: Vec::new(),
            ai_skills_generation: 0,
            ai_include_hidden_models: initial_ai_include_hidden_models,
            ai_selected_model: preferred_ai_session.model,
            ai_selected_effort: preferred_ai_session.effort,
            ai_selected_collaboration_mode: AiCollaborationModeSelection::Default,
            ai_selected_service_tier: AiServiceTierSelection::Standard,
            ai_mad_max_mode: initial_ai_mad_max_mode,
            ai_followup_prompt_state_by_thread: BTreeMap::new(),
            ai_event_epoch: 0,
            ai_event_task: Task::ready(()),
            ai_thread_catalog_refresh_epoch: 0,
            ai_thread_catalog_task: Task::ready(()),
            ai_attachment_picker_task: Task::ready(()),
            ai_workspace_states: BTreeMap::new(),
            ai_desktop_notification_state_by_workspace: BTreeMap::new(),
            ai_pending_desktop_notification_events_by_workspace: BTreeMap::new(),
            #[cfg(target_os = "macos")]
            desktop_notification_permission_task: Task::ready(()),
            #[cfg(target_os = "macos")]
            macos_notification_permission_state:
                crate::app::desktop_notifications::MacOsNotificationPermissionState::Unknown,
            #[cfg(target_os = "macos")]
            macos_notification_permission_request_in_flight: false,
            ai_hidden_runtimes: BTreeMap::new(),
            ai_runtime_starting_workspace_key: None,
            ai_worker_thread: None,
            ai_command_tx: None,
            ai_worker_workspace_key: None,
            ai_draft_workspace_root_override: None,
            ai_draft_workspace_target_id: None,
            ai_terminal_states_by_thread: BTreeMap::new(),
            ai_hidden_terminal_runtimes: BTreeMap::new(),
            ai_terminal_open: false,
            ai_terminal_follow_output: true,
            ai_terminal_height_px: 220.0,
            ai_terminal_input_draft: String::new(),
            ai_terminal_session: AiTerminalSessionState::default(),
            ai_terminal_input_state,
            ai_terminal_focus_handle: cx.focus_handle(),
            ai_terminal_surface_focused: false,
            ai_terminal_cursor_blink_visible: true,
            ai_terminal_cursor_blink_active: false,
            ai_terminal_cursor_output_suppressed: false,
            ai_terminal_panel_bounds: None,
            ai_terminal_grid_size: None,
            ai_terminal_pending_input: None,
            ai_terminal_event_task: Task::ready(()),
            ai_terminal_cursor_blink_task: Task::ready(()),
            ai_terminal_cursor_output_task: Task::ready(()),
            ai_terminal_runtime: None,
            ai_terminal_cursor_blink_generation: 0,
            ai_terminal_cursor_output_generation: 0,
            ai_terminal_runtime_generation: 0,
            ai_terminal_stop_requested: false,
            workspace_project_states: BTreeMap::new(),
            files_terminal_states_by_project: BTreeMap::new(),
            files_hidden_terminal_runtimes: BTreeMap::new(),
            files_terminal_open: false,
            files_terminal_follow_output: true,
            files_terminal_height_px: 220.0,
            files_terminal_session: AiTerminalSessionState::default(),
            files_terminal_focus_handle: cx.focus_handle(),
            files_terminal_restore_target: FilesTerminalRestoreTarget::default(),
            files_terminal_surface_focused: false,
            files_terminal_cursor_blink_visible: true,
            files_terminal_cursor_blink_active: false,
            files_terminal_cursor_output_suppressed: false,
            files_terminal_panel_bounds: None,
            files_terminal_grid_size: None,
            files_terminal_pending_input: None,
            files_terminal_event_task: Task::ready(()),
            files_terminal_cursor_blink_task: Task::ready(()),
            files_terminal_cursor_output_task: Task::ready(()),
            files_terminal_runtime: None,
            files_terminal_cursor_blink_generation: 0,
            files_terminal_cursor_output_generation: 0,
            files_terminal_runtime_generation: 0,
            files_terminal_stop_requested: false,
            repo_file_search_provider,
            repo_file_search_reload_task: Task::ready(()),
            repo_file_search_loading: false,
            ai_composer_file_completion_provider,
            ai_composer_file_completion_reload_task: Task::ready(()),
            ai_composer_file_completion_menu: None,
            ai_composer_file_completion_selected_ix: 0,
            ai_composer_file_completion_dismissed_token: None,
            ai_composer_file_completion_scroll_handle: ScrollHandle::default(),
            ai_composer_slash_command_menu: None,
            ai_composer_slash_command_selected_ix: 0,
            ai_composer_slash_command_dismissed_token: None,
            ai_composer_slash_command_scroll_handle: ScrollHandle::default(),
            ai_composer_skill_completion_menu: None,
            ai_composer_skill_completion_selected_ix: 0,
            ai_composer_skill_completion_dismissed_token: None,
            ai_composer_skill_completion_scroll_handle: ScrollHandle::default(),
            ai_composer_completion_sync_key: None,
            ai_worktree_base_branch_picker_state,
            ai_composer_input_state,
            ai_review_mode_active: false,
            ai_review_mode_thread_ids: BTreeSet::new(),
            ai_usage_popover_open: false,
            ai_composer_drafts: BTreeMap::new(),
            ai_composer_status_by_draft: BTreeMap::new(),
            ai_composer_status_generation: 0,
            ai_composer_status_generation_by_key: BTreeMap::new(),
            available_project_open_targets:
                crate::app::project_open::resolve_available_project_open_targets(),
            files: Vec::new(),
            file_status_by_path: BTreeMap::new(),
            project_picker_state,
            workspace_target_picker_state,
            review_left_picker_state,
            review_right_picker_state,
            branch_picker_state,
            branch_input_state,
            branch_input_has_text: false,
            commit_input_state,
            git_action_epoch: 0,
            git_action_task: Task::ready(()),
            git_action_loading: false,
            git_action_label: None,
            workspace_target_switch_loading: false,
            git_status_message: None,
            git_workspace_refresh_epoch: 0,
            git_workspace_refresh_task: Task::ready(()),
            git_workspace_active_root: None,
            git_workspace_loading: false,
            pending_git_workspace_refresh: None,
            last_git_workspace_fingerprint: None,
            last_commit_subject: None,
            recent_commits: Vec::new(),
            recent_commits_error: None,
            collapsed_files: BTreeSet::new(),
            selected_path: None,
            selected_status: None,
            file_line_stats: BTreeMap::new(),
            review_surface: ReviewWorkspaceSurfaceState::new(),
            review_files: Vec::new(),
            review_file_status_by_path: BTreeMap::new(),
            review_file_line_stats: BTreeMap::new(),
            review_overall_line_stats: LineStats::default(),
            review_compare_loading: false,
            review_compare_error: None,
            review_workspace_session: None,
            review_loaded_reuse_token: None,
            overall_line_stats: LineStats::default(),
            refresh_epoch: 0,
            auto_refresh_unmodified_streak: 0,
            auto_refresh_task: Task::ready(()),
            repo_watch_task: Task::ready(()),
            repo_watch_refresh_epoch: 0,
            repo_watch_pending_refresh: None,
            repo_watch_pending_git_workspace_refresh: false,
            repo_watch_pending_recent_commits_refresh: false,
            repo_watch_refresh_task: Task::ready(()),
            snapshot_epoch: 0,
            snapshot_task: Task::ready(()),
            snapshot_loading: false,
            snapshot_active_request: None,
            workflow_loading: false,
            line_stats_epoch: 0,
            line_stats_task: Task::ready(()),
            line_stats_loading: false,
            pending_line_stats_refresh: None,
            pending_snapshot_refresh: None,
            recent_commits_epoch: 0,
            recent_commits_task: Task::ready(()),
            recent_commits_loading: false,
            recent_commits_active_request: None,
            pending_recent_commits_refresh: None,
            last_recent_commits_fingerprint: None,
            pending_dirty_paths: BTreeSet::new(),
            last_snapshot_fingerprint: None,
            open_project_task: Task::ready(()),
            patch_epoch: 0,
            patch_task: Task::ready(()),
            patch_loading: false,
            in_app_menu_bar,
            focus_handle: cx.focus_handle(),
            repo_tree_focus_handle: cx.focus_handle(),
            files_editor_focus_handle: cx.focus_handle(),
            drag_selecting_rows: false,
            scroll_selected_after_reload: true,
            last_scroll_activity_at: Instant::now(),
            segment_prefetch_epoch: 0,
            segment_prefetch_task: Task::ready(()),
            fps: 0.0,
            frame_sample_count: 0,
            frame_sample_started_at: Instant::now(),
            ignore_next_frame_sample: false,
            fps_epoch: 0,
            fps_task: Task::ready(()),
            ai_perf_metrics: RefCell::new(AiPerfMetrics::default()),
            repo_discovery_failed: false,
            error_message: None,
            files_sidebar_collapsed: false,
            review_sidebar_collapsed: false,
            ai_thread_sidebar_collapsed: false,
            repo_tree: RepoTreeState::new(),
            repo_tree_inline_edit: None,
            repo_tree_context_menu: None,
            workspace_text_context_menu: None,
            file_editor_tabs: Vec::new(),
            active_file_editor_tab_id: None,
            next_file_editor_tab_id: 1,
            file_editor_tab_scroll_handle: ScrollHandle::default(),
            files_editor,
            editor_search_input_state,
            editor_replace_input_state,
            file_quick_open_input_state,
            file_quick_open_visible: false,
            file_quick_open_matches: Vec::new(),
            file_quick_open_selected_ix: 0,
            editor_path: None,
            editor_loading: false,
            editor_error: None,
            editor_dirty: false,
            editor_last_saved_text: None,
            editor_epoch: 0,
            editor_task: Task::ready(()),
            editor_save_loading: false,
            editor_save_epoch: 0,
            editor_save_task: Task::ready(()),
            editor_markdown_preview_task: Task::ready(()),
            editor_markdown_preview_blocks: Vec::new(),
            editor_markdown_preview_loading: false,
            editor_markdown_preview_revision: 0,
            editor_markdown_preview: false,
            editor_search_visible: false,
        };

        let branch_input_state = view.branch_input_state.clone();
        cx.subscribe(&branch_input_state, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.branch_input_has_text =
                    !this.branch_input_state.read(cx).value().trim().is_empty();
                cx.notify();
            }
        })
        .detach();

        let ai_composer_state = view.ai_composer_input_state.clone();
        cx.subscribe(&ai_composer_state, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.sync_ai_visible_composer_prompt_to_draft(cx);
                this.ai_composer_file_completion_dismissed_token = None;
                this.ai_composer_slash_command_dismissed_token = None;
                this.ai_composer_skill_completion_dismissed_token = None;
                this.sync_ai_composer_completion_menus(cx);
            }
            if matches!(event, InputEvent::Blur) {
                this.ai_composer_file_completion_menu = None;
                this.ai_composer_slash_command_menu = None;
                this.ai_composer_skill_completion_menu = None;
                cx.notify();
            }
            if should_send_ai_prompt_from_input_event(event) {
                this.ai_send_prompt_action_from_keyboard(cx);
            }
        })
        .detach();

        let ai_terminal_focus_handle = view.ai_terminal_focus_handle.clone();
        cx.on_focus_in(&ai_terminal_focus_handle, window, |this, _, cx| {
            this.ai_terminal_surface_focus_in(cx);
        })
        .detach();
        cx.on_focus_out(&ai_terminal_focus_handle, window, |this, _, _, cx| {
            this.ai_terminal_surface_focus_out(cx);
        })
        .detach();

        let files_terminal_focus_handle = view.files_terminal_focus_handle.clone();
        cx.on_focus_in(&files_terminal_focus_handle, window, |this, _, cx| {
            this.files_terminal_surface_focus_in(cx);
        })
        .detach();
        cx.on_focus_out(&files_terminal_focus_handle, window, |this, _, _, cx| {
            this.files_terminal_surface_focus_out(cx);
        })
        .detach();

        let file_quick_open_state = view.file_quick_open_input_state.clone();
        cx.subscribe(&file_quick_open_state, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.sync_file_quick_open_matches(cx);
            }
        })
        .detach();

        let editor_search_state = view.editor_search_input_state.clone();
        cx.subscribe(&editor_search_state, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.sync_editor_search_query(cx);
            }
            if let InputEvent::PressEnter { secondary } = event {
                this.navigate_editor_search(!secondary, cx);
            }
        })
        .detach();

        let editor_replace_state = view.editor_replace_input_state.clone();
        cx.subscribe(&editor_replace_state, |this, _, event, cx| {
            if let InputEvent::PressEnter { secondary } = event {
                if *secondary {
                    this.replace_all_editor_search_matches(cx);
                } else {
                    this.replace_current_editor_search_match(None, cx);
                }
            }
        })
        .detach();

        let weak_view = cx.entity().downgrade();
        // The multiline input consumes Tab for indentation before view-level keybindings run.
        // Intercept the keystroke at the app layer so the AI composer can queue prompts reliably.
        cx.intercept_keystrokes(move |event, window, cx| {
            let Some(view) = weak_view.upgrade() else {
                return;
            };
            if let Some(action) = hunk_picker_action_for_keystroke(&event.keystroke) {
                let handled = view.update(cx, |this, cx| {
                    this.handle_hunk_picker_keystroke(action, window, cx)
                });
                if handled {
                    return;
                }
            }
            if let Some(action) = file_quick_open_action_for_keystroke(&event.keystroke) {
                let handled = view.update(cx, |this, cx| {
                    this.handle_file_quick_open_keystroke(action, window, cx)
                });
                if handled {
                    return;
                }
            }
            if let Some(action) = ai_composer_completion_action_for_keystroke(&event.keystroke) {
                let handled = view.update(cx, |this, cx| {
                    this.ai_handle_composer_completion_keystroke(action, window, cx)
                });
                if handled {
                    return;
                }
            }
            if event.keystroke.key == "tab" && event.keystroke.modifiers.shift
                && !event.keystroke.modifiers.control
                && !event.keystroke.modifiers.alt && !event.keystroke.modifiers.platform
            {
                let handled = view.update(cx, |this, cx| {
                    let composer_focus_handle =
                        gpui::Focusable::focus_handle(this.ai_composer_input_state.read(cx), cx);
                    if !composer_focus_handle.is_focused(window) {
                        return false;
                    }
                    this.ai_cycle_composer_mode(window, cx);
                    true
                });
                if handled { return; }
            }
            if let Some(action) = ai_followup_prompt_action_for_keystroke(&event.keystroke) {
                let handled = view.update(cx, |this, cx| {
                    this.ai_handle_followup_prompt_keystroke(action, window, cx)
                });
                if handled { return; }
            }
            let Some(shortcut) = ai_composer_shortcut_for_keystroke(&event.keystroke) else {
                return;
            };
            view.update(cx, |this, cx| {
                this.ai_handle_composer_shortcut_keystroke(shortcut, window, cx);
            });
        })
        .detach();

        let branch_picker_state = view.branch_picker_state.clone();
        cx.subscribe(
            &branch_picker_state,
            |this, _, event: &HunkPickerEvent<BranchPickerDelegate>, cx| {
                let HunkPickerEvent::Confirm(branch_name) = event;
                let Some(branch_name) = branch_name.clone() else {
                    return;
                };
                if this.checked_out_branch_name() == Some(branch_name.as_str()) {
                    return;
                }
                this.checkout_branch(branch_name, cx);
            },
        )
        .detach();

        let ai_worktree_base_branch_picker_state =
            view.ai_worktree_base_branch_picker_state.clone();
        cx.subscribe(
            &ai_worktree_base_branch_picker_state,
            |this, _, event: &HunkPickerEvent<BranchPickerDelegate>, cx| {
                let HunkPickerEvent::Confirm(branch_name) = event;
                let Some(branch_name) = branch_name.clone() else {
                    return;
                };
                this.ai_select_worktree_base_branch(branch_name, cx);
            },
        )
        .detach();

        let project_picker_state = view.project_picker_state.clone();
        cx.subscribe(
            &project_picker_state,
            |this, _, event: &HunkPickerEvent<ProjectPickerDelegate>, cx| {
                let HunkPickerEvent::Confirm(project_path) = event;
                let Some(project_path) = project_path.clone() else {
                    return;
                };
                let project_path = PathBuf::from(project_path);
                if this.project_path.as_ref() == Some(&project_path) {
                    return;
                }
                this.activate_workspace_project_root(project_path, cx);
            },
        )
        .detach();

        let workspace_target_picker_state = view.workspace_target_picker_state.clone();
        cx.subscribe(
            &workspace_target_picker_state,
            |this, _, event: &HunkPickerEvent<WorkspaceTargetPickerDelegate>, cx| {
                let HunkPickerEvent::Confirm(target_id) = event;
                let Some(target_id) = target_id.clone() else {
                    return;
                };
                if this.active_workspace_target_id.as_deref() == Some(target_id.as_str()) {
                    return;
                }
                this.activate_workspace_target(target_id, cx);
            },
        )
        .detach();

        view.subscribe_review_compare_picker_states(cx);

        view.update_branch_picker_state(window, cx);
        view.update_ai_worktree_base_branch_picker_state(window, cx);
        view.update_project_picker_state(window, cx);
        view.update_workspace_target_picker_state(window, cx);
        view.update_review_compare_picker_states(window, cx);
        view.apply_theme_preference(window, cx);
        cx.observe_window_appearance(window, |this, window, cx| {
            this.sync_theme_with_system_if_needed(window, cx);
        })
        .detach();

        view.hydrate_workflow_cache_if_available(cx);
        view.hydrate_recent_commits_cache_if_available(cx);
        view.restore_active_workspace_target_root_from_state(cx);
        view.request_snapshot_refresh(cx);
        view.request_recent_commits_refresh(false, cx);
        view.prewarm_preview_highlighting(cx);
        view.preload_ai_runtime_on_startup(cx);
        view.start_auto_refresh(cx);
        view.start_repo_watch(cx);
        view.start_fps_monitor(cx);
        view.rebuild_ai_thread_sidebar_state();
        view.prune_expired_comments();
        view.refresh_comments_cache_from_store();
        view.maybe_schedule_startup_update_check(cx);
        view.restart_periodic_update_checks(cx);
        view.refresh_macos_notification_permission_status(cx);
        if view.workspace_view_mode == WorkspaceViewMode::Ai {
            view.maybe_prepare_ai_desktop_notifications(cx);
        }
        view
    }

    fn prewarm_preview_highlighting(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |_, cx| {
            let elapsed = cx.background_executor().spawn(async move {
                let started_at = Instant::now();
                let _ = hunk_language::preview_highlight_spans_for_language_hint(
                    Some("rust"),
                    "fn warm_preview_highlight_registry() {}\n",
                );
                started_at.elapsed()
            });
            let elapsed = elapsed.await;
            debug!(
                "prewarmed preview highlighting registry in {}ms",
                elapsed.as_millis()
            );
        })
        .detach();
    }
}
