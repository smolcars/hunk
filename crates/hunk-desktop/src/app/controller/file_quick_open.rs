const FILE_QUICK_OPEN_RESULT_LIMIT: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FileQuickOpenAction {
    SelectNext,
    SelectPrevious,
    Accept,
    Dismiss,
}

pub(super) fn file_quick_open_action_for_keystroke(
    keystroke: &gpui::Keystroke,
) -> Option<FileQuickOpenAction> {
    let modifiers = &keystroke.modifiers;
    if modifiers.modified() {
        return None;
    }

    match keystroke.key.as_str() {
        "down" => Some(FileQuickOpenAction::SelectNext),
        "up" => Some(FileQuickOpenAction::SelectPrevious),
        "enter" => Some(FileQuickOpenAction::Accept),
        "escape" => Some(FileQuickOpenAction::Dismiss),
        _ => None,
    }
}

impl DiffViewer {
    pub(super) fn quick_open_file_action(
        &mut self,
        _: &QuickOpenFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_root) = self.repo_root.clone() else {
            self.git_status_message = Some("No repository is open.".to_string());
            cx.notify();
            return;
        };

        if self.workspace_view_mode != WorkspaceViewMode::Files {
            self.set_workspace_view_mode(WorkspaceViewMode::Files, cx);
        }
        if self.workspace_view_mode != WorkspaceViewMode::Files {
            return;
        }

        self.file_quick_open_visible = true;
        self.file_quick_open_selected_ix = 0;
        self.request_repo_file_search_reload_for_workspace(Some(repo_root), cx);
        self.file_quick_open_input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        self.sync_file_quick_open_matches(cx);
        cx.notify();
    }

    pub(super) fn request_repo_file_search_reload_for_workspace(
        &mut self,
        workspace_root: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_root) = workspace_root else {
            self.repo_file_search_provider.clear();
            self.repo_file_search_reload_task = Task::ready(());
            self.repo_file_search_loading = false;
            self.sync_file_quick_open_matches(cx);
            cx.notify();
            return;
        };

        let provider = self.repo_file_search_provider.clone();
        let generation = provider.begin_reload(Some(workspace_root.clone()));
        self.repo_file_search_loading = true;
        cx.notify();

        self.repo_file_search_reload_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn({
                    let workspace_root = workspace_root.clone();
                    async move { hunk_git::git::load_visible_repo_file_paths(&workspace_root) }
                })
                .await;

            match result {
                Ok(paths) => {
                    provider.apply_reload(generation, workspace_root.as_path(), paths);
                }
                Err(error) => {
                    warn!(
                        "failed to refresh repo file search for {}: {error:#}",
                        workspace_root.display()
                    );
                    provider.apply_reload(generation, workspace_root.as_path(), Vec::new());
                }
            }

            let _ = this.update(cx, |this, cx| {
                this.repo_file_search_loading = false;
                this.sync_file_quick_open_matches(cx);
            });
        });
    }

    pub(super) fn handle_file_quick_open_keystroke(
        &mut self,
        action: FileQuickOpenAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.file_quick_open_visible || self.workspace_view_mode != WorkspaceViewMode::Files {
            return false;
        }

        window.prevent_default();
        cx.stop_propagation();

        match action {
            FileQuickOpenAction::SelectNext => {
                self.file_quick_open_selected_ix = (self.file_quick_open_selected_ix + 1)
                    .min(self.file_quick_open_matches.len().saturating_sub(1));
                cx.notify();
                true
            }
            FileQuickOpenAction::SelectPrevious => {
                self.file_quick_open_selected_ix =
                    self.file_quick_open_selected_ix.saturating_sub(1);
                cx.notify();
                true
            }
            FileQuickOpenAction::Accept => self.accept_file_quick_open_selection(window, cx),
            FileQuickOpenAction::Dismiss => {
                self.dismiss_file_quick_open(window, cx);
                true
            }
        }
    }

    pub(super) fn dismiss_file_quick_open(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.file_quick_open_visible {
            return;
        }

        self.file_quick_open_visible = false;
        self.file_quick_open_matches.clear();
        self.file_quick_open_selected_ix = 0;
        self.file_quick_open_input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.files_editor_focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn accept_file_quick_open_path(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_ix) = self.file_quick_open_matches.iter().position(|item| item == &path)
        else {
            return;
        };
        self.file_quick_open_selected_ix = selected_ix;
        let _ = self.accept_file_quick_open_selection(window, cx);
    }

    pub(super) fn sync_file_quick_open_matches(&mut self, cx: &mut Context<Self>) {
        if !self.file_quick_open_visible {
            return;
        }

        let query = self.file_quick_open_input_state.read(cx).value().to_string();
        let next_matches = self
            .repo_file_search_provider
            .matched_paths(query.as_str(), FILE_QUICK_OPEN_RESULT_LIMIT);
        let next_selected_ix = self
            .file_quick_open_selected_ix
            .min(next_matches.len().saturating_sub(1));

        if self.file_quick_open_matches == next_matches
            && self.file_quick_open_selected_ix == next_selected_ix
        {
            return;
        }

        self.file_quick_open_matches = next_matches;
        self.file_quick_open_selected_ix = next_selected_ix;
        cx.notify();
    }

    fn accept_file_quick_open_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(path) = self
            .file_quick_open_matches
            .get(self.file_quick_open_selected_ix)
            .cloned()
        else {
            return false;
        };

        self.request_file_editor_reload(path.clone(), cx);
        if self.editor_path.as_deref() != Some(path.as_str()) {
            return false;
        }

        self.selected_path = Some(path.clone());
        self.selected_status = self.status_for_path(path.as_str());
        self.dismiss_file_quick_open(window, cx);
        self.files_editor_focus_handle.focus(window, cx);
        true
    }
}
