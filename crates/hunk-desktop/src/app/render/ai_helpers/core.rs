fn ai_connection_label(
    state: AiConnectionState,
    cx: &mut Context<DiffViewer>,
) -> (&'static str, Hsla) {
    match state {
        AiConnectionState::Disconnected => ("Disconnected", cx.theme().muted_foreground),
        AiConnectionState::Connecting => ("Connecting", cx.theme().warning),
        AiConnectionState::Reconnecting => ("Reconnecting", cx.theme().warning),
        AiConnectionState::Ready => ("Connected", cx.theme().success),
        AiConnectionState::Failed => ("Failed", cx.theme().danger),
    }
}

fn ai_thread_status_label(
    status: ThreadLifecycleStatus,
    cx: &mut Context<DiffViewer>,
) -> (&'static str, Hsla) {
    let label = ai_thread_status_text(status);
    let color = match label {
        "active" => cx.theme().success,
        "archived" => cx.theme().warning,
        _ => cx.theme().muted_foreground,
    };
    (label, color)
}

fn ai_thread_status_text(status: ThreadLifecycleStatus) -> &'static str {
    match status {
        ThreadLifecycleStatus::Active => "active",
        ThreadLifecycleStatus::Idle => "idle",
        ThreadLifecycleStatus::NotLoaded => "not loaded",
        ThreadLifecycleStatus::Archived => "archived",
        ThreadLifecycleStatus::Closed => "closed",
    }
}

fn ai_thread_activity_label(unix_time: i64) -> Option<String> {
    if unix_time <= 0 {
        return None;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(unix_time);
    let elapsed = now.saturating_sub(unix_time).max(0);

    Some(if elapsed < 60 {
        "now".to_string()
    } else if elapsed < 60 * 60 {
        format!("{}m", elapsed / 60)
    } else if elapsed < 60 * 60 * 24 {
        format!("{}h", elapsed / (60 * 60))
    } else {
        format!("{}d", elapsed / (60 * 60 * 24))
    })
}

fn ai_thread_display_title(thread: &hunk_codex::state::ThreadSummary) -> String {
    thread
        .title
        .as_ref()
        .map(|title| title.trim())
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "Untitled thread".to_string())
}

