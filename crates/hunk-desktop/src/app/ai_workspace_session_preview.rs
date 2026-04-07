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
            vec![None; lines.len()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
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
                None,
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
                    None,
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
                    None,
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
                    None,
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
                    None,
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
                    None,
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
                        None,
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
                        None,
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
                    None,
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
        max_lines,
    )
}

fn ai_workspace_inline_diff_preview_lines(
    block: &AiWorkspaceBlock,
    inline_diff_state: &AiWorkspaceInlineDiffState,
    text_width_px: usize,
) -> AiWorkspacePreviewProjection {
    let mut structured_lines = Vec::<AiWorkspaceStructuredPreviewLine>::new();
    let projection = inline_diff_state.projection.as_ref();

    for (file_index, file) in projection.files.iter().enumerate() {
        if file_index > 0 {
            structured_lines.push((
                String::new(),
                AiWorkspacePreviewLineKind::Normal,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ));
        }

        let header_text = format!("{}  +{}  -{}", file.display_path, file.added, file.removed);
        structured_lines.push((
            header_text.clone(),
            AiWorkspacePreviewLineKind::DiffFileHeader,
            Some(AiWorkspacePreviewHitTarget::InlineDiff(
                AiWorkspaceInlineDiffHitTarget::FileHeader { file_index },
            )),
            Vec::new(),
            ai_workspace_inline_diff_file_header_style_spans(
                header_text.as_str(),
                file.display_path.as_str(),
                text_width_px,
            ),
            Vec::new(),
        ));

        for (meta_index, meta) in file.prelude_meta.iter().enumerate() {
            ai_workspace_push_inline_diff_structured_line(
                &mut structured_lines,
                meta.as_str(),
                AiWorkspacePreviewLineKind::DiffMeta,
                Some(AiWorkspacePreviewHitTarget::InlineDiff(
                    AiWorkspaceInlineDiffHitTarget::Meta {
                        file_index,
                        hunk_index: None,
                        meta_index,
                    },
                )),
                Some(AiWorkspacePreviewColorRole::Muted),
                false,
                true,
            );
        }

        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            ai_workspace_push_inline_diff_structured_line(
                &mut structured_lines,
                hunk.header.as_str(),
                AiWorkspacePreviewLineKind::DiffHunkHeader,
                Some(AiWorkspacePreviewHitTarget::InlineDiff(
                    AiWorkspaceInlineDiffHitTarget::HunkHeader {
                        file_index,
                        hunk_index,
                    },
                )),
                Some(AiWorkspacePreviewColorRole::Muted),
                false,
                false,
            );
            for (line_index, line) in hunk.lines.iter().enumerate() {
                let (kind, color_role, prefix) = match line.kind {
                    crate::app::ai_workspace_inline_diff::AiWorkspaceInlineDiffLineKind::Context => (
                        AiWorkspacePreviewLineKind::DiffContext,
                        Some(AiWorkspacePreviewColorRole::Foreground),
                        "  ",
                    ),
                    crate::app::ai_workspace_inline_diff::AiWorkspaceInlineDiffLineKind::Added => (
                        AiWorkspacePreviewLineKind::DiffAdded,
                        Some(AiWorkspacePreviewColorRole::Added),
                        "+ ",
                    ),
                    crate::app::ai_workspace_inline_diff::AiWorkspaceInlineDiffLineKind::Removed => (
                        AiWorkspacePreviewLineKind::DiffRemoved,
                        Some(AiWorkspacePreviewColorRole::Removed),
                        "- ",
                    ),
                };
                ai_workspace_push_inline_diff_structured_line(
                    &mut structured_lines,
                    format!("{prefix}{}", line.text.trim_end_matches('\r')).as_str(),
                    kind,
                    Some(AiWorkspacePreviewHitTarget::InlineDiff(
                        AiWorkspaceInlineDiffHitTarget::Line {
                            file_index,
                            hunk_index,
                            line_index,
                            kind: line.kind,
                        },
                    )),
                    color_role,
                    false,
                    false,
                );
            }
            if hunk.truncated_line_count > 0 {
                ai_workspace_push_inline_diff_structured_line(
                    &mut structured_lines,
                    format!("... {} more lines omitted", hunk.truncated_line_count).as_str(),
                    AiWorkspacePreviewLineKind::DiffMeta,
                    Some(AiWorkspacePreviewHitTarget::InlineDiff(
                        AiWorkspaceInlineDiffHitTarget::Meta {
                            file_index,
                            hunk_index: Some(hunk_index),
                            meta_index: hunk.lines.len(),
                        },
                    )),
                    Some(AiWorkspacePreviewColorRole::Muted),
                    false,
                    true,
                );
            }
            for (meta_index, meta) in hunk.trailing_meta.iter().enumerate() {
                ai_workspace_push_inline_diff_structured_line(
                    &mut structured_lines,
                    meta.as_str(),
                    AiWorkspacePreviewLineKind::DiffMeta,
                    Some(AiWorkspacePreviewHitTarget::InlineDiff(
                        AiWorkspaceInlineDiffHitTarget::Meta {
                            file_index,
                            hunk_index: Some(hunk_index),
                            meta_index,
                        },
                    )),
                    Some(AiWorkspacePreviewColorRole::Muted),
                    false,
                    true,
                );
            }
        }

        if file.truncated_hunk_count > 0 {
            ai_workspace_push_inline_diff_structured_line(
                &mut structured_lines,
                format!("... {} more hunks omitted", file.truncated_hunk_count).as_str(),
                AiWorkspacePreviewLineKind::DiffMeta,
                Some(AiWorkspacePreviewHitTarget::InlineDiff(
                    AiWorkspaceInlineDiffHitTarget::Meta {
                        file_index,
                        hunk_index: None,
                        meta_index: file.hunks.len(),
                    },
                )),
                Some(AiWorkspacePreviewColorRole::Muted),
                false,
                true,
            );
        }

        for (meta_index, meta) in file.epilogue_meta.iter().enumerate() {
            ai_workspace_push_inline_diff_structured_line(
                &mut structured_lines,
                meta.as_str(),
                AiWorkspacePreviewLineKind::DiffMeta,
                Some(AiWorkspacePreviewHitTarget::InlineDiff(
                    AiWorkspaceInlineDiffHitTarget::Meta {
                        file_index,
                        hunk_index: None,
                        meta_index,
                    },
                )),
                Some(AiWorkspacePreviewColorRole::Muted),
                false,
                true,
            );
        }
    }

    if let Some(notice) = inline_diff_state.presentation_policy.truncation_notice.as_deref() {
        if !structured_lines.is_empty() {
            structured_lines.push((
                String::new(),
                AiWorkspacePreviewLineKind::Normal,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ));
        }
        ai_workspace_push_inline_diff_structured_line(
            &mut structured_lines,
            notice,
            AiWorkspacePreviewLineKind::DiffMeta,
            Some(AiWorkspacePreviewHitTarget::InlineDiff(
                AiWorkspaceInlineDiffHitTarget::TruncationNotice,
            )),
            Some(AiWorkspacePreviewColorRole::Muted),
            true,
            true,
        );
    }

    ai_workspace_wrap_structured_preview_lines(
        structured_lines,
        Vec::new(),
        text_width_px,
        ai_workspace_preview_line_limit(block),
    )
}

