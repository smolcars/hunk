use gpui::{Element, ElementId, GlobalElementId, InspectorElementId, LayoutId};

const AI_INLINE_REVIEW_SCROLLBAR_SIZE: f32 = 12.0;
const AI_INLINE_REVIEW_LINE_NUMBER_GAP_PX: f32 = 10.0;
const AI_INLINE_REVIEW_MARKER_WIDTH_PX: f32 = 16.0;
const AI_INLINE_REVIEW_TEXT_LEFT_PADDING_PX: f32 = 12.0;

pub(crate) struct AiInlineReviewSurfaceElement {
    snapshot: Rc<crate::app::ai_inline_review::AiInlineReviewSurfaceSnapshot>,
    viewport_origin_px: usize,
    horizontal_offset: Pixels,
    mono_font_family: SharedString,
    ui_font_family: SharedString,
    old_line_number_width: f32,
    new_line_number_width: f32,
}

impl IntoElement for AiInlineReviewSurfaceElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for AiInlineReviewSurfaceElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for row in &self.snapshot.viewport.rows {
                let row_bounds = Bounds {
                    origin: point(
                        bounds.origin.x,
                        bounds.origin.y
                            + px(
                                row.surface_top_px
                                    .saturating_sub(self.viewport_origin_px) as f32,
                            ),
                    ),
                    size: size(bounds.size.width, px(row.height_px as f32)),
                };
                paint_ai_inline_review_row(
                    window,
                    cx,
                    row_bounds,
                    row,
                    self.horizontal_offset,
                    self.old_line_number_width,
                    self.new_line_number_width,
                    self.mono_font_family.clone(),
                    self.ui_font_family.clone(),
                );
            }

            if let Some(header) = self.snapshot.sticky_file_header.as_ref() {
                let header_bounds = Bounds {
                    origin: bounds.origin,
                    size: size(
                        bounds.size.width,
                        px(crate::app::ai_inline_review::AI_INLINE_REVIEW_FILE_HEADER_HEIGHT_PX as f32),
                    ),
                };
                paint_ai_inline_review_file_header(
                    window,
                    cx,
                    header_bounds,
                    header,
                    self.mono_font_family.clone(),
                    self.ui_font_family.clone(),
                );
            }
        });
    }
}

impl DiffViewer {
    fn render_ai_inline_review_status_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.current_ai_inline_review_mode() == AiInlineReviewMode::WorkingTree {
            return self.render_review_workspace_status_surface(cx);
        }

        let message = self
            .ai_inline_review_error
            .clone()
            .or_else(|| self.ai_inline_review_status_message.clone())
            .unwrap_or_else(|| {
                if self.ai_inline_review_session.is_some() {
                    "AI diff loaded, but the inline surface is unavailable. Reopen the diff to reload."
                        .to_string()
                } else {
                    "Select an AI code-change block to load a historical diff.".to_string()
                }
            });

