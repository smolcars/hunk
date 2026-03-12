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
        .bg(hunk_opacity(cx.theme().muted, is_dark, 0.22, 0.44))
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
                                .child("Loading AI workspace..."),
                        ),
                ),
        )
        .into_any_element()
}

fn render_ai_git_progress_overlay(
    progress: &AiGitProgressState,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let backdrop_bg = hunk_modal_backdrop(cx.theme(), is_dark);
    let modal_surface = hunk_modal_surface(cx.theme(), is_dark);
    let current_step_index = progress
        .steps
        .iter()
        .position(|step| *step == progress.step)
        .unwrap_or(0);

    div()
        .id("ai-git-progress-overlay")
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .bg(backdrop_bg)
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Middle, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_, _, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .size_full()
                .p_4()
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Middle, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_scroll_wheel(|_, _, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .size_full()
                        .overflow_y_scrollbar()
                        .child(
                            div()
                                .w_full()
                                .min_h_full()
                                .py_8()
                                .flex()
                                .items_start()
                                .justify_center()
                                .child(
                                    v_flex()
                                        .id("ai-git-progress-popup")
                                        .w_full()
                                        .max_w(px(460.0))
                                        .max_h(px(520.0))
                                        .min_h_0()
                                        .gap_4()
                                        .rounded(px(14.0))
                                        .border_1()
                                        .border_color(modal_surface.border)
                                        .bg(modal_surface.background)
                                        .p_4()
                                        .child(
                                            v_flex()
                                                .w_full()
                                                .min_w_0()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .text_lg()
                                                        .font_semibold()
                                                        .text_color(cx.theme().foreground)
                                                        .child(progress.action.title()),
                                                )
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .whitespace_normal()
                                                        .child(progress.action.summary()),
                                                ),
                                        )
                                        .when_some(progress.detail.clone(), |this, detail| {
                                            this.child(
                                                div()
                                                    .w_full()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(hunk_opacity(
                                                        cx.theme().border,
                                                        is_dark,
                                                        0.88,
                                                        0.70,
                                                    ))
                                                    .bg(hunk_blend(
                                                        cx.theme().background,
                                                        cx.theme().muted,
                                                        is_dark,
                                                        0.12,
                                                        0.18,
                                                    ))
                                                    .px_3()
                                                    .py_2()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(detail),
                                            )
                                        })
                                        .child(
                                            div()
                                                .flex_1()
                                                .h_full()
                                                .w_full()
                                                .min_w_0()
                                                .min_h_0()
                                                .overflow_y_scrollbar()
                                                .child(
                                                    v_flex()
                                                        .w_full()
                                                        .gap_2()
                                                        .children(progress.steps.iter().enumerate().map(
                                                            |(index, step)| {
                                                                let is_complete = index < current_step_index;
                                                                let is_current = index == current_step_index;

                                                                h_flex()
                                                                    .w_full()
                                                                    .items_center()
                                                                    .gap_3()
                                                                    .child(
                                                                        div()
                                                                            .w(px(16.0))
                                                                            .flex()
                                                                            .justify_center()
                                                                            .child(if is_current {
                                                                                gpui_component::spinner::Spinner::new()
                                                                                    .with_size(gpui_component::Size::Small)
                                                                                    .color(cx.theme().accent)
                                                                                    .into_any_element()
                                                                            } else if is_complete {
                                                                                Icon::new(IconName::Check)
                                                                                    .size(px(14.0))
                                                                                    .text_color(cx.theme().success)
                                                                                    .into_any_element()
                                                                            } else {
                                                                                div()
                                                                                    .size(px(10.0))
                                                                                    .rounded_full()
                                                                                    .bg(hunk_opacity(
                                                                                        cx.theme().muted,
                                                                                        is_dark,
                                                                                        0.34,
                                                                                        0.46,
                                                                                    ))
                                                                                    .into_any_element()
                                                                            }),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .flex_1()
                                                                            .min_w_0()
                                                                            .text_sm()
                                                                            .when(is_current, |this| this.font_semibold())
                                                                            .text_color(if is_complete || is_current {
                                                                                cx.theme().foreground
                                                                            } else {
                                                                                cx.theme().muted_foreground
                                                                            })
                                                                            .child(step.label()),
                                                                    )
                                                                    .into_any_element()
                                                            },
                                                        )),
                                                ),
                                        ),
                                ),
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
                .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.16, 0.24))
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

