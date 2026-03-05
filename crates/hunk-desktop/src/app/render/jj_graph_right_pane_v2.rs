impl DiffViewer {
    fn render_jj_graph_operations_panel_v2(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let branch_syncable = self.can_run_active_bookmark_actions();
        let sync_disabled = !self.can_sync_current_bookmark();
        let publish_disabled = !self.can_publish_current_bookmark();
        let push_revisions_disabled = !self.can_push_current_bookmark_revisions();
        let sync_tooltip = if !branch_syncable {
            "Activate a bookmark before syncing."
        } else if !self.branch_has_upstream {
            "Publish this bookmark before syncing."
        } else if !self.files.is_empty() {
            "Commit or discard working-copy changes before syncing."
        } else {
            "Fetch and update this bookmark from its upstream remote."
        };
        let publish_state_tooltip = if self.branch_has_upstream {
            "This bookmark already tracks upstream. Use Push Revisions below."
        } else if !branch_syncable {
            "Activate a bookmark before publishing."
        } else if !self.files.is_empty() {
            "Commit or discard working-copy changes before publishing."
        } else {
            "Publish this bookmark to upstream and start tracking it."
        };
        let active_review_blocker = self.active_review_action_blocker();
        let review_url_disabled = active_review_blocker.is_some();
        let recovery_candidate = self.latest_working_copy_recovery_candidate_for_active_bookmark();
        let pending_switch = self.pending_bookmark_switch();
        let push_revisions_label = "Push Revisions";
        let push_revisions_tooltip = if !branch_syncable {
            "Activate a bookmark before pushing revisions."
        } else if !self.branch_has_upstream {
            "Publish this bookmark first, then push grouped revisions."
        } else if self.branch_ahead_count == 0 {
            "No local revisions to push."
        } else if !self.files.is_empty() {
            "Commit or discard working-copy changes before pushing revisions."
        } else {
            "Push all unpushed revisions on this bookmark."
        };

        let active_bookmark_label = self
            .checked_out_bookmark_name()
            .map_or_else(|| "detached".to_string(), ToOwned::to_owned);
        let repo_label = self.project_display_name().to_lowercase();
        let active_bookmark_chip_label = format!("{repo_label}/{active_bookmark_label}");
        let sync_state_label = if !branch_syncable {
            "Detached".to_string()
        } else if self.branch_has_upstream {
            if self.branch_ahead_count > 0 {
                format!("{} to push", self.branch_ahead_count)
            } else {
                "Up to date".to_string()
            }
        } else {
            "Not published".to_string()
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

        let last_commit_text = self
            .last_commit_subject
            .as_deref()
            .map(str::trim_end)
            .filter(|text| !text.is_empty())
            .unwrap_or("No commits yet")
            .to_string();

        let included_count = self.included_commit_file_count();
        let total_count = self.files.len();
        let commit_message_present = !self.commit_input_state.read(cx).value().trim().is_empty();
        let commit_disabled = self.git_action_loading || !commit_message_present || included_count == 0;
        let describe_tip_disabled = self.git_action_loading
            || !commit_message_present
            || !branch_syncable
            || self.bookmark_revisions.is_empty();

        let bookmark_input_empty = self.branch_input_state.read(cx).value().trim().is_empty();
        let rename_disabled =
            self.git_action_loading || bookmark_input_empty || !self.can_run_active_bookmark_actions();
        let create_or_activate_disabled = self.git_action_loading || bookmark_input_empty;

        v_flex()
            .w_full()
            .gap_2()
            .px_3()
            .pt_2()
            .pb_2()
            .bg(cx.theme().sidebar.blend(cx.theme().muted.opacity(if is_dark {
                0.16
            } else {
                0.24
            })))
            .child(self.render_git_action_status_banner(cx))
            .when_some(pending_switch, |this, pending| {
                this.child(
                    v_flex()
                        .w_full()
                        .gap_1()
                        .px_2()
                        .py_1()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(cx.theme().warning.opacity(if is_dark { 0.90 } else { 0.72 }))
                        .bg(cx.theme().warning.opacity(if is_dark { 0.16 } else { 0.10 }))
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child("Switch Bookmark With Local Changes"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().foreground)
                                .whitespace_normal()
                                .child(format!(
                                    "{} files in working copy while switching {} -> {} at {}.",
                                    pending.changed_file_count,
                                    pending.source_bookmark,
                                    pending.target_bookmark,
                                    relative_time_label(Some(pending.unix_time))
                                )),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .gap_1()
                                .flex_wrap()
                                .child({
                                    let view = view.clone();
                                    Button::new("confirm-switch-move-v2")
                                        .primary()
                                        .compact()
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(7.0))
                                        .label("Move Changes to Target")
                                        .tooltip("Switch and carry current working-copy changes into the target bookmark.")
                                        .disabled(self.git_action_loading)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.confirm_pending_bookmark_switch_move_changes(cx);
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("confirm-switch-snapshot-v2")
                                        .outline()
                                        .compact()
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(7.0))
                                        .label("Snapshot Here, Then Switch")
                                        .tooltip("Keep current changes captured on the source bookmark, then switch to the target bookmark.")
                                        .disabled(self.git_action_loading)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.confirm_pending_bookmark_switch_snapshot(cx);
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("cancel-pending-switch-v2")
                                        .outline()
                                        .compact()
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(7.0))
                                        .label("Cancel")
                                        .tooltip("Cancel this bookmark switch and keep the current active bookmark.")
                                        .disabled(self.git_action_loading)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.cancel_pending_bookmark_switch(cx);
                                            });
                                        })
                                }),
                        ),
                )
            })
            .child(self.render_workspace_changes_panel(cx))
            .child(
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
                            .child("Bookmarks"),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_1()
                            .flex_wrap()
                            .child(
                                Button::new("bookmark-selector-v2")
                                    .outline()
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(7.0))
                                    .min_w(px(150.0))
                                    .bg(cx.theme().secondary.opacity(if is_dark { 0.50 } else { 0.70 }))
                                    .border_color(
                                        cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }),
                                    )
                                    .label(active_bookmark_chip_label)
                                    .dropdown_caret(true)
                                    .tooltip("Select a bookmark to activate it.")
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
                                                                this.checkout_bookmark(
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
                            .child({
                                let view = view.clone();
                                Button::new("sync-branch-v2")
                                    .outline()
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(7.0))
                                    .min_w(px(92.0))
                                    .label("Sync")
                                    .tooltip(sync_tooltip)
                                    .disabled(sync_disabled)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.sync_current_bookmark_from_remote(cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                let mut button = Button::new("bookmark-publish-state-v2")
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(7.0))
                                    .min_w(px(104.0))
                                    .label(if self.branch_has_upstream {
                                        "Published"
                                    } else {
                                        "Publish"
                                    })
                                    .tooltip(publish_state_tooltip)
                                    .disabled(self.branch_has_upstream || publish_disabled)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.publish_current_bookmark(cx);
                                        });
                                    });
                                if self.branch_has_upstream {
                                    button = button.outline();
                                } else {
                                    button = button.primary();
                                }
                                button.into_any_element()
                            })
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("State: {sync_state_label}")),
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
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_1()
                            .flex_wrap()
                            .child({
                                let view = view.clone();
                                Button::new("create-or-switch-bookmark-v2")
                                    .primary()
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(7.0))
                                    .label("Create / Activate")
                                    .tooltip("Create a bookmark from the entered name or activate it if it already exists.")
                                    .disabled(create_or_activate_disabled)
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.create_or_switch_bookmark_from_input(window, cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("rename-active-bookmark-v2")
                                    .outline()
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(7.0))
                                    .label("Rename Active")
                                    .tooltip("Rename the currently active bookmark to the entered name.")
                                    .disabled(rename_disabled)
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.rename_current_bookmark_from_input(window, cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                let blocker = active_review_blocker.clone();
                                Button::new("open-review-url-v2")
                                    .outline()
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(7.0))
                                    .label("Open PR/MR")
                                    .tooltip(blocker.clone().unwrap_or_else(|| {
                                        "Open a prefilled pull/merge request page for the active bookmark.".to_string()
                                    }))
                                    .disabled(review_url_disabled)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.open_current_bookmark_review_url(cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                let blocker = active_review_blocker.clone();
                                Button::new("copy-review-url-v2")
                                    .outline()
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(7.0))
                                    .label("Copy Review URL")
                                    .tooltip(blocker.unwrap_or_else(|| {
                                        "Copy a prefilled pull/merge request URL for the active bookmark.".to_string()
                                    }))
                                    .disabled(review_url_disabled)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.copy_current_bookmark_review_url(cx);
                                        });
                                    })
                            }),
                    ),
            )
            .when_some(recovery_candidate.as_ref(), |this, candidate| {
                this.child(
                    v_flex()
                        .w_full()
                        .gap_1()
                        .px_2()
                        .py_1()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
                        .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                            0.18
                        } else {
                            0.24
                        })))
                        .child(
                            div()
                                .w_full()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .whitespace_normal()
                                .child(format!(
                                    "{} files captured from {} -> {} at {}",
                                    candidate.changed_file_count,
                                    candidate.source_bookmark,
                                    candidate.switched_to_bookmark,
                                    relative_time_label(Some(candidate.unix_time))
                                )),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .gap_1()
                                .child({
                                    let view = view.clone();
                                    Button::new("recover-working-copy-v2")
                                        .outline()
                                        .compact()
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(7.0))
                                        .label("Restore Captured Changes")
                                        .tooltip("Restore files from the captured pre-switch working-copy revision.")
                                        .disabled(self.git_action_loading)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.recover_latest_working_copy_for_active_bookmark(cx);
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("discard-working-copy-recovery-v2")
                                        .outline()
                                        .compact()
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(7.0))
                                        .label("Discard Record")
                                        .tooltip("Discard this captured recovery record without restoring files.")
                                        .disabled(self.git_action_loading)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.discard_latest_working_copy_recovery_candidate_for_active_bookmark(cx);
                                            });
                                        })
                                }),
                        ),
                )
            })
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .p_2()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
                    .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                        0.24
                    } else {
                        0.26
                    })))
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("Commit includes {included_count}/{total_count} files")),
                            )
                            .when(included_count < total_count, |this| {
                                this.child({
                                    let view = view.clone();
                                    Button::new("commit-include-all-v2")
                                        .outline()
                                        .compact()
                                        .rounded(px(7.0))
                                        .label("Include All")
                                        .tooltip("Include every changed file in the next revision.")
                                        .disabled(self.git_action_loading)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.include_all_files_for_commit(cx);
                                            });
                                        })
                                })
                            }),
                    )
                    .child(
                        Input::new(&self.commit_input_state)
                            .h(px(82.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(cx.theme().border.opacity(if is_dark { 0.92 } else { 0.78 }))
                            .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                                0.24
                            } else {
                                0.12
                            })))
                            .disabled(self.git_action_loading),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_1()
                            .flex_wrap()
                            .child({
                                let view = view.clone();
                                Button::new("commit-staged-v2")
                                    .primary()
                                    .rounded(px(7.0))
                                    .label("Create Revision")
                                    .tooltip("Create a new revision from included files using the message above.")
                                    .disabled(commit_disabled)
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.commit_from_input(window, cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("push-bookmark-revisions-v2")
                                    .outline()
                                    .rounded(px(7.0))
                                    .label(push_revisions_label)
                                    .tooltip(push_revisions_tooltip)
                                    .disabled(push_revisions_disabled)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.push_current_bookmark_revisions(cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("describe-tip-revision-v2")
                                    .outline()
                                    .rounded(px(7.0))
                                    .label("Edit Working Revision")
                                    .tooltip("Rewrite the tip revision description for the active bookmark.")
                                    .disabled(describe_tip_disabled)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.describe_current_bookmark_from_input(cx);
                                        });
                                    })
                            }),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_h(px(28.0))
                            .px_2()
                            .py_1()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(cx.theme().border.opacity(if is_dark { 0.92 } else { 0.76 }))
                            .bg(cx.theme().secondary.opacity(if is_dark { 0.42 } else { 0.56 }))
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().foreground.opacity(0.90))
                            .whitespace_normal()
                            .child(last_commit_text),
                    ),
            )
            .child(self.render_revision_stack_panel(cx))
            .into_any_element()
    }
}
