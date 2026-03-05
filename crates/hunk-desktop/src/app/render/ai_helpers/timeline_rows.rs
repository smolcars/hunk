use gpui::{
    FontStyle, FontWeight, HighlightStyle, StrikethroughStyle, StyledText, UnderlineStyle,
};

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

fn ai_tool_preview_text(
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
            let mut preview = changes
                .iter()
                .take(3)
                .map(|change| change.path.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if changes.len() > 3 {
                if !preview.is_empty() {
                    preview.push('\n');
                }
                preview.push_str(&format!("+{} more files", changes.len() - 3));
            }
            (!preview.trim().is_empty()).then_some(preview)
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
            let receivers = if receiver_thread_ids.is_empty() {
                "no targets".to_string()
            } else {
                receiver_thread_ids.join(", ")
            };
            Some(format!("{tool:?} -> {receivers}"))
        }
        _ => (!content_text.is_empty()).then(|| content_text.to_string()),
    }
}

fn ai_tool_summary_is_placeholder(summary: &str) -> bool {
    let trimmed = summary.trim();
    trimmed.is_empty() || !trimmed.chars().any(|ch| ch.is_alphanumeric())
}

fn ai_tool_header_label(item: &hunk_codex::state::ItemSummary, content_text: &str) -> String {
    if let Some(summary) = item
        .display_metadata
        .as_ref()
        .and_then(|metadata| metadata.summary.as_deref())
        .filter(|value| !ai_tool_summary_is_placeholder(value))
    {
        return summary.to_string();
    }

    if let Some(preview_line) = ai_tool_preview_text(item, content_text)
        .and_then(|preview| {
            preview
                .lines()
                .map(str::trim)
                .find(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
    {
        return preview_line;
    }

    ai_item_display_label(item.kind.as_str()).to_string()
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
        .border_color(cx.theme().border.opacity(if is_dark { 0.84 } else { 0.68 }))
        .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
            0.10
        } else {
            0.16
        })))
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

fn ai_tool_detail_section(
    title: &str,
    content: String,
    mono: bool,
    max_height: Option<gpui::Pixels>,
    scroll_x: bool,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let container = div()
        .w_full()
        .min_w_0()
        .rounded(px(8.0))
        .border_1()
        .border_color(cx.theme().border.opacity(if is_dark { 0.85 } else { 0.68 }))
        .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
            0.10
        } else {
            0.14
        })))
        .overflow_hidden()
        .px_2()
        .py_1p5();

    let mut text = div()
        .w_full()
        .min_w_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .whitespace_normal()
        .child(content);
    if mono {
        text = text.font_family(cx.theme().mono_font_family.clone());
    }
    if scroll_x {
        text = text.whitespace_nowrap();
    }

    let container = container.child(text);
    let container = match (max_height, scroll_x) {
        (Some(max_height), true) => container
            .max_h(max_height)
            .overflow_scrollbar()
            .occlude()
            .into_any_element(),
        (Some(max_height), false) => container
            .max_h(max_height)
            .overflow_y_scrollbar()
            .occlude()
            .into_any_element(),
        (None, true) => container.overflow_x_scrollbar().into_any_element(),
        (None, false) => container.into_any_element(),
    };

    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(title.to_string()),
        )
        .child(container)
        .into_any_element()
}

