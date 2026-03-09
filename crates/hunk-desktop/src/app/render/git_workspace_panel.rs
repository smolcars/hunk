const GIT_WORKSPACE_RAIL_WIDTH: f32 = 396.0;

impl DiffViewer {
    fn render_git_workspace_operations_panel_v2(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .items_stretch()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .min_h_0()
                    .child(self.render_workspace_changes_panel(cx)),
            )
            .child(
                v_flex()
                    .flex_none()
                    .w(px(GIT_WORKSPACE_RAIL_WIDTH))
                    .min_w(px(GIT_WORKSPACE_RAIL_WIDTH))
                    .h_full()
                    .min_h_0()
                    .gap_3()
                    .child(self.render_git_branch_panel(cx))
                    .child(self.render_git_commit_panel(cx)),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(GIT_RECENT_COMMITS_PANEL_WIDTH))
                    .min_w(px(GIT_RECENT_COMMITS_PANEL_WIDTH))
                    .h_full()
                    .min_h_0()
                    .child(self.render_git_recent_commits_panel(cx)),
            )
            .into_any_element()
    }

    fn render_git_metric_pill(
        &self,
        label: impl Into<SharedString>,
        tone: HunkAccentTone,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_tinted_button(cx.theme(), is_dark, tone);

        div()
            .px_2()
            .py_1()
            .rounded(px(999.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_xs()
            .font_semibold()
            .text_color(colors.text)
            .child(label.into())
            .into_any_element()
    }

    fn render_git_branch_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_git_workspace(cx.theme(), is_dark);
        let create_worktree_loading = self.git_action_loading_named("Create worktree");
        let activate_branch_loading = self.git_action_loading_named("Activate branch");
        let sync_loading = self.git_action_loading_named("Sync branch");
        let publish_loading = self.git_action_loading_named("Publish branch");
        let open_review_loading = self.git_action_loading_named("Open PR/MR");
        let copy_review_loading = self.git_action_loading_named("Copy PR/MR URL");
        let git_controls_busy = self.git_controls_busy();
        let create_worktree_disabled = git_controls_busy || !self.worktree_branch_input_has_text;
        let branch_syncable = self.can_run_active_branch_actions();
        let sync_disabled = !self.can_sync_current_branch();
        let publish_disabled = !self.can_publish_current_branch();
        let create_or_activate_disabled = git_controls_busy || !self.branch_input_has_text;
        let active_review_blocker = self.active_review_action_blocker();
        let review_url_disabled = active_review_blocker.is_some();
        let active_target_label = self
            .workspace_targets
            .iter()
            .find(|target| self.active_workspace_target_id.as_deref() == Some(target.id.as_str()))
            .map(|target| target.display_name.clone())
            .or_else(|| {
                self.repo_root.as_ref().and_then(|root| {
                    root.file_name()
                        .and_then(|name| name.to_str())
                        .map(str::to_string)
                })
            })
            .unwrap_or_else(|| "Primary Checkout".to_string());
        let active_branch_label = self
            .checked_out_branch_name()
            .map_or_else(|| "detached".to_string(), ToOwned::to_owned);
        let sync_state_label = if !branch_syncable {
            "Detached".to_string()
        } else if self.branch_has_upstream {
            if self.branch_ahead_count > 0 || self.branch_behind_count > 0 {
                format!(
                    "{} ahead / {} behind",
                    self.branch_ahead_count, self.branch_behind_count
                )
            } else {
                "Up to date".to_string()
            }
        } else {
            "Not published".to_string()
        };
        let sync_tooltip = if !branch_syncable {
            "Activate a branch before syncing."
        } else if !self.branch_has_upstream {
            "Publish this branch before syncing."
        } else if !self.files.is_empty() {
            "Commit or discard working tree changes before syncing."
        } else {
            "Fetch and fast-forward this branch from its upstream remote."
        };
        let publish_state_tooltip = if self.branch_has_upstream {
            "This branch already tracks upstream. Use Push below."
        } else if !branch_syncable {
            "Activate a branch before publishing."
        } else if !self.files.is_empty() {
            "Commit or discard working tree changes before publishing."
        } else {
            "Publish this branch to upstream and start tracking it."
        };
        v_flex()
            .w_full()
            .gap_1p5()
            .p_2p5()
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
                            .child("Workspace & Branch Controls"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(sync_state_label),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .gap_1p5()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Workspace Target"),
                    )
                    .child(
                        Select::new(&self.workspace_target_picker_state)
                            .with_size(gpui_component::Size::Small)
                            .placeholder(active_target_label)
                            .search_placeholder("Find a branch or project")
                            .rounded(px(8.0))
                            .w_full()
                            .bg(colors.muted_card.background)
                            .border_color(colors.muted_card.border)
                            .disabled(git_controls_busy || self.workspace_targets.is_empty())
                            .empty(
                                h_flex()
                                    .h(px(72.0))
                                    .justify_center()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("No workspace targets available."),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Create Worktree"),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_h(px(32.0))
                            .px_2()
                            .py_1()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.muted_card.border)
                            .bg(colors.muted_card.background)
                            .child(
                                Input::new(&self.worktree_branch_input_state)
                                    .appearance(false)
                                    .bordered(false)
                                    .focus_bordered(false)
                                    .w_full()
                                    .disabled(git_controls_busy),
                            ),
                    )
                    .child({
                        let view = view.clone();
                        Button::new("git-create-managed-worktree")
                            .primary()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .loading(create_worktree_loading)
                            .label("Create Worktree")
                            .tooltip(
                                "Create a managed worktree under .hunkdiff/worktrees/worktree-N for this branch.",
                            )
                            .disabled(create_worktree_disabled)
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.create_managed_worktree_from_input(window, cx);
                                });
                            })
                    }),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1p5()
                    .flex_wrap()
                    .child(self.render_git_metric_pill(
                        if self.branch_has_upstream {
                            "Published"
                        } else {
                            "Local Only"
                        },
                        if self.branch_has_upstream {
                            HunkAccentTone::Success
                        } else {
                            HunkAccentTone::Warning
                        },
                        cx,
                    ))
                    .child(self.render_git_metric_pill(
                        format!("Ahead {}", self.branch_ahead_count),
                        if self.branch_ahead_count > 0 {
                            HunkAccentTone::Accent
                        } else {
                            HunkAccentTone::Neutral
                        },
                        cx,
                    ))
                    .child(self.render_git_metric_pill(
                        format!("Behind {}", self.branch_behind_count),
                        if self.branch_behind_count > 0 {
                            HunkAccentTone::Warning
                        } else {
                            HunkAccentTone::Neutral
                        },
                        cx,
                    )),
            )
            .child(
                Select::new(&self.branch_picker_state)
                    .with_size(gpui_component::Size::Small)
                    .placeholder(active_branch_label)
                    .search_placeholder("Find a branch")
                    .rounded(px(8.0))
                    .w_full()
                    .bg(colors.muted_card.background)
                    .border_color(colors.muted_card.border)
                    .disabled(git_controls_busy)
                    .empty(
                        h_flex()
                            .h(px(72.0))
                            .justify_center()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No branches match your search."),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .min_h(px(32.0))
                    .px_2()
                    .py_1()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(colors.muted_card.border)
                    .bg(colors.muted_card.background)
                    .child(
                        Input::new(&self.branch_input_state)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .w_full()
                            .disabled(git_controls_busy),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1p5()
                    .flex_wrap()
                    .child({
                        let view = view.clone();
                        Button::new("create-or-switch-branch-v3")
                            .primary()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .loading(activate_branch_loading)
                            .label("Create / Switch")
                            .tooltip(
                                "Create a branch from the entered name or switch to it if it already exists.",
                            )
                            .disabled(create_or_activate_disabled)
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.create_or_switch_branch_from_input(window, cx);
                                });
                            })
                    }),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1p5()
                    .flex_wrap()
                    .child({
                        let view = view.clone();
                        Button::new("sync-branch-v3")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .loading(sync_loading)
                            .label("Sync")
                            .tooltip(sync_tooltip)
                            .disabled(sync_disabled)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.sync_current_branch_from_remote(cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        let mut button = Button::new("branch-publish-state-v3")
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .loading(publish_loading)
                            .label(if self.branch_has_upstream {
                                "Published"
                            } else {
                                "Publish"
                            })
                            .tooltip(publish_state_tooltip)
                            .disabled(self.branch_has_upstream || publish_disabled)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.publish_current_branch(cx);
                                });
                            });
                        if self.branch_has_upstream {
                            button = button.outline();
                        } else {
                            button = button.primary();
                        }
                        button
                    }),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1p5()
                    .flex_wrap()
                    .child({
                        let view = view.clone();
                        let blocker = active_review_blocker.clone();
                        Button::new("open-review-url-v3")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .loading(open_review_loading)
                            .label("Open PR/MR")
                            .tooltip(blocker.clone().unwrap_or_else(|| {
                                "Open a prefilled pull/merge request page for the active branch."
                                    .to_string()
                            }))
                            .disabled(review_url_disabled)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.open_current_branch_review_url(cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        let blocker = active_review_blocker.clone();
                        Button::new("copy-review-url-v3")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .loading(copy_review_loading)
                            .label("Copy Review URL")
                            .tooltip(blocker.unwrap_or_else(|| {
                                "Copy a prefilled pull/merge request URL for the active branch."
                                    .to_string()
                            }))
                            .disabled(review_url_disabled)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.copy_current_branch_review_url(cx);
                                });
                            })
                    }),
            )
            .into_any_element()
    }

}
