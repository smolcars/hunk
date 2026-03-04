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

struct AiAccountPanelView<'a> {
    account: Option<&'a codex_app_server_protocol::Account>,
    requires_openai_auth: bool,
    pending_chatgpt_login_id: Option<&'a str>,
    pending_chatgpt_auth_url: Option<&'a str>,
    rate_limits: Option<&'a codex_app_server_protocol::RateLimitSnapshot>,
    is_dark: bool,
}

fn render_ai_account_panel_for_view(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    render_ai_account_panel(
        AiAccountPanelView {
            account: this.ai_account.as_ref(),
            requires_openai_auth: this.ai_requires_openai_auth,
            pending_chatgpt_login_id: this.ai_pending_chatgpt_login_id.as_deref(),
            pending_chatgpt_auth_url: this.ai_pending_chatgpt_auth_url.as_deref(),
            rate_limits: this.ai_rate_limits.as_ref(),
            is_dark,
        },
        view,
        cx,
    )
}

fn render_ai_account_panel(
    panel: AiAccountPanelView<'_>,
    view: Entity<DiffViewer>,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let login_pending = panel.pending_chatgpt_login_id.is_some();
    let summary = ai_account_summary(panel.account, panel.requires_openai_auth);

    v_flex()
        .w_full()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(if panel.is_dark { 0.20 } else { 0.40 }))
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
        .when_some(ai_rate_limit_summary(panel.rate_limits), |this, summary| {
            this.child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(summary),
            )
        })
        .when_some(panel.pending_chatgpt_login_id, |this, login_id| {
            this.child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(format!("loginId: {login_id}")),
            )
        })
        .when_some(panel.pending_chatgpt_auth_url, |this, auth_url| {
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
                        .disabled(panel.account.is_none())
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_logout_account_action(cx);
                            });
                        })
                }),
        )
        .into_any_element()
}

struct AiSessionControlsPanelView<'a> {
    models: &'a [codex_app_server_protocol::Model],
    experimental_features: &'a [codex_app_server_protocol::ExperimentalFeature],
    collaboration_modes: &'a [codex_app_server_protocol::CollaborationModeMask],
    include_hidden_models: bool,
    selected_model: Option<&'a str>,
    selected_effort: Option<&'a str>,
    selected_collaboration_mode: Option<&'a str>,
    is_dark: bool,
}

fn render_ai_session_controls_panel_for_view(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    render_ai_session_controls_panel(
        AiSessionControlsPanelView {
            models: this.ai_models.as_slice(),
            experimental_features: this.ai_experimental_features.as_slice(),
            collaboration_modes: this.ai_collaboration_modes.as_slice(),
            include_hidden_models: this.ai_include_hidden_models,
            selected_model: this.ai_selected_model.as_deref(),
            selected_effort: this.ai_selected_effort.as_deref(),
            selected_collaboration_mode: this.ai_selected_collaboration_mode.as_deref(),
            is_dark,
        },
        view,
        cx,
    )
}

