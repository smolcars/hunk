use gpui::{
    AnyElement, App, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Point, SharedString, TextAlign, TextRun, Window,
    point, px,
};

#[derive(Clone)]
struct ReviewWorkspaceCodeRowCellPaint {
    panel_width: Option<gpui::Pixels>,
    line_number_width: f32,
    background: gpui::Hsla,
    gutter_background: gpui::Hsla,
    gutter_divider: gpui::Hsla,
    text_color: gpui::Hsla,
    line_color: gpui::Hsla,
    marker_color: gpui::Hsla,
    marker: SharedString,
    line_number: SharedString,
    segments: Vec<crate::app::data::CachedStyledSegment>,
}

#[derive(Clone)]
struct ReviewWorkspaceCodeRowElement {
    left: ReviewWorkspaceCodeRowCellPaint,
    right: ReviewWorkspaceCodeRowCellPaint,
    center_divider: gpui::Hsla,
    mono_font_family: SharedString,
}

impl ReviewWorkspaceCodeRowElement {
    fn new(
        left: ReviewWorkspaceCodeRowCellPaint,
        right: ReviewWorkspaceCodeRowCellPaint,
        center_divider: gpui::Hsla,
        mono_font_family: SharedString,
    ) -> Self {
        Self {
            left,
            right,
            center_divider,
            mono_font_family,
        }
    }
}

impl IntoElement for ReviewWorkspaceCodeRowElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ReviewWorkspaceCodeRowElement {
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
        let left_width = self.left.panel_width.unwrap_or(bounds.size.width / 2.);
        let right_width = self
            .right
            .panel_width
            .unwrap_or((bounds.size.width - left_width).max(Pixels::ZERO));
        let left_bounds = Bounds {
            origin: bounds.origin,
            size: gpui::size(left_width, bounds.size.height),
        };
        let right_bounds = Bounds {
            origin: point(bounds.origin.x + left_width, bounds.origin.y),
            size: gpui::size(right_width, bounds.size.height),
        };

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.paint_cell(window, cx, left_bounds, &self.left, true);
            self.paint_cell(window, cx, right_bounds, &self.right, false);
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: point(right_bounds.origin.x - px(1.0), bounds.origin.y),
                    size: gpui::size(px(1.0), bounds.size.height),
                },
                self.center_divider,
            ));
        });
    }
}

