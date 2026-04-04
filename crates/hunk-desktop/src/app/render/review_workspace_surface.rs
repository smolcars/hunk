impl DiffViewer {
    fn current_or_fresh_review_surface_snapshot(
        &mut self,
    ) -> Option<review_workspace_session::ReviewWorkspaceSurfaceSnapshot> {
        if !self.uses_review_workspace_sections_surface() {
            return None;
        }

        self.refresh_review_surface_snapshot()
            .and_then(|_| self.current_review_surface_snapshot().cloned())
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
        let is_dark = cx.theme().mode.is_dark();
        let editor_chrome = crate::app::theme::hunk_editor_chrome_colors(cx.theme(), is_dark);
        let search_match_count = self.active_editor_search_match_count();
        let review_surface_snapshot = self.current_or_fresh_review_surface_snapshot();
        let layout = self.diff_column_layout();
        let scroller = if let Some(surface) = review_surface_snapshot.as_ref() {
            self.render_review_workspace_viewport_scroller(surface, cx)
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
            .on_key_down({
                let view = view.clone();
                move |event, window, cx| {
                    let handled = view.update(cx, |this, cx| {
                        let uses_primary_shortcut = if cfg!(target_os = "macos") {
                            event.keystroke.modifiers.platform
                        } else {
                            event.keystroke.modifiers.control
                        };
                        if uses_primary_shortcut
                            && !event.keystroke.modifiers.shift
                            && event.keystroke.key == "f"
                        {
                            this.toggle_editor_search(true, window, cx);
                            return true;
                        }
                        false
                    });
                    if handled {
                        cx.stop_propagation();
                    }
                }
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .when(self.workspace_view_mode == WorkspaceViewMode::Diff, |this| {
                        this.child(self.render_review_compare_controls(cx))
                    })
                    .when(self.editor_search_visible, |this| {
                        this.child(self.render_workspace_search_bar(
                            view.clone(),
                            editor_chrome,
                            is_dark,
                            search_match_count,
                            false,
                            cx,
                        ))
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
            .review_compare_error
            .clone()
            .or_else(|| self.review_surface.status_message.clone())
            .unwrap_or_else(|| {
                if self.review_compare_loading {
                    "Loading comparison...".to_string()
                } else if self.project_path.is_none() {
                    "Open a Git repository to compare workspaces.".to_string()
                } else if self.review_left_source_id.is_none()
                    || self.review_right_source_id.is_none()
                {
                    "Select two compare sources.".to_string()
                } else if self.review_files.is_empty() {
                    "No files changed between the selected sources.".to_string()
                } else {
                    "Loading comparison...".to_string()
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

    fn render_review_workspace_surface_child(
        &self,
        surface: &review_workspace_session::ReviewWorkspaceSurfaceSnapshot,
        visible_pixel_range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .absolute()
            .top(px(visible_pixel_range.start as f32))
            .left_0()
            .right_0()
            .h(px(visible_pixel_range.len() as f32))
            .child(self.render_review_workspace_viewport(
                surface,
                &surface.viewport,
                visible_pixel_range.start,
                cx,
            ))
            .into_any_element()
    }

    fn render_review_workspace_viewport_scroller(
        &self,
        surface: &review_workspace_session::ReviewWorkspaceSurfaceSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scroll_handle = self.review_surface.diff_scroll_handle.clone();

        div()
            .id("review-workspace-viewport-scroll")
            .size_full()
            .track_scroll(&scroll_handle)
            .overflow_y_scroll()
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(surface.viewport.total_surface_height_px as f32))
                    .when_some(surface.viewport.visible_pixel_range(), |this, visible_pixel_range| {
                        this.child(self.render_review_workspace_surface_child(
                            surface,
                            visible_pixel_range,
                            cx,
                        ))
                    }),
            )
            .into_any_element()
    }

    fn render_review_workspace_viewport(
        &self,
        surface: &review_workspace_session::ReviewWorkspaceSurfaceSnapshot,
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
                surface,
                viewport,
                viewport_origin_px,
                layout,
                cx,
            ))
            .when_some(surface.active_comment_editor_overlay.as_ref(), |this, overlay| {
                this.child(self.render_active_row_comment_overlay(
                    overlay.row_index,
                    overlay.top_px,
                    cx,
                ))
            })
            .into_any_element()
    }

    fn render_diff_workspace_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let surface = self.render_review_workspace_surface(window, cx);
        self.render_tree_workspace_screen("hunk-diff-workspace", surface, cx)
    }
}
