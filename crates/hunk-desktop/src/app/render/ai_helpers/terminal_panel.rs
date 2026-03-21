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
        let shell_colors = hunk_git_workspace(cx.theme(), is_dark).shell;
        let chrome = hunk_editor_chrome_colors(cx.theme(), is_dark);
        let has_session = state.screen.is_some() || state.has_transcript;
        let show_inline_prompt = !state.running && state.accepts_input;
        let status_text = state.status_message.clone().or_else(|| {
            if state.display_offset > 0 {
                Some("Viewing scrollback".to_string())
            } else {
                None
            }
        });

        Some(
            v_flex()
                .w_full()
                .h(px(state.height_px))
                .min_h(px(160.0))
                .border_t_1()
                .border_color(shell_colors.border)
                .bg(shell_colors.background)
                .child(
                    v_flex()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .px_3()
                                .py_2()
                                .border_b_1()
                                .border_color(hunk_opacity(shell_colors.border, is_dark, 0.92, 0.78))
                                .child(
                                    h_flex()
                                        .min_w_0()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_semibold()
                                                .text_color(cx.theme().foreground)
                                                .child("Terminal"),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .text_color(cx.theme().muted_foreground)
                                                .child(state.shell_label.clone()),
                                        )
                                        .child(
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded(px(999.0))
                                                .bg(hunk_opacity(status_color, is_dark, 0.95, 0.90)),
                                        )
                                        .when_some(status_text, |this, status_text| {
                                            this.child(
                                                div()
                                                    .min_w_0()
                                                    .text_xs()
                                                    .text_color(if self.ai_terminal_session.status
                                                        == AiTerminalSessionStatus::Failed
                                                    {
                                                        cx.theme().danger
                                                    } else {
                                                        cx.theme().muted_foreground
                                                    })
                                                    .truncate()
                                                    .child(status_text),
                                            )
                                        }),
                                )
                                .child(
                                    h_flex()
                                        .flex_none()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            div()
                                                .max_w(px(460.0))
                                                .truncate()
                                                .text_xs()
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .text_color(cx.theme().muted_foreground)
                                                .child(state.cwd_label.clone()),
                                        )
                                        .when(state.display_offset > 0, |this| {
                                            this.child({
                                                let view = view.clone();
                                                Button::new("ai-terminal-bottom")
                                                    .compact()
                                                    .ghost()
                                                    .with_size(gpui_component::Size::Small)
                                                    .rounded(px(8.0))
                                                    .label("Bottom")
                                                    .on_click(move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.ai_scroll_terminal_to_bottom_action(cx);
                                                        });
                                                    })
                                            })
                                        })
                                        .when(state.running, |this| {
                                            this.child({
                                                let view = view.clone();
                                                Button::new("ai-terminal-stop")
                                                    .compact()
                                                    .ghost()
                                                    .with_size(gpui_component::Size::Small)
                                                    .rounded(px(8.0))
                                                    .label("Stop")
                                                    .on_click(move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.ai_stop_terminal_command_action(cx);
                                                        });
                                                    })
                                            })
                                        })
                                        .when(!state.running && state.has_last_command, |this| {
                                            this.child({
                                                let view = view.clone();
                                                Button::new("ai-terminal-rerun")
                                                    .compact()
                                                    .ghost()
                                                    .with_size(gpui_component::Size::Small)
                                                    .rounded(px(8.0))
                                                    .icon(Icon::new(IconName::Undo2).size(px(12.0)))
                                                    .tooltip("Rerun last command")
                                                    .on_click(move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.ai_rerun_terminal_command_action(cx);
                                                        });
                                                    })
                                            })
                                        })
                                        .when(!state.running && state.has_output, |this| {
                                            this.child({
                                                let view = view.clone();
                                                Button::new("ai-terminal-clear")
                                                    .compact()
                                                    .ghost()
                                                    .with_size(gpui_component::Size::Small)
                                                    .rounded(px(8.0))
                                                    .icon(Icon::new(IconName::Delete).size(px(12.0)))
                                                    .tooltip("Clear terminal session")
                                                    .on_click(move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.ai_clear_terminal_session_action(cx);
                                                        });
                                                    })
                                            })
                                        })
                                        .child({
                                            let view = view.clone();
                                            Button::new("ai-terminal-hide")
                                                .compact()
                                                .ghost()
                                                .with_size(gpui_component::Size::Small)
                                                .rounded(px(8.0))
                                                .icon(Icon::new(IconName::Close).size(px(14.0)))
                                                .tooltip("Hide terminal")
                                                .on_click(move |_, _, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.ai_toggle_terminal_drawer_action(cx);
                                                    });
                                                })
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .flex_1()
                                .min_h_0()
                                .bg(chrome.background)
                                .key_context("AiTerminal")
                                .track_focus(&self.ai_terminal_focus_handle)
                                .on_mouse_down(MouseButton::Left, {
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_focus_terminal_surface_action(cx);
                                        });
                                    }
                                })
                                .on_key_down({
                                    let view = view.clone();
                                    move |event, window, cx| {
                                        let handled = view.update(cx, |this, cx| {
                                            this.ai_terminal_surface_key_down(
                                                &event.keystroke,
                                                window,
                                                cx,
                                            )
                                        });
                                        if handled {
                                            cx.stop_propagation();
                                        }
                                    }
                                })
                                .px_3()
                                .pt_2()
                                .pb_2()
                                .when(state.surface_focused, |this| {
                                    this.border_1()
                                        .border_color(hunk_opacity(status_color, is_dark, 0.82, 0.68))
                                })
                                .when(!state.surface_focused, |this| this.border_1().border_color(chrome.background))
                                .child(self.render_ai_terminal_surface(state, is_dark, cx)),
                        )
                        .when(show_inline_prompt, |this| {
                            this.child(
                                h_flex()
                                    .w_full()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .py_1p5()
                                    .border_t_1()
                                    .border_color(hunk_opacity(shell_colors.border, is_dark, 0.92, 0.78))
                                    .bg(hunk_blend(chrome.background, shell_colors.background, is_dark, 0.08, 0.12))
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_sm()
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .text_color(if has_session {
                                                cx.theme().muted_foreground
                                            } else {
                                                status_color
                                            })
                                            .child(if cfg!(target_os = "windows") { ">" } else { "$" }),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(
                                                Input::new(&self.ai_terminal_input_state)
                                                    .appearance(false)
                                                    .bordered(false)
                                                    .focus_bordered(false)
                                                    .w_full()
                                                    .disabled(!state.accepts_input),
                                            ),
                                    ),
                            )
                        }),
                )
                .into_any_element(),
        )
    }
}
