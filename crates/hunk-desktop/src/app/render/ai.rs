use hunk_codex::state::ItemStatus;
use hunk_codex::state::ThreadLifecycleStatus;
use hunk_codex::state::TurnStatus;

impl DiffViewer {
    fn render_ai_workspace_screen(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.repo_discovery_failed {
            return self.render_open_project_empty_state(cx);
        }

        if let Some(error_message) = &self.error_message {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error_message.clone()),
                )
                .into_any_element();
        }

        let is_dark = cx.theme().mode.is_dark();
        let view = cx.entity();
        let active_bookmark = self
            .checked_out_bookmark_name()
            .map_or_else(|| "detached".to_string(), ToOwned::to_owned);
        let threads = self.ai_visible_threads();
        let pending_approvals = self.ai_visible_pending_approvals();
        let pending_approvals_for_timeline = pending_approvals.clone();
        let pending_approval_count = pending_approvals.len();
        let pending_user_inputs = self.ai_visible_pending_user_inputs();
        let pending_user_inputs_for_timeline = pending_user_inputs.clone();
        let pending_user_input_count = pending_user_inputs.len();
        let selected_thread_id = self.current_ai_thread_id();
        let in_progress_turn = selected_thread_id
            .as_ref()
            .and_then(|thread_id| self.current_ai_in_progress_turn_id(thread_id.as_str()));
        let (connection_label, connection_color) = ai_connection_label(self.ai_connection_state, cx);

        v_flex()
            .size_full()
            .w_full()
            .min_h_0()
            .key_context("AiWorkspace")
            .child(
                h_flex()
                    .w_full()
                    .min_h(px(48.0))
                    .items_center()
                    .justify_between()
                    .py_2()
                    .px_3()
                    .gap_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().muted.opacity(if is_dark { 0.32 } else { 0.62 }))
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(div().text_sm().font_semibold().child("Codex Agent Workspace")),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .items_end()
                            .gap_1()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap_3()
                                    .flex_wrap()
                                    .justify_end()
                                    .child(render_ai_account_actions_for_view(
                                        self,
                                        view.clone(),
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("Active bookmark: {active_bookmark}")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if pending_approval_count > 0 {
                                                cx.theme().warning
                                            } else {
                                                cx.theme().muted_foreground
                                            })
                                            .child(format!("Approvals: {pending_approval_count}")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if pending_user_input_count > 0 {
                                                cx.theme().warning
                                            } else {
                                                cx.theme().muted_foreground
                                            })
                                            .child(format!("Inputs: {pending_user_input_count}")),
                                    )
                                    .child({
                                        let view = view.clone();
                                        let enable_mad_max = !self.ai_mad_max_mode;
                                        Button::new("ai-toggle-mad-max")
                                            .compact()
                                            .outline()
                                            .with_size(gpui_component::Size::Small)
                                            .label(if self.ai_mad_max_mode {
                                                "Mad Max On"
                                            } else {
                                                "Mad Max Off"
                                            })
                                            .on_click(move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.ai_set_mad_max_mode(enable_mad_max, cx);
                                                });
                                            })
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(connection_color)
                                            .child(connection_label),
                                    ),
                            )
                            .child(render_ai_account_panel_for_view(self, view.clone(), cx)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(
                        h_resizable("hunk-ai-workspace")
                            .child(
                                resizable_panel()
                                    .size(px(300.0))
                                    .size_range(px(240.0)..px(440.0))
                                    .child(
                                        v_flex()
                                            .size_full()
                                            .min_h_0()
                                            .border_r_1()
                                            .border_color(cx.theme().border)
                                            .bg(cx.theme().sidebar.opacity(if is_dark { 0.95 } else { 0.98 }))
                                            .child(
                                                h_flex()
                                                    .w_full()
                                                    .h_10()
                                                    .items_center()
                                                    .justify_between()
                                                    .px_2()
                                                    .border_b_1()
                                                    .border_color(cx.theme().border)
                                                    .child(div().text_sm().font_semibold().child("Threads"))
                                                    .child(
                                                        h_flex()
                                                            .items_center()
                                                            .gap_1()
                                                            .child({
                                                                let view = view.clone();
                                                                Button::new("ai-thread-refresh")
                                                                    .compact()
                                                                    .outline()
                                                                    .with_size(gpui_component::Size::Small)
                                                                    .label("Refresh")
                                                                    .on_click(move |_, _, cx| {
                                                                        view.update(cx, |this, cx| {
                                                                            this.ai_refresh_threads(cx);
                                                                        });
                                                                    })
                                                            })
                                                            .child({
                                                                let view = view.clone();
                                                                Button::new("ai-thread-new")
                                                                    .compact()
                                                                    .primary()
                                                                    .with_size(gpui_component::Size::Small)
                                                                    .label("New")
                                                                    .on_click(move |_, window, cx| {
                                                                        view.update(cx, |this, cx| {
                                                                            this.ai_create_thread_action(window, cx);
                                                                        });
                                                                    })
                                                            }),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_h_0()
                                                    .relative()
                                                    .child(
                                                        div()
                                                            .id("ai-thread-list-scroll-area")
                                                            .size_full()
                                                            .track_scroll(&self.ai_thread_list_scroll_handle)
                                                            .overflow_y_scroll()
                                                            .child(
                                                                v_flex()
                                                                    .w_full()
                                                                    .gap_1()
                                                                    .p_2()
                                                                    .when(threads.is_empty(), |this| {
                                                                        this.child(
                                                                            div()
                                                                                .rounded_md()
                                                                                .border_1()
                                                                                .border_color(cx.theme().border)
                                                                                .bg(cx.theme().muted.opacity(if is_dark {
                                                                                    0.22
                                                                                } else {
                                                                                    0.40
                                                                                }))
                                                                                .p_2()
                                                                                .child(
                                                                                    div()
                                                                                        .text_xs()
                                                                                        .text_color(
                                                                                            cx.theme().muted_foreground,
                                                                                        )
                                                                                        .child(
                                                                                            "No threads in this workspace yet.",
                                                                                        ),
                                                                                ),
                                                                        )
                                                                    })
                                                                    .children(threads.into_iter().map(|thread| {
                                                                        let thread_id = thread.id.clone();
                                                                        let title = thread
                                                                            .title
                                                                            .clone()
                                                                            .unwrap_or_else(|| thread.id.clone());
                                                                        let selected = selected_thread_id
                                                                            .as_deref()
                                                                            == Some(thread.id.as_str());
                                                                        let thread_border_color = if selected {
                                                                            cx.theme().success.opacity(if is_dark {
                                                                                0.98
                                                                            } else {
                                                                                0.90
                                                                            })
                                                                        } else {
                                                                            cx.theme().border.opacity(if is_dark {
                                                                                0.90
                                                                            } else {
                                                                                0.74
                                                                            })
                                                                        };
                                                                        let thread_bg = if selected {
                                                                            cx.theme().background.blend(
                                                                                cx.theme().success.opacity(if is_dark {
                                                                                    0.28
                                                                                } else {
                                                                                    0.20
                                                                                }),
                                                                            )
                                                                        } else {
                                                                            cx.theme().background.blend(
                                                                                cx.theme().muted.opacity(if is_dark {
                                                                                    0.16
                                                                                } else {
                                                                                    0.28
                                                                                }),
                                                                            )
                                                                        };
                                                                        let thread_hover_bg = if selected {
                                                                            cx.theme().background.blend(
                                                                                cx.theme().success.opacity(if is_dark {
                                                                                    0.38
                                                                                } else {
                                                                                    0.30
                                                                                }),
                                                                            )
                                                                        } else {
                                                                            cx.theme().secondary_hover
                                                                        };
                                                                        let thread_marker_color = if selected {
                                                                            if is_dark {
                                                                                cx.theme().success.lighten(0.18)
                                                                            } else {
                                                                                cx.theme().success.darken(0.08)
                                                                            }
                                                                        } else {
                                                                            cx.theme().border.opacity(if is_dark {
                                                                                0.82
                                                                            } else {
                                                                                0.68
                                                                            })
                                                                        };
                                                                        let thread_title_color = if selected {
                                                                            cx.theme().foreground
                                                                        } else {
                                                                            cx.theme().foreground.opacity(if is_dark {
                                                                                0.94
                                                                            } else {
                                                                                0.90
                                                                            })
                                                                        };
                                                                        let thread_id_color = if selected {
                                                                            cx.theme().success
                                                                        } else {
                                                                            cx.theme().muted_foreground
                                                                        };
                                                                        let (status_label, status_color) =
                                                                            ai_thread_status_label(
                                                                                thread.status,
                                                                                cx,
                                                                            );
                                                                        let view = view.clone();

                                                                        div()
                                                                            .rounded_md()
                                                                            .border_1()
                                                                            .when(selected, |this| this.border_2())
                                                                            .border_color(thread_border_color)
                                                                            .bg(thread_bg)
                                                                            .p_2()
                                                                            .gap_1()
                                                                            .hover(move |style| {
                                                                                style.bg(thread_hover_bg).cursor_pointer()
                                                                            })
                                                                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                                                                view.update(cx, |this, cx| {
                                                                                    this.ai_select_thread(thread_id.clone(), cx);
                                                                                });
                                                                            })
                                                                            .child(
                                                                                h_flex()
                                                                                    .w_full()
                                                                                    .items_center()
                                                                                    .justify_between()
                                                                                    .gap_2()
                                                                                    .child(
                                                                                        h_flex()
                                                                                            .min_w_0()
                                                                                            .items_center()
                                                                                            .gap_1()
                                                                                            .child(
                                                                                                div()
                                                                                                    .w(px(8.0))
                                                                                                    .h(px(8.0))
                                                                                                    .rounded(px(999.0))
                                                                                                    .bg(thread_marker_color),
                                                                                            )
                                                                                            .child(
                                                                                                div()
                                                                                                    .flex_1()
                                                                                                    .min_w_0()
                                                                                                    .text_sm()
                                                                                                    .font_medium()
                                                                                                    .text_color(thread_title_color)
                                                                                                    .truncate()
                                                                                                    .child(title),
                                                                                            ),
                                                                                    )
                                                                                    .child(
                                                                                        h_flex()
                                                                                            .items_center()
                                                                                            .gap_1()
                                                                                            .child(
                                                                                                div()
                                                                                                    .text_xs()
                                                                                                    .font_semibold()
                                                                                                    .text_color(status_color)
                                                                                                    .child(status_label),
                                                                                            ),
                                                                                    ),
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .text_xs()
                                                                                    .text_color(thread_id_color)
                                                                                    .font_family(
                                                                                        cx.theme().mono_font_family.clone(),
                                                                                    )
                                                                                    .truncate()
                                                                                    .child(thread.id),
                                                                            )
                                                                            .into_any_element()
                                                                    })),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .absolute()
                                                            .top_0()
                                                            .right_0()
                                                            .bottom_0()
                                                            .w(px(16.0))
                                                            .child(
                                                                Scrollbar::vertical(&self.ai_thread_list_scroll_handle)
                                                                    .scrollbar_show(ScrollbarShow::Always),
                                                            ),
                                                    ),
                                            ),
                                    ),
                            )
                            .child(
                                resizable_panel().child(
                                    v_flex()
                                        .size_full()
                                        .min_h_0()
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_h_0()
                                                .relative()
                                                .child(
                                                    div()
                                                        .id("ai-timeline-scroll-area")
                                                        .size_full()
                                                        .track_scroll(&self.ai_timeline_scroll_handle)
                                                        .overflow_y_scroll()
                                                        .child(
                                                            v_flex()
                                                                .w_full()
                                                                .min_h_0()
                                                                .gap_2()
                                                                .p_3()
                                                                .bg(cx.theme().background)
                                                .child(
                                                    h_flex()
                                                        .w_full()
                                                        .items_center()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_semibold()
                                                                .child("Timeline:"),
                                                        )
                                                        .when_some(
                                                            selected_thread_id.clone(),
                                                            |this, thread_id| {
                                                                let thread_id_hover_color =
                                                                    cx.theme().foreground;
                                                                let copy_thread_id = thread_id.clone();
                                                                let view = view.clone();
                                                                this.child(
                                                                    div()
                                                                        .text_xs()
                                                                        .text_color(
                                                                            cx.theme().muted_foreground,
                                                                        )
                                                                        .font_family(
                                                                            cx.theme()
                                                                                .mono_font_family
                                                                                .clone(),
                                                                        )
                                                                        .hover(move |style| {
                                                                            style
                                                                                .text_color(
                                                                                    thread_id_hover_color,
                                                                                )
                                                                                .cursor_pointer()
                                                                        })
                                                                        .on_mouse_down(
                                                                            MouseButton::Left,
                                                                            move |_, window, cx| {
                                                                                view.update(
                                                                                    cx,
                                                                                    |this, cx| {
                                                                                        this.ai_copy_thread_id_action(
                                                                                            copy_thread_id.clone(),
                                                                                            window,
                                                                                            cx,
                                                                                        );
                                                                                    },
                                                                                );
                                                                            },
                                                                        )
                                                                        .child(thread_id),
                                                                )
                                                            },
                                                        ),
                                                )
                                                .when_some(self.ai_error_message.clone(), |this, error| {
                                                    this.child(
                                                        div()
                                                            .rounded_md()
                                                            .border_1()
                                                            .border_color(cx.theme().danger)
                                                            .bg(cx.theme().danger.opacity(if is_dark {
                                                                0.16
                                                            } else {
                                                                0.10
                                                            }))
                                                            .p_2()
                                                            .text_xs()
                                                            .text_color(cx.theme().danger)
                                                            .whitespace_normal()
                                                            .child(error),
                                                    )
                                                })
                                                .when_some(
                                                    self.ai_status_message.clone(),
                                                    |this, status| {
                                                        this.child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(
                                                                    cx.theme().muted_foreground,
                                                                )
                                                                .whitespace_normal()
                                                            .child(status),
                                                        )
                                                    },
                                                )
                                                .when(
                                                    self.ai_mad_max_mode
                                                        || !pending_approvals_for_timeline.is_empty()
                                                        || !pending_user_inputs_for_timeline.is_empty(),
                                                    |this| {
                                                        this.child(
                                                            v_flex()
                                                                .w_full()
                                                                .gap_1()
                                                                .when(self.ai_mad_max_mode, |this| {
                                                                    this.child(
                                                                        div()
                                                                            .rounded_md()
                                                                            .border_1()
                                                                            .border_color(
                                                                                cx.theme().danger,
                                                                            )
                                                                            .bg(cx.theme().danger.opacity(if is_dark {
                                                                                0.16
                                                                            } else {
                                                                                0.10
                                                                            }))
                                                                            .p_2()
                                                                            .child(
                                                                                div()
                                                                                    .text_xs()
                                                                                    .font_semibold()
                                                                                    .text_color(
                                                                                        cx.theme().danger,
                                                                                    )
                                                                                    .child(
                                                                                        "Mad Max mode is enabled: approvals are auto-accepted with full sandbox access.",
                                                                                    ),
                                                                            ),
                                                                    )
                                                                })
                                                                .when(
                                                                    !pending_approvals_for_timeline
                                                                        .is_empty(),
                                                                    |this| {
                                                                        this.child(
                                                                            v_flex()
                                                                                .w_full()
                                                                                .gap_1()
                                                                                .rounded_md()
                                                                                .border_1()
                                                                                .border_color(
                                                                                    cx.theme().warning,
                                                                                )
                                                                                .bg(cx.theme().warning.opacity(if is_dark {
                                                                                    0.14
                                                                                } else {
                                                                                    0.08
                                                                                }))
                                                                                .p_2()
                                                                                .child(
                                                                                    div()
                                                                                        .text_xs()
                                                                                        .font_semibold()
                                                                                        .text_color(
                                                                                            cx.theme().warning,
                                                                                        )
                                                                                        .child(
                                                                                            "Pending approvals",
                                                                                        ),
                                                                                )
                                                                                .children(
                                                                                    pending_approvals_for_timeline
                                                                                        .iter()
                                                                                        .map(
                                                                                            |approval| {
                                                                                                let approve_request_id =
                                                                                                    approval
                                                                                                        .request_id
                                                                                                        .clone();
                                                                                                let decline_request_id =
                                                                                                    approval
                                                                                                        .request_id
                                                                                                        .clone();
                                                                                                let view =
                                                                                                    view.clone();
                                                                                                v_flex()
                                                                                                    .w_full()
                                                                                                    .gap_1()
                                                                                                    .rounded(px(
                                                                                                        8.0,
                                                                                                    ))
                                                                                                    .border_1()
                                                                                                    .border_color(
                                                                                                        cx.theme()
                                                                                                            .border,
                                                                                                    )
                                                                                                    .bg(cx.theme().background)
                                                                                                    .p_2()
                                                                                                    .child(
                                                                                                        h_flex()
                                                                                                            .w_full()
                                                                                                            .items_center()
                                                                                                            .justify_between()
                                                                                                            .gap_2()
                                                                                                            .child(
                                                                                                                div()
                                                                                                                    .text_xs()
                                                                                                                    .font_semibold()
                                                                                                                    .child(
                                                                                                                        ai_approval_kind_label(
                                                                                                                            approval
                                                                                                                                .kind,
                                                                                                                        ),
                                                                                                                    ),
                                                                                                            )
                                                                                                            .child(
                                                                                                                div()
                                                                                                                    .text_xs()
                                                                                                                    .text_color(
                                                                                                                        cx.theme()
                                                                                                                            .muted_foreground,
                                                                                                                    )
                                                                                                                    .font_family(
                                                                                                                        cx.theme()
                                                                                                                            .mono_font_family
                                                                                                                            .clone(),
                                                                                                                    )
                                                                                                                    .child(
                                                                                                                        approval
                                                                                                                            .request_id
                                                                                                                            .clone(),
                                                                                                                    ),
                                                                                                            ),
                                                                                                    )
                                                                                                    .child(
                                                                                                        div()
                                                                                                            .text_xs()
                                                                                                            .text_color(
                                                                                                                cx.theme()
                                                                                                                    .muted_foreground,
                                                                                                            )
                                                                                                            .whitespace_normal()
                                                                                                            .child(
                                                                                                                ai_approval_description(
                                                                                                                    approval,
                                                                                                                ),
                                                                                                            ),
                                                                                                    )
                                                                                                    .when_some(
                                                                                                        approval.reason
                                                                                                            .clone(),
                                                                                                        |this, reason| {
                                                                                                            this.child(
                                                                                                                div()
                                                                                                                    .text_xs()
                                                                                                                    .text_color(
                                                                                                                        cx.theme()
                                                                                                                            .muted_foreground,
                                                                                                                    )
                                                                                                                    .whitespace_normal()
                                                                                                                    .child(
                                                                                                                        reason,
                                                                                                                    ),
                                                                                                            )
                                                                                                        },
                                                                                                    )
                                                                                                    .child(
                                                                                                        h_flex()
                                                                                                            .w_full()
                                                                                                            .items_center()
                                                                                                            .gap_1()
                                                                                                            .child(
                                                                                                                {
                                                                                                                    let view =
                                                                                                                        view
                                                                                                                            .clone();
                                                                                                                    Button::new(
                                                                                                                        format!(
                                                                                                                            "ai-approval-accept-{}",
                                                                                                                            approval
                                                                                                                                .request_id
                                                                                                                        ),
                                                                                                                    )
                                                                                                                    .compact()
                                                                                                                    .primary()
                                                                                                                    .with_size(gpui_component::Size::Small)
                                                                                                                    .label("Accept")
                                                                                                                    .on_click(move |_, _, cx| {
                                                                                                                        view.update(cx, |this, cx| {
                                                                                                                            this.ai_resolve_pending_approval_action(
                                                                                                                                approve_request_id.clone(),
                                                                                                                                AiApprovalDecision::Accept,
                                                                                                                                cx,
                                                                                                                            );
                                                                                                                        });
                                                                                                                    })
                                                                                                                },
                                                                                                            )
                                                                                                            .child(
                                                                                                                {
                                                                                                                    Button::new(
                                                                                                                        format!(
                                                                                                                            "ai-approval-decline-{}",
                                                                                                                            approval
                                                                                                                                .request_id
                                                                                                                        ),
                                                                                                                    )
                                                                                                                    .compact()
                                                                                                                    .outline()
                                                                                                                    .with_size(gpui_component::Size::Small)
                                                                                                                    .label("Decline")
                                                                                                                    .on_click(move |_, _, cx| {
                                                                                                                        view.update(cx, |this, cx| {
                                                                                                                            this.ai_resolve_pending_approval_action(
                                                                                                                                decline_request_id.clone(),
                                                                                                                                AiApprovalDecision::Decline,
                                                                                                                                cx,
                                                                                                                            );
                                                                                                                        });
                                                                                                                    })
                                                                                                                },
                                                                                                            ),
                                                                                                    )
                                                                                            },
                                                                                        ),
                                                                                ),
                                                                        )
                                                                    },
                                                                )
                                                                .when(
                                                                    !pending_user_inputs_for_timeline
                                                                        .is_empty(),
                                                                    |this| {
                                                                        this.child(
                                                                            render_ai_pending_user_inputs_panel(
                                                                                pending_user_inputs_for_timeline
                                                                                    .as_slice(),
                                                                                &self
                                                                                    .ai_pending_user_input_answers,
                                                                                view.clone(),
                                                                                is_dark,
                                                                                cx,
                                                                            ),
                                                                        )
                                                                    },
                                                                ),
                                                        )
                                                    },
                                                )
                                                .when(selected_thread_id.is_none(), |this| {
                                                    this.child(
                                                        div()
                                                            .rounded_md()
                                                            .border_1()
                                                            .border_color(cx.theme().border)
                                                            .bg(cx.theme().muted.opacity(if is_dark {
                                                                0.22
                                                            } else {
                                                                0.40
                                                            }))
                                                            .p_3()
                                                            .child(
                                                                div()
                                                                    .text_sm()
                                                                    .text_color(
                                                                        cx.theme().muted_foreground,
                                                                    )
                                                                    .child(
                                                                        "Select a thread or start a new one to begin.",
                                                                    ),
                                                            ),
                                                    )
                                                })
                                                        .when_some(selected_thread_id.clone(), |this, thread_id| {
                                                            let turn_ids = self.ai_timeline_turn_ids(thread_id.as_str());
                                                            this.when(turn_ids.is_empty(), |this| {
                                                                this.child(
                                                                    div()
                                                                        .rounded_md()
                                                                        .border_1()
                                                                        .border_color(cx.theme().border)
                                                                        .bg(cx.theme().muted.opacity(if is_dark {
                                                                            0.22
                                                                        } else {
                                                                            0.40
                                                                        }))
                                                                        .p_3()
                                                                        .child(
                                                                            div()
                                                                                .text_sm()
                                                                                .text_color(
                                                                                    cx.theme().muted_foreground,
                                                                                )
                                                                                .child("No turns yet. Send a prompt to start."),
                                                                        ),
                                                                )
                                                            })
                                                            .children(turn_ids.into_iter().filter_map(|turn_id| {
                                                                let turn = self.ai_state_snapshot.turns.get(&turn_id)?;
                                                                let turn_status = ai_turn_status_label(turn.status);
                                                                let item_ids = self.ai_timeline_item_ids(turn_id.as_str());
                                                                let diff_preview = self
                                                                    .ai_state_snapshot
                                                                    .turn_diffs
                                                                    .get(turn_id.as_str())
                                                                    .cloned();

                                                                Some(
                                                                    v_flex()
                                                                        .w_full()
                                                                        .gap_1p5()
                                                                        .p_2()
                                                                        .rounded_md()
                                                                        .border_1()
                                                                        .border_color(cx.theme().border)
                                                                        .bg(cx.theme().background.blend(
                                                                            cx.theme().muted.opacity(if is_dark {
                                                                                0.20
                                                                            } else {
                                                                                0.30
                                                                            }),
                                                                        ))
                                                                        .child(
                                                                            h_flex()
                                                                                .w_full()
                                                                                .items_center()
                                                                                .justify_between()
                                                                                .child(
                                                                                    div()
                                                                                        .text_xs()
                                                                                        .font_semibold()
                                                                                        .child(format!(
                                                                                            "Turn {}",
                                                                                            turn.id
                                                                                        )),
                                                                                )
                                                                                .child(
                                                                                    div()
                                                                                        .text_xs()
                                                                                        .text_color(
                                                                                            if turn.status
                                                                                                == TurnStatus::Completed
                                                                                            {
                                                                                                cx.theme().success
                                                                                            } else {
                                                                                                cx.theme().warning
                                                                                            },
                                                                                        )
                                                                                        .child(turn_status),
                                                                                ),
                                                                        )
                                                                        .children(item_ids.into_iter().filter_map(|item_id| {
                                                                            let item = self.ai_state_snapshot.items.get(&item_id)?;
                                                                            let status = ai_item_status_label(item.status);
                                                                            let item_label = ai_item_display_label(item.kind.as_str()).to_string();
                                                                            let command_output_collapsible =
                                                                                item.kind == "commandExecution";
                                                                            let command_output_expanded = command_output_collapsible
                                                                                && self.ai_expanded_command_output_item_ids.contains(item.id.as_str());
                                                                            let (item_content, command_output_truncated) =
                                                                                if command_output_collapsible && !command_output_expanded {
                                                                                    ai_truncate_multiline_content(
                                                                                        item.content.as_str(),
                                                                                        3,
                                                                                    )
                                                                                } else {
                                                                                    (item.content.clone(), false)
                                                                                };

                                                                            Some(
                                                                                v_flex()
                                                                                    .w_full()
                                                                                    .gap_0p5()
                                                                                    .p_2()
                                                                                    .rounded(px(8.0))
                                                                                    .border_1()
                                                                                    .border_color(
                                                                                        cx.theme().border.opacity(if is_dark {
                                                                                            0.90
                                                                                        } else {
                                                                                            0.72
                                                                                        }),
                                                                                    )
                                                                                    .bg(cx.theme().background.blend(
                                                                                        cx.theme().muted.opacity(if is_dark {
                                                                                            0.10
                                                                                        } else {
                                                                                            0.16
                                                                                        }),
                                                                                    ))
                                                                                    .child(
                                                                                        h_flex()
                                                                                            .w_full()
                                                                                            .items_center()
                                                                                            .justify_between()
                                                                                            .child(
                                                                                                div()
                                                                                                    .text_xs()
                                                                                                    .font_medium()
                                                                                                    .child(item_label),
                                                                                            )
                                                                                            .child(
                                                                                                div()
                                                                                                    .text_xs()
                                                                                                    .text_color(
                                                                                                        ai_item_status_color(
                                                                                                            item.status,
                                                                                                            cx,
                                                                                                        ),
                                                                                                    )
                                                                                                    .child(status),
                                                                                            ),
                                                                                    )
                                                                                    .when(!item_content.is_empty(), |this| {
                                                                                        this.child(
                                                                                            div()
                                                                                                .text_xs()
                                                                                                .font_family(
                                                                                                    cx.theme()
                                                                                                        .mono_font_family
                                                                                                        .clone(),
                                                                                                )
                                                                                                .text_color(
                                                                                                    cx.theme()
                                                                                                        .muted_foreground,
                                                                                                )
                                                                                                .whitespace_normal()
                                                                                                .child(item_content.clone()),
                                                                                        )
                                                                                    })
                                                                                    .when(
                                                                                        command_output_collapsible
                                                                                            && (command_output_truncated
                                                                                                || command_output_expanded),
                                                                                        |this| {
                                                                                            let item_id = item.id.clone();
                                                                                            let view = view.clone();
                                                                                            this.child(
                                                                                                Button::new(
                                                                                                    format!(
                                                                                                        "ai-toggle-command-output-{item_id}"
                                                                                                    ),
                                                                                                )
                                                                                                .compact()
                                                                                                .outline()
                                                                                                .with_size(gpui_component::Size::Small)
                                                                                                .label(if command_output_expanded {
                                                                                                    "Collapse output"
                                                                                                } else {
                                                                                                    "Show full output"
                                                                                                })
                                                                                                .on_click(move |_, _, cx| {
                                                                                                    view.update(cx, |this, cx| {
                                                                                                        this.ai_toggle_command_output_expansion_action(
                                                                                                            item_id.clone(),
                                                                                                            cx,
                                                                                                        );
                                                                                                    });
                                                                                                }),
                                                                                            )
                                                                                        },
                                                                                    )
                                                                                    .into_any_element(),
                                                                            )
                                                                        }))
                                                                        .when_some(diff_preview, |this, diff| {
                                                                            let diff_line_count = diff.lines().count();
                                                                            this.child(
                                                                                h_flex()
                                                                                    .w_full()
                                                                                    .items_center()
                                                                                    .justify_between()
                                                                                    .gap_2()
                                                                                    .pt_1()
                                                                                    .border_t_1()
                                                                                    .border_color(cx.theme().border)
                                                                                    .child(
                                                                                        div()
                                                                                            .text_xs()
                                                                                            .text_color(
                                                                                                cx.theme().muted_foreground,
                                                                                            )
                                                                                            .child(format!(
                                                                                                "Turn diff available ({diff_line_count} lines)",
                                                                                            )),
                                                                                    )
                                                                                    .child({
                                                                                        let view = view.clone();
                                                                                        Button::new(
                                                                                            format!(
                                                                                                "ai-open-review-tab-{}",
                                                                                                turn.id
                                                                                            ),
                                                                                        )
                                                                                        .compact()
                                                                                        .outline()
                                                                                        .with_size(gpui_component::Size::Small)
                                                                                        .label("View Diff")
                                                                                        .on_click(move |_, _, cx| {
                                                                                            view.update(cx, |this, cx| {
                                                                                                this.ai_open_review_tab(cx);
                                                                                            });
                                                                                        })
                                                                                    }),
                                                                            )
                                                                        })
                                                                        .into_any_element(),
                                                                )
                                                            }))
                                                        }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .top_0()
                                                .right_0()
                                                .bottom_0()
                                                .w(px(16.0))
                                                .child(
                                                    Scrollbar::vertical(&self.ai_timeline_scroll_handle)
                                                        .scrollbar_show(ScrollbarShow::Always),
                                                ),
                                        ),
                                )
                                .child(
                                    v_flex()
                                        .w_full()
                                        .min_h(px(210.0))
                                        .p_3()
                                        .gap_2()
                                        .border_t_1()
                                        .border_color(cx.theme().border)
                                        .bg(cx.theme().muted.opacity(if is_dark { 0.2 } else { 0.45 }))
                                        .child(
                                            h_flex()
                                                .w_full()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_semibold()
                                                        .child("Composer"),
                                                )
                                                .when_some(in_progress_turn.clone(), |this, turn_id| {
                                                    this.child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(cx.theme().warning)
                                                            .child(format!("In progress: {turn_id}")),
                                                    )
                                                }),
                                        )
                                        .child(Input::new(&self.ai_composer_input_state).w_full().h(px(88.0)))
                                        .child(
                                            h_flex()
                                                .w_full()
                                                .items_center()
                                                .gap_1()
                                                .flex_wrap()
                                                .child({
                                                    let view = view.clone();
                                                    Button::new("ai-send-prompt")
                                                        .compact()
                                                        .primary()
                                                        .with_size(gpui_component::Size::Small)
                                                        .label("Send")
                                                        .on_click(move |_, window, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.ai_send_prompt_action(window, cx);
                                                            });
                                                        })
                                                })
                                                .child({
                                                    let view = view.clone();
                                                    Button::new("ai-start-review")
                                                        .compact()
                                                        .outline()
                                                        .with_size(gpui_component::Size::Small)
                                                        .label("Start Review")
                                                        .on_click(move |_, window, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.ai_start_review_action(window, cx);
                                                            });
                                                        })
                                                })
                                                .child({
                                                    let view = view.clone();
                                                    Button::new("ai-interrupt-turn")
                                                        .compact()
                                                        .outline()
                                                        .with_size(gpui_component::Size::Small)
                                                        .label("Interrupt")
                                                        .disabled(in_progress_turn.is_none())
                                                        .on_click(move |_, _, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.ai_interrupt_turn_action(cx);
                                                            });
                                                        })
                                                })
                                                .child(render_ai_session_controls_panel_for_view(
                                                    self,
                                                    view.clone(),
                                                    cx,
                                                ))
                                        )
                                        .child(Input::new(&self.ai_review_input_state).w_full().h(px(30.0)))
                                ),
                        ),
                    ),
                    ),
            )
            .into_any_element()
    }
}
