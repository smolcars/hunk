fn should_store_legacy_diff_surface_rows(
    workspace_view_mode: WorkspaceViewMode,
    has_review_workspace_session: bool,
) -> bool {
    workspace_view_mode != WorkspaceViewMode::Diff || !has_review_workspace_session
}

impl DiffViewer {
    fn empty_workspace_project_state() -> WorkspaceProjectState {
        WorkspaceProjectState {
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
            files: Vec::new(),
            file_status_by_path: BTreeMap::new(),
            last_commit_subject: None,
            recent_commits: Vec::new(),
            recent_commits_error: None,
            collapsed_files: BTreeSet::new(),
            selected_path: None,
            selected_status: None,
            diff_rows: Vec::new(),
            diff_row_metadata: Vec::new(),
            diff_row_segment_cache: Vec::new(),
            file_row_ranges: Vec::new(),
            file_line_stats: BTreeMap::new(),
            review_surface: ReviewWorkspaceSurfaceState::new(),
            review_files: Vec::new(),
            review_file_status_by_path: BTreeMap::new(),
            review_file_line_stats: BTreeMap::new(),
            review_overall_line_stats: LineStats::default(),
            review_compare_loading: false,
            review_compare_error: None,
            review_workspace_session: None,
            review_workspace_editor_session: None,
            review_loaded_snapshot_fingerprint: None,
            review_last_selected_path: None,
            overall_line_stats: LineStats::default(),
            last_git_workspace_fingerprint: None,
            recent_commits_loading: false,
            last_recent_commits_fingerprint: None,
            last_snapshot_fingerprint: None,
            repo_tree: RepoTreeState::new(),
            file_editor_tabs: Vec::new(),
            active_file_editor_tab_id: None,
            next_file_editor_tab_id: 0,
            file_editor_tab_scroll_handle: ScrollHandle::default(),
            files_editor: Rc::new(RefCell::new(crate::app::native_files_editor::FilesEditor::new())),
            file_quick_open_visible: false,
            file_quick_open_matches: Vec::new(),
            file_quick_open_selected_ix: 0,
            editor_path: None,
            editor_error: None,
            editor_dirty: false,
            editor_last_saved_text: None,
            editor_markdown_preview_blocks: Vec::new(),
            editor_markdown_preview_revision: 0,
            editor_markdown_preview: false,
            editor_search_visible: false,
            selection_anchor_row: None,
            selection_head_row: None,
        }
    }

    fn prepare_current_workspace_project_state_for_storage(&mut self) {
        if self.workspace_view_mode == WorkspaceViewMode::Files {
            self.capture_sidebar_repo_scroll_anchor();
            if self.repo_tree.full_cache.is_some() {
                self.sync_full_repo_tree_cache_from_current();
            }
        }

        self.sync_active_file_editor_tab_state();

        self.repo_tree_context_menu = None;
        self.repo_tree_inline_edit = None;
        self.repo_tree.epoch = self.repo_tree.epoch.saturating_add(1);
        self.repo_tree.task = Task::ready(());
        self.repo_tree.loading = false;
        self.repo_tree.reload_pending = false;

        self.editor_epoch = self.editor_epoch.saturating_add(1);
        self.editor_task = Task::ready(());
        self.editor_loading = false;
        self.editor_save_epoch = self.editor_save_epoch.saturating_add(1);
        self.editor_save_task = Task::ready(());
        self.editor_save_loading = false;
        self.editor_markdown_preview_task = Task::ready(());
        self.editor_markdown_preview_loading = false;

        for tab in &mut self.file_editor_tabs {
            tab.reload_epoch = tab.reload_epoch.saturating_add(1);
            tab.reload_task = Task::ready(());
            tab.loading = false;
            tab.save_epoch = tab.save_epoch.saturating_add(1);
            tab.save_task = Task::ready(());
            tab.save_loading = false;
            tab.markdown_preview_task = Task::ready(());
            tab.markdown_preview_loading = false;
        }
    }

