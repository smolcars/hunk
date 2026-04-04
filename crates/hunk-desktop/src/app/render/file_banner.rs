use crate::app::native_files_editor::paint::{
    paint_editor_line, shape_editor_line, single_color_text_run,
};

#[derive(Clone)]
struct ReviewWorkspaceFileHeaderPaint {
    row_background: gpui::Hsla,
    row_divider: gpui::Hsla,
    accent_strip: gpui::Hsla,
    badge_background: gpui::Hsla,
    badge_border: gpui::Hsla,
    badge_label: SharedString,
    badge_text_color: gpui::Hsla,
    path: SharedString,
    path_text_color: gpui::Hsla,
    stats_label: SharedString,
    stats_label_color: gpui::Hsla,
    stats_added: SharedString,
    stats_added_color: gpui::Hsla,
    stats_removed: SharedString,
    stats_removed_color: gpui::Hsla,
    stats_changed: SharedString,
    stats_changed_color: gpui::Hsla,
    collapse_label: SharedString,
    collapse_text_color: gpui::Hsla,
    control_background: gpui::Hsla,
    control_border: gpui::Hsla,
    view_label: SharedString,
    view_text_color: gpui::Hsla,
    view_background: gpui::Hsla,
}

fn build_review_workspace_file_header_paint(
    theme: &Theme,
    path: &str,
    status: FileStatus,
    stats: LineStats,
    is_selected: bool,
    is_collapsed: bool,
    can_view_file: bool,
) -> ReviewWorkspaceFileHeaderPaint {
    let is_dark = theme.mode.is_dark();
    let chrome = hunk_diff_chrome(theme, is_dark);
    let colors = hunk_file_status_banner(theme, status, is_dark, is_selected);
    let line_stats = hunk_line_stats(theme, is_dark);

    ReviewWorkspaceFileHeaderPaint {
        row_background: colors.row_background,
        row_divider: chrome.row_divider,
        accent_strip: colors.accent_strip,
        badge_background: colors.badge_background,
        badge_border: colors.badge_border,
        badge_label: SharedString::from(colors.label),
        badge_text_color: theme.foreground,
        path: SharedString::from(path.to_string()),
        path_text_color: theme.foreground,
        stats_label: SharedString::from("file"),
        stats_label_color: theme.muted_foreground,
        stats_added: SharedString::from(format!("+{}", stats.added)),
        stats_added_color: line_stats.added,
        stats_removed: SharedString::from(format!("-{}", stats.removed)),
        stats_removed_color: line_stats.removed,
        stats_changed: SharedString::from(format!("chg {}", stats.changed())),
        stats_changed_color: line_stats.changed,
        collapse_label: SharedString::from(if is_collapsed { ">" } else { "v" }),
        collapse_text_color: colors.arrow,
        control_background: hunk_blend(theme.background, theme.muted, is_dark, 0.18, 0.12),
        control_border: hunk_opacity(theme.border, is_dark, 0.88, 0.72),
        view_label: SharedString::from("View File"),
        view_text_color: if can_view_file {
            theme.foreground
        } else {
            hunk_opacity(theme.muted_foreground, is_dark, 0.80, 0.92)
        },
        view_background: if can_view_file {
            hunk_blend(theme.background, theme.muted, is_dark, 0.18, 0.12)
        } else {
            hunk_blend(theme.background, theme.muted, is_dark, 0.10, 0.06)
        },
    }
}

#[derive(Clone, Copy)]
struct ReviewWorkspaceFileHeaderControlsLayout {
    collapse_bounds: Bounds<Pixels>,
    view_bounds: Bounds<Pixels>,
}

fn review_workspace_file_header_controls_layout(
    bounds: Bounds<Pixels>,
) -> ReviewWorkspaceFileHeaderControlsLayout {
    let left_padding = px(12.0);
    let right_padding = px(12.0);
    let collapse_width = px(22.0);
    let collapse_height = px(22.0);
    let view_width = px(72.0);
    let view_height = px(22.0);

    ReviewWorkspaceFileHeaderControlsLayout {
        collapse_bounds: Bounds {
            origin: point(
                bounds.origin.x + left_padding,
                bounds.origin.y + ((bounds.size.height - collapse_height) / 2.).max(Pixels::ZERO),
            ),
            size: gpui::size(collapse_width, collapse_height),
        },
        view_bounds: Bounds {
            origin: point(
                bounds.origin.x + bounds.size.width - right_padding - view_width,
                bounds.origin.y + ((bounds.size.height - view_height) / 2.).max(Pixels::ZERO),
            ),
            size: gpui::size(view_width, view_height),
        },
    }
}

