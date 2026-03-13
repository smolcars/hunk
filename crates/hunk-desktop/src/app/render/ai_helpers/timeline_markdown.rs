fn ai_markdown_code_line_text(spans: &[hunk_domain::markdown_preview::MarkdownCodeSpan]) -> String {
    let mut text = String::new();
    for span in spans {
        text.push_str(span.text.as_str());
    }
    text
}

fn ai_markdown_code_block_text(
    lines: &[Vec<hunk_domain::markdown_preview::MarkdownCodeSpan>],
) -> String {
    lines
        .iter()
        .map(|line| ai_markdown_code_line_text(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn ai_markdown_code_token_color(
    default_color: Hsla,
    token: MarkdownCodeTokenKind,
    is_dark: bool,
) -> Hsla {
    markdown_syntax_color(default_color, token, is_dark)
}

fn ai_markdown_code_block_text_and_highlights(
    lines: &[Vec<hunk_domain::markdown_preview::MarkdownCodeSpan>],
    default_color: Hsla,
    is_dark: bool,
) -> (SharedString, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut highlights = Vec::new();
    let mut cursor = 0usize;

    for (line_ix, line_spans) in lines.iter().enumerate() {
        if line_ix > 0 {
            text.push('\n');
            cursor += 1;
        }

        for span in line_spans {
            if span.text.is_empty() {
                continue;
            }

            let start = cursor;
            text.push_str(span.text.as_str());
            cursor += span.text.len();

            let token_color = ai_markdown_code_token_color(default_color, span.token, is_dark);
            if token_color != default_color {
                highlights.push((
                    start..cursor,
                    HighlightStyle {
                        color: Some(token_color),
                        ..HighlightStyle::default()
                    },
                ));
            }
        }
    }

    (text.into(), highlights)
}

fn ai_chat_markdown_selection_surfaces(
    row_id: &str,
    blocks: &[MarkdownPreviewBlock],
) -> Arc<[AiTextSelectionSurfaceSpec]> {
    let mut surfaces = Vec::new();

    for (block_ix, block) in blocks.iter().enumerate() {
        let block_separator = if surfaces.is_empty() { "" } else { "\n\n" };
        match block {
            MarkdownPreviewBlock::Heading { spans, .. } => {
                let text = ai_chat_markdown_text(spans);
                if text.is_empty() {
                    continue;
                }
                surfaces.push(
                    AiTextSelectionSurfaceSpec::new(
                        ai_timeline_text_surface_id(row_id, "message-heading", block_ix),
                        text,
                    )
                    .with_separator_before(block_separator),
                );
            }
            MarkdownPreviewBlock::Paragraph(spans)
            | MarkdownPreviewBlock::UnorderedListItem(spans)
            | MarkdownPreviewBlock::BlockQuote(spans) => {
                let text = ai_chat_markdown_text(spans);
                if text.is_empty() {
                    continue;
                }
                let surface_kind = match block {
                    MarkdownPreviewBlock::Paragraph(_) => "message-paragraph",
                    MarkdownPreviewBlock::UnorderedListItem(_) => "message-list",
                    MarkdownPreviewBlock::BlockQuote(_) => "message-quote",
                    _ => unreachable!(),
                };
                surfaces.push(
                    AiTextSelectionSurfaceSpec::new(
                        ai_timeline_text_surface_id(row_id, surface_kind, block_ix),
                        text,
                    )
                    .with_separator_before(block_separator),
                );
            }
            MarkdownPreviewBlock::OrderedListItem { spans, .. } => {
                let text = ai_chat_markdown_text(spans);
                if text.is_empty() {
                    continue;
                }
                surfaces.push(
                    AiTextSelectionSurfaceSpec::new(
                        ai_timeline_text_surface_id(row_id, "message-list", block_ix),
                        text,
                    )
                    .with_separator_before(block_separator),
                );
            }
            MarkdownPreviewBlock::CodeBlock { lines, .. } => {
                if lines.is_empty() {
                    continue;
                }
                surfaces.push(
                    AiTextSelectionSurfaceSpec::new(
                        ai_timeline_text_surface_id(row_id, "message-code", block_ix),
                        ai_markdown_code_block_text(lines),
                    )
                    .with_separator_before(block_separator),
                );
            }
            MarkdownPreviewBlock::ThematicBreak => {}
        }
    }

    surfaces.into()
}

fn ai_render_chat_markdown_message(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    markdown: &str,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let blocks = hunk_domain::markdown_preview::parse_markdown_preview(markdown);
    if blocks.is_empty() {
        return div().w_full().text_sm().child("").into_any_element();
    }
    let selection_surfaces = ai_chat_markdown_selection_surfaces(row_id, blocks.as_slice());

    v_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .children(blocks.iter().enumerate().map(|(block_ix, block)| {
            ai_render_chat_markdown_block(
                this,
                view.clone(),
                row_id,
                block_ix,
                block,
                selection_surfaces.clone(),
                is_dark,
                cx,
            )
        }))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn ai_render_chat_markdown_block(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    block_ix: usize,
    block: &MarkdownPreviewBlock,
    selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    match block {
        MarkdownPreviewBlock::Heading { level, spans } => ai_render_chat_inline_spans(
            this,
            view,
            row_id,
            ai_timeline_text_surface_id(row_id, "message-heading", block_ix),
            selection_surfaces,
            spans,
            matches!(level, 1 | 2),
            true,
            cx.theme().foreground,
            is_dark,
            cx,
        ),
        MarkdownPreviewBlock::Paragraph(spans) => ai_render_chat_inline_spans(
            this,
            view,
            row_id,
            ai_timeline_text_surface_id(row_id, "message-paragraph", block_ix),
            selection_surfaces,
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
                    this,
                    view,
                    row_id,
                    ai_timeline_text_surface_id(row_id, "message-list", block_ix),
                    selection_surfaces,
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
                    this,
                    view,
                    row_id,
                    ai_timeline_text_surface_id(row_id, "message-list", block_ix),
                    selection_surfaces,
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
                    this,
                    view,
                    row_id,
                    ai_timeline_text_surface_id(row_id, "message-quote", block_ix),
                    selection_surfaces.clone(),
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
            let code_surface_id = ai_timeline_text_surface_id(row_id, "message-code", block_ix);
            let default_color = cx.theme().foreground;
            let is_dark_theme = cx.theme().mode.is_dark();
            let (text, highlights) =
                ai_markdown_code_block_text_and_highlights(lines, default_color, is_dark_theme);
            let styled_text = if highlights.is_empty() {
                StyledText::new(text.clone())
            } else {
                StyledText::new(text.clone()).with_highlights(highlights)
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
                        .w_full()
                        .min_w_0()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.88, 0.74))
                        .bg(hunk_opacity(cx.theme().secondary, is_dark, 0.30, 0.44))
                        .p_2()
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .overflow_x_scrollbar()
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .text_xs()
                                        .font_family(cx.theme().mono_font_family.clone())
                                        .whitespace_normal()
                                        .child(
                                            div().w_full().min_w_0().child(
                                                ai_render_selectable_styled_text(
                                                    this,
                                                    view.clone(),
                                                    row_id,
                                                    code_surface_id,
                                                    selection_surfaces.clone(),
                                                    styled_text,
                                                    is_dark,
                                                    cx,
                                                ),
                                            ),
                                        ),
                                ),
                        ),
                )
                .into_any_element()
        }
        MarkdownPreviewBlock::ThematicBreak => div()
            .h(px(1.0))
            .w_full()
            .bg(hunk_opacity(cx.theme().border, is_dark, 0.8, 0.95))
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
    let code_background = hunk_opacity(cx.theme().secondary, is_dark, 0.30, 0.42);
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

#[allow(clippy::too_many_arguments)]
fn ai_render_chat_inline_spans(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    surface_id: impl Into<String>,
    selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
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
        .child(ai_render_selectable_styled_text(
            this,
            view,
            row_id,
            surface_id,
            selection_surfaces,
            styled_text,
            is_dark,
            cx,
        ));

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
