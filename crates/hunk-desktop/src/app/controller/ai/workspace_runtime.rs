impl DiffViewer {
    fn clear_ai_state_outside_current_project(&mut self) {
        let hidden_workspace_keys = self
            .ai_hidden_runtimes
            .keys()
            .filter(|workspace_key| {
                !ai_thread_workspace_matches_current_project(
                    std::path::Path::new(workspace_key.as_str()),
                    self.workspace_targets.as_slice(),
                    self.project_path.as_deref(),
                    self.repo_root.as_deref(),
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        for workspace_key in hidden_workspace_keys {
            self.shutdown_ai_runtime_for_workspace_blocking(workspace_key.as_str());
        }

        if self.ai_worker_workspace_key.as_ref().is_some_and(|workspace_key| {
            !ai_thread_workspace_matches_current_project(
                std::path::Path::new(workspace_key.as_str()),
                self.workspace_targets.as_slice(),
                self.project_path.as_deref(),
                self.repo_root.as_deref(),
            )
        }) {
            let workspace_key = self.ai_worker_workspace_key.clone();
            if let Some(workspace_key) = workspace_key {
                self.shutdown_ai_runtime_for_workspace_blocking(workspace_key.as_str());
            }
        }

        let removable_workspace_keys = self
            .ai_workspace_states
            .keys()
            .filter(|workspace_key| {
                !ai_thread_workspace_matches_current_project(
                    std::path::Path::new(workspace_key.as_str()),
                    self.workspace_targets.as_slice(),
                    self.project_path.as_deref(),
                    self.repo_root.as_deref(),
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        for workspace_key in removable_workspace_keys {
            self.ai_forget_deleted_workspace_state(workspace_key.as_str());
        }
    }

    pub(super) fn ai_handle_workspace_change(
        &mut self,
        previous_workspace_key: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let next_workspace_key = self.ai_workspace_key();
        self.ai_handle_workspace_change_to(previous_workspace_key, next_workspace_key, cx);
    }

    pub(super) fn ai_handle_workspace_change_to(
        &mut self,
        previous_workspace_key: Option<String>,
        next_workspace_key: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if previous_workspace_key == next_workspace_key {
            self.ai_sync_workspace_preferences(cx);
            return;
        }

        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.sync_ai_visible_terminal_input_to_state(cx);
        let previous_workspace_is_in_current_project = previous_workspace_key
            .as_deref()
            .is_some_and(|workspace_key| {
                ai_thread_workspace_matches_current_project(
                    std::path::Path::new(workspace_key),
                    self.workspace_targets.as_slice(),
                    self.project_path.as_deref(),
                    self.repo_root.as_deref(),
                )
            });
        if previous_workspace_is_in_current_project {
            self.store_current_ai_workspace_state(previous_workspace_key.as_deref());
            self.park_visible_ai_runtime();
        } else {
            self.clear_ai_state_outside_current_project();
        }
        self.stop_ai_terminal_runtime("workspace changed");
        self.restore_ai_workspace_state_for_key(next_workspace_key.as_deref());
        self.sync_ai_worktree_base_branch_picker_state(cx);
        self.ai_composer_skill_completion_menu = None;
        self.ai_composer_skill_completion_selected_ix = 0;
        self.ai_composer_skill_completion_dismissed_token = None;
        self.restore_ai_visible_composer_from_current_draft(cx);
        self.restore_ai_visible_terminal_input(cx);
        // Some callers update the visible thread/draft selection after this returns, so reload the
        // composer completion source from the destination workspace instead of the stale one.
        self.request_ai_composer_file_completion_reload_for_workspace(
            ai_completion_reload_workspace_root(next_workspace_key.as_deref()),
            cx,
        );
        if self.workspace_view_mode == WorkspaceViewMode::Ai {
            self.refresh_ai_repo_thread_catalog(cx);
            self.ensure_ai_runtime_started_for_workspace_key(next_workspace_key.as_deref(), cx);
        }
        self.ai_sync_workspace_preferences(cx);
    }
}

fn ai_completion_reload_workspace_root(
    next_workspace_key: Option<&str>,
) -> Option<std::path::PathBuf> {
    next_workspace_key.map(std::path::PathBuf::from)
}
