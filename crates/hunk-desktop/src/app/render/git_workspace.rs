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
                    .gap_0p5()
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Git Workflow"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Manage branches, working tree changes, commits, publishing, and review handoff.",
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
}
