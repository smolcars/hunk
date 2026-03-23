fn ai_reasoning_effort_key(effort: &codex_protocol::openai_models::ReasoningEffort) -> String {
    serde_json::to_value(effort)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{effort:?}").to_lowercase())
}

fn render_ai_pending_user_inputs_panel(
    requests: &[AiPendingUserInputRequest],
    answer_overrides: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
    view: Entity<DiffViewer>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(hunk_opacity(cx.theme().accent, is_dark, 0.84, 0.66))
        .bg(hunk_opacity(cx.theme().accent, is_dark, 0.14, 0.08))
        .p_2()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().accent)
                .child("Pending user input"),
        )
        .children(requests.iter().enumerate().map(|(request_index, request)| {
            let submit_request_id = request.request_id.clone();
            let request_answers = answer_overrides
                .get(request.request_id.as_str())
                .cloned()
                .unwrap_or_default();
            let view = view.clone();

            v_flex()
                .w_full()
                .gap_1()
                .rounded(px(8.0))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .p_2()
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_xs().font_semibold().child("Tool input request"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .font_family(cx.theme().mono_font_family.clone())
                                .child(request.request_id.clone()),
                        ),
                )
                .children(request.questions.iter().enumerate().map(|(question_index, question)| {
                    let selected_answer = request_answers
                        .get(question.id.as_str())
                        .and_then(|answers| answers.first())
                        .cloned()
                        .unwrap_or_default();
                    let selected_answer_display = if question.is_secret {
                        "****".to_string()
                    } else {
                        selected_answer.clone()
                    };

                    v_flex()
                        .w_full()
                        .gap_1()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.92, 0.74))
                        .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.12, 0.20))
                        .p_2()
                        .child(div().text_xs().font_semibold().child(question.header.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .whitespace_normal()
                                .child(question.question.clone()),
                        )
                        .when(question.is_secret, |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().warning)
                                    .child("Secret response requested."),
                            )
                        })
                        .when(!question.options.is_empty(), |this| {
                            this.child(
                                v_flex()
                                    .w_full()
                                    .gap_1()
                                    .children(question.options.iter().enumerate().map(
                                        |(option_index, option)| {
                                            let option_label = option.label.clone();
                                            let option_label_for_click = option_label.clone();
                                            let option_description = option.description.clone();
                                            let question_id = question.id.clone();
                                            let request_id = request.request_id.clone();
                                            let button_id = format!(
                                                "ai-user-input-option-{request_index}-{question_index}-{option_index}"
                                            );
                                            let selected = option_label == selected_answer;
                                            let view = view.clone();
                                            let option_button = if selected {
                                                Button::new(button_id)
                                                    .compact()
                                                    .primary()
                                                    .with_size(gpui_component::Size::Small)
                                                    .label(option_label)
                                            } else {
                                                Button::new(button_id)
                                                    .compact()
                                                    .outline()
                                                    .with_size(gpui_component::Size::Small)
                                                    .label(option_label)
                                            };

                                            v_flex()
                                                .w_full()
                                                .gap_0p5()
                                                .child(option_button.on_click(move |_, _, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.ai_select_pending_user_input_option_action(
                                                            request_id.clone(),
                                                            question_id.clone(),
                                                            option_label_for_click.clone(),
                                                            cx,
                                                        );
                                                    });
                                                }))
                                                .when(!option_description.is_empty(), |this| {
                                                    this.child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .whitespace_normal()
                                                            .child(option_description),
                                                    )
                                                })
                                                .into_any_element()
                                        },
                                    )),
                            )
                            .when(!selected_answer.is_empty(), |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .font_family(cx.theme().mono_font_family.clone())
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("Selected: {selected_answer_display}")),
                                )
                            })
                        })
                        .when(question.options.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("No predefined options. Blank answer will be submitted."),
                            )
                        })
                        .into_any_element()
                }))
                .child(
                    h_flex().w_full().items_center().justify_end().child({
                        let view = view.clone();
                        Button::new(format!("ai-user-input-submit-{request_index}"))
                            .compact()
                            .primary()
                            .with_size(gpui_component::Size::Small)
                            .label("Submit")
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.ai_submit_pending_user_input_action(
                                        submit_request_id.clone(),
                                        cx,
                                    );
                                });
                            })
                    }),
                )
                .into_any_element()
        }))
        .into_any_element()
}
