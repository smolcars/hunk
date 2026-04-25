fn render_ai_account_actions_for_view(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    _: &mut Context<DiffViewer>,
) -> AnyElement {
    let login_pending = this.ai_pending_chatgpt_login_id.is_some();

    h_flex()
        .items_center()
        .gap_1()
        .flex_wrap()
        .when(!login_pending && this.ai_account.is_none(), |this| {
            this.child({
                let view = view.clone();
                Button::new("ai-account-login")
                    .compact()
                    .primary()
                    .with_size(gpui_component::Size::Small)
                    .label("Login")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.ai_start_chatgpt_login_action(cx);
                        });
                    })
            })
        })
        .when(login_pending, |this| {
            this.child({
                let view = view.clone();
                Button::new("ai-account-cancel-login")
                    .compact()
                    .outline()
                    .with_size(gpui_component::Size::Small)
                    .label("Cancel Login")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.ai_cancel_chatgpt_login_action(cx);
                        });
                    })
            })
        })
        .when(this.ai_account.is_some(), |this| {
            this.child({
                let view = view.clone();
                Button::new("ai-account-logout")
                    .compact()
                    .outline()
                    .with_size(gpui_component::Size::Small)
                    .label("Logout")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.ai_logout_account_action(cx);
                        });
                    })
            })
        })
        .into_any_element()
}

struct AiSessionControlsPanelView<'a> {
    models: &'a [hunk_codex::protocol::Model],
    selected_model: Option<&'a str>,
    selected_effort: Option<&'a str>,
    selected_thread_mode: AiNewThreadStartMode,
    show_thread_mode_picker: bool,
    thread_mode_editable: bool,
    read_only: bool,
    mad_max_mode: bool,
}

fn render_ai_session_controls_panel_for_view(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    selected_thread_mode: AiNewThreadStartMode,
    show_thread_mode_picker: bool,
    thread_mode_editable: bool,
    read_only: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    render_ai_session_controls_panel(
        AiSessionControlsPanelView {
            models: this.ai_models.as_slice(),
            selected_model: this.ai_selected_model.as_deref(),
            selected_effort: this.ai_selected_effort.as_deref(),
            selected_thread_mode,
            show_thread_mode_picker,
            thread_mode_editable,
            read_only,
            mad_max_mode: this.ai_mad_max_mode,
        },
        view,
        cx,
    )
}

fn render_ai_session_controls_panel(
    panel: AiSessionControlsPanelView<'_>,
    view: Entity<DiffViewer>,
    _cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let controls_gap = px(2.0);
    let model_label = ai_model_picker_label(panel.models, panel.selected_model);
    let selected_model = panel
        .selected_model
        .and_then(|selected| panel.models.iter().find(|model| model.id == selected));
    let effort_options = selected_model
        .map(|model| {
            model
                .supported_reasoning_efforts
                .iter()
                .map(|option| {
                    (
                        ai_reasoning_effort_key(&option.reasoning_effort),
                        ai_reasoning_effort_label(
                            ai_reasoning_effort_key(&option.reasoning_effort).as_str(),
                        ),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let effort_label = ai_effort_picker_label(panel.selected_effort, selected_model);
    let approval_policy_label = ai_approval_policy_picker_label(panel.mad_max_mode);
    let thread_mode_label = panel.selected_thread_mode.label().to_string();
    let controls_locked_tooltip = "Session settings are locked while the agent is working.";
    let (visible_models, hidden_models): (Vec<_>, Vec<_>) = panel
        .models
        .iter()
        .map(|model| (model.id.clone(), model.display_name.clone(), model.hidden))
        .partition(|(_, _, hidden)| !*hidden);

    h_flex()
        .min_w_0()
        .items_center()
        .gap(controls_gap)
        .flex_wrap()
        .child({
            let view = view.clone();
            let selected_model = panel.selected_model.map(ToOwned::to_owned);
            Button::new("ai-session-model-dropdown")
                .compact()
                .ghost()
                .rounded(px(999.0))
                .with_size(gpui_component::Size::Small)
                .px_1()
                .dropdown_caret(true)
                .disabled(panel.read_only)
                .label(model_label)
                .when(panel.read_only, |this| this.tooltip(controls_locked_tooltip))
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
                    for (model_id, label, _) in &visible_models {
                        let model_id_value = model_id.clone();
                        let label = label.clone();
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .checked(
                                    selected_model.as_deref() == Some(model_id_value.as_str()),
                                )
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let selected = model_id_value.clone();
                                        view.update(cx, |this, cx| {
                                            this.ai_select_model_action(Some(selected.clone()), cx);
                                        });
                                    }
                                }),
                        );
                    }
                    if !hidden_models.is_empty() {
                        menu = menu
                            .item(PopupMenuItem::separator())
                            .item(PopupMenuItem::label("Hidden Models"));
                        for (model_id, label, _) in &hidden_models {
                            let model_id_value = model_id.clone();
                            let label = label.clone();
                            menu = menu.item(
                                PopupMenuItem::new(label)
                                    .checked(selected_model.as_deref() == Some(model_id_value.as_str()))
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
                    }
                    menu
                })
        })
        .child({
            let view = view.clone();
            let selected_effort = panel.selected_effort.map(ToOwned::to_owned);
            Button::new("ai-session-effort-dropdown")
                .compact()
                .ghost()
                .rounded(px(999.0))
                .with_size(gpui_component::Size::Small)
                .px_1()
                .dropdown_caret(true)
                .disabled(panel.read_only || selected_model.is_none())
                .label(effort_label)
                .when(panel.read_only, |this| this.tooltip(controls_locked_tooltip))
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
                                .checked(selected_effort.as_deref() == Some(effort_value.as_str()))
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
        })
        .child({
            let view = view.clone();
            let mad_max_mode = panel.mad_max_mode;
            Button::new("ai-session-approval-policy-dropdown")
                .compact()
                .ghost()
                .rounded(px(999.0))
                .with_size(gpui_component::Size::Small)
                .px_1()
                .dropdown_caret(true)
                .disabled(panel.read_only)
                .label(approval_policy_label)
                .when(panel.read_only, |this| this.tooltip(controls_locked_tooltip))
                .dropdown_menu(move |menu, _, _| {
                    let mut menu = menu;
                    for (full_access, label) in ai_approval_policy_options() {
                        menu = menu.item(
                            PopupMenuItem::new(*label)
                                .checked(mad_max_mode == *full_access)
                                .on_click({
                                    let view = view.clone();
                                    let full_access = *full_access;
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_set_mad_max_mode(full_access, cx);
                                        });
                                    }
                                }),
                        );
                    }
                    menu
                })
        })
        .when(panel.show_thread_mode_picker, |this| {
            this.child({
                let view = view.clone();
                let selected_mode = panel.selected_thread_mode;
                Button::new("ai-session-thread-mode-dropdown")
                    .compact()
                    .ghost()
                    .rounded(px(999.0))
                    .with_size(gpui_component::Size::Small)
                    .px_1()
                    .dropdown_caret(true)
                    .label(thread_mode_label)
                    .disabled(panel.read_only || !panel.thread_mode_editable)
                    .tooltip(if panel.read_only {
                        controls_locked_tooltip
                    } else {
                        "Thread mode can only be changed before the first prompt is sent."
                    })
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::new("Local")
                                .checked(matches!(selected_mode, AiNewThreadStartMode::Local))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_select_new_thread_start_mode_action(
                                                AiNewThreadStartMode::Local,
                                                cx,
                                            );
                                        });
                                    }
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Worktree")
                                .checked(matches!(selected_mode, AiNewThreadStartMode::Worktree))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_select_new_thread_start_mode_action(
                                                AiNewThreadStartMode::Worktree,
                                                cx,
                                            );
                                        });
                                    }
                                }),
                        )
                    })
            })
        })
        .into_any_element()
}