        div()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .px_4()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(message),
                    ),
            )
            .into_any_element()
    }

    fn current_ai_inline_review_surface_scroll_top_px(&self) -> usize {
        self.ai_inline_review_surface
            .diff_scroll_handle
            .offset()
            .y
            .min(Pixels::ZERO)
            .abs()
            .as_f32()
            .round() as usize
    }

    fn current_ai_inline_review_surface_scroll_offset(&self) -> Point<Pixels> {
        if self.workspace_view_mode == WorkspaceViewMode::Ai && self.ai_inline_review_is_open() {
            return self.ai_inline_review_surface.diff_scroll_handle.offset();
        }

        point(px(0.), px(0.))
    }

    fn current_ai_inline_review_surface_snapshot(
        &mut self,
    ) -> Option<crate::app::ai_inline_review::AiInlineReviewSurfaceSnapshot> {
        self.ai_sync_historical_inline_review_session_if_needed();
        let scroll_top_px = self.current_ai_inline_review_surface_scroll_top_px();
        let viewport_bounds = self.ai_inline_review_surface.diff_scroll_handle.bounds();
        let viewport_height_px = viewport_bounds.size.height.max(Pixels::ZERO).as_f32().round()
            as usize;
        let session_row_count = self.active_review_workspace_session()?.row_count();

        if self.ai_inline_review_surface.geometry.as_ref().is_none_or(|geometry| {
            geometry.row_count() != session_row_count
        }) {
            let session = self.active_review_workspace_session()?;
            self.ai_inline_review_surface.geometry =
                Some(crate::app::ai_inline_review::AiInlineReviewDisplayGeometry::build(
                    session,
                ));
        }

        let visible_row_range = self
            .ai_inline_review_surface
            .geometry
            .as_ref()
            .and_then(|geometry| {
                geometry.render_row_range_for_viewport(
                    scroll_top_px,
                    viewport_height_px.max(1),
                    crate::app::ai_inline_review::AI_INLINE_REVIEW_OVERSCAN_ROWS,
                )
            });
        if let Some(visible_row_range) = visible_row_range
            && let Some(session) = self.active_review_workspace_session_mut()
        {
            crate::app::ai_inline_review::ensure_ai_inline_review_visible_row_caches(
                session,
                visible_row_range,
            );
        }

        let geometry = self.ai_inline_review_surface.geometry.as_ref()?;
        let session = self.active_review_workspace_session()?;
        Some(crate::app::ai_inline_review::build_ai_inline_review_surface_snapshot(
            geometry,
            session,
            scroll_top_px,
            viewport_height_px.max(1),
            crate::app::ai_inline_review::AI_INLINE_REVIEW_OVERSCAN_ROWS,
        ))
    }

    fn render_ai_inline_review_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let snapshot = self.current_ai_inline_review_surface_snapshot();
        let scroll_handle = self.ai_inline_review_surface.diff_scroll_handle.clone();
        let horizontal_scroll_handle = self
            .ai_inline_review_surface
            .diff_horizontal_scroll_handle
            .clone();
        let scrollbar_size = px(AI_INLINE_REVIEW_SCROLLBAR_SIZE);
        let (old_digits, new_digits) = self
            .active_review_workspace_session()
            .map(|session| session.line_number_digit_widths())
            .unwrap_or((DIFF_LINE_NUMBER_MIN_DIGITS, DIFF_LINE_NUMBER_MIN_DIGITS));
        let old_line_number_width = crate::app::data::line_number_column_width(old_digits);
        let new_line_number_width = crate::app::data::line_number_column_width(new_digits);
        let gutter_width =
            ai_inline_review_code_gutter_width(old_line_number_width, new_line_number_width);
        let viewport_width = self
            .ai_inline_review_surface
            .diff_scroll_handle
            .bounds()
            .size
            .width
            .max(Pixels::ZERO);
        let code_viewport_width =
            (viewport_width - gutter_width - px(AI_INLINE_REVIEW_TEXT_LEFT_PADDING_PX))
                .max(Pixels::ZERO);
        let content_width = self.ai_inline_review_horizontal_content_width();
        clamp_scroll_handle_x(
            &self.ai_inline_review_surface.diff_horizontal_scroll_handle,
            (content_width - code_viewport_width).max(Pixels::ZERO),
        );
        let horizontal_offset = self
            .ai_inline_review_surface
            .diff_horizontal_scroll_handle
            .offset()
            .x;

        if let Some(snapshot) = snapshot {
            return div()
                .relative()
                .size_full()
                .child(
                    div()
                        .id("ai-inline-review-scroll")
                        .size_full()
                        .track_scroll(&scroll_handle)
                        .overflow_y_scroll()
                        .on_scroll_wheel(cx.listener(|this, event, _, cx| {
                            if this.on_ai_inline_review_horizontal_scroll_wheel(event, cx) {
                                cx.stop_propagation();
                            }
                        }))
                        .child(
                            div()
                                .relative()
                                .w_full()
                                .h(px(snapshot.viewport.total_surface_height_px as f32))
                                .when_some(
                                    snapshot.viewport.visible_pixel_range.clone(),
                                    |this, range| {
                                        this.child(
                                            div()
                                                .absolute()
                                                .top(px(range.start as f32))
                                                .left_0()
                                                .right_0()
                                                .h(px(range.len() as f32))
                                                .child(
                                                    AiInlineReviewSurfaceElement {
                                                        snapshot: std::rc::Rc::new(
                                                            snapshot.clone(),
                                                        ),
                                                        viewport_origin_px: range.start,
                                                        horizontal_offset,
                                                        mono_font_family: cx
                                                            .theme()
                                                            .mono_font_family
                                                            .clone(),
                                                        ui_font_family: cx
                                                            .theme()
                                                            .font_family
                                                            .clone(),
                                                        old_line_number_width,
                                                        new_line_number_width,
                                                    }
                                                    .into_any_element(),
                                                ),
                                        )
                                    },
                                ),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .right(px(8.0))
                        .bottom_0()
                        .w(scrollbar_size)
                        .child(
                            Scrollbar::vertical(&scroll_handle)
                                .scrollbar_show(ScrollbarShow::Always),
                        ),
                )
                .child(render_diff_horizontal_scrollbar(
                    "ai-inline-review-horizontal-scroll",
                    &horizontal_scroll_handle,
                    gutter_width + px(AI_INLINE_REVIEW_TEXT_LEFT_PADDING_PX),
                    code_viewport_width,
                    content_width,
                ))
                .into_any_element();
        }

        self.render_ai_inline_review_status_surface(cx)
    }

    fn ai_inline_review_horizontal_content_width(&self) -> Pixels {
        let (left_columns, right_columns) = self
            .active_review_workspace_session()
            .map(|session| session.max_code_display_columns(4))
            .unwrap_or_default();
        px(left_columns.max(right_columns) as f32 * DIFF_MONO_CHAR_WIDTH)
    }

    fn on_ai_inline_review_horizontal_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(delta_x) = diff_horizontal_wheel_delta(event) else {
            return false;
        };
        let (old_digits, new_digits) = self
            .active_review_workspace_session()
            .map(|session| session.line_number_digit_widths())
            .unwrap_or((DIFF_LINE_NUMBER_MIN_DIGITS, DIFF_LINE_NUMBER_MIN_DIGITS));
        let old_line_number_width = crate::app::data::line_number_column_width(old_digits);
        let new_line_number_width = crate::app::data::line_number_column_width(new_digits);
        let gutter_width =
            ai_inline_review_code_gutter_width(old_line_number_width, new_line_number_width);
        let viewport_width = self
            .ai_inline_review_surface
            .diff_scroll_handle
            .bounds()
            .size
            .width
            .max(Pixels::ZERO);
        let code_viewport_width =
            (viewport_width - gutter_width - px(AI_INLINE_REVIEW_TEXT_LEFT_PADDING_PX))
                .max(Pixels::ZERO);
        let max_scroll =
            (self.ai_inline_review_horizontal_content_width() - code_viewport_width)
                .max(Pixels::ZERO);
        let handled = scroll_diff_handle_x(
            &self.ai_inline_review_surface.diff_horizontal_scroll_handle,
            delta_x,
            max_scroll,
        );
        if handled {
            self.last_scroll_activity_at = Instant::now();
            cx.notify();
        }
        handled
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_ai_inline_review_row(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    row: &crate::app::ai_inline_review::AiInlineReviewViewportRow,
    horizontal_offset: Pixels,
    old_line_number_width: f32,
    new_line_number_width: f32,
    mono_font_family: SharedString,
    ui_font_family: SharedString,
) {
    match &row.kind {
        crate::app::ai_inline_review::AiInlineReviewViewportRowKind::FileHeader { header } => {
            paint_ai_inline_review_file_header(
                window,
                cx,
                bounds,
                header,
                mono_font_family,
                ui_font_family,
            );
        }
        crate::app::ai_inline_review::AiInlineReviewViewportRowKind::Meta { row_kind, text } => {
            paint_ai_inline_review_meta_row(window, cx, bounds, *row_kind, text, mono_font_family);
        }
        crate::app::ai_inline_review::AiInlineReviewViewportRowKind::Code { lines } => {
            paint_ai_inline_review_code_row(
                window,
                cx,
                bounds,
                row.stable_id,
                lines,
                horizontal_offset,
                old_line_number_width,
                new_line_number_width,
                mono_font_family,
            );
        }
    }
}

