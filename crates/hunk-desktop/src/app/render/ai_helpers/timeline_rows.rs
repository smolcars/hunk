use gpui::{
    FontStyle, FontWeight, HighlightStyle, StrikethroughStyle, StyledText, UnderlineStyle,
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiTimelineItemRole {
    User,
    Assistant,
    Tool,
}

const AI_TIMELINE_CONTENT_LANE_MAX_WIDTH: f32 = 960.0;

struct AiCommandExecutionDisplayDetails {
    command: String,
    cwd: String,
    process_id: Option<String>,
    status: String,
    action_summaries: Vec<String>,
    exit_code: Option<i32>,
    duration_ms: Option<i64>,
}

fn ai_timeline_item_role(kind: &str) -> AiTimelineItemRole {
    match kind {
        "userMessage" => AiTimelineItemRole::User,
        "agentMessage" | "plan" => AiTimelineItemRole::Assistant,
        _ => AiTimelineItemRole::Tool,
    }
}

fn ai_timeline_item_is_renderable(item: &hunk_codex::state::ItemSummary) -> bool {
    if matches!(item.kind.as_str(), "reasoning" | "webSearch") {
        let has_content = !item.content.trim().is_empty();
        let has_metadata = item.display_metadata.as_ref().is_some_and(|metadata| {
            metadata
                .summary
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                || metadata
                    .details_json
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
        });
        return has_content || has_metadata;
    }

    true
}

fn ai_timeline_row_is_renderable(this: &DiffViewer, row: &AiTimelineRow) -> bool {
    match &row.source {
        AiTimelineRowSource::Item { item_key } => this
            .ai_state_snapshot
            .items
            .get(item_key.as_str())
            .is_some_and(ai_timeline_item_is_renderable),
        AiTimelineRowSource::Group { group_id } => this
            .ai_timeline_group(group_id.as_str())
            .is_some_and(|group| !group.child_row_ids.is_empty()),
        AiTimelineRowSource::TurnDiff { turn_key } => this
            .ai_state_snapshot
            .turn_diffs
            .get(turn_key.as_str())
            .is_some_and(|diff| !diff.trim().is_empty()),
    }
}

fn ai_timeline_item_details_json(item: &hunk_codex::state::ItemSummary) -> Option<&str> {
    item.display_metadata
        .as_ref()
        .and_then(|metadata| metadata.details_json.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn ai_command_execution_display_details(
    item: &hunk_codex::state::ItemSummary,
) -> Option<AiCommandExecutionDisplayDetails> {
    let details_json = ai_timeline_item_details_json(item)?;
    let details = serde_json::from_str::<serde_json::Value>(details_json).ok()?;
    let object = details.as_object()?;
    if object.get("kind").and_then(|value| value.as_str()) != Some("commandExecution") {
        return None;
    }

    Some(AiCommandExecutionDisplayDetails {
        command: object
            .get("command")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        cwd: object
            .get("cwd")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        process_id: object
            .get("processId")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        status: object
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        action_summaries: object
            .get("actionSummaries")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        exit_code: object
            .get("exitCode")
            .and_then(|value| value.as_i64())
            .and_then(|value| i32::try_from(value).ok()),
        duration_ms: object.get("durationMs").and_then(|value| value.as_i64()),
    })
}

fn ai_tool_compact_preview_text(
    item: &hunk_codex::state::ItemSummary,
    content_text: &str,
) -> Option<String> {
    if let Some(details) = ai_command_execution_display_details(item) {
        return Some(details.command);
    }

    let details_json = ai_timeline_item_details_json(item)?;
    let thread_item =
        serde_json::from_str::<codex_app_server_protocol::ThreadItem>(details_json).ok();
    match thread_item {
        Some(codex_app_server_protocol::ThreadItem::FileChange { changes, .. }) => {
            let first_path = changes.first()?.path.clone();
            if changes.len() == 1 {
                Some(first_path)
            } else {
                Some(format!("{first_path} (+{} more files)", changes.len() - 1))
            }
        }
        Some(codex_app_server_protocol::ThreadItem::McpToolCall { server, tool, .. }) => {
            Some(format!("{server} :: {tool}"))
        }
        Some(codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. }) => Some(tool),
        Some(codex_app_server_protocol::ThreadItem::CollabAgentToolCall {
            tool,
            receiver_thread_ids,
            ..
        }) => {
            let receiver_summary = match receiver_thread_ids.len() {
                0 => "no targets".to_string(),
                1 => receiver_thread_ids[0].clone(),
                count => format!("{count} targets"),
            };
            Some(format!("{tool:?} -> {receiver_summary}"))
        }
        _ => content_text
            .lines()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    }
}

fn ai_tool_summary_is_placeholder(summary: &str) -> bool {
    let trimmed = summary.trim();
    trimmed.is_empty() || !trimmed.chars().any(|ch| ch.is_alphanumeric())
}

fn ai_tool_header_title(item: &hunk_codex::state::ItemSummary) -> String {
    item.display_metadata
        .as_ref()
        .and_then(|metadata| metadata.summary.as_deref())
        .filter(|value| !ai_tool_summary_is_placeholder(value))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| ai_item_display_label(item.kind.as_str()).to_string())
}

fn ai_tool_compact_summary(
    item: &hunk_codex::state::ItemSummary,
    content_text: &str,
) -> Option<String> {
    let summary = ai_tool_compact_preview_text(item, content_text)?;
    let summary = summary.trim();
    if summary.is_empty() {
        return None;
    }

    let title = ai_tool_header_title(item);
    (summary != title).then(|| summary.to_string())
}

#[cfg(test)]
fn ai_tool_header_label(item: &hunk_codex::state::ItemSummary, content_text: &str) -> String {
    let title = ai_tool_header_title(item);
    if title != ai_item_display_label(item.kind.as_str()) {
        return title;
    }

    if let Some(preview_line) = ai_tool_compact_summary(item, content_text) {
        return preview_line;
    }

    title
}

fn ai_duration_ms_label(duration_ms: Option<i64>) -> Option<String> {
    let duration_ms = duration_ms?;
    let millis = u64::try_from(duration_ms).ok()?;
    Some(ai_activity_elapsed_label(std::time::Duration::from_millis(
        millis,
    )))
}

fn ai_tool_meta_chip(
    label: &str,
    value: String,
    mono: bool,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    h_flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded(px(6.0))
        .border_1()
        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.84, 0.68))
        .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.10, 0.16))
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child({
            let value = if value.trim().is_empty() {
                "none".to_string()
            } else {
                value
            };
            let mut element = div()
                .min_w_0()
                .text_xs()
                .text_color(cx.theme().foreground)
                .child(value);
            if mono {
                element = element.font_family(cx.theme().mono_font_family.clone());
            }
            element
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn ai_tool_detail_section(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    surface_id: impl Into<String>,
    selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
    title: &str,
    content: String,
    mono: bool,
    max_height: Option<gpui::Pixels>,
    scroll_x: bool,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let surface_id = surface_id.into();
    let container = div()
        .w_full()
        .min_w_0()
        .rounded(px(8.0))
        .border_1()
        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.85, 0.68))
        .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.10, 0.14))
        .overflow_hidden()
        .px_2()
        .py_1p5();

    let mut text = div()
        .w_full()
        .min_w_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .whitespace_normal();
    if mono {
        text = text.font_family(cx.theme().mono_font_family.clone());
    }
    if scroll_x {
        text = text.whitespace_nowrap();
    }
    text = text.child(
        div()
            .w_full()
            .min_w_0()
            .child(ai_render_selectable_styled_text(
                this,
                view,
                row_id,
                surface_id,
                selection_surfaces,
                StyledText::new(content),
                is_dark,
                cx,
            )),
    );

    let content = match (max_height, scroll_x) {
        (Some(max_height), true) => div()
            .w_full()
            .min_w_0()
            .max_h(max_height)
            .overflow_scrollbar()
            .occlude()
            .child(text)
            .into_any_element(),
        (Some(max_height), false) => div()
            .w_full()
            .min_w_0()
            .max_h(max_height)
            .overflow_y_scrollbar()
            .occlude()
            .child(text)
            .into_any_element(),
        (None, true) => div()
            .w_full()
            .min_w_0()
            .overflow_x_scrollbar()
            .child(text)
            .into_any_element(),
        (None, false) => text.into_any_element(),
    };
    let container = container.child(content);

    v_flex()
        .w_full()
        .min_w_0()
        .items_stretch()
        .gap_1()
        .child(
            div()
                .w_full()
                .min_w_0()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .whitespace_nowrap()
                .child(title.to_string()),
        )
        .child(container)
        .into_any_element()
}

