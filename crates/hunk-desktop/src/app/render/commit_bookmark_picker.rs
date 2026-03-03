impl DiffViewer {
    fn render_workspace_change_row(
        &self,
        row_id: usize,
        file: &ChangedFile,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let included_in_commit = !self.commit_excluded_files.contains(file.path.as_str());
        let is_selected = self.selected_path.as_deref() == Some(file.path.as_str());
        let is_dark = cx.theme().mode.is_dark();
        let (status_label, status_color) = change_status_label_color(file.status, cx);
        let is_tracked = file.is_tracked();
        let tracking_label = if is_tracked { "tracked" } else { "untracked" };
        let tracking_color = if is_tracked {
            cx.theme().secondary.opacity(if is_dark { 0.36 } else { 0.56 })
        } else {
            cx.theme().warning.opacity(if is_dark { 0.30 } else { 0.20 })
        };
        let undo_tooltip = if is_tracked {
            "Restore this file to the parent revision."
        } else {
            "Delete this untracked file from the working copy."
        };
        let row_bg = if is_selected {
            cx.theme().accent.opacity(if is_dark { 0.22 } else { 0.14 })
        } else {
            cx.theme().background.opacity(0.0)
        };
        let path = file.path.clone();

        h_flex()
            .id(("workspace-change-row", row_id))
            .w_full()
            .items_center()
            .gap_1()
            .px_1()
            .py_0p5()
            .rounded(px(6.0))
            .bg(row_bg)
            .child({
                let view = view.clone();
                let path = path.clone();
                let include = included_in_commit;
                Button::new(("workspace-commit-include-toggle", row_id))
                    .outline()
                    .compact()
                    .rounded(px(5.0))
                    .min_w(px(22.0))
                    .h(px(20.0))
                    .label(if include { "x" } else { "" })
                    .tooltip(if include {
                        "Included in next revision"
                    } else {
                        "Excluded from next revision"
                    })
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        view.update(cx, |this, cx| {
                            this.toggle_commit_file_included(path.clone(), !include, cx);
                        });
                    })
            })
            .child(
                div()
                    .px_1()
                    .py_0p5()
                    .rounded(px(4.0))
                    .text_xs()
                    .font_semibold()
                    .bg(status_color.opacity(if is_dark { 0.24 } else { 0.16 }))
                    .text_color(cx.theme().foreground)
                    .child(status_label),
            )
            .child(
                div()
                    .px_1()
                    .py_0p5()
                    .rounded(px(4.0))
                    .text_xs()
                    .font_semibold()
                    .bg(tracking_color)
                    .text_color(cx.theme().foreground)
                    .child(tracking_label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().foreground)
                    .child(path.clone()),
            )
            .child({
                let view = view.clone();
                let path = path.clone();
                Button::new(("workspace-change-undo", row_id))
                    .outline()
                    .compact()
                    .rounded(px(5.0))
                    .label("Undo")
                    .tooltip(undo_tooltip)
                    .disabled(self.git_action_loading)
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        view.update(cx, |this, cx| {
                            this.undo_working_copy_file(path.clone(), is_tracked, cx);
                        });
                    })
            })
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| {
                    this.select_file(path.clone(), cx);
                });
            })
            .into_any_element()
    }

    fn render_branch_picker_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let bookmark_input_empty = self.branch_input_state.read(cx).value().trim().is_empty();
        let rename_disabled =
            self.git_action_loading || bookmark_input_empty || !self.can_run_active_bookmark_actions();
        let create_or_activate_disabled = self.git_action_loading || bookmark_input_empty;

        v_flex()
            .w_full()
            .gap_1()
            .p_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.94 } else { 0.74 }))
            .bg(cx.theme().background.blend(cx.theme().secondary.opacity(if is_dark {
                0.32
            } else {
                0.20
            })))
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Bookmarks"),
            )
            .child(
                div()
                    .id("jj-bookmark-picker-scroll")
                    .max_h(px(144.0))
                    .overflow_y_scroll()
                    .occlude()
                    .child(
                        v_flex().w_full().gap_1().children(
                            self.branches
                                .iter()
                                .enumerate()
                                .map(|(ix, branch)| {
                                    let view = view.clone();
                                    let branch_name = branch.name.clone();
                                    let activate_view = view.clone();
                                    let activate_branch_name = branch_name.clone();
                                    let move_disabled =
                                        self.git_action_loading || self.files.is_empty() || branch.is_current;

                                    h_flex()
                                        .id(("branch-row", ix))
                                        .w_full()
                                        .min_w_0()
                                        .items_center()
                                        .gap_1()
                                        .px_2()
                                        .py_0p5()
                                        .rounded(px(6.0))
                                        .bg(if branch.is_current {
                                            cx.theme().accent.opacity(if is_dark { 0.28 } else { 0.18 })
                                        } else {
                                            cx.theme().background.opacity(0.0)
                                        })
                                        .on_click(move |_, window, cx| {
                                            activate_view.update(cx, |this, cx| {
                                                this.checkout_bookmark(
                                                    activate_branch_name.clone(),
                                                    window,
                                                    cx,
                                                );
                                            });
                                        })
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .text_xs()
                                                .font_medium()
                                                .text_color(cx.theme().foreground)
                                                .child(branch.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .pl_2()
                                                .whitespace_nowrap()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(relative_time_label(branch.tip_unix_time)),
                                        )
                                        .child({
                                            let move_view = view.clone();
                                            let move_branch_name = branch_name.clone();
                                            Button::new(("bookmark-row-move", ix))
                                                .outline()
                                                .compact()
                                                .rounded(px(6.0))
                                                .label("Move")
                                                .disabled(move_disabled)
                                                .tooltip("Switch to this bookmark and carry current working-copy changes.")
                                                .on_click(move |_, _, cx| {
                                                    cx.stop_propagation();
                                                    move_view.update(cx, |this, cx| {
                                                        this.checkout_bookmark_with_change_transfer(
                                                            move_branch_name.clone(),
                                                            cx,
                                                        );
                                                    });
                                                })
                                        })
                                        .into_any_element()
                                }),
                        ),
                    ),
            )
            .child(
                Input::new(&self.branch_input_state)
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.92 } else { 0.76 }))
                    .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                        0.22
                    } else {
                        0.14
                    })))
                    .disabled(self.git_action_loading),
            )
            .child({
                let view = view.clone();
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .flex_wrap()
                    .child(
                        Button::new("create-or-switch-bookmark")
                            .primary()
                            .rounded(px(7.0))
                            .label("Create / Activate")
                            .tooltip("Create a bookmark from the entered name or activate it if it already exists.")
                            .disabled(create_or_activate_disabled)
                            .on_click({
                                let view = view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.create_or_switch_bookmark_from_input(window, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new("rename-active-bookmark")
                            .outline()
                            .rounded(px(7.0))
                            .label("Rename Active")
                            .tooltip("Rename the currently active bookmark to the entered name.")
                            .disabled(rename_disabled)
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.rename_current_bookmark_from_input(window, cx);
                                });
                            }),
                    )
            })
            .into_any_element()
    }
}