    fn take_current_workspace_project_state(&mut self) -> WorkspaceProjectState {
        self.prepare_current_workspace_project_state_for_storage();
        let store_legacy_diff_rows = should_store_legacy_diff_surface_rows(
            self.workspace_view_mode,
            self.review_workspace_session.is_some(),
        );
        let diff_rows = if store_legacy_diff_rows {
            std::mem::take(&mut self.diff_rows)
        } else {
            self.diff_rows.clear();
            Vec::new()
        };
        let diff_row_metadata = if store_legacy_diff_rows {
            std::mem::take(&mut self.diff_row_metadata)
        } else {
            self.diff_row_metadata.clear();
            Vec::new()
        };
        let diff_row_segment_cache = if store_legacy_diff_rows {
            std::mem::take(&mut self.diff_row_segment_cache)
        } else {
            self.diff_row_segment_cache.clear();
            Vec::new()
        };
        WorkspaceProjectState {
            repo_root: self.repo_root.take(),
            workspace_targets: std::mem::take(&mut self.workspace_targets),
            active_workspace_target_id: self.active_workspace_target_id.take(),
            git_workspace: std::mem::take(&mut self.git_workspace),
            review_compare_sources: std::mem::take(&mut self.review_compare_sources),
            review_default_left_source_id: self.review_default_left_source_id.take(),
            review_default_right_source_id: self.review_default_right_source_id.take(),
            review_left_source_id: self.review_left_source_id.take(),
            review_right_source_id: self.review_right_source_id.take(),
            review_loaded_left_source_id: self.review_loaded_left_source_id.take(),
            review_loaded_right_source_id: self.review_loaded_right_source_id.take(),
            review_loaded_collapsed_files: std::mem::take(&mut self.review_loaded_collapsed_files),
            branch_name: std::mem::take(&mut self.branch_name),
            branch_has_upstream: self.branch_has_upstream,
            branch_ahead_count: self.branch_ahead_count,
            branch_behind_count: self.branch_behind_count,
            working_copy_commit_id: self.working_copy_commit_id.take(),
            branches: std::mem::take(&mut self.branches),
            git_working_tree_scroll_handle: std::mem::take(&mut self.git_working_tree_scroll_handle),
            recent_commits_scroll_handle: std::mem::take(&mut self.recent_commits_scroll_handle),
            files: std::mem::take(&mut self.files),
            file_status_by_path: std::mem::take(&mut self.file_status_by_path),
            last_commit_subject: self.last_commit_subject.take(),
            recent_commits: std::mem::take(&mut self.recent_commits),
            recent_commits_error: self.recent_commits_error.take(),
            collapsed_files: std::mem::take(&mut self.collapsed_files),
            selected_path: self.selected_path.take(),
            selected_status: self.selected_status.take(),
            diff_rows,
            diff_row_metadata,
            diff_row_segment_cache,
            file_row_ranges: std::mem::take(&mut self.file_row_ranges),
            file_line_stats: std::mem::take(&mut self.file_line_stats),
            review_surface: std::mem::replace(
                &mut self.review_surface,
                ReviewWorkspaceSurfaceState::new(),
            ),
            review_files: std::mem::take(&mut self.review_files),
            review_file_status_by_path: std::mem::take(&mut self.review_file_status_by_path),
            review_file_line_stats: std::mem::take(&mut self.review_file_line_stats),
            review_overall_line_stats: self.review_overall_line_stats,
            review_compare_loading: self.review_compare_loading,
            review_compare_error: self.review_compare_error.take(),
            review_workspace_session: self.review_workspace_session.take(),
            review_workspace_editor_session: self.review_workspace_editor_session.take(),
            review_loaded_snapshot_fingerprint: self.review_loaded_snapshot_fingerprint.take(),
            review_last_selected_path: self.review_last_selected_path.take(),
            overall_line_stats: self.overall_line_stats,
            last_git_workspace_fingerprint: self.last_git_workspace_fingerprint.take(),
            recent_commits_loading: self.recent_commits_loading,
            last_recent_commits_fingerprint: self.last_recent_commits_fingerprint.take(),
            last_snapshot_fingerprint: self.last_snapshot_fingerprint.take(),
            repo_tree: std::mem::replace(&mut self.repo_tree, RepoTreeState::new()),
            file_editor_tabs: std::mem::take(&mut self.file_editor_tabs),
            active_file_editor_tab_id: self.active_file_editor_tab_id.take(),
            next_file_editor_tab_id: self.next_file_editor_tab_id,
            file_editor_tab_scroll_handle: std::mem::take(&mut self.file_editor_tab_scroll_handle),
            files_editor: std::mem::replace(
                &mut self.files_editor,
                Rc::new(RefCell::new(crate::app::native_files_editor::FilesEditor::new())),
            ),
            file_quick_open_visible: self.file_quick_open_visible,
            file_quick_open_matches: std::mem::take(&mut self.file_quick_open_matches),
            file_quick_open_selected_ix: self.file_quick_open_selected_ix,
            editor_path: self.editor_path.take(),
            editor_error: self.editor_error.take(),
            editor_dirty: self.editor_dirty,
            editor_last_saved_text: self.editor_last_saved_text.take(),
            editor_markdown_preview_blocks: std::mem::take(&mut self.editor_markdown_preview_blocks),
            editor_markdown_preview_revision: self.editor_markdown_preview_revision,
            editor_markdown_preview: self.editor_markdown_preview,
            editor_search_visible: self.editor_search_visible,
            selection_anchor_row: self.selection_anchor_row.take(),
            selection_head_row: self.selection_head_row.take(),
        }
    }