fn render_ai_command_execution_details(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    details: &AiCommandExecutionDisplayDetails,
    output: &str,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let mut chips = Vec::new();
    chips.push(ai_tool_meta_chip("status", details.status.clone(), false, is_dark, cx));
    chips.push(ai_tool_meta_chip("cwd", details.cwd.clone(), true, is_dark, cx));
    if let Some(process_id) = details.process_id.as_ref() {
        chips.push(ai_tool_meta_chip("pid", process_id.clone(), true, is_dark, cx));
    }
    if let Some(exit_code) = details.exit_code {
        chips.push(ai_tool_meta_chip("exit", exit_code.to_string(), true, is_dark, cx));
    }
    if let Some(duration) = ai_duration_ms_label(details.duration_ms) {
        chips.push(ai_tool_meta_chip("duration", duration, false, is_dark, cx));
    }

    let command_surface_id = ai_timeline_text_surface_id(row_id, "tool-command", 0);
    let action_surface_id = ai_timeline_text_surface_id(row_id, "tool-actions", 0);
    let output_surface_id = ai_timeline_text_surface_id(row_id, "tool-output", 0);
    let mut selection_surfaces = vec![AiTextSelectionSurfaceSpec::new(
        command_surface_id.clone(),
        details.command.clone(),
    )];
    if !details.action_summaries.is_empty() {
        selection_surfaces.push(
            AiTextSelectionSurfaceSpec::new(
                action_surface_id.clone(),
                details.action_summaries.join("\n"),
            )
            .with_separator_before("\n\n"),
        );
    }
    let trimmed_output = output.trim().to_string();
    if !trimmed_output.is_empty() {
        selection_surfaces.push(
            AiTextSelectionSurfaceSpec::new(output_surface_id.clone(), trimmed_output.clone())
                .with_separator_before("\n\n"),
        );
    }
    let selection_surfaces = ai_text_selection_surfaces(selection_surfaces);

    let mut sections = vec![ai_tool_detail_section(
        this,
        view.clone(),
        row_id,
        command_surface_id,
        selection_surfaces.clone(),
        "Command",
        details.command.clone(),
        true,
        None,
        true,
        is_dark,
        cx,
    )];
    if !details.action_summaries.is_empty() {
        sections.push(ai_tool_detail_section(
            this,
            view.clone(),
            row_id,
            action_surface_id,
            selection_surfaces.clone(),
            "Actions",
            details.action_summaries.join("\n"),
            false,
            Some(px(140.0)),
            false,
            is_dark,
            cx,
        ));
    }
    if !trimmed_output.is_empty() {
        sections.push(ai_tool_detail_section(
            this,
            view.clone(),
            row_id,
            output_surface_id,
            selection_surfaces.clone(),
            "Output",
            trimmed_output,
            true,
            Some(px(220.0)),
            false,
            is_dark,
            cx,
        ));
    }

    v_flex()
        .w_full()
        .min_w_0()
        .items_stretch()
        .gap_1p5()
        .when(!chips.is_empty(), |this| {
            this.child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .flex_wrap()
                    .children(chips),
            )
        })
        .children(sections)
        .into_any_element()
}

