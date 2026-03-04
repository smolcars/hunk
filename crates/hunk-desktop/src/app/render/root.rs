impl DiffViewer {
    fn render_in_app_menu_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu_bar) = self.in_app_menu_bar.clone() else {
            return div().into_any_element();
        };
        let is_dark = cx.theme().mode.is_dark();
        h_flex()
            .w_full()
            .h_8()
            .items_center()
            .px_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().title_bar.blend(
                cx.theme()
                    .muted
                    .opacity(if is_dark { 0.16 } else { 0.24 }),
            ))
            .child(div().flex_1().min_w_0().h_full().child(menu_bar))
            .into_any_element()
    }

    fn render_diff_workspace_screen(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .child(if self.sidebar_collapsed {
                self.render_diff(cx).into_any_element()
            } else {
                h_resizable("hunk-diff-workspace")
                    .child(
                        resizable_panel()
                            .size(px(300.0))
                            .size_range(px(240.0)..px(520.0))
                            .child(self.render_tree(cx)),
                    )
                    .child(resizable_panel().child(self.render_diff(cx)))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_file_workspace_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.repo_discovery_failed {
            return self.render_open_project_empty_state(cx);
        }

        if let Some(error_message) = &self.error_message {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error_message.clone()),
                )
                .into_any_element();
        }

        div()
            .size_full()
            .child(if self.sidebar_collapsed {
                self.render_file_editor(window, cx).into_any_element()
            } else {
                h_resizable("hunk-file-workspace")
                    .child(
                        resizable_panel()
                            .size(px(300.0))
                            .size_range(px(240.0)..px(520.0))
                            .child(self.render_tree(cx)),
                    )
                    .child(resizable_panel().child(self.render_file_editor(window, cx)))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_jj_workspace_screen(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.repo_discovery_failed {
            return self.render_open_project_empty_state(cx);
        }

        if let Some(error_message) = &self.error_message {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error_message.clone()),
                )
                .into_any_element();
        }

        div()
            .size_full()
            .min_h_0()
            .pb(px(APP_BOTTOM_SAFE_INSET))
            .child(self.render_jj_workspace_graph_shell(cx))
            .into_any_element()
    }

    fn render_app_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let files_selected = self.workspace_view_mode == WorkspaceViewMode::Files;
        let diff_selected = self.workspace_view_mode == WorkspaceViewMode::Diff;
        let jj_selected = self.workspace_view_mode == WorkspaceViewMode::JjWorkspace;
        let ai_selected = self.workspace_view_mode == WorkspaceViewMode::Ai;
        let workspace_label = if ai_selected {
            "Codex AI Workspace"
        } else if jj_selected {
            "JJ Graph Workspace"
        } else if files_selected {
            "Files Workspace"
        } else {
            "Review Workspace"
        };
        let active_bookmark = self
            .checked_out_bookmark_name()
            .map_or_else(|| "detached".to_string(), ToOwned::to_owned);

        h_flex()
            .w_full()
            .h_10()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .border_t_1()
            .border_color(cx.theme().border.opacity(if is_dark { 0.88 } else { 0.68 }))
            .bg(cx.theme().sidebar.blend(cx.theme().muted.opacity(if is_dark {
                0.18
            } else {
                0.22
            })))
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .when(self.workspace_view_mode.supports_sidebar_tree(), |this| {
                        this.child({
                            let view = view.clone();
                            let mut button = Button::new("footer-toggle-sidebar")
                                .compact()
                                .rounded(px(7.0))
                                .icon(
                                    Icon::new(if self.sidebar_collapsed {
                                        IconName::ChevronRight
                                    } else {
                                        IconName::ChevronLeft
                                    })
                                    .size(px(14.0)),
                                )
                                .min_w(px(30.0))
                                .h(px(28.0))
                                .tooltip(if self.sidebar_collapsed {
                                    "Show file tree (Cmd/Ctrl+B)"
                                } else {
                                    "Hide file tree (Cmd/Ctrl+B)"
                                })
                                .on_click(move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.toggle_sidebar_tree(cx);
                                        this.focus_handle.focus(window, cx);
                                    });
                                });
                            if self.sidebar_collapsed {
                                button = button.outline();
                            } else {
                                button = button.primary();
                            }
                            button.into_any_element()
                        })
                    })
                    .child({
                        let view = view.clone();
                        let mut button = Button::new("footer-workspace-files")
                            .compact()
                            .rounded(px(7.0))
                            .label("Files")
                            .min_w(px(52.0))
                            .h(px(28.0))
                            .tooltip("Switch to file view (Cmd/Ctrl+1)")
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_workspace_view_mode(WorkspaceViewMode::Files, cx);
                                    this.focus_handle.focus(window, cx);
                                });
                            });
                        if files_selected {
                            button = button.primary();
                        } else {
                            button = button.outline();
                        }
                        button.into_any_element()
                    })
                    .child({
                        let view = view.clone();
                        let mut button = Button::new("footer-workspace-diff")
                            .compact()
                            .rounded(px(7.0))
                            .label("Review")
                            .min_w(px(56.0))
                            .h(px(28.0))
                            .tooltip("Switch to review mode (Cmd/Ctrl+2)")
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_workspace_view_mode(WorkspaceViewMode::Diff, cx);
                                    this.focus_handle.focus(window, cx);
                                });
                            });
                        if diff_selected {
                            button = button.primary();
                        } else {
                            button = button.outline();
                        }
                        button.into_any_element()
                    })
                    .child({
                        let view = view.clone();
                        let mut button = Button::new("footer-workspace-jj")
                            .compact()
                            .rounded(px(7.0))
                            .label("Git (JJ)")
                            .min_w(px(52.0))
                            .h(px(28.0))
                            .tooltip("Switch to JJ graph workspace (Cmd/Ctrl+3)")
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_workspace_view_mode(
                                        WorkspaceViewMode::JjWorkspace,
                                        cx,
                                    );
                                    this.focus_handle.focus(window, cx);
                                });
                            });
                        if jj_selected {
                            button = button.primary();
                        } else {
                            button = button.outline();
                        }
                        button.into_any_element()
                    })
                    .child({
                        let view = view.clone();
                        let mut button = Button::new("footer-workspace-ai")
                            .compact()
                            .rounded(px(7.0))
                            .label("AI")
                            .min_w(px(42.0))
                            .h(px(28.0))
                            .tooltip("Switch to AI coding workspace (Cmd/Ctrl+4)")
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_workspace_view_mode(WorkspaceViewMode::Ai, cx);
                                    this.focus_handle.focus(window, cx);
                                });
                            });
                        if ai_selected {
                            button = button.primary();
                        } else {
                            button = button.outline();
                        }
                        button.into_any_element()
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(workspace_label),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Active bookmark: {active_bookmark}")),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} changed files • active bookmark: {}",
                        self.files.len(),
                        active_bookmark
                    )),
            )
            .into_any_element()
    }
}

