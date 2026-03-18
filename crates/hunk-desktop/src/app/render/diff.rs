#[derive(Clone, Copy)]
struct DiffColumnLayout {
    left_panel_width: Pixels,
    right_panel_width: Pixels,
}

#[derive(Clone)]
struct DiffSplitDrag(EntityId);

impl Render for DiffSplitDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
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
        if self.repo_root.is_some()
            && self.workspace_view_mode != WorkspaceViewMode::Diff
            && self.files.is_empty()
        {
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
        let sticky_file_banner =
            self.render_visible_file_banner(visible_row, logical_top.offset_in_item, cx);
        let layout = self.diff_column_layout();

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
        let view = cx.entity();

        v_flex()
            .size_full()
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .when(self.workspace_view_mode == WorkspaceViewMode::Diff, |this| {
                        this.child(self.render_review_compare_controls(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .relative()
                            .child(
                                canvas(
                                    {
                                        let view = view.clone();
                                        move |bounds, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.update_diff_split_bounds(bounds, cx);
                                            });
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(
                                v_flex()
                                    .size_full()
                                    .items_stretch()
                                    .child(self.render_diff_column_header(
                                        layout,
                                        old_label.clone(),
                                        new_label.clone(),
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_h_0()
                                            .relative()
                                            .child(
                                                div()
                                                    .size_full()
                                                    .on_scroll_wheel(
                                                        cx.listener(Self::on_diff_list_scroll_wheel),
                                                    )
                                                    .child(list),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .top_0()
                                                    .left_0()
                                                    .right_0()
                                                    .child(sticky_file_banner),
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
                            .when_some(layout, |this, layout| {
                                this.child(self.render_diff_split_handle(layout, cx))
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_diff_column_header(
        &self,
        layout: Option<DiffColumnLayout>,
        old_label: String,
        new_label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        let chrome = hunk_diff_chrome(cx.theme(), is_dark);
        let divider = hunk_opacity(cx.theme().border, is_dark, 0.82, 0.72);
        let left_width = layout.map(|layout| layout.left_panel_width);
        let right_width = layout.map(|layout| layout.right_panel_width);

        h_flex()
            .w_full()
            .border_b_1()
            .border_color(chrome.row_divider)
            .bg(chrome.column_header_background)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .border_r_1()
                    .border_color(divider)
                    .when_some(left_width, |this, width| {
                        this.w(width).min_w(width).max_w(width).flex_none()
                    })
                    .when(left_width.is_none(), |this| this.flex_1().min_w_0())
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .text_xs()
                            .font_semibold()
                            .font_family(cx.theme().mono_font_family.clone())
                            .bg(chrome.column_header_badge_background)
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
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .when_some(right_width, |this, width| {
                        this.w(width).min_w(width).max_w(width).flex_none()
                    })
                    .when(right_width.is_none(), |this| this.flex_1().min_w_0())
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .text_xs()
                            .font_semibold()
                            .font_family(cx.theme().mono_font_family.clone())
                            .bg(chrome.column_header_badge_background)
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
            )
            .into_any_element()
    }

    fn render_diff_split_handle(
        &self,
        layout: DiffColumnLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity_id = cx.entity_id();
        let group = SharedString::from("diff-split-handle");
        let is_dark = cx.theme().mode.is_dark();
        let chrome = hunk_diff_chrome(cx.theme(), is_dark);
        let hit_width = px(DIFF_SPLIT_HANDLE_HIT_WIDTH);
        let line_width = px(DIFF_SPLIT_HANDLE_WIDTH);
        let handle_left = (layout.left_panel_width - hit_width / 2.).max(px(0.));
        let hover_color = hunk_tone(cx.theme().accent, is_dark, 0.32, 0.46);

        h_flex()
            .id("diff-split-handle")
            .absolute()
            .top_0()
            .bottom_0()
            .left(handle_left)
            .w(hit_width)
            .justify_center()
            .cursor_col_resize()
            .group(group.clone())
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_drag(DiffSplitDrag(entity_id), |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_drag_move(cx.listener(move |this, event: &DragMoveEvent<DiffSplitDrag>, _, cx| {
                if event.drag(cx).0 != entity_id {
                    return;
                }
                this.update_diff_split_ratio_from_position(event.event.position, cx);
            }))
            .child(
                div()
                    .h_full()
                    .w(line_width)
                    .bg(chrome.center_divider)
                    .group_hover(group, |this| this.bg(hover_color)),
            )
            .into_any_element()
    }

    fn update_diff_split_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let width_changed = self.diff_split_bounds.is_none_or(|current| {
            (current.left() - bounds.left()).abs() > px(0.5)
                || (current.size.width - bounds.size.width).abs() > px(0.5)
        });
        let clamped_ratio = self.clamp_diff_split_ratio(bounds.size.width, self.diff_split_ratio);
        let ratio_changed = (clamped_ratio - self.diff_split_ratio).abs() > f32::EPSILON;

        if !width_changed && !ratio_changed {
            return;
        }

        self.diff_split_bounds = Some(bounds);
        self.diff_split_ratio = clamped_ratio;
        cx.notify();
    }

    fn update_diff_split_ratio_from_position(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.diff_split_bounds else {
            return;
        };
        let local_x = (position.x - bounds.left()).clamp(px(0.), bounds.size.width);
        let next_ratio = self.clamp_diff_split_ratio(bounds.size.width, local_x / bounds.size.width);
        if (next_ratio - self.diff_split_ratio).abs() <= f32::EPSILON {
            return;
        }

        self.diff_split_ratio = next_ratio;
        cx.notify();
    }

    fn diff_column_layout(&self) -> Option<DiffColumnLayout> {
        let bounds = self.diff_split_bounds?;
        let total_width = bounds.size.width;
        if total_width <= px(0.) {
            return None;
        }

        let left_gutter = px(self.diff_left_line_number_width + DIFF_MARKER_GUTTER_WIDTH + 16.0);
        let right_gutter = px(self.diff_right_line_number_width + DIFF_MARKER_GUTTER_WIDTH + 16.0);
        let minimum_content_width = px(DIFF_SPLIT_MIN_CODE_WIDTH);
        let left_min = left_gutter + minimum_content_width;
        let right_min = right_gutter + minimum_content_width;
        let minimum_total = left_min + right_min;

        let left_panel_width = if total_width <= minimum_total {
            let shared_content = (total_width - left_gutter - right_gutter).max(px(0.)) / 2.;
            left_gutter + shared_content
        } else {
            (total_width * self.diff_split_ratio).clamp(left_min, total_width - right_min)
        };

        Some(DiffColumnLayout {
            left_panel_width,
            right_panel_width: total_width - left_panel_width,
        })
    }

    fn clamp_diff_split_ratio(&self, total_width: Pixels, candidate_ratio: f32) -> f32 {
        let left_gutter = px(self.diff_left_line_number_width + DIFF_MARKER_GUTTER_WIDTH + 16.0);
        let right_gutter = px(self.diff_right_line_number_width + DIFF_MARKER_GUTTER_WIDTH + 16.0);
        let minimum_content_width = px(DIFF_SPLIT_MIN_CODE_WIDTH);
        let left_min = left_gutter + minimum_content_width;
        let right_min = right_gutter + minimum_content_width;
        if total_width <= px(0.) || total_width <= left_min + right_min {
            return 0.5;
        }

        let left_width =
            (total_width * candidate_ratio).clamp(left_min, total_width - right_min);
        left_width / total_width
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
                    .border_color(hunk_opacity(cx.theme().border, is_dark, 0.92, 0.74))
                    .bg(hunk_blend(cx.theme().sidebar, cx.theme().muted, is_dark, 0.22, 0.34))
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
                            .child("Choose a folder that contains a Git repository."),
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

        let stats = self
            .active_diff_file_line_stats()
            .get(path.as_str())
            .copied()
            .unwrap_or_default();
        self.render_sticky_file_status_banner_row(header_row_ix, path.as_str(), status, stats, cx)
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

    fn diff_column_labels(&self) -> (String, String) {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return (
                self.review_compare_source_label(self.review_left_source_id.as_deref()),
                self.review_compare_source_label(self.review_right_source_id.as_deref()),
            );
        }

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

    fn render_review_compare_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let left_label = self.review_compare_source_label(self.review_left_source_id.as_deref());
        let right_label = self.review_compare_source_label(self.review_right_source_id.as_deref());
        let reset_available = self.review_compare_reset_available();
        let picker_surface = hunk_blend(
            cx.theme().background,
            cx.theme().muted,
            is_dark,
            0.24,
            0.16,
        );
        let picker_border = hunk_opacity(cx.theme().border, is_dark, 0.96, 0.84);
        let picker_title = hunk_opacity(cx.theme().foreground, is_dark, 0.82, 0.90);
        let arrow_color = hunk_tone(cx.theme().accent, is_dark, 0.26, 0.42);
        let status_message = if let Some(error) = self.review_compare_error.as_ref() {
            error.clone()
        } else if self.review_compare_loading {
            "Loading comparison...".to_string()
        } else if !self.review_comments_enabled() {
            "Custom compare mode is read-only. Comments are disabled.".to_string()
        } else {
            self.review_compare_source_detail(self.review_left_source_id.as_deref())
                .zip(self.review_compare_source_detail(self.review_right_source_id.as_deref()))
                .map(|(left, right)| format!("{left} -> {right}"))
                .unwrap_or_else(|| "Choose a base source and a compare source.".to_string())
        };

        v_flex()
            .w_full()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.88, 0.72))
            .bg(hunk_blend(
                cx.theme().title_bar,
                cx.theme().muted,
                is_dark,
                0.16,
                0.24,
            ))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Diff Sources"),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if self.review_compare_error.is_some() {
                                        cx.theme().danger
                                    } else if self.review_compare_loading {
                                        cx.theme().warning
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(status_message),
                            )
                            .child({
                                let view = view.clone();
                                Button::new("review-compare-reset")
                                    .compact()
                                    .outline()
                                    .rounded(px(7.0))
                                    .label("Reset")
                                    .disabled(!reset_available || self.review_compare_loading)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.reset_review_compare_selection(cx);
                                        });
                                    })
                            }),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        v_flex()
                            .min_w(px(240.0))
                            .flex_1()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(picker_title)
                                    .child("Base"),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .p_1()
                                    .rounded(px(10.0))
                                    .border_1()
                                    .border_color(picker_border)
                                    .bg(picker_surface)
                                    .child(
                                        Select::new(&self.review_left_picker_state)
                                            .with_size(gpui_component::Size::Medium)
                                            .placeholder(left_label)
                                            .search_placeholder("Find a branch or worktree")
                                            .rounded(px(8.0))
                                            .w_full()
                                            .disabled(self.review_compare_sources.is_empty())
                                            .empty(
                                                h_flex()
                                                    .h(px(72.0))
                                                    .justify_center()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("No compare sources available."),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(20.0))
                            .flex_none()
                            .text_color(arrow_color)
                            .child(Icon::new(IconName::ArrowRight).size(px(20.0))),
                    )
                    .child(
                        v_flex()
                            .min_w(px(240.0))
                            .flex_1()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(picker_title)
                                    .child("Compare"),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .p_1()
                                    .rounded(px(10.0))
                                    .border_1()
                                    .border_color(picker_border)
                                    .bg(picker_surface)
                                    .child(
                                        Select::new(&self.review_right_picker_state)
                                            .with_size(gpui_component::Size::Medium)
                                            .placeholder(right_label)
                                            .search_placeholder("Find a branch or worktree")
                                            .rounded(px(8.0))
                                            .w_full()
                                            .disabled(self.review_compare_sources.is_empty())
                                            .empty(
                                                h_flex()
                                                    .h(px(72.0))
                                                    .justify_center()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("No compare sources available."),
                                            ),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}