fn render_ai_command_execution_details(
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

    let mut sections = vec![ai_tool_detail_section(
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
            "Actions",
            details.action_summaries.join("\n"),
            false,
            Some(px(140.0)),
            false,
            is_dark,
            cx,
        ));
    }
    if !output.trim().is_empty() {
        sections.push(ai_tool_detail_section(
            "Output",
            output.trim().to_string(),
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

fn ai_render_chat_markdown_message(
    this: &DiffViewer,
    markdown: &str,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let blocks = hunk_domain::markdown_preview::parse_markdown_preview(markdown);
    if blocks.is_empty() {
        return div().w_full().text_sm().child("").into_any_element();
    }

    v_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .children(
            blocks
                .iter()
                .map(|block| ai_render_chat_markdown_block(this, block, is_dark, cx)),
        )
        .into_any_element()
}

fn ai_render_chat_markdown_block(
    this: &DiffViewer,
    block: &MarkdownPreviewBlock,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    match block {
        MarkdownPreviewBlock::Heading { level, spans } => ai_render_chat_inline_spans(
            spans,
            matches!(level, 1 | 2),
            true,
            cx.theme().foreground,
            is_dark,
            cx,
        ),
        MarkdownPreviewBlock::Paragraph(spans) => ai_render_chat_inline_spans(
            spans,
            false,
            false,
            cx.theme().foreground,
            is_dark,
            cx,
        ),
        MarkdownPreviewBlock::UnorderedListItem(spans) => h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("-"),
            )
            .child(
                div().flex_1().min_w_0().child(ai_render_chat_inline_spans(
                    spans,
                    false,
                    false,
                    cx.theme().foreground,
                    is_dark,
                    cx,
                )),
            )
            .into_any_element(),
        MarkdownPreviewBlock::OrderedListItem { number, spans } => h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{number}.")),
            )
            .child(
                div().flex_1().min_w_0().child(ai_render_chat_inline_spans(
                    spans,
                    false,
                    false,
                    cx.theme().foreground,
                    is_dark,
                    cx,
                )),
            )
            .into_any_element(),
        MarkdownPreviewBlock::BlockQuote(spans) => h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("|"),
            )
            .child(
                div().flex_1().min_w_0().child(ai_render_chat_inline_spans(
                    spans,
                    false,
                    false,
                    cx.theme().muted_foreground,
                    is_dark,
                    cx,
                )),
            )
            .into_any_element(),
        MarkdownPreviewBlock::CodeBlock { language, lines } => {
            let language_label = language.clone().unwrap_or_else(|| "code".to_string());
            let code_rows = if lines.is_empty() {
                vec![
                    div()
                        .w_full()
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .child("")
                        .into_any_element(),
                ]
            } else {
                lines
                    .iter()
                    .map(|line_spans| {
                        h_flex()
                            .w_full()
                            .items_start()
                            .gap_0()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .flex_wrap()
                            .whitespace_normal()
                            .children(line_spans.iter().map(|span| {
                                let token_color = this.markdown_code_token_color(
                                    cx.theme().foreground,
                                    span.token,
                                    cx,
                                );
                                div()
                                    .flex_none()
                                    .whitespace_nowrap()
                                    .text_color(token_color)
                                    .child(span.text.clone())
                                    .into_any_element()
                            }))
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            };

            v_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .child(language_label),
                )
                .child(
                    div()
                        .min_w_0()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(cx.theme().border.opacity(if is_dark { 0.88 } else { 0.74 }))
                        .bg(cx.theme().secondary.opacity(if is_dark { 0.30 } else { 0.44 }))
                        .p_2()
                        .child(v_flex().min_w_0().children(code_rows)),
                )
                .into_any_element()
        }
        MarkdownPreviewBlock::ThematicBreak => div()
            .h(px(1.0))
            .w_full()
            .bg(cx.theme().border.opacity(if is_dark { 0.8 } else { 0.95 }))
            .into_any_element(),
    }
}

fn ai_chat_markdown_text_and_highlights(
    spans: &[MarkdownInlineSpan],
    base_color: Hsla,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> (SharedString, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let text = ai_chat_markdown_text(spans);
    let mut highlights = Vec::new();
    let link_color = cx.theme().primary;
    let code_background = cx.theme().secondary.opacity(if is_dark { 0.30 } else { 0.42 });
    let mut cursor = 0;

    for span in spans {
        if span.style.hard_break {
            if !text[..cursor].ends_with('\n') {
                cursor += 1;
            }
            continue;
        }
        if span.text.is_empty() {
            continue;
        }

        let start = cursor;
        let end = start + span.text.len();
        cursor = end;

        let mut highlight = HighlightStyle::default();
        if span.style.link.is_some() {
            highlight.color = Some(link_color);
            highlight.font_weight = Some(FontWeight::SEMIBOLD);
            highlight.underline = Some(UnderlineStyle {
                thickness: px(1.0),
                color: Some(link_color),
                wavy: false,
            });
        }
        if span.style.bold {
            highlight.font_weight = Some(FontWeight::SEMIBOLD);
        }
        if span.style.italic {
            highlight.font_style = Some(FontStyle::Italic);
        }
        if span.style.code {
            highlight.background_color = Some(code_background);
            highlight.color.get_or_insert(base_color);
            highlight.font_weight.get_or_insert(FontWeight::MEDIUM);
        }
        if span.style.strikethrough {
            highlight.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.0),
                color: Some(highlight.color.unwrap_or(base_color)),
            });
        }

        if highlight != HighlightStyle::default() {
            highlights.push((start..end, highlight));
        }
    }

    (text.into(), highlights)
}

fn ai_chat_markdown_text(spans: &[MarkdownInlineSpan]) -> String {
    let mut text = String::new();
    for span in spans {
        if span.style.hard_break {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            continue;
        }
        text.push_str(span.text.as_str());
    }
    text
}

fn ai_render_chat_inline_spans(
    spans: &[MarkdownInlineSpan],
    large: bool,
    emphasized: bool,
    base_color: Hsla,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let (text, highlights) = ai_chat_markdown_text_and_highlights(spans, base_color, is_dark, cx);
    if text.is_empty() {
        return div().w_full().text_sm().child("").into_any_element();
    }

    let styled_text = if highlights.is_empty() {
        StyledText::new(text.clone())
    } else {
        StyledText::new(text.clone()).with_highlights(highlights)
    };

    let mut row = div()
        .min_w_0()
        .w_full()
        .text_color(base_color)
        .child(styled_text);

    if large {
        row = row.text_lg();
    } else {
        row = row.text_sm();
    }
    if emphasized {
        row = row.font_semibold();
    }

    row.into_any_element()
}

