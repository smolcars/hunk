impl DiffViewer {
    fn render_comments_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let comments = self.comments_preview_records();
        let open_count = self.comments_open_count();
        let stale_count = self.comments_stale_count();
        let resolved_count = self.comments_resolved_count();

        v_flex()
            .absolute()
            .top(px(48.0))
            .right(px(12.0))
            .w(px(520.0))
            .h(px(520.0))
            .rounded(px(10.0))
            .border_1()
            .overflow_hidden()
            .border_color(cx.theme().border.opacity(if is_dark { 0.92 } else { 0.72 }))
            .bg(cx.theme().popover.blend(
                cx.theme()
                    .background
                    .opacity(if is_dark { 0.20 } else { 0.08 }),
            ))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.88 } else { 0.70 }))
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(format!("Comments ({open_count} open)")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Copy one or all comment bundles with context."),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child({
                                let view = view.clone();
                                Button::new("comments-copy-all-open")
                                    .compact()
                                    .outline()
                                    .rounded(px(7.0))
                                    .label("Copy All Open")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.copy_all_open_comment_bundles(cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("comments-close-preview")
                                    .compact()
                                    .ghost()
                                    .rounded(px(7.0))
                                    .label("Close")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.close_comments_preview(cx);
                                        });
                                    })
                            }),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.82 } else { 0.66 }))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Show non-open comments"),
                    )
                    .child({
                        let view = view.clone();
                        Button::new("comments-toggle-non-open")
                            .compact()
                            .outline()
                            .rounded(px(7.0))
                            .label(if self.comments_show_non_open { "On" } else { "Off" })
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_comments_show_non_open(!this.comments_show_non_open, cx);
                                });
                            })
                    }),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.82 } else { 0.66 }))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Bulk actions"),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child({
                                let view = view.clone();
                                Button::new("comments-bulk-reopen-stale")
                                    .compact()
                                    .outline()
                                    .rounded(px(7.0))
                                    .label(format!("Reopen stale ({stale_count})"))
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.reopen_all_stale_comments(cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("comments-bulk-resolve-stale")
                                    .compact()
                                    .outline()
                                    .rounded(px(7.0))
                                    .label(format!("Resolve stale ({stale_count})"))
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.resolve_all_stale_comments(cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("comments-bulk-delete-resolved")
                                    .compact()
                                    .ghost()
                                    .rounded(px(7.0))
                                    .label(format!("Delete resolved ({resolved_count})"))
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.delete_all_resolved_comments(cx);
                                        });
                                    })
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .when(comments.is_empty(), |this| {
                        this.child(
                            div()
                                .px_3()
                                .py_4()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("No comments in this scope."),
                        )
                    })
                    .children(comments.into_iter().enumerate().map(|(ix, comment)| {
                        let comment_id = comment.id.clone();
                        let jump_id = comment_id.clone();
                        let reopen_id = comment_id.clone();
                        let copy_id = comment_id.clone();
                        let delete_id = comment_id.clone();
                        let line_hint = format!(
                            "old {} | new {}",
                            comment
                                .old_line
                                .map(|line| line.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            comment
                                .new_line
                                .map(|line| line.to_string())
                                .unwrap_or_else(|| "-".to_string())
                        );
                        let status_text = Self::comment_status_label(comment.status).to_string();
                        let status_color = match comment.status {
                            CommentStatus::Open => cx.theme().success,
                            CommentStatus::Stale => cx.theme().warning,
                            CommentStatus::Resolved => cx.theme().muted_foreground,
                        };

                        v_flex()
                            .id(("comments-preview-item", ix))
                            .gap_1()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(if is_dark { 0.74 } else { 0.58 }))
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_2()
                                            .child(
                                                v_flex()
                                                    .min_w_0()
                                                    .gap_0p5()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_semibold()
                                                            .text_color(cx.theme().foreground)
                                                            .truncate()
                                                            .child(comment.file_path),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .child(line_hint),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_semibold()
                                                    .text_color(status_color)
                                                    .child(status_text),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_2()
                                            .flex_wrap()
                                            .child({
                                                let view = view.clone();
                                                Button::new(("comments-jump", ix))
                                                    .compact()
                                                    .ghost()
                                                    .rounded(px(7.0))
                                                    .label("Go to")
                                                    .on_click(move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.jump_to_comment_by_id(
                                                                jump_id.clone(),
                                                                cx,
                                                            );
                                                        });
                                                    })
                                            })
                                            .child({
                                                let view = view.clone();
                                                Button::new(("comments-copy", ix))
                                                    .compact()
                                                    .outline()
                                                    .rounded(px(7.0))
                                                    .label("Copy")
                                                    .on_click(move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.copy_comment_bundle_by_id(
                                                                copy_id.clone(),
                                                                cx,
                                                            );
                                                        });
                                                    })
                                            })
                                            .when(comment.status != CommentStatus::Open, |this| {
                                                this.child({
                                                    let view = view.clone();
                                                    Button::new(("comments-reopen", ix))
                                                        .compact()
                                                        .outline()
                                                        .rounded(px(7.0))
                                                        .label("Reopen")
                                                        .on_click(move |_, _, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.reopen_comment_by_id(
                                                                    reopen_id.clone(),
                                                                    cx,
                                                                );
                                                            });
                                                        })
                                                })
                                            })
                                            .child({
                                                let view = view.clone();
                                                Button::new(("comments-delete", ix))
                                                    .compact()
                                                    .ghost()
                                                    .rounded(px(7.0))
                                                    .label("Delete")
                                                    .on_click(move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.delete_comment_by_id(
                                                                delete_id.clone(),
                                                                cx,
                                                            );
                                                        });
                                                    })
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .whitespace_normal()
                                    .text_color(cx.theme().foreground)
                                    .child(comment.comment_text),
                            )
                            .into_any_element()
                    })),
            )
            .when_some(self.comment_status_message.as_ref(), |this, message| {
                this.child(
                    div()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(cx.theme().border.opacity(if is_dark { 0.82 } else { 0.66 }))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(message.clone()),
                )
            })
            .into_any_element()
    }

    fn render_row_comment_affordance(&self, row_ix: usize, cx: &mut Context<Self>) -> AnyElement {
        if !self.row_supports_comments(row_ix) {
            return div().into_any_element();
        }

        let open_count = self.row_open_comment_count(row_ix);
        let hovered = self.hovered_comment_row == Some(row_ix);
        let editing = self.active_comment_editor_row == Some(row_ix);
        if !hovered && !editing && open_count == 0 {
            return div().into_any_element();
        }

        let view = cx.entity();
        let stable_row_id = self.diff_row_stable_id(row_ix);
        h_flex()
            .absolute()
            .top(px(4.0))
            .right(px(8.0))
            .items_center()
            .gap_1()
            .child(if open_count > 0 {
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .text_xs()
                    .font_semibold()
                    .bg(cx.theme().primary.opacity(if cx.theme().mode.is_dark() {
                        0.34
                    } else {
                        0.18
                    }))
                    .text_color(cx.theme().primary_foreground)
                    .child(open_count.to_string())
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(
                Button::new(("row-note", stable_row_id))
                    .compact()
                    .outline()
                    .rounded(px(6.0))
                    .label("Note")
                    .on_click(move |_, window, cx| {
                        view.update(cx, |this, cx| {
                            this.open_comment_editor_for_row(row_ix, window, cx);
                        });
                    }),
            )
            .into_any_element()
    }

    fn render_row_comment_editor(&self, row_ix: usize, cx: &mut Context<Self>) -> AnyElement {
        if self.active_comment_editor_row != Some(row_ix) {
            return div().into_any_element();
        }

        let view = cx.entity();
        let anchor = self.build_row_comment_anchor(row_ix);
        let file_path = anchor
            .as_ref()
            .map(|anchor| anchor.file_path.clone())
            .unwrap_or_else(|| "file".to_string());
        let line_hint = anchor.as_ref().map_or_else(
            || "old - | new -".to_string(),
            |anchor| {
                format!(
                    "old {} | new {}",
                    anchor
                        .old_line
                        .map(|line| line.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    anchor
                        .new_line
                        .map(|line| line.to_string())
                        .unwrap_or_else(|| "-".to_string())
                )
            },
        );
        let is_dark = cx.theme().mode.is_dark();

        v_flex()
            .w(px(380.0))
            .max_w(px(420.0))
            .gap_2()
            .px_2p5()
            .py_2()
            .rounded(px(9.0))
            .border_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.74 }))
            .bg(cx
                .theme()
                .popover
                .blend(
                    cx.theme()
                        .muted
                        .opacity(if is_dark { 0.16 } else { 0.10 }),
                ))
            .child(
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(file_path),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(line_hint),
                    ),
            )
            .child(
                Input::new(&self.comment_input_state)
                    .rounded(px(8.0))
                    .h(px(64.0))
                    .border_1()
                    .border_color(cx.theme().border.opacity(if is_dark { 0.88 } else { 0.72 }))
                    .bg(cx.theme().background.blend(
                        cx.theme()
                            .muted
                            .opacity(if is_dark { 0.20 } else { 0.08 }),
                    )),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child({
                        let view = view.clone();
                        Button::new(("comment-editor-cancel", row_ix))
                            .compact()
                            .outline()
                            .rounded(px(7.0))
                            .label("Cancel")
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.cancel_comment_editor(window, cx);
                                });
                            })
                    })
                    .child({
                        let view = view.clone();
                        Button::new(("comment-editor-save", row_ix))
                            .compact()
                            .primary()
                            .rounded(px(7.0))
                            .label("Save Comment")
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.save_active_comment(window, cx);
                                });
                            })
                    }),
            )
            .when_some(self.comment_status_message.as_ref(), |this, message| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(message.clone()),
                )
            })
            .into_any_element()
    }
}
