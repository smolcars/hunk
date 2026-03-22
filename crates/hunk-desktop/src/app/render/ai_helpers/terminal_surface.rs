use std::ops::Range;
use hunk_terminal::{
    TerminalColorSnapshot, TerminalCursorShapeSnapshot, TerminalNamedColorSnapshot,
    TerminalScreenSnapshot,
};

const AI_TERMINAL_FONT_SIZE_PX: f32 = 12.0;
const AI_TERMINAL_WIDE_CHAR_SPACER_FLAG: u16 = 0b0000_0000_0100_0000;
const AI_TERMINAL_LEADING_WIDE_CHAR_SPACER_FLAG: u16 = 0b0000_0100_0000_0000;

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
    link_ranges: Arc<[MarkdownLinkRange]>,
    background_rects: Arc<[AiTerminalBackgroundRect]>,
    cursor_overlays: Arc<[AiTerminalCursorOverlay]>,
    text_runs: Arc<[gpui::TextRun]>,
    selection_range: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq)]
struct AiTerminalTextRunStyle {
    color: gpui::Hsla,
    underline: Option<gpui::UnderlineStyle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AiTerminalSurfacePaintOptions {
    surface_focused: bool,
    cursor_blink_visible: bool,
    cursor_output_suppressed: bool,
    is_dark: bool,
}

#[derive(Clone)]
struct AiTerminalSurfaceElement {
    element_id: gpui::ElementId,
    view: Entity<DiffViewer>,
    screen: Arc<TerminalScreenSnapshot>,
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
            .child("Starting shell...")
            .into_any_element()
    }

    fn render_ai_terminal_vt_surface(
        &self,
        screen: &Arc<TerminalScreenSnapshot>,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selection_enabled = ai_terminal_supports_text_selection(screen);
        let text_style = ai_terminal_surface_text_style(is_dark, cx);
        let paint = AiTerminalSurfacePaintOptions {
            surface_focused: self.ai_terminal_surface_focused,
            cursor_blink_visible: self.ai_terminal_cursor_blink_visible,
            cursor_output_suppressed: self.ai_terminal_cursor_output_suppressed,
            is_dark,
        };
        let lines = ai_terminal_paint_lines(
            self,
            screen,
            &text_style,
            paint,
            cx,
        );
        let selection_surfaces = ai_terminal_selection_surfaces(lines.as_slice());

        AiTerminalSurfaceElement {
            element_id: "ai-terminal-surface".into(),
            view: cx.entity(),
            screen: screen.clone(),
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
        cx: &mut App,
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
        let rows = ((bounds.size.height / line_height).floor() as u16).max(1);
        let cols = ((bounds.size.width / cell_width).floor() as u16).max(1);
        self.view.update(cx, |this, cx| {
            this.ai_resize_terminal_surface(rows, cols, cx);
        });

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
        let hitbox_for_scroll = layout.hitbox.clone();
        let lines_for_mouse = self.lines.clone();
        let surfaces_for_mouse = self.selection_surfaces.clone();
        let view = self.view.clone();
        let screen = self.screen.clone();
        let cell_width = layout.cell_width;
        let line_height = layout.line_height;
        let bounds_origin = bounds.origin;
        let selection_enabled = self.selection_enabled;

        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !hitbox_for_mouse_down.is_hovered(window)
            {
                return;
            }

            let (line, column) = ai_terminal_surface_grid_point_from_position(
                screen.as_ref(),
                bounds_origin,
                event.position,
                cell_width,
                line_height,
            );
            let handled = view.update(cx, |this, cx| {
                this.ai_terminal_surface_mouse_down(event, line, column, cx)
            });
            if handled {
                cx.stop_propagation();
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
            let pressed_link = ai_terminal_link_for_hit(lines_for_mouse.as_ref(), &hit).map(
                |range| AiPressedMarkdownLink {
                    surface_id: hit.surface_id.clone(),
                    raw_target: range.raw_target,
                    mouse_down_position: event.position,
                    dragged: false,
                },
            );

            view.update(cx, |this, cx| {
                this.ai_set_pressed_markdown_link(pressed_link.clone());
                if selection_enabled {
                    this.ai_begin_text_selection(
                        crate::app::AI_TERMINAL_TEXT_SELECTION_ROW_ID.to_string(),
                        surfaces_for_mouse.clone(),
                        hit.surface_id.as_str(),
                        hit.index,
                        window,
                        cx,
                    );
                }
            });
        });

        let lines_for_mouse = self.lines.clone();
        let view = self.view.clone();
        let screen = self.screen.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble {
                return;
            }

            view.update(cx, |this, _| {
                this.ai_mark_pressed_markdown_link_dragged(event.position);
            });

            if !hitbox_for_mouse_move.is_hovered(window) {
                return;
            }

            let (line, column) = ai_terminal_surface_grid_point_from_position(
                screen.as_ref(),
                bounds_origin,
                event.position,
                cell_width,
                line_height,
            );
            let handled = view.update(cx, |this, cx| {
                this.ai_terminal_surface_mouse_move(event, line, column, cx)
            });
            if handled {
                cx.stop_propagation();
                return;
            }

            if !selection_enabled {
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
        let screen = self.screen.clone();
        let lines_for_mouse = self.lines.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble {
                return;
            }

            if event.button == MouseButton::Left {
                view.update(cx, |this, cx| {
                    this.ai_end_text_selection(cx);
                    if !hitbox_for_mouse_up.is_hovered(window) {
                        let _ = this.ai_take_pressed_markdown_link();
                    }
                });
            }

            if !hitbox_for_mouse_up.is_hovered(window) {
                return;
            }

            let (line, column) = ai_terminal_surface_grid_point_from_position(
                screen.as_ref(),
                bounds_origin,
                event.position,
                cell_width,
                line_height,
            );
            let handled = view.update(cx, |this, cx| {
                this.ai_terminal_surface_mouse_up(event, line, column, cx)
            });
            if handled {
                cx.stop_propagation();
                return;
            }

            if event.button != MouseButton::Left {
                return;
            }

            view.update(cx, |this, cx| {
                let Some(pressed_link) = this.ai_take_pressed_markdown_link() else {
                    return;
                };
                let Some(hit) = ai_terminal_hit_test(
                    lines_for_mouse.as_ref(),
                    bounds_origin,
                    event.position,
                    cell_width,
                    line_height,
                ) else {
                    return;
                };
                if pressed_link.surface_id != hit.surface_id {
                    return;
                }
                if pressed_link.dragged {
                    return;
                }
                let activated = ai_terminal_link_for_hit(lines_for_mouse.as_ref(), &hit)
                    .is_some_and(|range| range.raw_target == pressed_link.raw_target);
                if activated {
                    this.activate_markdown_link(pressed_link.raw_target, Some(window), cx);
                }
            });
        });

