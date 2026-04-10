const AI_COMPOSER_SURFACE_MAX_WIDTH: f32 = 740.0;
const AI_USAGE_POPOVER_MAX_WIDTH: f32 = 434.0;
const AI_USAGE_ROW_LABEL_WIDTH: f32 = 68.0;
const AI_USAGE_ROW_BAR_HEIGHT: f32 = 14.0;
const AI_USAGE_ROW_DETAILS_WIDTH: f32 = 134.0;

struct TerminalPanelState {
    kind: WorkspaceTerminalKind,
    open: bool,
    cwd_label: String,
    shell_label: String,
    status_message: Option<String>,
    status: AiTerminalSessionStatus,
    running: bool,
    surface_focused: bool,
    screen: Option<Arc<TerminalScreenSnapshot>>,
    display_offset: usize,
    has_transcript: bool,
    has_output: bool,
    has_last_command: bool,
    transcript: String,
    height_px: f32,
}

fn ai_terminal_shell_label(config: &AppConfig) -> String {
    crate::terminal_env::terminal_shell_label(&config.terminal)
}

impl DiffViewer {
    fn render_ai_workspace_screen(
        &mut self,
        ai_view_state: Option<AiVisibleFrameState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
        let ai_view_state = ai_view_state.unwrap_or_else(|| self.visible_ai_frame_state());
        self.sync_current_ai_followup_prompt_state();
        let show_global_loading_overlay = self.ai_bootstrap_loading;
        let selected_thread_id = ai_view_state.selected_thread_id.clone();
        let composer_followup_prompt = self.current_ai_followup_prompt();
        let composer_followup_action = self.current_ai_followup_prompt_action();
        let (selected_thread_mode_for_picker, thread_mode_picker_editable) =
            self.ai_thread_mode_picker_state(ai_view_state.selected_thread_start_mode);
        let composer_attachment_paths = ai_view_state.composer_attachment_paths.clone();
        let composer_attachment_count = composer_attachment_paths.len();
        let ai_commit_and_push_loading = self.git_action_loading_named("Commit and Push");
        let ai_create_branch_and_push_loading =
            self.git_action_loading_named("Create Branch and Push");
        let ai_open_pr_loading = self.git_action_loading_named("Open PR");
        let ai_delete_worktree_loading = self.git_action_loading_named("Delete Worktree");
        let composer_drop_border_color = if ai_view_state.model_supports_image_inputs {
            hunk_opacity(cx.theme().accent, is_dark, 0.78, 0.62)
        } else {
            hunk_opacity(cx.theme().warning, is_dark, 0.88, 0.74)
        };
        let composer_drop_bg = if ai_view_state.model_supports_image_inputs {
            hunk_opacity(cx.theme().accent, is_dark, 0.14, 0.10)
        } else {
            hunk_opacity(cx.theme().warning, is_dark, 0.14, 0.08)
        };

        let sidebar_state = AiThreadSidebarState {
            project_count: ai_view_state.project_count,
            threads_loading: ai_view_state.threads_loading,
            selected_thread_id: selected_thread_id.clone(),
        };
        let timeline_state = AiTimelinePanelState {
            active_branch: ai_view_state.active_branch.clone(),
            workspace_label: ai_view_state.active_workspace_label.clone(),
            show_worktree_base_branch_picker: ai_view_state.show_worktree_base_branch_picker,
            selected_worktree_base_branch: ai_view_state.selected_worktree_base_branch.clone(),
            selected_thread_id: selected_thread_id.clone(),
            inline_review_selected_row_id: ai_view_state.inline_review_selected_row_id.clone(),
            selected_thread_start_mode: ai_view_state.selected_thread_start_mode,
            pending_approvals: ai_view_state.pending_approvals.clone(),
            pending_user_inputs: ai_view_state.pending_user_inputs.clone(),
            pending_thread_start: ai_view_state.pending_thread_start.clone(),
            timeline_total_turn_count: ai_view_state.timeline_total_turn_count,
            timeline_visible_turn_count: ai_view_state.timeline_visible_turn_count,
            timeline_hidden_turn_count: ai_view_state.timeline_hidden_turn_count,
            timeline_visible_row_ids: ai_view_state.timeline_visible_row_ids.clone(),
            timeline_loading: ai_view_state.timeline_loading,
            show_select_thread_empty_state: ai_view_state.show_select_thread_empty_state,
            show_no_turns_empty_state: ai_view_state.show_no_turns_empty_state,
            ai_publish_blocker: ai_view_state.ai_publish_blocker.clone(),
            ai_publish_disabled: ai_view_state.ai_publish_disabled,
            ai_commit_and_push_loading,
            ai_create_branch_and_push_loading,
            ai_open_pr_disabled: ai_view_state.ai_open_pr_disabled,
            ai_open_pr_loading,
            ai_managed_worktree_target: ai_view_state.ai_managed_worktree_target.clone(),
            ai_delete_worktree_blocker: ai_view_state.ai_delete_worktree_blocker.clone(),
            ai_delete_worktree_loading,
            ai_error_message: self.ai_error_message.clone(),
            ai_requires_openai_auth: self.ai_requires_openai_auth,
            ai_pending_chatgpt_login_id: self.ai_pending_chatgpt_login_id.clone(),
            ai_account_connected: self.ai_account.is_some(),
        };
        let composer_state = AiComposerPanelState {
            composer_feedback: ai_view_state.composer_feedback.clone(),
            composer_attachment_paths,
            composer_attachment_count,
            model_supports_image_inputs: ai_view_state.model_supports_image_inputs,
            review_mode_active: self.ai_review_mode_active,
            usage_popover_open: self.ai_usage_popover_open,
            current_mode_label: crate::app::ai_composer_commands::ai_composer_mode_label(
                self.ai_review_mode_active,
                self.ai_selected_collaboration_mode,
            )
            .to_string(),
            fast_mode_enabled: matches!(
                self.ai_selected_service_tier,
                hunk_domain::state::AiServiceTierSelection::Fast
            ),
            selected_thread_mode_for_picker,
            thread_mode_picker_editable,
            session_controls_read_only: ai_view_state.composer_interrupt_available,
            selected_thread_context_usage: ai_view_state.selected_thread_context_usage.clone(),
            composer_send_waiting_on_connection: ai_view_state.composer_send_waiting_on_connection,
            composer_interrupt_available: ai_view_state.composer_interrupt_available,
            queued_message_count: ai_view_state.queued_message_count,
            review_action_blocker: ai_view_state.review_action_blocker.clone(),
            followup_prompt: composer_followup_prompt,
            followup_prompt_action: composer_followup_action,
            composer_drop_border_color,
            composer_drop_bg,
        };
        let terminal_state = TerminalPanelState {
            kind: WorkspaceTerminalKind::Ai,
            open: self.ai_terminal_open,
            cwd_label: ai_view_state.terminal_cwd_label.clone(),
            shell_label: ai_terminal_shell_label(&self.config),
            status_message: self.ai_terminal_session.status_message.clone(),
            status: self.ai_terminal_session.status,
            running: self.ai_terminal_is_running(),
            surface_focused: self.ai_terminal_surface_focused,
            screen: self.ai_terminal_session.screen.clone(),
            display_offset: self
                .ai_terminal_session
                .screen
                .as_ref()
                .map(|screen| screen.display_offset)
                .unwrap_or(0),
            has_transcript: !self.ai_terminal_session.transcript.trim().is_empty(),
            has_output: self.ai_terminal_session.screen.is_some()
                || !self.ai_terminal_session.transcript.trim().is_empty(),
            has_last_command: self.ai_terminal_session.last_command.is_some(),
            transcript: self.ai_terminal_session.transcript.clone(),
            height_px: self.ai_terminal_height_px,
        };
        let composer_panel =
            self.render_ai_composer_panel(view.clone(), &composer_state, is_dark, cx);
        let terminal_panel = self
            .render_workspace_terminal_panel(view.clone(), &terminal_state, is_dark, cx)
            .filter(|_| terminal_state.open);
        let workspace = self.render_ai_workspace_content(
            view,
            AiWorkspaceContentSections {
                sidebar: &sidebar_state,
                timeline: &timeline_state,
                terminal_panel,
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

    fn render_ai_usage_popover_card(
        &self,
        view: Entity<Self>,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let popover_surface = hunk_completion_menu(cx.theme(), is_dark).panel;
        let account_summary = ai_account_summary(
            self.ai_account.as_ref(),
            self.ai_requires_openai_auth,
            self.ai_bootstrap_loading && self.ai_account.is_none() && !self.ai_requires_openai_auth,
        );
        let (five_hour_window, weekly_window) = self
            .ai_rate_limits
            .as_ref()
            .map(ai_rate_limit_windows)
            .unwrap_or((None, None));

        v_flex()
            .id("ai-usage-popover")
            .w_full()
            .max_w(px(AI_USAGE_POPOVER_MAX_WIDTH))
            .rounded(px(16.0))
            .border_1()
            .border_color(popover_surface.border)
            .bg(popover_surface.background)
            .shadow_lg()
            .px_3()
            .py_2()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Status"),
                    )
                    .child({
                        let view = view.clone();
                        Button::new("ai-usage-close")
                            .compact()
                            .ghost()
                            .rounded(px(8.0))
                            .label("Close")
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.ai_close_usage_overlay_action(cx);
                                });
                            })
                    }),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(account_summary),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(render_ai_usage_row(
                        "5h limit",
                        five_hour_window.as_ref(),
                        is_dark,
                        cx,
                    ))
                    .child(render_ai_usage_row(
                        "7d limit",
                        weekly_window.as_ref(),
                        is_dark,
                        cx,
                    )),
            )
            .into_any_element()
    }
}

