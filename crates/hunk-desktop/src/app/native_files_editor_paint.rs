use std::cmp::min;
use std::collections::BTreeMap;
use std::ops::Range;

use gpui::{
    App, Bounds, Font, Hitbox, Hsla, Pixels, Point, ShapedLine, SharedString, TextAlign, TextRun,
    Window, fill, point, px, size,
};
use gpui_component::ActiveTheme as _;
use hunk_editor::{DisplayRow, DisplayRowKind, FoldRegion};
use hunk_language::HighlightCapture;
use hunk_text::{TextPosition, TextSnapshot};

use super::{FilesEditorPalette, ScrollDirection};

#[derive(Clone)]
pub(crate) struct EditorLayout {
    pub(super) line_height: Pixels,
    pub(super) font_size: Pixels,
    pub(super) cell_width: Pixels,
    pub(super) gutter_columns: usize,
    pub(super) hitbox: Hitbox,
    pub(super) display_snapshot: hunk_editor::DisplaySnapshot,
}

impl EditorLayout {
    pub(super) fn content_origin_x(&self) -> Pixels {
        self.hitbox.bounds.origin.x
            + px(10.0)
            + (self.cell_width * (self.gutter_columns as f32 + 1.0))
    }

    pub(super) fn line_number_origin_x(&self) -> Pixels {
        self.hitbox.bounds.origin.x + self.cell_width + px(2.0)
    }

    pub(super) fn fold_marker_origin_x(&self) -> Pixels {
        self.hitbox.bounds.origin.x + px(2.0)
    }

    pub(super) fn fold_marker_bounds_for_row(
        &self,
        row_index: usize,
        visible_row_start: usize,
    ) -> Bounds<Pixels> {
        let y = self.hitbox.bounds.origin.y
            + (self.line_height * row_index.saturating_sub(visible_row_start) as f32);
        Bounds {
            origin: point(self.fold_marker_origin_x(), y),
            size: size(self.cell_width, self.line_height),
        }
    }
}

