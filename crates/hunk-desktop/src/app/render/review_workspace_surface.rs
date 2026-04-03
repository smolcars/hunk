impl DiffViewer {
    fn current_or_fresh_review_surface_snapshot(
        &mut self,
    ) -> Option<review_workspace_session::ReviewWorkspaceSurfaceSnapshot> {
        if !self.uses_review_workspace_sections_surface() {
            return None;
        }

        self.refresh_review_surface_snapshot()
            .and_then(|_| self.current_review_surface_snapshot().cloned())
            .or_else(|| {
                let session = self.review_workspace_session.as_ref()?;
                let mut surface = session.build_surface_snapshot(
                    self.current_review_surface_scroll_top_px(),
                    self.review_surface
                        .diff_scroll_handle
                        .bounds()
                        .size
                        .height
                        .max(Pixels::ZERO)
                        .as_f32()
                        .round() as usize,
                    1,
                    REVIEW_SECTION_ROW_OVERSCAN_ROWS,
                );
                self.decorate_review_surface_snapshot(&mut surface);
                Some(surface)
            })
    }

    fn render_review_workspace_sticky_file_banner(
        &self,
        surface: &review_workspace_session::ReviewWorkspaceSurfaceSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(header) = surface.sticky_file_header.as_ref() else {
            return div().w_full().h(px(0.)).into_any_element();
        };

        self.render_sticky_file_status_banner_row(
            header.row_index,
            header.path.as_str(),
            header.status,
            header.line_stats,
            cx,
        )
    }

    fn render_review_workspace_surface(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
        let review_surface_snapshot = self.current_or_fresh_review_surface_snapshot();
        let sticky_file_banner = review_surface_snapshot
            .as_ref()
            .map(|surface| self.render_review_workspace_sticky_file_banner(surface, cx))
            .unwrap_or_else(|| {
                let row_count = self.active_diff_row_count();
                let visible_row = self
                    .current_review_surface_top_row()
                    .unwrap_or(0)
                    .min(row_count.saturating_sub(1));
                self.render_visible_file_banner(visible_row, px(0.), cx)
            });
        let layout = self.diff_column_layout();
        let scroller = if let Some(surface) = review_surface_snapshot.as_ref() {
            self.render_review_workspace_sections_scroller(surface, cx)
        } else {
            self.render_review_workspace_status_surface(cx)
        };

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
                                                    .child(scroller),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .top_0()
                                                    .left_0()
                                                    .right_0()
                                                    .child(sticky_file_banner),
                                            )
                                            .when_some(
                                                self.render_active_row_comment_overlay(cx),
                                                |this, overlay| {
                                                    this.child(overlay)
                                                },
                                            )
                                            .when(
                                                self.uses_review_workspace_sections_surface(),
                                                |this| {
                                                    this.child(
                                                        div()
                                                            .absolute()
                                                            .top_0()
                                                            .right(right_inset)
                                                            .bottom(vertical_bar_bottom)
                                                            .w(scrollbar_size)
                                                            .child(
                                                                Scrollbar::vertical(
                                                                    &self.review_surface
                                                                        .diff_scroll_handle,
                                                                )
                                                                .scrollbar_show(
                                                                    ScrollbarShow::Always,
                                                                ),
                                                            ),
                                                    )
                                                },
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

    fn render_review_workspace_status_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let message = self
            .active_diff_row(0)
            .map(|row| row.text.clone())
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| "Loading comparison...".to_string());

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

    fn render_review_workspace_surface_children(
        &self,
        surface: &review_workspace_session::ReviewWorkspaceSurfaceSnapshot,
        visible_pixel_range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let painted_surface = div()
            .absolute()
            .top(px(visible_pixel_range.start as f32))
            .left_0()
            .right_0()
            .h(px(visible_pixel_range.len() as f32))
            .child(self.render_review_workspace_viewport(
                &surface.viewport,
                visible_pixel_range.start,
                cx,
            ))
            .into_any_element();

        let sparse_overlays = surface
            .overlays
            .iter()
            .map(|overlay| match &overlay.kind {
                review_workspace_session::ReviewWorkspaceSurfaceOverlayKind::FileHeaderControls {
                    path,
                    status,
                } => div()
                    .absolute()
                    .top(px(overlay.top_px as f32))
                    .left_0()
                    .right_0()
                    .h(px(overlay.height_px as f32))
                    .child(self.render_review_workspace_file_header_controls_overlay(
                        overlay.row_index,
                        path.as_str(),
                        *status,
                        self.is_row_selected(overlay.row_index),
                        cx,
                    ))
                    .into_any_element(),
                review_workspace_session::ReviewWorkspaceSurfaceOverlayKind::CommentAffordance => div()
                    .absolute()
                    .top(px(overlay.top_px as f32))
                    .left_0()
                    .right_0()
                    .h(px(overlay.height_px as f32))
                    .child(self.render_row_comment_affordance(overlay.row_index, cx))
                    .into_any_element(),
            })
            .collect::<Vec<_>>();

        vec![painted_surface]
            .into_iter()
            .chain(sparse_overlays)
            .collect()
    }

    fn render_review_workspace_sections_scroller(
        &self,
        surface: &review_workspace_session::ReviewWorkspaceSurfaceSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scroll_handle = self.review_surface.diff_scroll_handle.clone();
        let surface_children = surface
            .viewport
            .visible_pixel_range()
            .map(|visible_pixel_range| {
                self.render_review_workspace_surface_children(
                    surface,
                    visible_pixel_range,
                    cx,
                )
            })
            .unwrap_or_default();

        div()
            .id("review-workspace-sections-scroll")
            .size_full()
            .track_scroll(&scroll_handle)
            .overflow_y_scroll()
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(surface.viewport.total_surface_height_px as f32))
                    .children(surface_children),
            )
            .into_any_element()
    }

    fn render_review_workspace_viewport(
        &self,
        viewport: &review_workspace_session::ReviewWorkspaceViewportSnapshot,
        viewport_origin_px: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let visible_height_px = viewport
            .visible_pixel_range()
            .map(|range| range.len())
            .unwrap_or_default();
        let layout = self.diff_column_layout();

        div()
            .id("review-workspace-viewport")
            .relative()
            .w_full()
            .h(px(visible_height_px as f32))
            .child(self.render_review_workspace_viewport_element(
                viewport,
                viewport_origin_px,
                layout,
                cx,
            ))
            .into_any_element()
    }

    fn render_diff_workspace_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .size_full()
            .child(if self.sidebar_collapsed {
                self.render_review_workspace_surface(window, cx)
                    .into_any_element()
            } else {
                h_resizable("hunk-diff-workspace")
                    .child(
                        resizable_panel()
                            .size(px(300.0))
                            .size_range(px(240.0)..px(520.0))
                            .child(self.render_tree(cx)),
                    )
                    .child(
                        resizable_panel().child(self.render_review_workspace_surface(window, cx)),
                    )
                    .into_any_element()
            })
            .into_any_element()
    }
}
