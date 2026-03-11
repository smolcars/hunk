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
                error!("failed to load app state from {}: {err:#}", store.path().display());
                (Some(store), AppState::default())
            }
        }
    }

    fn load_legacy_last_project_path(config_store: &ConfigStore) -> Option<PathBuf> {
        let raw = std::fs::read_to_string(config_store.path()).ok()?;
        let value = raw.parse::<toml::Value>().ok()?;
        value
            .get("last_project_path")
            .and_then(toml::Value::as_str)
            .map(PathBuf::from)
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

    fn set_last_project_path(&mut self, project_path: Option<PathBuf>) {
        if self.state.last_project_path == project_path {
            return;
        }

        self.state.last_project_path = project_path;
        self.persist_state();
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
        let Some(cache) = self.state.git_workflow_cache.clone() else {
            return;
        };
        let Some(root) = cache.root.clone() else {
            return;
        };
        let cached_project_root =
            hunk_git::worktree::primary_repo_root(root.as_path()).unwrap_or_else(|_| root.clone());
        let Some(expected_root) = self
            .project_path
            .clone()
            .or_else(|| self.state.last_project_path.clone())
        else {
            return;
        };
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
                    untracked: file.untracked,
                })
                .collect(),
            last_commit_subject: self.last_commit_subject.clone(),
            cached_unix_time: 0,
        };

        if let Some(previous) = self.state.git_workflow_cache.as_ref() {
            let mut previous_without_time = previous.clone();
            previous_without_time.cached_unix_time = 0;
            if previous_without_time == cache {
                return;
            }
        }

        cache.cached_unix_time = Self::workflow_cache_unix_time();
        self.state.git_workflow_cache = Some(cache);
        self.persist_state();
    }

    fn sync_theme_with_system_if_needed(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.config.theme != ThemePreference::System {
            return;
        }
        self.apply_theme_preference(window, cx);
    }

    pub(super) fn set_theme_preference(
        &mut self,
        theme: ThemePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.config.theme == theme {
            return;
        }

        self.config.theme = theme;
        self.apply_theme_preference(window, cx);
        self.persist_config();
        cx.notify();
    }

    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (config_store, config) = Self::load_app_config();
        let (state_store, mut state) = Self::load_app_state();
        let database_store = Self::load_database_store();
        if state.last_project_path.is_none()
            && let Some(config_store) = config_store.as_ref()
            && let Some(last_project_path) = Self::load_legacy_last_project_path(config_store)
        {
            state.last_project_path = Some(last_project_path);
            if let Some(state_store) = state_store.as_ref()
                && let Err(err) = state_store.save(&state)
            {
                error!(
                    "failed to migrate app state to {}: {err:#}",
                    state_store.path().display()
                );
            }
        }
        let last_project_path = state.last_project_path.clone();
        let initial_ai_workspace_key = last_project_path
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
        let diff_show_whitespace = config.show_whitespace;
        let diff_show_eol_markers = config.show_eol_markers;
        let branch_picker_state = cx.new(|cx| {
            SelectState::new(BranchPickerDelegate::default(), None, window, cx).searchable(true)
        });
        let ai_worktree_base_branch_picker_state = cx.new(|cx| {
            SelectState::new(BranchPickerDelegate::default(), None, window, cx).searchable(true)
        });
        let workspace_target_picker_state = cx.new(|cx| {
            SelectState::new(WorkspaceTargetPickerDelegate::default(), None, window, cx)
                .searchable(true)
        });
        let review_left_picker_state = cx.new(|cx| {
            SelectState::new(ReviewComparePickerDelegate::default(), None, window, cx)
                .searchable(true)
        });
        let review_right_picker_state = cx.new(|cx| {
            SelectState::new(ReviewComparePickerDelegate::default(), None, window, cx)
                .searchable(true)
        });
        let branch_input_state = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Create or activate branch")
        });
        let commit_input_state = cx
            .new(|cx| InputState::new(window, cx).multi_line(true).rows(4).placeholder("Commit message"));
        let editor_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor("text")
                .line_number(true)
                .soft_wrap(false)
                .placeholder("Select a file from Files tree to edit it.")
        });
        let comment_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(3)
                .placeholder("Add comment for this diff row")
        });
        let ai_composer_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(4)
                .placeholder("Ask for follow-up changes")
        });
        let in_app_menu_bar = (!cfg!(target_os = "macos")).then(|| AppMenuBar::new(cx));

        let mut view = Self {
            config_store,
            config,
            settings_draft: None,
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
            project_path: last_project_path,
            repo_root: None,
            workspace_targets: Vec::new(),
            active_workspace_target_id: None,
            git_workspace: GitWorkspaceState::default(),
            review_compare_sources: Vec::new(),
            review_default_left_source_id: None,
            review_default_right_source_id: None,
            review_left_source_id: None,
            review_right_source_id: None,
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
            ai_scroll_timeline_to_bottom: false,
            ai_timeline_follow_output: true,
            ai_thread_list_scroll_handle: ScrollHandle::default(),
            ai_git_progress: None,
            ai_thread_title_refresh_state_by_thread: BTreeMap::new(),
            ai_timeline_list_state: ListState::new(0, ListAlignment::Top, px(360.0)),
            ai_timeline_list_row_count: 0,
            ai_timeline_visible_turn_limit_by_thread: BTreeMap::new(),
            ai_timeline_turn_ids_by_thread: BTreeMap::new(),
            ai_timeline_row_ids_by_thread: BTreeMap::new(),
            ai_timeline_rows_by_id: BTreeMap::new(),
            ai_timeline_groups_by_id: BTreeMap::new(),
            ai_timeline_group_parent_by_child_row_id: BTreeMap::new(),
            ai_in_progress_turn_started_at: BTreeMap::new(),
            ai_composer_activity_elapsed_second: None,
            ai_expanded_timeline_row_ids: BTreeSet::new(),
            ai_text_selection: None,
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
            ai_include_hidden_models: initial_ai_include_hidden_models,
            ai_selected_model: None,
            ai_selected_effort: None,
            ai_selected_collaboration_mode: AiCollaborationModeSelection::Default,
            ai_selected_service_tier: AiServiceTierSelection::Standard,
            ai_mad_max_mode: initial_ai_mad_max_mode,
            ai_event_epoch: 0,
            ai_event_task: Task::ready(()),
            ai_thread_catalog_refresh_epoch: 0,
            ai_thread_catalog_task: Task::ready(()),
            ai_attachment_picker_task: Task::ready(()),
            ai_workspace_states: BTreeMap::new(),
            ai_hidden_runtimes: BTreeMap::new(),
            ai_worker_thread: None,
            ai_command_tx: None,
            ai_worker_workspace_key: None,
            ai_draft_workspace_target_id: None,
            ai_worktree_base_branch_picker_state,
            ai_composer_input_state,
            ai_composer_drafts: BTreeMap::new(),
            ai_composer_status_by_draft: BTreeMap::new(),
            files: Vec::new(),
            file_status_by_path: BTreeMap::new(),
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
            staged_commit_files: BTreeSet::new(),
            last_commit_subject: None,
            recent_commits: Vec::new(),
            recent_commits_error: None,
            collapsed_files: BTreeSet::new(),
            selected_path: None,
            selected_status: None,
            diff_rows: Vec::new(),
            diff_row_metadata: Vec::new(),
            diff_row_segment_cache: Vec::new(),
            diff_visible_file_header_lookup: Vec::new(),
            diff_visible_hunk_header_lookup: Vec::new(),
            file_row_ranges: Vec::new(),
            file_line_stats: BTreeMap::new(),
            diff_list_state: ListState::new(0, ListAlignment::Top, px(360.0)),
            diff_show_whitespace,
            diff_show_eol_markers,
            diff_left_line_number_width: line_number_column_width(DIFF_LINE_NUMBER_MIN_DIGITS),
            diff_right_line_number_width: line_number_column_width(DIFF_LINE_NUMBER_MIN_DIGITS),
            review_files: Vec::new(),
            review_file_status_by_path: BTreeMap::new(),
            review_file_line_stats: BTreeMap::new(),
            review_overall_line_stats: LineStats::default(),
            review_compare_loading: false,
            review_compare_error: None,
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
            selection_anchor_row: None,
            selection_head_row: None,
            drag_selecting_rows: false,
            scroll_selected_after_reload: true,
            last_visible_row_start: None,
            last_diff_scroll_offset: None,
            last_scroll_activity_at: Instant::now(),
            segment_prefetch_anchor_row: None,
            segment_prefetch_epoch: 0,
            segment_prefetch_task: Task::ready(()),
            fps: 0.0,
            frame_sample_count: 0,
            frame_sample_started_at: Instant::now(),
            fps_epoch: 0,
            fps_task: Task::ready(()),
            repo_discovery_failed: false,
            error_message: None,
            sidebar_collapsed: false,
            repo_tree: RepoTreeState::new(),
            repo_tree_inline_edit: None,
            repo_tree_context_menu: None,
            editor_input_state,
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
        };

        let editor_state = view.editor_input_state.clone();
        cx.observe(&editor_state, |this, _, cx| {
            this.sync_editor_dirty_from_input(cx);
        })
        .detach();

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
            }
            if should_send_ai_prompt_from_input_event(event) {
                this.ai_send_prompt_action_from_keyboard(cx);
            }
        })
        .detach();

        let branch_picker_state = view.branch_picker_state.clone();
        cx.subscribe(
            &branch_picker_state,
            |this, _, event: &SelectEvent<BranchPickerDelegate>, cx| {
                let SelectEvent::Confirm(branch_name) = event;
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

        let ai_worktree_base_branch_picker_state = view.ai_worktree_base_branch_picker_state.clone();
        cx.subscribe(
            &ai_worktree_base_branch_picker_state,
            |this, _, event: &SelectEvent<BranchPickerDelegate>, cx| {
                let SelectEvent::Confirm(branch_name) = event;
                let Some(branch_name) = branch_name.clone() else {
                    return;
                };
                this.ai_select_worktree_base_branch(branch_name, cx);
            },
        )
        .detach();

        let workspace_target_picker_state = view.workspace_target_picker_state.clone();
        cx.subscribe(
            &workspace_target_picker_state,
            |this, _, event: &SelectEvent<WorkspaceTargetPickerDelegate>, cx| {
                let SelectEvent::Confirm(target_id) = event;
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

        view.install_list_scroll_handlers(cx);
        view.subscribe_review_compare_picker_states(cx);

        view.update_branch_picker_state(window, cx);
        view.update_ai_worktree_base_branch_picker_state(window, cx);
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
        view.preload_ai_runtime_on_startup(cx);
        view.start_auto_refresh(cx);
        view.start_repo_watch(cx);
        view.start_fps_monitor(cx);
        view.prune_expired_comments();
        view.refresh_comments_cache_from_store();
        view
    }

    fn install_list_scroll_handlers(&self, cx: &mut Context<Self>) {
        let weak_view = cx.entity().downgrade();

        self.diff_list_state.set_scroll_handler({
            let weak_view = weak_view.clone();
            move |event, _, cx| {
                let visible_row = event.visible_range.start;
                let _ = weak_view.update(cx, |this, cx| {
                    this.sync_selected_file_from_visible_row(visible_row, cx);
                });
            }
        });

        self.ai_timeline_list_state.set_scroll_handler({
            let weak_view = weak_view.clone();
            move |_, _, cx| {
                let weak_view = weak_view.clone();
                // GPUI invokes the scroll handler while the list state is mutably borrowed.
                // Defer follow-output recomputation until that borrow is released.
                cx.defer(move |cx| {
                    let Some(view) = weak_view.upgrade() else {
                        return;
                    };
                    view.update(cx, |this, cx| {
                        let previous_follow_output = this.ai_timeline_follow_output;
                        this.refresh_ai_timeline_follow_output_from_scroll();
                        if this.ai_timeline_follow_output != previous_follow_output {
                            cx.notify();
                        }
                    });
                });
            }
        });
    }
}
