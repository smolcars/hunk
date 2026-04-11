const AI_COMPOSER_FILE_COMPLETION_MENU_GAP_Y: f32 = 12.0;

struct AiComposerPanelState {
    composer_feedback: Option<AiComposerFeedbackState>,
    composer_attachment_paths: Arc<[PathBuf]>,
    composer_attachment_count: usize,
    model_supports_image_inputs: bool,
    review_mode_active: bool,
    usage_popover_open: bool,
    show_mode_badge: bool,
    current_mode_label: String,
    fast_mode_enabled: bool,
    selected_thread_mode_for_picker: AiNewThreadStartMode,
    show_thread_mode_picker: bool,
    thread_mode_picker_editable: bool,
    session_controls_read_only: bool,
    selected_thread_context_usage: Option<hunk_codex::state::ThreadTokenUsageSummary>,
    composer_send_waiting_on_connection: bool,
    composer_interrupt_available: bool,
    queued_message_count: usize,
    review_action_blocker: Option<String>,
    followup_prompt: Option<AiFollowupPrompt>,
    followup_prompt_action: AiFollowupPromptAction,
    composer_drop_border_color: Hsla,
    composer_drop_bg: Hsla,
}

struct AiComposerCompletionMenuShell<'a> {
    menu_id: &'static str,
    scroll_area_id: &'static str,
    anchor_position: Point<Pixels>,
    min_width: Pixels,
    max_width: Pixels,
    max_height: Pixels,
    scroll_handle: &'a ScrollHandle,
}

fn ai_composer_mode_badge_label(mode: &str) -> String {
    mode.to_string()
}

fn ai_composer_mode_badge_icon(mode: &str) -> Option<HunkIconName> {
    match mode {
        "Code" => Some(HunkIconName::Computer),
        "Plan" => Some(HunkIconName::NotebookPen),
        "Review" => Some(HunkIconName::UserStar),
        _ => None,
    }
}

fn ai_render_composer_status_chip(
    label: &str,
    icon: Option<HunkIconName>,
    border_color: Hsla,
    background_color: Hsla,
    text_color: Hsla,
) -> AnyElement {
    let content = if let Some(icon) = icon {
        h_flex()
            .items_center()
            .gap_1()
            .text_xs()
            .font_semibold()
            .text_color(text_color)
            .child(Icon::new(icon).size(px(12.0)))
            .child(label.to_string())
            .into_any_element()
    } else {
        h_flex()
            .items_center()
            .gap_1()
            .text_xs()
            .font_semibold()
            .text_color(text_color)
            .child(label.to_string())
            .into_any_element()
    };

    div()
        .rounded(px(999.0))
        .border_1()
        .border_color(border_color)
        .bg(background_color)
        .px_2()
        .py_0p5()
        .child(content)
        .into_any_element()
}

