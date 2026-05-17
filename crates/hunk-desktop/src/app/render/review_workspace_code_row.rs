use gpui::{App, SharedString, TextRun, Window, point, px};

struct DiffCellRenderSpec {
    side: &'static str,
    line: Option<u32>,
    cell_kind: DiffCellKind,
    peer_kind: DiffCellKind,
    panel_width: Option<Pixels>,
    horizontal_offset: Pixels,
}

#[derive(Clone)]
struct ReviewWorkspaceCodeRowCellPaint {
    panel_width: Option<gpui::Pixels>,
    line_number_width: f32,
    horizontal_offset: gpui::Pixels,
    background: gpui::Hsla,
    gutter_background: gpui::Hsla,
    gutter_divider: gpui::Hsla,
    text_color: gpui::Hsla,
    line_color: gpui::Hsla,
    marker_color: gpui::Hsla,
    marker: SharedString,
    line_number: SharedString,
    display_row: hunk_editor::WorkspaceDisplayRow,
    syntax_spans: Vec<crate::app::native_files_editor::paint::RowSyntaxSpan>,
    changed_ranges: Vec<std::ops::Range<usize>>,
}

#[derive(Clone)]
struct ReviewWorkspaceMetaRowPaint {
    kind: DiffRowKind,
    text: SharedString,
    background: gpui::Hsla,
    foreground: gpui::Hsla,
    accent: gpui::Hsla,
    border: gpui::Hsla,
}

struct ReviewWorkspaceTextRunStyle {
    default_foreground: gpui::Hsla,
    font: gpui::Font,
    changed_bg: gpui::Hsla,
    search_bg: gpui::Hsla,
}

fn paint_review_workspace_code_row(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    left: &ReviewWorkspaceCodeRowCellPaint,
    right: &ReviewWorkspaceCodeRowCellPaint,
    center_divider: gpui::Hsla,
    mono_font_family: SharedString,
) {
    let left_width = left.panel_width.unwrap_or(bounds.size.width / 2.);
    let right_width = right
        .panel_width
        .unwrap_or((bounds.size.width - left_width).max(Pixels::ZERO));
    let left_bounds = Bounds {
        origin: bounds.origin,
        size: gpui::size(left_width, bounds.size.height),
    };
    let right_bounds = Bounds {
        origin: point(bounds.origin.x + left_width, bounds.origin.y),
        size: gpui::size(right_width, bounds.size.height),
    };

    paint_review_workspace_code_cell(
        window,
        cx,
        left_bounds,
        left,
        true,
        center_divider,
        mono_font_family.clone(),
    );
    paint_review_workspace_code_cell(
        window,
        cx,
        right_bounds,
        right,
        false,
        center_divider,
        mono_font_family,
    );
    window.paint_quad(gpui::fill(
        Bounds {
            origin: point(right_bounds.origin.x - px(1.0), bounds.origin.y),
            size: gpui::size(px(1.0), bounds.size.height),
        },
        center_divider,
    ));
}