fn render_ai_thread_sidebar_row(
    thread: hunk_codex::state::ThreadSummary,
    selected_thread_id: Option<&str>,
    view: Entity<DiffViewer>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let thread_id = thread.id.clone();
    let title = ai_thread_display_title(&thread);
    let selected = selected_thread_id == Some(thread.id.as_str());
    let row_background = if selected {
        cx.theme().secondary_active
    } else {
        cx.theme().background.opacity(0.0)
    };
    let row_hover_background = cx.theme().secondary_hover;
    let title_color = if selected {
        cx.theme().foreground
    } else {
        hunk_opacity(cx.theme().foreground, is_dark, 0.94, 0.92)
    };
    let metadata_color = if selected {
        hunk_opacity(cx.theme().muted_foreground, is_dark, 0.94, 0.98)
    } else {
        hunk_opacity(cx.theme().muted_foreground, is_dark, 0.82, 0.92)
    };
    let time_color = if selected {
        hunk_opacity(cx.theme().foreground, is_dark, 0.56, 0.66)
    } else {
        hunk_opacity(cx.theme().muted_foreground, is_dark, 0.72, 0.82)
    };
    let (status_label, status_color) = ai_thread_status_label(thread.status, cx);
    let status_color = match thread.status {
        ThreadLifecycleStatus::Active => hunk_opacity(status_color, is_dark, 0.82, 0.72),
        ThreadLifecycleStatus::Archived => hunk_opacity(status_color, is_dark, 0.88, 0.78),
        _ => metadata_color,
    };
    let status_indicator = match thread.status {
        ThreadLifecycleStatus::Active => Some(
            div()
                .flex_none()
                .size(px(8.0))
                .rounded_full()
                .bg(cx.theme().success)
                .into_any_element(),
        ),
        ThreadLifecycleStatus::Idle | ThreadLifecycleStatus::NotLoaded => None,
        ThreadLifecycleStatus::Archived | ThreadLifecycleStatus::Closed => Some(
            div()
                .flex_none()
                .text_xs()
                .text_color(status_color)
                .child(status_label)
                .into_any_element(),
        ),
    };
    let activity_label = ai_thread_activity_label(thread.updated_at);
    let archive_button_color = if selected {
        hunk_opacity(cx.theme().foreground, is_dark, 0.70, 0.78)
    } else {
        hunk_opacity(cx.theme().muted_foreground, is_dark, 0.60, 0.72)
    };
    let select_view = view.clone();
    let archive_view = view.clone();
    let archive_thread_id = thread.id.clone();
    let archive_button_id = format!(
        "ai-thread-archive-{}",
        archive_thread_id.replace('\u{1f}', "--"),
    );

    div()
        .rounded(px(10.0))
        .bg(row_background)
        .px_2()
        .py_1p5()
        .gap_0p5()
        .hover(move |style| style.bg(row_hover_background).cursor_pointer())
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            select_view.update(cx, |this, cx| {
                this.ai_select_thread(thread_id.clone(), window, cx);
            });
        })
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_medium()
                        .text_color(title_color)
                        .truncate()
                        .child(title),
                )
                .when_some(activity_label, |this, label| {
                    this.child(
                        div()
                            .flex_none()
                            .text_xs()
                            .font_medium()
                            .text_color(time_color)
                            .child(label),
                    )
                })
                .child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .gap_1()
                        .when_some(status_indicator, |this, indicator| this.child(indicator))
                        .child(
                            div()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .child({
                                    let view = archive_view.clone();
                                    Button::new(archive_button_id)
                                        .ghost()
                                        .compact()
                                        .rounded(px(7.0))
                                        .icon(Icon::new(IconName::Inbox).size(px(12.0)))
                                        .text_color(archive_button_color)
                                        .min_w(px(22.0))
                                        .h(px(20.0))
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
                ),
        )
        .into_any_element()
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

fn ai_item_display_label(kind: &str) -> &str {
    match kind {
        "userMessage" => "User",
        "agentMessage" => "Agent",
        "commandExecution" => "Command",
        "fileChange" => "File Change",
        "plan" => "Plan",
        "reasoning" => "Reasoning",
        "mcpToolCall" => "MCP Tool Call",
        "dynamicToolCall" => "Tool Call",
        "collabAgentToolCall" => "Collab Tool Call",
        "webSearch" => "Web Search",
        "imageView" => "Image View",
        "enteredReviewMode" => "Review Mode Entered",
        "exitedReviewMode" => "Review Mode Exited",
        "contextCompaction" => "Context Compaction",
        _ => kind,
    }
}

fn ai_truncate_multiline_content(content: &str, max_lines: usize) -> (String, bool) {
    if max_lines == 0 {
        return (String::new(), !content.is_empty());
    }

    let lines = content.lines().collect::<Vec<_>>();
    if lines.len() <= max_lines {
        return (content.to_string(), false);
    }

    let mut truncated = lines
        .into_iter()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n");
    truncated.push_str("\n...");
    (truncated, true)
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
    selected_collaboration_mode: hunk_domain::state::AiCollaborationModeSelection,
    selected_service_tier: AiServiceTierSelection,
}

fn render_ai_session_controls_panel_for_view(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    render_ai_session_controls_panel(
        AiSessionControlsPanelView {
            models: this.ai_models.as_slice(),
            experimental_features: this.ai_experimental_features.as_slice(),
            selected_model: this.ai_selected_model.as_deref(),
            selected_effort: this.ai_selected_effort.as_deref(),
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
    let collaboration_label = ai_collaboration_picker_label(panel.selected_collaboration_mode);
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
                                    .checked(
                                        selected_model.as_deref()
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
                                    selected_effort.as_deref() == Some(effort_value.as_str()),
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
                .label(service_tier_label)
                .dropdown_menu(move |menu, _, _| {
                    menu.item(
                        PopupMenuItem::new("Standard")
                            .checked(matches!(
                                selected_service_tier,
                                AiServiceTierSelection::Standard
                            ))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_select_service_tier_action(
                                            AiServiceTierSelection::Standard,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Fast")
                            .checked(matches!(
                                selected_service_tier,
                                AiServiceTierSelection::Fast
                            ))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_select_service_tier_action(
                                            AiServiceTierSelection::Fast,
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
                    .label(collaboration_label)
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

fn ai_service_tier_picker_label(selected: AiServiceTierSelection) -> String {
    match selected {
        AiServiceTierSelection::Standard => "Standard".to_string(),
        AiServiceTierSelection::Fast => "Fast".to_string(),
        AiServiceTierSelection::Flex => "Flex".to_string(),
    }
}