fn render_ai_chat_timeline_row_for_view(
    this: &DiffViewer,
    row_id: &str,
    view: Entity<DiffViewer>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
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

                    let row_element = h_flex()
                        .w_full()
                        .min_w_0()
                        .when(is_user, |this| this.justify_end())
                        .when(!is_user, |this| this.justify_start())
                        .child(
                            v_flex()
                                .w_full()
                                .max_w(bubble_max_width)
                                .min_w_0()
                                .gap_1p5()
                                .when(is_user, |this| {
                                    this.px_3()
                                        .py_2()
                                        .overflow_hidden()
                                        .rounded(px(12.0))
                                        .border_1()
                                        .border_color(cx.theme().accent.opacity(if is_dark {
                                            0.72
                                        } else {
                                            0.48
                                        }))
                                        .bg(cx.theme().accent.opacity(if is_dark {
                                            0.18
                                        } else {
                                            0.12
                                        }))
                                })
                                .child(
                                    h_flex()
                                        .w_full()
                                        .min_w_0()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .flex_none()
                                                .whitespace_nowrap()
                                                .text_xs()
                                                .font_semibold()
                                                .child(role_label),
                                        )
                                )
                                .when(!bubble_text.is_empty(), |container| {
                                    container.child(ai_render_chat_markdown_message(
                                        this, bubble_text, is_dark, cx,
                                    ))
                                }),
                        );
                    ai_timeline_row_with_animation(this, row.id.as_str(), row_element)
                }
                AiTimelineItemRole::Tool => {
                    let content_text = item.content.trim();
                    let label = ai_tool_header_label(item, content_text);
                    let status = ai_item_status_label(item.status);
                    let status_color = ai_item_status_color(item.status, cx);
                    let command_details = (item.kind == "commandExecution")
                        .then(|| ai_command_execution_display_details(item))
                        .flatten();
                    let details_text = if item.kind == "commandExecution" {
                        content_text
                    } else {
                        ai_timeline_item_details_json(item).unwrap_or(content_text)
                    };
                    let has_details = command_details.is_some() || !details_text.is_empty();
                    let expanded =
                        has_details && this.ai_expanded_timeline_row_ids.contains(row.id.as_str());
                    let preview_source = ai_tool_preview_text(item, content_text)
                        .unwrap_or_default();
                    let (preview, _preview_truncated) = if !preview_source.is_empty() {
                        ai_truncate_multiline_content(preview_source.as_str(), 3)
                    } else {
                        (String::new(), false)
                    };
                    let show_preview = !preview.is_empty() && !expanded;
                    let show_toggle = has_details;
                    let toggle_id =
                        format!("ai-toggle-timeline-row-{}", row.id.replace('\u{1f}', "--"));

                    let row_element = h_flex()
                        .w_full()
                        .min_w_0()
                        .justify_start()
                        .child(
                            v_flex()
                                .max_w(px(900.0))
                                .w_full()
                                .min_w_0()
                                .gap_1()
                                .px_2p5()
                                .py_2()
                                .overflow_hidden()
                                .rounded(px(10.0))
                                .border_1()
                                .border_color(cx.theme().border.opacity(if is_dark {
                                    0.88
                                } else {
                                    0.72
                                }))
                                .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                                    0.14
                                } else {
                                    0.20
                                })))
                                .child(
                                    h_flex()
                                        .w_full()
                                        .min_w_0()
                                        .items_start()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            h_flex()
                                                .flex_1()
                                                .min_w_0()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .text_xs()
                                                        .font_semibold()
                                                        .truncate()
                                                        .child(label.to_string()),
                                                )
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .text_xs()
                                                        .text_color(status_color)
                                                        .child(status),
                                                ),
                                        )
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
                                                        "Hide details"
                                                    } else {
                                                        "Show details"
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
                                        }),
                                )
                                .when(show_preview, |this| {
                                    this.child(ai_tool_detail_section(
                                        "Preview",
                                        preview.clone(),
                                        true,
                                        None,
                                        false,
                                        is_dark,
                                        cx,
                                    ))
                                })
                                .when(expanded, |this| {
                                    let expanded_details = command_details
                                        .as_ref()
                                        .map(|details| {
                                            render_ai_command_execution_details(
                                                details,
                                                content_text,
                                                is_dark,
                                                cx,
                                            )
                                        })
                                        .unwrap_or_else(|| {
                                            ai_tool_detail_section(
                                                "Details",
                                                details_text.to_string(),
                                                true,
                                                Some(px(240.0)),
                                                false,
                                                is_dark,
                                                cx,
                                            )
                                        });
                                    this.child(expanded_details)
                                }),
                        );
                    ai_timeline_row_with_animation(this, row.id.as_str(), row_element)
                }
            }
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
                        .border_color(cx.theme().border.opacity(if is_dark { 0.9 } else { 0.74 }))
                        .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                            0.16
                        } else {
                            0.22
                        })))
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
                        .when(!preview.is_empty(), |this| {
                            this.child(ai_tool_detail_section(
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
