use gpui::*;

use super::paint::{
    LineNumberPaintParams, build_row_syntax_spans, build_text_runs_for_row, matching_bracket_pair,
    paint_cursor, paint_fold_marker, paint_indent_guides, paint_line_number,
    paint_matching_brackets, paint_overlays, paint_scope_highlight, paint_selection,
    paint_whitespace_markers, selection_range_for_row,
};
use super::{EditorLayout, FilesEditorElement};

impl FilesEditorElement {
    pub(crate) fn new(
        state: super::SharedFilesEditor,
        is_focused: bool,
        style: TextStyle,
        palette: super::FilesEditorPalette,
    ) -> Self {
        Self {
            state,
            is_focused,
            style,
            palette,
        }
    }
}

impl IntoElement for FilesEditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FilesEditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorLayout;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = gpui::Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let font_id = window.text_system().resolve_font(&self.style.font());
        let font_size = self.style.font_size.to_pixels(window.rem_size());
        let line_height = self.style.line_height_in_pixels(window.rem_size());
        let cell_width = window
            .text_system()
            .advance(font_id, font_size, 'm')
            .map(|size| size.width)
            .unwrap_or_else(|_| px(8.0));
        let columns = (bounds.size.width / cell_width).floor().max(1.0) as usize;
        let rows = (bounds.size.height / line_height).floor().max(1.0) as usize;

        let gutter_columns = self
            .state
            .borrow()
            .editor
            .display_snapshot()
            .line_count
            .max(1)
            .to_string()
            .len()
            + 1;
        let editor_columns = columns.saturating_sub(gutter_columns + 2).max(1);
        let display_snapshot = self.state.borrow_mut().apply_layout(editor_columns, rows);

        EditorLayout {
            line_height,
            font_size,
            cell_width,
            gutter_columns,
            hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
            display_snapshot,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mouse_down_layout = layout.clone();
        let mouse_drag_layout = layout.clone();
        let mouse_state = self.state.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _cx| {
            if phase == DispatchPhase::Bubble
                && event.button == gpui::MouseButton::Left
                && mouse_down_layout.hitbox.is_hovered(window)
                && mouse_state.borrow_mut().handle_mouse_down(
                    event.position,
                    &mouse_down_layout,
                    event.modifiers.shift,
                )
            {
                window.refresh();
            }
        });
        let mouse_state = self.state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
            if phase == DispatchPhase::Bubble
                && event.dragging()
                && mouse_state
                    .borrow_mut()
                    .handle_mouse_drag(event.position, &mouse_drag_layout)
            {
                window.refresh();
            }
        });
        let mouse_state = self.state.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _cx| {
            if phase == DispatchPhase::Bubble
                && event.button == gpui::MouseButton::Left
                && mouse_state.borrow_mut().handle_mouse_up()
            {
                window.refresh();
            }
        });

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(fill(bounds, self.palette.background));

            let content_origin = point(layout.content_origin_x(), bounds.origin.y + px(1.0));
            let gutter_x = bounds.origin.x + (layout.cell_width * layout.gutter_columns as f32);
            window.paint_quad(fill(
                Bounds {
                    origin: point(gutter_x + px(4.0), bounds.origin.y),
                    size: size(px(1.0), bounds.size.height),
                },
                self.palette.border,
            ));

            let state = self.state.borrow();
            let selection = state.editor.selection();
            let current_line = selection.head.line;
            let snapshot = state.editor.buffer().snapshot();
            let syntax_spans_by_row = build_row_syntax_spans(
                &layout.display_snapshot.visible_rows,
                &state.syntax_highlights,
                &snapshot,
            );
            let active_scope = state.active_scope();
            let matching_brackets = matching_bracket_pair(&snapshot, selection.head);
            let mut row_origin = content_origin;
            for row in &layout.display_snapshot.visible_rows {
                paint_scope_highlight(window, row, row_origin, layout, self.palette, active_scope);
                if row.source_line == current_line {
                    window.paint_quad(fill(
                        Bounds {
                            origin: point(bounds.origin.x, row_origin.y),
                            size: size(bounds.size.width, layout.line_height),
                        },
                        self.palette.active_line_background,
                    ));
                }
                if let Some(selection_range) = selection_range_for_row(row, selection) {
                    paint_selection(
                        window,
                        row_origin,
                        layout,
                        selection_range,
                        self.palette.selection_background,
                    );
                }
                for highlight in &row.search_highlights {
                    paint_selection(
                        window,
                        row_origin,
                        layout,
                        highlight.start_column..highlight.end_column,
                        hsla(
                            self.palette.selection_background.h,
                            self.palette.selection_background.s,
                            self.palette.selection_background.l,
                            0.35,
                        ),
                    );
                }
                paint_overlays(window, row, row_origin, layout, self.palette);
                paint_matching_brackets(
                    window,
                    row,
                    row_origin,
                    layout,
                    self.palette,
                    matching_brackets,
                );

                paint_line_number(
                    window,
                    cx,
                    row,
                    layout,
                    LineNumberPaintParams {
                        origin: row_origin,
                        current_line,
                        palette: self.palette,
                        font: self.style.font(),
                    },
                );
                paint_fold_marker(
                    window,
                    cx,
                    row,
                    layout,
                    row_origin,
                    self.palette,
                    self.style.font(),
                    state.is_foldable_line(row.source_line),
                    state.is_folded_line(row.source_line),
                );
                paint_indent_guides(window, row, row_origin, layout, self.palette, 4);

                let runs = build_text_runs_for_row(
                    row,
                    syntax_spans_by_row
                        .get(&row.row_index)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]),
                    self.style.font(),
                    self.palette.default_foreground,
                    self.palette.muted_foreground,
                    cx,
                );
                let line = window.text_system().shape_line(
                    row.text.clone().into(),
                    layout.font_size,
                    &runs,
                    None,
                );
                let _ = line.paint(
                    row_origin,
                    layout.line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
                paint_whitespace_markers(
                    window,
                    cx,
                    row,
                    row_origin,
                    layout,
                    self.palette,
                    self.style.font(),
                );
                row_origin.y += layout.line_height;
            }

            if self.is_focused {
                paint_cursor(
                    window,
                    &layout.display_snapshot.visible_rows,
                    selection.head,
                    content_origin,
                    layout,
                    self.palette.cursor,
                );
            }

            if layout.hitbox.is_hovered(window) {
                window.set_cursor_style(CursorStyle::IBeam, &layout.hitbox);
            }
        });
    }
}
