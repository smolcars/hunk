const GIT_WORKSPACE_RAIL_MAX_WIDTH: f32 = 368.0;

impl DiffViewer {
    fn render_git_workspace_operations_panel_v2(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(self.render_git_workspace_hero(cx))
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
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
                            .w(px(GIT_WORKSPACE_RAIL_MAX_WIDTH))
                            .min_w(px(GIT_WORKSPACE_RAIL_MAX_WIDTH))
                            .h_full()
                            .min_h_0()
                            .gap_3()
                            .child(self.render_git_branch_panel(cx))
                            .child(self.render_git_commit_panel(cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_git_workspace_hero(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_git_workspace(cx.theme(), is_dark);
        let active_branch_label = self
            .checked_out_branch_name()
            .map_or_else(|| "detached".to_string(), ToOwned::to_owned);
        let tracked_count = self.files.iter().filter(|file| file.is_tracked()).count();
        let untracked_count = self.files.len().saturating_sub(tracked_count);
        let staged_count = self.staged_commit_file_count();
        let overall_changed_lines = self.overall_line_stats.changed();
        let branch_syncable = self.can_run_active_branch_actions();
        let primary_action = self.render_git_hero_primary_action(cx);
        let active_review_blocker = self.active_review_action_blocker();
        let review_tooltip = active_review_blocker.clone().unwrap_or_else(|| {
            "Open a prefilled pull/merge request page for the active branch.".to_string()
        });
        let review_disabled = active_review_blocker.is_some();
        let review_is_primary = branch_syncable
            && self.branch_has_upstream
            && self.branch_ahead_count == 0
            && self.branch_behind_count == 0;
        let summary = if !branch_syncable {
            "Detached HEAD. Activate or create a branch to publish, sync, and review."
                .to_string()
        } else if self.files.is_empty() {
            "Working tree clean. Branch actions are available.".to_string()
        } else {
            format!(
                "{} changed files ready for review and commit.",
                self.files.len()
            )
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
            .gap_3()
            .p_4()
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.hero.border)
            .bg(colors.hero.background)
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .flex_wrap()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Active Branch"),
                            )
                            .child(
                                div()
                                    .text_lg()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(active_branch_label),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .whitespace_normal()
                                    .child(summary),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .justify_end()
                            .child(primary_action)
                            .when(!review_is_primary, |this| {
                                this.child({
                                    let view = view.clone();
                                    Button::new("git-hero-open-review")
                                        .outline()
                                        .rounded(px(9.0))
                                        .label("Open PR/MR")
                                        .tooltip(review_tooltip)
                                        .disabled(review_disabled)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.open_current_branch_review_url(cx);
                                            });
                                        })
                                })
                            }),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child(self.render_git_metric_pill(
                        if self.branch_has_upstream {
                            "Published"
                        } else {
                            "Unpublished"
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
                    ))
                    .child(self.render_git_metric_pill(
                        format!("Changed {}", self.files.len()),
                        if self.files.is_empty() {
                            HunkAccentTone::Neutral
                        } else {
                            HunkAccentTone::Accent
                        },
                        cx,
                    ))
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
                    .when(overall_changed_lines > 0, |this| {
                        this.child(self.render_line_stats("git-hero", self.overall_line_stats, cx))
                    }),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap_2()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Last Commit"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .whitespace_normal()
                                    .child(last_commit_text),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_git_hero_primary_action(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let branch_syncable = self.can_run_active_branch_actions();
        let sync_loading = self.git_action_loading_named("Sync branch");
        let publish_loading = self.git_action_loading_named("Publish branch");
        let push_loading = self.git_action_loading_named("Push branch");
        let open_review_loading = self.git_action_loading_named("Open PR/MR");
        let sync_disabled = !self.can_sync_current_branch();
        let publish_disabled = !self.can_publish_current_branch();
        let push_available = self.can_push_current_branch() || push_loading;
        let push_disabled = !push_available || (self.git_action_loading && !push_loading);
        let sync_tooltip = if !branch_syncable {
            "Activate a branch before syncing."
        } else if !self.branch_has_upstream {
            "Publish this branch before syncing."
        } else if !self.files.is_empty() {
            "Commit or discard working tree changes before syncing."
        } else {
            "Fetch and fast-forward this branch from its upstream remote."
        };
        let publish_tooltip = if self.branch_has_upstream {
            "This branch already tracks upstream. Use Push below."
        } else if !branch_syncable {
            "Activate a branch before publishing."
        } else if !self.files.is_empty() {
            "Commit or discard working tree changes before publishing."
        } else {
            "Publish this branch to upstream and start tracking it."
        };
        let push_tooltip = if !branch_syncable {
            "Activate a branch before pushing."
        } else if !self.branch_has_upstream {
            "Publish this branch before pushing."
        } else if self.branch_ahead_count == 0 {
            "No local commits to push."
        } else if !self.files.is_empty() {
            "Commit or discard working tree changes before pushing."
        } else {
            "Push all local commits on this branch."
        };
        let review_tooltip = self.active_review_action_blocker().unwrap_or_else(|| {
            "Open a prefilled pull/merge request page for the active branch.".to_string()
        });
        let review_disabled = self.active_review_action_blocker().is_some();

        if !branch_syncable {
            return Button::new("git-hero-detached")
                .primary()
                .rounded(px(9.0))
                .label("Detached")
                .tooltip("Activate or create a branch to continue.")
                .disabled(true)
                .into_any_element();
        }

        if !self.branch_has_upstream {
            return {
                let view = view.clone();
                Button::new("git-hero-publish")
                    .primary()
                    .rounded(px(9.0))
                    .loading(publish_loading)
                    .label(if publish_loading {
                        "Publishing..."
                    } else {
                        "Publish Branch"
                    })
                    .tooltip(publish_tooltip)
                    .disabled(publish_disabled)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.publish_current_branch(cx);
                        });
                    })
                    .into_any_element()
            };
        }

        if self.branch_ahead_count > 0 {
            return {
                let view = view.clone();
                Button::new("git-hero-push")
                    .primary()
                    .rounded(px(9.0))
                    .loading(push_loading)
                    .label(if push_loading {
                        "Pushing...".to_string()
                    } else {
                        format!("Push {} commits", self.branch_ahead_count)
                    })
                    .tooltip(push_tooltip)
                    .disabled(push_disabled)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.push_current_branch(cx);
                        });
                    })
                    .into_any_element()
            };
        }

        if self.branch_behind_count > 0 {
            return {
                let view = view.clone();
                Button::new("git-hero-sync")
                    .primary()
                    .rounded(px(9.0))
                    .loading(sync_loading)
                    .label(if sync_loading {
                        "Syncing...".to_string()
                    } else {
                        format!("Sync {} commits", self.branch_behind_count)
                    })
                    .tooltip(sync_tooltip)
                    .disabled(sync_disabled)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.sync_current_branch_from_remote(cx);
                        });
                    })
                    .into_any_element()
            };
        }

        {
            let view = view.clone();
            Button::new("git-hero-review")
                .primary()
                .rounded(px(9.0))
                .loading(open_review_loading)
                .label("Open PR/MR")
                .tooltip(review_tooltip)
                .disabled(review_disabled)
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.open_current_branch_review_url(cx);
                    });
                })
                .into_any_element()
        }
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
        let activate_branch_loading = self.git_action_loading_named("Activate branch");
        let sync_loading = self.git_action_loading_named("Sync branch");
        let publish_loading = self.git_action_loading_named("Publish branch");
        let rename_loading = self.git_action_loading_named("Rename branch");
        let open_review_loading = self.git_action_loading_named("Open PR/MR");
        let copy_review_loading = self.git_action_loading_named("Copy PR/MR URL");
        let branch_syncable = self.can_run_active_branch_actions();
        let sync_disabled = !self.can_sync_current_branch();
        let publish_disabled = !self.can_publish_current_branch();
        let rename_disabled = self.git_action_loading
            || !self.can_run_active_branch_actions()
            || !self.branch_input_has_text;
        let create_or_activate_disabled = self.git_action_loading || !self.branch_input_has_text;
        let active_review_blocker = self.active_review_action_blocker();
        let review_url_disabled = active_review_blocker.is_some();
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
        let branch_menu_entries = self
            .branches
            .iter()
            .map(|branch| {
                (
                    branch.name.clone(),
                    branch.is_current,
                    relative_time_label(branch.tip_unix_time),
                )
            })
            .collect::<Vec<_>>();

        v_flex()
            .w_full()
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
                            .child("Branch Controls"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(sync_state_label),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
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
                Button::new("branch-selector-v3")
                    .outline()
                    .with_size(gpui_component::Size::Small)
                    .rounded(px(8.0))
                    .loading(activate_branch_loading)
                    .w_full()
                    .bg(colors.muted_card.background)
                    .border_color(colors.muted_card.border)
                    .label(active_branch_label)
                    .dropdown_caret(true)
                    .tooltip("Select a branch to activate it.")
                    .disabled(self.git_action_loading)
                    .dropdown_menu({
                        let view = view.clone();
                        move |menu, _, _| {
                            branch_menu_entries.iter().fold(menu, |menu, entry| {
                                let view = view.clone();
                                let branch_name = entry.0.clone();
                                let branch_label = format!("{} · {}", entry.0, entry.2);

                                menu.item(
                                    PopupMenuItem::new(branch_label)
                                        .checked(entry.1)
                                        .on_click(move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.checkout_branch(
                                                    branch_name.clone(),
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }),
                                )
                            })
                        }
                    }),
            )
            .child(
                div()
                    .w_full()
                    .min_h(px(42.0))
                    .px_2p5()
                    .py_2()
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
                            .disabled(self.git_action_loading),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child({
                        let view = view.clone();
                        Button::new("create-or-switch-branch-v3")
                            .primary()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .loading(activate_branch_loading)
                            .label("Create / Activate")
                            .tooltip(
                                "Create a branch from the entered name or activate it if it already exists.",
                            )
                            .disabled(create_or_activate_disabled)
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.create_or_switch_branch_from_input(window, cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        Button::new("rename-active-branch-v3")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .loading(rename_loading)
                            .label("Rename")
                            .tooltip("Rename the currently active branch to the entered name.")
                            .disabled(rename_disabled)
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.rename_current_branch_from_input(window, cx);
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
                    .gap_2()
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

    fn render_git_commit_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let colors = hunk_git_workspace(cx.theme(), is_dark);
        let create_commit_loading = self.git_action_loading_named("Create commit");
        let push_loading = self.git_action_loading_named("Push branch");
        let push_available = self.can_push_current_branch() || push_loading;
        let push_disabled = !push_available || (self.git_action_loading && !push_loading);
        let push_tooltip = if !self.can_run_active_branch_actions() {
            "Activate a branch before pushing."
        } else if !self.branch_has_upstream {
            "Publish this branch before pushing."
        } else if self.branch_ahead_count == 0 {
            "No local commits to push."
        } else if !self.files.is_empty() {
            "Commit or discard working tree changes before pushing."
        } else {
            "Push all local commits on this branch."
        };
        let staged_count = self.staged_commit_file_count();
        let total_count = self.files.len();
        let commit_disabled = staged_count == 0 || (self.git_action_loading && !create_commit_loading);
        let commit_readiness_label = if staged_count == 0 {
            "Stage files to commit".to_string()
        } else {
            "Enter a message and commit".to_string()
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
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Commit & Publish"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("Staged {staged_count}/{total_count} changed files")),
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
                        format!("To Push {}", self.branch_ahead_count),
                        if self.branch_ahead_count > 0 {
                            HunkAccentTone::Accent
                        } else {
                            HunkAccentTone::Neutral
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(colors.muted_card.border)
                    .bg(colors.muted_card.background)
                    .px_3()
                    .py_2()
                    .child(
                        Input::new(&self.commit_input_state)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .w_full()
                            .h(px(100.0))
                            .disabled(self.git_action_loading),
                    ),
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
                            .label(if create_commit_loading {
                                "Creating Commit..."
                            } else {
                                "Create Commit"
                            })
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
                        Button::new("push-branch-v3")
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
                            .text_sm()
                            .text_color(cx.theme().foreground.opacity(0.92))
                            .whitespace_normal()
                            .child(last_commit_text),
                    ),
            )
            .into_any_element()
    }
}
