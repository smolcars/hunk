use std::ops::Range;
use hunk_terminal::{
    TerminalColorSnapshot, TerminalCursorShapeSnapshot, TerminalNamedColorSnapshot,
    TerminalScreenSnapshot,
};

const AI_TERMINAL_FONT_SIZE_PX: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct AiTerminalCellStyle {
    color: gpui::Hsla,
    background: gpui::Hsla,
    underline: Option<gpui::UnderlineStyle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTerminalRenderCell {
    character: char,
    fg: TerminalColorSnapshot,
    bg: TerminalColorSnapshot,
    zerowidth: String,
    cursor: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct AiTerminalBackgroundRect {
    column: usize,
    width: usize,
    color: gpui::Hsla,
}

#[derive(Debug, Clone, PartialEq)]
struct AiTerminalPaintLine {
    surface_id: String,
    text: SharedString,
    column_byte_offsets: Arc<[usize]>,
    background_rects: Arc<[AiTerminalBackgroundRect]>,
    text_runs: Arc<[gpui::TextRun]>,
    selection_range: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq)]
struct AiTerminalTextRunStyle {
    color: gpui::Hsla,
    underline: Option<gpui::UnderlineStyle>,
}

#[derive(Clone)]
struct AiTerminalSurfaceElement {
    element_id: gpui::ElementId,
    view: Entity<DiffViewer>,
    lines: Arc<[AiTerminalPaintLine]>,
    selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
    selection_enabled: bool,
    selection_background: gpui::Hsla,
    text_style: gpui::TextStyle,
}

#[derive(Clone)]
struct AiTerminalSurfaceLayout {
    hitbox: gpui::Hitbox,
    cell_width: Pixels,
    line_height: Pixels,
    font_size: Pixels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTerminalHit {
    surface_id: String,
    index: usize,
}

impl DiffViewer {
    fn render_ai_terminal_surface(
        &self,
        state: &AiTerminalPanelState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(screen) = state.screen.as_ref() {
            return self.render_ai_terminal_vt_surface(screen, is_dark, cx);
        }

        if state.has_transcript {
            return v_flex()
                .w_full()
                .gap_0p5()
                .children(
                    state
                        .transcript
                        .lines()
                        .map(|line| {
                            div()
                                .w_full()
                                .text_xs()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_color(cx.theme().foreground)
                                .whitespace_nowrap()
                                .child(line.to_string())
                                .into_any_element()
                        })
                        .collect::<Vec<_>>(),
                )
                .into_any_element();
        }

        div()
            .w_full()
            .text_xs()
            .font_family(cx.theme().mono_font_family.clone())
            .text_color(cx.theme().muted_foreground)
            .child("Run a command to start a terminal session.")
            .into_any_element()
    }

    fn render_ai_terminal_vt_surface(
        &self,
        screen: &TerminalScreenSnapshot,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selection_enabled = ai_terminal_supports_text_selection(screen);
        let text_style = ai_terminal_surface_text_style(is_dark, cx);
        let lines = ai_terminal_paint_lines(self, screen, &text_style, is_dark, cx);
        let selection_surfaces = ai_terminal_selection_surfaces(lines.as_slice());

        AiTerminalSurfaceElement {
            element_id: "ai-terminal-surface".into(),
            view: cx.entity(),
            lines: lines.into(),
            selection_surfaces,
            selection_enabled,
            selection_background: hunk_text_selection_background(cx.theme(), is_dark),
            text_style,
        }
        .into_any_element()
    }
}

impl IntoElement for AiTerminalSurfaceElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for AiTerminalSurfaceElement {
    type RequestLayoutState = ();
    type PrepaintState = AiTerminalSurfaceLayout;

    fn id(&self) -> Option<gpui::ElementId> {
        Some(self.element_id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let mut style = gpui::Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = gpui::relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let font = self.text_style.font();
        let font_id = window.text_system().resolve_font(&font);
        let font_size = self.text_style.font_size.to_pixels(window.rem_size());
        let line_height = self.text_style.line_height_in_pixels(window.rem_size());
        let cell_width = window
            .text_system()
            .advance(font_id, font_size, 'm')
            .map(|advance| advance.width)
            .unwrap_or_else(|_| px(8.0));

        AiTerminalSurfaceLayout {
            hitbox: window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
            cell_width,
            line_height,
            font_size,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let hitbox_for_mouse_down = layout.hitbox.clone();
        let hitbox_for_mouse_move = layout.hitbox.clone();
        let hitbox_for_mouse_up = layout.hitbox.clone();
        let lines_for_mouse = self.lines.clone();
        let surfaces_for_mouse = self.selection_surfaces.clone();
        let view = self.view.clone();
        let cell_width = layout.cell_width;
        let line_height = layout.line_height;
        let bounds_origin = bounds.origin;
        let selection_enabled = self.selection_enabled;

        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if !selection_enabled
                || phase != gpui::DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !hitbox_for_mouse_down.is_hovered(window)
            {
                return;
            }

            let Some(hit) = ai_terminal_hit_test(
                lines_for_mouse.as_ref(),
                bounds_origin,
                event.position,
                cell_width,
                line_height,
            ) else {
                return;
            };

            view.update(cx, |this, cx| {
                this.ai_begin_text_selection(
                    crate::app::AI_TERMINAL_TEXT_SELECTION_ROW_ID.to_string(),
                    surfaces_for_mouse.clone(),
                    hit.surface_id.as_str(),
                    hit.index,
                    window,
                    cx,
                );
            });
        });

        let lines_for_mouse = self.lines.clone();
        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if !selection_enabled
                || phase != gpui::DispatchPhase::Bubble
                || !hitbox_for_mouse_move.is_hovered(window)
            {
                return;
            }

            let dragging_terminal_selection = view.read(cx).ai_text_selection.as_ref().is_some_and(
                |selection| {
                    selection.row_id == crate::app::AI_TERMINAL_TEXT_SELECTION_ROW_ID
                        && selection.dragging
                },
            );
            if !dragging_terminal_selection {
                return;
            }

            let Some(hit) = ai_terminal_hit_test(
                lines_for_mouse.as_ref(),
                bounds_origin,
                event.position,
                cell_width,
                line_height,
            ) else {
                return;
            };

            view.update(cx, |this, cx| {
                this.ai_update_text_selection(hit.surface_id.as_str(), hit.index, cx);
            });
        });

        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if !selection_enabled
                || phase != gpui::DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !hitbox_for_mouse_up.is_hovered(window)
            {
                return;
            }

            view.update(cx, |this, cx| {
                this.ai_end_text_selection(cx);
            });
        });

        if self.selection_enabled && layout.hitbox.is_hovered(window) {
            window.set_cursor_style(gpui::CursorStyle::IBeam, &layout.hitbox);
        }

        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for (row_index, line) in self.lines.iter().enumerate() {
                let row_origin = point(
                    bounds.origin.x,
                    bounds.origin.y + layout.line_height * row_index as f32,
                );

                ai_terminal_paint_selection(
                    line,
                    row_origin,
                    layout.cell_width,
                    layout.line_height,
                    self.selection_background,
                    window,
                );

                for background in line.background_rects.iter() {
                    window.paint_quad(fill(
                        gpui::Bounds {
                            origin: point(
                                row_origin.x + layout.cell_width * background.column as f32,
                                row_origin.y,
                            ),
                            size: gpui::size(
                                layout.cell_width * background.width as f32,
                                layout.line_height,
                            ),
                        },
                        background.color,
                    ));
                }

                let shaped = window.text_system().shape_line(
                    line.text.clone(),
                    layout.font_size,
                    line.text_runs.as_ref(),
                    None,
                );
                let _ = shaped.paint(
                    row_origin,
                    layout.line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        });
    }
}