fn render_ai_usage_row(
    label: &str,
    window: Option<&codex_app_server_protocol::RateLimitWindow>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let used_percent = window.map(|window| window.used_percent.clamp(0, 100) as u8);
    let remaining_percent = used_percent
        .map(|used_percent| 100_u8.saturating_sub(used_percent))
        .unwrap_or(0);
    let reset_label = window
        .and_then(|window| window.resets_at)
        .map(ai_format_rate_limit_reset_compact);

    h_flex()
        .w_full()
        .items_center()
        .gap_3()
        .child(
            div()
                .w(px(AI_USAGE_ROW_LABEL_WIDTH))
                .text_xs()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().muted_foreground)
                .child(format!("{label}:")),
        )
        .child(
            div()
                .flex_1()
                .h(px(AI_USAGE_ROW_BAR_HEIGHT))
                .rounded(px(999.0))
                .border_1()
                .border_color(hunk_opacity(cx.theme().border, is_dark, 0.78, 0.62))
                .bg(hunk_opacity(cx.theme().muted, is_dark, 0.22, 0.40))
                .child(
                    div()
                        .h_full()
                        .rounded(px(999.0))
                        .bg(hunk_opacity(cx.theme().foreground, is_dark, 0.96, 0.92))
                        .w(gpui::relative(remaining_percent as f32 / 100.0)),
                ),
        )
        .child(
            div()
                .w(px(AI_USAGE_ROW_DETAILS_WIDTH))
                .text_xs()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().foreground)
                .child(match reset_label {
                    Some(reset_label) => {
                        format!("{remaining_percent}% left (resets {reset_label})")
                    }
                    None => "Unavailable".to_string(),
                }),
        )
        .into_any_element()
}

fn ai_format_rate_limit_reset_compact(unix_seconds: i64) -> String {
    let Ok(utc_datetime) = time::OffsetDateTime::from_unix_timestamp(unix_seconds) else {
        return unix_seconds.to_string();
    };

    let local_datetime = time::UtcOffset::current_local_offset()
        .map(|offset| utc_datetime.to_offset(offset))
        .unwrap_or(utc_datetime);
    let now = time::UtcOffset::current_local_offset()
        .ok()
        .map(|offset| time::OffsetDateTime::now_utc().to_offset(offset))
        .unwrap_or_else(time::OffsetDateTime::now_utc);

    if local_datetime.date() == now.date() {
        let minute = local_datetime.minute();
        let (hour, meridiem) = ai_hour_and_meridiem(local_datetime.hour());
        format!("{hour}:{minute:02} {meridiem}")
    } else {
        let minute = local_datetime.minute();
        let (hour, meridiem) = ai_hour_and_meridiem(local_datetime.hour());
        format!(
            "{} {}, {hour}:{minute:02} {meridiem}",
            ai_month_short(local_datetime.month()),
            local_datetime.day()
        )
    }
}