fn render_ai_pending_thread_start(
    pending: &AiPendingThreadStart,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let pending_colors = hunk_pending_message(cx.theme(), is_dark);
    let status_text = if pending.thread_id.is_some() {
        "Sending first message..."
    } else {
        "Creating thread..."
    };
    let elapsed_seconds = pending.started_at.elapsed().as_secs();
    let attachment_summary = match pending.local_images.len() {
        0 => None,
        1 => Some("1 attachment".to_string()),
        count => Some(format!("{count} attachments")),
    };
    let message_text = if pending.prompt.trim().is_empty() {
        attachment_summary
            .clone()
            .unwrap_or_else(|| "Submitting request...".to_string())
    } else {
        pending.prompt.clone()
    };
    let attachment_status = attachment_summary.unwrap_or_else(|| "No attachments".to_string());

    h_flex()
        .w_full()
        .min_w_0()
        .justify_end()
        .child(
            v_flex()
                .max_w(px(680.0))
                .w_full()
                .min_w_0()
                .gap_1p5()
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_xs()
                                .font_semibold()
                                .child("You"),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .text_color(pending_colors.text)
                        .whitespace_normal()
                        .child(message_text),
                )
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    gpui_component::spinner::Spinner::new()
                                        .with_size(gpui_component::Size::Small)
                                        .color(pending_colors.meta),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(pending_colors.meta)
                                        .child(status_text),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .text_xs()
                                .text_color(pending_colors.meta)
                                .child(format!(
                                    "{} | {} | {}s",
                                    pending.start_mode.label(),
                                    attachment_status,
                                    elapsed_seconds
                                )),
                        ),
                ),
        )
        .into_any_element()
}

fn render_ai_pending_steer(
    pending: &AiPendingSteer,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let pending_colors = hunk_pending_message(cx.theme(), is_dark);
    let elapsed_seconds = pending.started_at.elapsed().as_secs();
    let attachment_status = match pending.local_images.len() {
        0 => "No attachments".to_string(),
        1 => "1 attachment".to_string(),
        count => format!("{count} attachments"),
    };
    let message_text = if pending.prompt.trim().is_empty() {
        attachment_status.clone()
    } else {
        pending.prompt.clone()
    };

    h_flex()
        .w_full()
        .min_w_0()
        .justify_end()
        .child(
            v_flex()
                .max_w(px(680.0))
                .w_full()
                .min_w_0()
                .gap_1p5()
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_xs()
                                .font_semibold()
                                .child("You"),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .text_color(pending_colors.text)
                        .whitespace_normal()
                        .child(message_text),
                )
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    gpui_component::spinner::Spinner::new()
                                        .with_size(gpui_component::Size::Small)
                                        .color(pending_colors.meta),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(pending_colors.meta)
                                        .child("Waiting to steer running turn..."),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .text_xs()
                                .text_color(pending_colors.meta)
                                .child(format!("{} | {}s", attachment_status, elapsed_seconds)),
                        ),
                ),
        )
        .into_any_element()
}

fn render_ai_queued_message(
    queued: &AiQueuedUserMessage,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let pending_colors = hunk_pending_message(cx.theme(), is_dark);
    let elapsed_seconds = queued.queued_at.elapsed().as_secs();
    let attachment_status = match queued.local_images.len() {
        0 => "No attachments".to_string(),
        1 => "1 attachment".to_string(),
        count => format!("{count} attachments"),
    };
    let message_text = if queued.prompt.trim().is_empty() {
        attachment_status.clone()
    } else {
        queued.prompt.clone()
    };

    h_flex()
        .w_full()
        .min_w_0()
        .justify_end()
        .child(
            v_flex()
                .max_w(px(680.0))
                .w_full()
                .min_w_0()
                .gap_1p5()
                .child(
                    div()
                        .whitespace_nowrap()
                        .text_xs()
                        .font_semibold()
                        .child("You"),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .text_color(pending_colors.text)
                        .whitespace_normal()
                        .child(message_text),
                )
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().accent)
                                .child("queued, waiting for current turn to finish."),
                        )
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .text_xs()
                                .text_color(pending_colors.meta)
                                .child(format!("{} | {}s", attachment_status, elapsed_seconds)),
                        ),
                ),
        )
        .into_any_element()
}