fn ai_terminal_supports_text_selection(screen: &TerminalScreenSnapshot) -> bool {
    !screen.mode.alt_screen && !screen.mode.mouse_mode
}

fn ai_terminal_surface_text_style(is_dark: bool, cx: &App) -> gpui::TextStyle {
    let chrome = hunk_editor_chrome_colors(cx.theme(), is_dark);
    gpui::TextStyle {
        color: chrome.foreground,
        font_family: cx.theme().mono_font_family.clone(),
        font_size: px(AI_TERMINAL_FONT_SIZE_PX).into(),
        line_height: gpui::relative(1.45),
        ..Default::default()
    }
}

fn ai_terminal_paint_lines(
    this: &DiffViewer,
    screen: &TerminalScreenSnapshot,
    text_style: &gpui::TextStyle,
    is_dark: bool,
    cx: &App,
) -> Vec<AiTerminalPaintLine> {
    ai_terminal_screen_grid(screen)
        .into_iter()
        .enumerate()
        .map(|(row_index, row)| {
            ai_terminal_paint_line(this, row_index, &row, screen, text_style, is_dark, cx)
        })
        .collect()
}

fn ai_terminal_paint_line(
    this: &DiffViewer,
    row_index: usize,
    row: &[AiTerminalRenderCell],
    screen: &TerminalScreenSnapshot,
    text_style: &gpui::TextStyle,
    is_dark: bool,
    cx: &App,
) -> AiTerminalPaintLine {
    let default_foreground = ai_terminal_snapshot_color(
        TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Foreground),
        is_dark,
        cx,
    );
    let default_background = ai_terminal_snapshot_color(
        TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Background),
        is_dark,
        cx,
    );

    let surface_id = ai_terminal_text_surface_id(row_index);
    let mut text = String::with_capacity(row.len());
    let mut column_byte_offsets = Vec::with_capacity(row.len() + 1);
    let mut background_rects = Vec::new();
    let mut text_runs = Vec::new();
    let mut active_run_style: Option<AiTerminalTextRunStyle> = None;
    let mut active_run_start = 0usize;

    column_byte_offsets.push(0);

    for cell in row {
        let start = text.len();
        text.push(cell.character);
        text.push_str(cell.zerowidth.as_str());
        let end = text.len();
        column_byte_offsets.push(end);

        let style = ai_terminal_cell_style(
            cell,
            screen.cursor.shape,
            default_foreground,
            default_background,
            is_dark,
            cx,
        );

        if style.background != default_background {
            ai_terminal_push_background_rect(
                &mut background_rects,
                column_byte_offsets.len().saturating_sub(2),
                style.background,
            );
        }

        let run_style = AiTerminalTextRunStyle {
            color: style.color,
            underline: style.underline,
        };

        if active_run_style.as_ref() != Some(&run_style) {
            if let Some(previous_style) = active_run_style.take() {
                ai_terminal_push_text_run(
                    &mut text_runs,
                    text_style.font(),
                    &previous_style,
                    active_run_start,
                    start,
                );
            }
            active_run_style = Some(run_style);
            active_run_start = start;
        }
    }

    if let Some(style) = active_run_style.as_ref() {
        ai_terminal_push_text_run(
            &mut text_runs,
            text_style.font(),
            style,
            active_run_start,
            text.len(),
        );
    }

    AiTerminalPaintLine {
        surface_id: surface_id.clone(),
        text: text.into(),
        column_byte_offsets: column_byte_offsets.into(),
        background_rects: background_rects.into(),
        text_runs: text_runs.into(),
        selection_range: this.ai_text_selection_range_for_surface(surface_id.as_str()),
    }
}

