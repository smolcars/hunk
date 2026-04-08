fn ai_workspace_message_preview_lines(
    markdown: &str,
    text_width_px: usize,
    block: &AiWorkspaceBlock,
) -> AiWorkspacePreviewProjection {
    let max_lines = ai_workspace_preview_line_limit(block);
    if markdown.trim().is_empty() {
        return (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
    }

    let blocks = hunk_domain::markdown_preview::parse_markdown_preview(markdown);
    if blocks.is_empty() {
        let lines = ai_workspace_wrap_text(
            markdown,
            ai_workspace_chars_per_line(text_width_px, false, false),
            max_lines,
        );
        return (
            lines.clone(),
            vec![AiWorkspacePreviewLineKind::Normal; lines.len()],
            vec![Vec::new(); lines.len()],
            vec![Vec::new(); lines.len()],
            vec![Vec::new(); lines.len()],
            Vec::new(),
        );
    }

    let mut structured_lines = Vec::<AiWorkspaceStructuredPreviewLine>::new();
    let mut copy_regions = Vec::<AiWorkspaceCopyRegion>::new();
    for (block_index, markdown_block) in blocks.into_iter().enumerate() {
        if block_index > 0 {
            structured_lines.push((
                String::new(),
                AiWorkspacePreviewLineKind::Normal,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ));
        }
        match markdown_block {
            hunk_domain::markdown_preview::MarkdownPreviewBlock::Heading { spans, .. } => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    text,
                    AiWorkspacePreviewLineKind::Heading,
                    link_ranges,
                    style_spans,
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::Paragraph(spans) => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    text,
                    AiWorkspacePreviewLineKind::Normal,
                    link_ranges,
                    style_spans,
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::UnorderedListItem(spans) => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    format!("- {text}"),
                    AiWorkspacePreviewLineKind::Normal,
                    ai_workspace_offset_link_ranges(link_ranges, 2),
                    ai_workspace_offset_style_spans(style_spans, 2),
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::OrderedListItem {
                number,
                spans,
            } => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                let prefix = format!("{number}. ");
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    format!("{prefix}{text}"),
                    AiWorkspacePreviewLineKind::Normal,
                    ai_workspace_offset_link_ranges(link_ranges, prefix.len()),
                    ai_workspace_offset_style_spans(style_spans, prefix.len()),
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::BlockQuote(spans) => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    format!("| {text}"),
                    AiWorkspacePreviewLineKind::Quote,
                    ai_workspace_offset_link_ranges(link_ranges, 2),
                    ai_workspace_offset_style_spans(style_spans, 2),
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::CodeBlock { language, lines } => {
                let copy_region_start = structured_lines.len();
                if let Some(language) = language.filter(|value| !value.trim().is_empty()) {
                    ai_workspace_push_markdown_block_line(
                        &mut structured_lines,
                        language,
                        AiWorkspacePreviewLineKind::Quote,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    );
                }
                let mut code_lines = Vec::with_capacity(lines.len());
                for line in lines {
                    let (text, syntax_spans) =
                        ai_workspace_markdown_code_line_text_and_spans(&line);
                    code_lines.push(text.clone());
                    structured_lines.push((
                        text,
                        AiWorkspacePreviewLineKind::Code,
                        Vec::new(),
                        Vec::new(),
                        syntax_spans,
                    ));
                }
                if !code_lines.is_empty() {
                    copy_regions.push(AiWorkspaceCopyRegion {
                        line_range: copy_region_start..structured_lines.len(),
                        text: code_lines.join("\n"),
                        tooltip: "Copy code block",
                        success_message: "Copied code block.",
                    });
                }
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::ThematicBreak => {
                structured_lines.push((
                    "----".to_string(),
                    AiWorkspacePreviewLineKind::Rule,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                ));
            }
        }
    }

    ai_workspace_wrap_structured_preview_lines(
        structured_lines,
        copy_regions,
        text_width_px,
        ai_workspace_preview_line_limit(block),
    )
}

fn ai_workspace_push_markdown_block_line(
    structured_lines: &mut Vec<AiWorkspaceStructuredPreviewLine>,
    text: String,
    kind: AiWorkspacePreviewLineKind,
    link_ranges: Vec<MarkdownLinkRange>,
    style_spans: Vec<AiWorkspacePreviewStyleSpan>,
    syntax_spans: Vec<AiWorkspacePreviewSyntaxSpan>,
) {
    if !text.trim().is_empty() {
        structured_lines.push((text, kind, link_ranges, style_spans, syntax_spans));
    }
}

fn ai_workspace_wrap_structured_preview_lines(
    structured_lines: Vec<AiWorkspaceStructuredPreviewLine>,
    copy_regions: Vec<AiWorkspaceCopyRegion>,
    text_width_px: usize,
    max_lines: usize,
) -> AiWorkspacePreviewProjection {
    if max_lines == 0 {
        return (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
    }

    let mut wrapped_lines = Vec::new();
    let mut wrapped_kinds = Vec::new();
    let mut wrapped_link_ranges = Vec::new();
    let mut wrapped_style_spans = Vec::new();
    let mut wrapped_syntax_spans = Vec::new();
    let mut wrapped_copy_regions = Vec::new();
    let mut structured_line_to_wrapped_start = Vec::new();
    let mut structured_line_to_wrapped_end = Vec::new();

    let total_structured_lines = structured_lines.len();
    for (line_index, (line, kind, link_ranges, style_spans, syntax_spans)) in
        structured_lines.into_iter().enumerate()
    {
        let wrapped_start = wrapped_lines.len();
        structured_line_to_wrapped_start.push(wrapped_start);
        let has_more_input = line_index + 1 < total_structured_lines;
        let max_chars_per_line =
            ai_workspace_chars_per_line(text_width_px, false, kind.is_monospace());
        let wrapped = ai_workspace_wrap_text_ranges(line.as_str(), max_chars_per_line, usize::MAX);
        let wrapped = if wrapped.is_empty() {
            std::iter::once(0..0).collect::<Vec<_>>()
        } else {
            wrapped
        };

        for (wrapped_index, wrapped_range) in wrapped.into_iter().enumerate() {
            wrapped_lines.push(line[wrapped_range.clone()].to_string());
            wrapped_kinds.push(kind);
            wrapped_link_ranges.push(ai_workspace_clip_link_ranges(
                link_ranges.as_slice(),
                wrapped_range.clone(),
            ));
            wrapped_style_spans.push(ai_workspace_clip_style_spans(
                style_spans.as_slice(),
                wrapped_range.clone(),
            ));
            wrapped_syntax_spans.push(ai_workspace_clip_syntax_spans(
                syntax_spans.as_slice(),
                wrapped_range,
            ));
            if wrapped_lines.len() == max_lines {
                if has_more_input || wrapped_index > 0 {
                    ai_workspace_append_ellipsis(wrapped_lines.last_mut());
                }
                structured_line_to_wrapped_end.push(wrapped_lines.len());
                wrapped_copy_regions.extend(ai_workspace_clip_copy_regions(
                    copy_regions.as_slice(),
                    structured_line_to_wrapped_start.as_slice(),
                    structured_line_to_wrapped_end.as_slice(),
                    wrapped_lines.len(),
                ));
                return (
                    wrapped_lines,
                    wrapped_kinds,
                    wrapped_link_ranges,
                    wrapped_style_spans,
                    wrapped_syntax_spans,
                    wrapped_copy_regions,
                );
            }
        }
        structured_line_to_wrapped_end.push(wrapped_lines.len());
    }

    wrapped_copy_regions.extend(ai_workspace_clip_copy_regions(
        copy_regions.as_slice(),
        structured_line_to_wrapped_start.as_slice(),
        structured_line_to_wrapped_end.as_slice(),
        wrapped_lines.len(),
    ));

    (
        wrapped_lines,
        wrapped_kinds,
        wrapped_link_ranges,
        wrapped_style_spans,
        wrapped_syntax_spans,
        wrapped_copy_regions,
    )
}