fn paint_ai_inline_review_file_header(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    header: &crate::app::review_workspace_session::ReviewWorkspaceVisibleFileHeader,
    mono_font_family: SharedString,
    ui_font_family: SharedString,
) {
    let paint = build_review_workspace_file_header_paint(
        cx.theme(),
        header.path.as_str(),
        header.status,
        header.line_stats,
        false,
        false,
        false,
    );
    let is_dark = cx.theme().mode.is_dark();
    window.paint_quad(fill(bounds, paint.row_background));
    window.paint_quad(fill(
        Bounds {
            origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - px(1.0)),
            size: size(bounds.size.width, px(1.0)),
        },
        paint.row_divider,
    ));
    window.paint_quad(fill(
        Bounds {
            origin: bounds.origin,
            size: size(px(2.0), bounds.size.height),
        },
        paint.accent_strip,
    ));

    let badge_text = paint.badge_label.clone();
    let badge_style = gpui::TextStyle {
        color: paint.badge_text_color,
        font_family: ui_font_family,
        font_size: px(11.0).into(),
        line_height: gpui::relative(1.25),
        ..Default::default()
    };
    let badge_font = badge_style.font();
    let badge_font_size = badge_style.font_size.to_pixels(window.rem_size());
    let badge_runs = vec![single_color_text_run(
        badge_text.len(),
        paint.badge_text_color,
        badge_font.clone(),
    )];
    let badge_shape = shape_editor_line(window, badge_text.clone(), badge_font_size, &badge_runs);
    let badge_bounds = Bounds {
        origin: point(bounds.origin.x + px(12.0), bounds.origin.y + px(8.0)),
        size: size((badge_shape.width() + px(20.0)).max(px(40.0)), px(18.0)),
    };
    let path_origin_x = badge_bounds.origin.x + badge_bounds.size.width + px(12.0);
    window.paint_quad(fill(badge_bounds, paint.badge_background));
    window.paint_quad(fill(
        Bounds {
            origin: badge_bounds.origin,
            size: size(badge_bounds.size.width, px(1.0)),
        },
        paint.badge_border,
    ));

    paint_editor_line(
        window,
        cx,
        &badge_shape,
        point(
            badge_bounds.origin.x + ((badge_bounds.size.width - badge_shape.width()) / 2.).max(Pixels::ZERO),
            badge_bounds.origin.y + px(2.0),
        ),
        badge_style.line_height_in_pixels(window.rem_size()),
    );

    let path_style = gpui::TextStyle {
        color: paint.path_text_color,
        font_family: mono_font_family.clone(),
        font_size: px(12.0).into(),
        line_height: gpui::relative(1.35),
        ..Default::default()
    };
    let path_font = path_style.font();
    let path_font_size = path_style.font_size.to_pixels(window.rem_size());
    let path_runs = vec![single_color_text_run(
        paint.path.len(),
        paint.path_text_color,
        path_font.clone(),
    )];
    let path_shape = shape_editor_line(window, paint.path.clone(), path_font_size, &path_runs);
    paint_editor_line(
        window,
        cx,
        &path_shape,
        point(path_origin_x, bounds.origin.y + px(7.0)),
        path_style.line_height_in_pixels(window.rem_size()),
    );

    let stats_label = format!(
        "{}  {}  {}",
        paint.stats_added, paint.stats_removed, paint.stats_changed,
    );
    let stats_style = gpui::TextStyle {
        color: hunk_opacity(cx.theme().muted_foreground, is_dark, 0.92, 0.88),
        font_family: mono_font_family,
        font_size: px(11.0).into(),
        line_height: gpui::relative(1.25),
        ..Default::default()
    };
    let stats_font = stats_style.font();
    let stats_font_size = stats_style.font_size.to_pixels(window.rem_size());
    let stats_runs = vec![single_color_text_run(
        stats_label.len(),
        stats_style.color,
        stats_font,
    )];
    let stats_shape = shape_editor_line(window, stats_label.into(), stats_font_size, &stats_runs);
    paint_editor_line(
        window,
        cx,
        &stats_shape,
        point(path_origin_x, bounds.origin.y + px(20.0)),
        stats_style.line_height_in_pixels(window.rem_size()),
    );
}

