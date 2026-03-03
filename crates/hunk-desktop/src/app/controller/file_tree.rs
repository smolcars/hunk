use std::path::{Component, Path, PathBuf};

impl DiffViewer {
    pub(super) fn toggle_sidebar_tree_action(
        &mut self,
        _: &ToggleSidebarTree,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_sidebar_tree(cx);
    }

    pub(super) fn toggle_sidebar_tree(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        if !self.sidebar_collapsed && self.repo_tree.nodes.is_empty() && !self.repo_tree.loading {
            self.request_repo_tree_reload(cx);
        }
        cx.notify();
    }

    pub(super) fn switch_to_files_view_action(
        &mut self,
        _: &SwitchToFilesView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.set_workspace_view_mode(WorkspaceViewMode::Files, cx);
    }

    pub(super) fn switch_to_review_view_action(
        &mut self,
        _: &SwitchToReviewView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.set_workspace_view_mode(WorkspaceViewMode::Diff, cx);
    }

    pub(super) fn switch_to_graph_view_action(
        &mut self,
        _: &SwitchToGraphView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.set_workspace_view_mode(WorkspaceViewMode::JjWorkspace, cx);
    }

    pub(super) fn set_workspace_view_mode(&mut self, mode: WorkspaceViewMode, cx: &mut Context<Self>) {
        let previous_mode = self.workspace_view_mode;
        if previous_mode == mode {
            if !self.sidebar_collapsed
                && mode != WorkspaceViewMode::JjWorkspace
                && self.repo_tree.nodes.is_empty()
                && !self.repo_tree.loading
            {
                self.request_repo_tree_reload(cx);
            }
            return;
        }

        if previous_mode == WorkspaceViewMode::Files {
            self.capture_sidebar_repo_scroll_anchor();
            if self.repo_tree.full_cache.is_some() {
                self.sync_full_repo_tree_cache_from_current();
            }
        }

        self.workspace_view_mode = mode;
        if mode != WorkspaceViewMode::Files {
            self.repo_tree_inline_edit = None;
            self.repo_tree_context_menu = None;
        }

        if mode == WorkspaceViewMode::Files {
            if previous_mode == WorkspaceViewMode::Diff || self.repo_tree.changed_only {
                if !self.restore_full_repo_tree_from_cache() && !self.repo_tree.loading {
                    self.request_repo_tree_reload(cx);
                }
            } else if self.repo_tree.nodes.is_empty() && !self.repo_tree.loading {
                self.request_repo_tree_reload(cx);
            }

            let target_path = self.editor_path.clone().or_else(|| self.selected_path.clone()).or_else(|| {
                self.files
                    .iter()
                    .find(|file| file.status != FileStatus::Deleted)
                    .map(|file| file.path.clone())
            });
            if let Some(path) = target_path {
                let editor_already_open = self.editor_path.as_deref() == Some(path.as_str())
                    && !self.editor_loading
                    && self.editor_error.is_none();
                if !editor_already_open
                    && self.prevent_unsaved_editor_discard(Some(path.as_str()), cx)
                {
                    return;
                }
                self.selected_path = Some(path.clone());
                self.selected_status = self.status_for_path(path.as_str());
                if !editor_already_open {
                    self.request_file_editor_reload(path, cx);
                }
            } else {
                if self.prevent_unsaved_editor_discard(None, cx) {
                    return;
                }
                self.selected_path = None;
                self.selected_status = None;
                self.clear_editor_state(cx);
            }
        } else if mode == WorkspaceViewMode::Diff {
            let selected_in_changed_files = self
                .selected_path
                .as_ref()
                .is_some_and(|selected| self.files.iter().any(|file| &file.path == selected));
            if !selected_in_changed_files {
                self.selected_path = self.files.first().map(|file| file.path.clone());
                self.selected_status = self
                    .selected_path
                    .as_deref()
                    .and_then(|selected| self.status_for_path(selected));
            }
            self.request_repo_tree_reload(cx);
            self.scroll_selected_after_reload = true;
            self.request_selected_diff_reload(cx);
        }
        cx.notify();
    }

    pub(super) fn toggle_repo_tree_directory(&mut self, path: String, cx: &mut Context<Self>) {
        self.repo_tree_context_menu = None;
        if self.repo_tree.expanded_dirs.contains(path.as_str()) {
            self.repo_tree.expanded_dirs.remove(path.as_str());
        } else {
            self.repo_tree.expanded_dirs.insert(path);
        }
        self.rebuild_repo_tree_rows();
        cx.notify();
    }