fn render_ai_chat_timeline_row_for_view(
    this: &DiffViewer,
    row_id: &str,
    view: Entity<DiffViewer>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    if let Some(pending) = this.ai_pending_steer_for_row_id(row_id) {
        return render_ai_pending_steer(&pending, is_dark, cx);
    }
    if let Some(queued) = this.ai_queued_message_for_row_id(row_id) {
        return render_ai_queued_message(&queued, is_dark, cx);
    }

    let Some(row) = this.ai_timeline_row(row_id) else {
        return div().w_full().h(px(0.0)).into_any_element();
    };
    if !ai_timeline_row_is_renderable(this, row) {
        return div().w_full().h(px(0.0)).into_any_element();
    }

    match &row.source {
        AiTimelineRowSource::Item { item_key } => {
            let Some(item) = this.ai_state_snapshot.items.get(item_key.as_str()) else {
                return div().w_full().h(px(0.0)).into_any_element();
            };
            let role = ai_timeline_item_role(item.kind.as_str());
            match role {
                AiTimelineItemRole::User | AiTimelineItemRole::Assistant => {
                    let is_user = role == AiTimelineItemRole::User;
                    let role_label = if is_user {
                        "You"
                    } else if item.kind == "plan" {
                        "Plan"
                    } else {
                        "Assistant"
                    };
                    let bubble_max_width = if is_user { px(680.0) } else { px(700.0) };
                    let text_content = item.content.trim();
                    let fallback_summary = item
                        .display_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.summary.as_deref())
                        .unwrap_or_default();
                    let bubble_text = if text_content.is_empty() {
                        fallback_summary
                    } else {
                        text_content
                    };
                    let message_hover_group =
                        format!("ai-message-hover-{}", row.id.replace('\u{1f}', "--"));
                    let copy_message_id =
                        format!("ai-copy-message-{}", row.id.replace('\u{1f}', "--"));
                    let copy_message_text = bubble_text.to_string();

                    let row_element = h_flex()
                        .w_full()
                        .min_w_0()
                        .when(is_user, |this| this.justify_end())
                        .when(!is_user, |this| this.justify_start())
                        .child(
                            v_flex()
                                .group(message_hover_group.clone())
                                .w_full()
                                .max_w(bubble_max_width)
                                .min_w_0()
                                .gap_1p5()
                                .child(
                                    h_flex()
                                        .w_full()
                                        .min_w_0()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            div()
                                                .flex_none()
                                                .whitespace_nowrap()
                                                .text_xs()
                                                .font_semibold()
                                                .child(role_label),
                                        )
                                        .when(!bubble_text.is_empty(), |header| {
                                            let hover_group = message_hover_group.clone();
                                            let view = view.clone();
                                            let message = copy_message_text.clone();
                                            header.child(
                                                div()
                                                    .flex_none()
                                                    .invisible()
                                                    .group_hover(hover_group, |this| this.visible())
                                                    .child(
                                                        Button::new(copy_message_id.clone())
                                                            .ghost()
                                                            .compact()
                                                            .rounded(px(7.0))
                                                            .icon(Icon::new(IconName::Copy).size(px(12.0)))
                                                            .text_color(cx.theme().muted_foreground)
                                                            .min_w(px(22.0))
                                                            .h(px(20.0))
                                                            .tooltip("Copy message")
                                                            .on_click(move |_, window, cx| {
                                                                view.update(cx, |this, cx| {
                                                                    this.ai_copy_message_action(
                                                                        message.clone(),
                                                                        window,
                                                                        cx,
                                                                    );
                                                                });
                                                            }),
                                                    ),
                                            )
                                        })
                                )
                                .when(!bubble_text.is_empty(), |container| {
                                    container.child(ai_render_chat_markdown_message(
                                        this,
                                        view.clone(),
                                        row.id.as_str(),
                                        bubble_text,
                                        is_dark,
                                        cx,
                                    ))
                                }),
                        );
                    ai_timeline_row_with_animation(this, row.id.as_str(), row_element)
                }
                AiTimelineItemRole::Tool => {
                    render_ai_tool_item_row(
                        this,
                        view,
                        row.id.as_str(),
                        item,
                        is_dark,
                        false,
                        cx,
                    )
                }
            }
        }
        AiTimelineRowSource::Group { group_id } => {
            let Some(group) = this.ai_timeline_group(group_id.as_str()) else {
                return div().w_full().h(px(0.0)).into_any_element();
            };
            render_ai_timeline_group_row(this, view, row, group, is_dark, cx)
        }
        AiTimelineRowSource::TurnDiff { turn_key } => {
            let Some(diff) = this.ai_state_snapshot.turn_diffs.get(turn_key.as_str()) else {
                return div().w_full().h(px(0.0)).into_any_element();
            };
            let diff_text = diff.trim();
            if diff_text.is_empty() {
                return div().w_full().h(px(0.0)).into_any_element();
            }
            let diff_line_count = diff_text.lines().count();
            let expanded = this.ai_expanded_timeline_row_ids.contains(row.id.as_str());
            let (preview, preview_truncated) = if expanded {
                (diff_text.to_string(), false)
            } else {
                ai_truncate_multiline_content(diff_text, 10)
            };
            let show_toggle = preview_truncated || expanded;
            let view_diff_button_id =
                format!("ai-open-review-tab-{}", row.turn_id.replace('\u{1f}', "--"));
            let toggle_id = format!("ai-toggle-diff-row-{}", row.id.replace('\u{1f}', "--"));

            let row_element = h_flex()
                .w_full()
                .min_w_0()
                .justify_start()
                .child(
                    v_flex()
                        .max_w(px(920.0))
                        .w_full()
                        .min_w_0()
                        .gap_1()
                        .px_2p5()
                        .py_2()
                        .overflow_hidden()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.9, 0.74))
                        .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.16, 0.22))
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .items_start()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_xs()
                                        .font_semibold()
                                        .whitespace_nowrap()
                                        .truncate()
                                        .child(format!("Code Diff ({diff_line_count} lines)")),
                                )
                                .child(
                                    h_flex()
                                        .items_center()
                                        .gap_1()
                                        .when(show_toggle, |this| {
                                            let row_id = row.id.clone();
                                            let view = view.clone();
                                            this.child(
                                                Button::new(toggle_id)
                                                    .compact()
                                                    .outline()
                                                    .with_size(gpui_component::Size::Small)
                                                    .icon(
                                                        Icon::new(if expanded {
                                                            IconName::ChevronDown
                                                        } else {
                                                            IconName::ChevronRight
                                                        })
                                                        .size(px(12.0)),
                                                    )
                                                    .tooltip(if expanded {
                                                        "Collapse diff preview"
                                                    } else {
                                                        "Expand diff preview"
                                                    })
                                                    .on_click(move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.ai_toggle_timeline_row_expansion_action(
                                                                row_id.clone(),
                                                                cx,
                                                            );
                                                        });
                                                    }),
                                            )
                                        })
                                        .child({
                                            let view = view.clone();
                                            Button::new(view_diff_button_id)
                                                .compact()
                                                .outline()
                                                .with_size(gpui_component::Size::Small)
                                                .label("View Diff")
                                                .on_click(move |_, _, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.ai_open_review_tab(cx);
                                                    });
                                                })
                                        }),
                                ),
                        )
                        .when(!preview.is_empty(), |container| {
                            let preview_surface_id =
                                ai_timeline_text_surface_id(row.id.as_str(), "diff-preview", 0);
                            let preview_selection_surfaces = ai_text_selection_surfaces(vec![
                                AiTextSelectionSurfaceSpec::new(
                                    preview_surface_id.clone(),
                                    preview.clone(),
                                ),
                            ]);
                            container.child(ai_tool_detail_section(
                                this,
                                view.clone(),
                                row.id.as_str(),
                                preview_surface_id,
                                preview_selection_surfaces,
                                "Preview",
                                preview.clone(),
                                true,
                                expanded.then_some(px(280.0)),
                                false,
                                is_dark,
                                cx,
                            ))
                        }),
                );
            ai_timeline_row_with_animation(this, row.id.as_str(), row_element)
        }
    }
}

fn ai_timeline_row_with_animation(
    this: &DiffViewer,
    row_id: &str,
    row: gpui::Div,
) -> AnyElement {
    let row = h_flex()
        .w_full()
        .min_w_0()
        .justify_center()
        .child(
            div()
                .w_full()
                .max_w(px(AI_TIMELINE_CONTENT_LANE_MAX_WIDTH))
                .min_w_0()
                .px_1()
                .py_1p5()
                .child(row),
        );
    if this.reduced_motion_enabled() {
        row.into_any_element()
    } else {
        row.with_animation(
            row_id.to_string(),
            Animation::new(this.animation_duration_ms(170))
                .with_easing(cubic_bezier(0.32, 0.72, 0.0, 1.0)),
            |this, delta| {
                let entering = 1.0 - delta;
                this.top(px(entering * 7.0)).opacity(0.76 + (0.24 * delta))
            },
        )
        .into_any_element()
    }
}
