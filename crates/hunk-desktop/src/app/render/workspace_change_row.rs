impl DiffViewer {
    fn render_workspace_change_row(
        &self,
        row_ix: usize,
        file: &ChangedFile,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let staged_for_commit = self.staged_commit_files.contains(file.path.as_str());
        let is_selected = self.selected_path.as_deref() == Some(file.path.as_str());
        let is_dark = cx.theme().mode.is_dark();
        let card_surface = hunk_card_surface(cx.theme(), is_dark);
        let undo_loading = self.git_action_loading_named("Undo file changes");
        let (status_label, status_color) = change_status_label_color(file.status, cx);
        let is_tracked = file.is_tracked();
        let tracking_label = if is_tracked { "tracked" } else { "untracked" };
        let tracking_tone = if is_tracked {
            HunkAccentTone::Neutral
        } else {
            HunkAccentTone::Warning
        };
        let tracking_colors = hunk_tinted_button(cx.theme(), is_dark, tracking_tone);
        let row_background = if is_selected {
            hunk_blend(card_surface.background, status_color, is_dark, 0.18, 0.10)
        } else {
            card_surface.background
        };
        let row_border = if is_selected {
            hunk_opacity(status_color, is_dark, 0.54, 0.34)
        } else {
            card_surface.border
        };
        let status_badge_background = hunk_opacity(status_color, is_dark, 0.18, 0.10);
        let status_badge_border = hunk_opacity(status_color, is_dark, 0.62, 0.38);
        let accent_strip = hunk_tone(status_color, is_dark, 0.18, 0.10);
        let undo_tooltip = if is_tracked {
            "Restore this file to HEAD."
        } else {
            "Delete this untracked file from the working tree."
        };
        let stage_colors = if staged_for_commit {
            hunk_tinted_button(cx.theme(), is_dark, HunkAccentTone::Success)
        } else {
            hunk_tinted_button(cx.theme(), is_dark, HunkAccentTone::Neutral)
        };
        let line_stats = self.file_line_stats.get(file.path.as_str()).copied().unwrap_or_default();
        let path = file.path.clone();

        v_flex()
            .id(("workspace-change-row", row_ix))
            .relative()
            .overflow_hidden()
            .flex_none()
            .w_full()
            .gap_1p5()
            .px_3()
            .py_2p5()
            .rounded(px(10.0))
            .border_1()
            .border_color(row_border)
            .bg(row_background)
            .child({
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(status_badge_border)
                            .bg(status_badge_background)
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(status_label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().foreground)
                            .child(path.clone()),
                    )
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded(px(999.0))
                                    .border_1()
                                    .border_color(tracking_colors.border)
                                    .bg(tracking_colors.background)
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(tracking_colors.text)
                                    .child(tracking_label),
                            )
                            .when(line_stats.changed() > 0, |this| {
                                this.child(self.render_line_stats("lines", line_stats, cx))
                            }),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .flex_wrap()
                            .child({
                                let view = view.clone();
                                let path = path.clone();
                                let staged = staged_for_commit;
                                let mut button =
                                    Button::new(("workspace-commit-stage-toggle", row_ix))
                                        .compact()
                                        .rounded(px(7.0))
                                        .min_w(px(82.0))
                                        .label(if staged { "Unstage" } else { "Stage" })
                                        .tooltip(if staged {
                                            "Remove this file from the next commit."
                                        } else {
                                            "Stage this file for the next commit."
                                        })
                                        .on_click(move |_, _, cx| {
                                            cx.stop_propagation();
                                            view.update(cx, |this, cx| {
                                                this.toggle_commit_file_staged(
                                                    path.clone(),
                                                    !staged,
                                                    cx,
                                                );
                                            });
                                        });
                                if staged {
                                    button = button
                                        .primary()
                                        .bg(stage_colors.background)
                                        .border_color(stage_colors.border)
                                        .text_color(stage_colors.text);
                                } else {
                                    button = button
                                        .outline()
                                        .bg(stage_colors.background)
                                        .border_color(stage_colors.border)
                                        .text_color(stage_colors.text);
                                }
                                button
                            })
                            .child({
                                let view = view.clone();
                                let path = path.clone();
                                Button::new(("workspace-change-undo", row_ix))
                                    .outline()
                                    .compact()
                                    .rounded(px(7.0))
                                    .loading(undo_loading)
                                    .label("Undo")
                                    .tooltip(undo_tooltip)
                                    .disabled(self.git_action_loading)
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        view.update(cx, |this, cx| {
                                            this.undo_working_copy_file(
                                                path.clone(),
                                                is_tracked,
                                                cx,
                                            );
                                        });
                                    })
                            }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(3.0))
                    .bg(accent_strip),
            )
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| {
                    this.select_file(path.clone(), cx);
                });
            })
            .into_any_element()
    }
}
