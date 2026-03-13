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
        if previous_workspace_key == next_workspace_key {
            self.ai_sync_workspace_preferences(cx);
            return;
        }

        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.store_current_ai_workspace_state(previous_workspace_key.as_deref());
        self.park_visible_ai_runtime();
        self.restore_ai_workspace_state_for_key(next_workspace_key.as_deref());
        self.sync_ai_worktree_base_branch_picker_state(cx);
        self.restore_ai_visible_composer_from_current_draft(cx);
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
