impl DiffViewer {
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
        self.save_current_editor_file(window, cx);
    }

    pub(super) fn reload_current_editor_file(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.editor_path.clone() else {
            return;
        };

        self.request_file_editor_reload(path, cx);
    }

    pub(super) fn request_file_editor_reload(&mut self, path: String, cx: &mut Context<Self>) {
        if self.prevent_unsaved_editor_discard(Some(path.as_str()), cx) {
            return;
        }

        let retain_markdown_preview = if self.editor_path.as_deref() == Some(path.as_str()) {
            self.editor_markdown_preview
        } else {
            false
        };
        let Some(repo_root) = self.repo_root.clone() else {
            self.editor_loading = false;
            self.editor_error = Some("No repository is open.".to_string());
            self.editor_path = None;
            self.editor_last_saved_text = None;
            self.editor_dirty = false;
            self.editor_markdown_preview = false;
            self.invalidate_editor_markdown_preview();
            self.files_editor.borrow_mut().clear();
            cx.notify();
            return;
        };

        let epoch = self.next_editor_epoch();
        self.cancel_editor_task();
        self.editor_loading = true;
        self.editor_error = None;
        self.editor_path = Some(path.clone());
        self.editor_markdown_preview =
            is_markdown_path(path.as_str()) && retain_markdown_preview;
        self.invalidate_editor_markdown_preview();
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
                    if epoch != this.editor_epoch {
                        return;
                    }

                    this.editor_loading = false;
                    match result {
                        Ok(document) => {
                            let text = document.text;
                            this.editor_last_saved_text = Some(text.clone());
                            this.editor_dirty = false;
                            this.editor_error = None;
                            let open_result = this.open_files_editor_document(
                                path.as_str(),
                                &repo_root,
                                text.as_str(),
                                cx,
                            );
                            if let Err(err) = open_result {
                                this.editor_error =
                                    Some(format!("File editor failed to open {}: {err:#}", path));
                                this.files_editor.borrow_mut().clear();
                            } else if this.editor_markdown_preview {
                                this.schedule_editor_markdown_preview_parse(cx);
                            }
                        }
                        Err(err) => {
                            this.editor_last_saved_text = None;
                            this.editor_dirty = false;
                            this.editor_error = Some(format!("Editor unavailable: {err}"));
                            this.files_editor.borrow_mut().clear();
                        }
                    }

                    cx.notify();
                });
            }
        });
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

        let editor_already_open = self.editor_path.as_deref() == Some(path.as_str())
            && !self.editor_loading
            && self.editor_error.is_none();
        if !editor_already_open && self.prevent_unsaved_editor_discard(Some(path.as_str()), cx) {
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

        let needs_reload = self.editor_path.as_deref() != Some(path.as_str())
            || self.editor_loading
            || self.editor_error.is_some();
        if needs_reload {
            self.request_file_editor_reload(path.clone(), cx);
            if self.editor_path.as_deref() != Some(path.as_str()) {
                return false;
            }
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
        cx.notify();

        self.editor_save_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                save_file_editor_document(&repo_root, path_for_write.as_str(), text_to_write.as_str())
            });
            let result = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.editor_save_epoch {
                        return;
                    }

                    this.editor_save_loading = false;
                    match result {
                        Ok(()) => {
                            if this.editor_path.as_deref() == Some(status_path.as_str()) {
                                this.editor_last_saved_text = Some(saved_text.clone());
                                this.files_editor.borrow_mut().mark_saved();
                                this.sync_editor_dirty_from_input(cx);
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
        self.cancel_editor_task();
        self.cancel_editor_save_task();
        self.editor_path = None;
        self.editor_loading = false;
        self.editor_error = None;
        self.editor_dirty = false;
        self.editor_last_saved_text = None;
        self.editor_save_loading = false;
        self.editor_markdown_preview = false;
        self.invalidate_editor_markdown_preview();
        self.files_editor.borrow_mut().clear();
        self.editor_search_visible = false;
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

    fn open_files_editor_document(
        &mut self,
        relative_path: &str,
        repo_root: &std::path::Path,
        text: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let absolute_path = repo_root.join(relative_path);
        self.files_editor
            .borrow_mut()
            .open_document(&absolute_path, text)?;
        self.sync_editor_search_query(cx);

        let focus_handle = self.files_editor_focus_handle.clone();
        if let Err(err) = Self::update_any_window(cx, |window, cx| {
            focus_handle.focus(window, cx);
        }) {
            error!("failed to focus files editor: {err:#}");
            return Err(err);
        }
        Ok(())
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
}