    fn apply_workspace_project_state(&mut self, state: WorkspaceProjectState) {
        self.reset_recent_commits_state();
        self.clear_git_workspace_state();
        self.cancel_patch_reload();
        self.cancel_line_stats_refresh();
        self.pending_dirty_paths.clear();
        self.git_status_message = None;
        self.error_message = None;
        self.repo_discovery_failed = false;
        self.branch_name = "unknown".to_string();
        self.branch_has_upstream = false;
        self.branch_ahead_count = 0;
        self.branch_behind_count = 0;
        self.working_copy_commit_id = None;
        self.workspace_target_switch_loading = false;
        self.review_compare_loading = false;
        self.review_compare_error = None;
        self.repo_root = state.repo_root;
        self.workspace_targets = state.workspace_targets;
        self.active_workspace_target_id = state.active_workspace_target_id;
        self.git_workspace = state.git_workspace;
        self.review_compare_sources = state.review_compare_sources;
        self.review_default_left_source_id = state.review_default_left_source_id;
        self.review_default_right_source_id = state.review_default_right_source_id;
        self.review_left_source_id = state.review_left_source_id;
        self.review_right_source_id = state.review_right_source_id;
        self.review_loaded_left_source_id = state.review_loaded_left_source_id;
        self.review_loaded_right_source_id = state.review_loaded_right_source_id;
        self.review_loaded_collapsed_files = state.review_loaded_collapsed_files;
        self.branch_name = state.branch_name;
        self.branch_has_upstream = state.branch_has_upstream;
        self.branch_ahead_count = state.branch_ahead_count;
        self.branch_behind_count = state.branch_behind_count;
        self.working_copy_commit_id = state.working_copy_commit_id;
        self.branches = state.branches;
        self.git_working_tree_scroll_handle = state.git_working_tree_scroll_handle;
        self.recent_commits_scroll_handle = state.recent_commits_scroll_handle;
        self.files = state.files;
        self.file_status_by_path = state.file_status_by_path;
        self.last_commit_subject = state.last_commit_subject;
        self.recent_commits = state.recent_commits;
        self.recent_commits_error = state.recent_commits_error;
        self.collapsed_files = state.collapsed_files;
        self.selected_path = state.selected_path;
        self.selected_status = state.selected_status;
        self.diff_rows = state.diff_rows;
        self.diff_row_metadata = state.diff_row_metadata;
        self.diff_row_segment_cache = state.diff_row_segment_cache;
        self.file_row_ranges = state.file_row_ranges;
        self.file_line_stats = state.file_line_stats;
        self.review_surface = state.review_surface;
        self.review_files = state.review_files;
        self.review_file_status_by_path = state.review_file_status_by_path;
        self.review_file_line_stats = state.review_file_line_stats;
        self.review_overall_line_stats = state.review_overall_line_stats;
        self.review_compare_loading = state.review_compare_loading;
        self.review_compare_error = state.review_compare_error;
        self.review_workspace_session = state.review_workspace_session;
        self.review_workspace_editor_session = state.review_workspace_editor_session;
        self.review_loaded_snapshot_fingerprint = state.review_loaded_snapshot_fingerprint;
        self.review_last_selected_path = state.review_last_selected_path;
        self.overall_line_stats = state.overall_line_stats;
        self.last_git_workspace_fingerprint = state.last_git_workspace_fingerprint;
        self.recent_commits_loading = state.recent_commits_loading;
        self.last_recent_commits_fingerprint = state.last_recent_commits_fingerprint;
        self.last_snapshot_fingerprint = state.last_snapshot_fingerprint;
        self.repo_tree = state.repo_tree;
        self.file_editor_tabs = state.file_editor_tabs;
        self.active_file_editor_tab_id = state.active_file_editor_tab_id;
        self.next_file_editor_tab_id = state.next_file_editor_tab_id;
        self.file_editor_tab_scroll_handle = state.file_editor_tab_scroll_handle;
        self.files_editor = state.files_editor;
        self.file_quick_open_visible = state.file_quick_open_visible;
        self.file_quick_open_matches = state.file_quick_open_matches;
        self.file_quick_open_selected_ix = state.file_quick_open_selected_ix;
        self.editor_path = state.editor_path;
        self.editor_error = state.editor_error;
        self.editor_dirty = state.editor_dirty;
        self.editor_last_saved_text = state.editor_last_saved_text;
        self.editor_markdown_preview_blocks = state.editor_markdown_preview_blocks;
        self.editor_markdown_preview_revision = state.editor_markdown_preview_revision;
        self.editor_markdown_preview = state.editor_markdown_preview;
        self.editor_search_visible = state.editor_search_visible;
        self.selection_anchor_row = state.selection_anchor_row;
        self.selection_head_row = state.selection_head_row;

        self.snapshot_loading = false;
        self.snapshot_active_request = None;
        self.workflow_loading = false;
        self.patch_loading = false;
        self.line_stats_loading = false;
        self.recent_commits_active_request = None;
        self.pending_recent_commits_refresh = None;
        self.pending_snapshot_refresh = None;
        self.pending_line_stats_refresh = None;
        self.scroll_selected_after_reload = false;
        self.last_scroll_activity_at = Instant::now();
    }

