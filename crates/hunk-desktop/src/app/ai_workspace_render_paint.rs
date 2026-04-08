fn ai_workspace_paint_selection(
    line: &AiWorkspacePaintLine,
    selection_range: Option<Range<usize>>,
    selection_background: gpui::Hsla,
    window: &mut Window,
) {
    let Some(selection_range) = selection_range else {
        return;
    };
    let Some(local_range) = ai_workspace_line_selection_range(line, selection_range) else {
        return;
    };
    let Some((start_column, end_column)) =
        ai_workspace_selection_columns(line.column_byte_offsets.as_ref(), &local_range)
    else {
        return;
    };
    if start_column == end_column {
        return;
    }

    window.paint_quad(fill(
        Bounds {
            origin: point(
                line.origin.x + line.char_width * start_column as f32,
                line.origin.y,
            ),
            size: size(
                line.char_width * (end_column - start_column) as f32,
                line.line_height,
            ),
        },
        selection_background,
    ));
}

fn ai_workspace_line_selection_range(
    line: &AiWorkspacePaintLine,
    selection_range: Range<usize>,
) -> Option<Range<usize>> {
    let start = selection_range.start.max(line.surface_byte_range.start);
    let end = selection_range.end.min(line.surface_byte_range.end);
    (start < end).then(|| {
        start.saturating_sub(line.surface_byte_range.start)
            ..end.saturating_sub(line.surface_byte_range.start)
    })
}

fn ai_workspace_selection_columns(
    column_byte_offsets: &[usize],
    range: &Range<usize>,
) -> Option<(usize, usize)> {
    if range.is_empty() || column_byte_offsets.len() < 2 {
        return None;
    }

    let start_column = column_byte_offsets
        .partition_point(|offset| *offset <= range.start)
        .saturating_sub(1);
    let end_column = column_byte_offsets.partition_point(|offset| *offset < range.end);
    (start_column < end_column).then_some((start_column, end_column))
}

fn ai_workspace_text_runs_for_line(
    line: &AiWorkspacePaintLine,
    default_color: gpui::Hsla,
    link_color: gpui::Hsla,
    font: Font,
    theme: &gpui_component::Theme,
) -> Vec<TextRun> {
    if !line.syntax_spans.is_empty() {
        let mut runs = Vec::new();
        let mut cursor = 0usize;
        for syntax_span in line.syntax_spans.iter() {
            if syntax_span.range.start > cursor {
                runs.push(single_color_text_run(
                    syntax_span.range.start - cursor,
                    default_color,
                    font.clone(),
                ));
            }
            runs.push(single_color_text_run(
                syntax_span.range.len(),
                crate::app::render::markdown_syntax_color(theme, default_color, syntax_span.token),
                font.clone(),
            ));
            cursor = syntax_span.range.end;
        }
        if cursor < line.text.len() {
            runs.push(single_color_text_run(
                line.text.len() - cursor,
                default_color,
                font,
            ));
        }
        return runs;
    }

    if line.style_spans.is_empty() && line.link_ranges.is_empty() {
        return vec![single_color_text_run(
            line.text.len().max(1),
            default_color,
            font,
        )];
    }

    let mut boundaries = vec![0usize, line.text.len()];
    for span in line.style_spans.iter() {
        boundaries.push(span.range.start);
        boundaries.push(span.range.end);
    }
    for range in line.link_ranges.iter() {
        boundaries.push(range.range.start);
        boundaries.push(range.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let code_background =
        crate::app::theme::hunk_opacity(theme.secondary, theme.mode.is_dark(), 0.30, 0.42);
    let mut runs = Vec::new();

    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];
        if start >= end {
            continue;
        }

        let mut run_font = font.clone();
        let mut color = default_color;
        let mut background_color = None;
        let mut underline = None;
        let mut strikethrough = None;

        for span in line
            .style_spans
            .iter()
            .filter(|span| span.range.start <= start && span.range.end >= end)
        {
            if let Some(color_role) = span.color_role {
                color = ai_workspace_color_for_role(theme, color_role);
            }
            if span.bold {
                run_font.weight = FontWeight::SEMIBOLD;
            }
            if span.italic {
                run_font.style = gpui::FontStyle::Italic;
            }
            if span.code {
                background_color = Some(code_background);
                run_font.weight = FontWeight::MEDIUM;
            }
            if span.link {
                color = link_color;
                underline = Some(gpui::UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(link_color),
                    wavy: false,
                });
            }
            if span.strikethrough {
                strikethrough = Some(gpui::StrikethroughStyle {
                    thickness: px(1.0),
                    color: Some(color),
                });
            }
        }

        if underline.is_none()
            && line
                .link_ranges
                .iter()
                .any(|range| range.range.start <= start && range.range.end >= end)
        {
            color = link_color;
            underline = Some(gpui::UnderlineStyle {
                thickness: px(1.0),
                color: Some(link_color),
                wavy: false,
            });
        }

        if strikethrough.is_some() {
            strikethrough = Some(gpui::StrikethroughStyle {
                thickness: px(1.0),
                color: Some(color),
            });
        }

        runs.push(TextRun {
            len: end - start,
            color,
            font: run_font,
            background_color,
            underline,
            strikethrough,
        });
    }

    if runs.is_empty() {
        vec![single_color_text_run(
            line.text.len().max(1),
            default_color,
            font,
        )]
    } else {
        runs
    }
}