fn ai_terminal_push_background_rect(
    rects: &mut Vec<AiTerminalBackgroundRect>,
    column: usize,
    color: gpui::Hsla,
) {
    if let Some(previous) = rects.last_mut()
        && previous.color == color
        && previous.column + previous.width == column
    {
        previous.width += 1;
        return;
    }

    rects.push(AiTerminalBackgroundRect {
        column,
        width: 1,
        color,
    });
}

fn ai_terminal_push_text_run(
    runs: &mut Vec<gpui::TextRun>,
    font: gpui::Font,
    style: &AiTerminalTextRunStyle,
    start: usize,
    end: usize,
) {
    let len = end.saturating_sub(start);
    if len == 0 {
        return;
    }

    runs.push(gpui::TextRun {
        len,
        color: style.color,
        font,
        background_color: None,
        underline: style.underline,
        strikethrough: None,
    });
}

fn ai_terminal_selection_surfaces(
    lines: &[AiTerminalPaintLine],
) -> Arc<[AiTextSelectionSurfaceSpec]> {
    ai_text_selection_surfaces(
        lines.iter()
            .enumerate()
            .map(|(row_index, line)| {
                let surface =
                    AiTextSelectionSurfaceSpec::new(line.surface_id.clone(), line.text.to_string());
                if row_index == 0 {
                    surface
                } else {
                    surface.with_separator_before("\n")
                }
            })
            .collect(),
    )
}

