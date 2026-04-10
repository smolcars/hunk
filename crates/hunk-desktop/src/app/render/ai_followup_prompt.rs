fn ai_followup_prompt_title(kind: AiFollowupPromptKind) -> &'static str {
    match kind {
        AiFollowupPromptKind::Plan => "Plan Ready",
    }
}

fn ai_followup_prompt_body(kind: AiFollowupPromptKind) -> &'static str {
    match kind {
        AiFollowupPromptKind::Plan => {
            "Switch to Code and implement the latest plan, or keep the thread in Plan mode and send a custom follow-up."
        }
    }
}

fn ai_followup_prompt_icon(kind: AiFollowupPromptKind) -> HunkIconName {
    match kind {
        AiFollowupPromptKind::Plan => HunkIconName::NotebookPen,
    }
}

fn ai_followup_prompt_primary_label(kind: AiFollowupPromptKind) -> &'static str {
    match kind {
        AiFollowupPromptKind::Plan => "Accept Plan and Implement",
    }
}

fn ai_followup_prompt_secondary_label(kind: AiFollowupPromptKind) -> &'static str {
    match kind {
        AiFollowupPromptKind::Plan => "Tell Agent What To Do",
    }
}

fn render_ai_followup_prompt_card(
    view: Entity<DiffViewer>,
    prompt: AiFollowupPrompt,
    selected_action: AiFollowupPromptAction,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let colors = hunk_completion_menu(cx.theme(), is_dark);
    let primary_selected = selected_action == AiFollowupPromptAction::Primary;
    let secondary_selected = selected_action == AiFollowupPromptAction::Secondary;

    v_flex()
        .w_full()
        .gap_2()
        .rounded(px(18.0))
        .border_1()
        .border_color(colors.row_selected_border)
        .bg(colors.accent_soft_background)
        .px_3()
        .py_2p5()
        .child(
            v_flex()
                .w_full()
                .gap_0p5()
                .child(
                    h_flex()
                        .items_center()
                        .gap_1p5()
                        .child(Icon::new(ai_followup_prompt_icon(prompt.kind)).size(px(14.0)))
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(ai_followup_prompt_title(prompt.kind)),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(cx.theme().muted_foreground)
                        .child(ai_followup_prompt_body(prompt.kind)),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .flex_wrap()
                .child({
                    let view = view.clone();
                    Button::new(("ai-followup-prompt-primary", prompt.source_sequence))
                        .primary()
                        .compact()
                        .rounded(px(999.0))
                        .with_size(gpui_component::Size::Small)
                        .selected(primary_selected)
                        .label(ai_followup_prompt_primary_label(prompt.kind))
                        .on_click(move |_, window, cx| {
                            view.update(cx, |this, cx| {
                                this.accept_current_ai_followup_prompt(window, cx);
                            });
                        })
                })
                .child({
                    let view = view.clone();
                    Button::new(("ai-followup-prompt-secondary", prompt.source_sequence))
                        .ghost()
                        .compact()
                        .rounded(px(999.0))
                        .with_size(gpui_component::Size::Small)
                        .selected(secondary_selected)
                        .label(ai_followup_prompt_secondary_label(prompt.kind))
                        .on_click(move |_, window, cx| {
                            view.update(cx, |this, cx| {
                                this.prepare_custom_followup_for_current_prompt(window, cx);
                            });
                        })
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Use Left/Right or Up/Down to choose, then Enter."),
        )
        .into_any_element()
}