    pub(super) fn select_repo_tree_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.repo_tree_context_menu = None;
        if self.workspace_view_mode == WorkspaceViewMode::Files
            && self.prevent_unsaved_editor_discard(Some(path.as_str()), cx)
        {
            return;
        }

        self.selected_path = Some(path.clone());
        self.selected_status = self.status_for_path(path.as_str());
        if self.workspace_view_mode == WorkspaceViewMode::Files {
            self.request_file_editor_reload(path, cx);
        } else {
            self.scroll_to_file_start(&path);
            self.last_visible_row_start = None;
            self.last_diff_scroll_offset = None;
            self.last_scroll_activity_at = Instant::now();
        }
        cx.notify();
    }

    pub(super) fn repo_tree_new_file_action(
        &mut self,
        _: &RepoTreeNewFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.repo_tree_inline_edit.is_some() { return; }
        self.repo_tree_focus_handle.focus(window, cx);
        self.open_repo_tree_new_file_prompt_at(
            self.selected_repo_tree_file_target(),
            window,
            cx,
        );
    }

    pub(super) fn repo_tree_new_folder_action(
        &mut self,
        _: &RepoTreeNewFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.repo_tree_inline_edit.is_some() { return; }
        self.repo_tree_focus_handle.focus(window, cx);
        self.open_repo_tree_new_folder_prompt_at(
            self.selected_repo_tree_file_target(),
            window,
            cx,
        );
    }

    pub(super) fn repo_tree_rename_file_action(
        &mut self,
        _: &RepoTreeRenameFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.repo_tree_inline_edit.is_some() { return; }
        self.repo_tree_focus_handle.focus(window, cx);
        let Some(path) = self.selected_path.clone() else {
            let message = "Select a file in the tree before renaming.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        };
        self.open_repo_tree_rename_prompt_for_file(path, window, cx);
    }

    pub(super) fn open_repo_tree_new_file_prompt_at(
        &mut self,
        target: Option<(String, RepoTreeNodeKind)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ensure_repo_tree_manageable(cx) {
            return;
        }
        let base_dir = target
            .map(|(path, kind)| repo_tree_base_dir(path.as_str(), kind))
            .transpose()
            .ok()
            .flatten()
            .flatten();
        self.start_repo_tree_inline_edit(RepoTreePromptAction::CreateFile { base_dir }, window, cx);
    }

    pub(super) fn open_repo_tree_new_folder_prompt_at(
        &mut self,
        target: Option<(String, RepoTreeNodeKind)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ensure_repo_tree_manageable(cx) {
            return;
        }
        let base_dir = target
            .map(|(path, kind)| repo_tree_base_dir(path.as_str(), kind))
            .transpose()
            .ok()
            .flatten()
            .flatten();
        self.start_repo_tree_inline_edit(RepoTreePromptAction::CreateFolder { base_dir }, window, cx);
    }

    pub(super) fn open_repo_tree_rename_prompt_for_file(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ensure_repo_tree_manageable(cx) {
            return;
        }
        self.start_repo_tree_inline_edit(RepoTreePromptAction::RenameFile { path }, window, cx);
    }

    pub(super) fn delete_repo_tree_file_at(
        &mut self,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.ensure_repo_tree_manageable(cx) {
            return;
        }
        if self.prevent_unsaved_editor_discard(None, cx) {
            return;
        }

        let Some(repo_root) = self.repo_root.clone() else {
            return;
        };
        let result = fs_delete_repo_tree_file(&repo_root, path);
        match result {
            Ok(()) => {
                if self.selected_path.as_deref() == Some(path) {
                    self.selected_path = None;
                    self.selected_status = None;
                }
                if self.editor_path.as_deref() == Some(path) {
                    self.clear_editor_state(cx);
                }
                self.refresh_after_repo_tree_fs_mutation(cx);
                self.git_status_message = Some(format!("Deleted {path}"));
            }
            Err(err) => {
                let message = format!("Failed to delete {path}: {err:#}");
                self.git_status_message = Some(message.clone());
                Self::push_error_notification(message, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn copy_repo_tree_absolute_path(
        &mut self,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_root) = self.repo_root.clone() else {
            self.git_status_message = Some("No repository is open.".to_string());
            cx.notify();
            return;
        };
        let absolute = repo_root.join(path);
        cx.write_to_clipboard(ClipboardItem::new_string(
            absolute.display().to_string(),
        ));
        self.git_status_message = Some(format!("Copied absolute path for {path}"));
        cx.notify();
    }

    pub(super) fn copy_repo_tree_relative_path(
        &mut self,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        self.git_status_message = Some(format!("Copied relative path for {path}"));
        cx.notify();
    }

    pub(super) fn collapse_all_repo_tree_directories(&mut self, cx: &mut Context<Self>) {
        if self.repo_tree.expanded_dirs.is_empty() {
            return;
        }
        self.repo_tree.expanded_dirs.clear();
        self.rebuild_repo_tree_rows();
        cx.notify();
    }

    pub(super) fn open_repo_tree_context_menu(
        &mut self,
        target_path: Option<String>,
        target_kind: RepoTreeNodeKind,
        position: Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.repo_tree_context_menu = Some(RepoTreeContextMenuState {
            target_path,
            target_kind,
            position,
        });
        cx.notify();
    }

    pub(super) fn close_repo_tree_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.repo_tree_context_menu.is_none() {
            return;
        }
        self.repo_tree_context_menu = None;
        cx.notify();
    }

    pub(super) fn inline_repo_tree_new_entry(
        &self,
    ) -> Option<(Option<String>, bool, Entity<InputState>)> {
        let edit = self.repo_tree_inline_edit.as_ref()?;
        match &edit.action {
            RepoTreePromptAction::CreateFile { base_dir } => {
                Some((base_dir.clone(), false, edit.input_state.clone()))
            }
            RepoTreePromptAction::CreateFolder { base_dir } => {
                Some((base_dir.clone(), true, edit.input_state.clone()))
            }
            RepoTreePromptAction::RenameFile { .. } => None,
        }
    }

    pub(super) fn inline_repo_tree_rename_input_for_path(
        &self,
        path: &str,
    ) -> Option<Entity<InputState>> {
        let edit = self.repo_tree_inline_edit.as_ref()?;
        match &edit.action {
            RepoTreePromptAction::RenameFile { path: rename_path } if rename_path == path => {
                Some(edit.input_state.clone())
            }
            _ => None,
        }
    }

    fn selected_repo_tree_file_target(&self) -> Option<(String, RepoTreeNodeKind)> {
        self.selected_path
            .as_ref()
            .map(|path| (path.clone(), RepoTreeNodeKind::File))
    }

    fn ensure_repo_tree_manageable(&mut self, cx: &mut Context<Self>) -> bool {
        if self.workspace_view_mode != WorkspaceViewMode::Files {
            let message = "File management is only available in Files view.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return false;
        }
        if self.repo_root.is_none() {
            let message = "No repository is open.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return false;
        }
        true
    }

    fn start_repo_tree_inline_edit(
        &mut self,
        action: RepoTreePromptAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.repo_tree_context_menu = None;

        let (placeholder, initial_value) = match &action {
            RepoTreePromptAction::CreateFile { .. } => ("e.g. notes.md", String::new()),
            RepoTreePromptAction::CreateFolder { .. } => ("e.g. docs", String::new()),
            RepoTreePromptAction::RenameFile { path } => {
                ("New file name", file_name_from_repo_path(path).to_string())
            }
        };

        let prompt_input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        prompt_input.update(cx, |input, cx| {
            if !initial_value.is_empty() {
                input.set_value(initial_value, window, cx);
            }
        });

        let input_for_events = prompt_input.clone();
        cx.subscribe(&prompt_input, move |this, _, event, cx| match event {
            InputEvent::PressEnter { secondary } if !secondary => {
                if this.repo_tree_inline_edit.as_ref().is_some_and(|edit| {
                    edit.input_state.entity_id() == input_for_events.entity_id()
                }) {
                    this.submit_repo_tree_inline_edit(cx);
                }
            }
            InputEvent::Blur => {
                if this.repo_tree_inline_edit.as_ref().is_some_and(|edit| {
                    edit.input_state.entity_id() == input_for_events.entity_id()
                }) {
                    this.cancel_repo_tree_inline_edit(cx);
                }
            }
            _ => {}
        })
        .detach();

        self.repo_tree_inline_edit = Some(RepoTreeInlineEditState {
            action,
            input_state: prompt_input,
        });
        if let Some(edit) = self.repo_tree_inline_edit.as_ref() {
            edit.input_state.update(cx, |input, cx| {
                input.focus(window, cx);
            });
        }
        cx.notify();
    }

    pub(super) fn submit_repo_tree_inline_edit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(edit_state) = self.repo_tree_inline_edit.clone() else {
            return false;
        };
        let raw_value = edit_state.input_state.read(cx).value().to_string();
        let value = raw_value.trim();
        if value.is_empty() {
            let message = "Path cannot be empty.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return false;
        }

        let result = match edit_state.action {
            RepoTreePromptAction::CreateFile { base_dir } => {
                self.create_repo_tree_file(base_dir.as_deref(), value, cx)
            }
            RepoTreePromptAction::CreateFolder { base_dir } => {
                self.create_repo_tree_folder(base_dir.as_deref(), value, cx)
            }
            RepoTreePromptAction::RenameFile { path } => {
                self.rename_repo_tree_file(path.as_str(), value, cx)
            }
        };

        match result {
            Ok(message) => {
                self.repo_tree_inline_edit = None;
                self.git_status_message = Some(message);
                cx.notify();
                true
            }
            Err(err) => {
                let message = err.to_string();
                self.git_status_message = Some(message.clone());
                Self::push_warning_notification(message, cx);
                cx.notify();
                false
            }
        }
    }

    pub(super) fn cancel_repo_tree_inline_edit(&mut self, cx: &mut Context<Self>) {
        if self.repo_tree_inline_edit.is_none() {
            return;
        }
        self.repo_tree_inline_edit = None;
        cx.notify();
    }

    fn create_repo_tree_file(
        &mut self,
        base_dir: Option<&str>,
        requested_path: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<String> {
        let Some(repo_root) = self.repo_root.clone() else {
            anyhow::bail!("No repository is open.");
        };
        let relative_path = join_repo_relative(base_dir, requested_path)?;
        fs_create_repo_tree_file(&repo_root, &relative_path)?;
        self.expand_repo_tree_ancestors(relative_path.as_str());

        if !self.prevent_unsaved_editor_discard(Some(relative_path.as_str()), cx) {
            self.selected_path = Some(relative_path.clone());
            self.selected_status = None;
            if self.workspace_view_mode == WorkspaceViewMode::Files {
                self.request_file_editor_reload(relative_path.clone(), cx);
            }
        }
        self.refresh_after_repo_tree_fs_mutation(cx);
        Ok(format!("Created {}", relative_path))
    }

    fn create_repo_tree_folder(
        &mut self,
        base_dir: Option<&str>,
        requested_path: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<String> {
        let Some(repo_root) = self.repo_root.clone() else {
            anyhow::bail!("No repository is open.");
        };
        let relative_path = join_repo_relative(base_dir, requested_path)?;
        fs_create_repo_tree_directory(&repo_root, &relative_path)?;
        self.expand_repo_tree_ancestors(relative_path.as_str());
        self.repo_tree.expanded_dirs.insert(relative_path.clone());
        self.refresh_after_repo_tree_fs_mutation(cx);
        Ok(format!("Created folder {}", relative_path))
    }

    fn rename_repo_tree_file(
        &mut self,
        source_path: &str,
        requested_name: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<String> {
        let Some(repo_root) = self.repo_root.clone() else {
            anyhow::bail!("No repository is open.");
        };
        let destination_path = rename_destination_path(source_path, requested_name)?;
        if source_path == destination_path {
            anyhow::bail!("File name is unchanged.");
        }
        if self.prevent_unsaved_editor_discard(Some(destination_path.as_str()), cx) {
            anyhow::bail!("Save current changes before renaming a file.");
        }

        fs_rename_repo_tree_file(&repo_root, source_path, destination_path.as_str())?;
        self.expand_repo_tree_ancestors(destination_path.as_str());

        if self.selected_path.as_deref() == Some(source_path) {
            self.selected_path = Some(destination_path.clone());
            self.selected_status = None;
        }
        if self.editor_path.as_deref() == Some(source_path) {
            self.request_file_editor_reload(destination_path.clone(), cx);
        }

        self.refresh_after_repo_tree_fs_mutation(cx);
        Ok(format!("Renamed {} to {}", source_path, destination_path))
    }

    fn expand_repo_tree_ancestors(&mut self, path: &str) {
        if let Some(parent) = repo_relative_parent_dir(path) {
            let mut current = PathBuf::new();
            for component in Path::new(parent.as_str()).components() {
                if let Component::Normal(part) = component {
                    current.push(part);
                    self.repo_tree
                        .expanded_dirs
                        .insert(repo_relative_path_from_pathbuf(&current));
                }
            }
        }
    }

    fn refresh_after_repo_tree_fs_mutation(&mut self, cx: &mut Context<Self>) {
        self.request_snapshot_refresh_internal(true, cx);
        self.request_repo_tree_reload(cx);
    }

    pub(super) fn request_repo_tree_reload(&mut self, cx: &mut Context<Self>) {
        let Some(repo_root) = self.repo_root.clone() else {
            self.repo_tree.nodes.clear();
            self.repo_tree.rows.clear();
            self.repo_tree.file_count = 0;
            self.repo_tree.folder_count = 0;
            self.repo_tree.expanded_dirs.clear();
            self.repo_tree.scroll_anchor_path = None;
            self.repo_tree.row_count = 0;
            self.repo_tree.list_state.reset(0);
            self.clear_full_repo_tree_cache();
            self.repo_tree.loading = false;
            self.repo_tree.reload_pending = false;
            self.repo_tree.error = None;
            self.repo_tree.changed_only = false;
            self.repo_tree.last_reload = Instant::now();
            cx.notify();
            return;
        };

        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.next_repo_tree_epoch();
            self.repo_tree.task = Task::ready(());
            self.repo_tree.loading = false;
            self.repo_tree.reload_pending = false;
            self.repo_tree.error = None;
            self.repo_tree.changed_only = true;
            self.repo_tree.last_reload = std::time::Instant::now();
            self.rebuild_repo_tree_for_changed_files();
            cx.notify();
            return;
        }

        if self.repo_tree.loading {
            self.repo_tree.reload_pending = true;
            return;
        }

        let epoch = self.next_repo_tree_epoch();
        self.repo_tree.loading = true;
        self.repo_tree.reload_pending = false;
        self.repo_tree.error = None;
        self.repo_tree.changed_only = false;
        self.repo_tree.last_reload = std::time::Instant::now();

        self.repo_tree.task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_repo_tree(&repo_root) })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.repo_tree.epoch {
                        return;
                    }

                    self::apply_repo_tree_reload(this, result, cx);
                });
            }
        });
    }

    fn next_repo_tree_epoch(&mut self) -> usize {
        self.repo_tree.epoch = self.repo_tree.epoch.saturating_add(1);
        self.repo_tree.epoch
    }

    fn rebuild_repo_tree_rows(&mut self) {
        self.capture_sidebar_repo_scroll_anchor();
        self.repo_tree.rows = flatten_repo_tree_rows(&self.repo_tree.nodes, &self.repo_tree.expanded_dirs);
    }

    fn rebuild_repo_tree_for_changed_files(&mut self) {
        self.repo_tree.nodes = build_changed_files_tree(&self.files);
        self.repo_tree.file_count = count_repo_tree_kind(&self.repo_tree.nodes, RepoTreeNodeKind::File);
        self.repo_tree.folder_count =
            count_repo_tree_kind(&self.repo_tree.nodes, RepoTreeNodeKind::Directory);
        self.repo_tree.expanded_dirs.clear();
        self.rebuild_repo_tree_rows();
    }

    fn sync_full_repo_tree_cache_from_current(&mut self) {
        self.repo_tree.full_cache = Some(RepoTreeCacheState {
            nodes: self.repo_tree.nodes.clone(),
            file_count: self.repo_tree.file_count,
            folder_count: self.repo_tree.folder_count,
            expanded_dirs: self.repo_tree.expanded_dirs.clone(),
            error: self.repo_tree.error.clone(),
            scroll_anchor_path: self.repo_tree.scroll_anchor_path.clone(),
            fingerprint: self.last_snapshot_fingerprint.clone(),
        });
    }

    fn restore_full_repo_tree_from_cache(&mut self) -> bool {
        let Some(cache) = self.repo_tree.full_cache.as_ref() else {
            return false;
        };
        if cache.fingerprint != self.last_snapshot_fingerprint {
            return false;
        }

        self.repo_tree.nodes = cache.nodes.clone();
        self.repo_tree.file_count = cache.file_count;
        self.repo_tree.folder_count = cache.folder_count;
        self.repo_tree.expanded_dirs = cache.expanded_dirs.clone();
        self.repo_tree.error = cache.error.clone();
        self.repo_tree.rows = flatten_repo_tree_rows(&self.repo_tree.nodes, &self.repo_tree.expanded_dirs);
        self.repo_tree.scroll_anchor_path = cache.scroll_anchor_path.clone();
        self.repo_tree.row_count = 0;
        self.repo_tree.loading = false;
        self.repo_tree.reload_pending = false;
        self.repo_tree.changed_only = false;
        true
    }

    pub(super) fn clear_full_repo_tree_cache(&mut self) {
        self.repo_tree.full_cache = None;
    }
}

