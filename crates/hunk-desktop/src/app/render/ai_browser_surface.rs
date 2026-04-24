pub(crate) struct AiBrowserSurfaceElement {
    pub(crate) view: Entity<DiffViewer>,
    pub(crate) thread_id: String,
    pub(crate) image: Arc<gpui::RenderImage>,
}

pub(crate) struct AiBrowserSurfaceLayout {
    hitbox: gpui::Hitbox,
}

impl gpui::IntoElement for AiBrowserSurfaceElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for AiBrowserSurfaceElement {
    type RequestLayoutState = ();
    type PrepaintState = AiBrowserSurfaceLayout;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
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
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> Self::PrepaintState {
        let width = bounds.size.width.as_f32().round().max(1.0) as u32;
        let height = bounds.size.height.as_f32().round().max(1.0) as u32;
        let scale = window.scale_factor() as f32;
        let thread_id = self.thread_id.clone();
        self.view.update(cx, |this, cx| {
            this.ai_browser_surface_resize(thread_id.as_str(), width, height, scale, cx);
        });

        AiBrowserSurfaceLayout {
            hitbox: window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut gpui::Window,
        _cx: &mut gpui::App,
    ) {
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            if let Err(err) =
                window.paint_image(bounds, gpui::Corners::default(), self.image.clone(), 0, false)
            {
                eprintln!("failed to paint embedded browser frame: {err:#}");
            }
        });

        let hitbox = layout.hitbox.clone();
        let view = self.view.clone();
        let thread_id = self.thread_id.clone();
        window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }
            let point = browser_surface_point(hitbox.bounds, event.position);
            let handled = view.update(cx, |this, cx| {
                this.ai_browser_surface_mouse_down(
                    thread_id.as_str(),
                    point,
                    event.button,
                    event.modifiers,
                    window,
                    cx,
                )
            });
            if handled {
                cx.stop_propagation();
            }
        });

        let hitbox = layout.hitbox.clone();
        let view = self.view.clone();
        let thread_id = self.thread_id.clone();
        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }
            let point = browser_surface_point(hitbox.bounds, event.position);
            let handled = view.update(cx, |this, cx| {
                this.ai_browser_surface_mouse_move(
                    thread_id.as_str(),
                    point,
                    event.modifiers,
                    cx,
                )
            });
            if handled {
                cx.stop_propagation();
            }
        });

        let hitbox = layout.hitbox.clone();
        let view = self.view.clone();
        let thread_id = self.thread_id.clone();
        window.on_mouse_event(move |event: &gpui::ScrollWheelEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }
            let point = browser_surface_point(hitbox.bounds, event.position);
            let handled = view.update(cx, |this, cx| {
                this.ai_browser_surface_scroll_wheel(thread_id.as_str(), point, event, cx)
            });
            if handled {
                cx.stop_propagation();
            }
        });
    }
}

fn browser_surface_point(
    bounds: gpui::Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> hunk_browser::BrowserPhysicalPoint {
    hunk_browser::BrowserPhysicalPoint {
        x: (position.x - bounds.origin.x)
            .max(Pixels::ZERO)
            .min(bounds.size.width)
            .as_f32()
            .round() as i32,
        y: (position.y - bounds.origin.y)
            .max(Pixels::ZERO)
            .min(bounds.size.height)
            .as_f32()
            .round() as i32,
    }
}