fn paint_review_workspace_code_cell(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    cell: &ReviewWorkspaceCodeRowCellPaint,
    draw_right_divider: bool,
    center_divider: gpui::Hsla,
    mono_font_family: SharedString,
) {
    let padding_x = px(8.0);
    let gutter_padding_x = px(8.0);
    let marker_width = px(DIFF_MARKER_GUTTER_WIDTH);
    let gutter_width = review_workspace_code_cell_gutter_width(cell.line_number_width);

    window.paint_quad(gpui::fill(bounds, cell.background));
    let gutter_bounds = Bounds {
        origin: bounds.origin,
        size: gpui::size(gutter_width.min(bounds.size.width), bounds.size.height),
    };
    window.paint_quad(gpui::fill(gutter_bounds, cell.gutter_background));

    let gutter_divider_x = gutter_bounds.origin.x + gutter_bounds.size.width - px(1.0);
    window.paint_quad(gpui::fill(
        Bounds {
            origin: point(gutter_divider_x, gutter_bounds.origin.y),
            size: gpui::size(px(1.0), gutter_bounds.size.height),
        },
        cell.gutter_divider,
    ));

    if draw_right_divider {
        let divider_x = bounds.origin.x + bounds.size.width - px(1.0);
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(divider_x, bounds.origin.y),
                size: gpui::size(px(1.0), bounds.size.height),
            },
            center_divider,
        ));
    }

    let text_style = gpui::TextStyle {
        color: cell.text_color,
        font_family: mono_font_family,
        font_size: px(12.0).into(),
        line_height: gpui::relative(1.45),
        ..Default::default()
    };
    let font = text_style.font();
    let font_size = text_style.font_size.to_pixels(window.rem_size());
    let line_height = text_style.line_height_in_pixels(window.rem_size());
    let text_origin_y = bounds.origin.y + ((bounds.size.height - line_height) / 2.).max(Pixels::ZERO);

    let line_number_runs = vec![crate::app::native_files_editor::paint::single_color_text_run(
        cell.line_number.len(),
        cell.line_color,
        font.clone(),
    )];
    let line_number_shape = crate::app::native_files_editor::paint::shape_editor_line(
        window,
        cell.line_number.clone(),
        font_size,
        &line_number_runs,
    );
    let line_number_x = gutter_bounds.origin.x
        + gutter_padding_x
        + (px(cell.line_number_width) - line_number_shape.width()).max(Pixels::ZERO);
    crate::app::native_files_editor::paint::paint_editor_line(
        window,
        cx,
        &line_number_shape,
        point(line_number_x, text_origin_y),
        line_height,
    );

    let marker_runs = vec![crate::app::native_files_editor::paint::single_color_text_run(
        cell.marker.len(),
        cell.marker_color,
        font.clone(),
    )];
    let marker_shape = crate::app::native_files_editor::paint::shape_editor_line(
        window,
        cell.marker.clone(),
        font_size,
        &marker_runs,
    );
    let marker_origin_x =
        gutter_bounds.origin.x + gutter_padding_x + px(cell.line_number_width) + px(8.0);
    let marker_x = marker_origin_x + ((marker_width - marker_shape.width()) / 2.).max(Pixels::ZERO);
    crate::app::native_files_editor::paint::paint_editor_line(
        window,
        cx,
        &marker_shape,
        point(marker_x, text_origin_y),
        line_height,
    );

    let changed_bg = hunk_opacity(cell.marker_color, cx.theme().mode.is_dark(), 0.20, 0.11);
    let search_bg = hunk_opacity(
        hunk_text_selection_background(cx.theme(), cx.theme().mode.is_dark()),
        cx.theme().mode.is_dark(),
        0.42,
        0.26,
    );
    let mut text_runs = build_review_workspace_text_runs(
        cx,
        &cell.display_row,
        &cell.syntax_spans,
        &cell.changed_ranges,
        ReviewWorkspaceTextRunStyle {
            default_foreground: cell.text_color,
            font: font.clone(),
            changed_bg,
            search_bg,
        },
    );
    let mut text = cell.display_row.text.clone();
    if text_runs.is_empty() {
        text.push(' ');
        text_runs.push(TextRun {
            len: 1,
            color: cell.text_color,
            font: font.clone(),
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    let text_shape = crate::app::native_files_editor::paint::shape_editor_line(
        window,
        text.into(),
        font_size,
        &text_runs,
    );
    let text_origin_x = gutter_bounds.origin.x + gutter_bounds.size.width + padding_x;
    let text_mask_bounds = Bounds {
        origin: point(text_origin_x, bounds.origin.y),
        size: gpui::size(
            (bounds.right() - text_origin_x).max(Pixels::ZERO),
            bounds.size.height,
        ),
    };
    window.with_content_mask(Some(ContentMask { bounds: text_mask_bounds }), |window| {
        crate::app::native_files_editor::paint::paint_editor_line(
            window,
            cx,
            &text_shape,
            point(text_origin_x + cell.horizontal_offset, text_origin_y),
            line_height,
        );
    });
}

fn paint_review_workspace_meta_row(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    meta: &ReviewWorkspaceMetaRowPaint,
    mono_font_family: SharedString,
) {
    if meta.kind == DiffRowKind::HunkHeader {
        window.paint_quad(gpui::fill(bounds, meta.background));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - px(1.0)),
                size: gpui::size(bounds.size.width, px(1.0)),
            },
            meta.border,
        ));
        return;
    }

    window.paint_quad(gpui::fill(bounds, meta.background));
    window.paint_quad(gpui::fill(
        Bounds {
            origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - px(1.0)),
            size: gpui::size(bounds.size.width, px(1.0)),
        },
        meta.border,
    ));
    window.paint_quad(gpui::fill(
        Bounds {
            origin: bounds.origin,
            size: gpui::size(px(2.0), bounds.size.height),
        },
        meta.accent,
    ));

    let text_style = gpui::TextStyle {
        color: meta.foreground,
        font_family: mono_font_family,
        font_size: px(12.0).into(),
        line_height: gpui::relative(1.45),
        ..Default::default()
    };
    let font = text_style.font();
    let font_size = text_style.font_size.to_pixels(window.rem_size());
    let line_height = text_style.line_height_in_pixels(window.rem_size());
    let text_runs = vec![crate::app::native_files_editor::paint::single_color_text_run(
        meta.text.len(),
        meta.foreground,
        font,
    )];
    let shape = crate::app::native_files_editor::paint::shape_editor_line(
        window,
        meta.text.clone(),
        font_size,
        &text_runs,
    );
    let text_y = bounds.origin.y + ((bounds.size.height - line_height) / 2.).max(Pixels::ZERO);
    crate::app::native_files_editor::paint::paint_editor_line(
        window,
        cx,
        &shape,
        point(bounds.origin.x + px(12.0), text_y),
        line_height,
    );
}

