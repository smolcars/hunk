impl DiffViewer {
    fn git_action_loading_named(&self, action_label: &str) -> bool {
        self.git_action_loading
            && self
                .git_action_label
                .as_deref()
                .is_some_and(|label| label.eq_ignore_ascii_case(action_label))
    }

    fn render_git_workspace_operations_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_git_workspace_operations_panel_v2(cx)
    }

    fn render_workspace_changes_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        const GIT_WORKING_TREE_SCROLLBAR_GUTTER: f32 = 16.0;

        let view = cx.entity();
        let tracked_count = self.files.iter().filter(|file| file.is_tracked()).count();
        let untracked_count = self.files.len().saturating_sub(tracked_count);
        let staged_count = self.staged_commit_file_count();
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_git_workspace(cx.theme(), is_dark);

        v_flex()
            .w_full()
            .h_full()
            .min_h_0()
            .gap_2()
            .p_3()
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.card.border)
            .bg(colors.card.background)
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .justify_between()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Working Tree"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} files changed across tracked and untracked work.",
                                        self.files.len()
                                    )),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .child(self.render_git_metric_pill(
                                format!("Tracked {}", tracked_count),
                                HunkAccentTone::Neutral,
                                cx,
                            ))
                            .child(self.render_git_metric_pill(
                                format!("Untracked {}", untracked_count),
                                if untracked_count > 0 {
                                    HunkAccentTone::Warning
                                } else {
                                    HunkAccentTone::Neutral
                                },
                                cx,
                            ))
                            .child(self.render_git_metric_pill(
                                format!("Staged {}", staged_count),
                                if staged_count > 0 {
                                    HunkAccentTone::Success
                                } else {
                                    HunkAccentTone::Neutral
                                },
                                cx,
                            ))
                            .when(!self.files.is_empty(), |this| {
                                this.child({
                                    let view = view.clone();
                                    Button::new("git-stage-all")
                                        .outline()
                                        .compact()
                                        .rounded(px(8.0))
                                        .label("Stage All")
                                        .tooltip("Stage every changed file for the next commit.")
                                        .disabled(
                                            self.git_action_loading
                                                || staged_count == self.files.len(),
                                        )
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.stage_all_files_for_commit(cx);
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("git-unstage-all")
                                        .outline()
                                        .compact()
                                        .rounded(px(8.0))
                                        .label("Unstage All")
                                        .tooltip(
                                            "Remove every file from the next commit selection.",
                                        )
                                        .disabled(self.git_action_loading || staged_count == 0)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.unstage_all_files_for_commit(cx);
                                            });
                                        })
                                })
                            }),
                    ),
            )
            .child({
                let list_container = if self.files.is_empty() {
                    v_flex()
                        .w_full()
                        .h_full()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("No tracked or untracked changes."),
                        )
                        .into_any_element()
                } else {
                    v_flex()
                        .w_full()
                        .gap_1()
                        .pb_2()
                        .children(self.files.iter().enumerate().map(|(row_ix, file)| {
                            self.render_workspace_change_row(row_ix, file, cx)
                        }))
                        .into_any_element()
                };

                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(colors.muted_card.border)
                    .bg(colors.muted_card.background)
                    .child(
                        div()
                            .id("git-working-tree-scroll-area")
                            .size_full()
                            .track_scroll(&self.git_working_tree_scroll_handle)
                            .overflow_y_scroll()
                            .pr(px(GIT_WORKING_TREE_SCROLLBAR_GUTTER))
                            .child(
                                div()
                                    .w_full()
                                    .min_h_full()
                                    .p_1p5()
                                    .child(list_container),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(px(GIT_WORKING_TREE_SCROLLBAR_GUTTER))
                            .child(
                                Scrollbar::vertical(&self.git_working_tree_scroll_handle)
                                    .scrollbar_show(ScrollbarShow::Always),
                            ),
                    )
                    .into_any_element()
            })
            .into_any_element()
    }
}
