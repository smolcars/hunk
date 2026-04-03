#[derive(Clone)]
enum ReviewWorkspacePaintedRowKind {
    FileHeader {
        paint: Box<ReviewWorkspaceFileHeaderPaint>,
    },
    Code {
        left: Box<ReviewWorkspaceCodeRowCellPaint>,
        right: Box<ReviewWorkspaceCodeRowCellPaint>,
    },
    Meta(ReviewWorkspaceMetaRowPaint),
}

#[derive(Clone)]
struct ReviewWorkspacePaintedRow {
    row_index: usize,
    top_px: usize,
    height_px: usize,
    kind: ReviewWorkspacePaintedRowKind,
}

#[derive(Clone)]
struct ReviewWorkspaceViewportElement {
    view: Entity<DiffViewer>,
    rows: std::rc::Rc<Vec<ReviewWorkspacePaintedRow>>,
    center_divider: gpui::Hsla,
    mono_font_family: SharedString,
}

#[derive(Clone)]
struct ReviewWorkspaceSectionLayout {
    hitbox: gpui::Hitbox,
}

impl ReviewWorkspaceViewportElement {
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

impl IntoElement for ReviewWorkspaceViewportElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ReviewWorkspaceViewportElement {
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
                    origin: point(bounds.origin.x, bounds.origin.y + px(row.top_px as f32)),
                    size: gpui::size(bounds.size.width, px(row.height_px as f32)),
                };
                match &row.kind {
                    ReviewWorkspacePaintedRowKind::FileHeader { paint } => {
                        paint_review_workspace_file_header_row(
                            window,
                            cx,
                            row_bounds,
                            paint,
                            self.mono_font_family.clone(),
                        );
                    }
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
            let top = row.top_px as f32;
            let bottom = top + row.height_px as f32;
            local_y >= top && local_y < bottom
        })
        .map(|row| row.row_index)
}

impl DiffViewer {
    fn build_review_workspace_viewport_painted_rows(
        &self,
        viewport: &review_workspace_session::ReviewWorkspaceViewportSnapshot,
        viewport_origin_px: usize,
        layout: Option<DiffColumnLayout>,
        cx: &mut Context<Self>,
    ) -> Vec<ReviewWorkspacePaintedRow> {
        viewport
            .sections
            .iter()
            .flat_map(|viewport_section| viewport_section.rows.iter())
            .filter_map(|viewport_row| {
                let row_ix = viewport_row.row_index;
                let is_selected = self.is_row_selected(row_ix);
                let kind = if viewport_row.stream_kind == DiffStreamRowKind::FileHeader {
                    let path = viewport_row.file_path.as_deref()?;
                    let status = viewport_row.file_status.unwrap_or(FileStatus::Unknown);
                    let stats = viewport_row.file_line_stats.unwrap_or_default();
                    ReviewWorkspacePaintedRowKind::FileHeader {
                        paint: Box::new(self.build_review_workspace_file_header_paint(
                            path,
                            status,
                            stats,
                            is_selected,
                            cx,
                        )),
                    }
                } else {
                    match viewport_row.row_kind {
                        DiffRowKind::Code => {
                            let stable_row_id = viewport_row.stable_id;
                            let left = self.build_review_workspace_code_row_cell(
                                stable_row_id,
                                is_selected,
                                DiffCellRenderSpec {
                                    side: "left",
                                    line: viewport_row.left_line,
                                    cell_kind: viewport_row.left_cell_kind,
                                    peer_kind: viewport_row.right_cell_kind,
                                    panel_width: layout.map(|layout| layout.left_panel_width),
                                },
                                viewport_row,
                                cx,
                            );
                            let right = self.build_review_workspace_code_row_cell(
                                stable_row_id,
                                is_selected,
                                DiffCellRenderSpec {
                                    side: "right",
                                    line: viewport_row.right_line,
                                    cell_kind: viewport_row.right_cell_kind,
                                    peer_kind: viewport_row.left_cell_kind,
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
                                self.build_review_workspace_meta_row_paint(
                                    viewport_row.row_kind,
                                    &viewport_row.text,
                                    is_selected,
                                    cx,
                                ),
                            )
                        }
                    }
                };
                Some(ReviewWorkspacePaintedRow {
                    row_index: row_ix,
                    top_px: viewport_row.surface_top_px.saturating_sub(viewport_origin_px),
                    height_px: viewport_row.height_px,
                    kind,
                })
            })
            .collect()
    }

    fn render_review_workspace_viewport_element(
        &self,
        viewport: &review_workspace_session::ReviewWorkspaceViewportSnapshot,
        viewport_origin_px: usize,
        layout: Option<DiffColumnLayout>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let painted_rows = self.build_review_workspace_viewport_painted_rows(
            viewport,
            viewport_origin_px,
            layout,
            cx,
        );
        let chrome = hunk_diff_chrome(cx.theme(), cx.theme().mode.is_dark());
        ReviewWorkspaceViewportElement::new(
            cx.entity(),
            painted_rows,
            chrome.center_divider,
            cx.theme().mono_font_family.clone(),
        )
        .into_any_element()
    }
}