fn build_review_workspace_code_row_cell_paint(
    theme: &Theme,
    line_number_width: f32,
    row_stable_id: u64,
    row_is_selected: bool,
    spec: DiffCellRenderSpec,
    viewport_row: &review_workspace_session::ReviewWorkspaceViewportRow,
) -> ReviewWorkspaceCodeRowCellPaint {
    let side = spec.side;
    let cell_kind = spec.cell_kind;
    let peer_kind = spec.peer_kind;
    let is_dark = theme.mode.is_dark();
    let chrome = hunk_diff_chrome(theme, is_dark);
    let dark_add_tint: gpui::Hsla = gpui::rgb(0x2e4736).into();
    let dark_remove_tint: gpui::Hsla = gpui::rgb(0x4a3038).into();
    let dark_add_accent: gpui::Hsla = gpui::rgb(0x8fcea0).into();
    let dark_remove_accent: gpui::Hsla = gpui::rgb(0xeea9b4).into();

    let (mut background, marker_color, line_color, text_color, marker) =
        match (cell_kind, peer_kind) {
            (DiffCellKind::Added, _) => (
                hunk_pick(
                    is_dark,
                    theme.background.blend(dark_add_tint.opacity(0.62)),
                    hunk_blend(theme.background, theme.success, is_dark, 0.24, 0.11),
                ),
                hunk_pick(is_dark, dark_add_accent, theme.success.darken(0.18)),
                hunk_pick(
                    is_dark,
                    dark_add_accent.lighten(0.08),
                    theme.success.darken(0.16),
                ),
                theme.foreground,
                "+",
            ),
            (DiffCellKind::Removed, _) => (
                hunk_pick(
                    is_dark,
                    theme.background.blend(dark_remove_tint.opacity(0.62)),
                    hunk_blend(theme.background, theme.danger, is_dark, 0.24, 0.11),
                ),
                hunk_pick(is_dark, dark_remove_accent, theme.danger.darken(0.18)),
                hunk_pick(
                    is_dark,
                    dark_remove_accent.lighten(0.06),
                    theme.danger.darken(0.16),
                ),
                theme.foreground,
                "-",
            ),
            (DiffCellKind::Context, _) => (
                theme.background,
                hunk_tone(theme.muted_foreground, is_dark, 0.14, 0.10),
                hunk_tone(theme.muted_foreground, is_dark, 0.18, 0.12),
                theme.foreground,
                "",
            ),
            (DiffCellKind::None, _) => (
                theme.background,
                hunk_tone(theme.muted_foreground, is_dark, 0.14, 0.10),
                hunk_tone(theme.muted_foreground, is_dark, 0.18, 0.12),
                hunk_tone(theme.muted_foreground, is_dark, 0.08, 0.06),
                "",
            ),
        };
    if matches!(cell_kind, DiffCellKind::Context | DiffCellKind::None)
        && row_stable_id.is_multiple_of(2)
    {
        background = hunk_blend(background, theme.muted, is_dark, 0.06, 0.10);
    }
    if row_is_selected {
        background = hunk_blend(background, theme.primary, is_dark, 0.22, 0.13);
    }

    let (display_row, syntax_spans, changed_ranges) = if side == "left" {
        (
            viewport_row.left_cell.display_row.clone(),
            viewport_row.left_cell.syntax_spans.clone(),
            viewport_row.left_cell.changed_ranges.clone(),
        )
    } else {
        (
            viewport_row.right_cell.display_row.clone(),
            viewport_row.right_cell.syntax_spans.clone(),
            viewport_row.right_cell.changed_ranges.clone(),
        )
    };

    let mut gutter_background = match cell_kind {
        DiffCellKind::Added => {
            hunk_blend(chrome.gutter_background, theme.success, is_dark, 0.12, 0.07)
        }
        DiffCellKind::Removed => {
            hunk_blend(chrome.gutter_background, theme.danger, is_dark, 0.12, 0.07)
        }
        DiffCellKind::None => chrome.empty_gutter_background,
        DiffCellKind::Context => chrome.gutter_background,
    };
    if row_is_selected {
        gutter_background = hunk_blend(gutter_background, theme.primary, is_dark, 0.14, 0.10);
    }

    ReviewWorkspaceCodeRowCellPaint {
        panel_width: spec.panel_width,
        line_number_width,
        horizontal_offset: spec.horizontal_offset,
        background,
        gutter_background,
        gutter_divider: chrome.gutter_divider,
        text_color,
        line_color,
        marker_color,
        marker: SharedString::from(marker),
        line_number: SharedString::from(spec.line.map(|line| line.to_string()).unwrap_or_default()),
        display_row,
        syntax_spans,
        changed_ranges,
    }
}

