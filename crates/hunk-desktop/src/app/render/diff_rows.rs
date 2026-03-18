struct DiffCellRenderSpec<'a> {
    row_ix: usize,
    side: &'static str,
    cell: &'a DiffCell,
    peer_kind: DiffCellKind,
    panel_width: Option<Pixels>,
}

impl DiffViewer {
    fn render_meta_row(
        &self,
        ix: usize,
        row: &SideBySideRow,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(meta) = self.diff_row_metadata.get(ix)
            && meta.kind == DiffStreamRowKind::FileHeader
            && let (Some(path), Some(status)) = (meta.file_path.as_deref(), meta.file_status)
        {
            let stats = self
                .active_diff_file_line_stats()
                .get(path)
                .copied()
                .unwrap_or_default();
            return self.render_file_status_banner_row(
                ix,
                path,
                status,
                stats,
                is_selected,
                cx,
            );
        }

        let stable_row_id = self.diff_row_stable_id(ix);
        if row.kind == DiffRowKind::HunkHeader {
            let is_dark = cx.theme().mode.is_dark();
            let divider_bg = if is_selected {
                hunk_opacity(cx.theme().primary, is_dark, 0.34, 0.18)
            } else {
                hunk_opacity(cx.theme().muted, is_dark, 0.26, 0.40)
            };
            return div()
                .id(("diff-hunk-divider-row", stable_row_id))
                .h(px(6.0))
                .border_b_1()
                .border_color(hunk_opacity(cx.theme().border, is_dark, 0.92, 0.70))
                .bg(divider_bg)
                .w_full()
                .into_any_element();
        }

        let is_dark = cx.theme().mode.is_dark();

        let (background, foreground, accent) = match row.kind {
            DiffRowKind::HunkHeader => (
                cx.theme().primary_hover,
                cx.theme().primary_foreground,
                cx.theme().primary,
            ),
            DiffRowKind::Meta => {
                let line = row.text.as_str();
                if line.starts_with("new file mode") || line.starts_with("+++ b/") {
                    (
                        hunk_blend(cx.theme().background, cx.theme().success, is_dark, 0.22, 0.12),
                        hunk_tone(cx.theme().success, is_dark, 0.45, 0.10),
                        cx.theme().success,
                    )
                } else if line.starts_with("deleted file mode") || line.starts_with("--- a/") {
                    (
                        hunk_blend(cx.theme().background, cx.theme().danger, is_dark, 0.22, 0.12),
                        hunk_tone(cx.theme().danger, is_dark, 0.45, 0.10),
                        cx.theme().danger,
                    )
                } else if line.starts_with("diff --git") {
                    (
                        hunk_blend(cx.theme().background, cx.theme().accent, is_dark, 0.18, 0.10),
                        cx.theme().foreground,
                        cx.theme().accent,
                    )
                } else {
                    (
                        cx.theme().muted,
                        cx.theme().muted_foreground,
                        cx.theme().border,
                    )
                }
            }
            DiffRowKind::Empty => (
                cx.theme().background,
                cx.theme().muted_foreground,
                cx.theme().border,
            ),
            DiffRowKind::Code => (
                cx.theme().background,
                cx.theme().foreground,
                cx.theme().border,
            ),
        };

        let meta_row = div()
            .id(("diff-meta-row", stable_row_id))
            .relative()
            .overflow_x_hidden()
            .on_mouse_down(MouseButton::Left, {
                let row_ix = ix;
                cx.listener(move |this, event, window, cx| {
                    this.on_diff_row_mouse_down(row_ix, event, window, cx);
                })
            })
            .on_mouse_down(MouseButton::Middle, {
                let row_ix = ix;
                cx.listener(move |this, event, window, cx| {
                    this.on_diff_row_mouse_down(row_ix, event, window, cx);
                })
            })
            .on_mouse_move({
                let row_ix = ix;
                cx.listener(move |this, event, window, cx| {
                    this.on_diff_row_mouse_move(row_ix, event, window, cx);
                })
            })
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_diff_row_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_diff_row_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_diff_row_mouse_up))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::on_diff_row_mouse_up))
            .px_3()
            .py_0p5()
            .border_b_1()
            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.82, 0.70))
            .bg(if is_selected {
                hunk_blend(background, cx.theme().primary, is_dark, 0.24, 0.14)
            } else {
                background
            })
            .text_xs()
            .text_color(foreground)
            .font_family(cx.theme().mono_font_family.clone())
            .w_full()
            .whitespace_normal()
            .child(row.text.clone())
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .bg(accent),
            )
            .child(self.render_row_comment_affordance(ix, cx));

        self.render_diff_row_with_comment_editor(ix, meta_row.into_any_element(), cx)
    }

    fn render_code_row(
        &self,
        ix: usize,
        row_data: &SideBySideRow,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stable_row_id = self.diff_row_stable_id(ix);
        let chrome = hunk_diff_chrome(cx.theme(), cx.theme().mode.is_dark());
        let layout = self.diff_column_layout();
        let code_row = h_flex()
            .id(("diff-code-row", stable_row_id))
            .relative()
            .overflow_x_hidden()
            .items_stretch()
            .on_mouse_down(MouseButton::Left, {
                let row_ix = ix;
                cx.listener(move |this, event, window, cx| {
                    this.on_diff_row_mouse_down(row_ix, event, window, cx);
                })
            })
            .on_mouse_down(MouseButton::Middle, {
                let row_ix = ix;
                cx.listener(move |this, event, window, cx| {
                    this.on_diff_row_mouse_down(row_ix, event, window, cx);
                })
            })
            .on_mouse_move({
                let row_ix = ix;
                cx.listener(move |this, event, window, cx| {
                    this.on_diff_row_mouse_move(row_ix, event, window, cx);
                })
            })
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_diff_row_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_diff_row_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_diff_row_mouse_up))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::on_diff_row_mouse_up))
            .border_b_1()
            .border_color(chrome.row_divider)
            .w_full()
            .child(self.render_diff_cell(
                stable_row_id,
                is_selected,
                DiffCellRenderSpec {
                    row_ix: ix,
                    side: "left",
                    cell: &row_data.left,
                    peer_kind: row_data.right.kind,
                    panel_width: layout.map(|layout| layout.left_panel_width),
                },
                cx,
            ))
            .child(self.render_diff_cell(
                stable_row_id,
                is_selected,
                DiffCellRenderSpec {
                    row_ix: ix,
                    side: "right",
                    cell: &row_data.right,
                    peer_kind: row_data.left.kind,
                    panel_width: layout.map(|layout| layout.right_panel_width),
                },
                cx,
            ))
            .child(self.render_row_comment_affordance(ix, cx));

        self.render_diff_row_with_comment_editor(ix, code_row.into_any_element(), cx)
    }

    fn render_diff_row_with_comment_editor(
        &self,
        row_ix: usize,
        row: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .w_full()
            .child(row)
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .px_3()
                    .pt_1()
                    .child(self.render_row_comment_editor(row_ix, cx)),
            )
            .into_any_element()
    }

    fn render_diff_cell(
        &self,
        row_stable_id: u64,
        row_is_selected: bool,
        spec: DiffCellRenderSpec<'_>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let side = spec.side;
        let cell = spec.cell;
        let peer_kind = spec.peer_kind;
        let cell_id = if side == "left" {
            ("diff-cell-left", row_stable_id)
        } else {
            ("diff-cell-right", row_stable_id)
        };

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

        let line_number = cell.line.map(|line| line.to_string()).unwrap_or_default();
        let cached_row_segments = self
            .diff_row_segment_cache
            .get(spec.row_ix)
            .and_then(Option::as_ref);
        let segment_cache = if side == "left" {
            cached_row_segments.map(|segments| &segments.left)
        } else {
            cached_row_segments.map(|segments| &segments.right)
        };
        let fallback_segments;
        let styled_segments = if let Some(cached) = segment_cache {
            cached
        } else {
            fallback_segments =
                cached_runtime_fallback_segments(&cell.text, self.diff_show_whitespace);
            &fallback_segments
        };
        let line_number_width = if side == "left" {
            self.diff_left_line_number_width
        } else {
            self.diff_right_line_number_width
        };

        let should_draw_right_divider = side == "left";
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
        let gutter_width = line_number_width + DIFF_MARKER_GUTTER_WIDTH + 16.0;

        h_flex()
            .id(cell_id)
            .overflow_x_hidden()
            .items_stretch()
            .bg(background)
            .when_some(spec.panel_width, |this, width| {
                this.w(width).min_w(width).max_w(width).flex_none()
            })
            .when(spec.panel_width.is_none(), |this| this.flex_1().min_w_0())
            .when(should_draw_right_divider, |this| {
                this.border_r_1()
                    .border_color(chrome.center_divider)
            })
            .child(
                h_flex()
                    .items_start()
                    .gap_1()
                    .w(px(gutter_width))
                    .min_w(px(gutter_width))
                    .px_2()
                    .py_0p5()
                    .bg(gutter_background)
                    .border_r_1()
                    .border_color(chrome.gutter_divider)
                    .child(
                        h_flex()
                            .w(px(line_number_width))
                            .justify_end()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(line_color)
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .whitespace_nowrap()
                                    .child(line_number),
                            ),
                    )
                    .child(
                        h_flex()
                            .w(px(DIFF_MARKER_GUTTER_WIDTH))
                            .justify_center()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(marker_color)
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .whitespace_nowrap()
                                    .child(marker),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_start()
                    .gap_0()
                    .px_2()
                    .py_0p5()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(text_color)
                    .flex_wrap()
                    .whitespace_normal()
                    .children(styled_segments.iter().map(|segment| {
                        let segment_text = if self.diff_show_whitespace {
                            segment.whitespace_text.clone()
                        } else {
                            segment.plain_text.clone()
                        };
                        let segment_color =
                            diff_syntax_color(text_color, segment.syntax, cx.theme().mode.is_dark());
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .text_color(segment_color)
                            .when(segment.changed, |this| {
                                this.bg(hunk_opacity(marker_color, is_dark, 0.20, 0.11))
                            })
                            .child(segment_text)
                    }))
                    .when(
                        self.diff_show_eol_markers && cell.kind != DiffCellKind::None,
                        |this| {
                            this.child(
                                div()
                                    .flex_none()
                                    .whitespace_nowrap()
                                    .text_color(
                                        hunk_opacity(
                                            cx.theme().muted_foreground,
                                            is_dark,
                                            0.90,
                                            0.95,
                                        ),
                                    )
                                    .child("↵"),
                            )
                        },
                    ),
            )
            .into_any_element()
    }

    fn diff_row_stable_id(&self, row_ix: usize) -> u64 {
        self.diff_row_metadata
            .get(row_ix)
            .map(|row| row.stable_id)
            .unwrap_or(row_ix as u64)
    }
}