fn ai_terminal_text_surface_id(row_index: usize) -> String {
    format!(
        "{}\u{1f}row\u{1f}{row_index}",
        crate::app::AI_TERMINAL_TEXT_SELECTION_ROW_ID
    )
}

fn ai_terminal_hit_test(
    lines: &[AiTerminalPaintLine],
    origin: Point<Pixels>,
    position: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
) -> Option<AiTerminalHit> {
    let relative_x = (position.x - origin.x).max(px(0.0));
    let relative_y = (position.y - origin.y).max(px(0.0));
    let row_index = ((relative_y / line_height).floor() as usize).min(lines.len().saturating_sub(1));
    let line = lines.get(row_index)?;
    let max_column = line.column_byte_offsets.len().saturating_sub(1);
    let column = ((relative_x / cell_width).floor() as usize).min(max_column);

    Some(AiTerminalHit {
        surface_id: line.surface_id.clone(),
        index: line.column_byte_offsets[column],
    })
}

fn ai_terminal_paint_selection(
    line: &AiTerminalPaintLine,
    row_origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    selection_background: gpui::Hsla,
    window: &mut Window,
) {
    let Some(selection_range) = line.selection_range.as_ref() else {
        return;
    };
    let Some((start_column, end_column)) =
        ai_terminal_selection_columns(line.column_byte_offsets.as_ref(), selection_range)
    else {
        return;
    };

    if start_column == end_column {
        return;
    }

    window.paint_quad(fill(
        Bounds {
            origin: point(row_origin.x + cell_width * start_column as f32, row_origin.y),
            size: size(cell_width * (end_column - start_column) as f32, line_height),
        },
        selection_background,
    ));
}

fn ai_terminal_selection_columns(
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

fn ai_terminal_screen_grid(screen: &TerminalScreenSnapshot) -> Vec<Vec<AiTerminalRenderCell>> {
    let rows = usize::from(screen.rows.max(1));
    let cols = usize::from(screen.cols.max(1));
    let first_visible_line = screen
        .cells
        .iter()
        .map(|cell| cell.line)
        .min()
        .unwrap_or(screen.cursor.line.max(0));

    let mut grid = vec![
        vec![
            AiTerminalRenderCell {
                character: ' ',
                fg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Foreground),
                bg: TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Background),
                zerowidth: String::new(),
                cursor: false,
            };
            cols
        ];
        rows
    ];

    for cell in &screen.cells {
        let relative_line = cell.line - first_visible_line;
        if relative_line < 0 {
            continue;
        }
        let Ok(row_index) = usize::try_from(relative_line) else {
            continue;
        };
        if row_index >= rows || cell.column >= cols {
            continue;
        }

        grid[row_index][cell.column] = AiTerminalRenderCell {
            character: ai_terminal_render_character(cell.character),
            fg: cell.fg,
            bg: cell.bg,
            zerowidth: cell.zerowidth.iter().collect(),
            cursor: false,
        };
    }

    if screen.mode.show_cursor {
        let relative_line = screen.cursor.line - first_visible_line;
        if relative_line >= 0
            && let Ok(row_index) = usize::try_from(relative_line)
            && row_index < rows
            && screen.cursor.column < cols
        {
            grid[row_index][screen.cursor.column].cursor = true;
        }
    }

    grid
}

