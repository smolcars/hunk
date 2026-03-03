impl DiffViewer {
    fn render_jj_graph_focus_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let focused_bookmark = self.graph_selected_bookmark_ref();
        let focused_nodes = self.graph_focused_revision_nodes();
        let focused_revision_ix = self.graph_focused_revision_position().unwrap_or(0);
        let has_focused_revisions = !focused_nodes.is_empty();
        let focus_strip_expanded = focused_bookmark.is_some();
        let focus_transition_key = focused_bookmark
            .map(|bookmark| match bookmark.scope {
                GraphBookmarkScope::Local => format!("local:{}", bookmark.name),
                GraphBookmarkScope::Remote => format!(
                    "remote:{}@{}",
                    bookmark.name,
                    bookmark.remote.as_deref().unwrap_or("remote")
                ),
            })
            .unwrap_or_else(|| "none".to_string());
        let focus_transition_id = focus_transition_stable_id(focus_transition_key.as_str());
        let can_move_to_newer = has_focused_revisions && focused_revision_ix > 0;
        let can_move_to_older =
            has_focused_revisions && focused_revision_ix.saturating_add(1) < focused_nodes.len();

        v_flex()
            .w_full()
            .gap_1()
            .p_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
            .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                0.18
            } else {
                0.28
            })))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Bookmark Focus Strip"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} revisions", focused_nodes.len())),
                    ),
            )
            .child({
                let focus_message = if let Some(bookmark) = focused_bookmark {
                    match bookmark.scope {
                        GraphBookmarkScope::Local => format!("Focused: {}", bookmark.name),
                        GraphBookmarkScope::Remote => format!(
                            "Focused: {}@{}",
                            bookmark.name,
                            bookmark.remote.as_deref().unwrap_or("remote")
                        ),
                    }
                } else {
                    "Select a bookmark chip in the graph to focus its revision chain.".to_string()
                };
                let focused_label = div()
                    .text_xs()
                    .text_color(if focused_bookmark.is_some() {
                        cx.theme().foreground
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(focus_message);
                if !self.reduced_motion_enabled() {
                    focused_label
                        .with_animation(
                            ("jj-graph-focus-label-transition", focus_transition_id),
                            Animation::new(self.animation_duration_ms(180))
                                .with_easing(cubic_bezier(0.32, 0.72, 0.0, 1.0)),
                            |this, delta| {
                                let settling = 1.0 - delta;
                                this.opacity(0.84 + (0.16 * delta))
                                .top(px(settling * 3.0))
                        },
                    )
                        .into_any_element()
                } else {
                    focused_label.into_any_element()
                }
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .child({
                        let view = view.clone();
                        Button::new("jj-graph-focus-newer")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Newer")
                            .tooltip("Previous focused revision")
                            .disabled(focused_bookmark.is_none() || !can_move_to_newer)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.navigate_focused_graph_revision(-1, cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        Button::new("jj-graph-focus-older")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Older")
                            .tooltip("Next focused revision")
                            .disabled(focused_bookmark.is_none() || !can_move_to_older)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.navigate_focused_graph_revision(1, cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        Button::new("jj-graph-focus-clear")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Return to Full Graph")
                            .tooltip("Clear bookmark focus and show the full graph context.")
                            .disabled(focused_bookmark.is_none())
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.clear_graph_bookmark_selection(cx);
                                });
                            })
                    }),
            )
            .child({
                let body_content = if !focus_strip_expanded {
                    div().into_any_element()
                } else if focused_nodes.is_empty() {
                    div()
                        .w_full()
                        .px_1()
                        .py_1()
                        .rounded(px(6.0))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No focused revisions visible in this graph window.")
                        .into_any_element()
                } else {
                    v_flex()
                        .id("jj-graph-focus-strip-scroll")
                        .w_full()
                        .max_h(px(220.0))
                        .overflow_y_scroll()
                        .occlude()
                        .gap_0p5()
                        .children(focused_nodes.iter().enumerate().map(|(ix, node)| {
                            let is_selected = self.graph_node_is_selected(node.id.as_str());
                            let row_bg = if is_selected {
                                cx.theme().accent.opacity(if is_dark { 0.20 } else { 0.13 })
                            } else {
                                cx.theme().background.opacity(0.0)
                            };
                            let short_id = node.id.chars().take(12).collect::<String>();
                            let node_id = node.id.clone();
                            let view = view.clone();

                            h_flex()
                                .id(("jj-graph-focus-row", ix))
                                .w_full()
                                .items_center()
                                .gap_1()
                                .px_1()
                                .py_0p5()
                                .rounded(px(6.0))
                                .bg(row_bg)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.select_graph_focus_revision(node_id.clone(), cx);
                                    });
                                })
                                .child(
                                    div()
                                        .px_1()
                                        .py_0p5()
                                        .rounded(px(4.0))
                                        .text_xs()
                                        .font_family(cx.theme().mono_font_family.clone())
                                        .text_color(cx.theme().muted_foreground)
                                        .bg(cx.theme().muted.opacity(if is_dark { 0.32 } else { 0.42 }))
                                        .child(short_id),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_xs()
                                        .text_color(cx.theme().foreground)
                                        .child(node.subject.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .whitespace_nowrap()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(relative_time_label(Some(node.unix_time))),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(if ix == 0 { "tip" } else { "" }),
                                )
                                .into_any_element()
                        }))
                        .into_any_element()
                };

                if !self.reduced_motion_enabled() {
                    div()
                        .w_full()
                        .overflow_hidden()
                        .child(body_content)
                        .with_animation(
                            (
                                "jj-graph-focus-strip-expand",
                                u64::from(focus_strip_expanded),
                            ),
                            Animation::new(self.animation_duration_ms(220))
                                .with_easing(cubic_bezier(0.32, 0.72, 0.0, 1.0)),
                            move |this, delta| {
                                let progress = if focus_strip_expanded { delta } else { 1.0 - delta };
                                this.max_h(px(220.0 * progress)).opacity(0.2 + (0.8 * progress))
                            },
                        )
                        .into_any_element()
                } else {
                    div()
                        .w_full()
                        .overflow_hidden()
                        .child(body_content)
                        .max_h(px(if focus_strip_expanded { 220.0 } else { 0.0 }))
                        .opacity(if focus_strip_expanded { 1.0 } else { 0.0 })
                        .into_any_element()
                }
            })
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.graph_has_more {
                        "Focused revisions are limited to the loaded graph window."
                    } else {
                        "Focused revisions include all reachable nodes in the loaded graph."
                    }),
            )
            .into_any_element()
    }
}

fn focus_transition_stable_id(value: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(value, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}
