impl DiffViewer {
    fn render_tree_workspace_screen(
        &mut self,
        resize_id: &'static str,
        sidebar_collapsed: bool,
        surface: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .size_full()
            .child(if sidebar_collapsed {
                surface
            } else {
                h_resizable(resize_id)
                    .child(
                        resizable_panel()
                            .size(px(300.0))
                            .size_range(px(240.0)..px(520.0))
                            .child(self.render_tree(cx)),
                    )
                    .child(resizable_panel().child(surface))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_linux_client_title_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let menu_bar = self.in_app_menu_bar.clone();
        let is_dark = cx.theme().mode.is_dark();

        TitleBar::new()
            .child(
                h_flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .gap_3()
                    .pr_2()
                    .child(
                        h_flex().gap_2().items_center().flex_shrink_0().child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded(px(6.0))
                                .bg(hunk_blend(
                                    cx.theme().title_bar,
                                    cx.theme().accent,
                                    is_dark,
                                    0.18,
                                    0.10,
                                ))
                                .border_1()
                                .border_color(hunk_opacity(cx.theme().border, is_dark, 0.72, 0.52))
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Hunk"),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .when_some(menu_bar, |this, menu_bar| this.child(menu_bar)),
                    ),
            )
            .into_any_element()
    }

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
            .bg(hunk_blend(
                cx.theme().title_bar,
                cx.theme().muted,
                is_dark,
                0.16,
                0.24,
            ))
            .child(div().flex_1().min_w_0().h_full().child(menu_bar))
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

        let surface = self.render_file_editor(window, cx);
        self.render_tree_workspace_screen(
            "hunk-file-workspace",
            self.files_sidebar_collapsed,
            surface,
            cx,
        )
    }

    fn render_git_workspace_screen(&mut self, cx: &mut Context<Self>) -> AnyElement {
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

        let is_dark = cx.theme().mode.is_dark();
        let show_loading_overlay =
            self.git_workspace_loading && !self.git_workflow_ready_for_panel();
        let terminal_state = self.files_terminal_panel_state();
        let view = cx.entity();

        v_flex()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .pb(px(APP_BOTTOM_SAFE_INSET))
                    .child(self.render_git_workspace_panel(cx))
                    .when(show_loading_overlay, |this| {
                        this.child(render_git_workspace_loading_overlay(is_dark, cx))
                    })
                    .when_some(self.ai_git_progress.clone(), |this, progress| {
                        this.child(render_ai_git_progress_overlay(&progress, is_dark, cx))
                    }),
            )
            .when(terminal_state.open, |this| {
                this.child(
                    self.render_workspace_terminal_panel(view, &terminal_state, is_dark, cx)
                        .unwrap_or_else(|| div().into_any_element()),
                )
            })
            .into_any_element()
    }

