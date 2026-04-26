#[derive(Clone)]
struct AiThreadSidebarState {
    project_count: usize,
    threads_loading: bool,
    selected_thread_id: Option<String>,
}

struct AiTimelinePanelState {
    workspace_kind: AiWorkspaceKind,
    active_branch: String,
    workspace_label: String,
    show_worktree_base_branch_picker: bool,
    selected_worktree_base_branch: String,
    selected_thread_id: Option<String>,
    selected_thread_start_mode: Option<AiNewThreadStartMode>,
    pending_approvals: Arc<[AiPendingApproval]>,
    pending_user_inputs: Arc<[AiPendingUserInputRequest]>,
    pending_thread_start: Option<AiPendingThreadStart>,
    timeline_total_turn_count: usize,
    timeline_visible_turn_count: usize,
    timeline_hidden_turn_count: usize,
    timeline_visible_row_ids: Arc<[String]>,
    timeline_loading: bool,
    right_pane_mode: Option<AiWorkspaceRightPaneMode>,
    show_select_thread_empty_state: bool,
    show_no_turns_empty_state: bool,
    ai_publish_blocker: Option<String>,
    ai_publish_disabled: bool,
    ai_commit_and_push_loading: bool,
    ai_create_branch_and_push_loading: bool,
    ai_open_pr_disabled: bool,
    ai_open_pr_loading: bool,
    current_review_summary: Option<OpenReviewSummary>,
    ai_managed_worktree_target: Option<WorkspaceTargetSummary>,
    ai_delete_worktree_blocker: Option<String>,
    ai_delete_worktree_loading: bool,
    ai_error_message: Option<String>,
    ai_requires_openai_auth: bool,
    ai_pending_chatgpt_login_id: Option<String>,
    ai_account_connected: bool,
}

struct AiWorkspaceContentSections<'a> {
    sidebar: &'a AiThreadSidebarState,
    timeline: &'a AiTimelinePanelState,
    terminal_panel: Option<AnyElement>,
    composer_panel: AnyElement,
}

fn ai_preferred_shortcut_label(shortcuts: &[String]) -> Option<String> {
    let preferred = if cfg!(target_os = "macos") {
        shortcuts
            .iter()
            .find(|shortcut| shortcut.to_ascii_lowercase().contains("cmd"))
    } else {
        shortcuts
            .iter()
            .find(|shortcut| shortcut.to_ascii_lowercase().contains("ctrl"))
    }
    .or_else(|| shortcuts.first())?;
    Some(ai_format_shortcut_label(preferred.as_str()))
}

