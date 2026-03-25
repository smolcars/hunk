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
    theme: &gpui_component::Theme,
    default_color: Hsla,
    token: MarkdownCodeTokenKind,
) -> Hsla {
    markdown_syntax_color(theme, default_color, token)
}

fn ai_markdown_code_block_text_and_highlights(
    lines: &[Vec<hunk_domain::markdown_preview::MarkdownCodeSpan>],
    theme: &gpui_component::Theme,
    default_color: Hsla,
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

            let token_color = ai_markdown_code_token_color(theme, default_color, span.token);
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
                let text = markdown_inline_text_and_link_ranges(spans).0;
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
                let text = markdown_inline_text_and_link_ranges(spans).0;
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
                let text = markdown_inline_text_and_link_ranges(spans).0;
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
    theme: &gpui_component::Theme,
    is_dark: bool,
) -> AnyElement {
    const AI_PERF_SLOW_MARKDOWN_THRESHOLD_MS: f64 = 8.0;

    let cached = this
        .ai_markdown_row_cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(row_id).cloned())
        .filter(|entry| entry.markdown == markdown);
    let (
        blocks,
        selection_surfaces,
        markdown_cache_hit,
        parse_elapsed,
        comrak_parse_elapsed,
        transform_elapsed,
        code_highlight_elapsed,
        code_block_count,
        code_char_count,
        selection_surface_elapsed,
    ) = if let Some(entry) = cached {
        this.record_ai_markdown_cache_hit();
        (
            entry.blocks,
            entry.selection_surfaces,
            true,
            std::time::Duration::ZERO,
            std::time::Duration::ZERO,
            std::time::Duration::ZERO,
            std::time::Duration::ZERO,
            0,
            0,
            std::time::Duration::ZERO,
        )
    } else {
        let parse_started_at = std::time::Instant::now();
        let (blocks, parse_stats) =
            hunk_domain::markdown_preview::parse_markdown_preview_with_stats(markdown);
        let parse_elapsed = parse_started_at.elapsed();
        let selection_surface_started_at = std::time::Instant::now();
        let selection_surfaces = ai_chat_markdown_selection_surfaces(row_id, blocks.as_slice());
        let selection_surface_elapsed = selection_surface_started_at.elapsed();
        this.record_ai_markdown_cache_miss(
            parse_elapsed,
            parse_stats.comrak_parse,
            parse_stats.transform,
            parse_stats.code_highlight,
            parse_stats.code_block_count,
            parse_stats.code_char_count,
            selection_surface_elapsed,
        );
        let blocks: Arc<[MarkdownPreviewBlock]> = blocks.into();
        if let Ok(mut cache) = this.ai_markdown_row_cache.lock() {
            if cache.len() > 512 {
                cache.clear();
            }
            cache.insert(
                row_id.to_string(),
                AiMarkdownRowCacheEntry {
                    markdown: markdown.to_string(),
                    blocks: blocks.clone(),
                    selection_surfaces: selection_surfaces.clone(),
                },
            );
        }
        (
            blocks,
            selection_surfaces,
            false,
            parse_elapsed,
            parse_stats.comrak_parse,
            parse_stats.transform,
            parse_stats.code_highlight,
            parse_stats.code_block_count,
            parse_stats.code_char_count,
            selection_surface_elapsed,
        )
    };
    if blocks.is_empty() {
        return div().w_full().text_sm().child("").into_any_element();
    }
    let render_started_at = std::time::Instant::now();
    let block_count = blocks.len();
    let char_count = markdown.len();
    let element = v_flex()
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
                theme,
                is_dark,
            )
        }))
        .into_any_element();
    let render_elapsed = render_started_at.elapsed();
    this.record_ai_markdown_render_build(render_elapsed, block_count, char_count);

    if parse_elapsed.as_secs_f64() * 1_000.0 >= AI_PERF_SLOW_MARKDOWN_THRESHOLD_MS
        || render_elapsed.as_secs_f64() * 1_000.0 >= AI_PERF_SLOW_MARKDOWN_THRESHOLD_MS
    {
        tracing::info!(
            target: "ai_perf",
            concat!(
                "ai_perf_md_slow row={} cached={} chars={} blocks={} ",
                "parse_ms={:.1} doc_ms={:.1} xform_ms={:.1} code_ms={:.1} ",
                "code_blocks={} code_chars={} surf_ms={:.1} build_ms={:.1}"
            ),
            row_id,
            markdown_cache_hit,
            char_count,
            block_count,
            parse_elapsed.as_secs_f64() * 1_000.0,
            comrak_parse_elapsed.as_secs_f64() * 1_000.0,
            transform_elapsed.as_secs_f64() * 1_000.0,
            code_highlight_elapsed.as_secs_f64() * 1_000.0,
            code_block_count,
            code_char_count,
            selection_surface_elapsed.as_secs_f64() * 1_000.0,
            render_elapsed.as_secs_f64() * 1_000.0,
        );
    }

    element
}

