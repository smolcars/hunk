#[derive(Clone)]
struct AiTerminalResizeDrag(EntityId);

impl Render for AiTerminalResizeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl DiffViewer {
    fn render_workspace_terminal_panel(
        &self,
        view: Entity<Self>,
        state: &TerminalPanelState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !state.open {
            return None;
        }

        let status_color = match state.status {
            AiTerminalSessionStatus::Idle => cx.theme().muted_foreground,
            AiTerminalSessionStatus::Running => cx.theme().accent,
            AiTerminalSessionStatus::Completed => cx.theme().success,
            AiTerminalSessionStatus::Failed => cx.theme().danger,
            AiTerminalSessionStatus::Stopped => cx.theme().warning,
        };
        let shell_colors = hunk_git_workspace(cx.theme(), is_dark).shell;
        let chrome = hunk_editor_chrome_colors(cx.theme(), is_dark);
        let entity_id = cx.entity_id();
        let kind = state.kind;
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
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_update_terminal_panel_bounds(bounds, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_update_terminal_panel_bounds(bounds, cx);
                                        }
                                    }
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
                                .id(match kind {
                                    WorkspaceTerminalKind::Ai => "ai-terminal-resize-handle",
                                    WorkspaceTerminalKind::Files => "files-terminal-resize-handle",
                                })
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
                                        match kind {
                                            WorkspaceTerminalKind::Ai => {
                                                this.ai_resize_terminal_height_from_position(
                                                    event.event.position,
                                                    cx,
                                                );
                                            }
                                            WorkspaceTerminalKind::Files => {
                                                this.files_resize_terminal_height_from_position(
                                                    event.event.position,
                                                    cx,
                                                );
                                            }
                                        }
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
                                .h(px(30.0))
                                .items_center()
                                .gap_1()
                                .px_2()
                                .border_b_1()
                                .border_color(hunk_opacity(shell_colors.border, is_dark, 0.82, 0.68))
                                .bg(shell_colors.background)
                                .children(state.tabs.iter().map(|tab| {
                                    let view = view.clone();
                                    let tab_id = tab.id;
                                    let selected = tab_id == state.active_tab_id;
                                    let dot_color = match tab.status {
                                        AiTerminalSessionStatus::Idle => cx.theme().muted_foreground,
                                        AiTerminalSessionStatus::Running => cx.theme().accent,
                                        AiTerminalSessionStatus::Completed => cx.theme().success,
                                        AiTerminalSessionStatus::Failed => cx.theme().danger,
                                        AiTerminalSessionStatus::Stopped => cx.theme().warning,
                                    };
                                    h_flex()
                                        .id(match kind {
                                            WorkspaceTerminalKind::Ai => {
                                                SharedString::from(format!("ai-terminal-tab-{tab_id}"))
                                            }
                                            WorkspaceTerminalKind::Files => {
                                                SharedString::from(format!("files-terminal-tab-{tab_id}"))
                                            }
                                        })
                                        .h(px(24.0))
                                        .max_w(px(160.0))
                                        .min_w(px(72.0))
                                        .items_center()
                                        .gap_1()
                                        .px_2()
                                        .rounded(px(6.0))
                                        .bg(if selected {
                                            hunk_opacity(cx.theme().accent, is_dark, 0.18, 0.12)
                                        } else {
                                            hunk_opacity(shell_colors.border, is_dark, 0.20, 0.16)
                                        })
                                        .border_1()
                                        .border_color(if selected {
                                            hunk_opacity(cx.theme().accent, is_dark, 0.54, 0.42)
                                        } else {
                                            hunk_opacity(shell_colors.border, is_dark, 0.52, 0.42)
                                        })
                                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                            view.update(cx, |this, cx| match kind {
                                                WorkspaceTerminalKind::Ai => {
                                                    this.ai_select_terminal_tab(tab_id, cx);
                                                }
                                                WorkspaceTerminalKind::Files => {
                                                    this.files_select_terminal_tab(tab_id, None, cx);
                                                }
                                            });
                                        })
                                        .child(
                                            div()
                                                .size(px(6.0))
                                                .rounded(px(999.0))
                                                .bg(hunk_opacity(dot_color, is_dark, 0.94, 0.86)),
                                        )
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_xs()
                                                .text_color(if selected {
                                                    cx.theme().foreground
                                                } else {
                                                    cx.theme().muted_foreground
                                                })
                                                .child(tab.title.clone()),
                                        )
                                        .into_any_element()
                                }))
                                .child({
                                    let view = view.clone();
                                    Button::new(match kind {
                                        WorkspaceTerminalKind::Ai => "ai-terminal-new-tab",
                                        WorkspaceTerminalKind::Files => "files-terminal-new-tab",
                                    })
                                    .compact()
                                    .ghost()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(8.0))
                                    .icon(Icon::new(IconName::Plus).size(px(13.0)))
                                    .tooltip("New terminal tab")
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| match kind {
                                            WorkspaceTerminalKind::Ai => this.ai_new_terminal_tab_action(cx),
                                            WorkspaceTerminalKind::Files => {
                                                this.files_new_terminal_tab_action(Some(window), cx);
                                            }
                                        });
                                    })
                                }),
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
                                                    .text_color(if state.status
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
                                                        match kind {
                                                            WorkspaceTerminalKind::Ai => {
                                                                this.ai_scroll_terminal_to_bottom_action(cx);
                                                            }
                                                            WorkspaceTerminalKind::Files => {
                                                                this.files_scroll_terminal_to_bottom_action(cx);
                                                            }
                                                        }
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
                                                        match kind {
                                                            WorkspaceTerminalKind::Ai => {
                                                                this.ai_rerun_terminal_command_action(cx);
                                                            }
                                                            WorkspaceTerminalKind::Files => {
                                                                this.files_rerun_terminal_command_action(cx);
                                                            }
                                                        }
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
                                                        match kind {
                                                            WorkspaceTerminalKind::Ai => {
                                                                this.ai_clear_terminal_session_action(cx);
                                                            }
                                                            WorkspaceTerminalKind::Files => {
                                                                this.files_clear_terminal_session_action(cx);
                                                            }
                                                        }
                                                    });
                                                })
                                            })
                                        })
                                        .child({
                                            let view = view.clone();
                                    Button::new(match kind {
                                        WorkspaceTerminalKind::Ai => "ai-terminal-hide",
                                        WorkspaceTerminalKind::Files => "files-terminal-hide",
                                    })
                                                .compact()
                                                .ghost()
                                                .with_size(gpui_component::Size::Small)
                                                .rounded(px(8.0))
                                                .icon(Icon::new(IconName::Close).size(px(14.0)))
                                                .tooltip("Hide terminal")
                                                .on_click(move |_, window, cx| {
                                                    view.update(cx, |this, cx| {
                                                        match kind {
                                                            WorkspaceTerminalKind::Ai => {
                                                                this.ai_toggle_terminal_drawer_action(cx);
                                                            }
                                                            WorkspaceTerminalKind::Files => {
                                                                this.files_toggle_terminal_drawer_action(window, cx);
                                                            }
                                                        }
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
                                .key_context(match kind {
                                    WorkspaceTerminalKind::Ai => "AiTerminal",
                                    WorkspaceTerminalKind::Files => "FilesTerminal",
                                })
                                .track_focus(match kind {
                                    WorkspaceTerminalKind::Ai => &self.ai_terminal_focus_handle,
                                    WorkspaceTerminalKind::Files => &self.files_terminal_focus_handle,
                                })
                                .on_action(cx.listener(move |this, _: &AiTerminalSendCtrlC, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_ctrl_c_action(&AiTerminalSendCtrlC, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_ctrl_c_action(&AiTerminalSendCtrlC, window, cx);
                                        }
                                    }
                                }))
                                .on_action(cx.listener(move |this, _: &AiTerminalSendCtrlA, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_ctrl_a_action(&AiTerminalSendCtrlA, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_ctrl_a_action(&AiTerminalSendCtrlA, window, cx);
                                        }
                                    }
                                }))
                                .on_action(cx.listener(move |this, _: &AiTerminalSendTab, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_tab_action(&AiTerminalSendTab, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_tab_action(&AiTerminalSendTab, window, cx);
                                        }
                                    }
                                }))
                                .on_action(cx.listener(move |this, _: &AiTerminalSendBackTab, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_back_tab_action(&AiTerminalSendBackTab, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_back_tab_action(&AiTerminalSendBackTab, window, cx);
                                        }
                                    }
                                }))
                                .on_action(cx.listener(move |this, _: &AiTerminalSendUp, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_up_action(&AiTerminalSendUp, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_up_action(&AiTerminalSendUp, window, cx);
                                        }
                                    }
                                }))
                                .on_action(cx.listener(move |this, _: &AiTerminalSendDown, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_down_action(&AiTerminalSendDown, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_down_action(&AiTerminalSendDown, window, cx);
                                        }
                                    }
                                }))
                                .on_action(cx.listener(move |this, _: &AiTerminalSendLeft, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_left_action(&AiTerminalSendLeft, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_left_action(&AiTerminalSendLeft, window, cx);
                                        }
                                    }
                                }))
                                .on_action(cx.listener(move |this, _: &AiTerminalSendRight, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_right_action(&AiTerminalSendRight, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_right_action(&AiTerminalSendRight, window, cx);
                                        }
                                    }
                                }))
                                .on_action(cx.listener(move |this, _: &AiTerminalSendHome, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_home_action(&AiTerminalSendHome, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_home_action(&AiTerminalSendHome, window, cx);
                                        }
                                    }
                                }))
                                .on_action(cx.listener(move |this, _: &AiTerminalSendEnd, window, cx| {
                                    match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_terminal_send_end_action(&AiTerminalSendEnd, window, cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_terminal_send_end_action(&AiTerminalSendEnd, window, cx);
                                        }
                                    }
                                }))
                                .on_mouse_down(MouseButton::Left, {
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            match kind {
                                                WorkspaceTerminalKind::Ai => {
                                                    this.ai_focus_terminal_surface_action(cx);
                                                }
                                                WorkspaceTerminalKind::Files => {
                                                    this.files_focus_terminal_surface_action(cx);
                                                }
                                            }
                                        });
                                    }
                                })
                                .on_key_down({
                                    let view = view.clone();
                                    move |event, window, cx| {
                                        let handled = view.update(cx, |this, cx| {
                                            match kind {
                                                WorkspaceTerminalKind::Ai => {
                                                    this.ai_terminal_surface_key_down(
                                                        &event.keystroke,
                                                        window,
                                                        cx,
                                                    )
                                                }
                                                WorkspaceTerminalKind::Files => {
                                                    this.files_terminal_surface_key_down(
                                                        &event.keystroke,
                                                        window,
                                                        cx,
                                                    )
                                                }
                                            }
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
                                .child(self.render_workspace_terminal_surface(state, is_dark, cx)),
                        ),
                )
                .into_any_element(),
        )
    }
}
