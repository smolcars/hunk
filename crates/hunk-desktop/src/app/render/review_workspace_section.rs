#[derive(Clone, Copy)]
pub(crate) struct ReviewWorkspaceCommentAffordanceLayout {
    pub(crate) hit_bounds: Bounds<Pixels>,
    note_bounds: Bounds<Pixels>,
    badge_bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone)]
pub(crate) struct ReviewWorkspaceViewportPaintStyle {
    pub(crate) left_panel_width: Option<Pixels>,
    pub(crate) right_panel_width: Option<Pixels>,
    pub(crate) left_horizontal_offset: Pixels,
    pub(crate) right_horizontal_offset: Pixels,
    pub(crate) left_line_number_width: f32,
    pub(crate) right_line_number_width: f32,
    pub(crate) center_divider: gpui::Hsla,
    pub(crate) mono_font_family: SharedString,
    pub(crate) ui_font_family: SharedString,
}

pub(crate) fn paint_review_workspace_viewport_row(
    window: &mut Window,
    cx: &mut App,
    row_bounds: Bounds<Pixels>,
    viewport_row: &review_workspace_session::ReviewWorkspaceViewportRow,
    is_selected: bool,
    style: &ReviewWorkspaceViewportPaintStyle,
) {
    if viewport_row.stream_kind == DiffStreamRowKind::FileHeader {
        let Some(path) = viewport_row.file_path.as_deref() else {
            return;
        };
        let status = viewport_row.file_status.unwrap_or(FileStatus::Unknown);
        let stats = viewport_row.file_line_stats.unwrap_or_default();
        let paint = build_review_workspace_file_header_paint(
            cx.theme(),
            path,
            status,
            stats,
            is_selected,
            viewport_row.file_is_collapsed,
            viewport_row.can_view_file,
        );
        paint_review_workspace_file_header_row(
            window,
            cx,
            row_bounds,
            &paint,
            style.mono_font_family.clone(),
            style.ui_font_family.clone(),
        );
        return;
    }

    match viewport_row.row_kind {
        DiffRowKind::Code => {
            let left = build_review_workspace_code_row_cell_paint(
                cx.theme(),
                style.left_line_number_width,
                viewport_row.stable_id,
                is_selected,
                DiffCellRenderSpec {
                    side: "left",
                    line: viewport_row.left_line,
                    cell_kind: viewport_row.left_cell_kind,
                    peer_kind: viewport_row.right_cell_kind,
                    panel_width: style.left_panel_width,
                    horizontal_offset: style.left_horizontal_offset,
                },
                viewport_row,
            );
            let right = build_review_workspace_code_row_cell_paint(
                cx.theme(),
                style.right_line_number_width,
                viewport_row.stable_id,
                is_selected,
                DiffCellRenderSpec {
                    side: "right",
                    line: viewport_row.right_line,
                    cell_kind: viewport_row.right_cell_kind,
                    peer_kind: viewport_row.left_cell_kind,
                    panel_width: style.right_panel_width,
                    horizontal_offset: style.right_horizontal_offset,
                },
                viewport_row,
            );
            paint_review_workspace_code_row(
                window,
                cx,
                row_bounds,
                &left,
                &right,
                style.center_divider,
                style.mono_font_family.clone(),
            );
        }
        DiffRowKind::HunkHeader | DiffRowKind::Meta | DiffRowKind::Empty => {
            let meta = build_review_workspace_meta_row_paint(
                cx.theme(),
                viewport_row.row_kind,
                &viewport_row.text,
                is_selected,
            );
            paint_review_workspace_meta_row(
                window,
                cx,
                row_bounds,
                &meta,
                style.mono_font_family.clone(),
            );
        }
    }

    if let Some(comment_layout) = review_workspace_comment_affordance_layout(
        row_bounds,
        viewport_row.show_comment_affordance,
        viewport_row.open_comment_count,
    ) {
        paint_review_workspace_comment_affordance(
            window,
            cx,
            comment_layout,
            viewport_row.open_comment_count,
            style.ui_font_family.clone(),
        );
    }
}

pub(crate) fn paint_review_workspace_sticky_header(
    window: &mut Window,
    cx: &mut App,
    header: &review_workspace_session::ReviewWorkspaceVisibleFileHeader,
    is_selected: bool,
    can_view_file: bool,
    bounds: Bounds<Pixels>,
    style: &ReviewWorkspaceViewportPaintStyle,
) {
    let paint = build_review_workspace_file_header_paint(
        cx.theme(),
        header.path.as_str(),
        header.status,
        header.line_stats,
        is_selected,
        false,
        can_view_file,
    );
    paint_review_workspace_file_header_row(
        window,
        cx,
        bounds,
        &paint,
        style.mono_font_family.clone(),
        style.ui_font_family.clone(),
    );
}

pub(crate) fn review_workspace_row_is_selected(
    selected_row_range: Option<(usize, usize)>,
    row_index: usize,
) -> bool {
    selected_row_range
        .is_some_and(|(start, end)| row_index >= start && row_index <= end)
}

pub(crate) fn review_workspace_sticky_header_bounds(
    origin: gpui::Point<gpui::Pixels>,
    width: gpui::Pixels,
) -> Bounds<gpui::Pixels> {
    Bounds {
        origin,
        size: gpui::size(
            width,
            px(review_workspace_session::REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX as f32),
        ),
    }
}

