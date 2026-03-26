impl DiffViewer {
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
        let previous_terminal_thread_id = self
            .ai_terminal_runtime
            .as_ref()
            .map(|runtime| runtime.thread_id.clone())
            .or_else(|| self.current_ai_thread_id());
        if previous_workspace_key == next_workspace_key {
            self.ai_sync_workspace_preferences(cx);
            return;
        }

        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.sync_ai_visible_terminal_input_to_state(cx);
        if previous_workspace_key.is_some() {
            self.store_current_ai_workspace_state(previous_workspace_key.as_deref());
            self.park_visible_ai_runtime();
        }
        self.restore_ai_workspace_state_for_key(next_workspace_key.as_deref());
        let next_terminal_thread_id = self.current_ai_thread_id();
        self.ai_handle_terminal_thread_change(
            previous_terminal_thread_id,
            next_terminal_thread_id,
            cx,
        );
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