fn paint_review_workspace_file_header_row(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    paint: &ReviewWorkspaceFileHeaderPaint,
    mono_font_family: SharedString,
    ui_font_family: SharedString,
) {
    let left_padding = px(12.0);
    let collapse_button_reserve = px(30.0);
    let badge_gap = px(8.0);
    let right_padding = px(12.0);
    let stats_gap = px(8.0);
    let view_button_reserve = px(88.0);
    let badge_height = px(18.0);

    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_quad(gpui::fill(bounds, paint.row_background));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - px(1.0)),
                size: gpui::size(bounds.size.width, px(1.0)),
            },
            paint.row_divider,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: bounds.origin,
                size: gpui::size(px(2.0), bounds.size.height),
            },
            paint.accent_strip,
        ));

        let text_style = gpui::TextStyle {
            color: paint.path_text_color,
            font_family: mono_font_family.clone(),
            font_size: px(12.0).into(),
            line_height: gpui::relative(1.45),
            ..Default::default()
        };
        let font = text_style.font();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = text_style.line_height_in_pixels(window.rem_size());
        let text_y = bounds.origin.y + ((bounds.size.height - line_height) / 2.).max(Pixels::ZERO);

        let badge_runs = vec![single_color_text_run(
            paint.badge_label.len(),
            paint.badge_text_color,
            font.clone(),
        )];
        let badge_shape = shape_editor_line(window, paint.badge_label.clone(), font_size, &badge_runs);
        let badge_width = badge_shape.width() + px(12.0);
        let badge_x = bounds.origin.x + left_padding + collapse_button_reserve;
        let badge_y = bounds.origin.y + ((bounds.size.height - badge_height) / 2.).max(Pixels::ZERO);
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x, badge_y),
                size: gpui::size(badge_width, badge_height),
            },
            paint.badge_background,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x, badge_y),
                size: gpui::size(badge_width, px(1.0)),
            },
            paint.badge_border,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x, badge_y + badge_height - px(1.0)),
                size: gpui::size(badge_width, px(1.0)),
            },
            paint.badge_border,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x, badge_y),
                size: gpui::size(px(1.0), badge_height),
            },
            paint.badge_border,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x + badge_width - px(1.0), badge_y),
                size: gpui::size(px(1.0), badge_height),
            },
            paint.badge_border,
        ));
        paint_editor_line(
            window,
            cx,
            &badge_shape,
            point(
                badge_x + ((badge_width - badge_shape.width()) / 2.).max(Pixels::ZERO),
                text_y,
            ),
            line_height,
        );

        let stats_items = [
            (&paint.stats_changed, paint.stats_changed_color),
            (&paint.stats_removed, paint.stats_removed_color),
            (&paint.stats_added, paint.stats_added_color),
            (&paint.stats_label, paint.stats_label_color),
        ];
        let mut cursor_x = bounds.origin.x + bounds.size.width - right_padding - view_button_reserve;
        for (text, color) in stats_items {
            let runs = vec![single_color_text_run(text.len(), color, font.clone())];
            let shape = shape_editor_line(window, text.clone(), font_size, &runs);
            cursor_x -= shape.width();
            paint_editor_line(window, cx, &shape, point(cursor_x, text_y), line_height);
            cursor_x -= stats_gap;
        }

        let path_x = badge_x + badge_width + badge_gap;
        let path_right = (cursor_x - stats_gap).max(path_x);
        let path_bounds = Bounds {
            origin: point(path_x, bounds.origin.y),
            size: gpui::size((path_right - path_x).max(Pixels::ZERO), bounds.size.height),
        };
        let path_runs = vec![single_color_text_run(
            paint.path.len(),
            paint.path_text_color,
            font,
        )];
        let path_shape = shape_editor_line(window, paint.path.clone(), font_size, &path_runs);
        window.with_content_mask(Some(ContentMask { bounds: path_bounds }), |window| {
            paint_editor_line(window, cx, &path_shape, point(path_x, text_y), line_height);
        });

        let controls = review_workspace_file_header_controls_layout(bounds);
        window.paint_quad(gpui::fill(controls.collapse_bounds, paint.control_background));
        paint_review_workspace_outline(window, controls.collapse_bounds, paint.control_border);
        window.paint_quad(gpui::fill(controls.view_bounds, paint.view_background));
        paint_review_workspace_outline(window, controls.view_bounds, paint.control_border);

        let control_text_style = gpui::TextStyle {
            color: paint.view_text_color,
            font_family: ui_font_family.clone(),
            font_size: px(11.0).into(),
            line_height: gpui::relative(1.35),
            ..Default::default()
        };
        let control_font = control_text_style.font();
        let control_font_size = control_text_style.font_size.to_pixels(window.rem_size());
        let control_line_height = control_text_style.line_height_in_pixels(window.rem_size());

        let collapse_runs = vec![single_color_text_run(
            paint.collapse_label.len(),
            paint.collapse_text_color,
            control_font.clone(),
        )];
        let collapse_shape = shape_editor_line(
            window,
            paint.collapse_label.clone(),
            control_font_size,
            &collapse_runs,
        );
        paint_editor_line(
            window,
            cx,
            &collapse_shape,
            point(
                controls.collapse_bounds.origin.x
                    + ((controls.collapse_bounds.size.width - collapse_shape.width()) / 2.)
                        .max(Pixels::ZERO),
                controls.collapse_bounds.origin.y
                    + ((controls.collapse_bounds.size.height - control_line_height) / 2.)
                        .max(Pixels::ZERO),
            ),
            control_line_height,
        );

        let view_runs = vec![single_color_text_run(
            paint.view_label.len(),
            paint.view_text_color,
            control_font,
        )];
        let view_shape = shape_editor_line(
            window,
            paint.view_label.clone(),
            control_font_size,
            &view_runs,
        );
        paint_editor_line(
            window,
            cx,
            &view_shape,
            point(
                controls.view_bounds.origin.x
                    + ((controls.view_bounds.size.width - view_shape.width()) / 2.)
                        .max(Pixels::ZERO),
                controls.view_bounds.origin.y
                    + ((controls.view_bounds.size.height - control_line_height) / 2.)
                        .max(Pixels::ZERO),
            ),
            control_line_height,
        );
    });
}