    fn store_current_workspace_project_state(&mut self) {
        let Some(project_key) = self.current_workspace_project_key() else {
            return;
        };
        let state = self.take_current_workspace_project_state();
        self.workspace_project_states.insert(project_key, state);
    }

    fn restore_workspace_project_state(&mut self, project_root: &std::path::Path) -> bool {
        let project_key = project_root.to_string_lossy().to_string();
        let Some(state) = self.workspace_project_states.remove(project_key.as_str()) else {
            return false;
        };
        self.apply_workspace_project_state(state);
        true
    }

    fn discard_workspace_project_state(&mut self, project_root: &std::path::Path) {
        let project_key = project_root.to_string_lossy().to_string();
        let Some(mut state) = self.workspace_project_states.remove(project_key.as_str()) else {
            return;
        };
        for tab in &mut state.file_editor_tabs {
            tab.files_editor.borrow_mut().shutdown();
        }
        state.files_editor.borrow_mut().shutdown();
    }

    fn update_project_picker_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_project_path = self
            .project_path
            .as_deref()
            .or(self.state.active_project_path().map(std::path::PathBuf::as_path));
        let delegate = build_project_picker_delegate(
            self.state.workspace_project_paths.as_slice(),
            active_project_path,
        );
        let selected_index = project_picker_selected_index(
            self.state.workspace_project_paths.as_slice(),
            active_project_path,
        );
        Self::set_index_picker_state(
            &self.project_picker_state,
            delegate,
            selected_index,
            window,
            cx,
        );
        cx.notify();
    }

    fn sync_project_picker_state(&mut self, cx: &mut Context<Self>) {
        let active_project_path = self
            .project_path
            .as_deref()
            .or(self.state.active_project_path().map(std::path::PathBuf::as_path));
        let project_picker_state = self.project_picker_state.clone();
        let delegate = build_project_picker_delegate(
            self.state.workspace_project_paths.as_slice(),
            active_project_path,
        );
        let selected_index = project_picker_selected_index(
            self.state.workspace_project_paths.as_slice(),
            active_project_path,
        );

        Self::sync_index_picker_state(
            project_picker_state,
            delegate,
            selected_index,
            "failed to sync project picker state",
            cx,
        );
    }
}

#[cfg(test)]
mod workspace_project_state_tests {
    use super::should_store_legacy_diff_surface_rows;
    use crate::app::data::WorkspaceViewMode;

    #[test]
    fn diff_mode_with_workspace_session_drops_legacy_row_vectors() {
        assert!(!should_store_legacy_diff_surface_rows(
            WorkspaceViewMode::Diff,
            true,
        ));
    }

    #[test]
    fn files_mode_and_legacy_diff_mode_keep_flat_rows() {
        assert!(should_store_legacy_diff_surface_rows(
            WorkspaceViewMode::Files,
            true,
        ));
        assert!(should_store_legacy_diff_surface_rows(
            WorkspaceViewMode::Diff,
            false,
        ));
    }
}
