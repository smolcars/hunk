impl DiffViewer {
    fn render_git_action_status_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        let loading = self.git_action_loading;
        let headline = if loading {
            match self.git_action_label.as_deref() {
                Some(label) => format!("{label}..."),
                None => "Running workspace action...".to_string(),
            }
        } else {
            self.git_status_message
                .clone()
                .unwrap_or_else(|| "Ready.".to_string())
        };
        let detail = if loading {
            self.git_status_message.clone()
        } else {
            self.git_action_label.clone()
        };
        let detail_text = detail.unwrap_or_else(|| {
            "Actions update this banner when operations complete.".to_string()
        });

        v_flex()
            .w_full()
            .h(px(52.0))
            .overflow_hidden()
            .px_2()
            .py_1()
            .gap_0p5()
            .rounded(px(8.0))
            .border_1()
            .border_color(if loading {
                cx.theme().accent.opacity(if is_dark { 0.90 } else { 0.72 })
            } else {
                cx.theme().border.opacity(if is_dark { 0.90 } else { 0.70 })
            })
            .bg(if loading {
                cx.theme().accent.opacity(if is_dark { 0.22 } else { 0.12 })
            } else {
                cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                    0.24
                } else {
                    0.32
                }))
            })
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_xs()
                    .font_medium()
                    .text_color(if loading {
                        cx.theme().foreground
                    } else {
                        cx.theme().muted_foreground
                    })
                    .whitespace_nowrap()
                    .truncate()
                    .child(headline),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.9))
                    .whitespace_nowrap()
                    .truncate()
                    .child(detail_text),
            )
            .into_any_element()
    }

    fn render_jj_graph_operations_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_jj_graph_operations_panel_v2(cx)
    }

    fn render_revision_stack_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let revisions = &self.bookmark_revisions;
        let actions_enabled = self.can_run_active_bookmark_actions();
        let can_abandon_tip = !self.git_action_loading && actions_enabled && !revisions.is_empty();
        let can_squash_tip = !self.git_action_loading && actions_enabled && revisions.len() >= 2;
        let can_reorder_tip = !self.git_action_loading && actions_enabled && revisions.len() >= 2;
        let can_undo_all_working_copy = !self.git_action_loading && !self.files.is_empty();
        let can_undo_operation = !self.git_action_loading && self.can_undo_operation;
        let can_redo_operation = !self.git_action_loading && self.can_redo_operation;

        v_flex()
            .w_full()
            .gap_1()
            .p_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
            .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                0.20
            } else {
                0.26
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
                            .child("Revision Stack"),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{}", revisions.len())),
                            )
                            .child({
                                let view = view.clone();
                                Button::new("toggle-revision-stack")
                                    .outline()
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(7.0))
                                    .label(if self.revision_stack_collapsed {
                                        "Expand"
                                    } else {
                                        "Collapse"
                                    })
                                    .tooltip(if self.revision_stack_collapsed {
                                        "Show revision stack and advanced revision actions."
                                    } else {
                                        "Hide revision stack and advanced revision actions."
                                    })
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.toggle_revision_stack_collapsed(cx);
                                        });
                                    })
                            }),
                    ),
            )
            .when(self.revision_stack_collapsed, |this| {
                this.child(
                    div()
                        .w_full()
                        .px_1()
                        .py_0p5()
                        .rounded(px(6.0))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Collapsed by default. Expand to access revision stack actions."),
                )
            })
            .when(!self.revision_stack_collapsed, |this| {
                this.child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_1()
                        .flex_wrap()
                        .child({
                            let view = view.clone();
                            Button::new("reorder-top-two-revisions")
                                .outline()
                                .compact()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(7.0))
                                .label("Move Tip Down")
                                .tooltip("Reorder the stack so the current tip becomes second and the previous revision becomes tip.")
                                .disabled(!can_reorder_tip)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.reorder_current_bookmark_tip_older(cx);
                                    });
                                })
                        })
                        .child({
                            let view = view.clone();
                            Button::new("squash-tip-revision")
                                .outline()
                                .compact()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(7.0))
                                .label("Squash Into Parent")
                                .tooltip("Combine tip revision changes into its parent revision.")
                                .disabled(!can_squash_tip)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.squash_current_bookmark_tip_into_parent(cx);
                                    });
                                })
                        })
                        .child({
                            let view = view.clone();
                            Button::new("abandon-tip-revision")
                                .outline()
                                .compact()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(7.0))
                                .label("Drop Tip Revision")
                                .tooltip("Abandon and remove the current tip revision from the stack.")
                                .disabled(!can_abandon_tip)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.abandon_current_bookmark_tip(cx);
                                    });
                                })
                        })
                        .child({
                            let view = view.clone();
                            Button::new("undo-last-operation")
                                .outline()
                                .compact()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(7.0))
                                .label("Undo Op")
                                .tooltip("Undo the latest JJ operation in operation history.")
                                .bg(cx.theme().warning.opacity(if is_dark { 0.24 } else { 0.14 }))
                                .border_color(cx.theme().warning.opacity(if is_dark { 0.82 } else { 0.60 }))
                                .text_color(cx.theme().foreground)
                                .disabled(!can_undo_operation)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.undo_last_operation(cx);
                                    });
                                })
                        })
                        .child({
                            let view = view.clone();
                            Button::new("redo-last-operation")
                                .outline()
                                .compact()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(7.0))
                                .label("Redo Op")
                                .tooltip("Redo the most recently undone JJ operation.")
                                .bg(cx.theme().accent.opacity(if is_dark { 0.28 } else { 0.16 }))
                                .border_color(
                                    cx.theme().accent.opacity(if is_dark { 0.78 } else { 0.58 }),
                                )
                                .text_color(cx.theme().foreground)
                                .disabled(!can_redo_operation)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.redo_last_operation(cx);
                                    });
                                })
                        })
                        .child({
                            let view = view.clone();
                            Button::new("undo-all-working-copy-changes")
                                .outline()
                                .compact()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(7.0))
                                .label("Undo All")
                                .tooltip("Discard all working-copy changes using jj restore.")
                                .bg(cx.theme().danger.opacity(if is_dark { 0.24 } else { 0.14 }))
                                .border_color(cx.theme().danger.opacity(if is_dark { 0.82 } else { 0.60 }))
                                .text_color(cx.theme().foreground)
                                .disabled(!can_undo_all_working_copy)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.undo_all_working_copy_changes(cx);
                                    });
                                })
                        }),
                )
                .child({
                    if revisions.is_empty() {
                        div()
                            .w_full()
                            .px_1()
                            .py_0p5()
                            .rounded(px(6.0))
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("No revisions for this bookmark.")
                            .into_any_element()
                    } else {
                        v_flex()
                            .id("jj-revision-stack-scroll")
                            .w_full()
                            .max_h(px(180.0))
                            .overflow_y_scroll()
                            .occlude()
                            .gap_0p5()
                            .children(revisions.iter().enumerate().map(|(ix, revision)| {
                                let short_id = revision.id.chars().take(12).collect::<String>();
                                let row_bg = if ix == 0 {
                                    cx.theme().accent.opacity(if is_dark { 0.18 } else { 0.10 })
                                } else {
                                    cx.theme().background.opacity(0.0)
                                };

                                h_flex()
                                    .w_full()
                                    .items_center()
                                    .gap_1()
                                    .px_1()
                                    .py_0p5()
                                    .rounded(px(6.0))
                                    .bg(row_bg)
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
                                            .child(revision.subject.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .whitespace_nowrap()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(relative_time_label(Some(revision.unix_time))),
                                    )
                                    .into_any_element()
                            }))
                            .into_any_element()
                    }
                })
            })
            .into_any_element()
    }

    fn render_workspace_changes_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tracked_count = self.files.iter().filter(|file| file.is_tracked()).count();
        let untracked_count = self.files.len().saturating_sub(tracked_count);
        let is_dark = cx.theme().mode.is_dark();

        v_flex()
            .w_full()
            .gap_1()
            .p_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
            .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                0.20
            } else {
                0.26
            })))
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Working Copy"),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .flex_wrap()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} files (tracked: {}, untracked: {})",
                                self.files.len(),
                                tracked_count,
                                untracked_count
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground.opacity(0.9))
                            .child("Single unified working-copy list"),
                    ),
            )
            .child({
                let list_container = if self.files.is_empty() {
                    v_flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No tracked or untracked changes."),
                        )
                        .into_any_element()
                } else {
                    v_flex()
                        .id("jj-working-copy-scroll")
                        .size_full()
                        .overflow_y_scroll()
                        .occlude()
                        .gap_0p5()
                        .children(self.files.iter().enumerate().map(|(row_ix, file)| {
                            self.render_workspace_change_row(row_ix, file, cx)
                        }))
                        .into_any_element()
                };

                div()
                    .w_full()
                    .h(px(220.0))
                    .min_h(px(220.0))
                    .max_h(px(220.0))
                    .p_1()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.88 } else { 0.74 }))
                    .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                        0.12
                    } else {
                        0.18
                    })))
                    .child(list_container)
                    .into_any_element()
            })
            .into_any_element()
    }

}