        let view = self.view.clone();
        let screen = self.screen.clone();
        window.on_mouse_event(move |event: &gpui::ScrollWheelEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble || !hitbox_for_scroll.is_hovered(window) {
                return;
            }

            let (line, column) = ai_terminal_surface_grid_point_from_position(
                screen.as_ref(),
                bounds_origin,
                event.position,
                cell_width,
                line_height,
            );
            let handled = view.update(cx, |this, cx| {
                this.ai_terminal_surface_scroll_wheel(event, line, column, cx)
            });
            if handled {
                cx.stop_propagation();
            }
        });

        if layout.hitbox.is_hovered(window) {
            let hovered_link = ai_terminal_link_at_position(
                self.lines.as_ref(),
                bounds_origin,
                window.mouse_position(),
                cell_width,
                line_height,
            );
            let cursor_style = if self.screen.mode.mouse_mode {
                gpui::CursorStyle::Arrow
            } else if hovered_link.is_some() {
                gpui::CursorStyle::PointingHand
            } else if self.selection_enabled {
                gpui::CursorStyle::IBeam
            } else {
                gpui::CursorStyle::Arrow
            };
            window.set_cursor_style(cursor_style, &layout.hitbox);
        }

        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for (row_index, line) in self.lines.iter().enumerate() {
                let row_origin = point(
                    bounds.origin.x,
                    bounds.origin.y + layout.line_height * row_index as f32,
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

                ai_terminal_paint_selection(
                    line,
                    row_origin,
                    layout.cell_width,
                    layout.line_height,
                    self.selection_background,
                    window,
                );

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

                ai_terminal_paint_cursor_overlays(
                    line.cursor_overlays.as_ref(),
                    row_origin,
                    layout.cell_width,
                    layout.line_height,
                    window,
                );
            }
        });
    }
}

