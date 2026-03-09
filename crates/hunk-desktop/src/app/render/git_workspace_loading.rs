fn git_loading_skeleton_block(
    width_px: f32,
    height_px: f32,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    div()
        .w_full()
        .max_w(px(width_px))
        .h(px(height_px))
        .rounded(px(8.0))
        .bg(hunk_opacity(cx.theme().muted, is_dark, 0.22, 0.44))
        .into_any_element()
}

fn render_git_workspace_loading_overlay(
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    div()
        .absolute()
        .top_4()
        .left_0()
        .right_0()
        .child(
            h_flex()
                .w_full()
                .justify_center()
                .child(
                    h_flex()
                        .items_center()
                        .gap_3()
                        .rounded_full()
                        .border_1()
                        .border_color(hunk_opacity(cx.theme().warning, is_dark, 0.96, 0.82))
                        .bg(hunk_blend(cx.theme().background, cx.theme().warning, is_dark, 0.30, 0.18))
                        .px_4()
                        .py_2()
                        .child(
                            gpui_component::spinner::Spinner::new()
                                .with_size(gpui_component::Size::Small)
                                .color(cx.theme().warning),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child("Loading Git workspace..."),
                        ),
                ),
        )
        .into_any_element()
}

impl DiffViewer {
    fn git_workflow_ready_for_panel(&self) -> bool {
        self.git_workspace.root.is_some()
            || self.git_workspace.branch_name != "unknown"
            || !self.git_workspace.branches.is_empty()
            || !self.git_workspace.files.is_empty()
            || self.last_commit_subject.is_some()
    }

    fn render_git_workspace_panel_loading_skeleton(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();

        v_flex()
            .w_full()
            .gap_2()
            .children((0..4).map(|_| {
                v_flex()
                    .w_full()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.16, 0.24))
                    .px_3()
                    .py_2()
                    .child(git_loading_skeleton_block(
                        180.0,
                        11.0,
                        is_dark,
                        cx,
                    ))
                    .child(git_loading_skeleton_block(
                        360.0,
                        10.0,
                        is_dark,
                        cx,
                    ))
                    .child(git_loading_skeleton_block(
                        300.0,
                        10.0,
                        is_dark,
                        cx,
                    ))
            }))
            .into_any_element()
    }
}