fn ai_workspace_color_for_role(
    theme: &gpui_component::Theme,
    color_role: ai_workspace_session::AiWorkspacePreviewColorRole,
) -> gpui::Hsla {
    let is_dark = theme.mode.is_dark();
    let line_stats = crate::app::theme::hunk_line_stats(theme, is_dark);
    match color_role {
        ai_workspace_session::AiWorkspacePreviewColorRole::Accent => theme.accent,
        ai_workspace_session::AiWorkspacePreviewColorRole::Added => line_stats.added,
        ai_workspace_session::AiWorkspacePreviewColorRole::Removed => line_stats.removed,
        ai_workspace_session::AiWorkspacePreviewColorRole::Foreground => theme.foreground,
        ai_workspace_session::AiWorkspacePreviewColorRole::Muted => theme.muted_foreground,
    }
}

fn ai_workspace_preview_line_background(
    preview_kind: ai_workspace_session::AiWorkspacePreviewLineKind,
    _theme: &gpui_component::Theme,
    _is_dark: bool,
) -> Option<gpui::Hsla> {
    match preview_kind {
        _ => None,
    }
}

fn ai_workspace_block_palette(
    kind: ai_workspace_session::AiWorkspaceBlockKind,
    role: ai_workspace_session::AiWorkspaceBlockRole,
    selected: bool,
    hovered: bool,
    is_dark: bool,
    cx: &App,
) -> (
    gpui::Hsla,
    gpui::Hsla,
    gpui::Hsla,
    gpui::Hsla,
    gpui::Hsla,
    gpui::Hsla,
) {
    if kind == ai_workspace_session::AiWorkspaceBlockKind::Message {
        let transparent = gpui::hsla(0.0, 0.0, 0.0, 0.0);
        let link_color = cx.theme().primary;
        return (
            transparent,
            transparent,
            transparent,
            cx.theme().foreground,
            cx.theme().foreground,
            link_color,
        );
    }

    let disclosure_colors = crate::app::theme::hunk_disclosure_row(cx.theme(), is_dark);
    let accent = disclosure_colors.chevron;
    let (background, border) = match kind {
        ai_workspace_session::AiWorkspaceBlockKind::Plan => (
            if hovered {
                crate::app::theme::hunk_disclosure_row(cx.theme(), is_dark).hover_background
            } else {
                crate::app::theme::hunk_blend(
                    cx.theme().background,
                    cx.theme().muted,
                    is_dark,
                    0.14,
                    0.18,
                )
            },
            crate::app::theme::hunk_opacity(cx.theme().border, is_dark, 0.80, 0.70),
        ),
        ai_workspace_session::AiWorkspaceBlockKind::Tool
        | ai_workspace_session::AiWorkspaceBlockKind::Group
        | ai_workspace_session::AiWorkspaceBlockKind::DiffSummary
        | ai_workspace_session::AiWorkspaceBlockKind::Status => (
            if hovered {
                crate::app::theme::hunk_disclosure_row(cx.theme(), is_dark).hover_background
            } else {
                gpui::hsla(0.0, 0.0, 0.0, 0.0)
            },
            if selected {
                crate::app::theme::hunk_opacity(cx.theme().border, is_dark, 0.82, 0.70)
            } else {
                gpui::hsla(0.0, 0.0, 0.0, 0.0)
            },
        ),
        ai_workspace_session::AiWorkspaceBlockKind::Message => unreachable!(),
    };
    let title_color = if kind == ai_workspace_session::AiWorkspaceBlockKind::Plan {
        cx.theme().foreground
    } else {
        disclosure_colors.title
    };
    let preview_color = if role == ai_workspace_session::AiWorkspaceBlockRole::User {
        cx.theme().foreground
    } else {
        cx.theme().muted_foreground
    };
    let link_color = cx.theme().primary;
    (
        background,
        border,
        accent,
        title_color,
        preview_color,
        link_color,
    )
}