fn ai_terminal_supports_text_selection(screen: &TerminalScreenSnapshot) -> bool {
    !screen.mode.alt_screen
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
    paint: AiTerminalSurfacePaintOptions,
    cx: &App,
) -> Vec<AiTerminalPaintLine> {
    let cursor_render = AiTerminalCursorRenderContext {
        cursor_shape: crate::app::terminal_cursor::ai_terminal_effective_cursor_shape(
            screen.cursor.shape,
            paint.surface_focused,
            screen.mode.alt_screen,
        ),
        surface_focused: paint.surface_focused,
        cursor_visible: crate::app::terminal_cursor::ai_terminal_cursor_visible_for_paint(
            screen.cursor.shape,
            paint.surface_focused,
            paint.cursor_blink_visible,
            paint.cursor_output_suppressed,
        ),
        default_foreground: ai_terminal_snapshot_color(
            TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Foreground),
            paint.is_dark,
            cx,
        ),
        default_background: ai_terminal_snapshot_color(
            TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Background),
            paint.is_dark,
            cx,
        ),
        is_dark: paint.is_dark,
    };

    ai_terminal_screen_grid(screen)
        .into_iter()
        .enumerate()
        .map(|(row_index, row)| {
            ai_terminal_paint_line(this, row_index, &row, text_style, cursor_render, cx)
        })
        .collect()
}

