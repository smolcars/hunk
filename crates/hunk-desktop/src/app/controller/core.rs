impl DiffViewer {
    const AUTO_REFRESH_MAX_INTERVAL_MS: u64 = 60_000;
    const AUTO_REFRESH_QUICK_PROBE_MS: u64 = 3_000;
    const AUTO_REFRESH_BACKOFF_STEPS: u32 = 6;
    const REPO_WATCH_DEBOUNCE: Duration = Duration::from_millis(150);

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
        let Some(expected_root) = self
            .project_path
            .clone()
            .or_else(|| self.state.last_project_path.clone())
        else {
            return;
        };
        if root != expected_root {
            return;
        }

        self.project_path = Some(root.clone());
        self.repo_root = Some(root.clone());
        self.ai_sync_workspace_preferences(cx);
        self.branch_name = if cache.branch_name.is_empty() {
            "unknown".to_string()
        } else {
            cache.branch_name
        };
        self.branch_has_upstream = cache.branch_has_upstream;
        self.branch_ahead_count = cache.branch_ahead_count;
        self.can_undo_operation = cache.can_undo_operation;
        self.can_redo_operation = cache.can_redo_operation;
        self.branches = cache
            .branches
            .into_iter()
            .map(|branch| LocalBranch {
                name: branch.name,
                is_current: branch.is_current,
                tip_unix_time: branch.tip_unix_time,
            })
            .collect();
        self.bookmark_revisions = cache
            .bookmark_revisions
            .into_iter()
            .map(|revision| BookmarkRevision {
                id: revision.id,
                subject: revision.subject,
                unix_time: revision.unix_time,
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
        self.repo_discovery_failed = false;
        self.error_message = None;
        info!(
            "hydrated git workflow cache for {} (files={} branches={} revisions={})",
            root.display(),
            self.files.len(),
            self.branches.len(),
            self.bookmark_revisions.len()
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
            can_undo_operation: self.can_undo_operation,
            can_redo_operation: self.can_redo_operation,
            branches: self
                .branches
                .iter()
                .map(|branch| CachedLocalBranchState {
                    name: branch.name.clone(),
                    is_current: branch.is_current,
                    tip_unix_time: branch.tip_unix_time,
                })
                .collect(),
            bookmark_revisions: self
                .bookmark_revisions
                .iter()
                .map(|revision| CachedBookmarkRevisionState {
                    id: revision.id.clone(),
                    subject: revision.subject.clone(),
                    unix_time: revision.unix_time,
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
        let branch_input_state = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Select or create bookmark")
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
            branch_name: "unknown".to_string(),
            branch_has_upstream: false,
            branch_ahead_count: 0,
            working_copy_commit_id: None,
            can_undo_operation: false,
            can_redo_operation: false,
            branches: Vec::new(),
            bookmark_revisions: Vec::new(),
            jj_workspace_scroll_handle: ScrollHandle::default(),
            pending_bookmark_switch: None,
            show_jj_terms_glossary: false,
            workspace_view_mode: WorkspaceViewMode::JjWorkspace,
            ai_connection_state: AiConnectionState::Disconnected,
            ai_bootstrap_loading: false,
            ai_status_message: None,
            ai_error_message: None,
            ai_state_snapshot: hunk_codex::state::AiState::default(),
            ai_selected_thread_id: None,
            ai_scroll_timeline_to_bottom: false,
            ai_timeline_follow_output: true,
            ai_thread_list_scroll_handle: ScrollHandle::default(),
            ai_thread_inline_toast: None,
            ai_thread_inline_toast_epoch: 0,
            ai_thread_inline_toast_task: Task::ready(()),
            ai_timeline_list_state: ListState::new(0, ListAlignment::Top, px(360.0)),
            ai_timeline_list_row_count: 0,
            ai_timeline_visible_turn_limit_by_thread: BTreeMap::new(),
            ai_timeline_turn_ids_by_thread: BTreeMap::new(),
            ai_timeline_row_ids_by_thread: BTreeMap::new(),
            ai_timeline_rows_by_id: BTreeMap::new(),
            ai_in_progress_turn_started_at: BTreeMap::new(),
            ai_composer_activity_elapsed_second: None,
            ai_expanded_timeline_row_ids: BTreeSet::new(),
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
            ai_selected_collaboration_mode: None,
            ai_selected_service_tier: AiServiceTierSelection::Standard,
            ai_mad_max_mode: initial_ai_mad_max_mode,
            ai_event_epoch: 0,
            ai_event_task: Task::ready(()),
            ai_attachment_picker_task: Task::ready(()),
            ai_worker_thread: None,
            ai_command_tx: None,
            ai_composer_input_state,
            ai_composer_local_images: Vec::new(),
            files: Vec::new(),
            file_status_by_path: BTreeMap::new(),
            revision_stack_collapsed: true,
            branch_input_state,
            commit_input_state,
            commit_excluded_files: BTreeSet::new(),
            last_commit_subject: None,
            git_action_epoch: 0,
            git_action_task: Task::ready(()),
            git_action_loading: false,
            git_action_label: None,
            git_status_message: None,
            working_copy_recovery_candidates: Vec::new(),
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
            overall_line_stats: LineStats::default(),
            refresh_epoch: 0,
            auto_refresh_unmodified_streak: 0,
            auto_refresh_task: Task::ready(()),
            repo_watch_task: Task::ready(()),
            repo_watch_refresh_epoch: 0,
            repo_watch_refresh_task: Task::ready(()),
            snapshot_epoch: 0,
            snapshot_task: Task::ready(()),
            snapshot_loading: false,
            snapshot_active_request: None,
            workflow_loading: false,
            line_stats_epoch: 0,
            line_stats_task: Task::ready(()),
            line_stats_loading: false,
            pending_snapshot_refresh: None,
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

        let ai_composer_state = view.ai_composer_input_state.clone();
        cx.subscribe(&ai_composer_state, |this, _, event, cx| {
            if should_send_ai_prompt_from_input_event(event) {
                this.ai_send_prompt_action_from_keyboard(cx);
            }
        })
        .detach();

        view.apply_theme_preference(window, cx);
        cx.observe_window_appearance(window, |this, window, cx| {
            this.sync_theme_with_system_if_needed(window, cx);
        })
        .detach();

        view.hydrate_workflow_cache_if_available(cx);
        view.request_snapshot_refresh(cx);
        view.start_auto_refresh(cx);
        view.start_repo_watch(cx);
        view.start_fps_monitor(cx);
        view.prune_expired_comments();
        view.refresh_comments_cache_from_store();
        view
    }

    pub(super) fn open_project_action(
        &mut self,
        _: &OpenProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_project_picker(cx);
    }

    pub(super) fn select_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.selected_path = Some(path.clone());
        self.selected_status = self.status_for_path(path.as_str());
        self.scroll_to_file_start(&path);
        self.last_visible_row_start = None;
        self.last_diff_scroll_offset = None;
        self.last_scroll_activity_at = Instant::now();
        cx.notify();
    }

    pub(super) fn status_for_path(&self, path: &str) -> Option<FileStatus> {
        self.file_status_by_path.get(path).copied()
    }

    pub(super) fn request_snapshot_refresh(&mut self, cx: &mut Context<Self>) {
        self.request_snapshot_refresh_internal(SnapshotRefreshRequest::user(false), cx);
    }

    pub(super) fn request_snapshot_refresh_workflow_only(
        &mut self,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let request = if force {
            SnapshotRefreshRequest::user(true)
        } else {
            SnapshotRefreshRequest::background()
        };
        self.request_snapshot_refresh_internal(request, cx);
    }

    fn enqueue_snapshot_refresh(&mut self, request: SnapshotRefreshRequest) {
        self.pending_snapshot_refresh = Some(
            self.pending_snapshot_refresh
                .map_or(request, |pending| pending.merge(request)),
        );
    }

    fn active_snapshot_refresh_request(&self) -> SnapshotRefreshRequest {
        self.snapshot_active_request
            .unwrap_or(SnapshotRefreshRequest::background())
    }

    fn should_preempt_active_snapshot_refresh(&self, request: SnapshotRefreshRequest) -> bool {
        self.snapshot_loading && request.is_more_urgent_than(self.active_snapshot_refresh_request())
    }

    fn finish_snapshot_refresh_loading(&mut self) {
        self.snapshot_loading = false;
        self.snapshot_active_request = None;
    }

    fn schedule_line_stats_refresh(
        &mut self,
        repo_root: PathBuf,
        request: SnapshotRefreshRequest,
        scope: LineStatsRefreshScope,
        snapshot_epoch: usize,
        cold_start: bool,
        cx: &mut Context<Self>,
    ) {
        let epoch = self.next_line_stats_epoch();
        let refresh_root = repo_root.display().to_string();
        let scope_label = scope.label();
        let path_count = scope.path_count();
        let scope_for_load = scope.clone();
        self.line_stats_loading = true;
        info!(
            "git workspace line stats refresh start: epoch={} snapshot_epoch={} force={} priority={} scope={} path_count={} cold_start={} root={}",
            epoch,
            snapshot_epoch,
            request.force,
            request.priority.as_str(),
            scope_label,
            path_count,
            cold_start,
            refresh_root
        );

        self.line_stats_task = cx.spawn(async move |this, cx| {
            let started_at = Instant::now();
            let result = cx
                .background_executor()
                .spawn(async move {
                    match &scope_for_load {
                        LineStatsRefreshScope::Full => {
                            load_repo_file_line_stats_without_refresh(&repo_root)
                        }
                        LineStatsRefreshScope::Paths(paths) => {
                            load_repo_file_line_stats_for_paths_without_refresh(&repo_root, paths)
                        }
                    }
                })
                .await;

            match &result {
                Ok(file_line_stats) => {
                    let line_stats = Self::sum_line_stats(file_line_stats.values().copied());
                    info!(
                        "git workspace line stats ready: epoch={} snapshot_epoch={} force={} priority={} scope={} path_count={} elapsed_ms={} files={} added={} removed={} changed={} cold_start={}",
                        epoch,
                        snapshot_epoch,
                        request.force,
                        request.priority.as_str(),
                        scope_label,
                        path_count,
                        started_at.elapsed().as_millis(),
                        file_line_stats.len(),
                        line_stats.added,
                        line_stats.removed,
                        line_stats.changed(),
                        cold_start
                    );
                }
                Err(err) => {
                    error!(
                        "git workspace line stats load failed: epoch={} snapshot_epoch={} force={} priority={} scope={} path_count={} elapsed_ms={} cold_start={} err={err:#}",
                        epoch,
                        snapshot_epoch,
                        request.force,
                        request.priority.as_str(),
                        scope_label,
                        path_count,
                        started_at.elapsed().as_millis(),
                        cold_start
                    );
                }
            }

            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.line_stats_epoch {
                        return;
                    }

                    this.line_stats_loading = false;
                    if let Ok(file_line_stats) = result {
                        match scope {
                            LineStatsRefreshScope::Full => {
                                this.file_line_stats = file_line_stats;
                            }
                            LineStatsRefreshScope::Paths(paths) => {
                                for path in paths {
                                    this.file_line_stats.remove(path.as_str());
                                }
                                this.file_line_stats.extend(file_line_stats);
                            }
                        }
                        this.recompute_overall_line_stats_from_file_stats();
                    }
                    cx.notify();
                });
            }
        });
    }

    fn take_line_stats_refresh_scope(
        &mut self,
        request: SnapshotRefreshRequest,
    ) -> Option<LineStatsRefreshScope> {
        if self.files.is_empty() {
            self.pending_dirty_paths.clear();
            return None;
        }

        if request.priority == SnapshotRefreshPriority::Background {
            let pending_dirty_paths = std::mem::take(&mut self.pending_dirty_paths);
            if !pending_dirty_paths.is_empty() {
                let dirty_paths = self
                    .files
                    .iter()
                    .filter(|file| {
                        pending_dirty_paths.iter().any(|dirty_path| {
                            file.path == *dirty_path
                                || file
                                    .path
                                    .strip_prefix(dirty_path.as_str())
                                    .is_some_and(|suffix| suffix.starts_with('/'))
                        })
                    })
                    .map(|file| file.path.clone())
                    .collect::<BTreeSet<_>>();
                if !dirty_paths.is_empty() {
                    return Some(LineStatsRefreshScope::Paths(dirty_paths));
                }
                return None;
            }
        } else {
            self.pending_dirty_paths.clear();
        }

        Some(LineStatsRefreshScope::Full)
    }

    fn sum_line_stats<I>(stats: I) -> LineStats
    where
        I: IntoIterator<Item = LineStats>,
    {
        let mut total = LineStats::default();
        for line_stats in stats {
            total.added = total.added.saturating_add(line_stats.added);
            total.removed = total.removed.saturating_add(line_stats.removed);
        }
        total
    }

    fn recompute_overall_line_stats_from_file_stats(&mut self) {
        self.overall_line_stats = Self::sum_line_stats(
            self.files
                .iter()
                .filter_map(|file| self.file_line_stats.get(file.path.as_str()).copied()),
        );
    }

    fn maybe_run_pending_snapshot_refresh(&mut self, cx: &mut Context<Self>) {
        if self.snapshot_loading {
            return;
        }
        let Some(request) = self.pending_snapshot_refresh.take() else {
            return;
        };
        info!(
            "git workspace running queued refresh: force={} priority={}",
            request.force,
            request.priority.as_str()
        );
        self.request_snapshot_refresh_internal(request, cx);
    }

    pub(super) fn request_snapshot_refresh_internal(
        &mut self,
        request: SnapshotRefreshRequest,
        cx: &mut Context<Self>,
    ) {
        self.request_snapshot_refresh_with_scope(request, cx);
    }

    fn request_snapshot_refresh_with_scope(
        &mut self,
        request: SnapshotRefreshRequest,
        cx: &mut Context<Self>,
    ) {
        let request = self
            .pending_snapshot_refresh
            .take()
            .map_or(request, |pending| request.merge(pending));

        if self.snapshot_loading {
            if self.should_preempt_active_snapshot_refresh(request) {
                let active = self.active_snapshot_refresh_request();
                info!(
                    "git workspace refresh preempted: epoch={} active_priority={} next_priority={} force={}",
                    self.snapshot_epoch,
                    active.priority.as_str(),
                    request.priority.as_str(),
                    request.force
                );
                self.snapshot_task = Task::ready(());
                self.snapshot_active_request = None;
            } else {
                self.enqueue_snapshot_refresh(request);
                tracing::debug!(
                    "git workspace refresh deferred: queued refresh while epoch={} is still loading (force={} priority={})",
                    self.snapshot_epoch,
                    request.force,
                    request.priority.as_str()
                );
                return;
            }
        }
        if request.force {
            self.auto_refresh_unmodified_streak = 0;
        }
        let cold_start = self.last_snapshot_fingerprint.is_none();

        enum SnapshotRefreshStageA {
            Unchanged(RepoSnapshotFingerprint),
            Loaded {
                fingerprint: RepoSnapshotFingerprint,
                workflow: Box<WorkflowSnapshot>,
                loaded_without_refresh: bool,
            },
        }

        let source_dir_result = self
            .project_path
            .clone()
            .map(Ok)
            .unwrap_or_else(|| std::env::current_dir().context("failed to resolve current directory"));
        let previous_fingerprint = if request.force {
            None
        } else {
            self.last_snapshot_fingerprint.clone()
        };
        let prefer_stale_first = cold_start && !request.force;
        let epoch = self.next_snapshot_epoch();
        self.snapshot_loading = true;
        self.snapshot_active_request = Some(request);
        self.workflow_loading = true;
        let refresh_root = self
            .project_path
            .clone()
            .or_else(|| self.repo_root.clone())
            .unwrap_or_else(|| PathBuf::from("."));
        info!(
            "git workspace refresh start: epoch={} force={} priority={} cold_start={} root={}",
            epoch,
            request.force,
            request.priority.as_str(),
            cold_start,
            refresh_root.display()
        );
        cx.notify();

        self.snapshot_task = cx.spawn(async move |this, cx| {
            let started_at = Instant::now();
            // Stage A: resolve workspace state first so right-pane workflow data can paint early.
            let stage_a_result = match source_dir_result {
                Ok(source_dir) => {
                    cx.background_executor()
                        .spawn(async move {
                            let load_once = || -> Result<SnapshotRefreshStageA> {
                                if prefer_stale_first {
                                    let (fingerprint, workflow) =
                                        load_workflow_snapshot_with_fingerprint_without_refresh(
                                            &source_dir,
                                        )?;
                                    return Ok(SnapshotRefreshStageA::Loaded {
                                        fingerprint,
                                        workflow: Box::new(workflow),
                                        loaded_without_refresh: true,
                                    });
                                }

                                let (fingerprint, workflow) = load_workflow_snapshot_if_changed(
                                    &source_dir,
                                    previous_fingerprint.as_ref(),
                                )?;
                                match workflow {
                                    Some(workflow) => Ok(SnapshotRefreshStageA::Loaded {
                                        fingerprint,
                                        workflow: Box::new(workflow),
                                        loaded_without_refresh: false,
                                    }),
                                    None => Ok(SnapshotRefreshStageA::Unchanged(fingerprint)),
                                }
                            };

                            match load_once() {
                                Ok(result) => Ok(result),
                                Err(primary_err) => {
                                    warn!(
                                        "snapshot stage A stale-first load failed; retrying with working-copy refresh: {primary_err:#}"
                                    );

                                    let fallback = || -> Result<SnapshotRefreshStageA> {
                                        if prefer_stale_first {
                                            let (fingerprint, workflow) =
                                                load_workflow_snapshot_with_fingerprint(
                                                    &source_dir,
                                                )?;
                                            return Ok(SnapshotRefreshStageA::Loaded {
                                                fingerprint,
                                                workflow: Box::new(workflow),
                                                loaded_without_refresh: false,
                                            });
                                        }

                                        let (fingerprint, workflow) =
                                            load_workflow_snapshot_if_changed_without_refresh(
                                                &source_dir,
                                                previous_fingerprint.as_ref(),
                                            )?;
                                        match workflow {
                                            Some(workflow) => Ok(SnapshotRefreshStageA::Loaded {
                                                fingerprint,
                                                workflow: Box::new(workflow),
                                                loaded_without_refresh: true,
                                            }),
                                            None => {
                                                Ok(SnapshotRefreshStageA::Unchanged(fingerprint))
                                            }
                                        }
                                    };

                                    match fallback() {
                                        Ok(result) => Ok(result),
                                        Err(fallback_err) => Err(primary_err.context(format!(
                                            "snapshot stage A fallback load failed: {fallback_err:#}"
                                        ))),
                                    }
                                }
                            }
                        })
                        .await
                }
                Err(err) => Err(err),
            };

            let (fingerprint, workflow_snapshot, loaded_without_refresh) = match stage_a_result {
                Ok(SnapshotRefreshStageA::Loaded {
                    fingerprint,
                    workflow,
                    loaded_without_refresh,
                }) => (fingerprint, workflow, loaded_without_refresh),
                Ok(SnapshotRefreshStageA::Unchanged(fingerprint)) => {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            if epoch != this.snapshot_epoch {
                                return;
                            }

                            this.finish_snapshot_refresh_loading();
                            this.workflow_loading = false;
                            let elapsed = started_at.elapsed();
                            info!(
                                "git workspace refresh skipped: epoch={} force={} priority={} cold_start={} elapsed_ms={} (no repo changes)",
                                epoch,
                                request.force,
                                request.priority.as_str(),
                                cold_start,
                                elapsed.as_millis()
                            );
                            this.auto_refresh_unmodified_streak =
                                this.auto_refresh_unmodified_streak.saturating_add(1);
                            this.last_snapshot_fingerprint = Some(fingerprint);
                            cx.notify();
                            this.maybe_run_pending_snapshot_refresh(cx);
                        });
                    }
                    return;
                }
                Err(err) => {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, move |this, cx| {
                            if epoch != this.snapshot_epoch {
                                return;
                            }

                            this.finish_snapshot_refresh_loading();
                            this.workflow_loading = false;
                            let elapsed = started_at.elapsed();
                            error!(
                                "git workspace refresh failed: epoch={} force={} priority={} cold_start={} elapsed_ms={} err={err:#}",
                                epoch,
                                request.force,
                                request.priority.as_str(),
                                cold_start,
                                elapsed.as_millis()
                            );
                            this.apply_snapshot_error(err, cx);
                            this.maybe_run_pending_snapshot_refresh(cx);
                        });
                    }
                    return;
                }
            };

            let workflow_file_count = workflow_snapshot.files.len();
            let workflow_branch_count = workflow_snapshot.branches.len();
            let workflow_revision_count = workflow_snapshot.bookmark_revisions.len();
            let workflow_ready_elapsed = started_at.elapsed();
            let should_run_cold_start_reconcile = cold_start && loaded_without_refresh;
            info!(
                "git workspace workflow ready: epoch={} force={} priority={} elapsed_ms={} files={} branches={} bookmark_revisions={} cold_start={}",
                epoch,
                request.force,
                request.priority.as_str(),
                workflow_ready_elapsed.as_millis(),
                workflow_file_count,
                workflow_branch_count,
                workflow_revision_count,
                cold_start
            );

            let repo_root = workflow_snapshot.root.clone();
            let line_stats_repo_root = repo_root.clone();
            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.snapshot_epoch {
                        return;
                    }
                    this.auto_refresh_unmodified_streak = 0;
                    this.last_snapshot_fingerprint = Some(fingerprint);
                    this.workflow_loading = false;
                    this.apply_workflow_snapshot(*workflow_snapshot, true, cx);
                    if let Some(line_stats_scope) = this.take_line_stats_refresh_scope(request) {
                        this.schedule_line_stats_refresh(
                            line_stats_repo_root.clone(),
                            request,
                            line_stats_scope,
                            epoch,
                            cold_start,
                            cx,
                        );
                    } else {
                        this.cancel_line_stats_refresh();
                    }
                });
            } else {
                return;
            }

            let reconcile_repo_root = repo_root;
            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.snapshot_epoch {
                        return;
                    }

                    this.finish_snapshot_refresh_loading();
                    let elapsed = started_at.elapsed();
                    info!(
                        "git workspace refresh complete: epoch={} force={} priority={} total_elapsed_ms={} cold_start={} line_stats_pending={}",
                        epoch,
                        request.force,
                        request.priority.as_str(),
                        elapsed.as_millis(),
                        cold_start,
                        this.line_stats_loading
                    );

                    cx.notify();
                    this.maybe_run_pending_snapshot_refresh(cx);
                });
            } else {
                return;
            }

            if !should_run_cold_start_reconcile {
                return;
            }

            let reconcile_started_at = Instant::now();
            let reconcile_result = cx
                .background_executor()
                .spawn(async move { load_snapshot_fingerprint(&reconcile_repo_root) })
                .await;

            match &reconcile_result {
                Ok(_) => {
                    info!(
                        "git workspace cold-start reconcile probe complete: epoch={} force={} priority={} elapsed_ms={} cold_start={}",
                        epoch,
                        request.force,
                        request.priority.as_str(),
                        reconcile_started_at.elapsed().as_millis(),
                        cold_start
                    );
                }
                Err(err) => {
                    warn!(
                        "git workspace cold-start reconcile probe failed: epoch={} force={} priority={} elapsed_ms={} cold_start={} err={err:#}",
                        epoch,
                        request.force,
                        request.priority.as_str(),
                        reconcile_started_at.elapsed().as_millis(),
                        cold_start
                    );
                }
            }

            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.snapshot_epoch {
                        return;
                    }

                    let Ok(reconciled_fingerprint) = reconcile_result else {
                        return;
                    };
                    if this.last_snapshot_fingerprint.as_ref() == Some(&reconciled_fingerprint) {
                        return;
                    }

                    info!(
                        "git workspace cold-start reconcile detected drift: epoch={} force={} priority={} cold_start={} -> scheduling foreground refresh",
                        epoch,
                        request.force,
                        request.priority.as_str(),
                        cold_start
                    );
                    this.request_snapshot_refresh_internal(
                        SnapshotRefreshRequest::user(false),
                        cx,
                    );
                });
            }
        });
    }

    pub(super) fn open_project_picker(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Project".into()),
        });

        self.open_project_task = cx.spawn(async move |this, cx| {
            let selection = match prompt.await {
                Ok(selection) => selection,
                Err(err) => {
                    error!("project picker prompt channel closed: {err}");
                    return;
                }
            };

            let selected_path = match selection {
                Ok(Some(paths)) => paths.into_iter().next(),
                Ok(None) => None,
                Err(err) => {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.git_status_message =
                                Some(format!("Failed to open folder picker: {err:#}"));
                            cx.notify();
                        });
                    }
                    return;
                }
            };

            let Some(selected_path) = selected_path else {
                return;
            };

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.project_path = Some(selected_path.clone());
                    this.set_last_project_path(Some(selected_path));
                    this.git_status_message = None;
                    this.start_repo_watch(cx);
                    this.request_snapshot_refresh_internal(
                        SnapshotRefreshRequest::user(true),
                        cx,
                    );
                    cx.notify();
                });
            }
        });
    }

    fn apply_workflow_snapshot(
        &mut self,
        snapshot: WorkflowSnapshot,
        full_refresh: bool,
        cx: &mut Context<Self>,
    ) {
        let WorkflowSnapshot {
            root,
            working_copy_commit_id,
            branch_name,
            branch_has_upstream,
            branch_ahead_count,
            can_undo_operation,
            can_redo_operation,
            branches,
            bookmark_revisions,
            files,
            last_commit_subject,
        } = snapshot;

        info!("loaded workflow snapshot from {}", root.display());
        let root_changed = self.repo_root.as_ref() != Some(&root);
        let previous_selected_path = self.selected_path.clone();
        let previous_selected_status = self.selected_status;
        let previous_files = self.files.clone();

        self.project_path = Some(root.clone());
        self.set_last_project_path(Some(root.clone()));
        self.repo_root = Some(root);
        self.ai_sync_workspace_preferences(cx);
        self.working_copy_commit_id = Some(working_copy_commit_id);
        self.branch_name = branch_name;
        self.branch_has_upstream = branch_has_upstream;
        self.branch_ahead_count = branch_ahead_count;
        self.can_undo_operation = can_undo_operation;
        self.can_redo_operation = can_redo_operation;
        self.branches = branches;
        self.bookmark_revisions = bookmark_revisions;
        self.pending_bookmark_switch = None;
        self.files = files;
        self.file_status_by_path = self
            .files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect();
        self.file_line_stats
            .retain(|path, _| self.files.iter().any(|file| file.path == *path));
        self.recompute_overall_line_stats_from_file_stats();
        self.commit_excluded_files
            .retain(|path| self.files.iter().any(|file| file.path == *path));
        self.last_commit_subject = last_commit_subject;
        self.repo_discovery_failed = false;
        self.error_message = None;
        if full_refresh {
            self.clear_comment_ui_state();
        }
        if root_changed {
            self.start_repo_watch(cx);
            self.working_copy_recovery_candidates.clear();
            self.commit_excluded_files.clear();
            if full_refresh {
                self.repo_tree.nodes.clear();
                self.repo_tree.rows.clear();
                self.repo_tree.file_count = 0;
                self.repo_tree.folder_count = 0;
                self.repo_tree.expanded_dirs.clear();
                self.repo_tree.scroll_anchor_path = None;
                self.repo_tree.row_count = 0;
                self.repo_tree.list_state.reset(0);
                self.repo_tree.error = None;
                self.repo_tree.changed_only = false;
                self.clear_full_repo_tree_cache();
                self.clear_editor_state(cx);
            }
        }
        self.collapsed_files
            .retain(|path| self.files.iter().any(|file| file.path == *path));
        let current_selection = self.selected_path.clone();
        self.selected_path = if full_refresh && self.workspace_view_mode == WorkspaceViewMode::Files {
            current_selection.or_else(|| self.files.first().map(|file| file.path.clone()))
        } else {
            current_selection
                .filter(|selected| self.files.iter().any(|file| &file.path == selected))
                .or_else(|| self.files.first().map(|file| file.path.clone()))
        };
        self.selected_status = self
            .selected_path
            .as_deref()
            .and_then(|selected| self.status_for_path(selected));

        if full_refresh {
            let selected_changed = self.selected_path != previous_selected_path
                || self.selected_status != previous_selected_status;
            let repo_tree_structure_changed =
                Self::repo_tree_structure_changed(previous_files.as_slice(), self.files.as_slice());

            self.refresh_comments_cache_from_store();

            let should_reload_repo_tree = if root_changed {
                true
            } else if !self.workspace_view_mode.supports_sidebar_tree() {
                false
            } else {
                self.workspace_view_mode == WorkspaceViewMode::Diff || repo_tree_structure_changed
            };
            if should_reload_repo_tree {
                self.request_repo_tree_reload(cx);
            }

            // Avoid expensive diff reload churn while using non-diff workspace modes.
            if !self.workspace_view_mode.supports_diff_stream() {
                self.scroll_selected_after_reload = false;
            } else {
                // Always reload visible diff rows after any loaded snapshot.
                // Fingerprints include more than file lists/counts, and diff text can change while
                // aggregate line stats and selected path stay the same.
                self.scroll_selected_after_reload = selected_changed || self.diff_rows.is_empty();
                self.request_selected_diff_reload(cx);
            }
        }

        self.persist_workflow_cache();
        cx.notify();
    }

    fn apply_snapshot_error(&mut self, err: anyhow::Error, cx: &mut Context<Self>) {
        let missing_repository = Self::is_missing_repository_error(&err);
        let error_message = Self::format_error_chain(&err);
        self.finish_snapshot_refresh_loading();
        self.workflow_loading = false;

        if !missing_repository {
            self.repo_discovery_failed = false;
            self.error_message = Some(error_message);
            cx.notify();
            return;
        }

        self.cancel_line_stats_refresh();
        self.cancel_patch_reload();
        self.pending_dirty_paths.clear();
        self.last_snapshot_fingerprint = None;
        self.repo_root = None;
        self.branch_name = "unknown".to_string();
        self.branch_has_upstream = false;
        self.branch_ahead_count = 0;
        self.working_copy_commit_id = None;
        self.can_undo_operation = false;
        self.can_redo_operation = false;
        self.branches.clear();
        self.bookmark_revisions.clear();
        self.pending_bookmark_switch = None;
        self.show_jj_terms_glossary = false;
        self.git_action_label = None;
        self.files.clear();
        self.file_status_by_path.clear();
        self.working_copy_recovery_candidates.clear();
        self.last_commit_subject = None;
        self.commit_excluded_files.clear();
        self.selected_path = None;
        self.selected_status = None;
        self.overall_line_stats = LineStats::default();
        self.comments_cache.clear();
        self.comment_miss_streaks.clear();
        self.reset_comment_row_match_cache();
        self.clear_comment_ui_state();
        self.file_row_ranges.clear();
        self.file_line_stats.clear();
        self.diff_row_metadata.clear();
        self.diff_row_segment_cache.clear();
        self.invalidate_segment_prefetch();
        self.diff_visible_file_header_lookup.clear();
        self.diff_visible_hunk_header_lookup.clear();
        self.selection_anchor_row = None;
        self.selection_head_row = None;
        self.drag_selecting_rows = false;
        self.diff_rows = vec![message_row(
            DiffRowKind::Empty,
            "Use File > Open Project... (Cmd/Ctrl+Shift+O) to load a JJ repository.",
        )];
        self.sync_diff_list_state();
        self.recompute_diff_layout();
        self.repo_discovery_failed = true;
        self.error_message = None;
        self.repo_tree.nodes.clear();
        self.repo_tree.rows.clear();
        self.repo_tree.file_count = 0;
        self.repo_tree.folder_count = 0;
        self.repo_tree.expanded_dirs.clear();
        self.repo_tree.scroll_anchor_path = None;
        self.repo_tree.row_count = 0;
        self.repo_tree.list_state.reset(0);
        self.repo_tree.loading = false;
        self.repo_tree.reload_pending = false;
        self.repo_tree.error = None;
        self.repo_tree.changed_only = false;
        self.clear_full_repo_tree_cache();
        self.clear_editor_state(cx);
        if self.state.git_workflow_cache.is_some() {
            self.state.git_workflow_cache = None;
            self.persist_state();
        }
        cx.notify();
    }

    fn format_error_chain(err: &anyhow::Error) -> String {
        err.chain()
            .enumerate()
            .map(|(index, cause)| {
                if index == 0 {
                    cause.to_string()
                } else {
                    format!("caused by ({index}): {cause}")
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn is_missing_repository_error(err: &anyhow::Error) -> bool {
        err.chain().any(|cause| {
            let message = cause.to_string();
            message.contains("failed to discover jj repository")
                || message.contains("there is no jj repo")
                || message.contains("failed to discover git repository")
                || message.contains("could not find repository")
        })
    }

    fn is_repo_tree_structure_status(status: FileStatus) -> bool {
        matches!(
            status,
            FileStatus::Added
                | FileStatus::Deleted
                | FileStatus::Renamed
                | FileStatus::TypeChange
                | FileStatus::Untracked
        )
    }

    fn repo_tree_structure_signature(files: &[ChangedFile]) -> BTreeSet<String> {
        files
            .iter()
            .filter(|file| Self::is_repo_tree_structure_status(file.status))
            .map(|file| format!("{}\u{1f}{}", file.path, file.status.tag()))
            .collect()
    }

    fn repo_tree_structure_changed(previous: &[ChangedFile], next: &[ChangedFile]) -> bool {
        Self::repo_tree_structure_signature(previous) != Self::repo_tree_structure_signature(next)
    }

    fn request_selected_diff_reload(&mut self, cx: &mut Context<Self>) {
        let Some(repo_root) = self.repo_root.clone() else {
            self.cancel_patch_reload();
            self.comments_cache.clear();
            self.comment_miss_streaks.clear();
            self.reset_comment_row_match_cache();
            self.clear_comment_ui_state();
            self.diff_rows.clear();
            self.diff_row_metadata.clear();
            self.diff_row_segment_cache.clear();
            self.invalidate_segment_prefetch();
            self.diff_visible_file_header_lookup.clear();
            self.diff_visible_hunk_header_lookup.clear();
            self.selection_anchor_row = None;
            self.selection_head_row = None;
            self.drag_selecting_rows = false;
            self.sync_diff_list_state();
            self.file_row_ranges.clear();
            self.file_line_stats.clear();
            self.recompute_overall_line_stats_from_file_stats();
            self.recompute_diff_layout();
            return;
        };

        if self.files.is_empty() {
            self.cancel_patch_reload();
            self.diff_rows = vec![message_row(DiffRowKind::Empty, "No changed files.")];
            self.diff_row_metadata.clear();
            self.diff_row_segment_cache.clear();
            self.invalidate_segment_prefetch();
            self.diff_visible_file_header_lookup.clear();
            self.diff_visible_hunk_header_lookup.clear();
            self.selection_anchor_row = None;
            self.selection_head_row = None;
            self.drag_selecting_rows = false;
            self.sync_diff_list_state();
            self.file_row_ranges.clear();
            self.file_line_stats.clear();
            self.recompute_overall_line_stats_from_file_stats();
            self.recompute_diff_layout();
            self.reconcile_comments_with_loaded_diff();
            cx.notify();
            return;
        }

        let files = self.files.clone();
        let collapsed_files = self.collapsed_files.clone();
        let previous_file_line_stats = self.file_line_stats.clone();
        let expanded_files = files
            .iter()
            .filter(|file| !collapsed_files.contains(file.path.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let initial_files =
            Self::select_initial_diff_files(&expanded_files, self.selected_path.as_deref());
        let initial_paths = initial_files
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        let remaining_files = expanded_files
            .into_iter()
            .filter(|file| !initial_paths.contains(file.path.as_str()))
            .collect::<Vec<_>>();
        let epoch = self.next_patch_epoch();
        self.invalidate_segment_prefetch();
        self.patch_loading = true;
        if self.diff_rows.is_empty() {
            self.diff_rows = vec![message_row(
                DiffRowKind::Meta,
                format!("Loading diffs for {} files...", files.len()),
            )];
            self.diff_row_metadata.clear();
            self.diff_row_segment_cache.clear();
            self.invalidate_segment_prefetch();
            self.diff_visible_file_header_lookup.clear();
            self.diff_visible_hunk_header_lookup.clear();
            self.file_row_ranges.clear();
            self.selection_anchor_row = None;
            self.selection_head_row = None;
            self.drag_selecting_rows = false;
            self.sync_diff_list_state();
            self.recompute_diff_layout();
            cx.notify();
        }

        enum PatchProgressUpdate {
            Loaded {
                batch_ix: Option<usize>,
                total_batches: usize,
                elapsed: Duration,
                stream: DiffStream,
                pending_files: usize,
                finished: bool,
            },
            Error {
                batch_ix: Option<usize>,
                total_batches: usize,
                elapsed: Duration,
                err: anyhow::Error,
            },
        }

        self.patch_task = cx.spawn(async move |this, cx| {
            let (progress_tx, mut progress_rx) = mpsc::unbounded::<PatchProgressUpdate>();
            let patch_loader_task = cx.background_executor().spawn({
                let repo_root = repo_root.clone();
                let files = files.clone();
                let collapsed_files = collapsed_files.clone();
                let previous_file_line_stats = previous_file_line_stats.clone();
                let initial_files = initial_files.clone();
                let remaining_files = remaining_files.clone();
                async move {
                    let total_batches = remaining_files.len().div_ceil(DIFF_PROGRESSIVE_BATCH_FILES);
                    if initial_files.is_empty() {
                        let stream = build_diff_stream_from_patch_map(
                            &files,
                            &collapsed_files,
                            &previous_file_line_stats,
                            &BTreeMap::new(),
                            &BTreeSet::new(),
                        );
                        progress_tx
                            .unbounded_send(PatchProgressUpdate::Loaded {
                                batch_ix: None,
                                total_batches,
                                elapsed: Duration::ZERO,
                                stream,
                                pending_files: 0,
                                finished: true,
                            })
                            .ok();
                        return;
                    }

                    let session_started_at = Instant::now();
                    let session = match open_patch_session(&repo_root) {
                        Ok(session) => session,
                        Err(err) => {
                            progress_tx
                                .unbounded_send(PatchProgressUpdate::Error {
                                    batch_ix: None,
                                    total_batches,
                                    elapsed: session_started_at.elapsed(),
                                    err,
                                })
                                .ok();
                            return;
                        }
                    };

                    let mut loaded_patches = BTreeMap::new();
                    let mut loading_paths = remaining_files
                        .iter()
                        .map(|file| file.path.clone())
                        .collect::<BTreeSet<_>>();

                    let initial_stage_started_at = Instant::now();
                    match load_patches_for_files_from_session(&session, &initial_files) {
                        Ok(stage_patches) => {
                            loaded_patches.extend(stage_patches);
                            for file in &initial_files {
                                loading_paths.remove(file.path.as_str());
                            }
                            let stream = build_diff_stream_from_patch_map(
                                &files,
                                &collapsed_files,
                                &previous_file_line_stats,
                                &loaded_patches,
                                &loading_paths,
                            );
                            progress_tx
                                .unbounded_send(PatchProgressUpdate::Loaded {
                                    batch_ix: None,
                                    total_batches,
                                    elapsed: initial_stage_started_at.elapsed(),
                                    stream,
                                    pending_files: loading_paths.len(),
                                    finished: remaining_files.is_empty(),
                                })
                                .ok();
                        }
                        Err(err) => {
                            progress_tx
                                .unbounded_send(PatchProgressUpdate::Error {
                                    batch_ix: None,
                                    total_batches,
                                    elapsed: initial_stage_started_at.elapsed(),
                                    err,
                                })
                                .ok();
                            return;
                        }
                    }

                    for (batch_ix, batch) in remaining_files
                        .chunks(DIFF_PROGRESSIVE_BATCH_FILES)
                        .enumerate()
                    {
                        let stage_started_at = Instant::now();
                        let stage_files = batch.to_vec();
                        match load_patches_for_files_from_session(&session, &stage_files) {
                            Ok(stage_patches) => {
                                loaded_patches.extend(stage_patches);
                                for file in &stage_files {
                                    loading_paths.remove(file.path.as_str());
                                }
                                let stream = build_diff_stream_from_patch_map(
                                    &files,
                                    &collapsed_files,
                                    &previous_file_line_stats,
                                    &loaded_patches,
                                    &loading_paths,
                                );
                                progress_tx
                                    .unbounded_send(PatchProgressUpdate::Loaded {
                                        batch_ix: Some(batch_ix),
                                        total_batches,
                                        elapsed: stage_started_at.elapsed(),
                                        stream,
                                        pending_files: loading_paths.len(),
                                        finished: batch_ix.saturating_add(1) == total_batches,
                                    })
                                    .ok();
                            }
                            Err(err) => {
                                progress_tx
                                    .unbounded_send(PatchProgressUpdate::Error {
                                        batch_ix: Some(batch_ix),
                                        total_batches,
                                        elapsed: stage_started_at.elapsed(),
                                        err,
                                    })
                                    .ok();
                                return;
                            }
                        }
                    }
                }
            });

            while let Some(update) = progress_rx.next().await {
                let Some(this) = this.upgrade() else {
                    break;
                };
                this.update(cx, move |this, cx| {
                    if epoch != this.patch_epoch {
                        return;
                    }

                    match update {
                        PatchProgressUpdate::Loaded {
                            batch_ix,
                            total_batches,
                            elapsed,
                            stream,
                            pending_files,
                            finished,
                        } => {
                            match batch_ix {
                                Some(batch_ix) => {
                                    info!(
                                        "progressive diff batch {}/{} loaded in {:?} (rows={}, pending_files={})",
                                        batch_ix.saturating_add(1),
                                        total_batches,
                                        elapsed,
                                        stream.rows.len(),
                                        pending_files
                                    );
                                }
                                None => {
                                    info!(
                                        "initial diff stream loaded in {:?} (rows={}, files={})",
                                        elapsed,
                                        stream.rows.len(),
                                        stream.file_ranges.len()
                                    );
                                }
                            }

                            if finished {
                                this.patch_loading = false;
                            }
                            this.apply_loaded_diff_stream(stream);
                            cx.notify();
                        }
                        PatchProgressUpdate::Error {
                            batch_ix,
                            total_batches,
                            elapsed,
                            err,
                        } => {
                            this.patch_loading = false;
                            match batch_ix {
                                Some(batch_ix) => {
                                    error!(
                                        "progressive diff batch {}/{} failed after {:?}: {err:#}",
                                        batch_ix.saturating_add(1),
                                        total_batches,
                                        elapsed
                                    );
                                }
                                None => {
                                    error!("initial diff stage failed after {:?}: {err:#}", elapsed);
                                }
                            }
                            this.apply_diff_stream_error(err);
                            cx.notify();
                        }
                    }
                });
            }

            patch_loader_task.await;
        });
    }

    fn select_initial_diff_files(
        files: &[ChangedFile],
        selected_path: Option<&str>,
    ) -> Vec<ChangedFile> {
        if files.is_empty() {
            return Vec::new();
        }

        if let Some(selected_path) = selected_path
            && let Some(file) = files.iter().find(|file| file.path == selected_path)
        {
            return vec![file.clone()];
        }

        vec![files[0].clone()]
    }

    fn apply_loaded_diff_stream(&mut self, stream: DiffStream) {
        self.invalidate_segment_prefetch();
        self.diff_rows = stream.rows;
        self.diff_row_metadata = stream.row_metadata;
        self.diff_row_segment_cache = stream.row_segments;
        self.clamp_comment_rows_to_diff();
        self.clamp_selection_to_rows();
        self.drag_selecting_rows = false;
        self.sync_diff_list_state();
        self.file_row_ranges = stream.file_ranges;
        self.file_line_stats = stream.file_line_stats;
        if !self.patch_loading || !self.line_stats_loading {
            self.recompute_overall_line_stats_from_file_stats();
        }
        self.recompute_diff_layout();

        if self.workspace_view_mode == WorkspaceViewMode::Files {
            if self.selected_path.is_none() {
                self.selected_path = self.files.first().map(|file| file.path.clone());
            }
        } else {
            let has_selection = self
                .selected_path
                .as_ref()
                .is_some_and(|path| self.files.iter().any(|file| file.path == *path));
            if !has_selection {
                self.selected_path = self.files.first().map(|file| file.path.clone());
            }
        }

        self.selected_status = self
            .selected_path
            .as_deref()
            .and_then(|selected| self.status_for_path(selected));
        self.last_visible_row_start = None;
        self.recompute_diff_visible_header_lookup();
        self.rebuild_comment_row_match_cache();

        if self.scroll_selected_after_reload {
            self.scroll_selected_file_to_top();
            if !self.patch_loading {
                self.scroll_selected_after_reload = false;
            }
        }
        if !self.patch_loading {
            self.reconcile_comments_with_loaded_diff();
        }
    }

    fn apply_diff_stream_error(&mut self, err: anyhow::Error) {
        self.diff_rows = vec![message_row(
            DiffRowKind::Meta,
            format!("Failed to load diff stream: {err:#}"),
        )];
        self.diff_row_metadata.clear();
        self.diff_row_segment_cache.clear();
        self.invalidate_segment_prefetch();
        self.selection_anchor_row = None;
        self.selection_head_row = None;
        self.drag_selecting_rows = false;
        self.sync_diff_list_state();
        self.file_row_ranges.clear();
        self.recompute_diff_layout();
        self.diff_visible_file_header_lookup.clear();
        self.diff_visible_hunk_header_lookup.clear();
        self.scroll_selected_after_reload = false;
        self.clamp_comment_rows_to_diff();
        self.rebuild_comment_row_match_cache();
    }

}

fn should_send_ai_prompt_from_input_event(event: &InputEvent) -> bool {
    matches!(event, InputEvent::PressEnter { secondary: false })
}

#[cfg(test)]
mod ai_input_tests {
    use super::should_send_ai_prompt_from_input_event;
    use gpui_component::input::InputEvent;

    #[test]
    fn enter_sends_prompt() {
        assert!(should_send_ai_prompt_from_input_event(&InputEvent::PressEnter {
            secondary: false,
        }));
    }

    #[test]
    fn secondary_enter_does_not_send_prompt() {
        assert!(!should_send_ai_prompt_from_input_event(
            &InputEvent::PressEnter { secondary: true }
        ));
    }

    #[test]
    fn non_enter_events_do_not_send_prompt() {
        assert!(!should_send_ai_prompt_from_input_event(&InputEvent::Change));
        assert!(!should_send_ai_prompt_from_input_event(&InputEvent::Focus));
        assert!(!should_send_ai_prompt_from_input_event(&InputEvent::Blur));
    }
}
