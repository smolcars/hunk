fn ai_connection_label(
    state: AiConnectionState,
    cx: &mut Context<DiffViewer>,
) -> (&'static str, Hsla) {
    match state {
        AiConnectionState::Disconnected => ("Disconnected", cx.theme().muted_foreground),
        AiConnectionState::Connecting => ("Connecting", cx.theme().warning),
        AiConnectionState::Ready => ("Connected", cx.theme().success),
        AiConnectionState::Failed => ("Failed", cx.theme().danger),
    }
}

fn ai_thread_status_label(
    status: ThreadLifecycleStatus,
    cx: &mut Context<DiffViewer>,
) -> (&'static str, Hsla) {
    match status {
        ThreadLifecycleStatus::Active => ("active", cx.theme().success),
        ThreadLifecycleStatus::Archived => ("archived", cx.theme().warning),
        ThreadLifecycleStatus::Closed => ("closed", cx.theme().muted_foreground),
    }
}

fn ai_turn_status_label(status: TurnStatus) -> &'static str {
    match status {
        TurnStatus::InProgress => "in-progress",
        TurnStatus::Completed => "completed",
    }
}

fn ai_item_status_label(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Started => "started",
        ItemStatus::Streaming => "streaming",
        ItemStatus::Completed => "completed",
    }
}

fn ai_item_status_color(status: ItemStatus, cx: &mut Context<DiffViewer>) -> Hsla {
    match status {
        ItemStatus::Started => cx.theme().muted_foreground,
        ItemStatus::Streaming => cx.theme().accent,
        ItemStatus::Completed => cx.theme().success,
    }
}

fn ai_approval_kind_label(kind: AiApprovalKind) -> &'static str {
    match kind {
        AiApprovalKind::CommandExecution => "Command Execution Approval",
        AiApprovalKind::FileChange => "File Change Approval",
    }
}

fn ai_approval_description(approval: &AiPendingApproval) -> String {
    match approval.kind {
        AiApprovalKind::CommandExecution => {
            if let Some(command) = approval.command.as_ref() {
                return format!("Command: {command}");
            }
            if let Some(cwd) = approval.cwd.as_ref() {
                return format!("Requested in {}", cwd.display());
            }
            "Command execution request".to_string()
        }
        AiApprovalKind::FileChange => {
            if let Some(grant_root) = approval.grant_root.as_ref() {
                return format!("Grant write access under {}", grant_root.display());
            }
            "File change request".to_string()
        }
    }
}

fn render_ai_account_panel(
    account: Option<&codex_app_server_protocol::Account>,
    requires_openai_auth: bool,
    pending_chatgpt_login_id: Option<&str>,
    pending_chatgpt_auth_url: Option<&str>,
    rate_limits: Option<&codex_app_server_protocol::RateLimitSnapshot>,
    view: Entity<DiffViewer>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let login_pending = pending_chatgpt_login_id.is_some();
    let summary = ai_account_summary(account, requires_openai_auth);

    v_flex()
        .w_full()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(if is_dark { 0.20 } else { 0.40 }))
        .p_2()
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_xs().font_semibold().child("Account"))
                .child(
                    div()
                        .text_xs()
                        .text_color(if login_pending {
                            cx.theme().warning
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(if login_pending {
                            "Login Pending"
                        } else {
                            "Ready"
                        }),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .whitespace_normal()
                .child(summary),
        )
        .when_some(ai_rate_limit_summary(rate_limits), |this, summary| {
            this.child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(summary),
            )
        })
        .when_some(pending_chatgpt_login_id, |this, login_id| {
            this.child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(format!("loginId: {login_id}")),
            )
        })
        .when_some(pending_chatgpt_auth_url, |this, auth_url| {
            this.child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(format!("authUrl: {auth_url}")),
            )
        })
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap_1()
                .child({
                    let view = view.clone();
                    Button::new("ai-account-refresh")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .label("Refresh")
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_refresh_account(cx);
                            });
                        })
                })
                .child({
                    let view = view.clone();
                    Button::new("ai-account-login")
                        .compact()
                        .primary()
                        .with_size(gpui_component::Size::Small)
                        .label("Login")
                        .disabled(login_pending)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_start_chatgpt_login_action(cx);
                            });
                        })
                })
                .child({
                    let view = view.clone();
                    Button::new("ai-account-cancel-login")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .label("Cancel Login")
                        .disabled(!login_pending)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_cancel_chatgpt_login_action(cx);
                            });
                        })
                })
                .child({
                    let view = view.clone();
                    Button::new("ai-account-logout")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .label("Logout")
                        .disabled(account.is_none())
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_logout_account_action(cx);
                            });
                        })
                }),
        )
        .into_any_element()
}

fn ai_account_summary(
    account: Option<&codex_app_server_protocol::Account>,
    requires_openai_auth: bool,
) -> String {
    match account {
        Some(codex_app_server_protocol::Account::ApiKey { .. }) => {
            "Signed in with API key.".to_string()
        }
        Some(codex_app_server_protocol::Account::Chatgpt { email, plan_type }) => {
            format!("ChatGPT: {email} ({plan_type:?})")
        }
        None if requires_openai_auth => {
            "Sign in with ChatGPT to run coding agents.".to_string()
        }
        None => "No account connected.".to_string(),
    }
}

fn ai_rate_limit_summary(
    rate_limits: Option<&codex_app_server_protocol::RateLimitSnapshot>,
) -> Option<String> {
    let snapshot = rate_limits?;
    let primary = snapshot.primary.as_ref()?;
    let mut summary = format!("Rate limit: {}% used", primary.used_percent);
    if let Some(limit_name) = snapshot.limit_name.as_ref() {
        summary.push_str(&format!(" ({limit_name})"));
    }
    if let Some(resets_at) = primary.resets_at {
        summary.push_str(&format!(", resetsAt={resets_at}"));
    }
    Some(summary)
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
        .border_color(cx.theme().accent.opacity(if is_dark { 0.84 } else { 0.66 }))
        .bg(cx.theme().accent.opacity(if is_dark { 0.14 } else { 0.08 }))
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
                        .border_color(cx.theme().border.opacity(if is_dark { 0.92 } else { 0.74 }))
                        .bg(cx.theme().background.blend(
                            cx.theme().muted.opacity(if is_dark { 0.12 } else { 0.20 }),
                        ))
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