impl ReviewWorkspaceCodeRowElement {
    fn paint_cell(
        &self,
        window: &mut Window,
        cx: &mut App,
        bounds: Bounds<Pixels>,
        cell: &ReviewWorkspaceCodeRowCellPaint,
        draw_right_divider: bool,
    ) {
        let padding_x = px(8.0);
        let gutter_padding_x = px(8.0);
        let marker_width = px(DIFF_MARKER_GUTTER_WIDTH);
        let gutter_width = px(cell.line_number_width) + marker_width + px(16.0);

        window.paint_quad(gpui::fill(bounds, cell.background));
        let gutter_bounds = Bounds {
            origin: bounds.origin,
            size: gpui::size(gutter_width.min(bounds.size.width), bounds.size.height),
        };
        window.paint_quad(gpui::fill(gutter_bounds, cell.gutter_background));

        let gutter_divider_x = gutter_bounds.origin.x + gutter_bounds.size.width - px(1.0);
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(gutter_divider_x, gutter_bounds.origin.y),
                size: gpui::size(px(1.0), gutter_bounds.size.height),
            },
            cell.gutter_divider,
        ));

        if draw_right_divider {
            let divider_x = bounds.origin.x + bounds.size.width - px(1.0);
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: point(divider_x, bounds.origin.y),
                    size: gpui::size(px(1.0), bounds.size.height),
                },
                self.center_divider,
            ));
        }

        let text_style = gpui::TextStyle {
            color: cell.text_color,
            font_family: self.mono_font_family.clone(),
            font_size: px(12.0).into(),
            line_height: gpui::relative(1.45),
            ..Default::default()
        };
        let font = text_style.font();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = text_style.line_height_in_pixels(window.rem_size());
        let text_origin_y = bounds.origin.y + ((bounds.size.height - line_height) / 2.).max(Pixels::ZERO);

        let line_number_runs = vec![TextRun {
            len: cell.line_number.len(),
            color: cell.line_color,
            font: font.clone(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let line_number_shape = window.text_system().shape_line(
            cell.line_number.clone(),
            font_size,
            &line_number_runs,
            None,
        );
        let line_number_x = gutter_bounds.origin.x
            + gutter_padding_x
            + (px(cell.line_number_width) - line_number_shape.width()).max(Pixels::ZERO);
        let _ = line_number_shape.paint(
            point(line_number_x, text_origin_y),
            line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        );

        let marker_runs = vec![TextRun {
            len: cell.marker.len(),
            color: cell.marker_color,
            font: font.clone(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let marker_shape =
            window
                .text_system()
                .shape_line(cell.marker.clone(), font_size, &marker_runs, None);
        let marker_origin_x =
            gutter_bounds.origin.x + gutter_padding_x + px(cell.line_number_width) + px(8.0);
        let marker_x = marker_origin_x + ((marker_width - marker_shape.width()) / 2.).max(Pixels::ZERO);
        let _ = marker_shape.paint(
            point(marker_x, text_origin_y),
            line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        );

        let mut text = String::new();
        let mut text_runs = Vec::new();
        let changed_bg = hunk_opacity(cell.marker_color, cx.theme().mode.is_dark(), 0.20, 0.11);
        for segment in &cell.segments {
            let segment_text = segment.plain_text.as_ref();
            if segment_text.is_empty() {
                continue;
            }
            text.push_str(segment_text);
            text_runs.push(TextRun {
                len: segment_text.len(),
                color: diff_syntax_color(cx.theme(), cell.text_color, segment.syntax),
                font: font.clone(),
                background_color: segment.changed.then_some(changed_bg),
                underline: None,
                strikethrough: None,
            });
        }
        if text_runs.is_empty() {
            text.push(' ');
            text_runs.push(TextRun {
                len: 1,
                color: cell.text_color,
                font: font.clone(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }

        let text_shape = window
            .text_system()
            .shape_line(text.into(), font_size, &text_runs, None);
        let text_origin_x = gutter_bounds.origin.x + gutter_bounds.size.width + padding_x;
        let _ = text_shape.paint(
            point(text_origin_x, text_origin_y),
            line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

impl DiffViewer {
    fn render_review_workspace_code_row_element(
        &self,
        row_stable_id: u64,
        row_data: &SideBySideRow,
        row_is_selected: bool,
        viewport_row: &review_workspace_session::ReviewWorkspaceViewportRow,
        layout: Option<DiffColumnLayout>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let left = self.build_review_workspace_code_row_cell(
            row_stable_id,
            row_data,
            row_is_selected,
            DiffCellRenderSpec {
                row_ix: viewport_row.row_index,
                side: "left",
                cell: &row_data.left,
                peer_kind: row_data.right.kind,
                panel_width: layout.map(|layout| layout.left_panel_width),
            },
            viewport_row,
            cx,
        );
        let right = self.build_review_workspace_code_row_cell(
            row_stable_id,
            row_data,
            row_is_selected,
            DiffCellRenderSpec {
                row_ix: viewport_row.row_index,
                side: "right",
                cell: &row_data.right,
                peer_kind: row_data.left.kind,
                panel_width: layout.map(|layout| layout.right_panel_width),
            },
            viewport_row,
            cx,
        );
        let chrome = hunk_diff_chrome(cx.theme(), cx.theme().mode.is_dark());

        ReviewWorkspaceCodeRowElement::new(
            left,
            right,
            chrome.center_divider,
            cx.theme().mono_font_family.clone(),
        )
        .into_any_element()
    }

    fn build_review_workspace_code_row_cell(
        &self,
        row_stable_id: u64,
        row_data: &SideBySideRow,
        row_is_selected: bool,
        spec: DiffCellRenderSpec<'_>,
        viewport_row: &review_workspace_session::ReviewWorkspaceViewportRow,
        cx: &mut Context<Self>,
    ) -> ReviewWorkspaceCodeRowCellPaint {
        let _ = row_data;
        let side = spec.side;
        let cell = spec.cell;
        let peer_kind = spec.peer_kind;
        let is_dark = cx.theme().mode.is_dark();
        let chrome = hunk_diff_chrome(cx.theme(), is_dark);
        let dark_add_tint: gpui::Hsla = gpui::rgb(0x2e4736).into();
        let dark_remove_tint: gpui::Hsla = gpui::rgb(0x4a3038).into();
        let dark_add_accent: gpui::Hsla = gpui::rgb(0x8fcea0).into();
        let dark_remove_accent: gpui::Hsla = gpui::rgb(0xeea9b4).into();

        let (mut background, marker_color, line_color, text_color, marker) =
            match (cell.kind, peer_kind) {
                (DiffCellKind::Added, _) => (
                    hunk_pick(
                        is_dark,
                        cx.theme().background.blend(dark_add_tint.opacity(0.62)),
                        hunk_blend(cx.theme().background, cx.theme().success, is_dark, 0.24, 0.11),
                    ),
                    hunk_pick(is_dark, dark_add_accent, cx.theme().success.darken(0.18)),
                    hunk_pick(
                        is_dark,
                        dark_add_accent.lighten(0.08),
                        cx.theme().success.darken(0.16),
                    ),
                    cx.theme().foreground,
                    "+",
                ),
                (DiffCellKind::Removed, _) => (
                    hunk_pick(
                        is_dark,
                        cx.theme().background.blend(dark_remove_tint.opacity(0.62)),
                        hunk_blend(cx.theme().background, cx.theme().danger, is_dark, 0.24, 0.11),
                    ),
                    hunk_pick(is_dark, dark_remove_accent, cx.theme().danger.darken(0.18)),
                    hunk_pick(
                        is_dark,
                        dark_remove_accent.lighten(0.06),
                        cx.theme().danger.darken(0.16),
                    ),
                    cx.theme().foreground,
                    "-",
                ),
                (DiffCellKind::Context, _) => (
                    cx.theme().background,
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.14, 0.10),
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.18, 0.12),
                    cx.theme().foreground,
                    "",
                ),
                (DiffCellKind::None, _) => (
                    cx.theme().background,
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.14, 0.10),
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.18, 0.12),
                    hunk_tone(cx.theme().muted_foreground, is_dark, 0.08, 0.06),
                    "",
                ),
            };
        if matches!(cell.kind, DiffCellKind::Context | DiffCellKind::None)
            && row_stable_id.is_multiple_of(2)
        {
            background = hunk_blend(background, cx.theme().muted, is_dark, 0.06, 0.10);
        }
        if row_is_selected {
            background = hunk_blend(background, cx.theme().primary, is_dark, 0.22, 0.13);
        }

        let cached_row_segments = self.active_diff_row_segment_cache(spec.row_ix);
        let segment_cache = if side == "left" {
            cached_row_segments.map(|segments| &segments.left)
        } else {
            cached_row_segments.map(|segments| &segments.right)
        };
        let display_row = if side == "left" {
            &viewport_row.left_display_row
        } else {
            &viewport_row.right_display_row
        };
        let fallback_segments;
        let segments = if let Some(cached) = segment_cache {
            cached.clone()
        } else {
            fallback_segments = cached_runtime_fallback_segments(display_row.text.as_str());
            fallback_segments
        };

        let mut gutter_background = match cell.kind {
            DiffCellKind::Added => {
                hunk_blend(chrome.gutter_background, cx.theme().success, is_dark, 0.12, 0.07)
            }
            DiffCellKind::Removed => {
                hunk_blend(chrome.gutter_background, cx.theme().danger, is_dark, 0.12, 0.07)
            }
            DiffCellKind::None => chrome.empty_gutter_background,
            DiffCellKind::Context => chrome.gutter_background,
        };
        if row_is_selected {
            gutter_background =
                hunk_blend(gutter_background, cx.theme().primary, is_dark, 0.14, 0.10);
        }

        ReviewWorkspaceCodeRowCellPaint {
            panel_width: spec.panel_width,
            line_number_width: if side == "left" {
                self.review_surface.diff_left_line_number_width
            } else {
                self.review_surface.diff_right_line_number_width
            },
            background,
            gutter_background,
            gutter_divider: chrome.gutter_divider,
            text_color,
            line_color,
            marker_color,
            marker: SharedString::from(marker),
            line_number: SharedString::from(cell.line.map(|line| line.to_string()).unwrap_or_default()),
            segments,
        }
    }
}
