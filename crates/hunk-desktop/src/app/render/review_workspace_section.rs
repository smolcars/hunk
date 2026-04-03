#[derive(Clone)]
enum ReviewWorkspacePaintedRowKind {
    Skip,
    Code {
        left: Box<ReviewWorkspaceCodeRowCellPaint>,
        right: Box<ReviewWorkspaceCodeRowCellPaint>,
    },
    Meta(ReviewWorkspaceMetaRowPaint),
}

#[derive(Clone)]
struct ReviewWorkspacePaintedRow {
    row_index: usize,
    local_top_px: usize,
    height_px: usize,
    kind: ReviewWorkspacePaintedRowKind,
}

#[derive(Clone)]
struct ReviewWorkspaceSectionElement {
    view: Entity<DiffViewer>,
    rows: std::rc::Rc<Vec<ReviewWorkspacePaintedRow>>,
    center_divider: gpui::Hsla,
    mono_font_family: SharedString,
}

#[derive(Clone)]
struct ReviewWorkspaceSectionLayout {
    hitbox: gpui::Hitbox,
}

impl ReviewWorkspaceSectionElement {
    fn new(
        view: Entity<DiffViewer>,
        rows: Vec<ReviewWorkspacePaintedRow>,
        center_divider: gpui::Hsla,
        mono_font_family: SharedString,
    ) -> Self {
        Self {
            view,
            rows: std::rc::Rc::new(rows),
            center_divider,
            mono_font_family,
        }
    }
}

impl IntoElement for ReviewWorkspaceSectionElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ReviewWorkspaceSectionElement {
    type RequestLayoutState = ();
    type PrepaintState = ReviewWorkspaceSectionLayout;

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
        ReviewWorkspaceSectionLayout {
            hitbox: window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
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
        let rows = self.rows.clone();
        let hitbox = layout.hitbox.clone();
        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }
            let Some(row_ix) =
                review_workspace_row_at_position(rows.as_ref(), event.position, hitbox.bounds.origin)
            else {
                return;
            };
            view.update(cx, |this, cx| match event.button {
                gpui::MouseButton::Left | gpui::MouseButton::Middle => {
                    this.on_diff_row_mouse_down(row_ix, event, window, cx);
                }
                gpui::MouseButton::Right => {
                    this.open_diff_row_context_menu(row_ix, event.position, window, cx);
                    cx.stop_propagation();
                }
                _ => {}
            });
        });

        let rows = self.rows.clone();
        let hitbox = layout.hitbox.clone();
        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }
            let Some(row_ix) =
                review_workspace_row_at_position(rows.as_ref(), event.position, hitbox.bounds.origin)
            else {
                return;
            };
            view.update(cx, |this, cx| {
                this.on_diff_row_mouse_move(row_ix, event, window, cx);
            });
        });

        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble {
                return;
            }
            if matches!(event.button, gpui::MouseButton::Left | gpui::MouseButton::Middle) {
                view.update(cx, |this, cx| {
                    this.on_diff_row_mouse_up(event, window, cx);
                });
            }
        });

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for row in self.rows.iter() {
                let row_bounds = Bounds {
                    origin: point(bounds.origin.x, bounds.origin.y + px(row.local_top_px as f32)),
                    size: gpui::size(bounds.size.width, px(row.height_px as f32)),
                };
                match &row.kind {
                    ReviewWorkspacePaintedRowKind::Skip => {}
                    ReviewWorkspacePaintedRowKind::Code { left, right } => {
                        paint_review_workspace_code_row(
                            window,
                            cx,
                            row_bounds,
                            left,
                            right,
                            self.center_divider,
                            self.mono_font_family.clone(),
                        );
                    }
                    ReviewWorkspacePaintedRowKind::Meta(meta) => {
                        paint_review_workspace_meta_row(
                            window,
                            cx,
                            row_bounds,
                            meta,
                            self.mono_font_family.clone(),
                        );
                    }
                }
            }
        });
    }
}

fn review_workspace_row_at_position(
    rows: &[ReviewWorkspacePaintedRow],
    position: gpui::Point<gpui::Pixels>,
    origin: gpui::Point<gpui::Pixels>,
) -> Option<usize> {
    let local_y = (position.y - origin.y).max(gpui::Pixels::ZERO).as_f32();
    rows.iter()
        .find(|row| {
            let top = row.local_top_px as f32;
            let bottom = top + row.height_px as f32;
            local_y >= top && local_y < bottom
        })
        .map(|row| row.row_index)
}

impl DiffViewer {
    fn build_review_workspace_section_painted_rows(
        &self,
        viewport_section: &review_workspace_session::ReviewWorkspaceViewportSection,
        layout: Option<DiffColumnLayout>,
        cx: &mut Context<Self>,
    ) -> Vec<ReviewWorkspacePaintedRow> {
        viewport_section
            .rows
            .iter()
            .filter_map(|viewport_row| {
                let row_ix = viewport_row.row_index;
                let row = self.active_diff_row(row_ix)?;
                let is_selected = self.is_row_selected(row_ix);
                let kind = if self
                    .active_diff_row_metadata(row_ix)
                    .is_some_and(|meta| meta.kind == DiffStreamRowKind::FileHeader)
                {
                    ReviewWorkspacePaintedRowKind::Skip
                } else {
                    match row.kind {
                        DiffRowKind::Code => {
                            let stable_row_id = self.diff_row_stable_id(row_ix);
                            let left = self.build_review_workspace_code_row_cell(
                                stable_row_id,
                                row,
                                is_selected,
                                DiffCellRenderSpec {
                                    row_ix,
                                    side: "left",
                                    cell: &row.left,
                                    peer_kind: row.right.kind,
                                    panel_width: layout.map(|layout| layout.left_panel_width),
                                },
                                viewport_row,
                                cx,
                            );
                            let right = self.build_review_workspace_code_row_cell(
                                stable_row_id,
                                row,
                                is_selected,
                                DiffCellRenderSpec {
                                    row_ix,
                                    side: "right",
                                    cell: &row.right,
                                    peer_kind: row.left.kind,
                                    panel_width: layout.map(|layout| layout.right_panel_width),
                                },
                                viewport_row,
                                cx,
                            );
                            ReviewWorkspacePaintedRowKind::Code {
                                left: Box::new(left),
                                right: Box::new(right),
                            }
                        }
                        DiffRowKind::HunkHeader | DiffRowKind::Meta | DiffRowKind::Empty => {
                            ReviewWorkspacePaintedRowKind::Meta(
                                self.build_review_workspace_meta_row_paint(row, is_selected, cx),
                            )
                        }
                    }
                };
                Some(ReviewWorkspacePaintedRow {
                    row_index: row_ix,
                    local_top_px: viewport_row.local_top_px,
                    height_px: viewport_row.height_px,
                    kind,
                })
            })
            .collect()
    }

    fn render_review_workspace_section_element(
        &self,
        viewport_section: &review_workspace_session::ReviewWorkspaceViewportSection,
        layout: Option<DiffColumnLayout>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let painted_rows = self.build_review_workspace_section_painted_rows(
            viewport_section,
            layout,
            cx,
        );
        let chrome = hunk_diff_chrome(cx.theme(), cx.theme().mode.is_dark());
        ReviewWorkspaceSectionElement::new(
            cx.entity(),
            painted_rows,
            chrome.center_divider,
            cx.theme().mono_font_family.clone(),
        )
        .into_any_element()
    }
}
