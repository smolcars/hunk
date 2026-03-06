fn ai_loading_skeleton_block(
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
        .bg(cx.theme().muted.opacity(if is_dark { 0.22 } else { 0.44 }))
        .into_any_element()
}

fn render_ai_global_loading_overlay(
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
                        .border_color(cx.theme().warning.opacity(if is_dark { 0.96 } else { 0.82 }))
                        .bg(cx.theme().background.blend(cx.theme().warning.opacity(if is_dark {
                            0.30
                        } else {
                            0.18
                        })))
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
                                .child("Loading AI workspace..."),
                        ),
                ),
        )
        .into_any_element()
}

fn render_ai_thread_list_loading_skeleton(
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap_0p5()
        .children((0..6).map(|_| {
            v_flex()
                .w_full()
                .gap_0p5()
                .px_2()
                .py_1p5()
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(ai_loading_skeleton_block(
                            180.0,
                            12.0,
                            is_dark,
                            cx,
                        ))
                        .child(ai_loading_skeleton_block(
                            28.0,
                            10.0,
                            is_dark,
                            cx,
                        )),
                )
                .child(ai_loading_skeleton_block(
                    140.0,
                    10.0,
                    is_dark,
                    cx,
                ))
        }))
        .into_any_element()
}

fn render_ai_timeline_loading_skeleton(
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
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
                .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                    0.16
                } else {
                    0.24
                })))
                .p_3()
                .child(ai_loading_skeleton_block(
                    104.0,
                    11.0,
                    is_dark,
                    cx,
                ))
                .child(ai_loading_skeleton_block(
                    320.0,
                    10.0,
                    is_dark,
                    cx,
                ))
                .child(ai_loading_skeleton_block(
                    288.0,
                    10.0,
                    is_dark,
                    cx,
                ))
        }))
        .into_any_element()
}
