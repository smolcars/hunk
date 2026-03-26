impl DiffViewer {
    fn render_toolbar(
        &self,
        ai_view_state: Option<&AiVisibleFrameState>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let ai_selected = self.workspace_view_mode == WorkspaceViewMode::Ai;
        let is_dark = cx.theme().mode.is_dark();
        let git_selected = self.workspace_view_mode == WorkspaceViewMode::GitWorkspace;
        let review_selected = self.workspace_view_mode == WorkspaceViewMode::Diff;
        let project_label = self
            .project_path
            .clone()
            .or_else(|| self.repo_root.clone())
            .as_deref()
            .map(crate::app::project_picker::project_display_name)
            .unwrap_or_else(|| self.project_display_name());
        let repo_label = self
            .repo_root
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "No Git repository found".to_string());
        let active_branch = self
            .primary_checked_out_branch_name()
            .unwrap_or(self.branch_name.as_str())
            .to_string();
        let chip_colors = hunk_toolbar_chip(cx.theme(), is_dark);
        let toolbar_button_bg = hunk_dropdown_fill(cx.theme(), is_dark);
        let visible_line_stats = self.active_diff_overall_line_stats();
        let visible_file_count = if review_selected {
            self.active_diff_file_count()
        } else if git_selected {
            self.git_workspace.files.len()
        } else {
            self.files.len()
        };
        let ai_pending_approval_count = ai_view_state.map(|state| state.pending_approvals.len());
        let ai_pending_user_input_count =
            ai_view_state.map(|state| state.pending_user_inputs.len());
        let (ai_connection_status_label, ai_connection_status_color) =
            ai_connection_label(self.ai_connection_state, cx);
        let left = if self.workspace_view_mode.shows_toolbar_workspace_identity() {
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_2()
                .overflow_x_hidden()
                .child(
                    h_flex()
                        .items_center()
                        .min_w(px(260.0))
                        .max_w(px(320.0))
                        .px_1()
                        .py_0p5()
                        .rounded_md()
                        .bg(chip_colors.background)
                        .border_1()
                        .border_color(chip_colors.border)
                        .child(render_hunk_picker(
                            &self.project_picker_state,
                            HunkPickerConfig::new("project-picker", project_label)
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(8.0))
                                .background(chip_colors.background)
                                .border_color(chip_colors.border)
                                .min_width(px(258.0))
                                .max_width(px(318.0))
                                .disabled(self.state.workspace_project_paths.is_empty())
                                .empty(
                                    h_flex()
                                        .h(px(72.0))
                                        .justify_center()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No projects in this workspace."),
                                ),
                            cx,
                        )),
                )
                .child(
                    h_flex()
                        .items_center()
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .bg(chip_colors.background)
                        .border_1()
                        .border_color(chip_colors.border)
                        .child(
                            div()
                                .text_sm()
                                .font_medium()
                                .text_color(cx.theme().foreground)
                                .child(active_branch),
                        ),
                )
                .child(
                    h_flex()
                        .flex_none()
                        .min_w_0()
                        .max_w(px(560.0))
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .bg(chip_colors.background)
                        .border_1()
                        .border_color(chip_colors.border)
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .text_color(cx.theme().foreground.opacity(0.82))
                                .child(repo_label),
                        ),
                )
                .into_any_element()
        } else {
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(cx.theme().muted_foreground)
                        .child("Codex Workspace"),
                )
                .into_any_element()
        };

        let right = h_flex()
            .flex_none()
            .items_center()
            .gap_2()
            .when(review_selected, |this| {
                let view = view.clone();
                this.child(
                    Button::new("toggle-comments-preview")
                        .outline()
                        .compact()
                        .rounded(px(7.0))
                        .bg(toolbar_button_bg)
                        .label(format!("Comments ({})", self.comments_open_count()))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.toggle_comments_preview(cx);
                            });
                        }),
                )
            })
            .when(git_selected, |this| {
                this.when(self.git_workspace.overall_line_stats.changed() > 0, |this| {
                    this.child(self.render_line_stats(
                        "overall",
                        self.git_workspace.overall_line_stats,
                        cx,
                    ))
                })
                .child(self.render_git_metric_pill(
                    if self.git_workspace.branch_has_upstream {
                        "Published"
                    } else {
                        "Local Only"
                    },
                    if self.git_workspace.branch_has_upstream {
                        HunkAccentTone::Success
                    } else {
                        HunkAccentTone::Warning
                    },
                    cx,
                ))
                .child(self.render_git_metric_pill(
                    format!("Ahead {}", self.git_workspace.branch_ahead_count),
                    if self.git_workspace.branch_ahead_count > 0 {
                        HunkAccentTone::Accent
                    } else {
                        HunkAccentTone::Neutral
                    },
                    cx,
                ))
                .child(self.render_git_metric_pill(
                    format!("Behind {}", self.git_workspace.branch_behind_count),
                    if self.git_workspace.branch_behind_count > 0 {
                        HunkAccentTone::Warning
                    } else {
                        HunkAccentTone::Neutral
                    },
                    cx,
                ))
                .child(self.render_git_metric_pill(
                    format!("Changed {}", self.git_workspace.files.len()),
                    if self.git_workspace.files.is_empty() {
                        HunkAccentTone::Neutral
                    } else {
                        HunkAccentTone::Accent
                    },
                    cx,
                ))
            })
            .when(self.workspace_view_mode.shows_toolbar_change_summary(), |this| {
                this.child(self.render_line_stats("overall", visible_line_stats, cx))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} files", visible_file_count)),
                    )
            })
            .when(ai_selected, |this| {
                this.when_some(ai_pending_approval_count, |this, count| {
                    this.child(render_ai_header_metric_chip(
                        "Approvals",
                        count.to_string(),
                        if count > 0 {
                            cx.theme().warning
                        } else {
                            cx.theme().muted_foreground
                        },
                        is_dark,
                        cx,
                    ))
                })
                .when_some(ai_pending_user_input_count, |this, count| {
                    this.child(render_ai_header_metric_chip(
                        "Inputs",
                        count.to_string(),
                        if count > 0 {
                            cx.theme().warning
                        } else {
                            cx.theme().muted_foreground
                        },
                        is_dark,
                        cx,
                    ))
                })
                .child(render_ai_account_actions_for_view(self, view.clone(), cx))
                .child(render_ai_header_metric_chip(
                    "Status",
                    ai_connection_status_label.to_string(),
                    ai_connection_status_color,
                    is_dark,
                    cx,
                ))
            })
            .when(self.config.show_fps_counter, |this| {
                this.child(
                    div()
                        .text_sm()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(if self.fps >= 110.0 {
                            cx.theme().success
                        } else if self.fps >= 60.0 {
                            cx.theme().warning
                        } else {
                            cx.theme().danger
                        })
                        .child(format!("{:>3.0} fps", self.fps.round())),
                )
            })
            .into_any_element();

        h_flex()
            .w_full()
            .h_11()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn project_display_name(&self) -> String {
        self.repo_root
            .as_ref()
            .or(self.project_path.as_ref())
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| "Hunk".to_string())
    }

}