impl Render for DiffViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_scroll_offset = self.diff_list_state.scroll_px_offset_for_scrollbar();
        if self.last_diff_scroll_offset != Some(current_scroll_offset) {
            self.last_diff_scroll_offset = Some(current_scroll_offset);
            self.last_scroll_activity_at = Instant::now();
        }
        self.frame_sample_count = self.frame_sample_count.saturating_add(1);

        v_flex()
            .size_full()
            .relative()
            .key_context("DiffViewer")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next_line_action))
            .on_action(cx.listener(Self::select_previous_line_action))
            .on_action(cx.listener(Self::extend_selection_next_line_action))
            .on_action(cx.listener(Self::extend_selection_previous_line_action))
            .on_action(cx.listener(Self::copy_selection_action))
            .on_action(cx.listener(Self::select_all_rows_action))
            .on_action(cx.listener(Self::next_hunk_action))
            .on_action(cx.listener(Self::previous_hunk_action))
            .on_action(cx.listener(Self::next_file_action))
            .on_action(cx.listener(Self::previous_file_action))
            .on_action(cx.listener(Self::next_bookmark_revision_action))
            .on_action(cx.listener(Self::previous_bookmark_revision_action))
            .on_action(cx.listener(Self::toggle_sidebar_tree_action))
            .on_action(cx.listener(Self::switch_to_files_view_action))
            .on_action(cx.listener(Self::switch_to_review_view_action))
            .on_action(cx.listener(Self::switch_to_graph_view_action))
            .on_action(cx.listener(Self::switch_to_ai_view_action))
            .on_action(cx.listener(Self::ai_new_thread_action))
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::save_current_file_action))
            .on_action(cx.listener(Self::open_settings_action))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .when(!cfg!(target_os = "macos"), |this| {
                this.child(self.render_in_app_menu_bar(cx))
            })
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(match self.workspace_view_mode {
                        WorkspaceViewMode::Files => self.render_file_workspace_screen(window, cx),
                        WorkspaceViewMode::Diff => self.render_diff_workspace_screen(cx),
                        WorkspaceViewMode::JjWorkspace => self.render_jj_workspace_screen(cx),
                        WorkspaceViewMode::Ai => self.render_ai_workspace_screen(cx),
                    }),
            )
            .child(self.render_app_footer(cx))
            .when(self.comments_preview_open, |this| {
                this.child(self.render_comments_preview(cx))
            })
            .when(self.settings_draft.is_some(), |this| {
                this.child(self.render_settings_popup(cx))
            })
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