fn apply_repo_tree_reload(
    this: &mut DiffViewer,
    result: anyhow::Result<Vec<hunk_jj::jj::RepoTreeEntry>>,
    cx: &mut Context<DiffViewer>,
) {
    this.repo_tree.loading = false;
    match result {
        Ok(entries) => {
            let (file_count, folder_count) = count_non_ignored_repo_tree_entries(&entries);
            this.repo_tree.nodes = build_repo_tree(&entries);
            this.repo_tree.file_count = file_count;
            this.repo_tree.folder_count = folder_count;
            this.repo_tree.error = None;
            this.repo_tree.changed_only = false;
            this.repo_tree.expanded_dirs
                .retain(|path| repo_tree_has_directory(&this.repo_tree.nodes, path.as_str()));
            this.rebuild_repo_tree_rows();
            if let Some(path) = this.selected_path.clone()
                && this.workspace_view_mode == WorkspaceViewMode::Files
                && !repo_tree_contains_path(&this.repo_tree.nodes, path.as_str())
                && !this.prevent_unsaved_editor_discard(None, cx)
            {
                this.clear_editor_state(cx);
                this.selected_path = None;
                this.selected_status = None;
            }
            if this.workspace_view_mode != WorkspaceViewMode::Diff {
                this.sync_full_repo_tree_cache_from_current();
            }
        }
        Err(err) => {
            this.repo_tree.error = Some(format!("Failed to load repository tree: {err:#}"));
            this.repo_tree.nodes.clear();
            this.repo_tree.rows.clear();
            this.repo_tree.file_count = 0;
            this.repo_tree.folder_count = 0;
            this.repo_tree.expanded_dirs.clear();
            this.repo_tree.changed_only = false;
            this.repo_tree.scroll_anchor_path = None;
            this.repo_tree.row_count = 0;
            this.repo_tree.list_state.reset(0);
        }
    }

    if this.repo_tree.reload_pending {
        this.repo_tree.reload_pending = false;
        this.request_repo_tree_reload(cx);
        return;
    }

    cx.notify();
}

