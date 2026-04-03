use gpui::{App, SharedString, TextRun, Window, point, px};

struct DiffCellRenderSpec {
    side: &'static str,
    line: Option<u32>,
    cell_kind: DiffCellKind,
    peer_kind: DiffCellKind,
    panel_width: Option<Pixels>,
}

#[derive(Clone)]
struct ReviewWorkspaceCodeRowCellPaint {
    panel_width: Option<gpui::Pixels>,
    line_number_width: f32,
    background: gpui::Hsla,
    gutter_background: gpui::Hsla,
    gutter_divider: gpui::Hsla,
    text_color: gpui::Hsla,
    line_color: gpui::Hsla,
    marker_color: gpui::Hsla,
    marker: SharedString,
    line_number: SharedString,
    segments: Vec<crate::app::data::CachedStyledSegment>,
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
    let gutter_width = px(cell.line_number_width) + marker_width + px(16.0);

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

    let mut text = String::new();
    let mut text_runs = Vec::new();
    let changed_bg = hunk_opacity(cell.marker_color, cx.theme().mode.is_dark(), 0.20, 0.11);
    for segment in &cell.segments {
        let segment_text = segment.plain_text.as_ref();
        if segment_text.is_empty() {
            continue;
        }
        text.push_str(segment_text);
        text_runs.push(TextRun {
            len: segment_text.len(),
            color: diff_syntax_color(cx.theme(), cell.text_color, segment.syntax),
            font: font.clone(),
            background_color: segment.changed.then_some(changed_bg),
            underline: None,
            strikethrough: None,
        });
    }
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
    crate::app::native_files_editor::paint::paint_editor_line(
        window,
        cx,
        &text_shape,
        point(text_origin_x, text_origin_y),
        line_height,
    );
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

impl DiffViewer {
    fn build_review_workspace_code_row_cell(
        &self,
        row_stable_id: u64,
        row_is_selected: bool,
        spec: DiffCellRenderSpec,
        viewport_row: &review_workspace_session::ReviewWorkspaceViewportRow,
        cx: &mut Context<Self>,
    ) -> ReviewWorkspaceCodeRowCellPaint {
        let side = spec.side;
        let cell_kind = spec.cell_kind;
        let peer_kind = spec.peer_kind;
        let is_dark = cx.theme().mode.is_dark();
        let chrome = hunk_diff_chrome(cx.theme(), is_dark);
        let dark_add_tint: gpui::Hsla = gpui::rgb(0x2e4736).into();
        let dark_remove_tint: gpui::Hsla = gpui::rgb(0x4a3038).into();
        let dark_add_accent: gpui::Hsla = gpui::rgb(0x8fcea0).into();
        let dark_remove_accent: gpui::Hsla = gpui::rgb(0xeea9b4).into();

        let (mut background, marker_color, line_color, text_color, marker) =
            match (cell_kind, peer_kind) {
                (DiffCellKind::Added, _) => (
                    hunk_pick(
                        is_dark,
                        cx.theme().background.blend(dark_add_tint.opacity(0.62)),
                        hunk_blend(cx.theme().background, cx.theme().success, is_dark, 0.24, 0.11),
                    ),
                    hunk_pick(is_dark, dark_add_accent, cx.theme().success.darken(0.18)),
                    hunk_pick(
                        is_dark,
                        dark_add_accent.lighten(0.08),
                        cx.theme().success.darken(0.16),
                    ),
                    cx.theme().foreground,
                    "+",
                ),
                (DiffCellKind::Removed, _) => (
                    hunk_pick(
                        is_dark,
                        cx.theme().background.blend(dark_remove_tint.opacity(0.62)),
                        hunk_blend(cx.theme().background, cx.theme().danger, is_dark, 0.24, 0.11),
                    ),
                    hunk_pick(is_dark, dark_remove_accent, cx.theme().danger.darken(0.18)),
                    hunk_pick(
                        is_dark,
                        dark_remove_accent.lighten(0.06),
                        cx.theme().danger.darken(0.16),
                    ),
                    cx.theme().foreground,
                    "-",
                ),
                (DiffCellKind::Context, _) => (
                    cx.theme().background,
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.14, 0.10),
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.18, 0.12),
                    cx.theme().foreground,
                    "",
                ),
                (DiffCellKind::None, _) => (
                    cx.theme().background,
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.14, 0.10),
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.18, 0.12),
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.08, 0.06),
                    "",
                ),
            };
        if matches!(cell_kind, DiffCellKind::Context | DiffCellKind::None)
            && row_stable_id.is_multiple_of(2)
        {
            background = hunk_blend(background, cx.theme().muted, is_dark, 0.06, 0.10);
        }
        if row_is_selected {
            background = hunk_blend(background, cx.theme().primary, is_dark, 0.22, 0.13);
        }

        let segments = if side == "left" {
            viewport_row.left_segments.clone()
        } else {
            viewport_row.right_segments.clone()
        };

        let mut gutter_background = match cell_kind {
            DiffCellKind::Added => {
                hunk_blend(chrome.gutter_background, cx.theme().success, is_dark, 0.12, 0.07)
            }
            DiffCellKind::Removed => {
                hunk_blend(chrome.gutter_background, cx.theme().danger, is_dark, 0.12, 0.07)
            }
            DiffCellKind::None => chrome.empty_gutter_background,
            DiffCellKind::Context => chrome.gutter_background,
        };
        if row_is_selected {
            gutter_background =
                hunk_blend(gutter_background, cx.theme().primary, is_dark, 0.14, 0.10);
        }

        ReviewWorkspaceCodeRowCellPaint {
            panel_width: spec.panel_width,
            line_number_width: if side == "left" {
                self.review_surface.diff_left_line_number_width
            } else {
                self.review_surface.diff_right_line_number_width
            },
            background,
            gutter_background,
            gutter_divider: chrome.gutter_divider,
            text_color,
            line_color,
            marker_color,
            marker: SharedString::from(marker),
            line_number: SharedString::from(spec.line.map(|line| line.to_string()).unwrap_or_default()),
            segments,
        }
    }

    fn build_review_workspace_meta_row_paint(
        &self,
        row_kind: DiffRowKind,
        row_text: &str,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> ReviewWorkspaceMetaRowPaint {
        let is_dark = cx.theme().mode.is_dark();

        let (background, foreground, accent) = match row_kind {
            DiffRowKind::HunkHeader => (
                if is_selected {
                    hunk_opacity(cx.theme().primary, is_dark, 0.34, 0.18)
                } else {
                    hunk_opacity(cx.theme().muted, is_dark, 0.26, 0.40)
                },
                cx.theme().primary_foreground,
                cx.theme().primary,
            ),
            DiffRowKind::Meta => {
                let line = row_text;
                if line.starts_with("new file mode") || line.starts_with("+++ b/") {
                    (
                        hunk_blend(cx.theme().background, cx.theme().success, is_dark, 0.22, 0.12),
                        hunk_tone(cx.theme().success, is_dark, 0.45, 0.10),
                        cx.theme().success,
                    )
                } else if line.starts_with("deleted file mode") || line.starts_with("--- a/") {
                    (
                        hunk_blend(cx.theme().background, cx.theme().danger, is_dark, 0.22, 0.12),
                        hunk_tone(cx.theme().danger, is_dark, 0.45, 0.10),
                        cx.theme().danger,
                    )
                } else if line.starts_with("diff --git") {
                    (
                        hunk_blend(cx.theme().background, cx.theme().accent, is_dark, 0.18, 0.10),
                        cx.theme().foreground,
                        cx.theme().accent,
                    )
                } else {
                    (
                        cx.theme().muted,
                        cx.theme().muted_foreground,
                        cx.theme().border,
                    )
                }
            }
            DiffRowKind::Empty => (
                cx.theme().background,
                cx.theme().muted_foreground,
                cx.theme().border,
            ),
            DiffRowKind::Code => (
                cx.theme().background,
                cx.theme().foreground,
                cx.theme().border,
            ),
        };
        let background = if row_kind == DiffRowKind::HunkHeader {
            background
        } else if is_selected {
            hunk_blend(background, cx.theme().primary, is_dark, 0.24, 0.14)
        } else {
            background
        };

        ReviewWorkspaceMetaRowPaint {
            kind: row_kind,
            text: row_text.to_string().into(),
            background,
            foreground,
            accent,
            border: hunk_opacity(cx.theme().border, is_dark, 0.82, 0.70),
        }
    }
}