impl DiffViewer {
    fn render_ai_composer_panel(
        &self,
        view: Entity<Self>,
        state: &AiComposerPanelState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let feedback_strip =
            ai_render_composer_feedback_strip(state.composer_feedback.as_ref(), self, is_dark, cx);
        let usage_popover = state.usage_popover_open.then(|| {
            h_flex()
                .w_full()
                .justify_center()
                .child(self.render_ai_usage_popover_card(view.clone(), is_dark, cx))
                .into_any_element()
        });
        let composer_drop_border_color = state.composer_drop_border_color;
        let composer_drop_bg = state.composer_drop_bg;
        let footer_group_gap = px(6.0);
        let footer_button_gap = px(2.0);
        let footer_action_gap = px(8.0);
        let completion_colors = hunk_completion_menu(cx.theme(), is_dark);
        let context_usage_chip = state
            .selected_thread_context_usage
            .as_ref()
            .and_then(|usage| ai_render_context_usage_chip(usage, is_dark, cx));
        let attachment_chips = (!state.composer_attachment_paths.is_empty()).then(|| {
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
                                Button::new(("ai-remove-composer-attachment", index))
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
                                    }),
                            )
                            .into_any_element()
                    },
                ))
                .into_any_element()
        });
        let input_shell = div()
            .relative()
            .key_context("AiComposer AiWorkspace")
            .on_action(cx.listener(Self::ai_queue_prompt_action))
            .on_action(cx.listener(Self::ai_edit_last_queued_prompt_action))
            .capture_action(cx.listener(Self::ai_composer_paste_action))
            .child(
                Input::new(&self.ai_composer_input_state)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .w_full()
                    .h(px(100.0)),
            )
            .when_some(self.ai_composer_file_completion_menu.clone(), |this, menu| {
                this.child(
                    self.render_ai_composer_file_completion_menu(view.clone(), menu, is_dark, cx),
                )
            })
            .when_some(self.ai_composer_slash_command_menu.clone(), |this, menu| {
                this.child(
                    self.render_ai_composer_slash_command_menu(view.clone(), menu, is_dark, cx),
                )
            })
            .when_some(self.ai_composer_skill_completion_menu.clone(), |this, menu| {
                this.child(
                    self.render_ai_composer_skill_completion_menu(view.clone(), menu, is_dark, cx),
                )
            })
            .into_any_element();
        let footer = h_flex()
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
                        let model_supports_image_inputs = state.model_supports_image_inputs;
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
                        state.show_thread_mode_picker,
                        state.thread_mode_picker_editable,
                        state.session_controls_read_only,
                        cx,
                    ))
                    .when(state.show_mode_badge, |this| {
                        this.child(ai_render_composer_status_chip(
                            ai_composer_mode_badge_label(state.current_mode_label.as_str()).as_str(),
                            ai_composer_mode_badge_icon(state.current_mode_label.as_str()),
                            completion_colors.row_selected_border,
                            completion_colors.accent_soft_background,
                            cx.theme().foreground,
                        ))
                    })
                    .when(state.fast_mode_enabled, |this| {
                        this.child(ai_render_composer_status_chip(
                            "Fast",
                            Some(HunkIconName::Rocket),
                            completion_colors.row_selected_border,
                            completion_colors.accent_soft_background,
                            cx.theme().foreground,
                        ))
                    }),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_end()
                    .gap(footer_action_gap)
                    .when_some(context_usage_chip, |this, chip| this.child(chip))
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
                            let review_mode_active = state.review_mode_active;
                            let review_action_tooltip = state.review_action_blocker.clone().unwrap_or_else(|| {
                                "Review the current working-copy changes for correctness and regressions.".to_string()
                            });
                            Button::new("ai-send-prompt")
                                .compact()
                                .primary()
                                .rounded(px(999.0))
                                .with_size(gpui_component::Size::Small)
                                .icon(Icon::new(IconName::ArrowUp).size(px(16.0)))
                                .tooltip(if composer_send_waiting_on_connection {
                                    "Wait for Codex to finish connecting.".to_string()
                                } else if review_mode_active {
                                    review_action_tooltip
                                } else {
                                    "Send prompt".to_string()
                                })
                                .disabled(composer_send_waiting_on_connection)
                                .on_click(move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_send_prompt_action(window, cx);
                                    });
                                })
                        }
                    }),
            )
            .into_any_element();
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
                    .when_some(feedback_strip, |this, strip| {
                        this.child(strip)
                    })
                    .when_some(usage_popover, |this, popover| {
                        this.child(popover)
                    })
                    .when_some(state.followup_prompt, |this, prompt| {
                        this.child(
                            render_ai_followup_prompt_card(
                                view.clone(),
                                prompt,
                                state.followup_prompt_action,
                                is_dark,
                                cx,
                            ),
                        )
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
                            .when_some(attachment_chips, |this, chips| {
                                this.child(chips)
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
                            .child(input_shell)
                            .child(footer)
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
        let menu_colors = hunk_completion_menu(cx.theme(), is_dark);
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
        let mono_font_family = cx.theme().mono_font_family.clone();

        self.render_ai_composer_completion_menu_shell(
            AiComposerCompletionMenuShell {
                menu_id: "ai-composer-file-completion-menu",
                scroll_area_id: "ai-composer-file-completion-scroll-area",
                anchor_position,
                min_width: px(280.0),
                max_width: px(420.0),
                max_height: px(260.0),
                scroll_handle: &self.ai_composer_file_completion_scroll_handle,
            },
            is_dark,
            cx,
            menu.items.iter().enumerate().map(|(ix, path)| {
                let select_view = view.clone();
                let select_path = path.clone();
                let file_name = path
                    .rsplit('/')
                    .next()
                    .unwrap_or(path.as_str())
                    .to_string();
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
                    .hover(|style| style.bg(menu_colors.row_hover))
                    .when(selected, |this| {
                        this.bg(menu_colors.row_selected)
                            .border_1()
                            .border_color(menu_colors.row_selected_border)
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
                                menu_colors.accent_text
                            } else {
                                menu_colors.secondary_text
                            }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .w_full()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(menu_colors.primary_text)
                                    .font_family(mono_font_family.clone())
                                    .child(file_name),
                            )
                            .when(!dir_prefix.is_empty(), |this| {
                                this.child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .truncate()
                                        .text_xs()
                                        .font_family(mono_font_family.clone())
                                        .text_color(if selected {
                                            menu_colors.selected_secondary_text
                                        } else {
                                            menu_colors.secondary_text
                                        })
                                        .child(dir_prefix),
                                )
                            }),
                    )
                    .into_any_element()
            }),
        )
    }

    fn render_ai_composer_skill_completion_menu(
        &self,
        view: Entity<Self>,
        menu: AiComposerSkillCompletionMenuState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu_colors = hunk_completion_menu(cx.theme(), is_dark);
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
        let mono_font_family = cx.theme().mono_font_family.clone();

        self.render_ai_composer_completion_menu_shell(
            AiComposerCompletionMenuShell {
                menu_id: "ai-composer-skill-completion-menu",
                scroll_area_id: "ai-composer-skill-completion-scroll-area",
                anchor_position,
                min_width: px(320.0),
                max_width: px(460.0),
                max_height: px(280.0),
                scroll_handle: &self.ai_composer_skill_completion_scroll_handle,
            },
            is_dark,
            cx,
            menu.items.iter().enumerate().map(|(ix, item)| {
                let select_view = view.clone();
                let select_name = item.name.clone();
                let selected = ix == selected_ix;
                let title = item
                    .display_name
                    .as_deref()
                    .unwrap_or(item.name.as_str());
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
                    .hover(|style| style.bg(menu_colors.row_hover))
                    .when(selected, |this| {
                        this.bg(menu_colors.row_selected)
                            .border_1()
                            .border_color(menu_colors.row_selected_border)
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
                                menu_colors.accent_text
                            } else {
                                menu_colors.secondary_text
                            }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .w_full()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(menu_colors.primary_text)
                                    .child(title.to_string()),
                            )
                            .when(show_name, |this| {
                                this.child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .truncate()
                                        .text_xs()
                                        .font_family(mono_font_family.clone())
                                        .text_color(if selected {
                                            menu_colors.selected_secondary_text
                                        } else {
                                            menu_colors.secondary_text
                                        })
                                        .child(format!("${}", item.name)),
                                )
                            })
                            .when_some(item.description.clone(), |this, description| {
                                this.child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .text_xs()
                                        .whitespace_normal()
                                        .text_color(if selected {
                                            menu_colors.selected_secondary_text
                                        } else {
                                            menu_colors.secondary_text
                                        })
                                        .child(description),
                                )
                            }),
                    )
                    .into_any_element()
            }),
        )
    }

    fn render_ai_composer_slash_command_menu(
        &self,
        view: Entity<Self>,
        menu: crate::app::AiComposerSlashCommandMenuState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu_colors = hunk_completion_menu(cx.theme(), is_dark);
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
        let mono_font_family = cx.theme().mono_font_family.clone();

        self.render_ai_composer_completion_menu_shell(
            AiComposerCompletionMenuShell {
                menu_id: "ai-composer-slash-command-menu",
                scroll_area_id: "ai-composer-slash-command-scroll-area",
                anchor_position,
                min_width: px(320.0),
                max_width: px(460.0),
                max_height: px(280.0),
                scroll_handle: &self.ai_composer_slash_command_scroll_handle,
            },
            is_dark,
            cx,
            menu.items.iter().enumerate().map(|(ix, item)| {
                let select_view = view.clone();
                let command_name = item.item.name.to_string();
                let disabled = item.disabled_reason.is_some();
                let selected = ix == selected_ix;
                let is_active = selected && !disabled;

                h_flex()
                    .id(("ai-composer-slash-command-item", ix))
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .rounded(px(12.0))
                    .px_2p5()
                    .py_2()
                    .text_sm()
                    .when(!disabled, |this| this.hover(|style| style.bg(menu_colors.row_hover)))
                    .when(is_active, |this| {
                        this.bg(menu_colors.row_selected)
                            .border_1()
                            .border_color(menu_colors.row_selected_border)
                    })
                    .when(selected && disabled, |this| {
                        this.bg(menu_colors.row_hover)
                            .border_1()
                            .border_color(menu_colors.panel.border)
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
                            .flex_1()
                            .w_full()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .font_family(mono_font_family.clone())
                                    .text_color(if disabled {
                                        menu_colors.secondary_text
                                    } else {
                                        menu_colors.accent_text
                                    })
                                    .child(format!("/{}", item.item.name)),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(if is_active {
                                        menu_colors.selected_secondary_text
                                    } else {
                                        menu_colors.secondary_text
                                    })
                                    .child(item.disabled_reason.unwrap_or(item.item.description)),
                            ),
                    )
                    .into_any_element()
            }),
        )
    }

    fn render_ai_composer_completion_menu_shell<I>(
        &self,
        shell: AiComposerCompletionMenuShell<'_>,
        is_dark: bool,
        cx: &mut Context<Self>,
        rows: I,
    ) -> AnyElement
    where
        I: IntoIterator<Item = AnyElement>,
    {
        let menu_colors = hunk_completion_menu(cx.theme(), is_dark);

        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(shell.anchor_position)
                .offset(point(px(0.), -px(AI_COMPOSER_FILE_COMPLETION_MENU_GAP_Y)))
                .anchor(Corner::BottomLeft)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    div()
                        .id(shell.menu_id)
                        .min_w(shell.min_width)
                        .max_w(shell.max_width)
                        .relative()
                        .rounded(px(18.0))
                        .border_1()
                        .border_color(menu_colors.panel.border)
                        .bg(menu_colors.panel.background)
                        .overflow_hidden()
                        .shadow_lg()
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
                                .id(shell.scroll_area_id)
                                .max_h(shell.max_height)
                                .track_scroll(shell.scroll_handle)
                                .overflow_y_scroll()
                                .children(rows.into_iter().map(|row| {
                                    div().w_full().px_1().pr_3().child(row).into_any_element()
                                })),
                        )
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .right_0()
                                .bottom_0()
                                .w(px(12.0))
                                .child(
                                    Scrollbar::vertical(shell.scroll_handle)
                                        .scrollbar_show(ScrollbarShow::Always),
                                ),
                        ),
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

fn ai_render_composer_feedback_strip(
    feedback: Option<&AiComposerFeedbackState>,
    this: &DiffViewer,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> Option<AnyElement> {
    match feedback {
        Some(AiComposerFeedbackState::Status { message, tone }) => {
            Some(ai_render_composer_status_strip(message.as_str(), *tone, is_dark, cx))
        }
        Some(AiComposerFeedbackState::Activity(activity)) => {
            Some(ai_render_composer_activity_strip(this, activity, is_dark, cx))
        }
        None => None,
    }
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

fn ai_render_composer_activity_strip(
    this: &DiffViewer,
    activity: &AiComposerFeedbackActivity,
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
                        .child(activity.label.clone()),
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
                            .child(activity.label.clone())
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
                .child(ai_activity_elapsed_label(activity.started_at.elapsed())),
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