fn ai_terminal_paint_line(
    this: &DiffViewer,
    row_index: usize,
    row: &[AiTerminalRenderCell],
    text_style: &gpui::TextStyle,
    cursor_render: AiTerminalCursorRenderContext,
    cx: &App,
) -> AiTerminalPaintLine {
    let surface_id = ai_terminal_text_surface_id(row_index);
    let mut text = String::with_capacity(row.len());
    let mut column_byte_offsets = Vec::with_capacity(row.len() + 1);
    column_byte_offsets.push(0);

    for cell in row {
        text.push(cell.character);
        text.push_str(cell.zerowidth.as_str());
        column_byte_offsets.push(text.len());
    }

    let link_ranges = ai_terminal_link_ranges(text.as_str());
    let mut background_rects = Vec::new();
    let mut cursor_overlays = Vec::new();
    let mut text_runs = Vec::new();
    let mut active_run_style: Option<AiTerminalTextRunStyle> = None;
    let mut active_run_start = 0usize;

    for (column, cell) in row.iter().enumerate() {
        let start = column_byte_offsets[column];
        let end = column_byte_offsets[column + 1];
        let link_active = ai_terminal_link_ranges_contains(link_ranges.as_slice(), start, end);

        let (style, cursor_overlay) = ai_terminal_cell_style(
            cell,
            column,
            link_active,
            cursor_render,
            cx,
        );
        if let Some(cursor_overlay) = cursor_overlay {
            cursor_overlays.push(cursor_overlay);
        }

        if style.background != cursor_render.default_background {
            ai_terminal_push_background_rect(
                &mut background_rects,
                column,
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
        link_ranges: link_ranges.into(),
        background_rects: background_rects.into(),
        cursor_overlays: cursor_overlays.into(),
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

fn ai_terminal_link_at_position(
    lines: &[AiTerminalPaintLine],
    origin: Point<Pixels>,
    position: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
) -> Option<MarkdownLinkRange> {
    let hit = ai_terminal_hit_test(lines, origin, position, cell_width, line_height)?;
    ai_terminal_link_for_hit(lines, &hit)
}

fn ai_terminal_link_for_hit(
    lines: &[AiTerminalPaintLine],
    hit: &AiTerminalHit,
) -> Option<MarkdownLinkRange> {
    lines
        .iter()
        .find(|line| line.surface_id == hit.surface_id)
        .and_then(|line| {
            line.link_ranges
                .iter()
                .find(|range| range.range.contains(&hit.index))
        })
        .cloned()
}

fn ai_terminal_surface_grid_point_from_position(
    screen: &TerminalScreenSnapshot,
    origin: Point<Pixels>,
    position: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
) -> (i32, usize) {
    let max_column = usize::from(screen.cols.saturating_sub(1));
    let max_visible_line = i32::from(screen.rows.saturating_sub(1));
    let relative_x = (position.x - origin.x).max(px(0.0));
    let relative_y = (position.y - origin.y).max(px(0.0));
    let column = ((relative_x / cell_width) as usize).min(max_column);
    let visible_line = ((relative_y / line_height) as i32).clamp(0, max_visible_line);
    (visible_line - screen.display_offset as i32, column)
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

fn ai_terminal_link_ranges(text: &str) -> Vec<MarkdownLinkRange> {
    let mut link_ranges = Vec::new();
    let mut segment_start = None;

    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(start) = segment_start.take() {
                ai_terminal_push_link_range(&mut link_ranges, text, start, index);
            }
            continue;
        }

        if segment_start.is_none() {
            segment_start = Some(index);
        }
    }

    if let Some(start) = segment_start {
        ai_terminal_push_link_range(&mut link_ranges, text, start, text.len());
    }

    link_ranges
}

fn ai_terminal_push_link_range(
    link_ranges: &mut Vec<MarkdownLinkRange>,
    text: &str,
    start: usize,
    end: usize,
) {
    let Some((range, raw_target)) = ai_terminal_normalize_link_candidate(text, start..end) else {
        return;
    };

    if let Some(previous) = link_ranges.last_mut()
        && previous.raw_target == raw_target
        && previous.range.end == range.start
    {
        previous.range.end = range.end;
        return;
    }

    link_ranges.push(MarkdownLinkRange { range, raw_target });
}

fn ai_terminal_normalize_link_candidate(
    text: &str,
    mut range: Range<usize>,
) -> Option<(Range<usize>, String)> {
    let trimmed_start = text[range.clone()]
        .find(|ch: char| !matches!(ch, '(' | '[' | '{' | '<' | '"' | '\''))
        .map(|offset| range.start + offset)?;
    range.start = trimmed_start;

    let trimmed_slice = &text[range.clone()];
    let trimmed_end = trimmed_slice
        .trim_end_matches(|ch: char| {
            matches!(ch, '.' | ',' | ';' | ')' | ']' | '}' | '>' | '"' | '\'')
        })
        .len();
    range.end = range.start + trimmed_end;
    if range.is_empty() {
        return None;
    }

    let raw_target = text[range.clone()].to_string();
    if ai_terminal_is_url_target(raw_target.as_str()) {
        return Some((range, raw_target));
    }

    if let Some(normalized_target) = ai_terminal_normalize_file_target(raw_target.as_str())
    {
        return Some((range, normalized_target));
    }

    None
}

fn ai_terminal_is_url_target(raw_target: &str) -> bool {
    let normalized = raw_target.trim().to_ascii_lowercase();
    normalized.starts_with("http://")
        || normalized.starts_with("https://")
        || normalized.starts_with("mailto:")
}

fn ai_terminal_is_file_target(raw_target: &str) -> bool {
    let path_part = raw_target
        .split_once('#')
        .map(|(path, _)| path)
        .unwrap_or(raw_target);
    let path_part = path_part
        .rsplit_once(':')
        .and_then(|(path, suffix)| {
            suffix
                .parse::<usize>()
                .ok()
                .filter(|line| *line > 0)
                .map(|_| path)
        })
        .unwrap_or(path_part);

    path_part.starts_with('/')
        || path_part.starts_with("~/")
        || path_part.starts_with("./")
        || path_part.starts_with("../")
        || ai_terminal_is_windows_path(path_part)
        || (path_part.contains('/')
            && path_part
                .rsplit('/')
                .next()
                .is_some_and(|segment| segment.contains('.')))
}

fn ai_terminal_normalize_file_target(raw_target: &str) -> Option<String> {
    if !ai_terminal_is_file_target(raw_target) {
        return None;
    }

    let (path, line) = crate::app::markdown_links::split_markdown_file_target(raw_target);
    if path.is_empty() {
        return None;
    }

    Some(match line {
        Some(line) => format!("{path}:{line}"),
        None => path.to_string(),
    })
}

fn ai_terminal_is_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn ai_terminal_link_ranges_contains(
    link_ranges: &[MarkdownLinkRange],
    start: usize,
    end: usize,
) -> bool {
    link_ranges
        .iter()
        .any(|range| range.range.start < end && range.range.end > start)
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
        if ai_terminal_cell_is_wide_spacer(cell.flags) {
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

fn ai_terminal_render_character(character: char) -> char {
    if character == '\0' || character.is_control() {
        ' '
    } else {
        character
    }
}

fn ai_terminal_cell_is_wide_spacer(flags: u16) -> bool {
    flags
        & (AI_TERMINAL_WIDE_CHAR_SPACER_FLAG | AI_TERMINAL_LEADING_WIDE_CHAR_SPACER_FLAG)
        != 0
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
