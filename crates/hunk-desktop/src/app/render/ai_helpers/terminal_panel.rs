impl DiffViewer {
    fn render_ai_terminal_panel(
        &self,
        view: Entity<Self>,
        state: &AiTerminalPanelState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !state.open {
            return None;
        }

        let status_color = match self.ai_terminal_session.status {
            AiTerminalSessionStatus::Idle => cx.theme().muted_foreground,
            AiTerminalSessionStatus::Running => cx.theme().accent,
            AiTerminalSessionStatus::Completed => cx.theme().success,
            AiTerminalSessionStatus::Failed => cx.theme().danger,
            AiTerminalSessionStatus::Stopped => cx.theme().warning,
        };

        Some(
            v_flex()
                .w_full()
                .h(px(state.height_px))
                .min_h(px(160.0))
                .border_t_1()
                .border_color(hunk_opacity(cx.theme().border, is_dark, 0.86, 0.72))
                .bg(hunk_blend(
                    cx.theme().background,
                    cx.theme().secondary,
                    is_dark,
                    0.18,
                    0.14,
                ))
                .child(
                    v_flex()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .gap_2()
                        .px_4()
                        .pt_3()
                        .pb_3()
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    v_flex()
                                        .min_w_0()
                                        .gap_0p5()
                                        .child(
                                            h_flex()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_semibold()
                                                        .child("Terminal"),
                                                )
                                                .child(
                                                    div()
                                                        .rounded(px(999.0))
                                                        .border_1()
                                                        .border_color(hunk_opacity(
                                                            status_color,
                                                            is_dark,
                                                            0.80,
                                                            0.68,
                                                        ))
                                                        .bg(hunk_opacity(
                                                            status_color,
                                                            is_dark,
                                                            0.12,
                                                            0.08,
                                                        ))
                                                        .px_1p5()
                                                        .py_0p5()
                                                        .text_xs()
                                                        .text_color(status_color)
                                                        .child(state.status_label),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .truncate()
                                                .child(state.cwd_label.clone()),
                                        ),
                                )
                                .child(
                                    h_flex()
                                        .flex_none()
                                        .items_center()
                                        .gap_1()
                                        .child({
                                            let view = view.clone();
                                            Button::new("ai-terminal-smaller")
                                                .compact()
                                                .ghost()
                                                .rounded(px(8.0))
                                                .with_size(gpui_component::Size::Small)
                                                .label("-")
                                                .tooltip("Reduce terminal height")
                                                .on_click(move |_, _, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.ai_decrease_terminal_height_action(cx);
                                                    });
                                                })
                                        })
                                        .child({
                                            let view = view.clone();
                                            Button::new("ai-terminal-larger")
                                                .compact()
                                                .ghost()
                                                .rounded(px(8.0))
                                                .with_size(gpui_component::Size::Small)
                                                .label("+")
                                                .tooltip("Increase terminal height")
                                                .on_click(move |_, _, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.ai_increase_terminal_height_action(cx);
                                                    });
                                                })
                                        })
                                        .child({
                                            let view = view.clone();
                                            Button::new("ai-terminal-hide")
                                                .compact()
                                                .ghost()
                                                .rounded(px(8.0))
                                                .with_size(gpui_component::Size::Small)
                                                .label("Hide")
                                                .on_click(move |_, _, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.ai_toggle_terminal_drawer_action(cx);
                                                    });
                                                })
                                        }),
                                ),
                        )
                        .when_some(state.status_message.clone(), |this, status_message| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(if self.ai_terminal_session.status
                                        == AiTerminalSessionStatus::Failed
                                    {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(status_message),
                            )
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .rounded(px(10.0))
                                .border_1()
                                .border_color(hunk_opacity(cx.theme().border, is_dark, 0.72, 0.58))
                                .bg(hunk_blend(
                                    cx.theme().background,
                                    cx.theme().secondary,
                                    is_dark,
                                    0.44,
                                    0.34,
                                ))
                                .overflow_y_scrollbar()
                                .p_3()
                                .child(
                                    v_flex()
                                        .w_full()
                                        .gap_0p5()
                                        .children(if state.has_transcript {
                                            state
                                                .transcript
                                                .lines()
                                                .map(|line| {
                                                    div()
                                                        .w_full()
                                                        .text_xs()
                                                        .font_family(
                                                            cx.theme().mono_font_family.clone(),
                                                        )
                                                        .text_color(cx.theme().foreground)
                                                        .child(line.to_string())
                                                        .into_any_element()
                                                })
                                                .collect::<Vec<_>>()
                                        } else {
                                            vec![
                                                div()
                                                    .w_full()
                                                    .text_xs()
                                                    .font_family(
                                                        cx.theme().mono_font_family.clone(),
                                                    )
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(
                                                        "Run a command to start a terminal session.",
                                                    )
                                                    .into_any_element(),
                                            ]
                                        }),
                                ),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .items_end()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            Input::new(&self.ai_terminal_input_state)
                                                .appearance(false)
                                                .bordered(true)
                                                .focus_bordered(true)
                                                .w_full()
                                                .disabled(!state.can_run || state.running),
                                        ),
                                )
                                .child({
                                    let view = view.clone();
                                    Button::new("ai-terminal-run")
                                        .compact()
                                        .outline()
                                        .rounded(px(8.0))
                                        .label("Run")
                                        .disabled(!state.can_run || state.running)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_run_terminal_command_action(cx);
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("ai-terminal-stop")
                                        .compact()
                                        .ghost()
                                        .rounded(px(8.0))
                                        .label("Stop")
                                        .disabled(!state.running)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_stop_terminal_command_action(cx);
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("ai-terminal-rerun")
                                        .compact()
                                        .ghost()
                                        .rounded(px(8.0))
                                        .label("Rerun")
                                        .disabled(!state.has_last_command || state.running)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_rerun_terminal_command_action(cx);
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("ai-terminal-clear")
                                        .compact()
                                        .ghost()
                                        .rounded(px(8.0))
                                        .label("Clear")
                                        .disabled(!state.has_transcript)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_clear_terminal_session_action(cx);
                                            });
                                        })
                                }),
                        ),
                )
                .into_any_element(),
        )
    }
}
