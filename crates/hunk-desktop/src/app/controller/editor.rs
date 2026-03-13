impl DiffViewer {
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
            self.helix_files_editor.borrow_mut().clear();
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
                            let open_result =
                                this.open_helix_editor_document(path.as_str(), &repo_root, cx);
                            if let Err(err) = open_result {
                                this.editor_error = Some(format!(
                                    "Helix editor failed to open {}: {err:#}",
                                    path
                                ));
                                this.helix_files_editor.borrow_mut().clear();
                            } else if this.editor_markdown_preview {
                                this.schedule_editor_markdown_preview_parse(cx);
                            }
                        }
                        Err(err) => {
                            this.editor_last_saved_text = None;
                            this.editor_dirty = false;
                            this.editor_error = Some(format!("Editor unavailable: {err}"));
                            this.helix_files_editor.borrow_mut().clear();
                        }
                    }

                    cx.notify();
                });
            }
        });
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
        if !self.editor_dirty {
            self.git_status_message = Some("No unsaved changes.".to_string());
            cx.notify();
            return;
        }

        let current_text = self.current_editor_text();
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
                                this.helix_files_editor.borrow_mut().mark_saved();
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

        let current_text = self.current_editor_text();
        let saved_text = self.editor_last_saved_text.as_deref().unwrap_or_default();
        let dirty =
            self.helix_files_editor.borrow().is_dirty() || current_text.as_str() != saved_text;
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
        let markdown_text = self.current_editor_text();
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
        self.helix_files_editor.borrow_mut().clear();
    }

    pub(crate) fn current_editor_text(&self) -> String {
        self.helix_files_editor
            .borrow()
            .current_text()
            .unwrap_or_default()
    }

    fn open_helix_editor_document(
        &mut self,
        relative_path: &str,
        repo_root: &std::path::Path,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let absolute_path = repo_root.join(relative_path);
        self.helix_files_editor.borrow_mut().open_path(&absolute_path)?;

        let focus_handle = self.files_editor_focus_handle.clone();
        if let Err(err) = Self::update_any_window(cx, |window, cx| {
            focus_handle.focus(window, cx);
        }) {
            error!("failed to focus helix-backed files editor: {err:#}");
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

    fn next_editor_epoch(&mut self) -> usize {
        self.editor_epoch = self.editor_epoch.saturating_add(1);
        self.editor_epoch
    }

    fn next_editor_save_epoch(&mut self) -> usize {
        self.editor_save_epoch = self.editor_save_epoch.saturating_add(1);
        self.editor_save_epoch
    }
}