fn repo_tree_contains_path(nodes: &[RepoTreeNode], path: &str) -> bool {
    for node in nodes {
        if node.path == path {
            return true;
        }
        if repo_tree_contains_path(&node.children, path) {
            return true;
        }
    }
    false
}

fn repo_tree_has_directory(nodes: &[RepoTreeNode], path: &str) -> bool {
    for node in nodes {
        if node.kind == RepoTreeNodeKind::Directory && node.path == path {
            return true;
        }
        if repo_tree_has_directory(&node.children, path) {
            return true;
        }
    }
    false
}

fn repo_tree_base_dir(path: &str, kind: RepoTreeNodeKind) -> anyhow::Result<Option<String>> {
    match kind {
        RepoTreeNodeKind::Directory => Ok(Some(normalize_repo_relative_path_str(path)?)),
        RepoTreeNodeKind::File => Ok(repo_relative_parent_dir(path)),
    }
}

fn normalize_repo_relative_path(path: &str) -> anyhow::Result<PathBuf> {
    let raw = path.trim();
    if raw.is_empty() {
        anyhow::bail!("Path cannot be empty.");
    }
    let candidate = Path::new(raw);
    if candidate.is_absolute() {
        anyhow::bail!("Path must be relative to the repository root.");
    }

    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => anyhow::bail!("Path cannot contain `..`."),
            Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("Path must be relative to the repository root.")
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        anyhow::bail!("Path cannot be empty.");
    }
    Ok(normalized)
}