fn render_ai_session_controls_panel(
    panel: AiSessionControlsPanelView<'_>,
    view: Entity<DiffViewer>,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let model_label = ai_model_picker_label(panel.models, panel.selected_model);
    let selected_model = panel
        .selected_model
        .and_then(|selected| panel.models.iter().find(|model| model.id == selected));
    let selected_model_unavailable = panel
        .selected_model
        .is_some_and(|selected| panel.models.iter().all(|model| model.id != selected));
    let effort_options = selected_model
        .map(|model| {
            model
                .supported_reasoning_efforts
                .iter()
                .map(|option| {
                    (
                        ai_reasoning_effort_key(&option.reasoning_effort),
                        option.description.clone(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let effort_label = ai_effort_picker_label(panel.selected_effort, selected_model);
    let collaboration_enabled = ai_experimental_feature_enabled(
        panel.experimental_features,
        "collaboration_modes",
    ) && !panel.collaboration_modes.is_empty();
    let collaboration_label = ai_collaboration_picker_label(panel.selected_collaboration_mode);
    let model_items = panel
        .models
        .iter()
        .map(|model| {
            let suffix = if model.hidden { " (hidden)" } else { "" };
            (model.id.clone(), format!("{}{}", model.display_name, suffix))
        })
        .collect::<Vec<_>>();
    let collaboration_items = panel
        .collaboration_modes
        .iter()
        .map(|mode| mode.name.clone())
        .collect::<Vec<_>>();

    v_flex()
        .w_full()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(if panel.is_dark { 0.20 } else { 0.40 }))
        .p_2()
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_xs().font_semibold().child("Session"))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Models: {}", panel.models.len())),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap_1()
                .child({
                    let view = view.clone();
                    Button::new("ai-session-refresh")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .label("Refresh")
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_refresh_session_metadata(cx);
                            });
                        })
                })
                .child({
                    let view = view.clone();
                    let enable_hidden = !panel.include_hidden_models;
                    Button::new("ai-session-toggle-hidden-models")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .label(if panel.include_hidden_models {
                            "Hidden Models On"
                        } else {
                            "Hidden Models Off"
                        })
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_set_include_hidden_models_action(enable_hidden, cx);
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
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Model"))
                .child({
                    let view = view.clone();
                    let selected_model = panel.selected_model.map(ToOwned::to_owned);
                    Button::new("ai-session-model-dropdown")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .dropdown_caret(true)
                        .label(model_label)
                        .dropdown_menu(move |menu, _, _| {
                            let mut menu = menu.item(
                                PopupMenuItem::new("Server default")
                                    .checked(selected_model.is_none())
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_select_model_action(None, cx);
                                            });
                                        }
                                    }),
                            );
                            for (model_id, label) in &model_items {
                                let model_id_value = model_id.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(label.clone())
                                        .checked(
                                            selected_model
                                                .as_deref()
                                                == Some(model_id_value.as_str()),
                                        )
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                let selected = model_id_value.clone();
                                                view.update(cx, |this, cx| {
                                                    this.ai_select_model_action(
                                                        Some(selected.clone()),
                                                        cx,
                                                    );
                                                });
                                            }
                                        }),
                                );
                            }
                            menu
                        })
                }),
        )
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Effort"))
                .child({
                    let view = view.clone();
                    let selected_effort = panel.selected_effort.map(ToOwned::to_owned);
                    Button::new("ai-session-effort-dropdown")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .dropdown_caret(true)
                        .disabled(selected_model.is_none())
                        .label(effort_label)
                        .dropdown_menu(move |menu, _, _| {
                            let mut menu = menu.item(
                                PopupMenuItem::new("Model default")
                                    .checked(selected_effort.is_none())
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_select_effort_action(None, cx);
                                            });
                                        }
                                    }),
                            );
                            for (effort_key, description) in &effort_options {
                                let effort_value = effort_key.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(description.clone())
                                        .checked(
                                            selected_effort
                                                .as_deref()
                                                == Some(effort_value.as_str()),
                                        )
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                let selected = effort_value.clone();
                                                view.update(cx, |this, cx| {
                                                    this.ai_select_effort_action(
                                                        Some(selected.clone()),
                                                        cx,
                                                    );
                                                });
                                            }
                                        }),
                                );
                            }
                            menu
                        })
                }),
        )
        .when(collaboration_enabled, |this| {
            this.child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Collaboration"),
                    )
                    .child({
                        let view = view.clone();
                        let selected = panel.selected_collaboration_mode.map(ToOwned::to_owned);
                        Button::new("ai-session-collaboration-dropdown")
                            .compact()
                            .outline()
                            .with_size(gpui_component::Size::Small)
                            .dropdown_caret(true)
                            .label(collaboration_label)
                            .dropdown_menu(move |menu, _, _| {
                                let mut menu = menu.item(
                                    PopupMenuItem::new("Off")
                                        .checked(selected.is_none())
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.ai_select_collaboration_mode_action(
                                                        None, cx,
                                                    );
                                                });
                                            }
                                        }),
                                );
                                for mode_name in &collaboration_items {
                                    let mode_value = mode_name.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(mode_name.clone())
                                            .checked(
                                                selected.as_deref()
                                                    == Some(mode_value.as_str()),
                                            )
                                            .on_click({
                                                let view = view.clone();
                                                move |_, _, cx| {
                                                    let selected_mode = mode_value.clone();
                                                    view.update(cx, |this, cx| {
                                                        this.ai_select_collaboration_mode_action(
                                                            Some(selected_mode.clone()),
                                                            cx,
                                                        );
                                                    });
                                                }
                                            }),
                                    );
                                }
                                menu
                            })
                    }),
            )
        })
        .when(selected_model_unavailable, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().warning)
                    .whitespace_normal()
                    .child(
                        "Selected model is unavailable in this catalog. Hunk will fall back to server defaults.",
                    ),
            )
        })
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

fn ai_model_picker_label(
    models: &[codex_app_server_protocol::Model],
    selected_model: Option<&str>,
) -> String {
    match selected_model {
        Some(model_id) => models
            .iter()
            .find(|model| model.id == model_id)
            .map(|model| model.display_name.clone())
            .unwrap_or_else(|| format!("{model_id} (unavailable)")),
        None => "Server default".to_string(),
    }
}

fn ai_effort_picker_label(
    selected_effort: Option<&str>,
    selected_model: Option<&codex_app_server_protocol::Model>,
) -> String {
    if selected_model.is_none() {
        return "No model selected".to_string();
    }
    match selected_effort {
        Some(selected_key) => selected_model
            .and_then(|model| {
                model
                    .supported_reasoning_efforts
                    .iter()
                    .find(|option| ai_reasoning_effort_key(&option.reasoning_effort) == selected_key)
                    .map(|option| option.description.clone())
            })
            .unwrap_or_else(|| format!("{selected_key} (unsupported)")),
        None => "Model default".to_string(),
    }
}

fn ai_collaboration_picker_label(selected: Option<&str>) -> String {
    selected
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "Off".to_string())
}

fn ai_experimental_feature_enabled(
    features: &[codex_app_server_protocol::ExperimentalFeature],
    key: &str,
) -> bool {
    features
        .iter()
        .find(|feature| feature.name == key)
        .map(|feature| feature.enabled)
        .unwrap_or(false)
}

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
