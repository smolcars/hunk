#[derive(Clone)]
struct AiThreadSidebarState {
    project_count: usize,
    threads_loading: bool,
    selected_thread_id: Option<String>,
}

struct AiTimelinePanelState {
    active_branch: String,
    workspace_label: String,
    show_worktree_base_branch_picker: bool,
    selected_worktree_base_branch: String,
    selected_thread_id: Option<String>,
    inline_review_selected_row_id: Option<String>,
    selected_thread_start_mode: Option<AiNewThreadStartMode>,
    pending_approvals: Arc<[AiPendingApproval]>,
    pending_user_inputs: Arc<[AiPendingUserInputRequest]>,
    pending_thread_start: Option<AiPendingThreadStart>,
    timeline_total_turn_count: usize,
    timeline_visible_turn_count: usize,
    timeline_hidden_turn_count: usize,
    timeline_visible_row_ids: Arc<[String]>,
    timeline_loading: bool,
    show_select_thread_empty_state: bool,
    show_no_turns_empty_state: bool,
    ai_publish_blocker: Option<String>,
    ai_publish_disabled: bool,
    ai_commit_and_push_loading: bool,
    ai_open_pr_disabled: bool,
    ai_open_pr_loading: bool,
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
        v_flex()
            .size_full()
            .w_full()
            .min_h_0()
            .key_context("AiWorkspace")
            .on_action(cx.listener(Self::ai_interrupt_selected_turn_action))
            .child(
                div().flex_1().w_full().min_h_0().child(if self.ai_thread_sidebar_collapsed {
                    v_flex()
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
                        })
                        .into_any_element()
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
                        .child(
                            resizable_panel().child(
                                v_flex()
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
                                    }),
                            ),
                        )
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
                project_root,
                project_label,
                total_thread_count,
            } => (
                self.render_ai_thread_project_header_row(
                    view,
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
            AiThreadSidebarRowKind::EmptyProject { project_root } => (
                self.render_ai_thread_project_empty_row(project_root, is_dark, cx),
                AiPerfSidebarRowKind::EmptyProject,
            ),
            AiThreadSidebarRowKind::ProjectFooter {
                project_root,
                hidden_thread_count,
                expanded,
            } => (
                self.render_ai_thread_project_footer_row(
                    view,
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
                format!("New Thread  {shortcut}")
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
                    .child({
                        let new_button_view = view.clone();
                        let new_button_project_root = project_root.clone();
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
                            })
                    })
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
                    ),
            )
            .into_any_element()
    }

    fn render_ai_thread_project_empty_row(
        &self,
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
            .child("No threads")
            .into_any_element()
    }

    fn render_ai_thread_project_footer_row(
        &self,
        view: Entity<Self>,
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
        let selected_thread_project_label = state
            .selected_thread_id
            .as_deref()
            .and_then(|thread_id| self.ai_visible_project_root_with_context(Some(thread_id), None))
            .as_deref()
            .map(crate::app::project_picker::project_display_name);
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
            .child(
                h_flex()
                    .flex_1()
                    .justify_end()
                    .child(
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
                                                .disabled(
                                                    self.git_controls_busy()
                                                        || self.branches.is_empty(),
                                                )
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
                                    .child(format!("Branch: {}", state.active_branch)),
                            )
                            .child({
                        let view_for_primary = view.clone();
                        let view_for_menu = view.clone();
                        let available_project_open_targets =
                            self.available_project_open_targets.clone();
                        let preferred_project_open_target = self.preferred_project_open_target();
                        let primary_project_open_target = preferred_project_open_target
                            .or_else(|| available_project_open_targets.first().copied());
                        let project_open_tooltip = self.ai_project_open_tooltip();
                        let project_open_disabled = self.ai_project_open_path().is_none()
                            || primary_project_open_target.is_none();
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
                                        PopupMenuItem::new(
                                            "No supported editors or file managers found",
                                        )
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
                        let publish_tooltip = state.ai_publish_blocker.clone().unwrap_or_else(|| {
                            match state.selected_thread_start_mode {
                                Some(AiNewThreadStartMode::Local) => {
                                    "Commit and push this branch directly, or open PR/MR for it. If the current branch is the default branch, Hunk creates a review branch first.".to_string()
                                }
                                Some(AiNewThreadStartMode::Worktree) => {
                                    "Commit and push this worktree branch directly, or open PR/MR for the current branch.".to_string()
                                }
                                None => "Commit and push this branch directly, or open PR/MR for it.".to_string(),
                            }
                        });
                        Button::new("ai-publish-thread")
                            .compact()
                            .outline()
                            .with_size(gpui_component::Size::Small)
                            .rounded(px(8.0))
                            .loading(state.ai_commit_and_push_loading || state.ai_open_pr_loading)
                            .dropdown_caret(true)
                            .label("Publish")
                            .tooltip(publish_tooltip)
                            .disabled(state.ai_publish_disabled || state.ai_open_pr_disabled)
                            .dropdown_menu(move |menu, _, _| {
                                menu.item(
                                    PopupMenuItem::new(push_label.clone()).on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_commit_and_push_for_current_thread(cx);
                                            });
                                        }
                                    }),
                                )
                                .item(PopupMenuItem::separator())
                                .item(
                                    PopupMenuItem::new("Open PR").on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ai_open_pr_for_current_thread(cx);
                                            });
                                        }
                                    }),
                                )
                            })
                            .into_any_element()
                    })
                    .when_some(state.ai_managed_worktree_target.clone(), |this, target| {
                        let view = view.clone();
                        let tooltip = state.ai_delete_worktree_blocker.clone().unwrap_or_else(|| {
                            format!("Delete managed worktree '{}' after the thread is done.", target.name)
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
                            }),
                    ),
            )
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

        if state.inline_review_selected_row_id.is_some() {
            return h_resizable("hunk-ai-timeline-review-split")
                .child(
                    resizable_panel()
                        .size(px(720.0))
                        .size_range(px(420.0)..px(1280.0))
                        .child(timeline_column),
                )
                .child(
                    resizable_panel()
                        .size(px(640.0))
                        .size_range(px(360.0)..px(1280.0))
                        .child(self.render_ai_inline_review_pane(view, is_dark, cx)),
                )
                .into_any_element();
        }

        timeline_column.into_any_element()
    }

    fn render_ai_inline_review_pane(
        &mut self,
        view: Entity<Self>,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                                    .child("Review"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Selected AI diff"),
                            ),
                    )
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
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_review_workspace_surface(cx)),
            )
            .into_any_element()
    }
}