fn review_workspace_code_cell_gutter_width(line_number_width: f32) -> Pixels {
    px(line_number_width) + px(DIFF_MARKER_GUTTER_WIDTH) + px(16.0)
}

fn build_review_workspace_meta_row_paint(
    theme: &Theme,
    row_kind: DiffRowKind,
    row_text: &str,
    is_selected: bool,
) -> ReviewWorkspaceMetaRowPaint {
    let is_dark = theme.mode.is_dark();

    let (background, foreground, accent) = match row_kind {
        DiffRowKind::HunkHeader => (
            if is_selected {
                hunk_opacity(theme.primary, is_dark, 0.34, 0.18)
            } else {
                hunk_opacity(theme.muted, is_dark, 0.26, 0.40)
            },
            theme.primary_foreground,
            theme.primary,
        ),
        DiffRowKind::Meta => {
            if row_text.starts_with("new file mode") || row_text.starts_with("+++ b/") {
                (
                    hunk_blend(theme.background, theme.success, is_dark, 0.22, 0.12),
                    hunk_tone(theme.success, is_dark, 0.45, 0.10),
                    theme.success,
                )
            } else if row_text.starts_with("deleted file mode") || row_text.starts_with("--- a/") {
                (
                    hunk_blend(theme.background, theme.danger, is_dark, 0.22, 0.12),
                    hunk_tone(theme.danger, is_dark, 0.45, 0.10),
                    theme.danger,
                )
            } else if row_text.starts_with("diff --git") {
                (
                    hunk_blend(theme.background, theme.accent, is_dark, 0.18, 0.10),
                    theme.foreground,
                    theme.accent,
                )
            } else {
                (theme.muted, theme.muted_foreground, theme.border)
            }
        }
        DiffRowKind::Empty => (theme.background, theme.muted_foreground, theme.border),
        DiffRowKind::Code => (theme.background, theme.foreground, theme.border),
    };
    let background = if row_kind == DiffRowKind::HunkHeader {
        background
    } else if is_selected {
        hunk_blend(background, theme.primary, is_dark, 0.24, 0.14)
    } else {
        background
    };

    ReviewWorkspaceMetaRowPaint {
        kind: row_kind,
        text: row_text.to_string().into(),
        background,
        foreground,
        accent,
        border: hunk_opacity(theme.border, is_dark, 0.82, 0.70),
    }
}