    fn render_app_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let render_started_at = Instant::now();
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let files_selected = self.workspace_view_mode == WorkspaceViewMode::Files;
        let diff_selected = self.workspace_view_mode == WorkspaceViewMode::Diff;
        let git_selected = self.workspace_view_mode == WorkspaceViewMode::GitWorkspace;
        let ai_selected = self.workspace_view_mode == WorkspaceViewMode::Ai;
        let review_file_count = self.active_diff_file_count();
        let workspace_label = if ai_selected {
            "Codex AI Workspace"
        } else if git_selected {
            "Git Workspace"
        } else if files_selected {
            "Files Workspace"
        } else {
            "Review Workspace"
        };
        let active_branch = self
            .primary_checked_out_branch_name()
            .map_or_else(|| "detached".to_string(), ToOwned::to_owned);
        let footer_summary = if git_selected {
            if self.git_workspace.branch_has_upstream {
                format!(
                    "{} changed files • {} ahead • {} behind",
                    self.git_workspace.files.len(),
                    self.git_workspace.branch_ahead_count,
                    self.git_workspace.branch_behind_count
                )
            } else {
                format!(
                    "{} changed files • branch not published",
                    self.git_workspace.files.len()
                )
            }
        } else if self.workspace_view_mode == WorkspaceViewMode::Diff {
            format!(
                "{} compared files • {} -> {}",
                review_file_count,
                self.review_compare_source_label(self.review_left_source_id.as_deref()),
                self.review_compare_source_label(self.review_right_source_id.as_deref())
            )
        } else {
            format!(
                "{} changed files • active branch: {}",
                self.files.len(),
                active_branch
            )
        };
        let active_terminal_kind = self.active_workspace_terminal_kind();
        let terminal_open =
            active_terminal_kind.is_some_and(|kind| self.workspace_terminal_open(kind));
        let terminal_shortcut = ai_preferred_shortcut_label(
            self.config
                .keyboard_shortcuts
                .toggle_ai_terminal_drawer
                .as_slice(),
        );
        let terminal_tooltip = terminal_shortcut.as_ref().map_or_else(
            || {
                if terminal_open {
                    "Hide terminal".to_string()
                } else {
                    "Show terminal".to_string()
                }
            },
            |shortcut| {
                if terminal_open {
                    format!("Hide terminal ({shortcut})")
                } else {
                    format!("Show terminal ({shortcut})")
                }
            },
        );
        let footer_update_tooltip = match &self.update_status {
            UpdateStatus::Checking => "Checking for updates...".to_string(),
            UpdateStatus::Downloading { version } => {
                format!("Downloading Hunk {version}...")
            }
            UpdateStatus::ReadyToRestart { version } => {
                format!("Restart to update Hunk {version}")
            }
            UpdateStatus::Installing { version } => {
                format!("Installing Hunk {version}...")
            }
            _ => String::new(),
        };
        let footer_update_button_visible = matches!(
            self.update_status,
            UpdateStatus::Checking
                | UpdateStatus::Downloading { .. }
                | UpdateStatus::ReadyToRestart { .. }
        );
        let footer_update_ready = matches!(self.update_status, UpdateStatus::ReadyToRestart { .. });
        let footer_update_loading = matches!(
            self.update_status,
            UpdateStatus::Checking | UpdateStatus::Downloading { .. }
        );
        let footer_update_label = footer_update_ready.then_some("Restart to update");

