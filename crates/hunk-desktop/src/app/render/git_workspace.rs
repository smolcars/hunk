impl DiffViewer {
    fn render_git_workspace_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_git_workspace(cx.theme(), is_dark);
        let show_workflow_skeleton = self.workflow_loading && !self.git_workflow_ready_for_panel();
        let panel_body = if show_workflow_skeleton {
            self.render_git_workspace_panel_loading_skeleton(cx)
        } else {
            self.render_git_workspace_operations_panel(cx)
        };
        let active_branch_label = self
            .checked_out_branch_name()
            .map_or_else(|| "detached".to_string(), ToOwned::to_owned);
        let last_commit_text = self
            .last_commit_subject
            .as_deref()
            .map(str::trim_end)
            .filter(|text| !text.is_empty())
            .unwrap_or("No commits yet")
            .to_string();

        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .p_3()
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.shell.border)
            .bg(colors.shell.background)
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Git Workflow"),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .max_w(px(520.0))
                                    .truncate()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(active_branch_label),
                            )
                            .child(
                                div()
                                    .max_w(px(520.0))
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("Last commit: {last_commit_text}")),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .flex_wrap()
                                    .child(self.render_git_metric_pill(
                                        format!("Changed {}", self.git_workspace.files.len()),
                                        if self.git_workspace.files.is_empty() {
                                            HunkAccentTone::Neutral
                                        } else {
                                            HunkAccentTone::Accent
                                        },
                                        cx,
                                    ))
                                    .child(self.render_git_workspace_summary_line_stats(cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(panel_body),
            )
            .into_any_element()
    }

    fn render_git_workspace_summary_line_stats(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_line_stats(cx.theme(), is_dark);
        let surface = hunk_tinted_button(cx.theme(), is_dark, HunkAccentTone::Neutral);

        h_flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .rounded(px(999.0))
            .border_1()
            .border_color(surface.border)
            .bg(surface.background)
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Lines"),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(colors.added)
                    .child(format!("+{}", self.git_workspace.overall_line_stats.added)),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(colors.removed)
                    .child(format!("-{}", self.git_workspace.overall_line_stats.removed)),
            )
            .into_any_element()
    }
}
