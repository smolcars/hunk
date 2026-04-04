use std::rc::Rc;

use gpui::{
    App, Bounds, ContentMask, DispatchPhase, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, SharedString, Window, point, px,
};

use crate::app::{DiffViewer, review_workspace_session};

pub(crate) enum WorkspaceSurfaceElement {
    Files(crate::app::native_files_editor::FilesEditorElement),
    Review(ReviewWorkspaceSurfaceElement),
}

impl IntoElement for WorkspaceSurfaceElement {
    type Element = gpui::AnyElement;

    fn into_element(self) -> Self::Element {
        match self {
            Self::Files(element) => element.into_any_element(),
            Self::Review(element) => element.into_any_element(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ReviewWorkspaceSurfaceElement {
    pub(crate) view: gpui::Entity<DiffViewer>,
    pub(crate) viewport: Rc<review_workspace_session::ReviewWorkspaceViewportSnapshot>,
    pub(crate) sticky_file_header: Option<review_workspace_session::ReviewWorkspaceVisibleFileHeader>,
    pub(crate) sticky_file_can_view: bool,
    pub(crate) viewport_origin_px: usize,
    pub(crate) selected_row_range: Option<(usize, usize)>,
    pub(crate) left_panel_width: Option<Pixels>,
    pub(crate) right_panel_width: Option<Pixels>,
    pub(crate) left_line_number_width: f32,
    pub(crate) right_line_number_width: f32,
    pub(crate) center_divider: gpui::Hsla,
    pub(crate) mono_font_family: SharedString,
    pub(crate) ui_font_family: SharedString,
}

#[derive(Clone)]
pub(crate) struct ReviewWorkspaceSurfaceLayout {
    hitbox: gpui::Hitbox,
}

impl IntoElement for ReviewWorkspaceSurfaceElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ReviewWorkspaceSurfaceElement {
    type RequestLayoutState = ();
    type PrepaintState = ReviewWorkspaceSurfaceLayout;

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
        style.size.width = gpui::relative(1.).into();
        style.size.height = gpui::relative(1.).into();
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
        ReviewWorkspaceSurfaceLayout {
            hitbox: window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
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
        let viewport = self.viewport.clone();
        let sticky_file_header = self.sticky_file_header.clone();
        let sticky_file_can_view = self.sticky_file_can_view;
        let viewport_origin_px = self.viewport_origin_px;
        let hitbox = layout.hitbox.clone();
        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }

            if let Some(header) = &sticky_file_header {
                let sticky_bounds = crate::app::render::review_workspace_sticky_header_bounds(
                    hitbox.bounds.origin,
                    hitbox.bounds.size.width,
                );
                if sticky_bounds.contains(&event.position) {
                    if matches!(event.button, MouseButton::Left | MouseButton::Middle) {
                        let controls =
                            crate::app::render::review_workspace_file_header_controls_layout(
                                sticky_bounds,
                            );
                        if controls.collapse_bounds.contains(&event.position) {
                            let path = header.path.clone();
                            view.update(cx, |this, cx| {
                                this.toggle_file_collapsed(path, cx);
                                cx.stop_propagation();
                            });
                            return;
                        }
                        if controls.view_bounds.contains(&event.position) {
                            if !sticky_file_can_view {
                                cx.stop_propagation();
                                return;
                            }
                            let path = header.path.clone();
                            let status = header.status;
                            view.update(cx, |this, cx| {
                                let _ = this.open_file_in_files_workspace(path, status, window, cx);
                                cx.stop_propagation();
                            });
                            return;
                        }
                    }
                    cx.stop_propagation();
                    return;
                }
            }

            let Some(viewport_row) = review_workspace_row_at_position(
                viewport.as_ref(),
                viewport_origin_px,
                event.position,
                hitbox.bounds.origin,
            ) else {
                return;
            };
            let row_bounds = review_workspace_row_bounds(
                viewport_row,
                viewport_origin_px,
                hitbox.bounds.origin,
                hitbox.bounds.size.width,
            );
            if viewport_row.stream_kind == crate::app::data::DiffStreamRowKind::FileHeader
                && matches!(event.button, MouseButton::Left | MouseButton::Middle)
                && let (Some(path), Some(status)) =
                    (viewport_row.file_path.as_ref(), viewport_row.file_status)
            {
                let controls = crate::app::render::review_workspace_file_header_controls_layout(
                    row_bounds,
                );
                if controls.collapse_bounds.contains(&event.position) {
                    let path = path.clone();
                    view.update(cx, |this, cx| {
                        this.toggle_file_collapsed(path, cx);
                        cx.stop_propagation();
                    });
                    return;
                }
                if controls.view_bounds.contains(&event.position) && viewport_row.can_view_file {
                    let path = path.clone();
                    view.update(cx, |this, cx| {
                        let _ = this.open_file_in_files_workspace(path, status, window, cx);
                        cx.stop_propagation();
                    });
                    return;
                }
            }
            if let Some(comment_layout) = crate::app::render::review_workspace_comment_affordance_layout(
                row_bounds,
                viewport_row.show_comment_affordance,
                viewport_row.open_comment_count,
            ) && matches!(event.button, MouseButton::Left | MouseButton::Middle)
                && comment_layout.hit_bounds.contains(&event.position)
            {
                view.update(cx, |this, cx| {
                    this.open_comment_editor_for_row(viewport_row.row_index, window, cx);
                    cx.stop_propagation();
                });
                return;
            }

            view.update(cx, |this, cx| match event.button {
                MouseButton::Left | MouseButton::Middle => {
                    this.on_diff_row_mouse_down(viewport_row.row_index, event, window, cx);
                }
                MouseButton::Right => {
                    this.open_diff_row_context_menu(
                        viewport_row.row_index,
                        event.position,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
                _ => {}
            });
        });

        let viewport = self.viewport.clone();
        let viewport_origin_px = self.viewport_origin_px;
        let hitbox = layout.hitbox.clone();
        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }
            let Some(row) = review_workspace_row_at_position(
                viewport.as_ref(),
                viewport_origin_px,
                event.position,
                hitbox.bounds.origin,
            ) else {
                return;
            };
            view.update(cx, |this, cx| {
                this.on_diff_row_mouse_move(row.row_index, event, window, cx);
            });
        });

        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if matches!(event.button, MouseButton::Left | MouseButton::Middle) {
                view.update(cx, |this, cx| {
                    this.on_diff_row_mouse_up(event, window, cx);
                });
            }
        });

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for viewport_row in self
                .viewport
                .sections
                .iter()
                .flat_map(|section| section.rows.iter())
            {
                let row_bounds = review_workspace_row_bounds(
                    viewport_row,
                    self.viewport_origin_px,
                    bounds.origin,
                    bounds.size.width,
                );
                let is_selected = crate::app::render::review_workspace_row_is_selected(
                    self.selected_row_range,
                    viewport_row.row_index,
                );
                crate::app::render::paint_review_workspace_viewport_row(
                    window,
                    cx,
                    row_bounds,
                    viewport_row,
                    is_selected,
                    self.left_panel_width,
                    self.right_panel_width,
                    self.left_line_number_width,
                    self.right_line_number_width,
                    self.center_divider,
                    self.mono_font_family.clone(),
                    self.ui_font_family.clone(),
                );
            }

            if let Some(header) = self.sticky_file_header.as_ref() {
                let sticky_bounds = crate::app::render::review_workspace_sticky_header_bounds(
                    bounds.origin,
                    bounds.size.width,
                );
                let is_selected = crate::app::render::review_workspace_row_is_selected(
                    self.selected_row_range,
                    header.row_index,
                );
                crate::app::render::paint_review_workspace_sticky_header(
                    window,
                    cx,
                    header,
                    is_selected,
                    self.sticky_file_can_view,
                    sticky_bounds,
                    self.mono_font_family.clone(),
                    self.ui_font_family.clone(),
                );
            }
        });
    }
}

fn review_workspace_row_at_position<'a>(
    viewport: &'a review_workspace_session::ReviewWorkspaceViewportSnapshot,
    viewport_origin_px: usize,
    position: Point<Pixels>,
    origin: Point<Pixels>,
) -> Option<&'a review_workspace_session::ReviewWorkspaceViewportRow> {
    let local_y = (position.y - origin.y).max(Pixels::ZERO).as_f32().floor() as usize;
    viewport.row_at_viewport_position(viewport_origin_px, local_y)
}

fn review_workspace_row_bounds(
    viewport_row: &review_workspace_session::ReviewWorkspaceViewportRow,
    viewport_origin_px: usize,
    origin: Point<Pixels>,
    width: Pixels,
) -> Bounds<Pixels> {
    Bounds {
        origin: point(
            origin.x,
            origin.y
                + px(
                    viewport_row
                        .surface_top_px
                        .saturating_sub(viewport_origin_px) as f32,
                ),
        ),
        size: gpui::size(width, px(viewport_row.height_px as f32)),
    }
}
