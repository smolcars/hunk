impl DiffViewer {
    fn render_jj_workspace_graph_shell(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(
                div().size_full().min_h_0().min_w_0().child(
                    h_resizable("hunk-jj-graph-workspace")
                        .child(
                            resizable_panel()
                                .size(px(700.0))
                                .size_range(px(360.0)..px(1200.0))
                                .child(
                                    div()
                                        .size_full()
                                        .min_h_0()
                                        .min_w_0()
                                        .child(self.render_jj_graph_canvas(cx)),
                                ),
                        )
                        .child(
                            resizable_panel()
                                .size(px(440.0))
                                .size_range(px(320.0)..px(760.0))
                                .child(
                                    div()
                                        .size_full()
                                        .min_h_0()
                                        .min_w_0()
                                        .child(self.render_jj_graph_right_panel(cx)),
                                ),
                        ),
                ),
            )
            .into_any_element()
    }

    fn render_jj_graph_right_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        let panel_body = match self.graph_right_panel_mode {
            GraphRightPanelMode::ActiveWorkflow => self.render_jj_graph_active_workflow_panel(cx),
            GraphRightPanelMode::SelectedBookmark => self.render_jj_graph_selected_bookmark_panel(cx),
        };

        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .gap_2()
            .p_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
            .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                0.16
            } else {
                0.24
            })))
            .child(self.render_jj_graph_right_panel_mode_switch(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        div()
                            .id("jj-graph-right-scroll-area")
                            .size_full()
                            .track_scroll(&self.graph_right_panel_scroll_handle)
                            .overflow_y_scroll()
                            .child(v_flex().w_full().gap_2().pb_2().child(panel_body)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(px(16.0))
                            .child(
                                Scrollbar::vertical(&self.graph_right_panel_scroll_handle)
                                    .scrollbar_show(ScrollbarShow::Always),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_jj_graph_right_panel_mode_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let selected_available = self.graph_selected_bookmark.is_some();
        let active_selected = self.graph_right_panel_mode == GraphRightPanelMode::ActiveWorkflow;
        let bookmark_selected = self.graph_right_panel_mode == GraphRightPanelMode::SelectedBookmark;

        v_flex()
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Right Panel Mode"),
                    )
                    .child({
                        let view = view.clone();
                        let mut button = Button::new("jj-workspace-terms-toggle")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("JJ Terms")
                            .tooltip("Show a quick glossary of JJ terms used in this workspace.")
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.toggle_jj_terms_glossary(cx);
                                });
                            });
                        if self.show_jj_terms_glossary {
                            button = button.primary();
                        }
                        button
                    }),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .flex_wrap()
                    .child({
                        let view = view.clone();
                        let button = Button::new("jj-graph-right-mode-active")
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Active Workflow")
                            .tooltip("Show working-copy, commit, and active-bookmark actions.")
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_graph_right_panel_mode_active(cx);
                                });
                            });
                        if active_selected {
                            button.primary().into_any_element()
                        } else {
                            button.outline().into_any_element()
                        }
                    })
                    .child({
                        let view = view.clone();
                        let button = Button::new("jj-graph-right-mode-selected")
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Selected Bookmark")
                            .tooltip("Show bookmark-focused history and actions for the selected graph bookmark.")
                            .disabled(!selected_available)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_graph_right_panel_mode_selected(cx);
                                });
                            });
                        if bookmark_selected {
                            button.primary().into_any_element()
                        } else {
                            button.outline().into_any_element()
                        }
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if selected_available {
                                "Select bookmark chips in graph to populate bookmark mode."
                            } else {
                                "No bookmark selected in graph."
                            }),
                    ),
            )
            .when(self.show_jj_terms_glossary, |this| {
                this.child(self.render_jj_terms_glossary_card(cx))
            })
            .into_any_element()
    }

    fn render_jj_graph_active_workflow_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .w_full()
            .gap_2()
            .child(
                v_flex()
                    .w_full()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Active Workflow Mode"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Use this mode for working-copy changes, commit actions, and active bookmark operations."),
                    ),
            )
            .child(self.render_jj_graph_operations_panel(cx))
            .into_any_element()
    }

    fn render_jj_graph_selected_bookmark_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.graph_selected_bookmark.is_none() {
            return v_flex()
                .w_full()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No bookmark selected. Click a bookmark chip in the graph."),
                )
                .child({
                    let view = cx.entity();
                    Button::new("jj-graph-right-mode-fallback")
                        .outline()
                        .compact()
                        .with_size(gpui_component::Size::Small)
                        .rounded(px(7.0))
                        .label("Back to Active Workflow")
                        .tooltip("Return to active workflow actions.")
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.set_graph_right_panel_mode_active(cx);
                            });
                        })
                })
                .into_any_element();
        }

        v_flex()
            .w_full()
            .gap_2()
            .child(
                v_flex()
                    .w_full()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Selected Bookmark Mode"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Inspect a bookmark chain here, then activate it only when you want to make it your active working context."),
                    ),
            )
            .child({
                let view = cx.entity();
                let selected_local = self
                    .graph_selected_bookmark
                    .as_ref()
                    .is_some_and(|bookmark| bookmark.scope == GraphBookmarkScope::Local);
                Button::new("jj-graph-activate-selected-bookmark")
                    .primary()
                    .compact()
                    .with_size(gpui_component::Size::Small)
                    .rounded(px(7.0))
                    .label("Activate This Bookmark")
                    .tooltip("Switch active work to the selected local bookmark. If there are local changes, you will be asked how to switch.")
                    .disabled(self.git_action_loading || !selected_local)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.request_activate_selected_graph_bookmark(cx);
                        });
                    })
            })
            .child(self.render_jj_graph_inspector(cx))
            .child(self.render_jj_graph_focus_strip(cx))
            .into_any_element()
    }

    fn render_jj_graph_canvas(&self, cx: &mut Context<Self>) -> AnyElement {
        let graph_list_state = self.graph_list_state.clone();
        let is_dark = cx.theme().mode.is_dark();
        let nodes_len = self.graph_nodes.len();
        let view = cx.entity();
        let working_copy_color = self.graph_working_copy_color(is_dark);
        let active_target_color = self.graph_active_target_color(is_dark);
        let merge_color = self.graph_merge_color(is_dark);
        let legend_text_color = cx.theme().muted_foreground.opacity(if is_dark { 0.94 } else { 0.88 });
        let lane_focus = self.graph_selected_lane_hint();
        let max_lane_count = self.graph_max_lane_count();
        let (preferred_lane_start, _) = self.graph_lane_window(max_lane_count, lane_focus);

        v_flex()
            .size_full()
            .min_h_0()
            .gap_1()
            .p_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
            .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                0.16
            } else {
                0.24
            })))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Revision Graph"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("Tree mode graph · single-select"),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("Legend:"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(cx.theme().foreground)
                                    .child("○"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("commit ·"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(merge_color)
                                    .child("◆"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("merge ·"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(working_copy_color)
                                    .child("@"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("working-copy parent ·"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(active_target_color)
                                    .child("*"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("active bookmark target"),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(legend_text_color)
                            .child(format!(
                                "{} nodes{}",
                                nodes_len,
                                if self.graph_has_more { " (windowed)" } else { "" }
                            )),
                    ),
            )
            .child({
                if self.graph_nodes.is_empty() {
                    return div()
                        .flex_1()
                        .min_h_0()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No revisions available."),
                        )
                        .into_any_element();
                }

                let list = list(
                    graph_list_state.clone(),
                    cx.processor(move |this, ix: usize, _window, cx| {
                        let Some(node) = this.graph_nodes.get(ix) else {
                            return div().into_any_element();
                        };
                        let lane_row = this.graph_lane_rows.get(ix);
                        this.render_jj_graph_row(ix, node, lane_row, preferred_lane_start, cx)
                    }),
                )
                .flex_grow()
                .size_full()
                .with_sizing_behavior(ListSizingBehavior::Auto);

                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(list)
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(px(16.0))
                            .child(
                                Scrollbar::vertical(&graph_list_state)
                                    .scrollbar_show(ScrollbarShow::Always),
                            ),
                    )
                    .into_any_element()
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(legend_text_color)
                            .child(format!("{} edges", self.graph_edges.len())),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(legend_text_color)
                            .child(format!(
                                "active: {}",
                                self.graph_active_bookmark.as_deref().unwrap_or("detached")
                            )),
                    )
                    .when_some(self.graph_selected_bookmark.as_ref(), |this, selected| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(legend_text_color)
                                .child(format!("selected: {}", selected.name)),
                        )
                    })
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .font_semibold()
                                    .text_color(working_copy_color)
                                    .child("@"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("working-copy parent"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .font_semibold()
                                    .text_color(active_target_color)
                                    .child("*"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("active bookmark target"),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .font_semibold()
                                    .text_color(merge_color)
                                    .child("◆"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("merge commit"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(legend_text_color)
                                    .child("selected row = active focus"),
                            ),
                    )
                    .when(self.graph_has_more, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(legend_text_color)
                                .child("More history available in backend windowing."),
                        )
                    })
                    .child({
                        let view = view.clone();
                        Button::new("jj-graph-focus-active")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Focus Active Bookmark")
                            .tooltip("Select and focus the currently active bookmark in the graph.")
                            .disabled(self.graph_active_bookmark.is_none())
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.select_active_graph_bookmark(cx);
                                });
                            })
                    }),
            )
            .into_any_element()
    }

    fn render_jj_graph_row(
        &self,
        row_ix: usize,
        node: &GraphNode,
        lane_row: Option<&GraphLaneRow>,
        preferred_lane_start: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lane_row = lane_row.filter(|lane_row| lane_row.node_id == node.id);
        let view = cx.entity();
        let node_id = node.id.clone();
        let is_dark = cx.theme().mode.is_dark();
        let is_selected = self.graph_node_is_selected(node.id.as_str());
        let row_bg = if is_selected {
            cx.theme().accent.opacity(if is_dark { 0.22 } else { 0.14 })
        } else {
            cx.theme().background.opacity(0.0)
        };
        let row_border = if is_selected {
            cx.theme().accent.opacity(if is_dark { 0.72 } else { 0.58 })
        } else {
            cx.theme().border.opacity(0.0)
        };
        let parent_count = self
            .graph_edges
            .iter()
            .filter(|edge| edge.from == node.id)
            .count();
        let lane_count = lane_row.map_or(1, |row| row.lane_count.max(1));
        let node_lane = lane_row.map_or(0, |row| row.node_lane);
        let (lane_start, lane_end) =
            self.graph_lane_window_for_row(lane_count, preferred_lane_start, node_lane);
        let visible_lane_count = lane_end.saturating_sub(lane_start).max(1);
        let gutter_width = self.tree_lane_gutter_width(visible_lane_count);
        let short_id = node.id.chars().take(12).collect::<String>();

        let row = div()
            .id(("jj-graph-row", row_ix))
            .relative()
            .w_full()
            .py_0p5()
            .pl(px(gutter_width + 14.0))
            .pr_2()
            .rounded(px(6.0))
            .border_1()
            .border_color(row_border)
            .bg(row_bg)
            .on_click({
                let view = view.clone();
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.select_graph_node(node_id.clone(), cx);
                    });
                }
            })
            .child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .top_0()
                    .bottom_0()
                    .w(px(gutter_width))
                    .child(
                        self.render_jj_graph_lane_gutter(
                            node,
                            lane_row,
                            parent_count > 1,
                            (lane_start, lane_end),
                            cx,
                        ),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(cx.theme().muted_foreground)
                                    .child(short_id),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(relative_time_label(Some(node.unix_time))),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("parents:{parent_count}")),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(node.subject.clone()),
                    )
                    .child(
                        h_flex().w_full().items_center().gap_1().flex_wrap().children(
                            node.bookmarks.iter().enumerate().map(|(bookmark_ix, bookmark)| {
                                self.render_jj_graph_bookmark_chip(
                                    node.id.as_str(),
                                    row_ix,
                                    bookmark_ix,
                                    bookmark,
                                    cx,
                                )
                            }),
                        ),
                    ),
            );
        row.into_any_element()
    }

    fn render_jj_graph_lane_gutter(
        &self,
        node: &GraphNode,
        lane_row: Option<&GraphLaneRow>,
        is_merge_commit: bool,
        lane_window: (usize, usize),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (lane_start, lane_end) = lane_window;
        let is_dark = cx.theme().mode.is_dark();
        let lane_row = match lane_row {
            Some(lane_row) if lane_row.lane_count > 0 => lane_row,
            _ => {
                let marker = if node.is_working_copy_parent {
                    "@"
                } else if node.is_active_bookmark_target {
                    "*"
                } else {
                    "○"
                };
                let marker_color = if node.is_working_copy_parent {
                    self.graph_working_copy_color(is_dark)
                } else if node.is_active_bookmark_target {
                    self.graph_active_target_color(is_dark)
                } else {
                    cx.theme().foreground.opacity(if is_dark { 0.92 } else { 0.84 })
                };
                return div()
                    .w(px(18.0))
                    .pt_0p5()
                    .text_xs()
                    .font_semibold()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(marker_color)
                    .child(marker)
                    .into_any_element();
            }
        };

        let selected = self.graph_node_is_selected(node.id.as_str());
        let connector_color = if selected {
            cx.theme().accent.opacity(if is_dark { 1.0 } else { 0.88 })
        } else {
            cx.theme().muted_foreground.opacity(if is_dark { 0.94 } else { 0.76 })
        };
        let node_color = if node.is_working_copy_parent {
            self.graph_working_copy_color(is_dark)
        } else if node.is_active_bookmark_target {
            self.graph_active_target_color(is_dark)
        } else if is_merge_commit {
            self.graph_merge_color(is_dark)
        } else {
            cx.theme().foreground.opacity(if is_dark { 0.92 } else { 0.84 })
        };
        let node_marker = if node.is_working_copy_parent {
            Some("@")
        } else if node.is_active_bookmark_target {
            Some("*")
        } else if is_merge_commit {
            Some("◆")
        } else {
            None
        };

        let visible_lane_count = lane_end.saturating_sub(lane_start).max(1);
        let lane_spacing = 14.0_f32;
        let node_center_y = 9.0_f32;
        let node_size = 12.0_f32;
        let node_lane_x = lane_row.node_lane.saturating_sub(lane_start) as f32 * lane_spacing + 7.0;
        let mut gutter = div()
            .relative()
            .h_full()
            .w(px(self.tree_lane_gutter_width(visible_lane_count)))
            .min_h(px(26.0));

        for lane_ix in lane_start..lane_end {
            let top_vertical = lane_row.top_vertical.get(lane_ix).copied().unwrap_or(false);
            let bottom_vertical = lane_row.bottom_vertical.get(lane_ix).copied().unwrap_or(false);
            let has_vertical = top_vertical || bottom_vertical;
            if !has_vertical {
                continue;
            }
            let lane_x = lane_ix.saturating_sub(lane_start) as f32 * lane_spacing + 7.0;
            gutter = gutter.child(
                div()
                    .absolute()
                    .left(px(lane_x))
                    .top(px(-8.0))
                    .w(px(2.0))
                    .bottom(px(-8.0))
                    .bg(connector_color),
            );
        }

        for secondary_lane in &lane_row.secondary_parent_lanes {
            if *secondary_lane < lane_start || *secondary_lane >= lane_end {
                continue;
            }
            let start_lane = (*secondary_lane).min(lane_row.node_lane);
            let end_lane = (*secondary_lane).max(lane_row.node_lane);
            let start_lane = start_lane.max(lane_start);
            let end_lane = end_lane.min(lane_end.saturating_sub(1));
            let left = start_lane.saturating_sub(lane_start) as f32 * lane_spacing + 7.0;
            let width = (end_lane.saturating_sub(start_lane)) as f32 * lane_spacing + 2.0;
            gutter = gutter.child(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(node_center_y))
                    .w(px(width))
                    .h(px(2.0))
                    .bg(connector_color),
            );
        }

        gutter = gutter.child(
            div()
                .absolute()
                .left(px(node_lane_x - (node_size * 0.5)))
                .top(px(node_center_y - (node_size * 0.5)))
                .size(px(node_size))
                .rounded(if is_merge_commit { px(2.0) } else { px(999.0) })
                .border_2()
                .border_color(node_color.opacity(if is_dark { 1.0 } else { 0.92 }))
                .bg(node_color),
        );

        if let Some(marker) = node_marker {
            gutter = gutter
                .child(
                    div()
                        .absolute()
                        .left(px(node_lane_x - (node_size * 0.5)))
                        .top(px(node_center_y - (node_size * 0.5)))
                        .size(px(node_size))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .font_semibold()
                        .text_color(cx.theme().background)
                        .child(marker),
                );
        }

        gutter.into_any_element()
    }

    fn tree_lane_gutter_width(&self, lane_count: usize) -> f32 {
        lane_count.saturating_mul(14).saturating_add(2) as f32
    }

    fn graph_selected_lane_hint(&self) -> usize {
        let active_lane = self
            .graph_nodes
            .iter()
            .find(|node| node.is_active_bookmark_target)
            .and_then(|node| {
                self.graph_lane_rows
                    .iter()
                    .find(|row| row.node_id == node.id)
                    .map(|row| row.node_lane)
            });
        if let Some(active_lane) = active_lane {
            return active_lane;
        }

        self.graph_nodes
            .iter()
            .find(|node| node.is_working_copy_parent)
            .and_then(|node| {
                self.graph_lane_rows
                    .iter()
                    .find(|row| row.node_id == node.id)
                    .map(|row| row.node_lane)
            })
            .unwrap_or(0)
    }

    fn graph_max_lane_count(&self) -> usize {
        self.graph_lane_rows
            .iter()
            .map(|row| row.lane_count)
            .max()
            .unwrap_or(1)
            .max(1)
    }

    fn graph_lane_window(&self, lane_count: usize, node_lane: usize) -> (usize, usize) {
        let lane_count = lane_count.max(1);
        let max_visible = 12usize;
        if lane_count <= max_visible {
            return (0, lane_count);
        }

        let half = max_visible / 2;
        let mut start = node_lane.saturating_sub(half);
        let max_start = lane_count.saturating_sub(max_visible);
        if start > max_start {
            start = max_start;
        }

        (start, start.saturating_add(max_visible).min(lane_count))
    }

    fn graph_lane_window_for_row(
        &self,
        lane_count: usize,
        preferred_lane_start: usize,
        node_lane: usize,
    ) -> (usize, usize) {
        let lane_count = lane_count.max(1);
        let max_visible = 12usize;
        if lane_count <= max_visible {
            return (0, lane_count);
        }

        let max_start = lane_count.saturating_sub(max_visible);
        let mut start = preferred_lane_start.min(max_start);
        if node_lane < start {
            start = node_lane;
        } else if node_lane >= start.saturating_add(max_visible) {
            start = node_lane
                .saturating_add(1)
                .saturating_sub(max_visible)
                .min(max_start);
        }

        (start, start.saturating_add(max_visible).min(lane_count))
    }

    fn graph_working_copy_color(&self, is_dark: bool) -> gpui::Hsla {
        if is_dark {
            gpui::rgb(0xfbbf24).into()
        } else {
            gpui::rgb(0xb45309).into()
        }
    }

    fn graph_active_target_color(&self, is_dark: bool) -> gpui::Hsla {
        if is_dark {
            gpui::rgb(0x4ade80).into()
        } else {
            gpui::rgb(0x047857).into()
        }
    }

    fn graph_merge_color(&self, is_dark: bool) -> gpui::Hsla {
        if is_dark {
            gpui::rgb(0x60a5fa).into()
        } else {
            gpui::rgb(0x1d4ed8).into()
        }
    }

    fn graph_bookmark_base_color(
        &self,
        bookmark: &GraphBookmarkRef,
        is_dark: bool,
    ) -> gpui::Hsla {
        if bookmark.scope == GraphBookmarkScope::Remote {
            if is_dark {
                gpui::rgb(0xa78bfa).into()
            } else {
                gpui::rgb(0x6d28d9).into()
            }
        } else if bookmark.conflicted {
            if is_dark {
                gpui::rgb(0xf87171).into()
            } else {
                gpui::rgb(0xb91c1c).into()
            }
        } else if bookmark.needs_push {
            self.graph_working_copy_color(is_dark)
        } else if bookmark.tracked {
            self.graph_active_target_color(is_dark)
        } else {
            self.graph_merge_color(is_dark)
        }
    }

    fn graph_bookmark_chip_colors(
        &self,
        bookmark: &GraphBookmarkRef,
        is_dark: bool,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> (gpui::Hsla, gpui::Hsla, gpui::Hsla) {
        let base = self.graph_bookmark_base_color(bookmark, is_dark);
        let background = if selected {
            base.opacity(if is_dark { 0.44 } else { 0.22 })
        } else {
            base.opacity(if is_dark { 0.28 } else { 0.12 })
        };
        let border = if selected {
            base.opacity(if is_dark { 0.98 } else { 0.76 })
        } else {
            base.opacity(if is_dark { 0.72 } else { 0.48 })
        };
        let text = cx.theme().foreground;
        (background, border, text)
    }

    fn render_jj_graph_bookmark_chip(
        &self,
        node_id: &str,
        row_ix: usize,
        bookmark_ix: usize,
        bookmark: &GraphBookmarkRef,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let node_id = node_id.to_string();
        let name = bookmark.name.clone();
        let remote = bookmark.remote.clone();
        let scope = bookmark.scope;
        let activate_node_id = node_id.clone();
        let activate_name = bookmark.name.clone();
        let activate_remote = bookmark.remote.clone();
        let activate_scope = bookmark.scope;
        let selected = self.graph_selected_bookmark.as_ref().is_some_and(|selected| {
            selected.name == bookmark.name
                && selected.remote == bookmark.remote
                && selected.scope == bookmark.scope
        });

        let status_token = match bookmark.scope {
            GraphBookmarkScope::Local if bookmark.conflicted => "conflict",
            GraphBookmarkScope::Local if bookmark.tracked && bookmark.needs_push => "ahead",
            GraphBookmarkScope::Local if bookmark.tracked => "synced",
            GraphBookmarkScope::Local => "local",
            GraphBookmarkScope::Remote if bookmark.conflicted && bookmark.tracked => "track-conflict",
            GraphBookmarkScope::Remote if bookmark.conflicted => "conflict",
            GraphBookmarkScope::Remote if bookmark.tracked => "tracked",
            GraphBookmarkScope::Remote => "remote",
        };

        let mut label = match bookmark.scope {
            GraphBookmarkScope::Local => format!("L {} [{status_token}]", bookmark.name),
            GraphBookmarkScope::Remote => format!(
                "R {}@{} [{status_token}]",
                bookmark.name,
                bookmark.remote.as_deref().unwrap_or("remote")
            ),
        };
        if bookmark.is_active {
            label = format!("* {label}");
        }

        let tooltip = match (bookmark.scope, bookmark.tracked, bookmark.conflicted) {
            (GraphBookmarkScope::Local, _, true) => "Local bookmark (conflicted)".to_string(),
            (GraphBookmarkScope::Local, true, false) => "Local bookmark (published)".to_string(),
            (GraphBookmarkScope::Local, false, false) => "Local bookmark (not published)".to_string(),
            (GraphBookmarkScope::Remote, true, true) => {
                "Remote bookmark (tracked, conflicted)".to_string()
            }
            (GraphBookmarkScope::Remote, true, false) => "Remote bookmark (tracked)".to_string(),
            (GraphBookmarkScope::Remote, false, true) => {
                "Remote bookmark (untracked, conflicted)".to_string()
            }
            (GraphBookmarkScope::Remote, false, false) => "Remote bookmark (untracked)".to_string(),
        };

        let button_id = row_ix.saturating_mul(1_024).saturating_add(bookmark_ix);
        let mut button = Button::new(("jj-graph-bookmark-chip", button_id))
            .compact()
            .with_size(gpui_component::Size::Small)
            .rounded(px(6.0))
            .label(label)
            .tooltip(tooltip)
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                view.update(cx, |this, cx| {
                    this.select_graph_bookmark(
                        node_id.clone(),
                        name.clone(),
                        remote.clone(),
                        scope,
                        cx,
                    );
                });
            });

        let (chip_bg, chip_border, chip_text) =
            self.graph_bookmark_chip_colors(bookmark, is_dark, selected, cx);
        button = button
            .outline()
            .bg(chip_bg)
            .border_color(chip_border)
            .text_color(chip_text);

        let chip_wrapper = |child: AnyElement| {
            let view = cx.entity();
            div()
                .on_mouse_down(MouseButton::Left, {
                    let activate_node_id = activate_node_id.clone();
                    let activate_name = activate_name.clone();
                    let activate_remote = activate_remote.clone();
                    move |event, _, cx| {
                        cx.stop_propagation();
                        view.update(cx, |this, cx| {
                            if event.click_count >= 2 {
                                this.activate_graph_bookmark(
                                    activate_node_id.clone(),
                                    activate_name.clone(),
                                    activate_remote.clone(),
                                    activate_scope,
                                    cx,
                                );
                            }
                        });
                    }
                })
                .child(child)
                .into_any_element()
        };

        if selected && bookmark.scope == GraphBookmarkScope::Local {
            let view = cx.entity();
            return h_flex()
                .items_center()
                .gap_1()
                .child(chip_wrapper(button.into_any_element()))
                .child(
                    Input::new(&self.graph_action_input_state)
                        .h(px(22.0))
                        .w(px(164.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
                        .bg(cx.theme().background.opacity(if is_dark { 0.30 } else { 0.18 }))
                        .disabled(self.git_action_loading),
                )
                .child(
                    Button::new(("jj-graph-bookmark-inline-rename", button_id))
                        .outline()
                        .compact()
                        .with_size(gpui_component::Size::Small)
                        .rounded(px(6.0))
                        .label("Rename")
                        .tooltip("Rename the selected local bookmark.")
                        .disabled(self.git_action_loading)
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            view.update(cx, |this, cx| {
                                this.rename_selected_graph_bookmark_from_input(cx);
                            });
                        }),
                )
                .into_any_element();
        }

        chip_wrapper(button.into_any_element())
    }

    fn reduced_motion_enabled(&self) -> bool {
        self.config.reduce_motion
    }

    fn animation_duration_ms(&self, millis: u64) -> Duration {
        if self.reduced_motion_enabled() {
            Duration::from_millis(1)
        } else {
            Duration::from_millis(millis)
        }
    }

    fn render_jj_terms_glossary_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        v_flex()
            .w_full()
            .gap_0p5()
            .px_2()
            .py_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
            .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                0.22
            } else {
                0.30
            })))
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child("JJ Terms"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child("Working copy (`@`): your mutable local changes."),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child("Revision: an immutable committed node in the graph."),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child("Bookmark: a movable pointer to a revision."),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child("Publish: create remote tracking for a local bookmark."),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child("Sync: fetch remote bookmark updates into local history."),
            )
            .into_any_element()
    }
}