        let element = h_flex()
            .w_full()
            .h_10()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .border_t_1()
            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.88, 0.68))
            .bg(hunk_blend(
                cx.theme().sidebar,
                cx.theme().muted,
                is_dark,
                0.18,
                0.22,
            ))
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .when(self.active_sidebar_collapsed().is_some(), |this| {
                        this.child({
                            let view = view.clone();
                            let sidebar_collapsed = self.active_sidebar_collapsed().unwrap_or(false);
                            let sidebar_label = self.active_sidebar_label().unwrap_or("sidebar");
                            let mut button = Button::new("footer-toggle-sidebar")
                                .compact()
                                .rounded(px(7.0))
                                .icon(
                                    Icon::new(if sidebar_collapsed {
                                        IconName::ChevronRight
                                    } else {
                                        IconName::ChevronLeft
                                    })
                                    .size(px(14.0)),
                                )
                                .min_w(px(30.0))
                                .h(px(28.0))
                                .tooltip(if sidebar_collapsed {
                                    format!("Show {sidebar_label} (Cmd/Ctrl+B)")
                                } else {
                                    format!("Hide {sidebar_label} (Cmd/Ctrl+B)")
                                })
                                .on_click(move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.toggle_active_sidebar(cx);
                                        this.focus_handle.focus(window, cx);
                                    });
                                });
                            if sidebar_collapsed {
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
                            .icon(Icon::new(HunkIconName::FolderTree).size(px(14.0)))
                            .min_w(px(36.0))
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
                            .icon(Icon::new(HunkIconName::FileDiff).size(px(14.0)))
                            .min_w(px(36.0))
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
                        let mut button = Button::new("footer-workspace-git")
                            .compact()
                            .rounded(px(7.0))
                            .icon(Icon::new(HunkIconName::GitBranch).size(px(14.0)))
                            .min_w(px(36.0))
                            .h(px(28.0))
                            .tooltip("Switch to Git workspace (Cmd/Ctrl+3)")
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_workspace_view_mode(
                                        WorkspaceViewMode::GitWorkspace,
                                        cx,
                                    );
                                    this.focus_handle.focus(window, cx);
                                });
                            });
                        if git_selected {
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
                            .icon(Icon::new(HunkIconName::BotMessageSquare).size(px(14.0)))
                            .min_w(px(36.0))
                            .h(px(28.0))
                            .tooltip("Switch to AI coding workspace (Cmd/Ctrl+4)")
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.activate_ai_workspace(window, cx);
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
                            .child(format!("Active branch: {active_branch}")),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .children(footer_update_button_visible.then(|| {
                        let view = view.clone();
                        let mut button = Button::new("footer-update-status")
                            .compact()
                            .rounded(px(7.0))
                            .icon(Icon::new(HunkIconName::RotateCcw).size(px(14.0)))
                            .min_w(if footer_update_ready {
                                px(156.0)
                            } else {
                                px(30.0)
                            })
                            .h(px(28.0))
                            .tooltip(footer_update_tooltip.clone())
                            .loading(footer_update_loading)
                            .disabled(footer_update_loading)
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.install_available_update(Some(window), cx);
                                });
                            });
                        if let Some(label) = footer_update_label {
                            button = button.label(label);
                        }
                        if footer_update_ready {
                            button = button.primary();
                        } else {
                            button = button.outline();
                        }

                        div()
                            .relative()
                            .child(button)
                            .when(footer_update_ready, |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .top(px(2.0))
                                        .right(px(2.0))
                                        .w(px(8.0))
                                        .h(px(8.0))
                                        .rounded_full()
                                        .bg(hunk_blend(
                                            cx.theme().warning,
                                            cx.theme().accent,
                                            is_dark,
                                            0.28,
                                            0.18,
                                        ))
                                        .border_1()
                                        .border_color(hunk_opacity(
                                            cx.theme().background,
                                            is_dark,
                                            0.95,
                                            0.95,
                                        )),
                                )
                            })
                            .into_any_element()
                    }))
                    .when_some(active_terminal_kind, |this, kind| {
                        this.child({
                            let view = view.clone();
                            let mut button = Button::new("footer-toggle-terminal")
                                .compact()
                                .rounded(px(7.0))
                                .icon(Icon::new(IconName::SquareTerminal).size(px(14.0)))
                                .min_w(px(30.0))
                                .h(px(28.0))
                                .tooltip(terminal_tooltip.clone())
                                .on_click(move |_, window, cx| {
                                    view.update(cx, |this, cx| match kind {
                                        WorkspaceTerminalKind::Ai => {
                                            this.ai_toggle_terminal_drawer_action(cx);
                                        }
                                        WorkspaceTerminalKind::Files => {
                                            this.files_toggle_terminal_drawer_action(window, cx);
                                        }
                                    });
                                });
                            if terminal_open {
                                button = button.primary();
                            } else {
                                button = button.outline();
                            }
                            button.into_any_element()
                        })
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(footer_summary),
                    ),
            )
            .into_any_element();
        if ai_selected {
            self.record_ai_footer_render_timing(render_started_at.elapsed());
        }
        element
    }
}

