const AI_COMPOSER_FILE_COMPLETION_MENU_GAP_Y: f32 = 12.0;

struct AiComposerPanelState {
    composer_attachment_paths: Vec<PathBuf>,
    composer_attachment_count: usize,
    model_supports_image_inputs: bool,
    review_mode_active: bool,
    current_mode_label: String,
    selected_thread_mode_for_picker: AiNewThreadStartMode,
    thread_mode_picker_editable: bool,
    session_controls_read_only: bool,
    composer_send_waiting_on_connection: bool,
    composer_interrupt_available: bool,
    queued_message_count: usize,
    review_action_blocker: Option<String>,
    composer_drop_border_color: Hsla,
    composer_drop_bg: Hsla,
}

impl DiffViewer {
    fn render_ai_composer_panel(
        &self,
        view: Entity<Self>,
        state: &AiComposerPanelState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let composer_drop_border_color = state.composer_drop_border_color;
        let composer_drop_bg = state.composer_drop_bg;
        let footer_group_gap = px(6.0);
        let footer_button_gap = px(2.0);
        h_flex()
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
                            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.72, 0.58))
                            .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.06, 0.10))
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
                            .when(!state.composer_attachment_paths.is_empty(), |this| {
                                this.child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .gap_1()
                                        .flex_wrap()
                                        .children(state.composer_attachment_paths.iter().enumerate().map(
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
                                                    .border_color(hunk_opacity(cx.theme().border, is_dark, 0.70, 0.60))
                                                    .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.14, 0.18))
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
                                state.composer_attachment_count > 0
                                    && !state.model_supports_image_inputs,
                                |this| {
                                    this.child(
                                        div()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(cx.theme().warning)
                                            .bg(hunk_opacity(cx.theme().warning, is_dark, 0.14, 0.08))
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
                                div()
                                    .relative()
                                    .key_context("AiComposer")
                                    .on_action(cx.listener(Self::ai_queue_prompt_action))
                                    .on_action(cx.listener(Self::ai_edit_last_queued_prompt_action))
                                    .child(
                                        Input::new(&self.ai_composer_input_state)
                                            .appearance(false)
                                            .bordered(false)
                                            .focus_bordered(false)
                                            .w_full()
                                            .h(px(100.0)),
                                    )
                                    .when_some(
                                        self.ai_composer_file_completion_menu.clone(),
                                        |this, menu| {
                                            this.child(
                                                self.render_ai_composer_file_completion_menu(
                                                    view.clone(),
                                                    menu,
                                                    is_dark,
                                                    cx,
                                                ),
                                            )
                                        },
                                    )
                                    .when_some(
                                        self.ai_composer_slash_command_menu.clone(),
                                        |this, menu| {
                                            this.child(
                                                self.render_ai_composer_slash_command_menu(
                                                    view.clone(),
                                                    menu,
                                                    is_dark,
                                                    cx,
                                                ),
                                            )
                                        },
                                    )
                                    .when_some(
                                        self.ai_composer_skill_completion_menu.clone(),
                                        |this, menu| {
                                            this.child(
                                                self.render_ai_composer_skill_completion_menu(
                                                    view.clone(),
                                                    menu,
                                                    is_dark,
                                                    cx,
                                                ),
                                            )
                                        },
                                    ),
                            )
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .justify_between()
                                    .gap(footer_group_gap)
                                    .flex_wrap()
                                    .child(
                                        h_flex()
                                            .min_w_0()
                                            .items_center()
                                            .gap(footer_button_gap)
                                            .flex_wrap()
                                            .child({
                                                let view = view.clone();
                                                let model_supports_image_inputs =
                                                    state.model_supports_image_inputs;
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
                                                state.selected_thread_mode_for_picker,
                                                state.thread_mode_picker_editable,
                                                state.session_controls_read_only,
                                                cx,
                                            ))
                                            .child(
                                                div()
                                                    .rounded(px(999.0))
                                                    .border_1()
                                                    .border_color(hunk_opacity(
                                                        cx.theme().accent,
                                                        is_dark,
                                                        0.54,
                                                        0.44,
                                                    ))
                                                    .bg(hunk_opacity(
                                                        cx.theme().accent,
                                                        is_dark,
                                                        0.14,
                                                        0.10,
                                                    ))
                                                    .px_2()
                                                    .py_0p5()
                                                    .text_xs()
                                                    .font_semibold()
                                                    .text_color(cx.theme().accent)
                                                    .child(state.current_mode_label.clone()),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .justify_end()
                                            .gap(footer_button_gap)
                                            .child({
                                                let view = view.clone();
                                                if state.composer_interrupt_available {
                                                    Button::new("ai-interrupt-turn")
                                                        .compact()
                                                        .primary()
                                                        .rounded(px(999.0))
                                                        .with_size(gpui_component::Size::Small)
                                                        .icon(Icon::new(IconName::Close).size(px(16.0)))
                                                        .tooltip("Interrupt run")
                                                        .on_click(move |_, _, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.ai_interrupt_turn_action(cx);
                                                            });
                                                        })
                                                } else {
                                                    let composer_send_waiting_on_connection =
                                                        state.composer_send_waiting_on_connection;
                                                    let review_mode_active =
                                                        state.review_mode_active;
                                                    let review_action_tooltip = state
                                                        .review_action_blocker
                                                        .clone()
                                                        .unwrap_or_else(|| {
                                                            "Review the current working-copy changes for correctness and regressions.".to_string()
                                                        });
                                                    Button::new("ai-send-prompt")
                                                        .compact()
                                                        .primary()
                                                        .rounded(px(999.0))
                                                        .with_size(gpui_component::Size::Small)
                                                        .icon(Icon::new(IconName::ArrowUp).size(px(16.0)))
                                                        .tooltip(
                                                            if composer_send_waiting_on_connection {
                                                                "Wait for Codex to finish connecting."
                                                                    .to_string()
                                                            } else if review_mode_active {
                                                                review_action_tooltip
                                                            } else {
                                                                "Send prompt".to_string()
                                                            },
                                                        )
                                                        .disabled(composer_send_waiting_on_connection)
                                                        .on_click(move |_, window, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.ai_send_prompt_action(window, cx);
                                                            });
                                                        })
                                                }
                                            }),
                                    ),
                            )
                            .when(
                                state.composer_interrupt_available
                                    || state.queued_message_count > 0,
                                |this| {
                                    let queue_hint = if state.queued_message_count > 0 {
                                        let noun =
                                            if state.queued_message_count == 1 { "message" } else { "messages" };
                                        format!(
                                            "{} queued {}. Tab queues another follow-up. Ctrl+Shift+Up edits the newest queued message.",
                                            state.queued_message_count, noun
                                        )
                                    } else {
                                        "Tab queues a follow-up for after this turn finishes."
                                            .to_string()
                                    };
                                    this.child(
                                        div()
                                            .w_full()
                                            .min_w_0()
                                            .px_1()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(queue_hint),
                                    )
                                },
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_ai_composer_file_completion_menu(
        &self,
        view: Entity<Self>,
        menu: AiComposerFileCompletionMenuState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_range = menu.replace_range.start..menu.replace_range.start.saturating_add(1);
        let Some(anchor_position) = self
            .ai_composer_input_state
            .read(cx)
            .offset_range_bounds(&anchor_range)
            .map(|bounds| point(bounds.left(), bounds.top()))
        else {
            return div().into_any_element();
        };

        let selected_ix = self
            .ai_composer_file_completion_selected_ix
            .min(menu.items.len().saturating_sub(1));

        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(anchor_position)
                .offset(point(
                    px(0.),
                    -px(AI_COMPOSER_FILE_COMPLETION_MENU_GAP_Y),
                ))
                .anchor(Corner::BottomLeft)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    div()
                        .id("ai-composer-file-completion-menu")
                        .min_w(px(280.0))
                        .max_w(px(420.0))
                        .max_h(px(260.0))
                        .overflow_y_scrollbar()
                        .rounded(px(18.0))
                        .border_1()
                        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.78, 0.62))
                        .bg(cx.theme().popover)
                        .shadow_lg()
                        .p_1()
                        .children(menu.items.iter().enumerate().map(|(ix, path)| {
                            let select_view = view.clone();
                            let select_path = path.clone();
                            let file_name = path.rsplit('/').next().unwrap_or(path.as_str()).to_string();
                            let dir_prefix = path
                                .strip_suffix(file_name.as_str())
                                .unwrap_or_default()
                                .trim_end_matches('/')
                                .to_string();
                            let selected = ix == selected_ix;

                            h_flex()
                                .id(("ai-composer-file-completion-item", ix))
                                .w_full()
                                .min_w_0()
                                .items_center()
                                .gap_2()
                                .rounded(px(12.0))
                                .px_2()
                                .py_1p5()
                                .text_sm()
                                .hover(|style| {
                                    style.bg(hunk_opacity(
                                        cx.theme().accent,
                                        is_dark,
                                        0.22,
                                        0.14,
                                    ))
                                })
                                .when(selected, |this| {
                                    this.bg(hunk_opacity(
                                        cx.theme().accent,
                                        is_dark,
                                        0.28,
                                        0.18,
                                    ))
                                    .border_1()
                                    .border_color(hunk_opacity(
                                        cx.theme().accent,
                                        is_dark,
                                        0.68,
                                        0.58,
                                    ))
                                })
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    select_view.update(cx, |this, cx| {
                                        this.ai_accept_composer_file_completion_path(
                                            select_path.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                    cx.stop_propagation();
                                })
                                .child(
                                    Icon::new(IconName::File)
                                        .size(px(12.0))
                                        .text_color(if selected {
                                            cx.theme().accent
                                        } else {
                                            hunk_opacity(
                                                cx.theme().muted_foreground,
                                                is_dark,
                                                0.86,
                                                0.96,
                                            )
                                        }),
                                )
                                .child(
                                    v_flex()
                                        .min_w_0()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_color(if selected {
                                                    cx.theme().foreground
                                                } else {
                                                    hunk_opacity(
                                                        cx.theme().foreground,
                                                        is_dark,
                                                        0.96,
                                                        0.98,
                                                    )
                                                })
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .child(file_name),
                                        )
                                        .when(!dir_prefix.is_empty(), |this| {
                                            this.child(
                                                div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_xs()
                                                    .font_family(cx.theme().mono_font_family.clone())
                                                    .text_color(if selected {
                                                        hunk_opacity(
                                                            cx.theme().accent,
                                                            is_dark,
                                                            0.94,
                                                            0.90,
                                                        )
                                                    } else {
                                                        hunk_opacity(
                                                            cx.theme().muted_foreground,
                                                            is_dark,
                                                            0.82,
                                                            0.94,
                                                        )
                                                    })
                                                    .child(dir_prefix),
                                            )
                                        }),
                                )
                        })),
                ),
            )
            .into_any_element()
    }

    fn render_ai_composer_skill_completion_menu(
        &self,
        view: Entity<Self>,
        menu: AiComposerSkillCompletionMenuState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_range = menu.replace_range.start..menu.replace_range.start.saturating_add(1);
        let Some(anchor_position) = self
            .ai_composer_input_state
            .read(cx)
            .offset_range_bounds(&anchor_range)
            .map(|bounds| point(bounds.left(), bounds.top()))
        else {
            return div().into_any_element();
        };

        let selected_ix = self
            .ai_composer_skill_completion_selected_ix
            .min(menu.items.len().saturating_sub(1));

        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(anchor_position)
                .offset(point(
                    px(0.),
                    -px(AI_COMPOSER_FILE_COMPLETION_MENU_GAP_Y),
                ))
                .anchor(Corner::BottomLeft)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    div()
                        .id("ai-composer-skill-completion-menu")
                        .min_w(px(320.0))
                        .max_w(px(460.0))
                        .max_h(px(280.0))
                        .overflow_y_scrollbar()
                        .rounded(px(18.0))
                        .border_1()
                        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.78, 0.62))
                        .bg(cx.theme().popover)
                        .shadow_lg()
                        .p_1()
                        .children(menu.items.iter().enumerate().map(|(ix, item)| {
                            let select_view = view.clone();
                            let select_name = item.name.clone();
                            let selected = ix == selected_ix;
                            let title = item.display_name.as_deref().unwrap_or(item.name.as_str());
                            let show_name = item.display_name.as_deref() != Some(item.name.as_str());

                            h_flex()
                                .id(("ai-composer-skill-completion-item", ix))
                                .w_full()
                                .min_w_0()
                                .items_center()
                                .gap_2()
                                .rounded(px(12.0))
                                .px_2()
                                .py_1p5()
                                .text_sm()
                                .hover(|style| {
                                    style.bg(hunk_opacity(
                                        cx.theme().accent,
                                        is_dark,
                                        0.22,
                                        0.14,
                                    ))
                                })
                                .when(selected, |this| {
                                    this.bg(hunk_opacity(
                                        cx.theme().accent,
                                        is_dark,
                                        0.28,
                                        0.18,
                                    ))
                                    .border_1()
                                    .border_color(hunk_opacity(
                                        cx.theme().accent,
                                        is_dark,
                                        0.68,
                                        0.58,
                                    ))
                                })
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    select_view.update(cx, |this, cx| {
                                        this.ai_accept_composer_skill_completion_name(
                                            select_name.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                    cx.stop_propagation();
                                })
                                .child(
                                    Icon::new(IconName::Settings)
                                        .size(px(12.0))
                                        .mt_0p5()
                                        .text_color(if selected {
                                            cx.theme().accent
                                        } else {
                                            hunk_opacity(
                                                cx.theme().muted_foreground,
                                                is_dark,
                                                0.86,
                                                0.96,
                                            )
                                        }),
                                )
                                .child(
                                    v_flex()
                                        .min_w_0()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_color(if selected {
                                                    cx.theme().foreground
                                                } else {
                                                    hunk_opacity(
                                                        cx.theme().foreground,
                                                        is_dark,
                                                        0.96,
                                                        0.98,
                                                    )
                                                })
                                                .child(title.to_string()),
                                        )
                                        .when(show_name, |this| {
                                            this.child(
                                                div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_xs()
                                                    .font_family(cx.theme().mono_font_family.clone())
                                                    .text_color(hunk_opacity(
                                                        cx.theme().muted_foreground,
                                                        is_dark,
                                                        0.90,
                                                        0.94,
                                                    ))
                                                    .child(format!("${}", item.name)),
                                            )
                                        })
                                        .when_some(item.description.clone(), |this, description| {
                                            this.child(
                                                div()
                                                    .min_w_0()
                                                    .text_xs()
                                                    .whitespace_normal()
                                                    .text_color(hunk_opacity(
                                                        cx.theme().muted_foreground,
                                                        is_dark,
                                                        0.92,
                                                        0.96,
                                                    ))
                                                    .child(description),
                                            )
                                        }),
                                )
                                .into_any_element()
                        })),
                ),
            )
            .into_any_element()
    }

    fn render_ai_composer_slash_command_menu(
        &self,
        view: Entity<Self>,
        menu: crate::app::AiComposerSlashCommandMenuState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_range = menu.replace_range.start..menu.replace_range.start.saturating_add(1);
        let Some(anchor_position) = self
            .ai_composer_input_state
            .read(cx)
            .offset_range_bounds(&anchor_range)
            .map(|bounds| point(bounds.left(), bounds.top()))
        else {
            return div().into_any_element();
        };

        let selected_ix = self
            .ai_composer_slash_command_selected_ix
            .min(menu.items.len().saturating_sub(1));

        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(anchor_position)
                .offset(point(
                    px(0.),
                    -px(AI_COMPOSER_FILE_COMPLETION_MENU_GAP_Y),
                ))
                .anchor(Corner::BottomLeft)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    div()
                        .id("ai-composer-slash-command-menu")
                        .min_w(px(320.0))
                        .max_w(px(460.0))
                        .max_h(px(280.0))
                        .overflow_y_scrollbar()
                        .rounded(px(18.0))
                        .border_1()
                        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.78, 0.62))
                        .bg(cx.theme().popover)
                        .shadow_lg()
                        .p_1()
                        .children(menu.items.iter().enumerate().map(|(ix, item)| {
                            let select_view = view.clone();
                            let command_name = item.name.to_string();
                            let selected = ix == selected_ix;

                            h_flex()
                                .id(("ai-composer-slash-command-item", ix))
                                .w_full()
                                .min_w_0()
                                .items_center()
                                .gap_2()
                                .rounded(px(12.0))
                                .px_2()
                                .py_1p5()
                                .text_sm()
                                .hover(|style| {
                                    style.bg(hunk_opacity(
                                        cx.theme().accent,
                                        is_dark,
                                        0.22,
                                        0.14,
                                    ))
                                })
                                .when(selected, |this| {
                                    this.bg(hunk_opacity(
                                        cx.theme().accent,
                                        is_dark,
                                        0.28,
                                        0.18,
                                    ))
                                    .border_1()
                                    .border_color(hunk_opacity(
                                        cx.theme().accent,
                                        is_dark,
                                        0.68,
                                        0.58,
                                    ))
                                })
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    select_view.update(cx, |this, cx| {
                                        this.ai_accept_composer_slash_command_name(
                                            command_name.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                    cx.stop_propagation();
                                })
                                .child(
                                    v_flex()
                                        .min_w_0()
                                        .gap_0p5()
                                        .child(
                                            h_flex()
                                                .min_w_0()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .rounded(px(999.0))
                                                        .bg(hunk_opacity(
                                                            cx.theme().accent,
                                                            is_dark,
                                                            0.16,
                                                            0.10,
                                                        ))
                                                        .px_1p5()
                                                        .py_0p5()
                                                        .text_xs()
                                                        .font_family(
                                                            cx.theme()
                                                                .mono_font_family
                                                                .clone(),
                                                        )
                                                        .text_color(cx.theme().accent)
                                                        .child(format!("/{}", item.name)),
                                                )
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .truncate()
                                                        .text_color(if selected {
                                                            cx.theme().foreground
                                                        } else {
                                                            hunk_opacity(
                                                                cx.theme().foreground,
                                                                is_dark,
                                                                0.96,
                                                                0.98,
                                                            )
                                                        })
                                                        .child(item.label),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_xs()
                                                .text_color(if selected {
                                                    hunk_opacity(
                                                        cx.theme().accent,
                                                        is_dark,
                                                        0.94,
                                                        0.90,
                                                    )
                                                } else {
                                                    hunk_opacity(
                                                        cx.theme().muted_foreground,
                                                        is_dark,
                                                        0.82,
                                                        0.94,
                                                    )
                                                })
                                                .child(item.description),
                                        ),
                                )
                        })),
                ),
            )
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
    if let Some(status) = this.current_ai_composer_status_message()
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
            item.thread_id == thread_id && item.turn_id == turn_id && item.status != ItemStatus::Completed
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
    let shimmer_color = hunk_opacity(cx.theme().foreground, is_dark, 0.96, 0.78);

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
                .text_color(hunk_opacity(cx.theme().muted_foreground, is_dark, 0.84, 0.76))
                .child("·"),
        )
        .child(
            div()
                .text_xs()
                .text_color(hunk_opacity(cx.theme().muted_foreground, is_dark, 0.84, 0.76))
                .child(ai_activity_elapsed_label(activity.elapsed)),
        )
        .into_any_element()
}

fn ai_render_thread_start_mode_chip(
    start_mode: AiNewThreadStartMode,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let (border_color, background_color, text_color) = match start_mode {
        AiNewThreadStartMode::Local => (
            hunk_opacity(cx.theme().success, is_dark, 0.78, 0.64),
            hunk_opacity(cx.theme().success, is_dark, 0.18, 0.12),
            cx.theme().success,
        ),
        AiNewThreadStartMode::Worktree => (
            hunk_opacity(cx.theme().warning, is_dark, 0.82, 0.68),
            hunk_opacity(cx.theme().warning, is_dark, 0.18, 0.12),
            cx.theme().warning,
        ),
    };

    div()
        .flex_none()
        .rounded(px(999.0))
        .border_1()
        .border_color(border_color)
        .bg(background_color)
        .px_2()
        .py_0p5()
        .text_xs()
        .font_semibold()
        .text_color(text_color)
        .child(start_mode.label())
        .into_any_element()
}
