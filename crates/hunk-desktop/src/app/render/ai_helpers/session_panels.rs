struct AiAccountPanelView<'a> {
    account: Option<&'a codex_app_server_protocol::Account>,
    account_loading: bool,
    requires_openai_auth: bool,
    pending_chatgpt_login_id: Option<&'a str>,
    pending_chatgpt_auth_url: Option<&'a str>,
    rate_limits: Option<&'a codex_app_server_protocol::RateLimitSnapshot>,
    rate_limits_loading: bool,
}

fn render_ai_account_panel_for_view(
    this: &DiffViewer,
    _: Entity<DiffViewer>,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    render_ai_account_panel(
        AiAccountPanelView {
            account: this.ai_account.as_ref(),
            account_loading: this.ai_bootstrap_loading
                && this.ai_account.is_none()
                && !this.ai_requires_openai_auth,
            requires_openai_auth: this.ai_requires_openai_auth,
            pending_chatgpt_login_id: this.ai_pending_chatgpt_login_id.as_deref(),
            pending_chatgpt_auth_url: this.ai_pending_chatgpt_auth_url.as_deref(),
            rate_limits: this.ai_rate_limits.as_ref(),
            rate_limits_loading: this.ai_bootstrap_loading && this.ai_rate_limits.is_none(),
        },
        cx,
    )
}

fn render_ai_account_panel(
    panel: AiAccountPanelView<'_>,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let login_pending = panel.pending_chatgpt_login_id.is_some();
    let summary = ai_account_summary(
        panel.account,
        panel.requires_openai_auth,
        panel.account_loading,
    );
    let (five_hour_rate_limit, weekly_rate_limit) =
        ai_rate_limit_summary(panel.rate_limits, panel.rate_limits_loading);

    v_flex()
        .w_full()
        .min_w_0()
        .items_end()
        .gap_0p5()
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .justify_end()
                .items_center()
                .gap_2()
                .flex_wrap()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(summary),
                )
                .child(
                    div()
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .child(five_hour_rate_limit),
                )
                .child(
                    div()
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .child(weekly_rate_limit),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(if panel.account_loading
                            || panel.rate_limits_loading
                            || login_pending
                        {
                            cx.theme().warning
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(if panel.account_loading || panel.rate_limits_loading {
                            "Loading"
                        } else if login_pending {
                            "Login Pending"
                        } else {
                            "Ready"
                        }),
                ),
        )
        .when_some(panel.pending_chatgpt_auth_url, |this, auth_url| {
            this.child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(auth_url.to_string()),
            )
        })
        .into_any_element()
}

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
        .child({
            let view = view.clone();
            Button::new("ai-account-refresh")
                .compact()
                .outline()
                .with_size(gpui_component::Size::Small)
                .label("Refresh Account")
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
                .disabled(this.ai_account.is_none())
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.ai_logout_account_action(cx);
                    });
                })
        })
        .into_any_element()
}

struct AiSessionControlsPanelView<'a> {
    models: &'a [codex_app_server_protocol::Model],
    experimental_features: &'a [codex_app_server_protocol::ExperimentalFeature],
    selected_model: Option<&'a str>,
    selected_effort: Option<&'a str>,
    selected_thread_mode: AiNewThreadStartMode,
    thread_mode_editable: bool,
    read_only: bool,
    selected_collaboration_mode: hunk_domain::state::AiCollaborationModeSelection,
    selected_service_tier: AiServiceTierSelection,
}

