use std::time::Duration;

const AI_COMPOSER_SURFACE_MAX_WIDTH: f32 = 740.0;

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
        let threads = self.ai_visible_threads();
        let show_global_loading_overlay = self.ai_bootstrap_loading;
        let threads_loading = show_global_loading_overlay && threads.is_empty();
        let active_bookmark = self
            .checked_out_bookmark_name()
            .map_or_else(|| "detached".to_string(), ToOwned::to_owned);
        let pending_approvals = self.ai_visible_pending_approvals();
        let pending_approvals_for_timeline = pending_approvals.clone();
        let pending_approval_count = pending_approvals.len();
        let pending_user_inputs = self.ai_visible_pending_user_inputs();
        let pending_user_inputs_for_timeline = pending_user_inputs.clone();
        let pending_user_input_count = pending_user_inputs.len();
        let selected_thread_id = self.current_ai_thread_id();
        let previous_timeline_row_count = self.ai_timeline_list_row_count;
        let (
            timeline_total_turn_count,
            timeline_visible_turn_count,
            timeline_hidden_turn_count,
            timeline_visible_row_ids,
        ) =
            if let Some(thread_id) = selected_thread_id.as_deref() {
                let (total_turn_count, visible_turn_count, hidden_turn_count, visible_row_ids) =
                    self.ai_timeline_visible_rows_for_thread(thread_id);
                let visible_row_ids = visible_row_ids
                    .into_iter()
                    .filter(|row_id| {
                        self.ai_timeline_row(row_id.as_str()).is_some_and(|row| {
                            ai_timeline_row_is_renderable(self, row)
                        })
                    })
                    .collect::<Vec<_>>();
                self.sync_ai_timeline_list_state(visible_row_ids.len());
                (
                    total_turn_count,
                    visible_turn_count,
                    hidden_turn_count,
                    visible_row_ids,
                )
            } else {
                self.sync_ai_timeline_list_state(0);
                (0, 0, 0, Vec::new())
            };
        self.sync_ai_timeline_follow_output(
            timeline_visible_row_ids.len(),
            timeline_visible_row_ids.len() == previous_timeline_row_count,
        );
        let ai_timeline_follow_output = self.ai_timeline_follow_output;
        let timeline_loading =
            show_global_loading_overlay && selected_thread_id.is_some() && timeline_visible_row_ids.is_empty();
        let ai_timeline_list_state = self.ai_timeline_list_state.clone();
        let (connection_label, connection_color) = ai_connection_label(self.ai_connection_state, cx);
        let composer_attachment_paths = self.ai_composer_local_images.clone();
        let composer_attachment_count = composer_attachment_paths.len();
        let model_supports_image_inputs = self.current_ai_model_supports_image_inputs();
        let review_action_enabled = selected_thread_id.is_some();
        let composer_drop_border_color = if model_supports_image_inputs {
            cx.theme().accent.opacity(if is_dark { 0.78 } else { 0.62 })
        } else {
            cx.theme().warning.opacity(if is_dark { 0.88 } else { 0.74 })
        };
        let composer_drop_bg = if model_supports_image_inputs {
            cx.theme().accent.opacity(if is_dark { 0.14 } else { 0.10 })
        } else {
            cx.theme().warning.opacity(if is_dark { 0.14 } else { 0.08 })
        };
        let composer_panel = h_flex()
            .w_full()
            .justify_center()
            .px_4()
            .pt_6()
            .pb_4()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(AI_COMPOSER_SURFACE_MAX_WIDTH))
                    .gap_2()
                    .when_some(ai_render_composer_feedback_strip(self, is_dark, cx), |this, strip| {
                        this.child(strip)
                    })
                    .child(
                        v_flex()
                            .w_full()
                            .gap_3()
                            .rounded(px(28.0))
                            .border_1()
                            .border_color(cx.theme().border.opacity(if is_dark { 0.72 } else { 0.58 }))
                            .bg(cx.theme().background.blend(
                                cx.theme()
                                    .muted
                                    .opacity(if is_dark { 0.06 } else { 0.10 }),
                            ))
                            .px_4()
                            .pt_3()
                            .pb_2()
                            .drag_over::<gpui::ExternalPaths>(move |style, _, _, _| {
                                style
                                    .border_color(composer_drop_border_color)
                                    .bg(composer_drop_bg)
                            })
                            .on_drop(cx.listener(
                                move |this, paths: &gpui::ExternalPaths, window, cx| {
                                    this.ai_add_dropped_composer_paths_action(
                                        paths.paths().to_vec(),
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                },
                            ))
                            .when(!composer_attachment_paths.is_empty(), |this| {
                                this.child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .gap_1()
                                        .flex_wrap()
                                        .children(composer_attachment_paths.iter().enumerate().map(
                                            |(index, path)| {
                                                let remove_view = view.clone();
                                                let remove_path = path.clone();
                                                let path_display = path.display().to_string();
                                                let attachment_name =
                                                    ai_composer_attachment_display_name(path.as_path());
                                                h_flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .rounded(px(999.0))
                                                    .border_1()
                                                    .border_color(cx.theme().border.opacity(if is_dark {
                                                        0.70
                                                    } else {
                                                        0.60
                                                    }))
                                                    .bg(cx.theme().background.blend(cx.theme().muted.opacity(
                                                        if is_dark { 0.14 } else { 0.18 },
                                                    )))
                                                    .px_2()
                                                    .py_1p5()
                                                    .child(Icon::new(IconName::File).size(px(12.0)))
                                                    .child(
                                                        div()
                                                            .max_w(px(180.0))
                                                            .text_xs()
                                                            .truncate()
                                                            .child(attachment_name),
                                                    )
                                                    .child(
                                                        Button::new((
                                                            "ai-remove-composer-attachment",
                                                            index,
                                                        ))
                                                        .compact()
                                                        .ghost()
                                                        .rounded(px(999.0))
                                                        .with_size(gpui_component::Size::Small)
                                                        .icon(Icon::new(IconName::Close).size(px(12.0)))
                                                        .tooltip(format!("Remove {path_display}"))
                                                        .on_click(move |_, _, cx| {
                                                            remove_view.update(cx, |this, cx| {
                                                                this.ai_remove_composer_attachment_action(
                                                                    remove_path.clone(),
                                                                    cx,
                                                                );
                                                            });
                                                        })
                                                    )
                                                    .into_any_element()
                                            },
                                        )),
                                )
                            })
                            .when(
                                composer_attachment_count > 0 && !model_supports_image_inputs,
                                |this| {
                                    this.child(
                                        div()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(cx.theme().warning)
                                            .bg(cx.theme().warning.opacity(if is_dark {
                                                0.14
                                            } else {
                                                0.08
                                            }))
                                            .p_2()
                                            .text_xs()
                                            .text_color(cx.theme().warning)
                                            .whitespace_normal()
                                            .child(
                                                "Selected model does not support image attachments. Remove attachments or switch models.",
                                            ),
                                    )
                                },
                            )
                            .child(
                                Input::new(&self.ai_composer_input_state)
                                    .appearance(false)
                                    .bordered(false)
                                    .focus_bordered(false)
                                    .w_full()
                                    .h(px(100.0)),
                            )
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .flex_wrap()
                                    .child(
                                        h_flex()
                                            .min_w_0()
                                            .items_center()
                                            .gap_1()
                                            .flex_wrap()
                                            .child({
                                                let view = view.clone();
                                                Button::new("ai-open-attachment-picker")
                                                    .compact()
                                                    .ghost()
                                                    .rounded(px(999.0))
                                                    .with_size(gpui_component::Size::Small)
                                                    .label("📎")
                                                    .tooltip(if model_supports_image_inputs {
                                                        "Attach local screenshots/images to the next prompt."
                                                    } else {
                                                        "Selected model does not support image attachments."
                                                    })
                                                    .disabled(!model_supports_image_inputs)
                                                    .on_click(move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.ai_open_attachment_picker_action(cx);
                                                        });
                                                    })
                                            })
                                            .child(render_ai_session_controls_panel_for_view(
                                                self,
                                                view.clone(),
                                                cx,
                                            )),
                                    )
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .justify_end()
                                            .gap_1()
                                            .child({
                                                let view = view.clone();
                                                Button::new("ai-start-review")
                                                    .compact()
                                                    .ghost()
                                                    .rounded(px(999.0))
                                                    .with_size(gpui_component::Size::Small)
                                                    .label("Review")
                                                    .disabled(!review_action_enabled)
                                                    .on_click(move |_, window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.ai_start_review_action(window, cx);
                                                        });
                                                    })
                                            })
                                            .child({
                                                let view = view.clone();
                                                Button::new("ai-send-prompt")
                                                    .compact()
                                                    .primary()
                                                    .rounded(px(999.0))
                                                    .with_size(gpui_component::Size::Small)
                                                    .icon(Icon::new(IconName::ArrowUp).size(px(16.0)))
                                                    .tooltip("Send prompt")
                                                    .on_click(move |_, window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.ai_send_prompt_action(window, cx);
                                                        });
                                                    })
                                            }),
                                    ),
                            ),
                    ),
            );

        let workspace = v_flex()
            .size_full()
            .w_full()
            .min_h_0()
            .key_context("AiWorkspace")
            .on_action(cx.listener(Self::ai_interrupt_selected_turn_action))
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
                                            .when_some(
                                                self.ai_thread_inline_toast.clone(),
                                                |this, message| {
                                                    this.child(
                                                        div()
                                                            .w_full()
                                                            .px_2()
                                                            .pt_2()
                                                            .child(
                                                                div()
                                                                    .rounded_md()
                                                                    .border_1()
                                                                    .border_color(cx.theme().success.opacity(if is_dark {
                                                                        0.82
                                                                    } else {
                                                                        0.62
                                                                    }))
                                                                    .bg(cx.theme().success.opacity(if is_dark {
                                                                        0.18
                                                                    } else {
                                                                        0.10
                                                                    }))
                                                                    .px_2()
                                                                    .py_1()
                                                                    .text_xs()
                                                                    .text_color(cx.theme().success)
                                                                    .child(message),
                                                            ),
                                                    )
                                                },
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
                                                                    .when(threads_loading, |this| {
                                                                        this.child(render_ai_thread_list_loading_skeleton(
                                                                            is_dark,
                                                                            cx,
                                                                        ))
                                                                    })
                                                                    .when(threads.is_empty() && !threads_loading, |this| {
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
                                                                        let select_view = view.clone();
                                                                        let archive_view = view.clone();
                                                                        let archive_thread_id = thread.id.clone();
                                                                        let archive_button_id = format!(
                                                                            "ai-thread-archive-{}",
                                                                            archive_thread_id.replace('\u{1f}', "--"),
                                                                        );

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
                                                                                select_view.update(cx, |this, cx| {
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
                                                                                    )
                                                                                    .child(
                                                                                        div()
                                                                                            .on_mouse_down(
                                                                                                MouseButton::Left,
                                                                                                |_, _, cx| {
                                                                                                    cx.stop_propagation();
                                                                                                },
                                                                                            )
                                                                                            .child({
                                                                                                let view = archive_view.clone();
                                                                                                Button::new(archive_button_id)
                                                                                                    .compact()
                                                                                                    .outline()
                                                                                                    .with_size(gpui_component::Size::Small)
                                                                                                    .icon(
                                                                                                        Icon::new(IconName::Inbox)
                                                                                                            .size(px(12.0)),
                                                                                                    )
                                                                                                    .tooltip("Archive thread")
                                                                                                    .on_click(move |_, _, cx| {
                                                                                                        view.update(cx, |this, cx| {
                                                                                                            this.ai_archive_thread_action(
                                                                                                                archive_thread_id.clone(),
                                                                                                                cx,
                                                                                                            );
                                                                                                        });
                                                                                                    })
                                                                                            }),
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
                                                        .child(
                                                            v_flex()
                                                                .size_full()
                                                                .w_full()
                                                                .min_h_0()
                                                                .gap_2()
                                                                .p_3()
                                                                .bg(cx.theme().background)
                                                .child(
                                                    h_flex()
                                                        .w_full()
                                                        .items_center()
                                                        .justify_between()
                                                        .gap_2()
                                                        .child(
                                                            h_flex()
                                                                .flex_1()
                                                                .min_w_0()
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
                                                                        let copy_thread_id =
                                                                            thread_id.clone();
                                                                        let view = view.clone();
                                                                        this.child(
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
                                                        .when(self.ai_mad_max_mode, |this| {
                                                            this.child(
                                                                div()
                                                                    .flex_none()
                                                                    .text_xs()
                                                                    .text_color(cx.theme().danger)
                                                                    .child("Mad Max auto-approvals enabled"),
                                                            )
                                                        }),
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
                                                .when(
                                                    !pending_approvals_for_timeline.is_empty()
                                                        || !pending_user_inputs_for_timeline.is_empty(),
                                                    |this| {
                                                        this.child(
                                                            v_flex()
                                                                .w_full()
                                                                .gap_1()
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
                                                .when(timeline_loading, |this| {
                                                    this.child(
                                                        render_ai_timeline_loading_skeleton(
                                                            is_dark,
                                                            cx,
                                                        ),
                                                    )
                                                })
                                                .when(selected_thread_id.is_none() && !timeline_loading, |this| {
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
                                                .when_some(
                                                    selected_thread_id
                                                        .clone()
                                                        .filter(|_| !timeline_loading),
                                                    |this, thread_id| {
                                                            let timeline_row_ids_for_list = timeline_visible_row_ids.clone();
                                                            let timeline_list_state = ai_timeline_list_state.clone();
                                                            let view_for_list = view.clone();
                                                            let timeline_list = list(timeline_list_state.clone(), {
                                                                cx.processor(move |this, ix: usize, _window, cx| {
                                                                    let Some(row_id) = timeline_row_ids_for_list.get(ix) else {
                                                                        return div().w_full().h(px(0.0)).into_any_element();
                                                                    };
                                                                    render_ai_chat_timeline_row_for_view(
                                                                        this,
                                                                        row_id.as_str(),
                                                                        view_for_list.clone(),
                                                                        is_dark,
                                                                        cx,
                                                                    )
                                                                })
                                                            })
                                                            .size_full()
                                                            .with_sizing_behavior(ListSizingBehavior::Auto);
                                                            this.when(timeline_visible_turn_count == 0, |this| {
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
                                                            .when(
                                                                timeline_visible_turn_count > 0
                                                                    && timeline_visible_row_ids.is_empty(),
                                                                |this| {
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
                                                                                    .child("No timeline events for visible turns yet."),
                                                                            ),
                                                                    )
                                                                },
                                                            )
                                                            .when(timeline_hidden_turn_count > 0, |this| {
                                                                let load_older_thread_id = thread_id.clone();
                                                                let show_all_thread_id = thread_id.clone();
                                                                let view = view.clone();
                                                                this.child(
                                                                    h_flex()
                                                                        .w_full()
                                                                        .items_center()
                                                                        .justify_between()
                                                                        .gap_2()
                                                                        .rounded_md()
                                                                        .border_1()
                                                                        .border_color(
                                                                            cx.theme().border.opacity(if is_dark {
                                                                                0.90
                                                                            } else {
                                                                                0.74
                                                                            }),
                                                                        )
                                                                        .bg(cx.theme().background.blend(
                                                                            cx.theme().muted.opacity(if is_dark {
                                                                                0.16
                                                                            } else {
                                                                                0.24
                                                                            }),
                                                                        ))
                                                                        .p_2()
                                                                        .child(
                                                                            div()
                                                                                .text_xs()
                                                                                .text_color(
                                                                                    cx.theme().muted_foreground,
                                                                                )
                                                                                .child(format!(
                                                                                    "Showing latest {timeline_visible_turn_count} of {timeline_total_turn_count} turns.",
                                                                                )),
                                                                        )
                                                                        .child(
                                                                            h_flex()
                                                                                .items_center()
                                                                                .gap_1()
                                                                                .child({
                                                                                    let view = view.clone();
                                                                                    Button::new("ai-timeline-load-older-turns")
                                                                                        .compact()
                                                                                        .outline()
                                                                                        .with_size(gpui_component::Size::Small)
                                                                                        .label("Load older")
                                                                                        .on_click(move |_, _, cx| {
                                                                                            view.update(cx, |this, cx| {
                                                                                                this.ai_load_older_turns_action(
                                                                                                    load_older_thread_id.clone(),
                                                                                                    cx,
                                                                                                );
                                                                                            });
                                                                                        })
                                                                                })
                                                                                .child({
                                                                                    let view = view.clone();
                                                                                    Button::new("ai-timeline-show-all-turns")
                                                                                        .compact()
                                                                                        .outline()
                                                                                        .with_size(gpui_component::Size::Small)
                                                                                        .label("Show all")
                                                                                        .on_click(move |_, _, cx| {
                                                                                            view.update(cx, |this, cx| {
                                                                                                this.ai_show_full_timeline_action(
                                                                                                    show_all_thread_id.clone(),
                                                                                                    cx,
                                                                                                );
                                                                                            });
                                                                                        })
                                                                                }),
                                                                        ),
                                                                )
                                                            })
                                                            .when(!timeline_visible_row_ids.is_empty(), |this| {
                                                                let view = view.clone();
                                                                this.child(
                                                                    div()
                                                                        .flex_1()
                                                                        .min_h_0()
                                                                        .relative()
                                                                        .child(
                                                                            div()
                                                                                .size_full()
                                                                                .child(timeline_list),
                                                                        )
                                                                        .child(
                                                                            div()
                                                                                .absolute()
                                                                                .top_0()
                                                                                .right_0()
                                                                                .bottom_0()
                                                                                .w(px(16.0))
                                                                                .child(
                                                                                    Scrollbar::vertical(&timeline_list_state)
                                                                                        .scrollbar_show(ScrollbarShow::Always),
                                                                                ),
                                                                        )
                                                                        .when(!ai_timeline_follow_output, |this| {
                                                                            let view = view.clone();
                                                                            this.child(
                                                                                div()
                                                                                    .absolute()
                                                                                    .right(px(16.0))
                                                                                    .bottom(px(8.0))
                                                                                    .left_0()
                                                                                    .flex()
                                                                                    .justify_center()
                                                                                    .child(
                                                                                        Button::new(
                                                                                            "ai-timeline-scroll-to-bottom",
                                                                                        )
                                                                                        .compact()
                                                                                        .primary()
                                                                                        .with_size(gpui_component::Size::Small)
                                                                                        .icon(
                                                                                            Icon::new(IconName::ChevronDown)
                                                                                                .size(px(14.0)),
                                                                                        )
                                                                                        .tooltip("Scroll to the bottom")
                                                                                        .on_click(move |_, _, cx| {
                                                                                            view.update(cx, |this, cx| {
                                                                                                this.ai_scroll_timeline_to_bottom_action(cx);
                                                                                            });
                                                                                        }),
                                                                                    ),
                                                                            )
                                                                        }),
                                                                )
                                                            })
                                                        }),
                                                ),
                                        ),
                                )
                                .child(composer_panel),
                        ),
                    ),
                    ),
            )
            .into_any_element();

        div()
            .size_full()
            .relative()
            .child(workspace)
            .when(show_global_loading_overlay, |this| {
                this.child(render_ai_global_loading_overlay(is_dark, cx))
            })
            .into_any_element()
    }
}

fn ai_composer_attachment_display_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn ai_activity_elapsed_label(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    let remainder = seconds % 60;
    if minutes < 60 {
        if remainder == 0 {
            return format!("{minutes}m");
        }
        return format!("{minutes}m {remainder}s");
    }
    let hours = minutes / 60;
    let minute_remainder = minutes % 60;
    if minute_remainder == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {minute_remainder}m")
    }
}

struct AiComposerActivityDisplay {
    label: &'static str,
    elapsed: Duration,
    animation_key: String,
}

#[derive(Clone, Copy)]
enum AiComposerStatusTone {
    Danger,
    Warning,
}

fn ai_render_composer_feedback_strip(
    this: &DiffViewer,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> Option<AnyElement> {
    if let Some(status) = this.ai_status_message.as_deref()
        && let Some(tone) = ai_composer_status_tone(status)
    {
        return Some(ai_render_composer_status_strip(status, tone, is_dark, cx));
    }

    ai_current_composer_activity(this)
        .map(|activity| ai_render_composer_activity_strip(this, &activity, is_dark, cx))
}

fn ai_composer_status_tone(status: &str) -> Option<AiComposerStatusTone> {
    let lower = status.to_ascii_lowercase();
    if lower.contains("connected over websocket")
        || lower.contains("starting codex app server")
        || lower.starts_with("attached ")
        || lower.starts_with("submitted user input")
        || lower.starts_with("mad max mode ")
    {
        return None;
    }

    if lower.contains("interrupt")
        || lower.contains("failed")
        || lower.contains("disconnected")
        || lower.contains("error")
    {
        return Some(AiComposerStatusTone::Danger);
    }

    if lower.contains("cannot")
        || lower.contains("remove attachments")
        || lower.contains("select a thread")
        || lower.contains("open a workspace")
        || lower.contains("no in-progress")
        || lower.contains("no supported")
        || lower.contains("unsupported")
        || lower.contains("skipped")
        || lower.contains("already attached")
        || lower.contains("no files were supported")
        || lower.contains("user input request no longer exists")
    {
        return Some(AiComposerStatusTone::Warning);
    }

    None
}

fn ai_render_composer_status_strip(
    status: &str,
    tone: AiComposerStatusTone,
    _is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let text_color = match tone {
        AiComposerStatusTone::Danger => cx.theme().danger,
        AiComposerStatusTone::Warning => cx.theme().warning,
    };

    div()
        .w_full()
        .px_1()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(text_color)
                .child(status.to_string()),
        )
        .into_any_element()
}

fn ai_current_composer_activity(this: &DiffViewer) -> Option<AiComposerActivityDisplay> {
    let thread_id = this.current_ai_thread_id()?;
    let turn_id = this.current_ai_in_progress_turn_id(thread_id.as_str())?;
    let tracking_key = format!("{thread_id}::{turn_id}");
    let started_at = this.ai_in_progress_turn_started_at.get(tracking_key.as_str())?;
    let label = this
        .ai_state_snapshot
        .items
        .values()
        .filter(|item| {
            item.thread_id == thread_id
                && item.turn_id == turn_id
                && item.status != ItemStatus::Completed
        })
        .max_by_key(|item| item.last_sequence)
        .map(|item| ai_composer_activity_label_for_kind(item.kind.as_str()))
        .unwrap_or("Working");

    Some(AiComposerActivityDisplay {
        label,
        elapsed: started_at.elapsed(),
        animation_key: tracking_key,
    })
}

fn ai_composer_activity_label_for_kind(kind: &str) -> &'static str {
    match kind {
        "reasoning" => "Thinking",
        "commandExecution" => "Running",
        "fileChange" => "Editing",
        "dynamicToolCall" | "mcpToolCall" | "collabAgentToolCall" => "Tool",
        "webSearch" => "Searching",
        "agentMessage" | "plan" => "Writing",
        _ => "Working",
    }
}

fn ai_render_composer_activity_strip(
    this: &DiffViewer,
    activity: &AiComposerActivityDisplay,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let shimmer_duration = this.animation_duration_ms(1400);
    let shimmer_color = if is_dark {
        cx.theme().foreground.opacity(0.96)
    } else {
        cx.theme().foreground.opacity(0.78)
    };

    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .px_1()
        .child(
            div()
                .relative()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(activity.label),
                )
                .when(!this.reduced_motion_enabled(), |this| {
                    this.child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .opacity(0.0)
                            .text_xs()
                            .font_semibold()
                            .text_color(shimmer_color)
                            .child(activity.label)
                            .with_animation(
                                format!("ai-composer-text-sheen-{}", activity.animation_key),
                                Animation::new(shimmer_duration)
                                    .repeat()
                                    .with_easing(cubic_bezier(0.42, 0.0, 0.58, 1.0)),
                                |this, delta| {
                                    let opacity = if delta < 0.14 {
                                        delta / 0.14
                                    } else if delta > 0.58 {
                                        ((1.0 - delta) / 0.42).max(0.0)
                                    } else {
                                        1.0
                                    };
                                    this.opacity(0.50 * opacity)
                                },
                            ),
                    )
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground.opacity(if is_dark {
                    0.84
                } else {
                    0.76
                }))
                .child("·"),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground.opacity(if is_dark {
                    0.84
                } else {
                    0.76
                }))
                .child(ai_activity_elapsed_label(activity.elapsed)),
        )
        .into_any_element()
}
