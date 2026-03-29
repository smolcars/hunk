impl DiffViewer {
    fn next_review_editor_epoch(&mut self) -> usize {
        self.review_editor_session.load_epoch =
            self.review_editor_session.load_epoch.saturating_add(1);
        self.review_editor_session.load_epoch
    }

    fn clear_review_editor_session(&mut self) {
        self.next_review_editor_epoch();
        self.review_editor_session.loading = false;
        self.review_editor_session.error = None;
        self.review_editor_session.path = None;
        self.review_editor_session.left_source_id = None;
        self.review_editor_session.right_source_id = None;
        self.review_editor_session.left_present = false;
        self.review_editor_session.right_present = false;
        self.review_editor_session.load_task = Task::ready(());
        self.review_editor_session.left_editor.borrow_mut().clear();
        self.review_editor_session.right_editor.borrow_mut().clear();
    }

    fn review_editor_rows_for_path(&self, path: &str) -> Option<&[SideBySideRow]> {
        let range = self.file_row_ranges.iter().find(|range| range.path == path)?;
        self.diff_rows.get(range.start_row..range.end_row)
    }

    fn request_review_editor_reload(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            self.clear_review_editor_session();
            return;
        }

        let Some(path) = self.selected_path.clone() else {
            self.clear_review_editor_session();
            return;
        };
        if !self.active_diff_contains_path(path.as_str()) {
            self.clear_review_editor_session();
            return;
        }

        let Some(project_root) = self.project_path.clone() else {
            self.clear_review_editor_session();
            return;
        };
        let Some((left_source, right_source)) = self.selected_review_compare_sources() else {
            self.clear_review_editor_session();
            return;
        };
        let left_source_id = self.review_left_source_id.clone();
        let right_source_id = self.review_right_source_id.clone();

        if !force
            && self.review_editor_session.path.as_deref() == Some(path.as_str())
            && self.review_editor_session.left_source_id == left_source_id
            && self.review_editor_session.right_source_id == right_source_id
            && !self.review_editor_session.loading
            && self.review_editor_session.error.is_none()
        {
            return;
        }

        let epoch = self.next_review_editor_epoch();
        self.review_editor_session.loading = true;
        self.review_editor_session.error = None;
        self.review_editor_session.path = Some(path.clone());
        self.review_editor_session.left_source_id = left_source_id.clone();
        self.review_editor_session.right_source_id = right_source_id.clone();
        self.review_editor_session.load_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                load_compare_file_document(
                    &project_root,
                    &left_source,
                    &right_source,
                    path.as_str(),
                )
            });
            let result = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.review_editor_session.load_epoch {
                        return;
                    }

                    this.review_editor_session.loading = false;
                    match result {
                        Ok(document) => {
                            let path = document.path.clone();
                            let absolute_path = project_root.join(path.as_str());
                            let overlays = this
                                .review_editor_rows_for_path(path.as_str())
                                .map(build_review_editor_overlays)
                                .unwrap_or_default();
                            let left_result = this
                                .review_editor_session
                                .left_editor
                                .borrow_mut()
                                .open_document(&absolute_path, document.left_text.as_str());
                            let right_result = this
                                .review_editor_session
                                .right_editor
                                .borrow_mut()
                                .open_document(&absolute_path, document.right_text.as_str());

                            match left_result.and(right_result) {
                                Ok(()) => {
                                    this.review_editor_session.left_present = document.left_present;
                                    this.review_editor_session.right_present = document.right_present;
                                    this.review_editor_session.error = None;
                                    this.review_editor_session
                                        .left_editor
                                        .borrow_mut()
                                        .set_manual_overlays(overlays.0);
                                    this.review_editor_session
                                        .right_editor
                                        .borrow_mut()
                                        .set_manual_overlays(overlays.1);
                                }
                                Err(err) => {
                                    this.review_editor_session.error = Some(format!(
                                        "Review editor preview unavailable: {err:#}"
                                    ));
                                    this.review_editor_session.left_present = false;
                                    this.review_editor_session.right_present = false;
                                    this.review_editor_session.left_editor.borrow_mut().clear();
                                    this.review_editor_session.right_editor.borrow_mut().clear();
                                }
                            }
                        }
                        Err(err) => {
                            this.review_editor_session.error =
                                Some(format!("Review editor preview unavailable: {err:#}"));
                            this.review_editor_session.left_present = false;
                            this.review_editor_session.right_present = false;
                            this.review_editor_session.left_editor.borrow_mut().clear();
                            this.review_editor_session.right_editor.borrow_mut().clear();
                        }
                    }

                    cx.notify();
                });
            }
        });
        cx.notify();
    }
}