#[derive(Clone)]
pub(super) struct LineNumberPaintParams {
    pub(super) origin: Point<Pixels>,
    pub(super) current_line: usize,
    pub(super) palette: FilesEditorPalette,
    pub(super) font: Font,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowSyntaxSpan {
    pub(super) start_column: usize,
    pub(super) end_column: usize,
    pub(super) style_key: String,
}

#[derive(Clone, Copy)]
pub(crate) struct ResolvedSyntaxStyle {
    pub(super) color: Option<Hsla>,
    pub(super) background_color: Option<Hsla>,
    pub(super) underline: Option<gpui::UnderlineStyle>,
    pub(super) strikethrough: Option<gpui::StrikethroughStyle>,
    pub(super) font_weight: Option<gpui::FontWeight>,
    pub(super) font_style: Option<gpui::FontStyle>,
}

pub(super) fn current_line_text(snapshot: &TextSnapshot, line: usize) -> String {
    let start = snapshot.line_to_byte(line).unwrap_or(0);
    let end = if line + 1 < snapshot.line_count() {
        snapshot
            .line_to_byte(line + 1)
            .unwrap_or(snapshot.byte_len())
    } else {
        snapshot.byte_len()
    };
    snapshot
        .slice(start..end)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}

pub(super) fn last_position(snapshot: &TextSnapshot) -> Option<TextPosition> {
    let line = snapshot.line_count().checked_sub(1)?;
    Some(TextPosition::new(
        line,
        current_line_text(snapshot, line).chars().count(),
    ))
}

pub(super) fn uses_primary_shortcut(keystroke: &gpui::Keystroke) -> bool {
    if cfg!(target_os = "macos") {
        keystroke.modifiers.platform
    } else {
        keystroke.modifiers.control
    }
}

pub(crate) fn build_row_syntax_spans_for_row(
    row: &DisplayRow,
    captures: &[HighlightCapture],
    snapshot: &TextSnapshot,
) -> Vec<RowSyntaxSpan> {
    if !matches!(row.kind, DisplayRowKind::Text) || row.text.is_empty() {
        return Vec::new();
    }

    let Ok(row_start) =
        snapshot.position_to_byte(TextPosition::new(row.source_line, row.raw_start_column))
    else {
        return Vec::new();
    };
    let Ok(row_end) =
        snapshot.position_to_byte(TextPosition::new(row.source_line, row.raw_end_column))
    else {
        return Vec::new();
    };

    let mut row_spans = Vec::new();
    let mut scan_index = captures.partition_point(|capture| capture.byte_range.end <= row_start);
    while scan_index < captures.len() {
        let capture = &captures[scan_index];
        if capture.byte_range.start >= row_end {
            break;
        }

        let start = capture.byte_range.start.max(row_start);
        let end = capture.byte_range.end.min(row_end);
        if start < end {
            let Ok(start_position) = snapshot.byte_to_position(start) else {
                scan_index += 1;
                continue;
            };
            let Ok(end_position) = snapshot.byte_to_position(end) else {
                scan_index += 1;
                continue;
            };
            if start_position.line == row.source_line && end_position.line == row.source_line {
                let start_column = display_column_for_raw(row, start_position.column);
                let end_column = display_column_for_raw(row, end_position.column);
                if start_column < end_column {
                    push_compacted_row_syntax_span(
                        &mut row_spans,
                        RowSyntaxSpan {
                            start_column,
                            end_column,
                            style_key: capture.style_key.clone(),
                        },
                    );
                }
            }
        }
        scan_index += 1;
    }

    row_spans
}

fn push_compacted_row_syntax_span(spans: &mut Vec<RowSyntaxSpan>, next: RowSyntaxSpan) {
    if let Some(previous) = spans.last_mut()
        && previous.style_key == next.style_key
        && previous.end_column >= next.start_column
    {
        previous.end_column = previous.end_column.max(next.end_column);
        return;
    }

    spans.push(next);
}

pub(crate) fn single_color_text_run(len: usize, color: Hsla, font: Font) -> TextRun {
    TextRun {
        len,
        color,
        font,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

pub(crate) fn shape_editor_line(
    window: &mut Window,
    text: SharedString,
    font_size: Pixels,
    runs: &[TextRun],
) -> ShapedLine {
    window.text_system().shape_line(text, font_size, runs, None)
}

pub(crate) fn paint_editor_line(
    window: &mut Window,
    cx: &mut App,
    line: &ShapedLine,
    origin: Point<Pixels>,
    line_height: Pixels,
) {
    let _ = line.paint(origin, line_height, TextAlign::Left, None, window, cx);
}

pub(super) fn build_text_runs_for_row(
    row: &DisplayRow,
    syntax_spans: &[RowSyntaxSpan],
    syntax_styles: &BTreeMap<String, ResolvedSyntaxStyle>,
    font: Font,
    default_foreground: Hsla,
    muted_foreground: Hsla,
) -> Vec<TextRun> {
    if row.text.is_empty() {
        return Vec::new();
    }

    if !matches!(row.kind, DisplayRowKind::Text) {
        return vec![TextRun {
            len: row.text.len(),
            color: muted_foreground,
            font,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    let mut column_byte_offsets = Vec::with_capacity(row.text.chars().count() + 1);
    column_byte_offsets.push(0);
    for (byte_index, ch) in row.text.char_indices() {
        column_byte_offsets.push(byte_index + ch.len_utf8());
    }
    let total_columns = column_byte_offsets.len().saturating_sub(1);

    let mut runs = Vec::new();
    let mut current_column = 0usize;

    for span in syntax_spans {
        let span_start = span.start_column.min(total_columns);
        let span_end = span.end_column.min(total_columns);
        let applied_start = span_start.max(current_column);
        if applied_start >= span_end {
            continue;
        }

        if current_column < applied_start {
            push_text_run(
                &mut runs,
                &column_byte_offsets,
                current_column,
                applied_start,
                font.clone(),
                default_foreground,
                None,
            );
        }

        push_text_run(
            &mut runs,
            &column_byte_offsets,
            applied_start,
            span_end,
            font.clone(),
            default_foreground,
            syntax_styles.get(&span.style_key).copied(),
        );
        current_column = span_end;
    }

    if current_column < total_columns {
        push_text_run(
            &mut runs,
            &column_byte_offsets,
            current_column,
            total_columns,
            font.clone(),
            default_foreground,
            None,
        );
    }

    if runs.is_empty() {
        runs.push(TextRun {
            len: row.text.len(),
            color: default_foreground,
            font,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    runs
}

fn push_text_run(
    runs: &mut Vec<TextRun>,
    column_byte_offsets: &[usize],
    start_column: usize,
    end_column: usize,
    mut font: Font,
    default_foreground: Hsla,
    style: Option<ResolvedSyntaxStyle>,
) {
    if start_column >= end_column || end_column >= column_byte_offsets.len() {
        return;
    }

    let len = column_byte_offsets[end_column].saturating_sub(column_byte_offsets[start_column]);
    if len == 0 {
        return;
    }

    if let Some(style) = style {
        if let Some(weight) = style.font_weight {
            font.weight = weight;
        }
        if let Some(font_style) = style.font_style {
            font.style = font_style;
        }

        runs.push(TextRun {
            len,
            color: style.color.unwrap_or(default_foreground),
            font,
            background_color: style.background_color,
            underline: style.underline,
            strikethrough: style.strikethrough,
        });
        return;
    }

    runs.push(TextRun {
        len,
        color: default_foreground,
        font,
        background_color: None,
        underline: None,
        strikethrough: None,
    });
}

pub(crate) fn resolve_syntax_styles(
    spans_by_row: &BTreeMap<usize, Vec<RowSyntaxSpan>>,
    cx: &App,
) -> BTreeMap<String, ResolvedSyntaxStyle> {
    let mut styles = BTreeMap::new();
    for spans in spans_by_row.values() {
        for span in spans {
            styles.entry(span.style_key.clone()).or_insert_with(|| {
                let style = cx
                    .theme()
                    .highlight_theme
                    .style
                    .syntax
                    .style(&span.style_key);
                ResolvedSyntaxStyle {
                    color: style.and_then(|style| style.color),
                    background_color: style.and_then(|style| style.background_color),
                    underline: style.and_then(|style| style.underline),
                    strikethrough: style.and_then(|style| style.strikethrough),
                    font_weight: style.and_then(|style| style.font_weight),
                    font_style: style.and_then(|style| style.font_style),
                }
            });
        }
    }
    styles
}

pub(super) fn paint_line_number(
    window: &mut Window,
    cx: &mut App,
    row: &DisplayRow,
    layout: &EditorLayout,
    params: LineNumberPaintParams,
) {
    let label = if row.start_column == 0 {
        format!("{}", row.source_line + 1)
    } else {
        String::new()
    };
    let color = if row.source_line == params.current_line {
        params.palette.current_line_number
    } else {
        params.palette.line_number
    };
    let runs = vec![single_color_text_run(label.len(), color, params.font)];
    let line = shape_editor_line(window, label.into(), layout.font_size, &runs);
    paint_editor_line(
        window,
        cx,
        &line,
        point(layout.line_number_origin_x(), params.origin.y),
        layout.line_height,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_fold_marker(
    window: &mut Window,
    cx: &mut App,
    row: &DisplayRow,
    layout: &EditorLayout,
    row_origin: Point<Pixels>,
    palette: FilesEditorPalette,
    font: Font,
    foldable: bool,
    folded: bool,
) {
    if row.start_column != 0 || !foldable {
        return;
    }

    let label = if folded { "▸" } else { "▾" };
    let runs = vec![TextRun {
        len: label.len(),
        color: palette.fold_marker,
        font,
        background_color: None,
        underline: None,
        strikethrough: None,
    }];
    let line = shape_editor_line(window, label.into(), layout.font_size, &runs);
    paint_editor_line(
        window,
        cx,
        &line,
        point(layout.fold_marker_origin_x(), row_origin.y),
        layout.line_height,
    );
}

pub(super) fn selection_range_for_row(
    row: &DisplayRow,
    selection: hunk_text::Selection,
) -> Option<Range<usize>> {
    let selection = selection.range();
    if selection.is_empty()
        || row.source_line < selection.start.line
        || row.source_line > selection.end.line
    {
        return None;
    }

    let row_start = if row.source_line == selection.start.line {
        selection.start.column.max(row.raw_start_column)
    } else {
        row.raw_start_column
    };
    let row_end = if row.source_line == selection.end.line {
        selection.end.column.min(row.raw_end_column)
    } else {
        row.raw_end_column
    };
    (row_start < row_end)
        .then_some(display_column_for_raw(row, row_start)..display_column_for_raw(row, row_end))
}

pub(super) fn paint_selection(
    window: &mut Window,
    row_origin: Point<Pixels>,
    layout: &EditorLayout,
    columns: Range<usize>,
    color: Hsla,
) {
    window.paint_quad(fill(
        Bounds {
            origin: point(
                row_origin.x + (layout.cell_width * columns.start as f32),
                row_origin.y,
            ),
            size: size(
                layout.cell_width * columns.end.saturating_sub(columns.start) as f32,
                layout.line_height,
            ),
        },
        color,
    ));
}

pub(super) fn paint_whitespace_markers(
    window: &mut Window,
    cx: &mut App,
    row: &DisplayRow,
    row_origin: Point<Pixels>,
    layout: &EditorLayout,
    palette: FilesEditorPalette,
    font: Font,
) {
    for marker in &row.whitespace_markers {
        let label = match marker.kind {
            hunk_editor::WhitespaceKind::Space => "·",
            hunk_editor::WhitespaceKind::Tab => "→",
        };
        let runs = vec![TextRun {
            len: label.len(),
            color: palette.invisible,
            font: font.clone(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let line = shape_editor_line(window, label.into(), layout.font_size, &runs);
        paint_editor_line(
            window,
            cx,
            &line,
            point(
                row_origin.x + (layout.cell_width * marker.column as f32),
                row_origin.y,
            ),
            layout.line_height,
        );
    }
}

pub(super) fn paint_indent_guides(
    window: &mut Window,
    row: &DisplayRow,
    row_origin: Point<Pixels>,
    layout: &EditorLayout,
    palette: FilesEditorPalette,
    tab_width: usize,
) {
    if row.start_column != 0 || row.text.is_empty() {
        return;
    }

    let indent_width = row.text.chars().take_while(|ch| *ch == ' ').count();
    if indent_width < tab_width {
        return;
    }

    for column in (tab_width..=indent_width).step_by(tab_width.max(1)) {
        let guide_column = column.saturating_sub(tab_width);
        let x = row_origin.x + (layout.cell_width * guide_column as f32) - px(0.5);
        window.paint_quad(fill(
            Bounds {
                origin: point(x, row_origin.y),
                size: size(px(1.0), layout.line_height),
            },
            palette.indent_guide,
        ));
    }
}

pub(super) fn paint_overlays(
    window: &mut Window,
    row: &DisplayRow,
    row_origin: Point<Pixels>,
    layout: &EditorLayout,
    palette: FilesEditorPalette,
) {
    for overlay in &row.overlays {
        let colors = palette.overlay_colors(overlay.kind);
        if is_diff_overlay(overlay.kind) {
            window.paint_quad(fill(
                Bounds {
                    origin: point(row_origin.x, row_origin.y),
                    size: size(
                        layout.cell_width * row.text.chars().count() as f32,
                        layout.line_height,
                    ),
                },
                colors.inline_background,
            ));
        }

        if is_diagnostic_overlay(overlay.kind) {
            window.paint_quad(fill(
                Bounds {
                    origin: point(row_origin.x, row_origin.y + layout.line_height - px(2.0)),
                    size: size(
                        layout.cell_width * row.text.chars().count().max(1) as f32,
                        px(1.5),
                    ),
                },
                colors.inline_background,
            ));
        }

        window.paint_quad(fill(
            Bounds {
                origin: point(
                    layout.hitbox.bounds.origin.x
                        + (layout.cell_width * layout.gutter_columns as f32)
                        - px(3.0),
                    row_origin.y + px(4.0),
                ),
                size: size(px(2.0), layout.line_height - px(8.0)),
            },
            colors.gutter_marker,
        ));
    }
}

pub(super) fn paint_scope_highlight(
    window: &mut Window,
    row: &DisplayRow,
    row_origin: Point<Pixels>,
    layout: &EditorLayout,
    palette: FilesEditorPalette,
    active_scope: Option<FoldRegion>,
) {
    let Some(scope) = active_scope else {
        return;
    };
    if row.source_line < scope.start_line || row.source_line > scope.end_line {
        return;
    }

    window.paint_quad(fill(
        Bounds {
            origin: point(layout.hitbox.bounds.origin.x, row_origin.y),
            size: size(px(2.0), layout.line_height),
        },
        palette.current_scope,
    ));
}

pub(super) fn paint_matching_brackets(
    window: &mut Window,
    row: &DisplayRow,
    row_origin: Point<Pixels>,
    layout: &EditorLayout,
    palette: FilesEditorPalette,
    matching_brackets: Option<(TextPosition, TextPosition)>,
) {
    let Some((left, right)) = matching_brackets else {
        return;
    };

    for position in [left, right] {
        if position.line != row.source_line
            || position.column < row.raw_start_column
            || position.column >= row.raw_end_column
        {
            continue;
        }
        let column = display_column_for_raw(row, position.column);
        paint_selection(
            window,
            row_origin,
            layout,
            column..column.saturating_add(1),
            palette.bracket_match,
        );
    }
}

pub(super) fn paint_cursor(
    window: &mut Window,
    rows: &[DisplayRow],
    caret: TextPosition,
    content_origin: Point<Pixels>,
    layout: &EditorLayout,
    color: Hsla,
) {
    if let Some(row) = rows.iter().find(|row| {
        row.source_line == caret.line
            && row.raw_start_column <= caret.column
            && caret.column <= row.raw_end_column
    }) {
        let x = content_origin.x
            + (layout.cell_width * display_column_for_raw(row, caret.column) as f32);
        let y = content_origin.y
            + (layout.line_height * row.row_index.saturating_sub(rows[0].row_index) as f32);
        window.paint_quad(fill(
            Bounds {
                origin: point(x, y),
                size: size(px(1.5), layout.line_height),
            },
            color,
        ));
    }
}

pub(super) fn matching_bracket_pair(
    snapshot: &TextSnapshot,
    caret: TextPosition,
) -> Option<(TextPosition, TextPosition)> {
    let text = snapshot.text();
    let caret_byte = snapshot.position_to_byte(caret).ok()?;
    let candidate = bracket_at_or_before(&text, caret_byte)?;

    let (pair, is_opening) = bracket_pair(candidate.ch)?;
    if is_opening {
        let mut depth = 1usize;
        for (byte, ch) in text[candidate.byte + candidate.ch.len_utf8()..].char_indices() {
            let absolute_byte = candidate.byte + candidate.ch.len_utf8() + byte;
            if ch == candidate.ch {
                depth += 1;
            } else if ch == pair {
                depth -= 1;
                if depth == 0 {
                    return Some((
                        snapshot.byte_to_position(candidate.byte).ok()?,
                        snapshot.byte_to_position(absolute_byte).ok()?,
                    ));
                }
            }
        }
        return None;
    }

    let mut depth = 1usize;
    for (byte, ch) in text[..candidate.byte].char_indices().rev() {
        if ch == candidate.ch {
            depth += 1;
        } else if ch == pair {
            depth -= 1;
            if depth == 0 {
                return Some((
                    snapshot.byte_to_position(byte).ok()?,
                    snapshot.byte_to_position(candidate.byte).ok()?,
                ));
            }
        }
    }
    None
}

pub(super) fn display_column_for_raw(row: &DisplayRow, raw_column: usize) -> usize {
    let offset = raw_column.saturating_sub(row.raw_start_column);
    row.raw_column_offsets
        .get(offset)
        .copied()
        .unwrap_or_else(|| row.raw_column_offsets.last().copied().unwrap_or(0))
}

pub(super) fn raw_column_for_display(row: &DisplayRow, display_column: usize) -> usize {
    let clamped_display = min(display_column, row.text.chars().count());
    let offsets = &row.raw_column_offsets;
    if offsets.is_empty() {
        return row.raw_start_column;
    }

    match offsets.binary_search(&clamped_display) {
        Ok(index) => row.raw_start_column + index,
        Err(0) => row.raw_start_column,
        Err(index) if index >= offsets.len() => row.raw_start_column + offsets.len() - 1,
        Err(index) => {
            let previous_offset = offsets[index - 1];
            let next_offset = offsets[index];
            let snaps_to_next = clamped_display.saturating_sub(previous_offset)
                >= next_offset.saturating_sub(clamped_display);
            row.raw_start_column + if snaps_to_next { index } else { index - 1 }
        }
    }
}

pub(crate) fn scroll_direction_and_count(
    event: &gpui::ScrollWheelEvent,
    line_height: Pixels,
) -> Option<(ScrollDirection, usize)> {
    let delta = event.delta.pixel_delta(line_height);
    if delta.y.abs() < px(0.5) {
        return None;
    }

    Some((
        if delta.y > Pixels::ZERO {
            ScrollDirection::Backward
        } else {
            ScrollDirection::Forward
        },
        ((delta.y.abs() / line_height).ceil() as usize).max(1),
    ))
}

#[derive(Clone, Copy)]
struct BracketCandidate {
    byte: usize,
    ch: char,
}

fn bracket_at_or_before(text: &str, byte: usize) -> Option<BracketCandidate> {
    char_at_byte(text, byte)
        .filter(|candidate| bracket_pair(candidate.ch).is_some())
        .or_else(|| {
            previous_char(text, byte).filter(|candidate| bracket_pair(candidate.ch).is_some())
        })
}

fn char_at_byte(text: &str, byte: usize) -> Option<BracketCandidate> {
    text[byte.min(text.len())..]
        .chars()
        .next()
        .map(|ch| BracketCandidate { byte, ch })
}

fn previous_char(text: &str, byte: usize) -> Option<BracketCandidate> {
    if byte == 0 {
        return None;
    }

    let mut chars = text[..byte.min(text.len())].char_indices();
    let (byte, ch) = chars.next_back()?;
    Some(BracketCandidate { byte, ch })
}

fn bracket_pair(ch: char) -> Option<(char, bool)> {
    match ch {
        '(' => Some((')', true)),
        '[' => Some((']', true)),
        '{' => Some(('}', true)),
        ')' => Some(('(', false)),
        ']' => Some(('[', false)),
        '}' => Some(('{', false)),
        _ => None,
    }
}

fn is_diff_overlay(kind: hunk_editor::OverlayKind) -> bool {
    matches!(
        kind,
        hunk_editor::OverlayKind::DiffAddition
            | hunk_editor::OverlayKind::DiffDeletion
            | hunk_editor::OverlayKind::DiffModification
    )
}

fn is_diagnostic_overlay(kind: hunk_editor::OverlayKind) -> bool {
    matches!(
        kind,
        hunk_editor::OverlayKind::DiagnosticError
            | hunk_editor::OverlayKind::DiagnosticWarning
            | hunk_editor::OverlayKind::DiagnosticInfo
    )
}
