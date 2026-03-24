impl DiffViewer {
    fn active_file_editor_tab_index(&self) -> Option<usize> {
        let active_id = self.active_file_editor_tab_id?;
        self.file_editor_tabs.iter().position(|tab| tab.id == active_id)
    }

    fn file_editor_tab_index_for_id(&self, tab_id: usize) -> Option<usize> {
        self.file_editor_tabs.iter().position(|tab| tab.id == tab_id)
    }

    fn file_editor_tab_index_for_path(&self, path: &str) -> Option<usize> {
        self.file_editor_tabs
            .iter()
            .position(|tab| tab.path == path)
    }

    pub(super) fn sync_active_file_editor_tab_state(&mut self) {
        let Some(tab_index) = self.active_file_editor_tab_index() else {
            return;
        };
        let tab = &mut self.file_editor_tabs[tab_index];
        if let Some(path) = self.editor_path.clone() {
            tab.path = path;
        }
        tab.files_editor = self.files_editor.clone();
        tab.loading = self.editor_loading;
        tab.error = self.editor_error.clone();
        tab.dirty = self.editor_dirty;
        tab.last_saved_text = self.editor_last_saved_text.clone();
        tab.reload_epoch = self.editor_epoch;
        tab.reload_task = std::mem::replace(&mut self.editor_task, Task::ready(()));
        tab.save_loading = self.editor_save_loading;
        tab.save_epoch = self.editor_save_epoch;
        tab.save_task = std::mem::replace(&mut self.editor_save_task, Task::ready(()));
        tab.markdown_preview_task =
            std::mem::replace(&mut self.editor_markdown_preview_task, Task::ready(()));
        tab.markdown_preview_blocks = std::mem::take(&mut self.editor_markdown_preview_blocks);
        tab.markdown_preview_loading = self.editor_markdown_preview_loading;
        tab.markdown_preview_revision = self.editor_markdown_preview_revision;
        tab.markdown_preview = self.editor_markdown_preview;
    }

    fn restore_file_editor_tab_state(&mut self, tab_index: usize) {
        let tab = &mut self.file_editor_tabs[tab_index];
        self.active_file_editor_tab_id = Some(tab.id);
        self.files_editor = tab.files_editor.clone();
        self.editor_path = Some(tab.path.clone());
        self.editor_loading = tab.loading;
        self.editor_error = tab.error.clone();
        self.editor_dirty = tab.dirty;
        self.editor_last_saved_text = tab.last_saved_text.clone();
        self.editor_task = std::mem::replace(&mut tab.reload_task, Task::ready(()));
        self.editor_save_loading = tab.save_loading;
        self.editor_save_task = std::mem::replace(&mut tab.save_task, Task::ready(()));
        self.editor_markdown_preview_task =
            std::mem::replace(&mut tab.markdown_preview_task, Task::ready(()));
        self.editor_markdown_preview_blocks = std::mem::take(&mut tab.markdown_preview_blocks);
        self.editor_markdown_preview_loading = tab.markdown_preview_loading;
        self.editor_markdown_preview_revision = tab.markdown_preview_revision;
        self.editor_markdown_preview = tab.markdown_preview;
    }

    fn reset_active_file_editor_session(&mut self) {
        self.active_file_editor_tab_id = None;
        self.files_editor = Rc::new(RefCell::new(crate::app::native_files_editor::FilesEditor::new()));
        self.editor_path = None;
        self.editor_loading = false;
        self.editor_error = None;
        self.editor_dirty = false;
        self.editor_last_saved_text = None;
        self.editor_task = Task::ready(());
        self.editor_save_loading = false;
        self.editor_save_task = Task::ready(());
        self.editor_markdown_preview_task = Task::ready(());
        self.editor_markdown_preview_blocks.clear();
        self.editor_markdown_preview_loading = false;
        self.editor_markdown_preview_revision = 0;
        self.editor_markdown_preview = false;
        self.editor_search_visible = false;
    }

    fn create_file_editor_tab(&mut self, path: String) -> usize {
        let tab_id = self.next_file_editor_tab_id;
        self.next_file_editor_tab_id = self.next_file_editor_tab_id.saturating_add(1);
        self.file_editor_tabs.push(FileEditorTab::new(tab_id, path));
        self.file_editor_tabs.len().saturating_sub(1)
    }

    fn oldest_recyclable_file_editor_tab_index(&self) -> Option<usize> {
        self.file_editor_tabs.iter().enumerate().find_map(|(index, tab)| {
            let is_active = self.active_file_editor_tab_id == Some(tab.id);
            (!is_active && !tab.dirty && !tab.save_loading && !tab.loading).then_some(index)
        })
    }

    fn ensure_file_editor_tab_index(&mut self, path: &str) -> Option<usize> {
        if let Some(tab_index) = self.file_editor_tab_index_for_path(path) {
            return Some(tab_index);
        }

        if self.file_editor_tabs.len() >= FILE_EDITOR_TAB_LIMIT {
            let Some(tab_index) = self.oldest_recyclable_file_editor_tab_index() else {
                self.git_status_message = Some(
                    "Tab limit reached. Save or close another file tab before opening a new one."
                        .to_string(),
                );
                return None;
            };
            let removed = self.file_editor_tabs.remove(tab_index);
            removed.files_editor.borrow_mut().shutdown();
        }

        Some(self.create_file_editor_tab(path.to_string()))
    }

    fn activate_file_editor_tab_index(
        &mut self,
        tab_index: usize,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.sync_active_file_editor_tab_state();
        self.restore_file_editor_tab_state(tab_index);
        self.file_editor_tab_scroll_handle.scroll_to_item(tab_index);
        self.sync_editor_search_query(cx);
        if let Some(window) = window {
            self.files_editor_focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn close_active_file_editor_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self.active_file_editor_tab_index() else {
            return;
        };

        self.sync_active_file_editor_tab_state();
        let tab = &self.file_editor_tabs[tab_index];
        if tab.save_loading {
            self.git_status_message =
                Some(format!("Save in progress for {}. Wait before closing the tab.", tab.path));
            cx.notify();
            return;
        }
        if tab.dirty {
            self.git_status_message =
                Some(format!("Unsaved changes in {}. Save before closing the tab.", tab.path));
            cx.notify();
            return;
        }

        let removed = self.file_editor_tabs.remove(tab_index);
        removed.files_editor.borrow_mut().shutdown();
        drop(removed);

        if self.file_editor_tabs.is_empty() {
            self.reset_active_file_editor_session();
            self.selected_path = None;
            self.selected_status = None;
        } else {
            let next_index = tab_index.min(self.file_editor_tabs.len().saturating_sub(1));
            self.restore_file_editor_tab_state(next_index);
            self.selected_path = self.editor_path.clone();
            self.selected_status = self
                .editor_path
                .as_deref()
                .and_then(|path| self.status_for_path(path));
            self.sync_editor_search_query(cx);
            self.files_editor_focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn close_file_editor_tab_by_id(
        &mut self,
        tab_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self.file_editor_tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        if self.active_file_editor_tab_id == Some(tab_id) {
            self.close_active_file_editor_tab(window, cx);
            return;
        }

        let tab = &self.file_editor_tabs[tab_index];
        if tab.save_loading {
            self.git_status_message =
                Some(format!("Save in progress for {}. Wait before closing the tab.", tab.path));
            cx.notify();
            return;
        }
        if tab.dirty {
            self.git_status_message =
                Some(format!("Unsaved changes in {}. Save before closing the tab.", tab.path));
            cx.notify();
            return;
        }

        let removed = self.file_editor_tabs.remove(tab_index);
        removed.files_editor.borrow_mut().shutdown();
        cx.notify();
    }

    fn close_file_editor_tabs_for_path(&mut self, path: &str) {
        self.sync_active_file_editor_tab_state();
        let active_id = self.active_file_editor_tab_id;
        self.file_editor_tabs.retain_mut(|tab| {
            let retain = tab.path != path;
            if !retain {
                tab.files_editor.borrow_mut().shutdown();
            }
            retain
        });

        if active_id.is_some()
            && self
                .file_editor_tabs
                .iter()
                .all(|tab| Some(tab.id) != active_id)
        {
            if self.file_editor_tabs.is_empty() {
                self.reset_active_file_editor_session();
            } else {
                self.restore_file_editor_tab_state(0);
            }
        }
    }

    fn sync_file_editor_tab_path(&mut self, source_path: &str, destination_path: &str) {
        self.sync_active_file_editor_tab_state();
        for tab in &mut self.file_editor_tabs {
            if tab.path == source_path {
                tab.path = destination_path.to_string();
            }
        }
        if self.editor_path.as_deref() == Some(source_path) {
            self.editor_path = Some(destination_path.to_string());
        }
    }

    fn prevent_file_editor_tab_discard_for_path(
        &mut self,
        path: &str,
        action: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.editor_path.as_deref() == Some(path) {
            self.sync_editor_dirty_from_input(cx);
        }
        self.sync_active_file_editor_tab_state();

        if let Some(tab) = self
            .file_editor_tabs
            .iter()
            .find(|tab| tab.path == path && tab.save_loading)
        {
            self.git_status_message = Some(format!(
                "Save in progress for {}. Wait before {}.",
                tab.path, action
            ));
            cx.notify();
            return true;
        }

        if let Some(tab) = self.file_editor_tabs.iter().find(|tab| tab.path == path && tab.dirty) {
            self.git_status_message = Some(format!(
                "Unsaved changes in {}. Save before {}.",
                tab.path, action
            ));
            cx.notify();
            return true;
        }

        false
    }

    pub(super) fn view_current_review_file_action(
        &mut self,
        _: &ViewCurrentReviewFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }

        let Some(path) = self.selected_path.clone() else {
            self.set_git_warning_message("No review file is selected.".to_string(), Some(window), cx);
            return;
        };
        let status = self
            .selected_status
            .or_else(|| self.status_for_path(path.as_str()))
            .unwrap_or(FileStatus::Unknown);

        self.focus_handle.focus(window, cx);
        let _ = self.open_file_in_files_workspace(path, status, window, cx);
    }

    pub(super) fn save_current_file_action(
        &mut self,
        _: &SaveCurrentFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Files {
            return;
        }
        self.save_current_editor_file(window, cx);
    }

    pub(super) fn next_editor_tab_action(
        &mut self,
        _: &NextEditorTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Files || self.file_editor_tabs.len() < 2 {
            return;
        }

        let next_index = self
            .active_file_editor_tab_index()
            .map(|index| (index + 1) % self.file_editor_tabs.len())
            .unwrap_or(0);
        self.activate_file_editor_tab_index(next_index, Some(window), cx);
    }

    pub(super) fn previous_editor_tab_action(
        &mut self,
        _: &PreviousEditorTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Files || self.file_editor_tabs.len() < 2 {
            return;
        }

        let previous_index = self
            .active_file_editor_tab_index()
            .map(|index| {
                if index == 0 {
                    self.file_editor_tabs.len().saturating_sub(1)
                } else {
                    index.saturating_sub(1)
                }
            })
            .unwrap_or(0);
        self.activate_file_editor_tab_index(previous_index, Some(window), cx);
    }

    pub(super) fn close_editor_tab_action(
        &mut self,
        _: &CloseEditorTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Files {
            return;
        }
        self.close_active_file_editor_tab(window, cx);
    }

    pub(super) fn reload_current_editor_file(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.editor_path.clone() else {
            return;
        };
        if self.prevent_unsaved_editor_discard(Some(path.as_str()), cx) {
            return;
        }

        self.request_file_editor_reload(path, cx);
    }

    pub(super) fn request_file_editor_reload(&mut self, path: String, cx: &mut Context<Self>) -> bool {
        let Some(tab_index) = self.ensure_file_editor_tab_index(path.as_str()) else {
            cx.notify();
            return false;
        };
        self.activate_file_editor_tab_index(tab_index, None, cx);
        let Some(tab_id) = self.active_file_editor_tab_id else {
            return false;
        };

        let retain_markdown_preview = if self.editor_path.as_deref() == Some(path.as_str()) {
            self.editor_markdown_preview
        } else {
            false
        };
        let Some(repo_root) = self.repo_root.clone() else {
            self.editor_loading = false;
            self.editor_error = Some("No repository is open.".to_string());
            self.editor_last_saved_text = None;
            self.editor_dirty = false;
            self.editor_markdown_preview = false;
            self.invalidate_editor_markdown_preview();
            self.files_editor.borrow_mut().clear();
            self.sync_active_file_editor_tab_state();
            cx.notify();
            return true;
        };

        let epoch = self.next_editor_epoch();
        self.cancel_editor_task();
        self.editor_loading = true;
        self.editor_error = None;
        self.editor_path = Some(path.clone());
        self.editor_markdown_preview =
            is_markdown_path(path.as_str()) && retain_markdown_preview;
        self.invalidate_editor_markdown_preview();
        self.sync_active_file_editor_tab_state();
        cx.notify();

        self.editor_task = cx.spawn(async move |this, cx| {
            let target_path = path.clone();
            let repo_root_for_load = repo_root.clone();
            let result = cx.background_executor().spawn(async move {
                load_file_editor_document(
                    &repo_root_for_load,
                    target_path.as_str(),
                    FILE_EDITOR_MAX_BYTES,
                )
            });
            let result = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    let Some(tab_index) = this.file_editor_tab_index_for_id(tab_id) else {
                        return;
                    };
                    if epoch != this.file_editor_tabs[tab_index].reload_epoch {
                        return;
                    }

                    let is_active = this.active_file_editor_tab_id == Some(tab_id);
                    let tab_editor = this.file_editor_tabs[tab_index].files_editor.clone();

                    match result {
                        Ok(document) => {
                            let text = document.text;
                            let open_result = Self::open_files_editor_document_in(
                                &tab_editor,
                                path.as_str(),
                                &repo_root,
                                text.as_str(),
                            );
                            let should_schedule_preview = {
                                let tab = &mut this.file_editor_tabs[tab_index];
                                tab.loading = false;
                                tab.last_saved_text = Some(text.clone());
                                tab.dirty = false;
                                tab.error = None;
                                if let Err(err) = open_result {
                                    tab.error =
                                        Some(format!("File editor failed to open {}: {err:#}", path));
                                    tab.files_editor.borrow_mut().clear();
                                    false
                                } else {
                                    tab.markdown_preview
                                }
                            };
                            if is_active {
                                this.restore_file_editor_tab_state(tab_index);
                                this.sync_editor_search_query(cx);
                                this.focus_files_editor(cx);
                                if should_schedule_preview {
                                    this.schedule_editor_markdown_preview_parse(cx);
                                }
                            }
                        }
                        Err(err) => {
                            {
                                let tab = &mut this.file_editor_tabs[tab_index];
                                tab.loading = false;
                                tab.last_saved_text = None;
                                tab.dirty = false;
                                tab.error = Some(format!("Editor unavailable: {err}"));
                                tab.files_editor.borrow_mut().clear();
                            }
                            if is_active {
                                this.restore_file_editor_tab_state(tab_index);
                                this.sync_editor_search_query(cx);
                            }
                        }
                    }

                    cx.notify();
                });
            }
        });
        true
    }

    pub(super) fn can_open_file_in_files_workspace(
        &self,
        path: &str,
        status: FileStatus,
    ) -> bool {
        status != FileStatus::Deleted && self.path_exists_in_primary_checkout(path)
    }

    pub(super) fn open_file_in_files_workspace(
        &mut self,
        path: String,
        status: FileStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_open_file_in_files_workspace(path.as_str(), status) {
            let message = if status == FileStatus::Deleted {
                format!("{path} was deleted in this review and can't be opened in Files view.")
            } else {
                format!("{path} isn't available in the current workspace.")
            };
            self.set_git_warning_message(message, Some(window), cx);
            return false;
        }

        if self.workspace_view_mode != WorkspaceViewMode::Files {
            self.set_workspace_view_mode(WorkspaceViewMode::Files, cx);
            if self.workspace_view_mode != WorkspaceViewMode::Files {
                return false;
            }
        }

        self.selected_path = Some(path.clone());
        self.selected_status = self.status_for_path(path.as_str()).or(Some(status));

        if let Some(tab_index) = self.file_editor_tab_index_for_path(path.as_str()) {
            self.activate_file_editor_tab_index(tab_index, Some(window), cx);
            if (self.editor_loading || self.editor_error.is_some())
                && !self.request_file_editor_reload(path.clone(), cx)
            {
                return false;
            }
        } else if !self.request_file_editor_reload(path.clone(), cx) {
            return false;
        }

        self.files_editor_focus_handle.focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn save_current_editor_file(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_loading || self.editor_save_loading {
            return;
        }

        let Some(repo_root) = self.repo_root.clone() else {
            self.git_status_message = Some("No Git repository available.".to_string());
            cx.notify();
            return;
        };
        let Some(path) = self.editor_path.clone() else {
            self.git_status_message = Some("No file is open in editor.".to_string());
            cx.notify();
            return;
        };

        self.sync_editor_dirty_from_input(cx);
        let current_text = match self.current_editor_text() {
            Ok(text) => text,
            Err(err) => {
                self.editor_error = Some(format!("Editor unavailable: {err:#}"));
                self.git_status_message = Some(format!("Save blocked for {path}: {err:#}"));
                cx.notify();
                return;
            }
        };
        if !self.editor_dirty {
            self.git_status_message = Some("No unsaved changes.".to_string());
            cx.notify();
            return;
        }

        let text_to_write = current_text.clone();
        let saved_text = current_text;
        let path_for_write = path.clone();
        let status_path = path.clone();
        let epoch = self.next_editor_save_epoch();
        self.cancel_editor_save_task();
        self.editor_save_loading = true;
        self.editor_error = None;
        self.git_status_message = None;
        self.sync_active_file_editor_tab_state();
        cx.notify();
        let Some(tab_id) = self.active_file_editor_tab_id else {
            return;
        };

        self.editor_save_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                save_file_editor_document(&repo_root, path_for_write.as_str(), text_to_write.as_str())
            });
            let result = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    let Some(tab_index) = this.file_editor_tab_index_for_id(tab_id) else {
                        return;
                    };
                    if epoch != this.file_editor_tabs[tab_index].save_epoch {
                        return;
                    }

                    let is_active = this.active_file_editor_tab_id == Some(tab_id);
                    let tab_editor = this.file_editor_tabs[tab_index].files_editor.clone();
                    this.file_editor_tabs[tab_index].save_loading = false;

                    match result {
                        Ok(()) => {
                            {
                                let tab = &mut this.file_editor_tabs[tab_index];
                                tab.last_saved_text = Some(saved_text.clone());
                                tab.dirty = false;
                            }
                            tab_editor.borrow_mut().mark_saved();
                            if is_active {
                                this.restore_file_editor_tab_state(tab_index);
                            }
                            this.git_status_message = Some(format!("Saved {}", status_path));
                            this.request_snapshot_refresh(cx);
                        }
                        Err(err) => {
                            this.git_status_message =
                                Some(format!("Save failed for {}: {err:#}", status_path));
                        }
                    }

                    cx.notify();
                });
            }
        });
    }

    pub(super) fn toggle_editor_markdown_preview(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.editor_path.as_deref() else {
            return;
        };
        if !is_markdown_path(path) {
            self.editor_markdown_preview = false;
            self.clear_editor_markdown_preview_state();
            return;
        }

        self.editor_markdown_preview = !self.editor_markdown_preview;
        if self.editor_markdown_preview {
            self.schedule_editor_markdown_preview_parse(cx);
        } else {
            self.clear_editor_markdown_preview_state();
        }
        cx.notify();
    }

    pub(super) fn sync_editor_dirty_from_input(&mut self, cx: &mut Context<Self>) {
        if self.editor_loading || self.editor_path.is_none() {
            return;
        }

        let current_text = match self.current_editor_text() {
            Ok(text) => text,
            Err(err) => {
                self.editor_error = Some(format!("Editor unavailable: {err:#}"));
                self.editor_dirty = false;
                self.clear_editor_markdown_preview_state();
                cx.notify();
                return;
            }
        };
        let saved_text = self.editor_last_saved_text.as_deref().unwrap_or_default();
        let dirty =
            self.files_editor.borrow().is_dirty() || current_text.as_str() != saved_text;
        if self.editor_dirty != dirty {
            self.editor_dirty = dirty;
            cx.notify();
        }
        self.schedule_editor_markdown_preview_parse(cx);
    }

    fn invalidate_editor_markdown_preview(&mut self) {
        self.clear_editor_markdown_preview_state();
        self.next_editor_markdown_preview_revision();
    }

    fn next_editor_markdown_preview_revision(&mut self) -> usize {
        self.editor_markdown_preview_revision =
            self.editor_markdown_preview_revision.saturating_add(1);
        self.editor_markdown_preview_revision
    }

    fn schedule_editor_markdown_preview_parse(&mut self, cx: &mut Context<Self>) {
        if !self.editor_markdown_preview {
            self.clear_editor_markdown_preview_state();
            return;
        }

        let Some(path) = self.editor_path.as_deref().map(ToOwned::to_owned) else {
            self.clear_editor_markdown_preview_state();
            return;
        };
        if !is_markdown_path(path.as_str()) || self.editor_loading {
            self.clear_editor_markdown_preview_state();
            return;
        }

        self.cancel_editor_markdown_preview_task();
        let revision = self.next_editor_markdown_preview_revision();
        let Ok(markdown_text) = self.current_editor_text() else {
            self.clear_editor_markdown_preview_state();
            return;
        };
        self.editor_markdown_preview_loading = true;
        cx.notify();

        self.editor_markdown_preview_task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(MARKDOWN_PREVIEW_DEBOUNCE)
                .await;
            let preview_path = path;
            let blocks = cx.background_executor().spawn(async move {
                hunk_domain::markdown_preview::parse_markdown_preview(markdown_text.as_str())
            });
            let blocks = blocks.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if revision != this.editor_markdown_preview_revision {
                        return;
                    }
                    if this.editor_loading
                        || !this.editor_markdown_preview
                        || this.editor_path.as_deref() != Some(preview_path.as_str())
                    {
                        this.editor_markdown_preview_loading = false;
                        return;
                    }

                    this.editor_markdown_preview_blocks = blocks;
                    this.editor_markdown_preview_loading = false;
                    cx.notify();
                });
            }
        });
    }

    fn clear_editor_markdown_preview_state(&mut self) {
        self.cancel_editor_markdown_preview_task();
        self.editor_markdown_preview_blocks.clear();
        self.editor_markdown_preview_loading = false;
    }

    fn cancel_editor_markdown_preview_task(&mut self) {
        let previous_task =
            std::mem::replace(&mut self.editor_markdown_preview_task, Task::ready(()));
        drop(previous_task);
    }

    fn cancel_editor_task(&mut self) {
        let previous_task = std::mem::replace(&mut self.editor_task, Task::ready(()));
        drop(previous_task);
    }

    fn cancel_editor_save_task(&mut self) {
        let previous_task = std::mem::replace(&mut self.editor_save_task, Task::ready(()));
        drop(previous_task);
    }

    fn prevent_unsaved_editor_discard(
        &mut self,
        next_path: Option<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.editor_path.is_none() || self.editor_loading {
            return false;
        }
        if self.editor_save_loading {
            let current_path = self.editor_path.as_deref().unwrap_or_default();
            self.git_status_message = Some(format!(
                "Save in progress for {current_path}. Wait before switching files."
            ));
            cx.notify();
            return true;
        }

        self.sync_editor_dirty_from_input(cx);
        if !self.editor_dirty {
            return false;
        }

        let current_path = self.editor_path.as_deref().unwrap_or_default();
        let message = if next_path == Some(current_path) {
            format!("Unsaved changes in {current_path}. Save before reloading.")
        } else {
            format!("Unsaved changes in {current_path}. Save before switching files.")
        };
        self.git_status_message = Some(message);
        cx.notify();
        true
    }

    pub(super) fn clear_editor_state(&mut self, _cx: &mut Context<Self>) {
        self.sync_active_file_editor_tab_state();
        for tab in &mut self.file_editor_tabs {
            tab.files_editor.borrow_mut().shutdown();
        }
        self.file_editor_tabs.clear();
        self.reset_active_file_editor_session();
    }

    pub(crate) fn current_editor_text(&self) -> anyhow::Result<String> {
        self.files_editor
            .borrow()
            .current_text()
            .ok_or_else(|| anyhow::anyhow!("no active file editor buffer"))
    }

    pub(super) fn files_editor_copy_action(
        &mut self,
        _: &FilesEditorCopy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_markdown_preview || !self.files_editor_focus_handle.is_focused(window) {
            return;
        }
        let Some(text) = self.files_editor.borrow().copy_selection_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub(super) fn files_editor_cut_action(
        &mut self,
        _: &FilesEditorCut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_markdown_preview || !self.files_editor_focus_handle.is_focused(window) {
            return;
        }
        let Some(text) = self.files_editor.borrow_mut().cut_selection_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.sync_editor_dirty_from_input(cx);
        cx.notify();
    }

    pub(super) fn files_editor_paste_action(
        &mut self,
        _: &FilesEditorPaste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_markdown_preview || !self.files_editor_focus_handle.is_focused(window) {
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if self.files_editor.borrow_mut().paste_text(text.as_str()) {
            self.sync_editor_dirty_from_input(cx);
            cx.notify();
        }
    }

    pub(super) fn files_editor_move_up_action(
        &mut self,
        _: &FilesEditorMoveUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_vertical_action(false, false)
        });
    }

    pub(super) fn files_editor_move_down_action(
        &mut self,
        _: &FilesEditorMoveDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_vertical_action(true, false)
        });
    }

    pub(super) fn files_editor_select_up_action(
        &mut self,
        _: &FilesEditorSelectUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_vertical_action(false, true)
        });
    }

    pub(super) fn files_editor_select_down_action(
        &mut self,
        _: &FilesEditorSelectDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_vertical_action(true, true)
        });
    }

    pub(super) fn files_editor_move_left_action(
        &mut self,
        _: &FilesEditorMoveLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_horizontal_action(false, false)
        });
    }

    pub(super) fn files_editor_move_right_action(
        &mut self,
        _: &FilesEditorMoveRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_horizontal_action(true, false)
        });
    }

    pub(super) fn files_editor_select_left_action(
        &mut self,
        _: &FilesEditorSelectLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_horizontal_action(false, true)
        });
    }

    pub(super) fn files_editor_select_right_action(
        &mut self,
        _: &FilesEditorSelectRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_horizontal_action(true, true)
        });
    }

    pub(super) fn files_editor_move_to_beginning_of_line_action(
        &mut self,
        _: &FilesEditorMoveToBeginningOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_to_line_boundary_action(true, false)
        });
    }

    pub(super) fn files_editor_move_to_end_of_line_action(
        &mut self,
        _: &FilesEditorMoveToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_to_line_boundary_action(false, false)
        });
    }

    pub(super) fn files_editor_move_to_beginning_of_document_action(
        &mut self,
        _: &FilesEditorMoveToBeginningOfDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_to_document_boundary_action(true, false)
        });
    }

    pub(super) fn files_editor_move_to_end_of_document_action(
        &mut self,
        _: &FilesEditorMoveToEndOfDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_to_document_boundary_action(false, false)
        });
    }

    pub(super) fn files_editor_select_to_beginning_of_line_action(
        &mut self,
        _: &FilesEditorSelectToBeginningOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_to_line_boundary_action(true, true)
        });
    }

    pub(super) fn files_editor_select_to_end_of_line_action(
        &mut self,
        _: &FilesEditorSelectToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_to_line_boundary_action(false, true)
        });
    }

    pub(super) fn files_editor_select_to_beginning_of_document_action(
        &mut self,
        _: &FilesEditorSelectToBeginningOfDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_to_document_boundary_action(true, true)
        });
    }

    pub(super) fn files_editor_select_to_end_of_document_action(
        &mut self,
        _: &FilesEditorSelectToEndOfDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_to_document_boundary_action(false, true)
        });
    }

    pub(super) fn files_editor_move_to_previous_word_start_action(
        &mut self,
        _: &FilesEditorMoveToPreviousWordStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_word_action(false, false)
        });
    }

    pub(super) fn files_editor_move_to_next_word_end_action(
        &mut self,
        _: &FilesEditorMoveToNextWordEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_word_action(true, false)
        });
    }

    pub(super) fn files_editor_select_to_previous_word_start_action(
        &mut self,
        _: &FilesEditorSelectToPreviousWordStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_word_action(false, true)
        });
    }

    pub(super) fn files_editor_select_to_next_word_end_action(
        &mut self,
        _: &FilesEditorSelectToNextWordEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.move_word_action(true, true)
        });
    }

    pub(super) fn files_editor_page_up_action(
        &mut self,
        _: &FilesEditorPageUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.page_scroll_action(crate::app::native_files_editor::ScrollDirection::Backward)
        });
    }

    pub(super) fn files_editor_page_down_action(
        &mut self,
        _: &FilesEditorPageDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_files_editor_motion(window, cx, |editor| {
            editor.page_scroll_action(crate::app::native_files_editor::ScrollDirection::Forward)
        });
    }

    fn handle_files_editor_motion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&mut crate::app::native_files_editor::FilesEditor) -> bool,
    ) {
        if self.editor_markdown_preview || !self.files_editor_focus_handle.is_focused(window) {
            return;
        }
        if self.files_editor.borrow_mut().apply_motion_action(apply) {
            self.sync_editor_dirty_from_input(cx);
            cx.notify();
        }
    }

    fn open_files_editor_document_in(
        files_editor: &crate::app::native_files_editor::SharedFilesEditor,
        relative_path: &str,
        repo_root: &std::path::Path,
        text: &str,
    ) -> anyhow::Result<()> {
        let absolute_path = repo_root.join(relative_path);
        files_editor.borrow_mut().open_document(&absolute_path, text)?;
        Ok(())
    }

    fn focus_files_editor(&self, cx: &mut Context<Self>) {
        let focus_handle = self.files_editor_focus_handle.clone();
        if let Err(err) = Self::update_any_window(cx, |window, cx| {
            focus_handle.focus(window, cx);
        }) {
            error!("failed to focus files editor: {err:#}");
        }
    }

    fn update_any_window(
        cx: &mut Context<Self>,
        mut update: impl FnMut(&mut Window, &mut App),
    ) -> anyhow::Result<bool> {
        let window_handles = cx.windows().into_iter().collect::<Vec<_>>();
        for window_handle in window_handles {
            match cx.update_window(window_handle, |_, window, cx| update(window, cx)) {
                Ok(()) => return Ok(true),
                Err(err) if Self::is_window_not_found_error(&err) => continue,
                Err(err) => return Err(err),
            }
        }
        Ok(false)
    }

    fn is_window_not_found_error(err: &anyhow::Error) -> bool {
        err.chain()
            .any(|cause| cause.to_string().contains("window not found"))
    }

    pub(crate) fn defer_root_focus(&self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let focus_handle = self.focus_handle.clone();
        cx.defer(move |cx| {
            let result = cx.update_window(window_handle, |_, window, cx| {
                focus_handle.focus(window, cx);
            });
            if let Err(err) = result
                && !Self::is_window_not_found_error(&err)
            {
                error!("failed to restore root diff viewer focus: {err:#}");
            }
        });
    }

    fn next_editor_epoch(&mut self) -> usize {
        self.editor_epoch = self.editor_epoch.saturating_add(1);
        self.editor_epoch
    }

    fn next_editor_save_epoch(&mut self) -> usize {
        self.editor_save_epoch = self.editor_save_epoch.saturating_add(1);
        self.editor_save_epoch
    }

    pub(super) fn sync_editor_search_query(&mut self, cx: &mut Context<Self>) {
        let query = if self.editor_search_visible {
            self.editor_search_input_state.read(cx).value().to_string()
        } else {
            String::new()
        };
        self.files_editor
            .borrow_mut()
            .set_search_query(Some(query.as_str()));
        cx.notify();
    }

    pub(super) fn toggle_editor_search(
        &mut self,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor_search_visible = visible;
        if visible {
            self.editor_search_input_state.update(cx, |state, cx| {
                state.focus(window, cx);
            });
        } else {
            self.editor_search_input_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
            self.editor_replace_input_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
            self.files_editor.borrow_mut().set_search_query(None);
            self.files_editor_focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn toggle_editor_search_visibility(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_editor_search(!self.editor_search_visible, window, cx);
    }

    pub(super) fn navigate_editor_search(
        &mut self,
        forward: bool,
        cx: &mut Context<Self>,
    ) {
        if self.files_editor.borrow_mut().select_next_search_match(forward) {
            cx.notify();
        }
    }

    pub(super) fn replace_current_editor_search_match(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let replacement = self.editor_replace_input_state.read(cx).value().to_string();
        if self
            .files_editor
            .borrow_mut()
            .replace_selected_search_match(replacement.as_str())
        {
            self.sync_editor_dirty_from_input(cx);
            let _ = self.files_editor.borrow_mut().select_next_search_match(true);
            if let Some(window) = window {
                self.files_editor_focus_handle.focus(window, cx);
            }
            self.sync_active_file_editor_tab_state();
            cx.notify();
        }
    }

    pub(super) fn replace_all_editor_search_matches(&mut self, cx: &mut Context<Self>) {
        let replacement = self.editor_replace_input_state.read(cx).value().to_string();
        if self
            .files_editor
            .borrow_mut()
            .replace_all_search_matches(replacement.as_str())
        {
            self.sync_editor_dirty_from_input(cx);
            self.sync_active_file_editor_tab_state();
            cx.notify();
        }
    }
}