impl Render for DiffViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let render_started_at = Instant::now();
        let current_scroll_offset = self.current_review_surface_scroll_offset();
        if self.review_surface.last_diff_scroll_offset != Some(current_scroll_offset) {
            self.review_surface.last_diff_scroll_offset = Some(current_scroll_offset);
            self.last_scroll_activity_at = Instant::now();
            if let Some(visible_state) = self.refresh_review_surface_snapshot()
                && self.workspace_view_mode == WorkspaceViewMode::Diff
                && let Some(top_row) = visible_state.top_row
            {
                self.sync_selected_file_from_visible_row(top_row, cx);
            }
        } else if self.uses_review_workspace_sections_surface() {
            self.refresh_review_surface_snapshot();
        }
        let ai_selected = self.workspace_view_mode == WorkspaceViewMode::Ai;
        if ai_selected && self.ai_workspace_session.is_some() {
            let current_ai_scroll_offset = self.current_ai_workspace_surface_scroll_offset();
            if self.ai_workspace_surface_last_scroll_offset != Some(current_ai_scroll_offset) {
                self.ai_workspace_surface_last_scroll_offset = Some(current_ai_scroll_offset);
                self.last_scroll_activity_at = Instant::now();
                self.refresh_ai_timeline_follow_output_from_scroll();
            }
            if self.ai_inline_review_is_open() {
                let current_inline_review_scroll_offset =
                    self.current_ai_inline_review_surface_scroll_offset();
                if self.ai_inline_review_surface.last_diff_scroll_offset
                    != Some(current_inline_review_scroll_offset)
                {
                    self.ai_inline_review_surface.last_diff_scroll_offset =
                        Some(current_inline_review_scroll_offset);
                    self.last_scroll_activity_at = Instant::now();
                }
            }
        }
        if self.ignore_next_frame_sample {
            self.ignore_next_frame_sample = false;
        } else {
            self.frame_sample_count = self.frame_sample_count.saturating_add(1);
        }
        let ai_view_state = ai_selected.then(|| self.visible_ai_frame_state());
        let show_linux_client_title_bar = cfg!(target_os = "linux")
            && matches!(window.window_decorations(), Decorations::Client { .. });
        let element = v_flex()
            .size_full()
            .relative()
            .key_context(self.workspace_view_mode.root_key_context())
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
            .on_action(cx.listener(Self::view_current_review_file_action))
            .on_action(cx.listener(Self::toggle_sidebar_tree_action))
            .on_action(cx.listener(Self::switch_to_files_view_action))
            .on_action(cx.listener(Self::switch_to_review_view_action))
            .on_action(cx.listener(Self::switch_to_git_view_action))
            .on_action(cx.listener(Self::switch_to_ai_view_action))
            .on_action(cx.listener(Self::ai_toggle_terminal_drawer_shortcut_action))
            .on_action(cx.listener(Self::workspace_terminal_new_tab_action))
            .on_action(cx.listener(Self::workspace_terminal_close_tab_action))
            .on_action(cx.listener(Self::workspace_terminal_next_tab_action))
            .on_action(cx.listener(Self::workspace_terminal_previous_tab_action))
            .on_action(cx.listener(Self::ai_new_thread_action))
            .on_action(cx.listener(Self::ai_new_worktree_thread_shortcut_action))
            .on_action(cx.listener(Self::ai_open_working_tree_diff_viewer_action))
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::quick_open_file_action))
            .on_action(cx.listener(Self::save_current_file_action))
            .on_action(cx.listener(Self::next_editor_tab_action))
            .on_action(cx.listener(Self::previous_editor_tab_action))
            .on_action(cx.listener(Self::close_editor_tab_action))
            .on_action(cx.listener(Self::check_for_updates_action))
            .on_action(cx.listener(Self::open_about_hunk_action))
            .on_action(cx.listener(Self::open_settings_action))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .when(show_linux_client_title_bar, |this| {
                this.child(self.render_linux_client_title_bar(cx))
            })
            .when(
                !cfg!(target_os = "macos") && !show_linux_client_title_bar,
                |this| this.child(self.render_in_app_menu_bar(cx)),
            )
            .child(self.render_toolbar(ai_view_state.as_ref(), cx))
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(match self.workspace_view_mode {
                        WorkspaceViewMode::Files => self.render_file_workspace_screen(window, cx),
                        WorkspaceViewMode::Diff => self.render_diff_workspace_screen(window, cx),
                        WorkspaceViewMode::GitWorkspace => self.render_git_workspace_screen(cx),
                        WorkspaceViewMode::Ai => {
                            self.render_ai_workspace_screen(ai_view_state.clone(), cx)
                        }
                    }),
            )
            .child(self.render_app_footer(cx))
            .when(
                self.comments_preview_open && self.workspace_view_mode == WorkspaceViewMode::Diff,
                |this| this.child(self.render_comments_preview(cx)),
            )
            .when(self.file_quick_open_visible, |this| {
                this.child(self.render_file_quick_open_popup(window, cx))
            })
            .when(self.settings_draft.is_some(), |this| {
                this.child(self.render_settings_popup(cx))
            })
            .when_some(self.render_workspace_text_context_menu(cx), |this, menu| {
                this.child(menu)
            })
            .when_some(self.render_browser_context_menu(cx), |this, menu| {
                this.child(menu)
            })
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
            .into_any_element();
        if ai_selected {
            self.record_ai_app_render_timing(render_started_at.elapsed());
        }
        element
    }
}
