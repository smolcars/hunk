impl DiffViewer {
    pub(super) fn ai_handle_workspace_change(
        &mut self,
        previous_workspace_key: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if previous_workspace_key == self.ai_workspace_key() {
            self.ai_sync_workspace_preferences(cx);
            return;
        }

        self.next_ai_event_epoch();
        if let Some(command_tx) = self.ai_command_tx.take() {
            let _ = command_tx.send(AiWorkerCommand::Shutdown);
        }
        self.detach_ai_worker_thread_join("switching AI workspace");
        self.ai_worker_workspace_key = None;
        self.reset_ai_runtime_workspace_state();

        self.ai_sync_workspace_preferences(cx);
        self.restore_ai_visible_composer_from_current_draft(cx);
        if self.workspace_view_mode == WorkspaceViewMode::Ai {
            self.ensure_ai_runtime_started(cx);
        }
    }

    fn reset_ai_runtime_workspace_state(&mut self) {
        self.ai_connection_state = AiConnectionState::Disconnected;
        self.ai_bootstrap_loading = false;
        self.ai_error_message = None;
        self.ai_status_message = None;
        self.ai_state_snapshot = hunk_codex::state::AiState::default();
        self.ai_selected_thread_id = None;
        self.ai_new_thread_draft_active = false;
        self.ai_pending_new_thread_selection = false;
        self.ai_thread_title_refresh_state_by_thread.clear();
        self.ai_pending_approvals.clear();
        self.ai_pending_user_inputs.clear();
        self.ai_pending_user_input_answers.clear();
        self.ai_composer_status_by_draft.clear();
        self.ai_timeline_visible_turn_limit_by_thread.clear();
        self.ai_timeline_turn_ids_by_thread.clear();
        self.ai_timeline_row_ids_by_thread.clear();
        self.ai_timeline_rows_by_id.clear();
        self.ai_timeline_groups_by_id.clear();
        self.ai_timeline_group_parent_by_child_row_id.clear();
        self.ai_in_progress_turn_started_at.clear();
        self.ai_composer_activity_elapsed_second = None;
        self.ai_expanded_timeline_row_ids.clear();
        self.ai_text_selection = None;
        self.ai_scroll_timeline_to_bottom = false;
        self.ai_timeline_follow_output = true;
        reset_ai_timeline_list_measurements(self, 0);
    }
}