fn normalize_repo_relative_path_str(path: &str) -> anyhow::Result<String> {
    Ok(repo_relative_path_from_pathbuf(&normalize_repo_relative_path(path)?))
}

fn repo_relative_path_from_pathbuf(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn repo_relative_parent_dir(path: &str) -> Option<String> {
    let normalized = normalize_repo_relative_path(path).ok()?;
    let parent = normalized.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some(repo_relative_path_from_pathbuf(parent))
}

fn file_name_from_repo_path(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

fn join_repo_relative(base_dir: Option<&str>, requested_path: &str) -> anyhow::Result<String> {
    let requested = normalize_repo_relative_path(requested_path)?;
    let joined = if let Some(base_dir) = base_dir {
        let mut base = normalize_repo_relative_path(base_dir)?;
        base.push(requested);
        base
    } else {
        requested
    };
    Ok(repo_relative_path_from_pathbuf(&joined))
}

fn rename_destination_path(source_path: &str, requested_name: &str) -> anyhow::Result<String> {
    let trimmed = requested_name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("New file name cannot be empty.");
    }
    let candidate = Path::new(trimmed);
    if candidate.components().count() != 1 {
        anyhow::bail!("Rename expects a file name, not a path.");
    }
    let file_name = candidate
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid file name."))?;

    let source = normalize_repo_relative_path(source_path)?;
    let Some(parent) = source.parent() else {
        anyhow::bail!("Cannot resolve parent directory for `{source_path}`.");
    };
    let destination = if parent.as_os_str().is_empty() {
        PathBuf::from(file_name)
    } else {
        parent.join(file_name)
    };
    Ok(repo_relative_path_from_pathbuf(&destination))
}

fn fs_create_repo_tree_file(repo_root: &Path, relative_path: &str) -> anyhow::Result<()> {
    let normalized = normalize_repo_relative_path(relative_path)?;
    let absolute = repo_root.join(&normalized);
    if absolute.exists() {
        anyhow::bail!("`{}` already exists.", normalized.display());
    }
    let parent = absolute.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot create `{}` because parent directory is unavailable.",
            normalized.display()
        )
    })?;
    if !parent.exists() {
        anyhow::bail!(
            "Parent directory does not exist for `{}`.",
            normalized.display()
        );
    }

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&absolute)
        .with_context(|| format!("failed to create {}", absolute.display()))?;
    Ok(())
}

