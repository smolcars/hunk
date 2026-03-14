use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::*;
use helix_core::Position;
use helix_view::graphics::CursorKind;
use helix_view::{Document, View};

use super::{DocumentLayout, LineNumberPaintParams};

const INSERT_CURSOR_BLINK_PERIOD: Duration = Duration::from_millis(530);

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
    let primary_index = selection.primary_index();
    let content_origin = point(
        origin.x + px(10.0) + (layout.cell_width * (layout.gutter_columns as f32 + 1.0)),
        origin.y + px(1.0),
    );
    let content_width = (layout.hitbox.bounds.right() - content_origin.x).max(Pixels::ZERO);
    let max_col = (content_width / layout.cell_width) as usize;
    let Some((visible_start, _)) =
        visible_row_char_range(document, view, text, 0, layout.rows, max_col)
    else {
        return;
    };
    let Some((_, visible_end)) = visible_row_char_range(
        document,
        view,
        text,
        layout.rows.saturating_sub(1),
        layout.rows,
        max_col,
    ) else {
        return;
    };
    for (index, range) in selection.ranges().iter().enumerate() {
        if range.is_empty() {
            continue;
        }
        if index == primary_index && range.len() <= 1 {
            continue;
        }
        if range.to() <= visible_start || range.from() >= visible_end {
            continue;
        }
        let start = if range.from() <= visible_start {
            Position::new(0, 0)
        } else {
            view.screen_coords_at_pos(document, text, range.from())
                .unwrap_or_else(|| Position::new(0, 0))
        };
        let end = if range.to() >= visible_end {
            Position::new(layout.rows.saturating_sub(1), max_col)
        } else {
            view.screen_coords_at_pos(document, text, range.to())
                .unwrap_or_else(|| Position::new(layout.rows.saturating_sub(1), max_col))
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
        paint_selection_segment(
            window,
            content_origin,
            layout,
            end.row,
            0,
            end.col,
            background,
        );
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
            size: size(
                layout.cell_width * (end_col - start_col) as f32,
                layout.line_height,
            ),
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

pub(super) fn visible_row_char_range(
    document: &Document,
    view: &View,
    text: helix_core::ropey::RopeSlice<'_>,
    row: usize,
    rows: usize,
    max_col: usize,
) -> Option<(usize, usize)> {
    let row = row.min(u16::MAX as usize) as u16;
    let max_col = max_col.min(u16::MAX as usize) as u16;
    let start = view.pos_at_visual_coords(document, row, 0, true)?;
    let mut end = if (row as usize) + 1 < rows {
        view.pos_at_visual_coords(document, row.saturating_add(1), 0, true)
            .unwrap_or_else(|| text.len_chars())
    } else {
        view.pos_at_visual_coords(document, row, max_col, true)
            .unwrap_or_else(|| text.len_chars())
    }
    .min(text.len_chars());
    while end > start
        && text
            .get_char(end - 1)
            .is_some_and(|ch| matches!(ch, '\n' | '\r'))
    {
        end -= 1;
    }
    Some((start, end.max(start)))
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

pub(super) fn animated_cursor_kind(cursor_kind: CursorKind) -> CursorKind {
    match cursor_kind {
        CursorKind::Bar => {
            if insert_cursor_is_visible() {
                CursorKind::Bar
            } else {
                CursorKind::Hidden
            }
        }
        _ => cursor_kind,
    }
}

fn insert_cursor_is_visible() -> bool {
    static INSERT_CURSOR_EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = INSERT_CURSOR_EPOCH.get_or_init(Instant::now);
    (epoch.elapsed().as_millis() / INSERT_CURSOR_BLINK_PERIOD.as_millis()).is_multiple_of(2)
}

pub(super) struct CursorPaintParams<'a> {
    pub(super) document: &'a Document,
    pub(super) view: &'a View,
    pub(super) text: helix_core::ropey::RopeSlice<'a>,
    pub(super) content_origin: Point<Pixels>,
    pub(super) cell_width: Pixels,
    pub(super) line_height: Pixels,
    pub(super) kind: CursorKind,
    pub(super) color: Hsla,
}

pub(super) fn paint_cursors(window: &mut Window, params: CursorPaintParams<'_>) {
    let selection = params.document.selection(params.view.id);
    let primary_index = selection.primary_index();
    for (index, range) in selection.ranges().iter().enumerate() {
        let cursor = range.cursor(params.text);
        let Some(position) = params
            .view
            .screen_coords_at_pos(params.document, params.text, cursor)
        else {
            continue;
        };
        let bounds = cursor_bounds(
            point(
                params.content_origin.x + (params.cell_width * position.col as f32),
                params.content_origin.y + (params.line_height * position.row as f32),
            ),
            params.kind,
            params.cell_width,
            params.line_height,
        );
        let mut fill_color = params.color;
        fill_color.a = if index == primary_index { 0.55 } else { 0.32 };
        window.paint_quad(fill(bounds, fill_color));
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
