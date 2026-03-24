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
        let staged_count = self.staged_commit_file_count();
        let has_unstaged_changes = self.git_workspace.files.iter().any(|file| file.unstaged);
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_git_workspace(cx.theme(), is_dark);

        v_flex()
            .w_full()
            .h_full()
            .min_h_0()
            .gap_2()
            .p_2()
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
                                    .child("Changes"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} changed files",
                                        self.git_workspace.files.len()
                                    )),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .child(self.render_git_metric_pill(
                                format!("Staged {}", staged_count),
                                if staged_count > 0 {
                                    HunkAccentTone::Success
                                } else {
                                    HunkAccentTone::Neutral
                                },
                                cx,
                            ))
                            .when(!self.git_workspace.files.is_empty(), |this| {
                                this.child({
                                    let view = view.clone();
                                    Button::new("git-stage-all")
                                        .outline()
                                        .compact()
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(8.0))
                                        .label("Stage All")
                                        .tooltip("Stage every changed file.")
                                        .disabled(self.git_action_loading || !has_unstaged_changes)
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
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(8.0))
                                        .label("Unstage All")
                                        .tooltip("Unstage every staged file.")
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
                let list_container = if self.git_workspace.files.is_empty() {
                    v_flex()
                        .w_full()
                        .h_full()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No working tree changes."),
                        )
                        .into_any_element()
                } else {
                    v_flex()
                        .w_full()
                        .items_stretch()
                        .gap_1()
                        .pb_2()
                        .children(self.git_workspace.files.iter().enumerate().map(|(row_ix, file)| {
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
                            .child(
                                div()
                                    .w_full()
                                    .min_h_full()
                                    .p_1()
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

    fn render_git_commit_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_git_workspace(cx.theme(), is_dark);
        let create_commit_loading = self.git_action_loading_named("Create commit");
        let commit_and_push_loading = self.git_action_loading_named("Commit and Push");
        let generate_commit_message_loading =
            self.git_action_loading_named("Generate commit message");
        let push_loading = self.git_action_loading_named("Push branch");
        let git_controls_busy = self.git_rail_controls_busy();
        let push_button_colors = hunk_action_ready_button(cx.theme(), is_dark, HunkAccentTone::Accent);
        let push_available = self.can_push_current_branch_for_ui() || push_loading;
        let push_disabled = !push_available || (git_controls_busy && !push_loading);
        let push_tooltip = if !self.can_run_active_branch_actions_for_ui() {
            "Activate a branch before pushing."
        } else if !self.git_workspace.branch_has_upstream {
            "Publish this branch before pushing."
        } else if self.git_workspace.branch_ahead_count == 0 {
            "No local commits to push."
        } else {
            "Push all local commits on this branch."
        };
        let staged_count = self.staged_commit_file_count();
        let total_count = self.git_workspace.files.len();
        let commit_message_has_text = self
            .commit_input_state
            .read(cx)
            .value()
            .trim()
            .is_empty();
        let commit_message_has_text = !commit_message_has_text;
        let commit_disabled = staged_count == 0
            || !commit_message_has_text
            || (git_controls_busy && !create_commit_loading);
        let generate_commit_message_disabled =
            staged_count == 0 || (git_controls_busy && !generate_commit_message_loading);
        let commit_and_push_tooltip = self.combined_workspace_commit_and_push_tooltip();
        let commit_and_push_disabled =
            !self.can_run_combined_workspace_commit_and_push_for_ui() && !commit_and_push_loading;
        let commit_readiness_label = if staged_count == 0 {
            "Stage files".to_string()
        } else if !commit_message_has_text {
            "Add commit message".to_string()
        } else {
            "Ready to commit".to_string()
        };
        let last_commit_text = self
            .last_commit_subject
            .as_deref()
            .map(str::trim_end)
            .filter(|text| !text.is_empty())
            .unwrap_or("No commits yet")
            .to_string();

        v_flex()
            .w_full()
            .gap_2()
            .p_3()
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.rail.border)
            .bg(colors.rail.background)
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
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Commit & Publish"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("Staged {staged_count}/{total_count} files")),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child(self.render_git_metric_pill(
                        commit_readiness_label,
                        if commit_disabled {
                            HunkAccentTone::Warning
                        } else {
                            HunkAccentTone::Success
                        },
                        cx,
                    ))
                    .child(self.render_git_metric_pill(
                        format!("To Push {}", self.git_workspace.branch_ahead_count),
                        if self.git_workspace.branch_ahead_count > 0 {
                            HunkAccentTone::Accent
                        } else {
                            HunkAccentTone::Neutral
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .child(
                        Input::new(&self.commit_input_state)
                            .appearance(true)
                            .w_full()
                            .with_size(gpui_component::Size::Medium)
                            .px_2()
                            .h(px(112.0))
                            .rounded(px(8.0))
                            .bg(colors.muted_card.background)
                            .border_color(colors.muted_card.border)
                            .disabled(git_controls_busy),
                    )
                    .child({
                        let view = view.clone();
                        Button::new("git-generate-commit-message")
                            .ghost()
                            .compact()
                            .rounded(px(999.0))
                            .with_size(gpui_component::Size::Small)
                            .icon(Icon::new(HunkIconName::Wand).size(px(12.0)))
                            .tooltip("Generate a commit message from the staged changes.")
                            .loading(generate_commit_message_loading)
                            .disabled(generate_commit_message_disabled)
                            .text_color(cx.theme().muted_foreground)
                            .bg(colors.rail.background.opacity(0.96))
                            .border_1()
                            .border_color(colors.muted_card.border)
                            .min_w(px(20.0))
                            .h(px(20.0))
                            .absolute()
                            .left(px(10.0))
                            .bottom(px(10.0))
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.generate_commit_message_from_staged(window, cx);
                                });
                            })
                    }),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child({
                        let view = view.clone();
                        Button::new("commit-staged-v3")
                            .primary()
                            .rounded(px(8.0))
                            .loading(create_commit_loading)
                            .label(if create_commit_loading { "Committing..." } else { "Commit" })
                            .tooltip("Create a new commit from staged files using the message above.")
                            .disabled(commit_disabled)
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.commit_from_input(window, cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        let mut button = Button::new("push-branch-v3")
                            .outline()
                            .rounded(px(8.0))
                            .loading(push_loading)
                            .label(if push_loading { "Pushing..." } else { "Push" })
                            .tooltip(push_tooltip)
                            .disabled(push_disabled)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.push_current_branch(cx);
                                });
                            });
                        if !push_disabled {
                            button = button
                                .bg(push_button_colors.background)
                                .border_color(push_button_colors.border)
                                .text_color(push_button_colors.text);
                        }
                        button
                    })
                    .child({
                        let view = view.clone();
                        Button::new("commit-and-push-all-v1")
                            .outline()
                            .rounded(px(8.0))
                            .loading(commit_and_push_loading)
                            .label(if commit_and_push_loading {
                                "Working..."
                            } else {
                                "Stage, Commit & Push"
                            })
                            .tooltip(commit_and_push_tooltip)
                            .disabled(commit_and_push_disabled)
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.confirm_combined_workspace_commit_and_push(window, cx);
                                });
                            })
                    }),
            )
            .child(
                v_flex()
                    .w_full()
                    .gap_0p5()
                    .p_2()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(colors.muted_card.border)
                    .bg(colors.muted_card.background)
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Last Commit"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground.opacity(0.92))
                            .whitespace_normal()
                            .child(last_commit_text),
                    ),
            )
            .into_any_element()
    }
}
