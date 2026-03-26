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

fn ai_normalized_thread_title(title: &str) -> Option<String> {
    let mut words = title.split_whitespace();
    let first = words.next()?;
    let mut normalized = String::from(first);
    for word in words {
        normalized.push(' ');
        normalized.push_str(word);
    }
    Some(normalized)
}

fn ai_thread_display_title(thread: &hunk_codex::state::ThreadSummary) -> String {
    thread
        .title
        .as_deref()
        .and_then(ai_normalized_thread_title)
        .unwrap_or_else(|| "Untitled thread".to_string())
}

fn render_ai_thread_sidebar_row(
    thread: hunk_codex::state::ThreadSummary,
    workspace_label: String,
    selected_thread_id: Option<&str>,
    bookmarked: bool,
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
    let bookmark_button_color = if bookmarked {
        hunk_opacity(cx.theme().warning, is_dark, 0.92, 0.82)
    } else if selected {
        hunk_opacity(cx.theme().foreground, is_dark, 0.70, 0.78)
    } else {
        hunk_opacity(cx.theme().muted_foreground, is_dark, 0.60, 0.72)
    };
    let archive_button_color = if selected {
        hunk_opacity(cx.theme().foreground, is_dark, 0.70, 0.78)
    } else {
        hunk_opacity(cx.theme().muted_foreground, is_dark, 0.60, 0.72)
    };
    let archive_action_available = !matches!(
        thread.status,
        ThreadLifecycleStatus::Archived | ThreadLifecycleStatus::Closed
    );
    let select_view = view.clone();
    let bookmark_view = view.clone();
    let archive_view = view.clone();
    let bookmark_thread_id = thread.id.clone();
    let bookmark_button_id = format!(
        "ai-thread-bookmark-{}",
        bookmark_thread_id.replace('\u{1f}', "--"),
    );
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
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .w_full()
                                .text_sm()
                                .font_medium()
                                .text_color(title_color)
                                .whitespace_nowrap()
                                .truncate()
                                .child(title),
                        )
                        .child(
                            div()
                                .w_full()
                                .text_xs()
                                .text_color(metadata_color)
                                .whitespace_nowrap()
                                .truncate()
                                .child(workspace_label),
                        ),
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
                        .child(
                            div()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .child({
                                    let view = bookmark_view.clone();
                                    Button::new(bookmark_button_id)
                                        .ghost()
                                        .compact()
                                        .rounded(px(7.0))
                                        .icon(
                                            Icon::new(IconName::Star)
                                                .size(px(12.0))
                                                .text_color(bookmark_button_color),
                                        )
                                        .min_w(px(22.0))
                                        .h(px(20.0))
                                        .tooltip(if bookmarked {
                                            "Remove bookmark"
                                        } else {
                                            "Bookmark thread"
                                        })
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_toggle_thread_bookmark(
                                                    bookmark_thread_id.clone(),
                                                    cx,
                                                );
                                            });
                                        })
                                }),
                        )
                        .when_some(status_indicator, |this, indicator| this.child(indicator))
                        .when(archive_action_available, |this| {
                            this.child(
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
                            )
                        }),
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

fn ai_item_status_color(status: ItemStatus, theme: &gpui_component::Theme) -> Hsla {
    match status {
        ItemStatus::Started => theme.muted_foreground,
        ItemStatus::Streaming => theme.accent,
        ItemStatus::Completed => theme.success,
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

#[cfg(test)]
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
