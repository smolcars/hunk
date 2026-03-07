impl DiffViewer {
    fn render_workspace_change_row(
        &self,
        row_ix: usize,
        file: &ChangedFile,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let included_in_commit = !self.commit_excluded_files.contains(file.path.as_str());
        let is_selected = self.selected_path.as_deref() == Some(file.path.as_str());
        let is_dark = cx.theme().mode.is_dark();
        let undo_loading = self.git_action_loading_named("Undo file changes");
        let (status_label, status_color) = change_status_label_color(file.status, cx);
        let is_tracked = file.is_tracked();
        let tracking_label = if is_tracked { "tracked" } else { "untracked" };
        let tracking_color = if is_tracked {
            hunk_opacity(cx.theme().secondary, is_dark, 0.36, 0.56)
        } else {
            hunk_opacity(cx.theme().warning, is_dark, 0.30, 0.20)
        };
        let undo_tooltip = if is_tracked {
            "Restore this file to HEAD."
        } else {
            "Delete this untracked file from the working tree."
        };
        let row_bg = if is_selected {
            hunk_opacity(cx.theme().accent, is_dark, 0.22, 0.14)
        } else {
            cx.theme().background.opacity(0.0)
        };
        let path = file.path.clone();

        h_flex()
            .id(("workspace-change-row", row_ix))
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
                Button::new(("workspace-commit-include-toggle", row_ix))
                    .outline()
                    .compact()
                    .rounded(px(5.0))
                    .min_w(px(22.0))
                    .h(px(20.0))
                    .label(if include { "x" } else { "" })
                    .tooltip(if include {
                        "Included in next commit"
                    } else {
                        "Excluded from next commit"
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
                    .bg(hunk_opacity(status_color, is_dark, 0.24, 0.16))
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
                Button::new(("workspace-change-undo", row_ix))
                    .outline()
                    .compact()
                    .rounded(px(5.0))
                    .loading(undo_loading)
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
}
