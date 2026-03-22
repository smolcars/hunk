#[derive(Clone)]
struct AiTerminalResizeDrag(EntityId);

impl Render for AiTerminalResizeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

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
        let entity_id = cx.entity_id();
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
                .relative()
                .border_t_1()
                .border_color(shell_colors.border)
                .bg(shell_colors.background)
                .child(
                    canvas(
                        {
                            let view = view.clone();
                            move |bounds, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.ai_update_terminal_panel_bounds(bounds, cx);
                                });
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(
                    v_flex()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .child(
                            h_flex()
                                .id("ai-terminal-resize-handle")
                                .w_full()
                                .h(px(6.0))
                                .flex_none()
                                .cursor_row_resize()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_drag(AiTerminalResizeDrag(entity_id), |drag, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.new(|_| drag.clone())
                                })
                                .on_drag_move(cx.listener(
                                    move |this,
                                          event: &DragMoveEvent<AiTerminalResizeDrag>,
                                          _,
                                          cx| {
                                        if event.drag(cx).0 != entity_id {
                                            return;
                                        }
                                        this.ai_resize_terminal_height_from_position(
                                            event.event.position,
                                            cx,
                                        );
                                    },
                                ))
                                .child(
                                    div()
                                        .mt(px(2.0))
                                        .h(px(1.0))
                                        .w_full()
                                        .bg(hunk_opacity(shell_colors.border, is_dark, 0.72, 0.58)),
                                ),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .px_3()
                                .py_1()
                                .border_b_1()
                                .border_color(hunk_opacity(shell_colors.border, is_dark, 0.90, 0.74))
                                .child(
                                    h_flex()
                                        .min_w_0()
                                        .items_center()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_semibold()
                                                .text_color(cx.theme().foreground)
                                                .child("Terminal"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
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
                                .on_action(cx.listener(Self::ai_terminal_send_ctrl_c_action))
                                .on_action(cx.listener(Self::ai_terminal_send_ctrl_a_action))
                                .on_action(cx.listener(Self::ai_terminal_send_tab_action))
                                .on_action(cx.listener(Self::ai_terminal_send_back_tab_action))
                                .on_action(cx.listener(Self::ai_terminal_send_up_action))
                                .on_action(cx.listener(Self::ai_terminal_send_down_action))
                                .on_action(cx.listener(Self::ai_terminal_send_left_action))
                                .on_action(cx.listener(Self::ai_terminal_send_right_action))
                                .on_action(cx.listener(Self::ai_terminal_send_home_action))
                                .on_action(cx.listener(Self::ai_terminal_send_end_action))
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
                        ),
                )
                .into_any_element(),
        )
    }
}