fn ai_format_shortcut_label(shortcut: &str) -> String {
    shortcut
        .split_whitespace()
        .map(|stroke| {
            stroke
                .split('-')
                .map(|part| match part.to_ascii_lowercase().as_str() {
                    "cmd" => "Cmd".to_string(),
                    "ctrl" => "Ctrl".to_string(),
                    "alt" => "Alt".to_string(),
                    "shift" => "Shift".to_string(),
                    "super" => "Super".to_string(),
                    "secondary" => "Secondary".to_string(),
                    "space" => "Space".to_string(),
                    "enter" => "Enter".to_string(),
                    "escape" => "Esc".to_string(),
                    "up" => "Up".to_string(),
                    "down" => "Down".to_string(),
                    "left" => "Left".to_string(),
                    "right" => "Right".to_string(),
                    "tab" => "Tab".to_string(),
                    "backspace" => "Backspace".to_string(),
                    _ => part.to_ascii_uppercase(),
                })
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn ai_new_thread_shortcut_label(start_mode: AiNewThreadStartMode) -> Option<String> {
    match start_mode {
        AiNewThreadStartMode::Local => {
            ai_preferred_shortcut_label(&["cmd-n".to_string(), "ctrl-n".to_string()])
        }
        AiNewThreadStartMode::Worktree => {
            ai_preferred_shortcut_label(&["cmd-shift-n".to_string(), "ctrl-shift-n".to_string()])
        }
    }
}

fn ai_project_open_target_icon(target: project_open::ProjectOpenTargetId) -> Icon {
    let icon = match target {
        project_open::ProjectOpenTargetId::VsCode => Icon::new(HunkIconName::VisualStudioCode),
        project_open::ProjectOpenTargetId::Cursor => Icon::new(HunkIconName::Cursor),
        project_open::ProjectOpenTargetId::Zed => Icon::new(HunkIconName::Zed),
        project_open::ProjectOpenTargetId::Xcode => Icon::new(HunkIconName::Xcode),
        project_open::ProjectOpenTargetId::AndroidStudio => Icon::new(HunkIconName::AndroidStudio),
        project_open::ProjectOpenTargetId::FileManager => Icon::new(IconName::FolderClosed),
    };
    icon.size(px(14.0))
}

fn render_ai_header_metric_chip(
    label: &'static str,
    value: String,
    accent: Hsla,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    h_flex()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded(px(999.0))
        .border_1()
        .border_color(hunk_opacity(accent, is_dark, 0.34, 0.22))
        .bg(hunk_opacity(accent, is_dark, 0.12, 0.08))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(accent)
                .child(value),
        )
        .into_any_element()
}

impl DiffViewer {
    fn render_ai_workspace_content(
        &mut self,
        view: Entity<Self>,
        sections: AiWorkspaceContentSections<'_>,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let right_pane_mode = sections.timeline.right_pane_mode;
        let right_pane_open = right_pane_mode.is_some();
        let main_content_column = v_flex()
            .size_full()
            .min_h_0()
            .child(self.render_ai_timeline_panel(
                view.clone(),
                sections.timeline,
                is_dark,
                cx,
            ))
            .child(sections.composer_panel)
            .when_some(sections.terminal_panel, |this, terminal_panel| {
                this.child(terminal_panel)
            });
        let workspace_content = if right_pane_open {
            h_resizable("hunk-ai-workspace-content-split")
                .child(
                    resizable_panel()
                        .size(px(720.0))
                        .size_range(px(420.0)..px(1280.0))
                        .child(
                            v_flex()
                                .size_full()
                                .min_h_0()
                                .min_w_0()
                                .child(main_content_column),
                        ),
                )
                .child(
                    resizable_panel()
                        .size(px(640.0))
                        .size_range(px(360.0)..px(1280.0))
                        .child(
                            div()
                                .size_full()
                                .min_h_0()
                                .min_w_0()
                                .child(self.render_ai_right_pane(
                                    view.clone(),
                                    right_pane_mode,
                                    is_dark,
                                    cx,
                                )),
                        ),
                )
                .into_any_element()
        } else {
            main_content_column.into_any_element()
        };
        v_flex()
            .size_full()
            .w_full()
            .min_h_0()
            .key_context("AiWorkspace")
            .on_action(cx.listener(Self::ai_interrupt_selected_turn_action))
            .child(
                div().flex_1().w_full().min_h_0().child(if self.ai_thread_sidebar_collapsed {
                    workspace_content
                } else {
                    h_resizable("hunk-ai-workspace")
                        .child(
                            resizable_panel()
                                .size(px(300.0))
                                .size_range(px(240.0)..px(440.0))
                                .child(self.render_ai_thread_sidebar_panel(
                                    view.clone(),
                                    sections.sidebar,
                                    is_dark,
                                    cx,
                                )),
                        )
                        .child(resizable_panel().child(workspace_content))
                        .into_any_element()
                }),
            )
            .into_any_element()
    }

    fn render_ai_thread_sidebar_panel(
        &mut self,
        view: Entity<Self>,
        state: &AiThreadSidebarState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let render_started_at = Instant::now();
        self.sync_ai_thread_sidebar_list_state();
        let list_state = self.ai_thread_sidebar_list_state.clone();
        let visible_row_count = self.ai_thread_sidebar_rows.len();
        let row_state = state.clone();
        let list_view = view.clone();
        let list = list(list_state.clone(), {
            cx.processor(move |this, ix: usize, _window, cx| {
                let Some(row) = this.ai_thread_sidebar_rows().get(ix).cloned() else {
                    return div().into_any_element();
                };
                this.render_ai_thread_sidebar_list_row(
                    list_view.clone(),
                    &row_state,
                    row,
                    ix == 0,
                    is_dark,
                    cx,
                )
            })
        })
        .size_full()
        .map(|mut list| {
            list.style().restrict_scroll_to_axis = Some(true);
            list
        })
        .with_sizing_behavior(ListSizingBehavior::Auto);

        let element = v_flex()
            .size_full()
            .min_h_0()
            .bg(cx.theme().sidebar)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Threads"),
                    )
                    .child({
                        let view = view.clone();
                        h_flex()
                            .items_center()
                            .gap_1p5()
                            .child({
                                let view = view.clone();
                                Button::new("ai-add-project")
                                    .compact()
                                    .outline()
                                    .rounded(px(999.0))
                                    .with_size(gpui_component::Size::Small)
                                    .icon(Icon::new(HunkIconName::FolderPlus).size(px(14.0)))
                                    .label("Add Project")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.open_project_picker(cx);
                                        });
                                    })
                            })
                            .child(
                                Button::new("ai-thread-refresh")
                                    .ghost()
                                    .compact()
                                    .rounded(px(999.0))
                                    .with_size(gpui_component::Size::Small)
                                    .label("Refresh")
                                    .text_color(hunk_opacity(
                                        cx.theme().muted_foreground,
                                        is_dark,
                                        0.88,
                                        0.96,
                                    ))
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_refresh_threads(cx);
                                        });
                                    }),
                            )
                    }),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .size_full()
                        .overflow_y_scrollbar()
                        .px_2()
                        .pb_3()
                        .when(state.threads_loading, |this| {
                            this.child(render_ai_thread_list_loading_skeleton(is_dark, cx))
                        })
                        .when(state.project_count == 0 && !state.threads_loading, |this| {
                            this.child(
                                v_flex().w_full().items_center().pt_8().px_3().child(
                                    div()
                                        .text_xs()
                                        .text_color(hunk_opacity(
                                            cx.theme().muted_foreground,
                                            is_dark,
                                            0.86,
                                            0.96,
                                        ))
                                        .child("No threads in this workspace yet."),
                                ),
                            )
                        })
                        .when(state.project_count > 0 && !state.threads_loading, |this| {
                            this.child(list)
                        }),
                ),
            )
            .into_any_element();
        self.record_ai_thread_sidebar_render_timing(render_started_at.elapsed(), visible_row_count);
        element
    }

    fn sync_ai_thread_sidebar_list_state(&mut self) {
        let row_count = self.ai_thread_sidebar_rows.len();
        if self.ai_thread_sidebar_row_count == row_count {
            return;
        }

        let previous_top = self.ai_thread_sidebar_list_state.logical_scroll_top();
        self.ai_thread_sidebar_list_state.reset(row_count);
        let item_ix = if row_count == 0 {
            0
        } else {
            previous_top.item_ix.min(row_count.saturating_sub(1))
        };
        let offset_in_item = if row_count == 0 || item_ix != previous_top.item_ix {
            px(0.)
        } else {
            previous_top.offset_in_item
        };
        self.ai_thread_sidebar_list_state.scroll_to(ListOffset {
            item_ix,
            offset_in_item,
        });
        self.ai_thread_sidebar_row_count = row_count;
    }

    fn render_ai_thread_sidebar_list_row(
        &self,
        view: Entity<Self>,
        state: &AiThreadSidebarState,
        row: AiThreadSidebarRow,
        is_first: bool,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let render_started_at = Instant::now();
        let (element, row_kind) = match row.kind {
            AiThreadSidebarRowKind::ProjectHeader {
                workspace_kind,
                project_root,
                project_label,
                total_thread_count,
            } => (
                self.render_ai_thread_project_header_row(
                    view,
                    workspace_kind,
                    project_root,
                    project_label,
                    total_thread_count,
                    is_first,
                    is_dark,
                    cx,
                ),
                AiPerfSidebarRowKind::ProjectHeader,
            ),
            AiThreadSidebarRowKind::Thread { thread } => {
                let bookmarked = self.ai_thread_is_bookmarked(thread.id.as_str());
                (
                    render_ai_thread_sidebar_row(
                        thread,
                        state.selected_thread_id.as_deref(),
                        bookmarked,
                        view,
                        is_dark,
                        cx,
                    ),
                    AiPerfSidebarRowKind::Thread,
                )
            }
            AiThreadSidebarRowKind::EmptyProject {
                workspace_kind,
                project_root,
            } => (
                self.render_ai_thread_project_empty_row(workspace_kind, project_root, is_dark, cx),
                AiPerfSidebarRowKind::EmptyProject,
            ),
            AiThreadSidebarRowKind::ProjectFooter {
                workspace_kind,
                project_root,
                hidden_thread_count,
                expanded,
            } => (
                self.render_ai_thread_project_footer_row(
                    view,
                    workspace_kind,
                    project_root,
                    hidden_thread_count,
                    expanded,
                    cx,
                ),
                AiPerfSidebarRowKind::ProjectFooter,
            ),
        };
        self.record_ai_thread_sidebar_row_render_timing(row_kind, render_started_at.elapsed());
        element
    }

    #[allow(clippy::too_many_arguments)]
    fn render_ai_thread_project_header_row(
        &self,
        view: Entity<Self>,
        workspace_kind: AiWorkspaceKind,
        project_root: PathBuf,
        project_label: String,
        total_thread_count: usize,
        is_first: bool,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_key = project_root.to_string_lossy().to_string();
        let thread_count_label = if total_thread_count == 1 {
            "1 thread".to_string()
        } else {
            format!("{total_thread_count} threads")
        };
        let new_thread_label =
            if let Some(shortcut) = ai_new_thread_shortcut_label(AiNewThreadStartMode::Local) {
                if workspace_kind == AiWorkspaceKind::Chats {
                    format!("New Chat  {shortcut}")
                } else {
                    format!("New Thread  {shortcut}")
                }
            } else if workspace_kind == AiWorkspaceKind::Chats {
                "New Chat".to_string()
            } else {
                "New Thread".to_string()
            };
        let new_worktree_label =
            if let Some(shortcut) = ai_new_thread_shortcut_label(AiNewThreadStartMode::Worktree) {
                format!("New Worktree  {shortcut}")
            } else {
                "New Worktree".to_string()
            };

        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_2()
            .when(!is_first, |this| {
                this.pt_3().border_t_1().border_color(hunk_opacity(
                    cx.theme().border,
                    is_dark,
                    0.64,
                    0.84,
                ))
            })
            .when(is_first, |this| this.pt_1())
            .pb_1()
            .child(
                h_flex().flex_1().min_w_0().items_start().gap_1p5().child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .truncate()
                                .child(project_label),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(thread_count_label),
                        ),
                ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(if workspace_kind == AiWorkspaceKind::Chats {
                        let view = view.clone();
                        Button::new(format!("ai-thread-project-actions-{project_key}"))
                            .compact()
                            .outline()
                            .rounded(px(999.0))
                            .with_size(gpui_component::Size::Small)
                            .px_1()
                            .icon(Icon::new(HunkIconName::NotebookPen).size(px(14.0)))
                            .tooltip(new_thread_label.clone())
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.ai_start_chat_thread_draft(window, cx);
                                });
                            })
                            .into_any_element()
                    } else {
                        let new_button_view = view.clone();
                        let new_button_project_root = project_root.clone();
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new(format!("ai-thread-project-actions-{project_key}"))
                                    .compact()
                                    .outline()
                                    .rounded(px(999.0))
                                    .with_size(gpui_component::Size::Small)
                                    .px_1()
                                    .tooltip("New thread")
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::new(HunkIconName::NotebookPen).size(px(14.0)))
                                            .child(Icon::new(IconName::ChevronDown).size(px(12.0))),
                                    )
                                    .dropdown_menu(move |menu, _, _| {
                                        menu.item(PopupMenuItem::new(new_thread_label.clone()).on_click({
                                            let view = new_button_view.clone();
                                            let project_root = new_button_project_root.clone();
                                            move |_, window, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.ai_start_thread_draft_for_project_root(
                                                        project_root.clone(),
                                                        AiNewThreadStartMode::Local,
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }
                                        }))
                                        .item(
                                            PopupMenuItem::new(new_worktree_label.clone()).on_click({
                                                let view = new_button_view.clone();
                                                let project_root = new_button_project_root.clone();
                                                move |_, window, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.ai_start_thread_draft_for_project_root(
                                                            project_root.clone(),
                                                            AiNewThreadStartMode::Worktree,
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }
                                            }),
                                        )
                                    }),
                            )
                            .child(
                                Button::new(format!("ai-thread-project-remove-{project_key}"))
                                    .compact()
                                    .danger()
                                    .rounded(px(999.0))
                                    .with_size(gpui_component::Size::Small)
                                    .px_1()
                                    .icon(Icon::new(IconName::Delete).size(px(14.0)))
                                    .tooltip("Remove project")
                                    .on_click({
                                        let view = view.clone();
                                        let project_root = project_root.clone();
                                        move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.confirm_remove_workspace_project_action(
                                                    project_root.clone(),
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }
                                    }),
                            )
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn render_ai_thread_project_empty_row(
        &self,
        workspace_kind: AiWorkspaceKind,
        project_root: PathBuf,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_key = project_root.to_string_lossy().to_string();
        div()
            .id(format!("ai-thread-project-empty-{project_key}"))
            .w_full()
            .pl_5()
            .pb_2()
            .text_xs()
            .text_color(hunk_opacity(
                cx.theme().muted_foreground,
                is_dark,
                0.84,
                0.94,
            ))
            .child(if workspace_kind == AiWorkspaceKind::Chats {
                "No chats"
            } else {
                "No threads"
            })
            .into_any_element()
    }

    fn render_ai_thread_project_footer_row(
        &self,
        view: Entity<Self>,
        _: AiWorkspaceKind,
        project_root: PathBuf,
        hidden_thread_count: usize,
        expanded: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_key = project_root.to_string_lossy().to_string();
        let toggle_label = if expanded {
            "Show less".to_string()
        } else if hidden_thread_count == 1 {
            "Show 1 more".to_string()
        } else {
            format!("Show {hidden_thread_count} more")
        };

        h_flex()
            .w_full()
            .pl_5()
            .pb_2()
            .child(
                Button::new(format!("ai-thread-section-toggle-{project_key}"))
                    .ghost()
                    .compact()
                    .with_size(gpui_component::Size::Small)
                    .icon(
                        Icon::new(if expanded {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .size(px(12.0)),
                    )
                    .label(toggle_label)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.ai_toggle_thread_sidebar_project_expanded(project_key.clone(), cx);
                        });
                    }),
            )
            .into_any_element()
    }

    fn render_ai_timeline_panel(
        &mut self,
        view: Entity<Self>,
        state: &AiTimelinePanelState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .relative()
            .child(
                div().id("ai-timeline-scroll-area").size_full().child(
                    v_flex()
                        .size_full()
                        .w_full()
                        .min_h_0()
                        .gap_2()
                        .p_3()
                        .bg(cx.theme().background)
                        .child(self.render_ai_timeline_toolbar(view.clone(), state, is_dark, cx))
                        .when_some(state.ai_error_message.clone(), |this, error| {
                            this.child(self.render_ai_timeline_error_banner(
                                view.clone(),
                                state,
                                error,
                                is_dark,
                                cx,
                            ))
                        })
                        .when(
                            !state.pending_approvals.is_empty()
                                || !state.pending_user_inputs.is_empty(),
                            |this| {
                                this.child(self.render_ai_timeline_pending_panels(
                                    view.clone(),
                                    state,
                                    is_dark,
                                    cx,
                                ))
                            },
                        )
                        .when(state.timeline_loading, |this| {
                            this.child(render_ai_timeline_loading_skeleton(is_dark, cx))
                        })
                        .when_some(
                            state
                                .pending_thread_start
                                .clone()
                                .filter(|_| !state.timeline_loading),
                            |this, pending| {
                                this.child(render_ai_pending_thread_start(&pending, is_dark, cx))
                            },
                        )
                        .when(state.show_select_thread_empty_state, |this| {
                            this.child(
                                div()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .bg(hunk_opacity(cx.theme().muted, is_dark, 0.22, 0.40))
                                    .p_3()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Select a thread or start a new one to begin."),
                                    ),
                            )
                        })
                        .when_some(
                            state
                                .selected_thread_id
                                .clone()
                                .filter(|_| !state.timeline_loading),
                            |this, thread_id| {
                                this.child(self.render_ai_timeline_rows(
                                    view.clone(),
                                    state,
                                    thread_id,
                                    is_dark,
                                    cx,
                                ))
                            },
                        ),
                ),
            )
            .into_any_element()
    }

    fn render_ai_timeline_error_banner(
        &self,
        view: Entity<Self>,
        state: &AiTimelinePanelState,
        error: String,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let login_pending = state.ai_pending_chatgpt_login_id.is_some();
        let show_auth_actions = state.ai_requires_openai_auth && !state.ai_account_connected;

        let actions = h_flex()
            .items_center()
            .gap_1()
            .flex_wrap()
            .when(show_auth_actions && !login_pending, |this| {
                this.child({
                    let view = view.clone();
                    Button::new("ai-auth-error-login")
                        .compact()
                        .primary()
                        .with_size(gpui_component::Size::Small)
                        .label("Sign in")
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_start_chatgpt_login_action(cx);
                            });
                        })
                })
            })
            .when(show_auth_actions && login_pending, |this| {
                this.child({
                    let view = view.clone();
                    Button::new("ai-auth-error-cancel-login")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .label("Cancel login")
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_cancel_chatgpt_login_action(cx);
                            });
                        })
                })
            });

        div()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().danger)
            .bg(hunk_opacity(cx.theme().danger, is_dark, 0.16, 0.10))
            .p_2()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(cx.theme().danger)
                            .whitespace_normal()
                            .child(error),
                    )
                    .child(actions),
            )
            .into_any_element()
    }

    fn render_ai_timeline_toolbar(
        &self,
        view: Entity<Self>,
        state: &AiTimelinePanelState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let show_repo_actions = state.workspace_kind.shows_repo_actions();
        let selected_thread_project_label = show_repo_actions
            .then_some(state.selected_thread_id.as_deref())
            .flatten()
            .and_then(|thread_id| self.ai_visible_project_root_with_context(Some(thread_id), None))
            .as_deref()
            .map(crate::app::project_picker::project_display_name);

        let workspace_actions = if show_repo_actions {
            h_flex()
                .flex_none()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Target: {}", state.workspace_label)),
                )
                .when(state.show_worktree_base_branch_picker, |this| {
                    this.child(
                        h_flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Base Branch"),
                            )
                            .child(
                                render_hunk_picker(
                                    &self.ai_worktree_base_branch_picker_state,
                                    HunkPickerConfig::new(
                                        "ai-worktree-base-branch-picker",
                                        state.selected_worktree_base_branch.clone(),
                                    )
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(8.0))
                                    .width(px(220.0))
                                    .background(hunk_opacity(
                                        cx.theme().background,
                                        is_dark,
                                        0.82,
                                        0.98,
                                    ))
                                    .border_color(cx.theme().border)
                                    .disabled(self.git_controls_busy() || self.branches.is_empty())
                                    .empty(
                                        h_flex()
                                            .h(px(72.0))
                                            .justify_center()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("No branches available."),
                                    ),
                                    cx,
                                ),
                            ),
                    )
                })
                .child(
                    div()
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "Branch: {}{}",
                            state.active_branch,
                            state
                                .current_review_summary
                                .as_ref()
                                .map(|review| {
                                    let review_kind = match review.provider {
                                        hunk_forge::ForgeProvider::GitHub => "PR",
                                        hunk_forge::ForgeProvider::GitLab => "MR",
                                    };
                                    format!("  {review_kind} #{}", review.number)
                                })
                                .unwrap_or_default()
                        )),
                )
                .child({
                    let view = view.clone();
                    let diff_button_enabled = self.ai_can_open_inline_review_for_current_thread();
                    let diff_button_active =
                        state.right_pane_mode == Some(AiWorkspaceRightPaneMode::InlineReview)
                            && self.current_ai_inline_review_mode()
                                == AiInlineReviewMode::WorkingTree;
                    Button::new("ai-open-working-tree-diff")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .rounded(px(8.0))
                        .icon(Icon::new(HunkIconName::FileDiff).size(px(14.0)))
                        .tooltip(if diff_button_enabled {
                            if diff_button_active {
                                "Close working tree diff (Cmd/Ctrl+D)"
                            } else {
                                "Open working tree diff (Cmd/Ctrl+D)"
                            }
                        } else {
                            "No AI diff is available for the current thread yet"
                        })
                        .disabled(!diff_button_enabled)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.ai_toggle_inline_review_for_current_thread_in_mode(
                                    AiInlineReviewMode::WorkingTree,
                                    cx,
                                );
                            });
                        })
                })
                .child({
                    let view = view.clone();
                    let active = state.right_pane_mode == Some(AiWorkspaceRightPaneMode::Browser);
                    let button = Button::new("ai-open-browser")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .rounded(px(8.0))
                        .icon(Icon::new(IconName::Globe).size(px(14.0)))
                        .tooltip(if active {
                            "Close browser"
                        } else {
                            "Open browser"
                        });
                    if active {
                        button.primary()
                    } else {
                        button
                    }
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.ai_toggle_browser_for_current_thread(cx);
                        });
                    })
                })
                .child({
                    let view_for_primary = view.clone();
                    let view_for_menu = view.clone();
                    let available_project_open_targets = self.available_project_open_targets.clone();
                    let preferred_project_open_target = self.preferred_project_open_target();
                    let primary_project_open_target = preferred_project_open_target
                        .or_else(|| available_project_open_targets.first().copied());
                    let project_open_tooltip = self.ai_project_open_tooltip();
                    let project_open_disabled =
                        self.ai_project_open_path().is_none() || primary_project_open_target.is_none();
                    DropdownButton::new("ai-open-project-dropdown")
                        .button(
                            Button::new("ai-open-project")
                                .compact()
                                .outline()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(8.0))
                                .when_some(primary_project_open_target, |this, target| {
                                    this.icon(ai_project_open_target_icon(target))
                                })
                                .label("Open")
                                .tooltip(project_open_tooltip)
                                .disabled(project_open_disabled)
                                .on_click(move |_, _, cx| {
                                    view_for_primary.update(cx, |this, cx| {
                                        this.open_ai_workspace_in_preferred_project_target(cx);
                                    });
                                }),
                        )
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .rounded(px(8.0))
                        .disabled(project_open_disabled)
                        .dropdown_menu(move |menu, _, _| {
                            if available_project_open_targets.is_empty() {
                                return menu.item(
                                    PopupMenuItem::new("No supported editors or file managers found")
                                        .disabled(true),
                                );
                            }

                            available_project_open_targets.iter().copied().fold(
                                menu,
                                |menu, target| {
                                    let view = view_for_menu.clone();
                                    menu.item(
                                        PopupMenuItem::new(target.display_label())
                                            .icon(ai_project_open_target_icon(target))
                                            .on_click(move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.open_ai_workspace_in_project_target(
                                                        target, cx,
                                                    );
                                                });
                                            }),
                                    )
                                },
                            )
                        })
                        .into_any_element()
                })
                .child({
                    let view = view.clone();
                    let push_label = format!("Commit and Push to {}", state.active_branch);
                    let create_branch_label = "Create Branch and Push".to_string();
                    let current_review_summary = state.current_review_summary.clone();
                    let publish_tooltip =
                        state.ai_publish_blocker.clone().unwrap_or_else(|| {
                            match state.selected_thread_start_mode {
                                Some(AiNewThreadStartMode::Local) => {
                                    "Commit and push the current branch, create a fresh branch and push it, or open PR/MR for the current work. If the current branch is the default branch, Hunk creates a new branch automatically.".to_string()
                                }
                                Some(AiNewThreadStartMode::Worktree) => {
                                    "Commit and push this worktree branch, create a fresh branch and push it, or open PR/MR for the current work.".to_string()
                                }
                                None => "Commit and push this branch, create a fresh branch and push it, or open PR/MR for it.".to_string(),
                            }
                        });
                    Button::new("ai-publish-thread")
                        .compact()
                        .outline()
                        .with_size(gpui_component::Size::Small)
                        .rounded(px(8.0))
                        .loading(
                            state.ai_commit_and_push_loading
                                || state.ai_create_branch_and_push_loading
                                || state.ai_open_pr_loading,
                        )
                        .dropdown_caret(true)
                        .label("Publish")
                        .tooltip(publish_tooltip)
                        .disabled(state.ai_publish_disabled || state.ai_open_pr_disabled)
                        .dropdown_menu(move |menu, _, _| {
                            let menu = menu
                                .item(PopupMenuItem::new(push_label.clone()).on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_commit_and_push_for_current_thread(cx);
                                        });
                                    }
                                }))
                                .item(PopupMenuItem::new(create_branch_label.clone()).on_click({
                                    let view = view.clone();
                                    move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_create_branch_and_push_for_current_thread(
                                                window, cx,
                                            );
                                        });
                                    }
                                }))
                                .item(PopupMenuItem::separator());

                            if let Some(review) = current_review_summary.clone() {
                                let review_kind = match review.provider {
                                    hunk_forge::ForgeProvider::GitHub => "PR",
                                    hunk_forge::ForgeProvider::GitLab => "MR",
                                };
                                let review_for_open = review.clone();
                                let review_for_copy = review.clone();
                                menu.item(
                                    PopupMenuItem::new(format!(
                                        "View {review_kind} #{}",
                                        review.number
                                    ))
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                let review = review_for_open.clone();
                                                view.update(cx, |this, cx| {
                                                    this.open_review_summary_in_browser(
                                                        &review, cx,
                                                    );
                                                });
                                            }
                                        }),
                                )
                                .item(PopupMenuItem::new(format!("Copy {review_kind} Link")).on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let review = review_for_copy.clone();
                                        view.update(cx, |this, cx| {
                                            this.copy_review_summary_url(&review, cx);
                                        });
                                    }
                                }))
                            } else {
                                menu.item(PopupMenuItem::new("Open PR").on_click({
                                    let view = view.clone();
                                    move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_open_pr_for_current_thread(window, cx);
                                        });
                                    }
                                }))
                            }
                        })
                        .into_any_element()
                })
                .when_some(state.ai_managed_worktree_target.clone(), |this, target| {
                    let view = view.clone();
                    let tooltip = state.ai_delete_worktree_blocker.clone().unwrap_or_else(|| {
                        format!(
                            "Delete managed worktree '{}' after the thread is done.",
                            target.name
                        )
                    });
                    this.child(
                        Button::new("ai-delete-worktree")
                            .compact()
                            .danger()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .icon(Icon::new(IconName::Delete).size(px(14.0)))
                            .label("Delete Worktree")
                            .tooltip(tooltip)
                            .loading(state.ai_delete_worktree_loading)
                            .disabled(state.ai_delete_worktree_blocker.is_some())
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.ai_confirm_delete_current_worktree_action(window, cx);
                                });
                            }),
                    )
                })
                .when(self.ai_mad_max_mode, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().danger)
                            .child("Full access enabled"),
                    )
                })
                .into_any_element()
        } else {
            h_flex()
                .flex_none()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .child(state.workspace_label.clone()),
                )
                .into_any_element()
        };

        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .when_some(selected_thread_project_label, |this, project_label| {
                        this.child(
                            div()
                                .min_w_0()
                                .text_base()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .whitespace_nowrap()
                                .truncate()
                                .child(project_label),
                        )
                    })
                    .when_some(state.selected_thread_start_mode, |this, start_mode| {
                        this.child(ai_render_thread_start_mode_chip(start_mode, is_dark, cx))
                    }),
            )
            .child(h_flex().flex_1().justify_end().child(workspace_actions))
            .into_any_element()
    }

    fn render_ai_timeline_pending_panels(
        &self,
        view: Entity<Self>,
        state: &AiTimelinePanelState,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .w_full()
            .gap_1()
            .when(!state.pending_approvals.is_empty(), |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .gap_1()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().warning)
                        .bg(hunk_opacity(cx.theme().warning, is_dark, 0.14, 0.08))
                        .p_2()
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(cx.theme().warning)
                                .child("Pending approvals"),
                        )
                        .children(state.pending_approvals.iter().map(|approval| {
                            let approve_request_id = approval.request_id.clone();
                            let decline_request_id = approval.request_id.clone();
                            let view = view.clone();
                            v_flex()
                                .w_full()
                                .gap_1()
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(cx.theme().border)
                                .bg(cx.theme().background)
                                .p_2()
                                .child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_semibold()
                                                .child(ai_approval_kind_label(approval.kind)),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .child(approval.request_id.clone()),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .whitespace_normal()
                                        .child(ai_approval_description(approval)),
                                )
                                .when_some(approval.reason.clone(), |this, reason| {
                                    this.child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .whitespace_normal()
                                            .child(reason),
                                    )
                                })
                                .child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .gap_1()
                                        .child({
                                            let view = view.clone();
                                            Button::new(format!(
                                                "ai-approval-accept-{}",
                                                approval.request_id
                                            ))
                                            .compact()
                                            .primary()
                                            .with_size(gpui_component::Size::Small)
                                            .label("Accept")
                                            .on_click(
                                                move |_, _, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.ai_resolve_pending_approval_action(
                                                            approve_request_id.clone(),
                                                            AiApprovalDecision::Accept,
                                                            cx,
                                                        );
                                                    });
                                                },
                                            )
                                        })
                                        .child({
                                            let view = view.clone();
                                            Button::new(format!(
                                                "ai-approval-decline-{}",
                                                approval.request_id
                                            ))
                                            .compact()
                                            .outline()
                                            .with_size(gpui_component::Size::Small)
                                            .label("Decline")
                                            .on_click(
                                                move |_, _, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.ai_resolve_pending_approval_action(
                                                            decline_request_id.clone(),
                                                            AiApprovalDecision::Decline,
                                                            cx,
                                                        );
                                                    });
                                                },
                                            )
                                        }),
                                )
                        })),
                )
            })
            .when(!self.ai_pending_browser_approvals.is_empty(), |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .gap_1()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().warning)
                        .bg(hunk_opacity(cx.theme().warning, is_dark, 0.14, 0.08))
                        .p_2()
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(cx.theme().warning)
                                .child("Pending browser approvals"),
                        )
                        .children(self.ai_pending_browser_approvals.iter().map(|approval| {
                            let approve_request_id = approval.request_id.clone();
                            let decline_request_id = approval.request_id.clone();
                            let view = view.clone();
                            v_flex()
                                .w_full()
                                .gap_1()
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(cx.theme().border)
                                .bg(cx.theme().background)
                                .p_2()
                                .child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_semibold()
                                                .child("Browser Action Approval"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .child(approval.request_id.clone()),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .whitespace_normal()
                                        .child(format!("{:?}: {}", approval.kind, approval.summary)),
                                )
                                .child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .gap_1()
                                        .child({
                                            let view = view.clone();
                                            Button::new(format!(
                                                "ai-browser-approval-accept-{}",
                                                approval.request_id
                                            ))
                                            .compact()
                                            .primary()
                                            .with_size(gpui_component::Size::Small)
                                            .label("Accept")
                                            .on_click(move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.ai_resolve_pending_browser_approval_action(
                                                        approve_request_id.clone(),
                                                        AiApprovalDecision::Accept,
                                                        cx,
                                                    );
                                                });
                                            })
                                        })
                                        .child({
                                            let view = view.clone();
                                            Button::new(format!(
                                                "ai-browser-approval-decline-{}",
                                                approval.request_id
                                            ))
                                            .compact()
                                            .outline()
                                            .with_size(gpui_component::Size::Small)
                                            .label("Decline")
                                            .on_click(move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.ai_resolve_pending_browser_approval_action(
                                                        decline_request_id.clone(),
                                                        AiApprovalDecision::Decline,
                                                        cx,
                                                    );
                                                });
                                            })
                                        }),
                                )
                        })),
                )
            })
            .when(!state.pending_user_inputs.is_empty(), |this| {
                this.child(render_ai_pending_user_inputs_panel(
                    &state.pending_user_inputs,
                    &self.ai_pending_user_input_answers,
                    view.clone(),
                    is_dark,
                    cx,
                ))
            })
            .into_any_element()
    }

    fn render_ai_timeline_rows(
        &mut self,
        view: Entity<Self>,
        state: &AiTimelinePanelState,
        thread_id: String,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace_surface = self.render_ai_workspace_surface_scroller(cx);
        let uses_workspace_surface = workspace_surface.is_some();
        let timeline_body = (!state.timeline_visible_row_ids.is_empty())
            .then(|| workspace_surface.unwrap_or_else(|| div().size_full().into_any_element()));

        let timeline_column = v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .w_full()
            .gap_2()
            .when(state.show_no_turns_empty_state, |this| {
                this.child(
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(hunk_opacity(cx.theme().muted, is_dark, 0.22, 0.40))
                        .p_3()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("No turns yet. Send a prompt to start."),
                        ),
                )
            })
            .when(state.timeline_hidden_turn_count > 0, |this| {
                let load_older_thread_id = thread_id.clone();
                let show_all_thread_id = thread_id.clone();
                let view = view.clone();
                this.child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .rounded_md()
                        .border_1()
                        .border_color(hunk_opacity(cx.theme().border, is_dark, 0.90, 0.74))
                        .bg(hunk_blend(
                            cx.theme().background,
                            cx.theme().muted,
                            is_dark,
                            0.16,
                            0.24,
                        ))
                        .p_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "Showing latest {} of {} turns.",
                                    state.timeline_visible_turn_count,
                                    state.timeline_total_turn_count
                                )),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .child({
                                    let view = view.clone();
                                    Button::new("ai-timeline-load-older-turns")
                                        .compact()
                                        .outline()
                                        .with_size(gpui_component::Size::Small)
                                        .label("Load older")
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_load_older_turns_action(
                                                    load_older_thread_id.clone(),
                                                    cx,
                                                );
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("ai-timeline-show-all-turns")
                                        .compact()
                                        .outline()
                                        .with_size(gpui_component::Size::Small)
                                        .label("Show all")
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_show_full_timeline_action(
                                                    show_all_thread_id.clone(),
                                                    cx,
                                                );
                                            });
                                        })
                                }),
                        ),
                )
            })
            .when_some(timeline_body, |this, timeline_body| {
                this.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .relative()
                        .child(timeline_body)
                        .when(uses_workspace_surface, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .right(px(8.0))
                                    .bottom_0()
                                    .w(px(DIFF_SCROLLBAR_SIZE))
                                    .child(
                                        Scrollbar::vertical(
                                            &self.ai_workspace_surface_scroll_handle,
                                        )
                                        .scrollbar_show(ScrollbarShow::Always),
                                    ),
                            )
                            .when(!self.ai_timeline_follow_output, |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .right(px(24.0))
                                        .bottom(px(12.0))
                                        .left_0()
                                        .flex()
                                        .justify_center()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .child({
                                            let view = view.clone();
                                            Button::new("ai-workspace-scroll-to-bottom")
                                                .compact()
                                                .primary()
                                                .with_size(gpui_component::Size::Small)
                                                .icon(Icon::new(IconName::ChevronDown).size(px(14.0)))
                                                .tooltip("Scroll to the bottom")
                                                .on_click(move |_, _, cx| {
                                                    cx.stop_propagation();
                                                    view.update(cx, |this, cx| {
                                                        this.ai_scroll_timeline_to_bottom_action(cx);
                                                    });
                                                })
                                        }),
                                )
                            })
                        }),
                )
            });

        timeline_column.into_any_element()
    }

    fn render_ai_inline_review_pane(
        &mut self,
        view: Entity<Self>,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current_mode = self.current_ai_inline_review_mode();
        let pane_subtitle = match current_mode {
            AiInlineReviewMode::Historical => "Historical AI diff for the selected turn",
            AiInlineReviewMode::WorkingTree => "Current working tree diff for this thread workspace",
        };
        v_flex()
            .size_full()
            .min_h_0()
            .rounded_lg()
            .border_1()
            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.86, 0.74))
            .bg(hunk_opacity(cx.theme().background, is_dark, 0.96, 1.0))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Diff"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(pane_subtitle),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child({
                                let view = view.clone();
                                let active = self.current_ai_right_pane_mode()
                                    == Some(AiWorkspaceRightPaneMode::InlineReview);
                                let button = Button::new("ai-right-pane-mode-diff")
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(8.0))
                                    .label(AiWorkspaceRightPaneMode::InlineReview.label());
                                if active {
                                    button.outline()
                                } else {
                                    button.ghost()
                                }
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_set_right_pane_mode(
                                            AiWorkspaceRightPaneMode::InlineReview,
                                            cx,
                                        );
                                    });
                                })
                            })
                            .when(self.ai_browser_is_open(), |this| {
                                this.child({
                                    let view = view.clone();
                                    let active = self.current_ai_right_pane_mode()
                                        == Some(AiWorkspaceRightPaneMode::Browser);
                                    let button = Button::new("ai-right-pane-mode-browser")
                                        .compact()
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(8.0))
                                        .label(AiWorkspaceRightPaneMode::Browser.label());
                                    if active {
                                        button.outline()
                                    } else {
                                        button.ghost()
                                    }
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_set_right_pane_mode(
                                                AiWorkspaceRightPaneMode::Browser,
                                                cx,
                                            );
                                        });
                                    })
                                })
                            })
                            .child({
                                let active = current_mode == AiInlineReviewMode::Historical;
                                let view = view.clone();
                                let button = Button::new("ai-inline-review-mode-historical")
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(8.0))
                                    .label(AiInlineReviewMode::Historical.label());
                                if active {
                                    button.outline()
                                } else {
                                    button.ghost()
                                }
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_set_inline_review_mode(
                                            AiInlineReviewMode::Historical,
                                            cx,
                                        );
                                    });
                                })
                            })
                            .child({
                                let active = current_mode == AiInlineReviewMode::WorkingTree;
                                let view = view.clone();
                                let button = Button::new("ai-inline-review-mode-working-tree")
                                    .compact()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(8.0))
                                    .label(AiInlineReviewMode::WorkingTree.label());
                                if active {
                                    button.outline()
                                } else {
                                    button.ghost()
                                }
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_set_inline_review_mode(
                                            AiInlineReviewMode::WorkingTree,
                                            cx,
                                        );
                                    });
                                })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("ai-inline-review-open-review")
                                    .compact()
                                    .ghost()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(8.0))
                                    .label("Open in Review")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_open_review_tab(cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("ai-inline-review-close")
                                    .compact()
                                    .ghost()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(8.0))
                                    .label("Close")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_close_inline_review_action(cx);
                                        });
                                    })
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_ai_inline_review_surface(cx)),
            )
            .into_any_element()
    }

    fn render_ai_right_pane(
        &mut self,
        view: Entity<Self>,
        mode: Option<AiWorkspaceRightPaneMode>,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match mode {
            Some(AiWorkspaceRightPaneMode::InlineReview) => {
                self.render_ai_inline_review_pane(view, is_dark, cx)
            }
            Some(AiWorkspaceRightPaneMode::Browser) => {
                self.render_ai_browser_pane(view, is_dark, cx)
            }
            None => div().size_full().into_any_element(),
        }
    }

    fn render_ai_browser_pane(
        &mut self,
        view: Entity<Self>,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_thread_id = self.ai_selected_thread_id.clone();
        let browser_status = self.ai_browser_runtime.status();
        let status_label = match browser_status {
            hunk_browser::BrowserRuntimeStatus::Disabled => "Runtime unavailable",
            hunk_browser::BrowserRuntimeStatus::Configured => "Runtime configured",
            hunk_browser::BrowserRuntimeStatus::Ready => "Ready",
        };
        let session_snapshot = selected_thread_id
            .as_deref()
            .and_then(|thread_id| self.ai_browser_runtime.session(thread_id))
            .map(|session| session.state().clone());
        let session_state = session_snapshot;
        let browser_render_image = selected_thread_id.as_deref().and_then(|thread_id| {
            let latest_frame = session_state
                .as_ref()
                .and_then(|state| state.latest_frame.as_ref())?;
            self.ai_browser_render_frame_cache
                .as_ref()
                .filter(|cache| {
                    cache.thread_id == thread_id
                        && cache.frame_epoch == latest_frame.frame_epoch
                        && cache.width == latest_frame.width
                        && cache.height == latest_frame.height
                })
                .map(|cache| cache.image.clone())
        });
        let runtime_ready = browser_status == hunk_browser::BrowserRuntimeStatus::Ready;
        let can_go_back = session_state
            .as_ref()
            .is_some_and(|state| state.can_go_back && runtime_ready);
        let can_go_forward = session_state
            .as_ref()
            .is_some_and(|state| state.can_go_forward && runtime_ready);
        let loading = session_state
            .as_ref()
            .is_some_and(|state| state.loading && runtime_ready);
        let page_status = session_state
            .as_ref()
            .and_then(|state| state.load_error.as_ref())
            .map(|error| (error.as_str(), cx.theme().danger))
            .unwrap_or_else(|| {
                if loading {
                    ("Loading", cx.theme().warning)
                } else {
                    ("Idle", cx.theme().muted_foreground)
                }
            });
        let browser_tabs = session_state
            .as_ref()
            .map(|state| state.tabs.clone())
            .unwrap_or_default();
        let active_tab_id = session_state
            .as_ref()
            .map(|state| state.active_tab_id.clone());

        v_flex()
            .size_full()
            .min_h_0()
            .rounded_lg()
            .border_1()
            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.86, 0.74))
            .bg(hunk_opacity(cx.theme().background, is_dark, 0.96, 1.0))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(Icon::new(IconName::Globe).size(px(14.0)))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child("Browser"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(status_label),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .when(self.ai_inline_review_is_open(), |this| {
                                this.child({
                                    let view = view.clone();
                                    Button::new("ai-browser-pane-mode-diff")
                                        .compact()
                                        .ghost()
                                        .with_size(gpui_component::Size::Small)
                                        .rounded(px(8.0))
                                        .label(AiWorkspaceRightPaneMode::InlineReview.label())
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_set_right_pane_mode(
                                                    AiWorkspaceRightPaneMode::InlineReview,
                                                    cx,
                                                );
                                            });
                                        })
                                })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("ai-browser-close")
                                    .compact()
                                    .ghost()
                                    .with_size(gpui_component::Size::Small)
                                    .rounded(px(8.0))
                                    .label("Close")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_close_browser_action(cx);
                                        });
                                    })
                            }),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .h(px(34.0))
                    .bg(cx.theme().tab_bar)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .overflow_x_hidden()
                    .child(
                        h_flex()
                            .w_full()
                            .h_full()
                            .items_center()
                            .children(browser_tabs.iter().map(|tab| {
                                let is_active = active_tab_id.as_ref() == Some(&tab.tab_id);
                                let tab_id = tab.tab_id.as_str().to_string();
                                let label = tab
                                    .title
                                    .as_deref()
                                    .filter(|title| !title.trim().is_empty())
                                    .or_else(|| {
                                        tab.url
                                            .as_deref()
                                            .filter(|url| !url.trim().is_empty() && *url != "about:blank")
                                    })
                                    .unwrap_or("New Tab")
                                    .to_string();
                                let select_view = view.clone();
                                let close_view = view.clone();
                                let mut tab_surface = div()
                                    .id(format!("ai-browser-tab-{tab_id}"))
                                    .flex_none()
                                    .min_w(px(128.0))
                                    .max_w(px(220.0))
                                    .h_full()
                                    .px_2()
                                    .gap_1()
                                    .items_center()
                                    .border_r_1()
                                    .border_color(cx.theme().border)
                                    .on_mouse_down(MouseButton::Left, {
                                        let tab_id = tab_id.clone();
                                        move |_, _, cx| {
                                            select_view.update(cx, |this, cx| {
                                                this.ai_select_browser_tab_for_current_thread(
                                                    tab_id.clone(),
                                                    cx,
                                                );
                                            });
                                        }
                                    });
                                if is_active {
                                    tab_surface = tab_surface
                                        .bg(cx.theme().tab_active)
                                        .border_b_0();
                                } else {
                                    tab_surface = tab_surface
                                        .bg(cx.theme().tab)
                                        .hover(|this| this.bg(cx.theme().muted))
                                        .cursor_pointer();
                                }

                                tab_surface
                                    .child(
                                        h_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .truncate()
                                                    .text_xs()
                                                    .text_color(if is_active {
                                                        cx.theme().tab_active_foreground
                                                    } else {
                                                        cx.theme().tab_foreground
                                                    })
                                                    .child(label),
                                            )
                                            .when(tab.loading, |this| {
                                                this.child(
                                                    Icon::new(IconName::LoaderCircle)
                                                        .size(px(11.0))
                                                        .text_color(cx.theme().warning),
                                                )
                                            })
                                            .child({
                                                let tab_id = tab_id.clone();
                                                Button::new(format!("ai-browser-tab-close-{tab_id}"))
                                                    .ghost()
                                                    .xsmall()
                                                    .icon(Icon::new(IconName::Close).size(px(11.0)))
                                                    .tooltip("Close tab")
                                                    .on_click(move |_, _, cx| {
                                                        cx.stop_propagation();
                                                        close_view.update(cx, |this, cx| {
                                                            this.ai_close_browser_tab_for_current_thread(
                                                                tab_id.clone(),
                                                                cx,
                                                            );
                                                        });
                                                    })
                                            }),
                                    )
                                    .into_any_element()
                            }))
                            .child({
                                let view = view.clone();
                                Button::new("ai-browser-new-tab")
                                    .ghost()
                                    .xsmall()
                                    .icon(Icon::new(IconName::Plus).size(px(13.0)))
                                    .tooltip("New tab")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.ai_create_browser_tab_for_current_thread(cx);
                                        });
                                    })
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .border_b_1()
                                    .border_color(cx.theme().border),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1p5()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        {
                            let view = view.clone();
                            Button::new("ai-browser-back")
                                .compact()
                                .ghost()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(8.0))
                                .icon(Icon::new(IconName::ArrowLeft).size(px(13.0)))
                                .tooltip("Back")
                                .disabled(!can_go_back)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_apply_browser_action_for_current_thread(
                                            hunk_browser::BrowserAction::Back,
                                            cx,
                                        );
                                    });
                                })
                        },
                    )
                    .child(
                        {
                            let view = view.clone();
                            Button::new("ai-browser-forward")
                                .compact()
                                .ghost()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(8.0))
                                .icon(Icon::new(IconName::ArrowRight).size(px(13.0)))
                                .tooltip("Forward")
                                .disabled(!can_go_forward)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_apply_browser_action_for_current_thread(
                                            hunk_browser::BrowserAction::Forward,
                                            cx,
                                        );
                                    });
                                })
                        },
                    )
                    .child(
                        {
                            let view = view.clone();
                            Button::new("ai-browser-reload")
                                .compact()
                                .ghost()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(8.0))
                                .icon(Icon::new(HunkIconName::RotateCcw).size(px(13.0)))
                                .tooltip("Reload")
                                .disabled(!runtime_ready || loading)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_apply_browser_action_for_current_thread(
                                            hunk_browser::BrowserAction::Reload,
                                            cx,
                                        );
                                    });
                                })
                        },
                    )
                    .child(
                        {
                            let view = view.clone();
                            Button::new("ai-browser-stop")
                                .compact()
                                .ghost()
                                .with_size(gpui_component::Size::Small)
                                .rounded(px(8.0))
                                .icon(Icon::new(IconName::CircleX).size(px(13.0)))
                                .tooltip("Stop loading")
                                .disabled(!loading)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.ai_apply_browser_action_for_current_thread(
                                            hunk_browser::BrowserAction::Stop,
                                            cx,
                                        );
                                    });
                                })
                        },
                    )
                    .child(
                        Input::new(&self.ai_browser_address_input_state)
                            .with_size(gpui_component::Size::Small)
                            .appearance(true)
                            .cleanable(true)
                            .w_full()
                            .flex_1()
                            .min_w_0()
                            .rounded(px(8.0))
                            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.74, 0.60))
                            .bg(hunk_opacity(cx.theme().muted, is_dark, 0.22, 0.36))
                            .disabled(selected_thread_id.is_none()),
                    )
                    .child(
                        div()
                            .max_w(px(180.0))
                            .min_w(px(44.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(hunk_opacity(page_status.1, is_dark, 0.70, 0.52))
                            .bg(hunk_opacity(page_status.1, is_dark, 0.12, 0.08))
                            .px_2()
                            .py_1()
                            .text_xs()
                            .text_color(page_status.1)
                            .truncate()
                            .child(page_status.0.to_string()),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.74, 0.60))
                            .bg(hunk_opacity(cx.theme().muted, is_dark, 0.16, 0.28))
                            .px_2()
                            .py_1()
                            .child(Icon::new(HunkIconName::BotMessageSquare).size(px(12.0)))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if runtime_ready {
                                        "Agent ready"
                                    } else {
                                        "Agent paused"
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .bg(hunk_opacity(cx.theme().muted, is_dark, 0.12, 0.20))
                    .track_focus(&self.ai_browser_focus_handle)
                    .on_key_down({
                        let view = view.clone();
                        move |event, window, cx| {
                            let handled = view.update(cx, |this, cx| {
                                this.ai_browser_surface_key_down(&event.keystroke, window, cx)
                            });
                            if handled {
                                cx.stop_propagation();
                            }
                        }
                    })
                    .child(if let Some(render_image) = browser_render_image {
                        if let Some(thread_id) = selected_thread_id.clone() {
                            AiBrowserSurfaceElement {
                                view: view.clone(),
                                thread_id,
                                image: render_image,
                            }
                            .into_any_element()
                        } else {
                            div().size_full().into_any_element()
                        }
                    } else {
                        v_flex()
                            .size_full()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .px_4()
                            .child(Icon::new(IconName::Globe).size(px(32.0)))
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Browser runtime not connected"),
                            )
                            .child(
                                div()
                                    .max_w(px(360.0))
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .text_align(gpui::TextAlign::Center)
                                    .child("CEF offscreen rendering will attach here once the browser backend is available."),
                            )
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

}
