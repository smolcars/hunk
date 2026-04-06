use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    App, Bounds, ContentMask, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Window,
};

use crate::app::ai_workspace_render::{ai_workspace_hit_test, paint_ai_workspace_block};
use crate::app::{AiPressedMarkdownLink, DiffViewer, ai_workspace_session};

pub(crate) struct AiWorkspaceSurfaceElement {
    pub(crate) view: gpui::Entity<DiffViewer>,
    pub(crate) snapshot: Rc<ai_workspace_session::AiWorkspaceSurfaceSnapshot>,
    pub(crate) selection: Option<ai_workspace_session::AiWorkspaceSelection>,
    pub(crate) ui_font_family: gpui::SharedString,
    pub(crate) mono_font_family: gpui::SharedString,
    pub(crate) workspace_root: Option<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct AiWorkspaceSurfaceLayout {
    hitbox: Hitbox,
}

impl IntoElement for AiWorkspaceSurfaceElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for AiWorkspaceSurfaceElement {
    type RequestLayoutState = ();
    type PrepaintState = AiWorkspaceSurfaceLayout;

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
        bounds: Bounds<gpui::Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        AiWorkspaceSurfaceLayout {
            hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<gpui::Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let paint_started_at = Instant::now();
        let hitbox = layout.hitbox.clone();
        let snapshot = self.snapshot.clone();
        let view = self.view.clone();
        let workspace_root = self.workspace_root.clone();

        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || !matches!(event.button, MouseButton::Left | MouseButton::Middle)
                || !hitbox.is_hovered(window)
            {
                return;
            }

            view.read(cx).record_ai_workspace_surface_hit_test();
            let Some(hit) = ai_workspace_hit_test(
                snapshot.as_ref(),
                event.position,
                hitbox.bounds,
                workspace_root.as_deref(),
            ) else {
                return;
            };

            view.update(cx, |this, cx| {
                if let Some(toggle_row_id) = hit.toggle_row_id.clone()
                    && event.button == MouseButton::Left
                {
                    this.ai_workspace_toggle_row_expansion(toggle_row_id, cx);
                    cx.stop_propagation();
                    return;
                }

                let pressed_link = hit.text_hit.as_ref().and_then(|text_hit| {
                    text_hit
                        .link_target
                        .clone()
                        .map(|raw_target| AiPressedMarkdownLink {
                            surface_id: text_hit.surface_id.clone(),
                            raw_target,
                            mouse_down_position: event.position,
                            dragged: false,
                        })
                });
                this.ai_set_pressed_markdown_link(pressed_link);
                this.ai_select_workspace_selection(hit.selection.clone(), cx);
                if let Some(text_hit) = hit.text_hit.as_ref() {
                    this.ai_begin_text_selection(
                        hit.selection.block_id.clone(),
                        text_hit.selection_surfaces.clone(),
                        text_hit.surface_id.as_str(),
                        text_hit.index,
                        window,
                        cx,
                    );
                }
                cx.stop_propagation();
            });
        });

        let snapshot_for_mouse_move = self.snapshot.clone();
        let view_for_mouse_move = self.view.clone();
        let workspace_root_for_mouse_move = self.workspace_root.clone();
        let hitbox_for_mouse_move = layout.hitbox.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }

            view_for_mouse_move.update(cx, |this, _| {
                this.ai_mark_pressed_markdown_link_dragged(event.position);
            });

            let dragging_selection = view_for_mouse_move
                .read(cx)
                .ai_text_selection
                .as_ref()
                .is_some_and(|selection| selection.dragging);
            if !dragging_selection {
                return;
            }

            let Some(hit) = ai_workspace_hit_test(
                snapshot_for_mouse_move.as_ref(),
                event.position,
                hitbox_for_mouse_move.bounds,
                workspace_root_for_mouse_move.as_deref(),
            ) else {
                return;
            };
            let Some(text_hit) = hit.text_hit.as_ref() else {
                return;
            };

            view_for_mouse_move.update(cx, |this, cx| {
                this.ai_update_text_selection(text_hit.surface_id.as_str(), text_hit.index, cx);
            });
        });

        let snapshot_for_mouse_up = self.snapshot.clone();
        let view_for_mouse_up = self.view.clone();
        let workspace_root_for_mouse_up = self.workspace_root.clone();
        let hitbox_for_mouse_up = layout.hitbox.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                return;
            }

            view_for_mouse_up.update(cx, |this, cx| {
                this.ai_end_text_selection(cx);
                if !hitbox_for_mouse_up.is_hovered(window) {
                    let _ = this.ai_take_pressed_markdown_link();
                }
            });

            if !hitbox_for_mouse_up.is_hovered(window) {
                return;
            }

            view_for_mouse_up.update(cx, |this, cx| {
                let Some(pressed_link) = this.ai_take_pressed_markdown_link() else {
                    return;
                };
                let Some(hit) = ai_workspace_hit_test(
                    snapshot_for_mouse_up.as_ref(),
                    event.position,
                    hitbox_for_mouse_up.bounds,
                    workspace_root_for_mouse_up.as_deref(),
                ) else {
                    return;
                };
                let Some(text_hit) = hit.text_hit.as_ref() else {
                    return;
                };
                if pressed_link.dragged || pressed_link.surface_id != text_hit.surface_id {
                    return;
                }
                let activated = text_hit
                    .link_target
                    .as_ref()
                    .is_some_and(|target| target == &pressed_link.raw_target);
                if activated {
                    this.activate_markdown_link(pressed_link.raw_target, Some(window), cx);
                }
            });
        });

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for block in &self.snapshot.viewport.visible_blocks {
                paint_ai_workspace_block(
                    window,
                    cx,
                    bounds,
                    self.snapshot.scroll_top_px,
                    block,
                    self.selection
                        .as_ref()
                        .is_some_and(|selection| selection.matches_block(block.block.id.as_str())),
                    self.view.clone(),
                    self.ui_font_family.clone(),
                    self.mono_font_family.clone(),
                    self.workspace_root.as_deref(),
                );
            }
        });
        self.view.read(cx).record_ai_workspace_surface_paint_timing(
            paint_started_at.elapsed(),
            self.snapshot.viewport.visible_blocks.len(),
        );
    }
}