#[allow(clippy::too_many_arguments)]
fn ai_render_chat_markdown_block(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    block_ix: usize,
    block: &MarkdownPreviewBlock,
    selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
    theme: &gpui_component::Theme,
    is_dark: bool,
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
            theme.foreground,
            theme,
            is_dark,
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
            theme.foreground,
            theme,
            is_dark,
        ),
        MarkdownPreviewBlock::UnorderedListItem(spans) => h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
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
                    theme.foreground,
                    theme,
                    is_dark,
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
                    .text_color(theme.muted_foreground)
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
                    theme.foreground,
                    theme,
                    is_dark,
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
                    .text_color(theme.muted_foreground)
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
                    theme.muted_foreground,
                    theme,
                    is_dark,
                )),
            )
            .into_any_element(),
        MarkdownPreviewBlock::CodeBlock { language, lines } => {
            let language_label = language.clone().unwrap_or_else(|| "code".to_string());
            let code_surface_id = ai_timeline_text_surface_id(row_id, "message-code", block_ix);
            let copy_button_id = format!(
                "ai-copy-code-block-{}-{block_ix}",
                row_id.replace('\u{1f}', "--")
            );
            let code_text = ai_markdown_code_block_text(lines);
            let default_color = theme.foreground;
            let (text, highlights) =
                ai_markdown_code_block_text_and_highlights(lines, theme, default_color);
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
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .whitespace_nowrap()
                                .truncate()
                                .text_xs()
                                .font_family(theme.mono_font_family.clone())
                                .text_color(theme.muted_foreground)
                                .child(language_label),
                        )
                        .child(
                            Button::new(copy_button_id)
                                .flex_none()
                                .ghost()
                                .compact()
                                .rounded(px(7.0))
                                .icon(Icon::new(IconName::Copy).size(px(12.0)))
                                .text_color(theme.muted_foreground)
                                .min_w(px(22.0))
                                .h(px(20.0))
                                .tooltip("Copy code block")
                                .on_click({
                                    let view = view.clone();
                                    move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_copy_text_action(
                                                code_text.clone(),
                                                "Copied code block.",
                                                window,
                                                cx,
                                            );
                                        });
                                    }
                                }),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(hunk_opacity(theme.border, is_dark, 0.88, 0.74))
                        .bg(hunk_opacity(theme.secondary, is_dark, 0.30, 0.44))
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
                                        .font_family(theme.mono_font_family.clone())
                                        .whitespace_normal()
                                        .child(
                                            div().w_full().min_w_0().child(
                                                ai_render_selectable_styled_text(
                                                    this,
                                                    view.clone(),
                                                    row_id,
                                                    code_surface_id,
                                                    selection_surfaces.clone(),
                                                    ai_text_link_ranges(Vec::new()),
                                                    styled_text,
                                                    hunk_text_selection_background(theme, is_dark),
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
            .bg(hunk_opacity(theme.border, is_dark, 0.8, 0.95))
            .into_any_element(),
    }
}

type AiMarkdownHighlights = Vec<(std::ops::Range<usize>, HighlightStyle)>;

struct AiMarkdownInlineRenderData {
    text: SharedString,
    highlights: AiMarkdownHighlights,
    link_ranges: Vec<MarkdownLinkRange>,
}

fn ai_chat_markdown_text_and_highlights(
    spans: &[MarkdownInlineSpan],
    base_color: Hsla,
    theme: &gpui_component::Theme,
    is_dark: bool,
) -> AiMarkdownInlineRenderData {
    let (text, link_ranges) = markdown_inline_text_and_link_ranges(spans);
    let mut highlights = Vec::new();
    let link_color = theme.primary;
    let code_background = hunk_opacity(theme.secondary, is_dark, 0.30, 0.42);
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

    AiMarkdownInlineRenderData {
        text: text.into(),
        highlights,
        link_ranges,
    }
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
    theme: &gpui_component::Theme,
    is_dark: bool,
) -> AnyElement {
    let AiMarkdownInlineRenderData {
        text,
        highlights,
        link_ranges,
    } = ai_chat_markdown_text_and_highlights(spans, base_color, theme, is_dark);
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
            ai_text_link_ranges(link_ranges),
            styled_text,
            hunk_text_selection_background(theme, is_dark),
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