fn render_ai_session_controls_panel_for_view(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    selected_thread_mode: AiNewThreadStartMode,
    thread_mode_editable: bool,
    read_only: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    render_ai_session_controls_panel(
        AiSessionControlsPanelView {
            models: this.ai_models.as_slice(),
            experimental_features: this.ai_experimental_features.as_slice(),
            selected_model: this.ai_selected_model.as_deref(),
            selected_effort: this.ai_selected_effort.as_deref(),
            selected_thread_mode,
            thread_mode_editable,
            read_only,
            selected_collaboration_mode: this.ai_selected_collaboration_mode,
            selected_service_tier: this.ai_selected_service_tier,
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
    let model_label = ai_model_picker_label(panel.models, panel.selected_model);
    let experimental_features_label =
        ai_experimental_features_dropdown_label(panel.experimental_features);
    let experimental_feature_items =
        ai_sorted_experimental_feature_items(panel.experimental_features);
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
    let collaboration_enabled =
        ai_experimental_feature_enabled(panel.experimental_features, "collaboration_modes");
    let service_tier_label = ai_service_tier_picker_label(panel.selected_service_tier);
    let thread_mode_label = panel.selected_thread_mode.label().to_string();
    let collaboration_label = ai_collaboration_picker_label(panel.selected_collaboration_mode);
    let controls_locked_tooltip = "Session settings are locked while the agent is working.";
    let (visible_models, hidden_models): (Vec<_>, Vec<_>) = panel
        .models
        .iter()
        .map(|model| (model.id.clone(), model.display_name.clone(), model.hidden))
        .partition(|(_, _, hidden)| !*hidden);

    h_flex()
        .min_w_0()
        .items_center()
        .gap_1()
        .flex_wrap()
        .child({
            let view = view.clone();
            let selected_model = panel.selected_model.map(ToOwned::to_owned);
            Button::new("ai-session-model-dropdown")
                .compact()
                .ghost()
                .rounded(px(999.0))
                .with_size(gpui_component::Size::Small)
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
            let selected_service_tier = panel.selected_service_tier;
            Button::new("ai-session-service-tier-dropdown")
                .compact()
                .ghost()
                .rounded(px(999.0))
                .with_size(gpui_component::Size::Small)
                .dropdown_caret(true)
                .disabled(panel.read_only)
                .label(service_tier_label)
                .when(panel.read_only, |this| this.tooltip(controls_locked_tooltip))
                .dropdown_menu(move |menu, _, _| {
                    let mut menu = menu;
                    for (service_tier, label) in ai_service_tier_options() {
                        menu = menu.item(
                            PopupMenuItem::new(*label)
                                .checked(selected_service_tier == *service_tier)
                                .on_click({
                                    let view = view.clone();
                                    let service_tier = *service_tier;
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_select_service_tier_action(service_tier, cx);
                                        });
                                    }
                                }),
                        );
                    }
                    menu
                })
        })
        .child({
            let experimental_feature_items = experimental_feature_items.clone();
            Button::new("ai-session-experimental-features-dropdown")
                .compact()
                .ghost()
                .rounded(px(999.0))
                .with_size(gpui_component::Size::Small)
                .dropdown_caret(true)
                .disabled(panel.read_only)
                .label(experimental_features_label)
                .tooltip(if panel.read_only {
                    controls_locked_tooltip
                } else {
                    "Server-reported experimental features."
                })
                .dropdown_menu(move |menu, _, _| {
                    if experimental_feature_items.is_empty() {
                        return menu.item(PopupMenuItem::label(
                            "No experimental features reported",
                        ));
                    }

                    let mut menu = menu;
                    for (feature_name, enabled) in &experimental_feature_items {
                        menu = menu.item(
                            PopupMenuItem::new(feature_name.clone()).checked(*enabled),
                        );
                    }
                    menu
                })
        })
        .child({
            let view = view.clone();
            let selected_mode = panel.selected_thread_mode;
            Button::new("ai-session-thread-mode-dropdown")
                .compact()
                .ghost()
                .rounded(px(999.0))
                .with_size(gpui_component::Size::Small)
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
        .when(collaboration_enabled, |this| {
            this.child({
                let view = view.clone();
                let selected = panel.selected_collaboration_mode;
                Button::new("ai-session-collaboration-dropdown")
                    .compact()
                    .ghost()
                    .rounded(px(999.0))
                    .with_size(gpui_component::Size::Small)
                    .dropdown_caret(true)
                    .disabled(panel.read_only)
                    .label(collaboration_label)
                    .when(panel.read_only, |this| this.tooltip(controls_locked_tooltip))
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::new("Default")
                                .checked(matches!(
                                    selected,
                                    hunk_domain::state::AiCollaborationModeSelection::Default
                                ))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_select_collaboration_mode_action(
                                                hunk_domain::state::AiCollaborationModeSelection::Default,
                                                cx,
                                            );
                                        });
                                    }
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Plan")
                                .checked(matches!(
                                    selected,
                                    hunk_domain::state::AiCollaborationModeSelection::Plan
                                ))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_select_collaboration_mode_action(
                                                hunk_domain::state::AiCollaborationModeSelection::Plan,
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
    account: Option<&codex_app_server_protocol::Account>,
    requires_openai_auth: bool,
    account_loading: bool,
) -> String {
    if account_loading {
        return "Loading account...".to_string();
    }

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

fn ai_experimental_features_dropdown_label(
    features: &[codex_app_server_protocol::ExperimentalFeature],
) -> String {
    if features.is_empty() {
        return "Experiments 0".to_string();
    }

    let enabled_count = features.iter().filter(|feature| feature.enabled).count();
    format!("Experiments {enabled_count}/{}", features.len())
}

fn ai_sorted_experimental_feature_items(
    features: &[codex_app_server_protocol::ExperimentalFeature],
) -> Vec<(String, bool)> {
    let mut items = features
        .iter()
        .map(|feature| (feature.name.clone(), feature.enabled))
        .collect::<Vec<_>>();
    items.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
    items
}

fn ai_rate_limit_summary(
    rate_limits: Option<&codex_app_server_protocol::RateLimitSnapshot>,
    rate_limits_loading: bool,
) -> (String, String) {
    if rate_limits_loading {
        return ("5h: loading".to_string(), "weekly: loading".to_string());
    }

    let Some(snapshot) = rate_limits else {
        return ("5h: unavailable".to_string(), "weekly: unavailable".to_string());
    };

    let (five_hour_window, weekly_window) = ai_rate_limit_windows(snapshot);
    (
        ai_rate_limit_window_summary("5h", five_hour_window.as_ref()),
        ai_rate_limit_window_summary("weekly", weekly_window.as_ref()),
    )
}

fn ai_rate_limit_windows(
    snapshot: &codex_app_server_protocol::RateLimitSnapshot,
) -> (
    Option<codex_app_server_protocol::RateLimitWindow>,
    Option<codex_app_server_protocol::RateLimitWindow>,
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

fn ai_rate_limit_window_summary(
    label: &str,
    window: Option<&codex_app_server_protocol::RateLimitWindow>,
) -> String {
    let Some(window) = window else {
        return format!("{label}: unavailable");
    };

    let resets_at = window
        .resets_at
        .map(ai_format_rate_limit_reset_timestamp)
        .unwrap_or_else(|| "unknown".to_string());
    format!("{label}: {}% used, resets at {resets_at}", window.used_percent)
}

fn ai_format_rate_limit_reset_timestamp(unix_seconds: i64) -> String {
    let Ok(utc_datetime) = time::OffsetDateTime::from_unix_timestamp(unix_seconds) else {
        return unix_seconds.to_string();
    };

    match time::UtcOffset::current_local_offset() {
        Ok(offset) => {
            let local_datetime = utc_datetime.to_offset(offset);
            format!(
                "{} (Local {})",
                ai_format_human_datetime(local_datetime),
                ai_format_utc_offset(offset),
            )
        }
        Err(_) => format!("{} (UTC)", ai_format_human_datetime(utc_datetime)),
    }
}

fn ai_format_human_datetime(datetime: time::OffsetDateTime) -> String {
    let month = ai_month_short(datetime.month());
    let day = datetime.day();
    let year = datetime.year();
    let minute = datetime.minute();
    let (hour, meridiem) = ai_hour_and_meridiem(datetime.hour());
    format!("{month} {day}, {year} {hour:02}:{minute:02} {meridiem}")
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

fn ai_format_utc_offset(offset: time::UtcOffset) -> String {
    let total_seconds = offset.whole_seconds();
    let sign = if total_seconds < 0 { '-' } else { '+' };
    let absolute_seconds = total_seconds.unsigned_abs();
    let hours = absolute_seconds / 3600;
    let minutes = (absolute_seconds % 3600) / 60;
    format!("UTC{sign}{hours:02}:{minutes:02}")
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

fn ai_collaboration_picker_label(
    selected: hunk_domain::state::AiCollaborationModeSelection,
) -> String {
    selected.label().to_string()
}
