impl DiffViewer {
    fn render_jj_graph_operations_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let branch_syncable = self.can_run_active_bookmark_actions();
        let show_sync = branch_syncable && self.branch_has_upstream;
        let show_publish = branch_syncable && !self.branch_has_upstream;
        let show_push = branch_syncable && self.branch_has_upstream;
        let sync_disabled = !self.can_sync_current_bookmark();
        let push_or_publish_disabled = !self.can_push_or_publish_current_bookmark();
        let active_review_blocker = self.active_review_action_blocker();
        let review_url_disabled = active_review_blocker.is_some();
        let recovery_candidate = self.latest_working_copy_recovery_candidate_for_active_bookmark();
        let pending_switch = self.pending_bookmark_switch();
        let action_label = if show_publish {
            "Publish Bookmark"
        } else {
            "Push Bookmark"
        };
        let action_tooltip = if show_publish {
            "Publish this local bookmark to remote and start tracking it."
        } else {
            "Push new local revisions on this bookmark to its tracked remote."
        };
        let active_bookmark_label = self
            .checked_out_bookmark_name()
            .map_or_else(|| "detached".to_string(), ToOwned::to_owned);
        let sync_state_label = if !branch_syncable {
            "Detached".to_string()
        } else if self.branch_has_upstream {
            if self.branch_ahead_count > 0 {
                "Needs push".to_string()
            } else {
                "Up to date".to_string()
            }
        } else {
            "Not published".to_string()
        };
        let last_commit_text = self
            .last_commit_subject
            .as_deref()
            .map(str::trim_end)
            .filter(|text| !text.is_empty())
            .unwrap_or("No commits yet");
        let included_count = self.included_commit_file_count();
        let total_count = self.files.len();
        let commit_message_present = !self.commit_input_state.read(cx).value().trim().is_empty();
        let commit_disabled =
            self.git_action_loading || !commit_message_present || included_count == 0;
        let describe_tip_disabled = self.git_action_loading
            || !commit_message_present
            || !branch_syncable
            || self.bookmark_revisions.is_empty();

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
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Bookmarks & Revisions"),
                    )
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
                                    .child(format!("Active bookmark: {active_bookmark_label}")),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded(px(6.0))
                                    .text_xs()
                                    .font_semibold()
                                    .bg(cx.theme().secondary.opacity(if is_dark { 0.54 } else { 0.70 }))
                                    .text_color(cx.theme().foreground)
                                    .child(sync_state_label),
                            ),
                    ),
            )
            .when_some(self.git_status_message.as_ref(), |this, message| {
                this.child(
                    div()
                        .w_full()
                        .px_2()
                        .py_1()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.70 }))
                        .bg(cx.theme().background.blend(cx.theme().muted.opacity(if is_dark {
                            0.24
                        } else {
                            0.32
                        })))
                        .text_xs()
                        .font_medium()
                        .text_color(cx.theme().muted_foreground)
                        .whitespace_normal()
                        .child(message.clone()),
                )
            })
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
                                    Button::new("confirm-switch-move")
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
                                    Button::new("confirm-switch-snapshot")
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
                                    Button::new("cancel-pending-switch")
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
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .flex_wrap()
                    .child({
                        let view = view.clone();
                        Button::new("branch-picker-label")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .bg(cx.theme().secondary.opacity(if is_dark { 0.50 } else { 0.70 }))
                            .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
                            .label(active_bookmark_label.clone())
                            .tooltip("Open bookmark list to switch, move changes, create, or rename bookmarks.")
                            .disabled(self.git_action_loading)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.toggle_bookmark_picker(cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        let mut button = Button::new("branch-picker-toggle")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .min_w(px(24.0))
                            .h(px(24.0))
                            .icon(
                                Icon::new(if self.branch_picker_open {
                                    IconName::ChevronUp
                                } else {
                                    IconName::ChevronDown
                                })
                                .size(px(12.0)),
                            )
                            .tooltip(if self.branch_picker_open {
                                "Hide bookmark menu"
                            } else {
                                "Show bookmark menu"
                            })
                            .disabled(self.git_action_loading)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.toggle_bookmark_picker(cx);
                                });
                            });

                        if self.branch_picker_open {
                            button = button.primary();
                        }

                        button.into_any_element()
                    })
                    .when(show_sync, |this| {
                        this.child({
                            let view = view.clone();
                            Button::new("sync-branch")
                                .outline()
                                .compact()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(7.0))
                                .label("Sync Bookmark")
                                .tooltip("Fetch and update this bookmark from its upstream remote.")
                                .disabled(sync_disabled)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.sync_current_bookmark_from_remote(cx);
                                    });
                                })
                        })
                    })
                    .when(show_publish || show_push, |this| {
                        this.child({
                            let view = view.clone();
                            Button::new("publish-or-push")
                                .primary()
                                .compact()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(7.0))
                                .label(action_label)
                                .tooltip(action_tooltip)
                                .disabled(push_or_publish_disabled)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.push_or_publish_current_bookmark(cx);
                                    });
                                })
                            })
                    })
                    .child({
                        let view = view.clone();
                        let blocker = active_review_blocker.clone();
                        Button::new("open-review-url")
                            .primary()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Open PR/MR")
                            .tooltip(blocker.clone().unwrap_or_else(|| {
                                "Open a prefilled pull/merge request page for the active bookmark."
                                    .to_string()
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
                        Button::new("copy-review-url")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Copy Review URL")
                            .tooltip(blocker.unwrap_or_else(|| {
                                "Copy a prefilled pull/merge request URL for the active bookmark."
                                    .to_string()
                            }))
                            .disabled(review_url_disabled)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.copy_current_bookmark_review_url(cx);
                                });
                            })
                    })
                    .when_some(active_review_blocker, |this, reason| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .whitespace_normal()
                                .child(format!("PR/MR unavailable: {reason}")),
                        )
                    }),
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
                                .flex_wrap()
                                .child({
                                    let view = view.clone();
                                    Button::new("recover-working-copy")
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
                                    Button::new("discard-working-copy-recovery")
                                        .outline()
                                        .compact()
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(7.0))
                                        .label("Discard Recovery Record")
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
            .when(self.branch_picker_open, |this| {
                this.child(self.render_branch_picker_panel(cx))
            })
            .child(self.render_workspace_changes_panel(cx))
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
                            Button::new("commit-include-all")
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
            .child(self.render_revision_stack_panel(cx))
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
                        Button::new("commit-staged")
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
                        Button::new("describe-tip-revision")
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
                    .child(last_commit_text.to_string()),
            )
            .into_any_element()
    }

    fn render_revision_stack_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let revisions = &self.bookmark_revisions;
        let actions_enabled = self.can_run_active_bookmark_actions();
        let can_abandon_tip =
            !self.git_action_loading && actions_enabled && !revisions.is_empty();
        let can_squash_tip =
            !self.git_action_loading && actions_enabled && revisions.len() >= 2;
        let can_reorder_tip =
            !self.git_action_loading && actions_enabled && revisions.len() >= 2;
        let can_undo_all_working_copy = !self.git_action_loading && !self.files.is_empty();
        let can_redo_operation = !self.git_action_loading && self.can_redo_operation;

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
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Revision Stack"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{}", revisions.len())),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .flex_wrap()
                    .child({
                        let view = view.clone();
                        Button::new("reorder-top-two-revisions")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Move Tip Down")
                            .tooltip("Reorder the stack so the current tip becomes second and the previous revision becomes tip.")
                            .disabled(!can_reorder_tip)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.reorder_current_bookmark_tip_older(cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        Button::new("squash-tip-revision")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Squash Into Parent")
                            .tooltip("Combine tip revision changes into its parent revision.")
                            .disabled(!can_squash_tip)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.squash_current_bookmark_tip_into_parent(cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        Button::new("abandon-tip-revision")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Drop Tip Revision")
                            .tooltip("Abandon and remove the current tip revision from the stack.")
                            .disabled(!can_abandon_tip)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.abandon_current_bookmark_tip(cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        Button::new("undo-all-working-copy-changes")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Undo All")
                            .tooltip("Discard all working-copy changes using jj restore.")
                            .bg(cx.theme().warning.opacity(if is_dark { 0.24 } else { 0.14 }))
                            .border_color(cx.theme().warning.opacity(if is_dark { 0.82 } else { 0.60 }))
                            .text_color(cx.theme().foreground)
                            .disabled(!can_undo_all_working_copy)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.undo_all_working_copy_changes(cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        Button::new("redo-last-operation")
                            .outline()
                            .compact()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(7.0))
                            .label("Redo")
                            .tooltip("Redo the most recently undone JJ operation.")
                            .bg(cx.theme().accent.opacity(if is_dark { 0.28 } else { 0.16 }))
                            .border_color(
                                cx.theme().accent.opacity(if is_dark { 0.78 } else { 0.58 }),
                            )
                            .text_color(cx.theme().foreground)
                            .disabled(!can_redo_operation)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.redo_last_operation(cx);
                                });
                            })
                    }),
            )
            .child({
                if revisions.is_empty() {
                    return div()
                        .w_full()
                        .px_1()
                        .py_0p5()
                        .rounded(px(6.0))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No revisions for this bookmark.")
                        .into_any_element();
                }

                v_flex()
                    .w_full()
                    .max_h(px(180.0))
                    .overflow_y_scrollbar()
                    .gap_0p5()
                    .children(revisions.iter().enumerate().map(|(ix, revision)| {
                        let short_id = revision.id.chars().take(12).collect::<String>();
                        let row_bg = if ix == 0 {
                            cx.theme().accent.opacity(if is_dark { 0.18 } else { 0.10 })
                        } else {
                            cx.theme().background.opacity(0.0)
                        };

                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_1()
                            .px_1()
                            .py_0p5()
                            .rounded(px(6.0))
                            .bg(row_bg)
                            .child(
                                div()
                                    .px_1()
                                    .py_0p5()
                                    .rounded(px(4.0))
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(cx.theme().muted_foreground)
                                    .bg(cx.theme().muted.opacity(if is_dark { 0.32 } else { 0.42 }))
                                    .child(short_id),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().foreground)
                                    .child(revision.subject.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .whitespace_nowrap()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(relative_time_label(Some(revision.unix_time))),
                            )
                            .into_any_element()
                    }))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_workspace_changes_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tracked_count = self.files.iter().filter(|file| file.is_tracked()).count();
        let untracked_count = self.files.len().saturating_sub(tracked_count);
        let is_dark = cx.theme().mode.is_dark();

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
                    .child("Working Copy"),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .flex_wrap()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} files (tracked: {}, untracked: {})",
                                self.files.len(),
                                tracked_count,
                                untracked_count
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground.opacity(0.9))
                            .child("Single unified working-copy list"),
                    ),
            )
            .child({
                if self.files.is_empty() {
                    return div()
                        .w_full()
                        .px_1()
                        .py_0p5()
                        .rounded(px(6.0))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No files")
                        .into_any_element();
                }

                v_flex()
                    .w_full()
                    .max_h(px(220.0))
                    .overflow_y_scrollbar()
                    .gap_0p5()
                    .children(self.files.iter().enumerate().map(|(row_ix, file)| {
                        self.render_workspace_change_row(row_ix, file, cx)
                    }))
                    .into_any_element()
            })
            .into_any_element()
    }

}