fn ai_account_summary(
    account: Option<&hunk_codex::protocol::Account>,
    requires_openai_auth: bool,
    account_loading: bool,
) -> String {
    if account_loading {
        return "Loading account...".to_string();
    }

    match account {
        Some(hunk_codex::protocol::Account::ApiKey { .. }) => {
            "Signed in with API key.".to_string()
        }
        Some(hunk_codex::protocol::Account::Chatgpt { email, plan_type }) => {
            format!("ChatGPT: {email} ({plan_type:?})")
        }
        Some(hunk_codex::protocol::Account::AmazonBedrock { .. }) => {
            "Signed in with Amazon Bedrock.".to_string()
        }
        None if requires_openai_auth => {
            "Sign in with ChatGPT to run coding agents.".to_string()
        }
        None => "No account connected.".to_string(),
    }
}

fn ai_rate_limit_windows(
    snapshot: &hunk_codex::protocol::RateLimitSnapshot,
) -> (
    Option<hunk_codex::protocol::RateLimitWindow>,
    Option<hunk_codex::protocol::RateLimitWindow>,
) {
    let primary = snapshot.primary.clone();
    let secondary = snapshot.secondary.clone();
    let mut five_hour = None;
    let mut weekly = None;

    for window in [primary.clone(), secondary.clone()].into_iter().flatten() {
        match window.window_duration_mins {
            Some(300) => five_hour = Some(window.clone()),
            Some(10_080) => weekly = Some(window.clone()),
            _ => {}
        }
    }

    if five_hour.is_none() {
        five_hour = primary.clone().or(secondary.clone());
    }

    if weekly.is_none() {
        weekly = secondary
            .clone()
            .filter(|window| five_hour.as_ref() != Some(window))
            .or_else(|| {
                primary
                    .clone()
                    .filter(|window| five_hour.as_ref() != Some(window))
            });
    }

    (five_hour, weekly)
}

fn ai_month_short(month: time::Month) -> &'static str {
    match month {
        time::Month::January => "Jan",
        time::Month::February => "Feb",
        time::Month::March => "Mar",
        time::Month::April => "Apr",
        time::Month::May => "May",
        time::Month::June => "Jun",
        time::Month::July => "Jul",
        time::Month::August => "Aug",
        time::Month::September => "Sep",
        time::Month::October => "Oct",
        time::Month::November => "Nov",
        time::Month::December => "Dec",
    }
}

fn ai_hour_and_meridiem(hour_24: u8) -> (u8, &'static str) {
    match hour_24 {
        0 => (12, "AM"),
        1..=11 => (hour_24, "AM"),
        12 => (12, "PM"),
        _ => (hour_24 - 12, "PM"),
    }
}

fn ai_model_picker_label(
    models: &[hunk_codex::protocol::Model],
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
    selected_model: Option<&hunk_codex::protocol::Model>,
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
                    .map(|option| {
                        ai_reasoning_effort_label(
                            ai_reasoning_effort_key(&option.reasoning_effort).as_str(),
                        )
                    })
            })
            .unwrap_or_else(|| format!("{} (unsupported)", ai_reasoning_effort_label(selected_key))),
        None => "Model default".to_string(),
    }
}

fn ai_reasoning_effort_label(value: &str) -> String {
    match value {
        "minimal" => "Minimal".to_string(),
        "low" => "Low".to_string(),
        "medium" => "Medium".to_string(),
        "high" => "High".to_string(),
        "extra_high" | "extra-high" => "Extra High".to_string(),
        other => other
            .split(['_', '-'])
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}
