const GIT_RECENT_COMMITS_PANEL_WIDTH: f32 = 296.0;
const GIT_RECENT_COMMITS_SCROLLBAR_GUTTER: f32 = 16.0;

impl DiffViewer {
    fn render_git_recent_commits_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_git_workspace(cx.theme(), is_dark);
        let recent_count = self.recent_commits.len();
        let branch_scope_label = self
            .checked_out_branch_name()
            .unwrap_or(self.git_workspace.branch_name.as_str());
        let branch_scope_description = if branch_scope_label.is_empty() || branch_scope_label == "unknown" {
            "the current branch".to_string()
        } else if branch_scope_label == "detached" {
            "detached HEAD".to_string()
        } else {
            format!("branch {}", branch_scope_label)
        };
        let subtitle = format!("Latest commits on {branch_scope_description}.");

        let list_container = if self.recent_commits_loading && self.recent_commits.is_empty() {
            self.render_git_recent_commits_loading_skeleton(cx)
        } else if let Some(error) = self
            .recent_commits_error
            .as_ref()
            .filter(|_| self.recent_commits.is_empty())
        {
            v_flex()
                .w_full()
                .items_center()
                .justify_center()
                .p_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .whitespace_normal()
                        .child(error.clone()),
                )
                .into_any_element()
        } else if self.recent_commits.is_empty() {
            v_flex()
                .w_full()
                .items_center()
                .justify_center()
                .p_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .whitespace_normal()
                        .child("No recent commits on the current branch."),
                )
                .into_any_element()
        } else {
            v_flex()
                .w_full()
                .gap_1()
                .pb_2()
                .children(
                    self.recent_commits
                        .iter()
                        .map(|commit| self.render_git_recent_commit_row(commit, cx)),
                )
                .into_any_element()
        };

        v_flex()
            .w_full()
            .h_full()
            .min_h_0()
            .gap_2()
            .p_3()
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.rail.border)
            .bg(colors.rail.background)
            .child(
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Recent Commits"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .whitespace_normal()
                            .child(subtitle),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child(self.render_git_metric_pill(
                        format!("Showing {}", recent_count.min(DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT)),
                        HunkAccentTone::Neutral,
                        cx,
                    ))
                    .child(self.render_git_metric_pill(
                        if branch_scope_label == "detached" {
                            "Detached HEAD".to_string()
                        } else if branch_scope_label.is_empty() || branch_scope_label == "unknown" {
                            "Current Branch".to_string()
                        } else {
                            branch_scope_label.to_string()
                        },
                        HunkAccentTone::Neutral,
                        cx,
                    )),
            )
            .child(
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
                            .id("git-recent-commits-scroll-area")
                            .size_full()
                            .track_scroll(&self.recent_commits_scroll_handle)
                            .overflow_y_scroll()
                            .on_scroll_wheel(move |_, _, cx| {
                                view.update(cx, |this, _| {
                                    this.last_scroll_activity_at = Instant::now();
                                });
                            })
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
                            .w(px(GIT_RECENT_COMMITS_SCROLLBAR_GUTTER))
                            .child(
                                Scrollbar::vertical(&self.recent_commits_scroll_handle)
                                    .scrollbar_show(ScrollbarShow::Scrolling),
                            ),
                    ),
            )
            .when_some(
                self.recent_commits_error
                    .as_ref()
                    .filter(|_| !self.recent_commits.is_empty()),
                |this, error| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().warning)
                            .whitespace_normal()
                            .child(error.clone()),
                    )
                },
            )
            .into_any_element()
    }

    fn render_git_recent_commit_row(
        &self,
        commit: &RecentCommitSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_git_workspace(cx.theme(), is_dark);
        let short_commit_id = short_commit_id(commit.commit_id.as_str());
        let stable_row_id = stable_recent_commit_row_id(commit.commit_id.as_str());

        v_flex()
            .id(("git-recent-commit-row", stable_row_id))
            .w_full()
            .gap_1()
            .p_2()
            .rounded(px(10.0))
            .border_1()
            .border_color(colors.muted_card.border)
            .bg(colors.card.background)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .whitespace_normal()
                    .child(commit.subject.clone()),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded(px(999.0))
                            .bg(hunk_opacity(cx.theme().muted, is_dark, 0.40, 0.58))
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(short_commit_id),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(relative_time_label(commit.committed_unix_time)),
                    ),
            )
            .into_any_element()
    }

    fn render_git_recent_commits_loading_skeleton(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();

        v_flex()
            .w_full()
            .gap_1()
            .children((0..5).map(|_| {
                v_flex()
                    .w_full()
                    .gap_1()
                    .p_2()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.14, 0.24))
                    .child(git_loading_skeleton_block(220.0, 10.0, is_dark, cx))
                    .child(git_loading_skeleton_block(110.0, 9.0, is_dark, cx))
            }))
            .into_any_element()
    }
}

fn stable_recent_commit_row_id(commit_id: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    commit_id.hash(&mut hasher);
    hasher.finish()
}

fn short_commit_id(commit_id: &str) -> String {
    commit_id.chars().take(7).collect()
}
