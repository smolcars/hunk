use gpui::*;
use helix_view::graphics::CursorKind;
use helix_view::{Document, View};

use super::{DocumentLayout, LineNumberPaintParams};

pub(super) fn paint_current_line_background(
    window: &mut Window,
    origin: Point<Pixels>,
    layout: &DocumentLayout,
    first_row: usize,
    current_line: usize,
    background: Hsla,
) {
    if current_line < first_row {
        return;
    }
    let relative_row = current_line - first_row;
    if relative_row >= layout.rows {
        return;
    }
    let line_y = origin.y + px(1.0) + (layout.line_height * relative_row as f32);
    let content_x =
        origin.x + px(10.0) + (layout.cell_width * (layout.gutter_columns as f32 + 1.0));
    window.paint_quad(fill(
        Bounds {
            origin: point(content_x, line_y),
            size: size(px(10000.0), layout.line_height),
        },
        background,
    ));
}

pub(super) fn paint_selection_backgrounds(
    window: &mut Window,
    document: &Document,
    view: &View,
    text: helix_core::ropey::RopeSlice<'_>,
    origin: Point<Pixels>,
    layout: &DocumentLayout,
    background: Hsla,
) {
    let selection = document.selection(view.id);
    let content_origin = point(
        origin.x + px(10.0) + (layout.cell_width * (layout.gutter_columns as f32 + 1.0)),
        origin.y + px(1.0),
    );
    let content_width = (layout.hitbox.bounds.right() - content_origin.x).max(Pixels::ZERO);
    let max_col = (content_width / layout.cell_width) as usize;
    for range in selection.ranges() {
        if range.is_empty() {
            continue;
        }
        let Some(start) = view.screen_coords_at_pos(document, text, range.from()) else {
            continue;
        };
        let Some(end) = view.screen_coords_at_pos(document, text, range.to()) else {
            continue;
        };
        if start.row == end.row {
            paint_selection_segment(
                window,
                content_origin,
                layout,
                start.row,
                start.col,
                end.col,
                background,
            );
            continue;
        }
        paint_selection_segment(
            window,
            content_origin,
            layout,
            start.row,
            start.col,
            max_col,
            background,
        );
        for row in (start.row + 1)..end.row {
            paint_selection_segment(window, content_origin, layout, row, 0, max_col, background);
        }
        paint_selection_segment(window, content_origin, layout, end.row, 0, end.col, background);
    }
}

fn paint_selection_segment(
    window: &mut Window,
    content_origin: Point<Pixels>,
    layout: &DocumentLayout,
    row: usize,
    start_col: usize,
    end_col: usize,
    background: Hsla,
) {
    if end_col <= start_col {
        return;
    }
    let line_y = content_origin.y + (layout.line_height * row as f32);
    let line_x = content_origin.x + (layout.cell_width * start_col as f32);
    window.paint_quad(fill(
        Bounds {
            origin: point(line_x, line_y),
            size: size(layout.cell_width * (end_col - start_col) as f32, layout.line_height),
        },
        background,
    ));
}

pub(super) fn paint_line_numbers(
    window: &mut Window,
    cx: &mut App,
    layout: &DocumentLayout,
    params: LineNumberPaintParams,
) {
    let mut y = params.origin.y + px(1.0);
    for line_number in params.first_row..params.last_row {
        let color = if line_number == params.current_line {
            params.palette.current_line_number
        } else {
            params.palette.line_number
        };
        let text = format!("{:>digits$}", line_number + 1, digits = params.digits);
        let shaped = window.text_system().shape_line(
            text.clone().into(),
            layout.font_size,
            &[TextRun {
                len: text.len(),
                font: params.font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let _ = shaped.paint(
            point(params.origin.x, y),
            layout.line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        );
        y += layout.line_height;
    }
}

pub(super) fn palette_text_width(total_lines: usize) -> usize {
    total_lines.max(1).to_string().len()
}

pub(super) fn cursor_bounds(
    origin: Point<Pixels>,
    kind: CursorKind,
    cell_width: Pixels,
    line_height: Pixels,
) -> Bounds<Pixels> {
    match kind {
        CursorKind::Bar => Bounds {
            origin,
            size: size(px(2.0), line_height),
        },
        CursorKind::Block => Bounds {
            origin,
            size: size(cell_width, line_height),
        },
        CursorKind::Underline => Bounds {
            origin: origin + point(Pixels::ZERO, line_height - px(2.0)),
            size: size(cell_width, px(2.0)),
        },
        CursorKind::Hidden => Bounds {
            origin,
            size: size(Pixels::ZERO, Pixels::ZERO),
        },
    }
}

pub(super) fn mouse_text_position(
    view: &View,
    doc: &Document,
    position: Point<Pixels>,
    layout: &DocumentLayout,
) -> Option<usize> {
    let position = clamp_to_bounds(position, layout.hitbox.bounds);
    let origin = point(
        layout.hitbox.bounds.origin.x
            + px(10.0)
            + (layout.cell_width * (layout.gutter_columns as f32 + 1.0)),
        layout.hitbox.bounds.origin.y + px(1.0),
    );
    let row = ((position.y - origin.y).max(Pixels::ZERO) / layout.line_height) as u16;
    let column = ((position.x - origin.x).max(Pixels::ZERO) / layout.cell_width) as u16;
    view.pos_at_visual_coords(doc, row, column, true)
}

pub(super) fn clamp_to_bounds(position: Point<Pixels>, bounds: Bounds<Pixels>) -> Point<Pixels> {
    point(
        position.x.clamp(bounds.left(), bounds.right() - px(1.0)),
        position.y.clamp(bounds.top(), bounds.bottom() - px(1.0)),
    )
}