fn ai_terminal_cell_style(
    cell: &AiTerminalRenderCell,
    cursor_shape: TerminalCursorShapeSnapshot,
    default_foreground: gpui::Hsla,
    default_background: gpui::Hsla,
    is_dark: bool,
    cx: &App,
) -> AiTerminalCellStyle {
    let mut style = AiTerminalCellStyle {
        color: ai_terminal_snapshot_color(cell.fg, is_dark, cx),
        background: ai_terminal_snapshot_color(cell.bg, is_dark, cx),
        underline: None,
    };

    if cell.cursor {
        let cursor_color = ai_terminal_snapshot_color(
            TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Cursor),
            is_dark,
            cx,
        );

        match cursor_shape {
            TerminalCursorShapeSnapshot::Hidden => {}
            TerminalCursorShapeSnapshot::Underline => {
                style.underline = Some(gpui::UnderlineStyle {
                    thickness: px(1.5),
                    color: Some(cursor_color),
                    wavy: false,
                });
            }
            TerminalCursorShapeSnapshot::Beam => {
                style.background = hunk_opacity(cursor_color, is_dark, 0.32, 0.18);
            }
            TerminalCursorShapeSnapshot::Block | TerminalCursorShapeSnapshot::HollowBlock => {
                style.color = default_background;
                style.background = cursor_color;
            }
        }
    }

    if style.color == default_foreground
        && style.background == default_background
        && style.underline.is_none()
    {
        return AiTerminalCellStyle {
            color: default_foreground,
            background: default_background,
            underline: None,
        };
    }

    style
}

fn ai_terminal_render_character(character: char) -> char {
    if character == '\0' || character.is_control() {
        ' '
    } else {
        character
    }
}