fn paint_ai_inline_review_meta_row(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    row_kind: DiffRowKind,
    text: &str,
    mono_font_family: SharedString,
) {
    let meta = build_review_workspace_meta_row_paint(cx.theme(), row_kind, text, false);
    window.paint_quad(fill(bounds, meta.background));
    window.paint_quad(fill(
        Bounds {
            origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - px(1.0)),
            size: size(bounds.size.width, px(1.0)),
        },
        meta.border,
    ));
    if row_kind == DiffRowKind::HunkHeader {
        return;
    }

    window.paint_quad(fill(
        Bounds {
            origin: bounds.origin,
            size: size(px(2.0), bounds.size.height),
        },
        meta.accent,
    ));

    let text_style = gpui::TextStyle {
        color: meta.foreground,
        font_family: mono_font_family,
        font_size: px(12.0).into(),
        line_height: gpui::relative(1.35),
        ..Default::default()
    };
    let font = text_style.font();
    let font_size = text_style.font_size.to_pixels(window.rem_size());
    let runs = vec![single_color_text_run(text.len(), meta.foreground, font)];
    let shape = shape_editor_line(window, text.to_string().into(), font_size, &runs);
    paint_editor_line(
        window,
        cx,
        &shape,
        point(bounds.origin.x + px(12.0), bounds.origin.y + px(4.0)),
        text_style.line_height_in_pixels(window.rem_size()),
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_ai_inline_review_code_row(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    stable_id: u64,
    lines: &[crate::app::ai_inline_review::AiInlineReviewCodeLine],
    horizontal_offset: Pixels,
    old_line_number_width: f32,
    new_line_number_width: f32,
    mono_font_family: SharedString,
) {
    for (line_ix, line) in lines.iter().enumerate() {
        let line_bounds = Bounds {
            origin: point(
                bounds.origin.x,
                bounds.origin.y
                    + px(
                        (line_ix * crate::app::ai_inline_review::AI_INLINE_REVIEW_CODE_LINE_HEIGHT_PX)
                            as f32,
                    ),
            ),
            size: size(
                bounds.size.width,
                px(crate::app::ai_inline_review::AI_INLINE_REVIEW_CODE_LINE_HEIGHT_PX as f32),
            ),
        };
        paint_ai_inline_review_code_line(
            window,
            cx,
            line_bounds,
            stable_id.saturating_add(line_ix as u64),
            line,
            horizontal_offset,
            old_line_number_width,
            new_line_number_width,
            mono_font_family.clone(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_ai_inline_review_code_line(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    stable_id: u64,
    line: &crate::app::ai_inline_review::AiInlineReviewCodeLine,
    horizontal_offset: Pixels,
    old_line_number_width: f32,
    new_line_number_width: f32,
    mono_font_family: SharedString,
) {
    let is_dark = cx.theme().mode.is_dark();
    let chrome = hunk_diff_chrome(cx.theme(), is_dark);
    let (background, line_number_color, marker_color, marker) = match line.kind {
        DiffCellKind::Added => (
            hunk_blend(cx.theme().background, cx.theme().success, is_dark, 0.20, 0.10),
            hunk_tone(cx.theme().success, is_dark, 0.42, 0.10),
            hunk_tone(cx.theme().success, is_dark, 0.56, 0.18),
            "+",
        ),
        DiffCellKind::Removed => (
            hunk_blend(cx.theme().background, cx.theme().danger, is_dark, 0.20, 0.10),
            hunk_tone(cx.theme().danger, is_dark, 0.42, 0.10),
            hunk_tone(cx.theme().danger, is_dark, 0.56, 0.18),
            "-",
        ),
        _ => (
            if stable_id.is_multiple_of(2) {
                hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.05, 0.10)
            } else {
                cx.theme().background
            },
            hunk_tone(cx.theme().muted_foreground, is_dark, 0.16, 0.10),
            hunk_tone(cx.theme().muted_foreground, is_dark, 0.18, 0.12),
            " ",
        ),
    };
    let gutter_width =
        ai_inline_review_code_gutter_width(old_line_number_width, new_line_number_width);

    window.paint_quad(fill(bounds, background));
    let gutter_bounds = Bounds {
        origin: bounds.origin,
        size: size(gutter_width.min(bounds.size.width), bounds.size.height),
    };
    let gutter_background = match line.kind {
        DiffCellKind::Added => {
            hunk_blend(chrome.gutter_background, cx.theme().success, is_dark, 0.10, 0.06)
        }
        DiffCellKind::Removed => {
            hunk_blend(chrome.gutter_background, cx.theme().danger, is_dark, 0.10, 0.06)
        }
        _ => chrome.gutter_background,
    };
    window.paint_quad(fill(gutter_bounds, gutter_background));
    window.paint_quad(fill(
        Bounds {
            origin: point(gutter_bounds.origin.x + gutter_bounds.size.width - px(1.0), gutter_bounds.origin.y),
            size: size(px(1.0), gutter_bounds.size.height),
        },
        chrome.gutter_divider,
    ));

    let text_style = gpui::TextStyle {
        color: cx.theme().foreground,
        font_family: mono_font_family.clone(),
        font_size: px(12.0).into(),
        line_height: gpui::relative(1.45),
        ..Default::default()
    };
    let font = text_style.font();
    let font_size = text_style.font_size.to_pixels(window.rem_size());
    let line_height = text_style.line_height_in_pixels(window.rem_size());
    let text_y = bounds.origin.y + ((bounds.size.height - line_height) / 2.).max(Pixels::ZERO);

    paint_ai_inline_review_line_number(
        window,
        cx,
        point(bounds.origin.x + px(8.0), text_y),
        line.old_line,
        old_line_number_width,
        line_number_color,
        font.clone(),
        font_size,
        line_height,
    );
    paint_ai_inline_review_line_number(
        window,
        cx,
        point(
            bounds.origin.x + px(8.0 + old_line_number_width + AI_INLINE_REVIEW_LINE_NUMBER_GAP_PX),
            text_y,
        ),
        line.new_line,
        new_line_number_width,
        line_number_color,
        font.clone(),
        font_size,
        line_height,
    );

    let marker_runs = vec![single_color_text_run(marker.len(), marker_color, font.clone())];
    let marker_shape = shape_editor_line(window, marker.to_string().into(), font_size, &marker_runs);
    paint_editor_line(
        window,
        cx,
        &marker_shape,
        point(
            bounds.origin.x
                + px(
                    16.0 + old_line_number_width + new_line_number_width
                        + AI_INLINE_REVIEW_LINE_NUMBER_GAP_PX * 2.0,
                ),
            text_y,
        ),
        line_height,
    );

    let changed_bg = hunk_opacity(marker_color, is_dark, 0.20, 0.11);
    let text_runs = ai_inline_review_text_runs(
        cx,
        line.segments.as_slice(),
        cx.theme().foreground,
        font,
        changed_bg,
    );
    let text = if line.text.is_empty() { " ".to_string() } else { line.text.clone() };
    let text_shape = shape_editor_line(window, text.clone().into(), font_size, &text_runs);
    let text_origin_x = bounds.origin.x + gutter_width + px(AI_INLINE_REVIEW_TEXT_LEFT_PADDING_PX);
    let text_mask_bounds = Bounds {
        origin: point(text_origin_x, bounds.origin.y),
        size: size(
            (bounds.right() - text_origin_x).max(Pixels::ZERO),
            bounds.size.height,
        ),
    };
    window.with_content_mask(Some(ContentMask { bounds: text_mask_bounds }), |window| {
        paint_editor_line(
            window,
            cx,
            &text_shape,
            point(text_origin_x + horizontal_offset, text_y),
            line_height,
        );
    });
}

fn ai_inline_review_code_gutter_width(
    old_line_number_width: f32,
    new_line_number_width: f32,
) -> Pixels {
    px(old_line_number_width)
        + px(new_line_number_width)
        + px(AI_INLINE_REVIEW_LINE_NUMBER_GAP_PX * 2.0 + AI_INLINE_REVIEW_MARKER_WIDTH_PX)
        + px(24.0)
}

#[allow(clippy::too_many_arguments)]
fn paint_ai_inline_review_line_number(
    window: &mut Window,
    cx: &mut App,
    origin: Point<Pixels>,
    line_number: Option<u32>,
    width: f32,
    color: Hsla,
    font: gpui::Font,
    font_size: Pixels,
    line_height: Pixels,
) {
    let label = line_number.map(|line| line.to_string()).unwrap_or_default();
    let runs = vec![single_color_text_run(label.len(), color, font.clone())];
    let shape = shape_editor_line(window, label.clone().into(), font_size, &runs);
    paint_editor_line(
        window,
        cx,
        &shape,
        point(origin.x + (px(width) - shape.width()).max(Pixels::ZERO), origin.y),
        line_height,
    );
}

fn ai_inline_review_text_runs(
    cx: &App,
    segments: &[crate::app::data::CachedStyledSegment],
    default_foreground: Hsla,
    font: gpui::Font,
    changed_bg: Hsla,
) -> Vec<gpui::TextRun> {
    if segments.is_empty() {
        return vec![gpui::TextRun {
            len: 1,
            color: default_foreground,
            font,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    segments
        .iter()
        .map(|segment| gpui::TextRun {
            len: segment.plain_text.len(),
            color: diff_syntax_color(cx.theme(), default_foreground, segment.syntax),
            font: font.clone(),
            background_color: segment.changed.then_some(changed_bg),
            underline: None,
            strikethrough: None,
        })
        .collect()
}
