impl DiffViewer {
    fn render_workspace_change_row(
        &self,
        row_ix: usize,
        file: &ChangedFile,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let staged_for_commit = self.staged_commit_files.contains(file.path.as_str());
        let is_dark = cx.theme().mode.is_dark();
        let card_surface = hunk_card_surface(cx.theme(), is_dark);
        let undo_loading = self.git_action_loading_named("Undo file changes");
        let (status_label, status_color) = change_status_label_color(file.status, cx);
        let is_tracked = file.is_tracked();
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
            HunkButtonColors {
                background: hunk_blend(cx.theme().background, cx.theme().primary, is_dark, 0.12, 0.04),
                border: hunk_opacity(cx.theme().primary, is_dark, 0.90, 0.64),
                text: hunk_tone(cx.theme().primary, is_dark, 0.22, 0.40),
            }
        };
        let line_stats = self
            .git_workspace
            .file_line_stats
            .get(file.path.as_str())
            .copied()
            .unwrap_or_default();
        let path = file.path.clone();

        h_flex()
            .id(("workspace-change-row", row_ix))
            .relative()
            .overflow_hidden()
            .flex_none()
            .w_full()
            .items_center()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(px(9.0))
            .border_1()
            .border_color(card_surface.border)
            .bg(card_surface.background)
            .child({
                let view = view.clone();
                let path = path.clone();
                let mut button = Button::new(("workspace-commit-stage-toggle", row_ix))
                    .compact()
                    .rounded(px(6.0))
                    .min_w(px(22.0))
                    .h(px(22.0))
                    .bg(stage_colors.background)
                    .border_color(stage_colors.border)
                    .text_color(stage_colors.text)
                    .tooltip(if staged_for_commit {
                        "Remove this file from the next commit."
                    } else {
                        "Stage this file for the next commit."
                    })
                    .disabled(self.git_controls_busy())
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.toggle_commit_file_staged(path.clone(), !staged_for_commit, cx);
                        });
                    });
                if staged_for_commit {
                    button = button
                        .primary()
                        .icon(Icon::new(IconName::Check).size(px(12.0)));
                } else {
                    button = button.outline();
                }
                button
            })
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
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().foreground)
                            .child(path.clone()),
                    )
                    .when(line_stats.changed() > 0, |this| {
                        this.child(self.render_workspace_change_stats(line_stats, cx))
                    }),
            )
            .child({
                let view = view.clone();
                let path = path.clone();
                Button::new(("workspace-change-undo", row_ix))
                    .ghost()
                    .compact()
                    .rounded(px(999.0))
                    .with_size(gpui_component::Size::Small)
                    .icon(Icon::new(IconName::Undo2).size(px(12.0)))
                    .tooltip(undo_tooltip)
                    .loading(undo_loading)
                    .disabled(self.git_controls_busy())
                    .text_color(cx.theme().muted_foreground)
                    .min_w(px(22.0))
                    .h(px(22.0))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        view.update(cx, |this, cx| {
                            this.undo_working_copy_file(path.clone(), is_tracked, cx);
                        });
                    })
            })
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(3.0))
                    .bg(accent_strip),
            )
            .into_any_element()
    }

    fn render_workspace_change_stats(&self, stats: LineStats, cx: &mut Context<Self>) -> AnyElement {
        let colors = hunk_line_stats(cx.theme(), cx.theme().mode.is_dark());

        h_flex()
            .items_center()
            .gap_0p5()
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .child("("),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(colors.added)
                    .child(format!("+{}", stats.added)),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(colors.removed)
                    .child(format!("-{}", stats.removed)),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .child(")"),
            )
            .into_any_element()
    }
}