pub(crate) fn review_workspace_comment_affordance_layout(
    row_bounds: Bounds<Pixels>,
    show_comment_affordance: bool,
    open_comment_count: usize,
) -> Option<ReviewWorkspaceCommentAffordanceLayout> {
    if !show_comment_affordance {
        return None;
    }

    let top_inset = px(4.0);
    let right_inset = px(8.0);
    let gap = px(4.0);
    let note_width = px(48.0);
    let note_height = px(20.0);
    let badge_height = px(18.0);
    let badge_width = if open_comment_count > 0 {
        px(((open_comment_count.to_string().len() as f32 * 7.0) + 12.0).max(18.0))
    } else {
        Pixels::ZERO
    };

    let note_bounds = Bounds {
        origin: point(
            row_bounds.origin.x + row_bounds.size.width - right_inset - note_width,
            row_bounds.origin.y
                + ((row_bounds.size.height - note_height) / 2.).max(top_inset),
        ),
        size: gpui::size(note_width, note_height),
    };
    let badge_bounds = (open_comment_count > 0).then_some(Bounds {
        origin: point(
            note_bounds.origin.x - gap - badge_width,
            row_bounds.origin.y + ((row_bounds.size.height - badge_height) / 2.).max(Pixels::ZERO),
        ),
        size: gpui::size(badge_width, badge_height),
    });
    let hit_left = badge_bounds
        .map(|bounds| bounds.origin.x)
        .unwrap_or(note_bounds.origin.x);
    let hit_top = badge_bounds
        .map(|bounds| bounds.origin.y.min(note_bounds.origin.y))
        .unwrap_or(note_bounds.origin.y);
    let hit_bottom = badge_bounds
        .map(|bounds| {
            (bounds.origin.y + bounds.size.height).max(note_bounds.origin.y + note_bounds.size.height)
        })
        .unwrap_or(note_bounds.origin.y + note_bounds.size.height);

    Some(ReviewWorkspaceCommentAffordanceLayout {
        hit_bounds: Bounds {
            origin: point(hit_left, hit_top),
            size: gpui::size(
                (note_bounds.origin.x + note_bounds.size.width - hit_left).max(Pixels::ZERO),
                (hit_bottom - hit_top).max(Pixels::ZERO),
            ),
        },
        note_bounds,
        badge_bounds,
    })
}

fn paint_review_workspace_comment_affordance(
    window: &mut Window,
    cx: &mut App,
    layout: ReviewWorkspaceCommentAffordanceLayout,
    open_comment_count: usize,
    ui_font_family: SharedString,
) {
    let is_dark = cx.theme().mode.is_dark();
    let note_background = hunk_blend(
        cx.theme().background,
        cx.theme().muted,
        is_dark,
        0.18,
        0.12,
    );
    let note_border = hunk_opacity(cx.theme().border, is_dark, 0.88, 0.72);
    let note_text = cx.theme().foreground;
    let badge_background = hunk_opacity(cx.theme().primary, is_dark, 0.34, 0.18);
    let badge_text = cx.theme().primary_foreground;
    let note_label = SharedString::from("Note");
    let badge_label = SharedString::from(open_comment_count.to_string());

    let text_style = gpui::TextStyle {
        color: note_text,
        font_family: ui_font_family.clone(),
        font_size: px(11.0).into(),
        line_height: gpui::relative(1.35),
        ..Default::default()
    };
    let font = text_style.font();
    let font_size = text_style.font_size.to_pixels(window.rem_size());
    let line_height = text_style.line_height_in_pixels(window.rem_size());

    window.paint_quad(gpui::fill(layout.note_bounds, note_background));
    paint_review_workspace_outline(window, layout.note_bounds, note_border);

    let note_runs = vec![crate::app::native_files_editor::paint::single_color_text_run(
        note_label.len(),
        note_text,
        font.clone(),
    )];
    let note_shape = crate::app::native_files_editor::paint::shape_editor_line(
        window,
        note_label,
        font_size,
        &note_runs,
    );
    crate::app::native_files_editor::paint::paint_editor_line(
        window,
        cx,
        &note_shape,
        point(
            layout.note_bounds.origin.x
                + ((layout.note_bounds.size.width - note_shape.width()) / 2.).max(Pixels::ZERO),
            layout.note_bounds.origin.y
                + ((layout.note_bounds.size.height - line_height) / 2.).max(Pixels::ZERO),
        ),
        line_height,
    );

    if let Some(badge_bounds) = layout.badge_bounds {
        window.paint_quad(gpui::fill(badge_bounds, badge_background));
        let badge_runs = vec![crate::app::native_files_editor::paint::single_color_text_run(
            badge_label.len(),
            badge_text,
            font,
        )];
        let badge_shape = crate::app::native_files_editor::paint::shape_editor_line(
            window,
            badge_label,
            font_size,
            &badge_runs,
        );
        crate::app::native_files_editor::paint::paint_editor_line(
            window,
            cx,
            &badge_shape,
            point(
                badge_bounds.origin.x
                    + ((badge_bounds.size.width - badge_shape.width()) / 2.).max(Pixels::ZERO),
                badge_bounds.origin.y
                    + ((badge_bounds.size.height - line_height) / 2.).max(Pixels::ZERO),
            ),
            line_height,
        );
    }
}

fn paint_review_workspace_outline(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    border: gpui::Hsla,
) {
    window.paint_quad(gpui::fill(
        Bounds {
            origin: bounds.origin,
            size: gpui::size(bounds.size.width, px(1.0)),
        },
        border,
    ));
    window.paint_quad(gpui::fill(
        Bounds {
            origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - px(1.0)),
            size: gpui::size(bounds.size.width, px(1.0)),
        },
        border,
    ));
    window.paint_quad(gpui::fill(
        Bounds {
            origin: bounds.origin,
            size: gpui::size(px(1.0), bounds.size.height),
        },
        border,
    ));
    window.paint_quad(gpui::fill(
        Bounds {
            origin: point(bounds.origin.x + bounds.size.width - px(1.0), bounds.origin.y),
            size: gpui::size(px(1.0), bounds.size.height),
        },
        border,
    ));
}
