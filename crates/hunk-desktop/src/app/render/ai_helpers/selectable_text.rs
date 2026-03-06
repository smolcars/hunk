struct AiSelectableStyledText {
    element_id: gpui::ElementId,
    row_id: String,
    surface_id: String,
    selection_surfaces: std::sync::Arc<[AiTextSelectionSurfaceSpec]>,
    text: StyledText,
    selection_range: Option<std::ops::Range<usize>>,
    selection_background: Hsla,
    view: Entity<DiffViewer>,
}

impl AiSelectableStyledText {
    fn new(
        row_id: impl Into<String>,
        surface_id: impl Into<String>,
        selection_surfaces: std::sync::Arc<[AiTextSelectionSurfaceSpec]>,
        text: StyledText,
        selection_range: Option<std::ops::Range<usize>>,
        selection_background: Hsla,
        view: Entity<DiffViewer>,
    ) -> Self {
        let surface_id = surface_id.into();
        Self {
            element_id: surface_id.clone().into(),
            row_id: row_id.into(),
            surface_id,
            selection_surfaces,
            text,
            selection_range,
            selection_background,
            view,
        }
    }

    fn paint_selection(&self, layout: &gpui::TextLayout, window: &mut Window, cx: &mut App) {
        let Some(selection_range) = self.selection_range.clone() else {
            return;
        };
        if selection_range.is_empty() {
            return;
        }

        let selection_start = layout.position_for_index(selection_range.start);
        let selection_end = layout.position_for_index(selection_range.end);
        let bounds = layout.bounds();
        let line_height = layout.line_height();
        let Some((start_position, end_position)) = selection_start.zip(selection_end) else {
            return;
        };

        if start_position.y == end_position.y {
            window.paint_quad(gpui::fill(
                gpui::Bounds::from_corners(
                    start_position,
                    gpui::point(end_position.x, end_position.y + line_height),
                ),
                self.selection_background,
            ));
            return;
        }

        window.paint_quad(gpui::fill(
            gpui::Bounds::from_corners(
                start_position,
                gpui::point(bounds.right(), start_position.y + line_height),
            ),
            self.selection_background,
        ));

        if end_position.y > start_position.y + line_height {
            window.paint_quad(gpui::fill(
                gpui::Bounds::from_corners(
                    gpui::point(bounds.left(), start_position.y + line_height),
                    gpui::point(bounds.right(), end_position.y),
                ),
                self.selection_background,
            ));
        }

        window.paint_quad(gpui::fill(
            gpui::Bounds::from_corners(
                gpui::point(bounds.left(), end_position.y),
                gpui::point(end_position.x, end_position.y + line_height),
            ),
            self.selection_background,
        ));

        let _ = cx;
    }
}

impl gpui::Element for AiSelectableStyledText {
    type RequestLayoutState = ();
    type PrepaintState = gpui::Hitbox;

    fn id(&self) -> Option<gpui::ElementId> {
        Some(self.element_id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(None, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.text.prepaint(None, inspector_id, bounds, state, window, cx);
        window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let text_layout = self.text.layout().clone();
        let view = self.view.clone();
        let surface_id = self.surface_id.clone();
        let row_id = self.row_id.clone();
        let hitbox_for_mouse_down = hitbox.clone();
        let hitbox_for_mouse_move = hitbox.clone();
        let selection_surfaces = self.selection_surfaces.clone();

        window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !hitbox_for_mouse_down.is_hovered(window)
            {
                return;
            }

            let index = match text_layout.index_for_position(event.position) {
                Ok(index) | Err(index) => index,
            };
            view.update(cx, |this, cx| {
                this.ai_begin_text_selection(
                    row_id.clone(),
                    selection_surfaces.clone(),
                    surface_id.as_str(),
                    index,
                    window,
                    cx,
                );
            });
        });

        let text_layout = self.text.layout().clone();
        let view = self.view.clone();
        let surface_id = self.surface_id.clone();
        let row_id = self.row_id.clone();
        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble {
                return;
            }

            if !hitbox_for_mouse_move.is_hovered(window) {
                return;
            }

            let Some(is_dragging_row) = view.read(cx).ai_text_selection.as_ref().map(|selection| {
                selection.row_id == row_id && selection.dragging
            }) else {
                return;
            };
            if !is_dragging_row {
                return;
            }

            let index = match text_layout.index_for_position(event.position) {
                Ok(index) | Err(index) => index,
            };
            view.update(cx, |this, cx| {
                this.ai_update_text_selection(surface_id.as_str(), index, cx);
            });
        });

        let view = self.view.clone();
        window.on_mouse_event(move |_: &gpui::MouseUpEvent, phase, _window, cx| {
            if phase != gpui::DispatchPhase::Bubble {
                return;
            }
            view.update(cx, |this, cx| {
                this.ai_end_text_selection(cx);
            });
        });

        if hitbox.is_hovered(window) {
            window.set_cursor_style(gpui::CursorStyle::IBeam, hitbox);
        }

        self.paint_selection(&self.text.layout().clone(), window, cx);
        self.text
            .paint(None, inspector_id, bounds, &mut (), &mut (), window, cx);
    }
}

impl IntoElement for AiSelectableStyledText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[allow(clippy::too_many_arguments)]
fn ai_render_selectable_styled_text(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    surface_id: impl Into<String>,
    selection_surfaces: std::sync::Arc<[AiTextSelectionSurfaceSpec]>,
    styled_text: StyledText,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AiSelectableStyledText {
    let surface_id = surface_id.into();
    let selection_range = this.ai_text_selection_range_for_surface(surface_id.as_str());
    AiSelectableStyledText::new(
        row_id,
        surface_id,
        selection_surfaces,
        styled_text,
        selection_range,
        hunk_text_selection_background(cx.theme(), is_dark),
        view,
    )
}

fn ai_timeline_text_surface_id(
    row_id: &str,
    surface_kind: &str,
    surface_index: impl std::fmt::Display,
) -> String {
    format!("{row_id}\u{1f}{surface_kind}\u{1f}{surface_index}")
}

fn ai_text_selection_surfaces(
    surfaces: Vec<AiTextSelectionSurfaceSpec>,
) -> std::sync::Arc<[AiTextSelectionSurfaceSpec]> {
    surfaces.into()
}
