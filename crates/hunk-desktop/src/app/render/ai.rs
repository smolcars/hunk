use std::time::Duration;

const AI_COMPOSER_SURFACE_MAX_WIDTH: f32 = 740.0;

impl DiffViewer {
    fn render_ai_workspace_screen(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.repo_discovery_failed {
            return self.render_open_project_empty_state(cx);
        }

        if let Some(error_message) = &self.error_message {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error_message.clone()),
                )
                .into_any_element();
        }

        let is_dark = cx.theme().mode.is_dark();
        let view = cx.entity();
        let threads = self.ai_visible_threads();
        let show_global_loading_overlay = self.ai_bootstrap_loading;
        let threads_loading = show_global_loading_overlay && threads.is_empty();
        let active_branch = self.ai_active_workspace_branch_name();
        let pending_approvals = self.ai_visible_pending_approvals();
        let pending_approval_count = pending_approvals.len();
        let pending_user_inputs = self.ai_visible_pending_user_inputs();
        let pending_user_input_count = pending_user_inputs.len();
        let selected_thread_id = self.current_ai_thread_id();
        let pending_thread_start = self.ai_pending_thread_start_for_timeline();
        let selected_thread_start_mode = selected_thread_id
            .as_deref()
            .and_then(|thread_id| self.ai_thread_start_mode(thread_id));
        let (selected_thread_mode_for_picker, thread_mode_picker_editable) = self
            .ai_thread_mode_picker_state(selected_thread_start_mode);
        let show_worktree_base_branch_picker =
            self.ai_new_thread_draft_active
                && self.ai_new_thread_start_mode == AiNewThreadStartMode::Worktree;
        let selected_worktree_base_branch = self
            .ai_selected_worktree_base_branch_name()
            .unwrap_or("Choose base branch")
            .to_string();
        let (
            timeline_total_turn_count,
            timeline_visible_turn_count,
            timeline_hidden_turn_count,
            timeline_visible_row_ids,
        ) = if let Some(thread_id) = selected_thread_id.as_deref() {
            self.ai_timeline_visible_rows_for_thread(thread_id)
        } else {
            (0, 0, 0, Vec::new())
        };
        let ai_timeline_follow_output = self.ai_timeline_follow_output;
        let show_no_turns_empty_state = ai_should_show_no_turns_empty_state(
            timeline_visible_row_ids.len(),
            pending_thread_start.is_some(),
        );
        let timeline_loading = show_global_loading_overlay
            && selected_thread_id.is_some()
            && timeline_visible_row_ids.is_empty();
        let show_select_thread_empty_state =
            selected_thread_id.is_none() && !timeline_loading && pending_thread_start.is_none();
        let ai_timeline_list_state = self.ai_timeline_list_state.clone();
        let (connection_label, connection_color) = ai_connection_label(self.ai_connection_state, cx);
        let composer_attachment_paths = self.current_ai_composer_local_images();
        let composer_attachment_count = composer_attachment_paths.len();
        let composer_send_waiting_on_connection =
            crate::app::controller::ai_prompt_send_waiting_on_connection(
                self.ai_connection_state,
                self.ai_bootstrap_loading,
            );
        let composer_interrupt_available = selected_thread_id
            .as_deref()
            .and_then(|thread_id| self.current_ai_in_progress_turn_id(thread_id))
            .is_some();
        let queued_message_count = selected_thread_id
            .as_deref()
            .map(|thread_id| self.ai_queued_message_row_ids_for_thread(thread_id).len())
            .unwrap_or(0);
        let model_supports_image_inputs = self.current_ai_model_supports_image_inputs();
        let review_action_blocker = self.ai_review_blocker();
        let ai_publish_blocker = self.ai_publish_blocker();
        let ai_publish_disabled = ai_publish_blocker.is_some();
        let ai_commit_and_push_loading = self.git_action_loading_named("Commit and Push");
        let ai_open_pr_blocker = self.ai_open_pr_blocker();
        let ai_open_pr_disabled = ai_open_pr_blocker.is_some();
        let ai_open_pr_loading = self.git_action_loading_named("Open PR");
        let ai_managed_worktree_target = self.ai_current_managed_worktree_target();
        let ai_delete_worktree_blocker = ai_managed_worktree_target
            .as_ref()
            .and_then(|_| self.ai_delete_worktree_blocker());
        let ai_delete_worktree_loading = self.git_action_loading_named("Delete Worktree");
        let composer_drop_border_color = if model_supports_image_inputs {
            hunk_opacity(cx.theme().accent, is_dark, 0.78, 0.62)
        } else {
            hunk_opacity(cx.theme().warning, is_dark, 0.88, 0.74)
        };
        let composer_drop_bg = if model_supports_image_inputs {
            hunk_opacity(cx.theme().accent, is_dark, 0.14, 0.10)
        } else {
            hunk_opacity(cx.theme().warning, is_dark, 0.14, 0.08)
        };

        let header_state = AiWorkspaceHeaderState {
            active_branch: active_branch.clone(),
            show_worktree_base_branch_picker,
            selected_worktree_base_branch,
            pending_approval_count,
            pending_user_input_count,
            connection_label,
            connection_color,
        };
        let sidebar_state = AiThreadSidebarState {
            threads,
            threads_loading,
            selected_thread_id: selected_thread_id.clone(),
            new_thread_menu_action_context: self.focus_handle.clone(),
        };
        let timeline_state = AiTimelinePanelState {
            active_branch: active_branch.clone(),
            selected_thread_id: selected_thread_id.clone(),
            selected_thread_start_mode,
            pending_approvals,
            pending_user_inputs,
            pending_thread_start,
            timeline_total_turn_count,
            timeline_visible_turn_count,
            timeline_hidden_turn_count,
            timeline_visible_row_ids,
            timeline_loading,
            show_select_thread_empty_state,
            show_no_turns_empty_state,
            ai_timeline_list_state,
            ai_timeline_follow_output,
            ai_publish_blocker,
            ai_publish_disabled,
            ai_commit_and_push_loading,
            ai_open_pr_disabled,
            ai_open_pr_loading,
            ai_managed_worktree_target,
            ai_delete_worktree_blocker,
            ai_delete_worktree_loading,
            ai_error_message: self.ai_error_message.clone(),
        };
        let composer_state = AiComposerPanelState {
            composer_attachment_paths,
            composer_attachment_count,
            model_supports_image_inputs,
            selected_thread_mode_for_picker,
            thread_mode_picker_editable,
            session_controls_read_only: composer_interrupt_available,
            composer_send_waiting_on_connection,
            composer_interrupt_available,
            queued_message_count,
            review_action_blocker,
            composer_drop_border_color,
            composer_drop_bg,
        };

        let composer_panel =
            self.render_ai_composer_panel(view.clone(), &composer_state, is_dark, cx);
        let workspace = self.render_ai_workspace_content(
            view,
            AiWorkspaceContentSections {
                header: &header_state,
                sidebar: &sidebar_state,
                timeline: &timeline_state,
                composer_panel,
            },
            is_dark,
            cx,
        );

        div()
            .size_full()
            .relative()
            .child(workspace)
            .when(show_global_loading_overlay, |this| {
                this.child(render_ai_global_loading_overlay(is_dark, cx))
            })
            .when_some(self.ai_git_progress.clone(), |this, progress| {
                this.child(render_ai_git_progress_overlay(&progress, is_dark, cx))
            })
            .into_any_element()
    }
}