fn fs_create_repo_tree_directory(repo_root: &Path, relative_path: &str) -> anyhow::Result<()> {
    let normalized = normalize_repo_relative_path(relative_path)?;
    let absolute = repo_root.join(&normalized);
    if absolute.exists() {
        anyhow::bail!("`{}` already exists.", normalized.display());
    }
    std::fs::create_dir_all(&absolute)
        .with_context(|| format!("failed to create {}", absolute.display()))?;
    Ok(())
}

fn fs_rename_repo_tree_file(
    repo_root: &Path,
    source_path: &str,
    destination_path: &str,
) -> anyhow::Result<()> {
    let source = normalize_repo_relative_path(source_path)?;
    let destination = normalize_repo_relative_path(destination_path)?;
    let source_absolute = repo_root.join(&source);
    let destination_absolute = repo_root.join(&destination);

    if !source_absolute.is_file() {
        anyhow::bail!("`{}` is not a file.", source.display());
    }
    if destination_absolute.exists() {
        anyhow::bail!("`{}` already exists.", destination.display());
    }

    let destination_parent = destination_absolute.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot rename `{}` because destination parent is unavailable.",
            source.display()
        )
    })?;
    if !destination_parent.exists() {
        anyhow::bail!(
            "Destination parent directory does not exist for `{}`.",
            destination.display()
        );
    }

    std::fs::rename(&source_absolute, &destination_absolute).with_context(|| {
        format!(
            "failed to rename {} to {}",
            source_absolute.display(),
            destination_absolute.display()
        )
    })?;
    Ok(())
}

fn fs_delete_repo_tree_file(repo_root: &Path, path: &str) -> anyhow::Result<()> {
    let normalized = normalize_repo_relative_path(path)?;
    let absolute = repo_root.join(&normalized);
    if !absolute.is_file() {
        anyhow::bail!("`{}` is not a file.", normalized.display());
    }
    std::fs::remove_file(&absolute)
        .with_context(|| format!("failed to delete {}", absolute.display()))?;
    Ok(())
}