fn ai_workspace_inline_diff_file_header_style_spans(
    line: &str,
    display_path: &str,
    text_width_px: usize,
) -> Vec<AiWorkspacePreviewStyleSpan> {
    let mut spans = Vec::new();
    if !display_path.is_empty() && let Some(path_start) = line.find(display_path) {
        spans.push(AiWorkspacePreviewStyleSpan {
            range: path_start..path_start.saturating_add(display_path.len()),
            color_role: Some(AiWorkspacePreviewColorRole::Accent),
            bold: true,
            italic: false,
            strikethrough: false,
            code: false,
            link: false,
        });
    }
    spans.extend(ai_workspace_diff_stat_style_spans(
        ai_workspace_wrap_text(line, ai_workspace_chars_per_line(text_width_px, false, false), 1)
            .first()
            .map(String::as_str)
            .unwrap_or(line),
    ));
    spans
}

fn ai_workspace_push_inline_diff_structured_line(
    structured_lines: &mut Vec<AiWorkspaceStructuredPreviewLine>,
    text: &str,
    kind: AiWorkspacePreviewLineKind,
    hit_target: Option<AiWorkspacePreviewHitTarget>,
    color_role: Option<AiWorkspacePreviewColorRole>,
    bold: bool,
    italic: bool,
) {
    structured_lines.push((
        text.to_string(),
        kind,
        hit_target,
        Vec::new(),
        (!text.is_empty())
            .then(|| {
                vec![AiWorkspacePreviewStyleSpan {
                    range: 0..text.len(),
                    color_role,
                    bold,
                    italic,
                    strikethrough: false,
                    code: false,
                    link: false,
                }]
            })
            .unwrap_or_default(),
        Vec::new(),
    ));
}

fn ai_workspace_push_markdown_block_line(
    structured_lines: &mut Vec<AiWorkspaceStructuredPreviewLine>,
    text: String,
    kind: AiWorkspacePreviewLineKind,
    hit_target: Option<AiWorkspacePreviewHitTarget>,
    link_ranges: Vec<MarkdownLinkRange>,
    style_spans: Vec<AiWorkspacePreviewStyleSpan>,
    syntax_spans: Vec<AiWorkspacePreviewSyntaxSpan>,
) {
    if !text.trim().is_empty() {
        structured_lines.push((text, kind, hit_target, link_ranges, style_spans, syntax_spans));
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
            Vec::new(),
        );
    }

    let mut wrapped_lines = Vec::new();
    let mut wrapped_kinds = Vec::new();
    let mut wrapped_hit_targets = Vec::new();
    let mut wrapped_link_ranges = Vec::new();
    let mut wrapped_style_spans = Vec::new();
    let mut wrapped_syntax_spans = Vec::new();
    let mut wrapped_copy_regions = Vec::new();
    let mut structured_line_to_wrapped_start = Vec::new();
    let mut structured_line_to_wrapped_end = Vec::new();

    let total_structured_lines = structured_lines.len();
    for (line_index, (line, kind, hit_target, link_ranges, style_spans, syntax_spans)) in
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
            wrapped_hit_targets.push(hit_target.clone());
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
                    wrapped_hit_targets,
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
        wrapped_hit_targets,
        wrapped_link_ranges,
        wrapped_style_spans,
        wrapped_syntax_spans,
        wrapped_copy_regions,
    )
}
