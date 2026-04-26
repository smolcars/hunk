impl DiffViewer {
    pub(super) fn open_browser_context_menu(
        &mut self,
        target: hunk_browser::BrowserContextMenuTarget,
        position: Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.browser_context_menu = Some(BrowserContextMenuState { target, position });
        cx.notify();
    }

    pub(super) fn close_browser_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.browser_context_menu.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn browser_context_menu_navigate(
        &mut self,
        action: hunk_browser::BrowserAction,
        cx: &mut Context<Self>,
    ) {
        self.ai_apply_browser_action_for_current_thread(action, cx);
        self.close_browser_context_menu(cx);
    }

    pub(super) fn browser_context_menu_copy_page_url(&mut self, cx: &mut Context<Self>) {
        let Some(menu_state) = self.browser_context_menu.as_ref() else {
            return;
        };
        let page_url = menu_state.target.page_url.clone().or_else(|| {
            self.ai_selected_thread_id.as_deref().and_then(|thread_id| {
                self.ai_browser_runtime
                    .session(thread_id)
                    .and_then(|session| {
                        let url = session.state().url.as_deref()?;
                        (!url.is_empty()).then(|| url.to_string())
                    })
            })
        });
        let Some(page_url) = page_url else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(page_url));
        self.close_browser_context_menu(cx);
    }

    pub(super) fn browser_context_menu_copy_link_url(&mut self, cx: &mut Context<Self>) {
        let Some(link_url) = self
            .browser_context_menu
            .as_ref()
            .and_then(|menu| menu.target.link_url.clone())
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(link_url));
        self.close_browser_context_menu(cx);
    }

    pub(super) fn browser_context_menu_copy_source_url(&mut self, cx: &mut Context<Self>) {
        let Some(source_url) = self
            .browser_context_menu
            .as_ref()
            .and_then(|menu| menu.target.source_url.clone())
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(source_url));
        self.close_browser_context_menu(cx);
    }

    pub(super) fn browser_context_menu_copy_selected_text(&mut self, cx: &mut Context<Self>) {
        let Some(selection_text) = self
            .browser_context_menu
            .as_ref()
            .and_then(|menu| menu.target.selection_text.clone())
            .filter(|text| !text.trim().is_empty())
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(selection_text));
        self.close_browser_context_menu(cx);
    }

    pub(super) fn browser_context_menu_open_link_in_new_tab(&mut self, cx: &mut Context<Self>) {
        let Some(link_url) = self
            .browser_context_menu
            .as_ref()
            .and_then(|menu| menu.target.link_url.clone())
        else {
            return;
        };
        self.browser_context_menu_open_url_in_new_tab(link_url, cx);
    }

    pub(super) fn browser_context_menu_open_source_in_new_tab(&mut self, cx: &mut Context<Self>) {
        let Some(source_url) = self
            .browser_context_menu
            .as_ref()
            .and_then(|menu| menu.target.source_url.clone())
        else {
            return;
        };
        self.browser_context_menu_open_url_in_new_tab(source_url, cx);
    }

    pub(super) fn browser_context_menu_inspect_element(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.browser_context_menu.as_ref().map(|menu| menu.target.clone())
        else {
            return;
        };
        self.close_browser_context_menu(cx);
        self.ai_show_browser_devtools_for_current_thread(
            Some(hunk_browser::BrowserPhysicalPoint {
                x: target.x,
                y: target.y,
            }),
            cx,
        );
    }

    pub(super) fn browser_context_menu_edit_shortcut(
        &mut self,
        shortcut: BrowserEditShortcut,
        cx: &mut Context<Self>,
    ) {
        let Some(menu_state) = self.browser_context_menu.as_ref() else {
            return;
        };
        if !menu_state.target.editable {
            return;
        }
        self.ai_apply_browser_action_for_current_thread(
            hunk_browser::BrowserAction::Press {
                keys: browser_edit_shortcut_keys(shortcut).to_string(),
            },
            cx,
        );
        self.close_browser_context_menu(cx);
    }

    fn browser_context_menu_open_url_in_new_tab(
        &mut self,
        url: String,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            return;
        };
        self.close_browser_context_menu(cx);
        self.ai_browser_runtime
            .create_tab(thread_id.as_str(), Some(url), true);
        self.ai_browser_render_frame_cache = None;
        self.ai_ensure_active_browser_tab_backend(thread_id.as_str(), cx);
        self.ai_sync_browser_address_input(cx);
        cx.notify();
    }

    pub(super) fn open_workspace_text_context_menu(
        &mut self,
        target: WorkspaceTextContextMenuTarget,
        position: Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.workspace_text_context_menu = Some(WorkspaceTextContextMenuState { target, position });
        cx.notify();
    }

    pub(super) fn close_workspace_text_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.workspace_text_context_menu.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn workspace_text_context_menu_copy(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(menu_state) = self.workspace_text_context_menu.as_ref() else {
            return;
        };
        match &menu_state.target {
            WorkspaceTextContextMenuTarget::FilesEditor(_) => {
                let Some(text) = self.files_editor.borrow().copy_selection_text() else {
                    return;
                };
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            WorkspaceTextContextMenuTarget::SelectableText(_)
            | WorkspaceTextContextMenuTarget::Terminal(_) => {
                let target_row_id = match &menu_state.target {
                    WorkspaceTextContextMenuTarget::SelectableText(target) => {
                        Some(target.row_id.as_str())
                    }
                    WorkspaceTextContextMenuTarget::Terminal(target) => Some(match target.kind {
                        WorkspaceTerminalKind::Ai => crate::app::AI_TERMINAL_TEXT_SELECTION_ROW_ID,
                        WorkspaceTerminalKind::Files => {
                            crate::app::FILES_TERMINAL_TEXT_SELECTION_ROW_ID
                        }
                    }),
                    _ => None,
                };
                let Some(selection_text) = target_row_id.and_then(|row_id| {
                    self.ai_text_selection.as_ref().and_then(|selection| {
                        (selection.row_id == row_id)
                            .then_some(selection)
                            .and_then(AiTextSelection::selected_text)
                    })
                }) else {
                    return;
                };
                cx.write_to_clipboard(ClipboardItem::new_string(selection_text));
            }
            WorkspaceTextContextMenuTarget::DiffRows(_) => {
                let Some(selection_text) = self.selected_rows_as_text() else {
                    return;
                };
                cx.write_to_clipboard(ClipboardItem::new_string(selection_text));
            }
        }
        self.close_workspace_text_context_menu(cx);
    }

    pub(super) fn workspace_text_context_menu_cut(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(WorkspaceTextContextMenuState {
            target: WorkspaceTextContextMenuTarget::FilesEditor(_),
            ..
        }) = self.workspace_text_context_menu.as_ref()
        else {
            return;
        };
        let Some(text) = self.files_editor.borrow_mut().cut_selection_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.sync_editor_dirty_from_input(cx);
        self.close_workspace_text_context_menu(cx);
    }

    pub(super) fn workspace_text_context_menu_paste(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(menu_state) = self.workspace_text_context_menu.as_ref() else {
            return;
        };
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        match &menu_state.target {
            WorkspaceTextContextMenuTarget::FilesEditor(_) => {
                if self.files_editor.borrow_mut().paste_text(text.as_str()) {
                    self.sync_editor_dirty_from_input(cx);
                } else {
                    return;
                }
            }
            WorkspaceTextContextMenuTarget::Terminal(target) => {
                let pasted = match target.kind {
                    WorkspaceTerminalKind::Ai => self.ai_paste_terminal_from_clipboard(cx),
                    WorkspaceTerminalKind::Files => self.files_paste_terminal_from_clipboard(cx),
                };
                if !pasted {
                    return;
                }
            }
            WorkspaceTextContextMenuTarget::SelectableText(_)
            | WorkspaceTextContextMenuTarget::DiffRows(_) => return,
        }
        self.close_workspace_text_context_menu(cx);
        cx.notify();
    }

    pub(super) fn workspace_text_context_menu_select_all(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(menu_state) = self.workspace_text_context_menu.clone() else {
            return;
        };
        match menu_state.target {
            WorkspaceTextContextMenuTarget::FilesEditor(_) => {
                if !self.files_editor.borrow_mut().select_all_action() {
                    return;
                }
                self.sync_editor_dirty_from_input(cx);
            }
            WorkspaceTextContextMenuTarget::SelectableText(target) => {
                if !self.ai_select_all_text_for_surfaces(
                    target.row_id.as_str(),
                    target.selection_surfaces,
                    cx,
                ) {
                    return;
                }
            }
            WorkspaceTextContextMenuTarget::Terminal(target) => {
                let row_id = match target.kind {
                    WorkspaceTerminalKind::Ai => crate::app::AI_TERMINAL_TEXT_SELECTION_ROW_ID,
                    WorkspaceTerminalKind::Files => crate::app::FILES_TERMINAL_TEXT_SELECTION_ROW_ID,
                };
                if !self.ai_select_all_text_for_surfaces(
                    row_id,
                    target.selection_surfaces,
                    cx,
                ) {
                    return;
                }
            }
            WorkspaceTextContextMenuTarget::DiffRows(_) => {
                self.select_all_rows(cx);
            }
        }
        self.close_workspace_text_context_menu(cx);
    }

    pub(super) fn workspace_text_context_menu_clear_terminal(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(WorkspaceTextContextMenuState {
            target: WorkspaceTextContextMenuTarget::Terminal(target),
            ..
        }) = self.workspace_text_context_menu.as_ref()
        else {
            return;
        };
        match target.kind {
            WorkspaceTerminalKind::Ai => self.ai_clear_terminal_session_action(cx),
            WorkspaceTerminalKind::Files => self.files_clear_terminal_session_action(cx),
        }
        self.close_workspace_text_context_menu(cx);
    }

    pub(super) fn workspace_text_context_menu_open_link(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(WorkspaceTextContextMenuState {
            target: WorkspaceTextContextMenuTarget::SelectableText(target),
            ..
        }) = self.workspace_text_context_menu.as_ref()
        else {
            return;
        };
        let Some(raw_target) = target.link_target.clone() else {
            return;
        };
        self.activate_markdown_link(raw_target, None, cx);
        self.close_workspace_text_context_menu(cx);
    }
}

fn browser_edit_shortcut_keys(shortcut: BrowserEditShortcut) -> &'static str {
    let key = match shortcut {
        BrowserEditShortcut::Cut => "X",
        BrowserEditShortcut::Copy => "C",
        BrowserEditShortcut::Paste => "V",
        BrowserEditShortcut::SelectAll => "A",
    };
    if cfg!(target_os = "macos") {
        match key {
            "X" => "Cmd+X",
            "C" => "Cmd+C",
            "V" => "Cmd+V",
            "A" => "Cmd+A",
            _ => unreachable!(),
        }
    } else {
        match key {
            "X" => "Ctrl+X",
            "C" => "Ctrl+C",
            "V" => "Ctrl+V",
            "A" => "Ctrl+A",
            _ => unreachable!(),
        }
    }
}