fn ai_terminal_snapshot_color(
    color: TerminalColorSnapshot,
    is_dark: bool,
    cx: &App,
) -> gpui::Hsla {
    match color {
        TerminalColorSnapshot::Named(named) => ai_terminal_named_color(named, is_dark, cx),
        TerminalColorSnapshot::Indexed(index) => ai_terminal_indexed_color(index, is_dark, cx),
        TerminalColorSnapshot::Rgb { r, g, b } => gpui::Hsla::from(gpui::rgb(
            (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
        )),
    }
}

fn ai_terminal_named_color(
    color: TerminalNamedColorSnapshot,
    is_dark: bool,
    cx: &App,
) -> gpui::Hsla {
    let theme = cx.theme();
    let chrome = hunk_editor_chrome_colors(theme, is_dark);
    let magenta = hunk_blend(theme.accent, theme.danger, is_dark, 0.42, 0.30);
    let cyan = hunk_blend(theme.accent, theme.success, is_dark, 0.30, 0.26);
    let black = hunk_blend(chrome.background, chrome.foreground, is_dark, 0.14, 0.26);

    match color {
        TerminalNamedColorSnapshot::Black => black,
        TerminalNamedColorSnapshot::Red => theme.danger,
        TerminalNamedColorSnapshot::Green => theme.success,
        TerminalNamedColorSnapshot::Yellow => theme.warning,
        TerminalNamedColorSnapshot::Blue => theme.accent,
        TerminalNamedColorSnapshot::Magenta => magenta,
        TerminalNamedColorSnapshot::Cyan => cyan,
        TerminalNamedColorSnapshot::White => chrome.foreground,
        TerminalNamedColorSnapshot::BrightBlack => {
            hunk_opacity(chrome.foreground, is_dark, 0.62, 0.58)
        }
        TerminalNamedColorSnapshot::BrightRed => {
            hunk_blend(theme.danger, chrome.foreground, is_dark, 0.16, 0.08)
        }
        TerminalNamedColorSnapshot::BrightGreen => {
            hunk_blend(theme.success, chrome.foreground, is_dark, 0.16, 0.08)
        }
        TerminalNamedColorSnapshot::BrightYellow => {
            hunk_blend(theme.warning, chrome.foreground, is_dark, 0.14, 0.08)
        }
        TerminalNamedColorSnapshot::BrightBlue => {
            hunk_blend(theme.accent, chrome.foreground, is_dark, 0.14, 0.08)
        }
        TerminalNamedColorSnapshot::BrightMagenta => {
            hunk_blend(magenta, chrome.foreground, is_dark, 0.16, 0.08)
        }
        TerminalNamedColorSnapshot::BrightCyan => {
            hunk_blend(cyan, chrome.foreground, is_dark, 0.16, 0.08)
        }
        TerminalNamedColorSnapshot::BrightWhite => {
            hunk_blend(chrome.foreground, theme.background, is_dark, 0.02, 0.02)
        }
        TerminalNamedColorSnapshot::Foreground | TerminalNamedColorSnapshot::BrightForeground => {
            chrome.foreground
        }
        TerminalNamedColorSnapshot::Background => chrome.background,
        TerminalNamedColorSnapshot::Cursor => theme.primary,
        TerminalNamedColorSnapshot::DimBlack => hunk_opacity(black, is_dark, 0.58, 0.68),
        TerminalNamedColorSnapshot::DimRed => {
            hunk_opacity(theme.danger, is_dark, 0.72, 0.82)
        }
        TerminalNamedColorSnapshot::DimGreen => {
            hunk_opacity(theme.success, is_dark, 0.72, 0.82)
        }
        TerminalNamedColorSnapshot::DimYellow => {
            hunk_opacity(theme.warning, is_dark, 0.72, 0.82)
        }
        TerminalNamedColorSnapshot::DimBlue => {
            hunk_opacity(theme.accent, is_dark, 0.72, 0.82)
        }
        TerminalNamedColorSnapshot::DimMagenta => hunk_opacity(magenta, is_dark, 0.72, 0.82),
        TerminalNamedColorSnapshot::DimCyan => hunk_opacity(cyan, is_dark, 0.72, 0.82),
        TerminalNamedColorSnapshot::DimWhite | TerminalNamedColorSnapshot::DimForeground => {
            hunk_opacity(chrome.foreground, is_dark, 0.72, 0.82)
        }
    }
}

fn ai_terminal_indexed_color(index: u8, is_dark: bool, cx: &App) -> gpui::Hsla {
    match index {
        0 => ai_terminal_named_color(TerminalNamedColorSnapshot::Black, is_dark, cx),
        1 => ai_terminal_named_color(TerminalNamedColorSnapshot::Red, is_dark, cx),
        2 => ai_terminal_named_color(TerminalNamedColorSnapshot::Green, is_dark, cx),
        3 => ai_terminal_named_color(TerminalNamedColorSnapshot::Yellow, is_dark, cx),
        4 => ai_terminal_named_color(TerminalNamedColorSnapshot::Blue, is_dark, cx),
        5 => ai_terminal_named_color(TerminalNamedColorSnapshot::Magenta, is_dark, cx),
        6 => ai_terminal_named_color(TerminalNamedColorSnapshot::Cyan, is_dark, cx),
        7 => ai_terminal_named_color(TerminalNamedColorSnapshot::White, is_dark, cx),
        8 => ai_terminal_named_color(TerminalNamedColorSnapshot::BrightBlack, is_dark, cx),
        9 => ai_terminal_named_color(TerminalNamedColorSnapshot::BrightRed, is_dark, cx),
        10 => ai_terminal_named_color(TerminalNamedColorSnapshot::BrightGreen, is_dark, cx),
        11 => ai_terminal_named_color(TerminalNamedColorSnapshot::BrightYellow, is_dark, cx),
        12 => ai_terminal_named_color(TerminalNamedColorSnapshot::BrightBlue, is_dark, cx),
        13 => ai_terminal_named_color(TerminalNamedColorSnapshot::BrightMagenta, is_dark, cx),
        14 => ai_terminal_named_color(TerminalNamedColorSnapshot::BrightCyan, is_dark, cx),
        15 => ai_terminal_named_color(TerminalNamedColorSnapshot::BrightWhite, is_dark, cx),
        16..=231 => {
            let palette_index = index - 16;
            let red = palette_index / 36;
            let green = (palette_index % 36) / 6;
            let blue = palette_index % 6;
            gpui::Hsla::from(gpui::rgb(
                (u32::from(ai_terminal_cube_component(red)) << 16)
                    | (u32::from(ai_terminal_cube_component(green)) << 8)
                    | u32::from(ai_terminal_cube_component(blue)),
            ))
        }
        232..=255 => {
            let component = 8 + ((index - 232) * 10);
            gpui::Hsla::from(gpui::rgb(
                (u32::from(component) << 16) | (u32::from(component) << 8) | u32::from(component),
            ))
        }
    }
}

fn ai_terminal_cube_component(value: u8) -> u8 {
    match value {
        0 => 0,
        1 => 95,
        2 => 135,
        3 => 175,
        4 => 215,
        _ => 255,
    }
}