fn build_review_workspace_text_runs(
    cx: &App,
    display_row: &hunk_editor::WorkspaceDisplayRow,
    syntax_spans: &[crate::app::native_files_editor::paint::RowSyntaxSpan],
    changed_ranges: &[std::ops::Range<usize>],
    style: ReviewWorkspaceTextRunStyle,
) -> Vec<TextRun> {
    if display_row.text.is_empty() {
        return Vec::new();
    }

    let mut column_byte_offsets = Vec::with_capacity(display_row.text.chars().count() + 1);
    column_byte_offsets.push(0);
    for (byte_index, ch) in display_row.text.char_indices() {
        column_byte_offsets.push(byte_index + ch.len_utf8());
    }
    let total_columns = column_byte_offsets.len().saturating_sub(1);
    if total_columns == 0 {
        return Vec::new();
    }

    let mut boundaries = vec![0, total_columns];
    for span in syntax_spans {
        boundaries.push(span.start_column.min(total_columns));
        boundaries.push(span.end_column.min(total_columns));
    }
    for range in changed_ranges {
        if range.start < range.end {
            boundaries.push(range.start.min(total_columns));
            boundaries.push(range.end.min(total_columns));
        }
    }
    for highlight in &display_row.search_highlights {
        if highlight.start_column < highlight.end_column {
            boundaries.push(highlight.start_column.min(total_columns));
            boundaries.push(highlight.end_column.min(total_columns));
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut runs = Vec::new();
    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];
        if start >= end {
            continue;
        }

        let syntax = syntax_spans
            .iter()
            .find(|span| span.start_column <= start && start < span.end_column)
            .map(|span| {
                diff_syntax_color(
                    cx.theme(),
                    style.default_foreground,
                    review_workspace_syntax_token_for_style_key(span.style_key.as_str()),
                )
            })
            .unwrap_or(style.default_foreground);
        let background_color = if display_row
            .search_highlights
            .iter()
            .any(|highlight| highlight.start_column <= start && start < highlight.end_column)
        {
            Some(style.search_bg)
        } else if changed_ranges
            .iter()
            .any(|range| range.start <= start && start < range.end)
        {
            Some(style.changed_bg)
        } else {
            None
        };
        runs.push(TextRun {
            len: column_byte_offsets[end].saturating_sub(column_byte_offsets[start]),
            color: syntax,
            font: style.font.clone(),
            background_color,
            underline: None,
            strikethrough: None,
        });
    }

    if runs.is_empty() {
        runs.push(TextRun {
            len: display_row.text.len(),
            color: style.default_foreground,
            font: style.font,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    runs
}

fn review_workspace_syntax_token_for_style_key(
    style_key: &str,
) -> crate::app::highlight::SyntaxTokenKind {
    match style_key.split('.').next().unwrap_or_default() {
        "keyword" => crate::app::highlight::SyntaxTokenKind::Keyword,
        "string" => crate::app::highlight::SyntaxTokenKind::String,
        "number" => crate::app::highlight::SyntaxTokenKind::Number,
        "comment" => crate::app::highlight::SyntaxTokenKind::Comment,
        "function" => crate::app::highlight::SyntaxTokenKind::Function,
        "type" | "constructor" | "tag" => crate::app::highlight::SyntaxTokenKind::TypeName,
        "constant" | "attribute" | "boolean" => {
            crate::app::highlight::SyntaxTokenKind::Constant
        }
        "variable" | "property" | "parameter" => {
            crate::app::highlight::SyntaxTokenKind::Variable
        }
        "operator" | "punctuation" => crate::app::highlight::SyntaxTokenKind::Operator,
        _ => crate::app::highlight::SyntaxTokenKind::Plain,
    }
}
