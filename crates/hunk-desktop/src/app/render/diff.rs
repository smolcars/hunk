struct DiffCellRenderSpec<'a> {
    row_ix: usize,
    side: &'static str,
    cell: &'a DiffCell,
    peer_kind: DiffCellKind,
}

impl DiffViewer {
    fn render_diff(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.repo_discovery_failed {
            return self.render_open_project_empty_state(cx);
        }

        if let Some(error_message) = &self.error_message {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error_message.clone()),
                )
                .into_any_element();
        }
        if self.repo_root.is_some() && self.files.is_empty() {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No files changed"),
                )
                .into_any_element();
        }

        let (old_label, new_label) = self.diff_column_labels();
        let diff_list_state = self.diff_list_state.clone();
        let logical_top = diff_list_state.logical_scroll_top();
        let visible_row = logical_top.item_ix;
        if visible_row < self.diff_rows.len() {
            self.sync_selected_file_from_visible_row(visible_row, cx);
        }
        let sticky_hunk_banner = self.render_visible_hunk_banner(visible_row, cx);
        let sticky_file_banner =
            self.render_visible_file_banner(visible_row, logical_top.offset_in_item, cx);

        let list = list(diff_list_state.clone(), {
            cx.processor(move |this, ix: usize, _window, cx| {
                let Some(row) = this.diff_rows.get(ix) else {
                    return div().into_any_element();
                };
                let is_selected = this.is_row_selected(ix);

                match row.kind {
                    DiffRowKind::Code => this.render_code_row(ix, row, is_selected, cx),
                    DiffRowKind::HunkHeader | DiffRowKind::Meta | DiffRowKind::Empty => {
                        this.render_meta_row(ix, row, is_selected, cx)
                    }
                }
            })
        })
        .flex_grow()
        .size_full()
        .map(|mut this| {
            this.style().restrict_scroll_to_axis = Some(true);
            this
        })
        .with_sizing_behavior(ListSizingBehavior::Auto);

        let scrollbar_size = px(DIFF_SCROLLBAR_SIZE);
        let edge_inset = px(DIFF_BOTTOM_SAFE_INSET);
        let right_inset = px(DIFF_SCROLLBAR_RIGHT_INSET);
        let vertical_bar_bottom = edge_inset;
        let is_dark = cx.theme().mode.is_dark();

        v_flex()
            .size_full()
            .child(sticky_hunk_banner)
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        h_flex()
                            .w_full()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.78 }))
                            .bg(cx.theme().title_bar.blend(
                                cx.theme()
                                    .muted
                                    .opacity(if is_dark { 0.18 } else { 0.30 }),
                            ))
                            .child(
                                h_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .py_1()
                                    .border_r_1()
                                    .border_color(cx.theme().border.opacity(if is_dark {
                                        0.82
                                    } else {
                                        0.72
                                    }))
                                    .child(
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .text_xs()
                                            .font_semibold()
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .bg(cx.theme().muted.opacity(if is_dark {
                                                0.44
                                            } else {
                                                0.58
                                            }))
                                            .text_color(cx.theme().muted_foreground)
                                            .child("OLD"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .text_color(cx.theme().muted_foreground)
                                            .child(old_label),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .py_1()
                                    .child(
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .text_xs()
                                            .font_semibold()
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .bg(cx.theme().muted.opacity(if is_dark {
                                                0.44
                                            } else {
                                                0.58
                                            }))
                                            .text_color(cx.theme().muted_foreground)
                                            .child("NEW"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .text_color(cx.theme().muted_foreground)
                                            .child(new_label),
                                    ),
                            ),
                    )
                    .child(sticky_file_banner)
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .relative()
                            .child(
                                div()
                                    .size_full()
                                    .on_scroll_wheel(cx.listener(Self::on_diff_list_scroll_wheel))
                                    .child(list),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .right(right_inset)
                                    .bottom(vertical_bar_bottom)
                                    .w(scrollbar_size)
                                    .child(
                                        Scrollbar::vertical(&diff_list_state)
                                            .scrollbar_show(ScrollbarShow::Always),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_open_project_empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                v_flex()
                    .items_center()
                    .gap_3()
                    .max_w(px(520.0))
                    .px_8()
                    .py_6()
                    .rounded_lg()
                    .border_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.92 } else { 0.74 }))
                    .bg(cx.theme().sidebar.blend(cx.theme().muted.opacity(if is_dark {
                        0.22
                    } else {
                        0.34
                    })))
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Open a project"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Choose a folder that contains a JJ repository (or a Git repo to auto-init JJ).",
                            ),
                    )
                    .child(
                        Button::new("open-project-empty-state")
                            .primary()
                            .rounded(px(8.0))
                            .label("Open Project Folder")
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.open_project_picker(cx);
                                });
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Shortcut: Cmd/Ctrl+Shift+O"),
                    ),
            )
            .into_any_element()
    }

    fn render_visible_file_banner(
        &self,
        visible_row: usize,
        top_offset: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((header_row_ix, path, status)) = self.visible_file_header(visible_row) else {
            return div().w_full().h(px(0.)).into_any_element();
        };

        if visible_row == header_row_ix && top_offset.is_zero() {
            return div().w_full().h(px(0.)).into_any_element();
        }

        let stats = self.file_line_stats.get(path.as_str()).copied().unwrap_or_default();
        self.render_sticky_file_status_banner_row(header_row_ix, path.as_str(), status, stats, cx)
    }

    fn render_visible_hunk_banner(&self, visible_row: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some((path, header)) = self.visible_hunk_header(visible_row) else {
            return div().w_full().h(px(0.)).into_any_element();
        };

        let is_dark = cx.theme().mode.is_dark();
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .px_3()
            .py_0p5()
            .border_b_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.88 } else { 0.72 }))
            .bg(cx
                .theme()
                .title_bar
                .blend(
                    cx.theme()
                        .primary
                        .opacity(if is_dark { 0.20 } else { 0.09 }),
                ))
            .child(
                div()
                    .px_1p5()
                    .py_0p5()
                    .text_xs()
                    .font_semibold()
                    .font_family(cx.theme().mono_font_family.clone())
                    .rounded_sm()
                    .bg(cx
                        .theme()
                        .primary
                        .opacity(if is_dark { 0.38 } else { 0.22 }))
                    .text_color(cx.theme().primary_foreground)
                    .child("HUNK"),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .child(path),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(if is_dark {
                        cx.theme().primary.lighten(0.42)
                    } else {
                        cx.theme().primary.darken(0.12)
                    })
                    .child(header),
            )
            .into_any_element()
    }

    fn visible_hunk_header(&self, visible_row: usize) -> Option<(String, String)> {
        if self.diff_rows.is_empty() {
            return None;
        }

        let capped = visible_row.min(self.diff_rows.len().saturating_sub(1));

        if self.diff_row_metadata.len() == self.diff_rows.len() {
            let hunk_ix = self
                .diff_visible_hunk_header_lookup
                .get(capped)
                .copied()
                .flatten()?;
            let meta = self.diff_row_metadata.get(hunk_ix)?;
            let path = meta
                .file_path
                .clone()
                .or_else(|| self.selected_path.clone())
                .unwrap_or_else(|| "file".to_string());
            let header = self.diff_rows.get(hunk_ix)?.text.clone();
            return Some((path, header));
        }

        let hunk_ix = self
            .diff_visible_hunk_header_lookup
            .get(capped)
            .copied()
            .flatten()?;
        let path = self
            .selected_path
            .clone()
            .unwrap_or_else(|| "file".to_string());
        Some((path, self.diff_rows.get(hunk_ix)?.text.clone()))
    }

    fn visible_file_header(&self, visible_row: usize) -> Option<(usize, String, FileStatus)> {
        if self.diff_rows.is_empty() {
            return None;
        }

        let capped = visible_row.min(self.diff_rows.len().saturating_sub(1));

        if self.diff_row_metadata.len() == self.diff_rows.len() {
            let header_ix = self
                .diff_visible_file_header_lookup
                .get(capped)
                .copied()
                .flatten()?;
            let meta = self.diff_row_metadata.get(header_ix)?;
            if meta.kind == DiffStreamRowKind::EmptyState {
                return None;
            }
            let path = meta.file_path.clone()?;
            let status = meta
                .file_status
                .or_else(|| self.status_for_path(path.as_str()))
                .unwrap_or(FileStatus::Unknown);
            return Some((header_ix, path, status));
        }

        let header_ix = self
            .diff_visible_file_header_lookup
            .get(capped)
            .copied()
            .flatten()?;
        self.file_row_ranges
            .iter()
            .find(|range| range.start_row == header_ix)
            .map(|range| (range.start_row, range.path.clone(), range.status))
    }

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
            let stats = self.file_line_stats.get(path).copied().unwrap_or_default();
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
                cx.theme()
                    .primary
                    .opacity(if is_dark { 0.34 } else { 0.18 })
            } else {
                cx.theme().muted.opacity(if is_dark { 0.26 } else { 0.40 })
            };
            return div()
                .id(("diff-hunk-divider-row", stable_row_id))
                .h(px(6.0))
                .border_b_1()
                .border_color(cx.theme().border.opacity(if is_dark { 0.92 } else { 0.70 }))
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
                        cx.theme()
                            .background
                            .blend(
                                cx.theme()
                                    .success
                                    .opacity(if is_dark { 0.22 } else { 0.12 }),
                            ),
                        if is_dark {
                            cx.theme().success.lighten(0.45)
                        } else {
                            cx.theme().success.darken(0.10)
                        },
                        cx.theme().success,
                    )
                } else if line.starts_with("deleted file mode") || line.starts_with("--- a/") {
                    (
                        cx.theme()
                            .background
                            .blend(cx.theme().danger.opacity(if is_dark { 0.22 } else { 0.12 })),
                        if is_dark {
                            cx.theme().danger.lighten(0.45)
                        } else {
                            cx.theme().danger.darken(0.10)
                        },
                        cx.theme().danger,
                    )
                } else if line.starts_with("diff --git") {
                    (
                        cx.theme()
                            .background
                            .blend(cx.theme().accent.opacity(if is_dark { 0.18 } else { 0.10 })),
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
            .border_color(cx.theme().border.opacity(if is_dark { 0.82 } else { 0.70 }))
            .bg(if is_selected {
                background.blend(
                    cx.theme()
                        .primary
                        .opacity(if is_dark { 0.24 } else { 0.14 }),
                )
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

        v_flex()
            .w_full()
            .child(meta_row)
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .px_3()
                    .pt_1()
                    .child(self.render_row_comment_editor(ix, cx)),
            )
            .into_any_element()
    }

    fn render_code_row(
        &self,
        ix: usize,
        row_data: &SideBySideRow,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stable_row_id = self.diff_row_stable_id(ix);
        let code_row = h_flex()
            .id(("diff-code-row", stable_row_id))
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
            .border_b_1()
            .border_color(cx.theme().border.opacity(if cx.theme().mode.is_dark() {
                0.78
            } else {
                0.64
            }))
            .w_full()
            .child(self.render_diff_cell(
                stable_row_id,
                is_selected,
                DiffCellRenderSpec {
                    row_ix: ix,
                    side: "left",
                    cell: &row_data.left,
                    peer_kind: row_data.right.kind,
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
                },
                cx,
            ))
            .child(self.render_row_comment_affordance(ix, cx));

        v_flex()
            .w_full()
            .child(code_row)
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .px_3()
                    .pt_1()
                    .child(self.render_row_comment_editor(ix, cx)),
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
        let add_alpha = if is_dark { 0.24 } else { 0.11 };
        let remove_alpha = if is_dark { 0.24 } else { 0.11 };
        let dark_add_tint: gpui::Hsla = gpui::rgb(0x2e4736).into();
        let dark_remove_tint: gpui::Hsla = gpui::rgb(0x4a3038).into();
        let dark_add_accent: gpui::Hsla = gpui::rgb(0x8fcea0).into();
        let dark_remove_accent: gpui::Hsla = gpui::rgb(0xeea9b4).into();

        let (mut background, marker_color, line_color, text_color, marker) =
            match (cell.kind, peer_kind) {
                (DiffCellKind::Added, _) => (
                    if is_dark {
                        cx.theme().background.blend(dark_add_tint.opacity(0.62))
                    } else {
                        cx.theme()
                            .background
                            .blend(cx.theme().success.opacity(add_alpha))
                    },
                    if is_dark {
                        dark_add_accent
                    } else {
                        cx.theme().success.darken(0.18)
                    },
                    if is_dark {
                        dark_add_accent.lighten(0.08)
                    } else {
                        cx.theme().success.darken(0.16)
                    },
                    cx.theme().foreground,
                    "+",
                ),
                (DiffCellKind::Removed, _) => (
                    if is_dark {
                        cx.theme().background.blend(dark_remove_tint.opacity(0.62))
                    } else {
                        cx.theme()
                            .background
                            .blend(cx.theme().danger.opacity(remove_alpha))
                    },
                    if is_dark {
                        dark_remove_accent
                    } else {
                        cx.theme().danger.darken(0.18)
                    },
                    if is_dark {
                        dark_remove_accent.lighten(0.06)
                    } else {
                        cx.theme().danger.darken(0.16)
                    },
                    cx.theme().foreground,
                    "-",
                ),
                (DiffCellKind::Context, _) => (
                    cx.theme().background,
                    if is_dark {
                        cx.theme().muted_foreground.lighten(0.14)
                    } else {
                        cx.theme().muted_foreground.darken(0.10)
                    },
                    if is_dark {
                        cx.theme().muted_foreground.lighten(0.18)
                    } else {
                        cx.theme().muted_foreground.darken(0.12)
                    },
                    cx.theme().foreground,
                    "·",
                ),
                (DiffCellKind::None, _) => (
                    cx.theme().background,
                    if is_dark {
                        cx.theme().muted_foreground.lighten(0.14)
                    } else {
                        cx.theme().muted_foreground.darken(0.10)
                    },
                    if is_dark {
                        cx.theme().muted_foreground.lighten(0.18)
                    } else {
                        cx.theme().muted_foreground.darken(0.12)
                    },
                    if is_dark {
                        cx.theme().muted_foreground.lighten(0.08)
                    } else {
                        cx.theme().muted_foreground.darken(0.06)
                    },
                    "",
                ),
            };
        if matches!(cell.kind, DiffCellKind::Context | DiffCellKind::None)
            && row_stable_id.is_multiple_of(2)
        {
            background = background.blend(cx.theme().muted.opacity(if is_dark { 0.12 } else { 0.20 }));
        }
        if row_is_selected {
            background =
                background.blend(
                    cx.theme()
                        .primary
                        .opacity(if is_dark { 0.22 } else { 0.13 }),
                );
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
        let gutter_background = cx
            .theme()
            .background
            .blend(cx.theme().muted.opacity(if is_dark { 0.28 } else { 0.46 }));
        let gutter_width = line_number_width + DIFF_MARKER_GUTTER_WIDTH + 10.0;

        h_flex()
            .id(cell_id)
            .overflow_x_hidden()
            .px_1p5()
            .py_0p5()
            .gap_2()
            .items_start()
            .bg(background)
            .when(should_draw_right_divider, |this| {
                this.border_r_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.86 } else { 0.72 }))
            })
            .child(
                h_flex()
                    .items_start()
                    .gap_2()
                    .w(px(gutter_width))
                    .min_w(px(gutter_width))
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .bg(gutter_background)
                    .border_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.68 } else { 0.54 }))
                    .child(
                        div()
                            .w(px(line_number_width))
                            .text_xs()
                            .text_color(line_color)
                            .font_family(cx.theme().mono_font_family.clone())
                            .whitespace_nowrap()
                            .child(line_number),
                    )
                    .child(
                        div()
                            .w(px(DIFF_MARKER_GUTTER_WIDTH))
                            .text_xs()
                            .text_color(marker_color)
                            .font_family(cx.theme().mono_font_family.clone())
                            .whitespace_nowrap()
                            .child(marker),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_start()
                    .gap_0()
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
                            self.syntax_color_for_segment(text_color, segment.syntax, cx);
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .text_color(segment_color)
                            .when(segment.changed, |this| {
                                this.bg(marker_color.opacity(if is_dark { 0.20 } else { 0.11 }))
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
                                        cx.theme()
                                            .muted_foreground
                                            .opacity(if is_dark { 0.90 } else { 0.95 }),
                                    )
                                    .child("↵"),
                            )
                        },
                    ),
            )
            .flex_1()
            .min_w_0()
            .into_any_element()
    }

    fn syntax_color_for_segment(
        &self,
        default_color: gpui::Hsla,
        token: SyntaxTokenKind,
        cx: &mut Context<Self>,
    ) -> gpui::Hsla {
        let is_dark = cx.theme().mode.is_dark();
        let github = |dark: u32, light: u32| -> gpui::Hsla {
            if is_dark {
                gpui::rgb(dark).into()
            } else {
                gpui::rgb(light).into()
            }
        };
        match token {
            SyntaxTokenKind::Plain => default_color,
            SyntaxTokenKind::Keyword => github(0xff7b72, 0xcf222e),
            SyntaxTokenKind::String => github(0xa5d6ff, 0x0a3069),
            SyntaxTokenKind::Number => github(0x79c0ff, 0x0550ae),
            SyntaxTokenKind::Comment => github(0x8b949e, 0x57606a),
            SyntaxTokenKind::Function => github(0xd2a8ff, 0x8250df),
            SyntaxTokenKind::TypeName => github(0xffa657, 0x953800),
            SyntaxTokenKind::Constant => github(0x79c0ff, 0x0550ae),
            SyntaxTokenKind::Variable => github(0xffa657, 0x953800),
            SyntaxTokenKind::Operator => github(0xff7b72, 0xcf222e),
        }
    }

    fn diff_row_stable_id(&self, row_ix: usize) -> u64 {
        self.diff_row_metadata
            .get(row_ix)
            .map(|row| row.stable_id)
            .unwrap_or(row_ix as u64)
    }

    fn diff_column_labels(&self) -> (String, String) {
        let selected = self
            .selected_path
            .clone()
            .unwrap_or_else(|| "file".to_string());
        match self.selected_status.unwrap_or(FileStatus::Unknown) {
            FileStatus::Added | FileStatus::Untracked => ("/dev/null".to_string(), selected),
            FileStatus::Deleted => (selected, "/dev/null".to_string()),
            _ => ("Old".to_string(), "New".to_string()),
        }
    }
}
