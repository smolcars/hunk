impl DiffViewer {
    pub(super) fn ai_handle_terminal_dynamic_tool_call(
        &mut self,
        params: hunk_codex::protocol::DynamicToolCallParams,
        response_tx: std::sync::mpsc::Sender<hunk_codex::protocol::DynamicToolCallResponse>,
        cx: &mut Context<Self>,
    ) {
        if let Some(response) = self.ai_select_terminal_tool_thread(&params, cx) {
            if response_tx.send(response).is_err() {
                self.ai_status_message =
                    Some("Embedded terminal tool response receiver disconnected.".to_string());
                cx.notify();
            }
            return;
        }

        if let Some(confirmation) =
            crate::app::ai_terminal_dynamic_tools::terminal_dynamic_tool_confirmation(&params)
        {
            let request_id = params.call_id.clone();
            if self
                .ai_pending_terminal_approvals
                .iter()
                .any(|approval| approval.request_id == request_id)
            {
                let response = crate::app::ai_terminal_dynamic_tools::terminal_unavailable_response(
                    &params,
                    "This terminal action is already waiting for user confirmation.",
                );
                if response_tx.send(response).is_err() {
                    self.ai_status_message =
                        Some("Embedded terminal tool response receiver disconnected.".to_string());
                    cx.notify();
                }
                return;
            }

            self.ai_pending_terminal_approvals
                .push(AiPendingTerminalApproval {
                    request_id,
                    params,
                    kind: confirmation.kind,
                    summary: confirmation.summary,
                    response_tx,
                });
            self.ai_status_message = Some("Terminal action needs confirmation.".to_string());
            self.invalidate_ai_visible_frame_state_with_reason("timeline");
            cx.notify();
            return;
        }

        let response = self.ai_execute_terminal_dynamic_tool_with_safety(
            &params,
            crate::app::ai_terminal_dynamic_tools::TerminalToolSafetyMode::Enforce,
            cx,
        );
        if response_tx.send(response).is_err() {
            self.ai_status_message =
                Some("Embedded terminal tool response receiver disconnected.".to_string());
            cx.notify();
        }
    }

    pub(super) fn ai_resolve_pending_terminal_approval_action(
        &mut self,
        request_id: String,
        decision: AiApprovalDecision,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .ai_pending_terminal_approvals
            .iter()
            .position(|approval| approval.request_id == request_id)
        else {
            return;
        };
        let pending = self.ai_pending_terminal_approvals.remove(index);
        let response = match decision {
            AiApprovalDecision::Accept => self.ai_execute_terminal_dynamic_tool_with_safety(
                &pending.params,
                crate::app::ai_terminal_dynamic_tools::TerminalToolSafetyMode::AllowSensitiveOnce,
                cx,
            ),
            AiApprovalDecision::Decline => {
                crate::app::ai_terminal_dynamic_tools::terminal_confirmation_declined_response(
                    &pending.params,
                )
            }
        };
        if pending.response_tx.send(response).is_err() {
            self.ai_status_message =
                Some("Embedded terminal tool response receiver disconnected.".to_string());
        } else {
            self.ai_status_message = Some(match decision {
                AiApprovalDecision::Accept => "Terminal action approved.".to_string(),
                AiApprovalDecision::Decline => "Terminal action declined.".to_string(),
            });
        }
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        cx.notify();
    }

    fn ai_execute_terminal_dynamic_tool_with_safety(
        &mut self,
        params: &hunk_codex::protocol::DynamicToolCallParams,
        safety_mode: crate::app::ai_terminal_dynamic_tools::TerminalToolSafetyMode,
        cx: &mut Context<Self>,
    ) -> hunk_codex::protocol::DynamicToolCallResponse {
        use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

        let request = match hunk_codex::terminal_tools::parse_terminal_dynamic_tool_request(params)
        {
            Ok(request) => request,
            Err(error) => {
                return crate::app::ai_terminal_dynamic_tools::terminal_invalid_arguments_response(
                    params, &error,
                );
            }
        };

        if safety_mode == crate::app::ai_terminal_dynamic_tools::TerminalToolSafetyMode::Enforce
            && let Some(kind) =
                crate::app::ai_terminal_dynamic_tools::classify_terminal_request(&request)
        {
            return crate::app::ai_terminal_dynamic_tools::terminal_confirmation_required_response(
                params, kind,
            );
        }

        if let Some(response) = self.ai_select_terminal_tool_thread(params, cx) {
            return response;
        }

        if self.current_ai_workspace_kind() == AiWorkspaceKind::Chats {
            return crate::app::ai_terminal_dynamic_tools::terminal_unavailable_response(
                params,
                "The embedded terminal is unavailable in Chats.",
            );
        }

        match request {
            TerminalDynamicToolRequest::Open { tab_id } => {
                if let Some(tab_id) = terminal_tool_tab_id(tab_id)
                    && let Some(error) = self.ai_select_terminal_tab_for_tool(params, tab_id, cx)
                {
                    return error;
                }
                self.ai_terminal_set_open(true, cx);
                self.ensure_ai_terminal_session(cx);
                let tabs = self.ai_visible_terminal_tabs_snapshot();
                crate::app::ai_terminal_dynamic_tools::terminal_tabs_response(
                    params,
                    self.ai_terminal_active_tab_id,
                    tabs.as_slice(),
                    "Terminal was opened.",
                )
            }
            TerminalDynamicToolRequest::Tabs => {
                let tabs = self.ai_visible_terminal_tabs_snapshot();
                crate::app::ai_terminal_dynamic_tools::terminal_tabs_response(
                    params,
                    self.ai_terminal_active_tab_id,
                    tabs.as_slice(),
                    "Terminal tabs were read.",
                )
            }
            TerminalDynamicToolRequest::NewTab { activate } => {
                let Some(owner_key) = self.ai_current_terminal_owner_key() else {
                    return crate::app::ai_terminal_dynamic_tools::terminal_no_active_thread_response(
                        params,
                    );
                };
                if activate {
                    self.ai_new_terminal_tab_action(cx);
                } else {
                    self.ai_save_visible_terminal_tab();
                    let tab_id = self.ai_terminal_next_tab_id.max(1);
                    self.ai_terminal_next_tab_id = tab_id.saturating_add(1);
                    self.ai_terminal_tabs.push(TerminalTabState::new(tab_id));
                    self.ai_terminal_tabs.sort_by_key(|tab| tab.id);
                    self.ai_store_visible_terminal_state_for_thread(Some(owner_key.as_str()));
                    cx.notify();
                }
                let tabs = self.ai_visible_terminal_tabs_snapshot();
                crate::app::ai_terminal_dynamic_tools::terminal_tabs_response(
                    params,
                    self.ai_terminal_active_tab_id,
                    tabs.as_slice(),
                    "Terminal tab was created.",
                )
            }
            TerminalDynamicToolRequest::SelectTab { tab_id } => {
                if let Some(error) =
                    self.ai_select_terminal_tab_for_tool(params, tab_id.get(), cx)
                {
                    return error;
                }
                let tabs = self.ai_visible_terminal_tabs_snapshot();
                crate::app::ai_terminal_dynamic_tools::terminal_tabs_response(
                    params,
                    self.ai_terminal_active_tab_id,
                    tabs.as_slice(),
                    "Terminal tab was selected.",
                )
            }
            TerminalDynamicToolRequest::CloseTab { tab_id } => {
                if let Some(error) =
                    self.ai_select_terminal_tab_for_tool(params, tab_id.get(), cx)
                {
                    return error;
                }
                self.ai_close_terminal_tab_action(cx);
                let tabs = self.ai_visible_terminal_tabs_snapshot();
                crate::app::ai_terminal_dynamic_tools::terminal_tabs_response(
                    params,
                    self.ai_terminal_active_tab_id,
                    tabs.as_slice(),
                    "Terminal tab was closed.",
                )
            }
            TerminalDynamicToolRequest::Snapshot {
                tab_id,
                include_cells,
            } => {
                let tabs = self.ai_visible_terminal_tabs_snapshot();
                let target_tab_id = terminal_tool_tab_id(tab_id).unwrap_or(self.ai_terminal_active_tab_id);
                let Some(tab) = tabs.iter().find(|tab| tab.id == target_tab_id) else {
                    return terminal_missing_tab_response(params, target_tab_id);
                };
                crate::app::ai_terminal_dynamic_tools::terminal_snapshot_response(
                    params,
                    self.ai_terminal_active_tab_id,
                    tabs.as_slice(),
                    tab,
                    include_cells,
                )
            }
            TerminalDynamicToolRequest::Logs {
                tab_id,
                since_sequence,
                limit,
            } => {
                let tabs = self.ai_visible_terminal_tabs_snapshot();
                let target_tab_id = terminal_tool_tab_id(tab_id).unwrap_or(self.ai_terminal_active_tab_id);
                let Some(tab) = tabs.iter().find(|tab| tab.id == target_tab_id) else {
                    return terminal_missing_tab_response(params, target_tab_id);
                };
                crate::app::ai_terminal_dynamic_tools::terminal_logs_response(
                    params,
                    self.ai_terminal_active_tab_id,
                    tabs.as_slice(),
                    tab,
                    since_sequence,
                    limit,
                )
            }
            TerminalDynamicToolRequest::Run { .. }
            | TerminalDynamicToolRequest::Type { .. }
            | TerminalDynamicToolRequest::Paste { .. }
            | TerminalDynamicToolRequest::Press { .. }
            | TerminalDynamicToolRequest::Scroll { .. }
            | TerminalDynamicToolRequest::Resize { .. }
            | TerminalDynamicToolRequest::Kill { .. } => self.ai_execute_terminal_action_tool(
                params,
                request,
                cx,
            ),
        }
    }

    fn ai_execute_terminal_action_tool(
        &mut self,
        params: &hunk_codex::protocol::DynamicToolCallParams,
        request: hunk_codex::terminal_tools::TerminalDynamicToolRequest,
        cx: &mut Context<Self>,
    ) -> hunk_codex::protocol::DynamicToolCallResponse {
        use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

        match request {
            TerminalDynamicToolRequest::Run { tab_id, command } => {
                if let Some(tab_id) = terminal_tool_tab_id(tab_id)
                    && let Some(error) = self.ai_select_terminal_tab_for_tool(params, tab_id, cx)
                {
                    return error;
                }
                self.ai_run_command_in_terminal(None, command, cx);
                self.ai_terminal_action_success(params, "run", "Command was submitted.")
            }
            TerminalDynamicToolRequest::Type { tab_id, text } => {
                if let Some(error) = self.ai_prepare_terminal_input_tool(params, tab_id, cx) {
                    return error;
                }
                let Some(runtime) = self.ai_terminal_runtime.as_ref() else {
                    return terminal_no_running_session_response(params);
                };
                if let Err(error) = runtime.handle.write_input(text.as_bytes()) {
                    self.ai_terminal_session.status_message = Some(error.to_string());
                    self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                    return crate::app::ai_terminal_dynamic_tools::terminal_action_rejected_response(
                        params,
                        error.to_string().as_str(),
                    );
                }
                self.ai_terminal_session.status_message = None;
                self.ai_terminal_action_success(params, "type", "Text was sent to the terminal.")
            }
            TerminalDynamicToolRequest::Paste { tab_id, text } => {
                if let Some(error) = self.ai_prepare_terminal_input_tool(params, tab_id, cx) {
                    return error;
                }
                if !self.ai_paste_terminal_text(text.as_str(), cx) {
                    return terminal_no_running_session_response(params);
                }
                self.ai_terminal_action_success(params, "paste", "Text was pasted into the terminal.")
            }
            TerminalDynamicToolRequest::Press { tab_id, keys } => {
                if let Some(error) = self.ai_prepare_terminal_input_tool(params, tab_id, cx) {
                    return error;
                }
                if let Some(error) = self.ai_press_terminal_keys_for_tool(params, keys.as_str(), cx)
                {
                    return error;
                }
                self.ai_terminal_action_success(params, "press", "Key press was sent to the terminal.")
            }
            TerminalDynamicToolRequest::Scroll { tab_id, lines } => {
                if let Some(tab_id) = terminal_tool_tab_id(tab_id)
                    && let Some(error) = self.ai_select_terminal_tab_for_tool(params, tab_id, cx)
                {
                    return error;
                }
                if !self.ai_scroll_terminal_viewport(TerminalScroll::Delta(lines), cx) {
                    return crate::app::ai_terminal_dynamic_tools::terminal_action_rejected_response(
                        params,
                        "Terminal viewport could not be scrolled.",
                    );
                }
                self.ai_terminal_action_success(params, "scroll", "Terminal viewport was scrolled.")
            }
            TerminalDynamicToolRequest::Resize { tab_id, rows, cols } => {
                if let Some(tab_id) = terminal_tool_tab_id(tab_id)
                    && let Some(error) = self.ai_select_terminal_tab_for_tool(params, tab_id, cx)
                {
                    return error;
                }
                self.ai_resize_terminal_surface(rows, cols, cx);
                self.ai_terminal_action_success(params, "resize", "Terminal grid was resized.")
            }
            TerminalDynamicToolRequest::Kill { tab_id } => {
                if let Some(tab_id) = terminal_tool_tab_id(tab_id)
                    && let Some(error) = self.ai_select_terminal_tab_for_tool(params, tab_id, cx)
                {
                    return error;
                }
                if !self.ai_terminal_is_running() {
                    return terminal_no_running_session_response(params);
                }
                let Some(runtime) = self.ai_terminal_runtime.as_ref() else {
                    return terminal_no_running_session_response(params);
                };
                self.ai_terminal_stop_requested = true;
                if let Err(error) = runtime.handle.kill() {
                    self.ai_terminal_stop_requested = false;
                    self.ai_terminal_session.status_message = Some(error.to_string());
                    self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                    return crate::app::ai_terminal_dynamic_tools::terminal_action_rejected_response(
                        params,
                        error.to_string().as_str(),
                    );
                }
                self.ai_terminal_action_success(params, "kill", "Terminal process stop was requested.")
            }
            _ => crate::app::ai_terminal_dynamic_tools::terminal_action_rejected_response(
                params,
                "Unsupported terminal action request.",
            ),
        }
    }

    fn ai_prepare_terminal_input_tool(
        &mut self,
        params: &hunk_codex::protocol::DynamicToolCallParams,
        tab_id: Option<hunk_codex::terminal_tools::TerminalTabId>,
        cx: &mut Context<Self>,
    ) -> Option<hunk_codex::protocol::DynamicToolCallResponse> {
        if let Some(tab_id) = terminal_tool_tab_id(tab_id)
            && let Some(error) = self.ai_select_terminal_tab_for_tool(params, tab_id, cx)
        {
            return Some(error);
        }
        self.ai_terminal_set_open(true, cx);
        self.ensure_ai_terminal_session(cx);
        if self.ai_terminal_is_running() && self.ai_terminal_runtime.is_some() {
            None
        } else {
            Some(terminal_no_running_session_response(params))
        }
    }

    fn ai_press_terminal_keys_for_tool(
        &mut self,
        params: &hunk_codex::protocol::DynamicToolCallParams,
        keys: &str,
        cx: &mut Context<Self>,
    ) -> Option<hunk_codex::protocol::DynamicToolCallResponse> {
        let Some(keystroke) = terminal_tool_keystroke(keys) else {
            return Some(
                crate::app::ai_terminal_dynamic_tools::terminal_invalid_arguments_response(
                    params,
                    format!("Unsupported terminal key sequence '{keys}'.").as_str(),
                ),
            );
        };
        let terminal_mode = self.ai_terminal_session.screen.as_ref().map(|screen| screen.mode);
        if let Some(scroll) = ai_terminal_viewport_scroll_for_keystroke(&keystroke, terminal_mode) {
            if self.ai_scroll_terminal_viewport(scroll, cx) {
                return None;
            }
            return Some(
                crate::app::ai_terminal_dynamic_tools::terminal_action_rejected_response(
                    params,
                    "Terminal viewport could not be scrolled.",
                ),
            );
        }
        let Some(input) = ai_terminal_key_input_for_keystroke(&keystroke, terminal_mode) else {
            return Some(
                crate::app::ai_terminal_dynamic_tools::terminal_invalid_arguments_response(
                    params,
                    format!("Unsupported terminal key sequence '{keys}'.").as_str(),
                ),
            );
        };
        if self.ai_write_terminal_key_input(input, cx) {
            None
        } else {
            Some(terminal_no_running_session_response(params))
        }
    }

    fn ai_terminal_action_success(
        &self,
        params: &hunk_codex::protocol::DynamicToolCallParams,
        action: &str,
        message: &str,
    ) -> hunk_codex::protocol::DynamicToolCallResponse {
        let tabs = self.ai_visible_terminal_tabs_snapshot();
        crate::app::ai_terminal_dynamic_tools::terminal_action_response(
            params,
            self.ai_terminal_active_tab_id,
            tabs.as_slice(),
            action,
            message,
        )
    }

    fn ai_select_terminal_tab_for_tool(
        &mut self,
        params: &hunk_codex::protocol::DynamicToolCallParams,
        tab_id: TerminalTabId,
        cx: &mut Context<Self>,
    ) -> Option<hunk_codex::protocol::DynamicToolCallResponse> {
        if !self
            .ai_visible_terminal_tabs_snapshot()
            .iter()
            .any(|tab| tab.id == tab_id)
        {
            return Some(terminal_missing_tab_response(params, tab_id));
        }

        self.ai_select_terminal_tab(tab_id, cx);
        None
    }

    fn ai_select_terminal_tool_thread(
        &mut self,
        params: &hunk_codex::protocol::DynamicToolCallParams,
        cx: &mut Context<Self>,
    ) -> Option<hunk_codex::protocol::DynamicToolCallResponse> {
        let thread_id = params.thread_id.clone();
        let next_workspace_key = self
            .ai_thread_workspace_root(thread_id.as_str())
            .map(|root| root.to_string_lossy().to_string());
        let Some(next_workspace_key) = next_workspace_key else {
            return Some(crate::app::ai_terminal_dynamic_tools::terminal_no_workspace_response(
                params,
            ));
        };

        if self.ai_selected_thread_id.as_deref() == Some(thread_id.as_str())
            && self.ai_workspace_key().as_deref() == Some(next_workspace_key.as_str())
        {
            return None;
        }

        let previous_workspace_key = self.ai_workspace_key();
        self.ai_handle_workspace_change_to(previous_workspace_key, Some(next_workspace_key), cx);
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = true;
        self.ai_workspace_selection = None;
        self.ai_workspace_surface_last_scroll_offset = None;
        self.ai_expanded_timeline_row_ids.clear();
        self.ai_text_selection = None;
        self.ai_text_selection_drag_pointer = None;
        self.ai_text_selection_auto_scroll_task = Task::ready(());
        self.ai_new_thread_draft_active = false;
        self.ai_pending_new_thread_selection = false;
        let previous_terminal_thread_id = self.current_ai_thread_id();
        self.ai_selected_thread_id = Some(thread_id.clone());
        self.ai_review_mode_active = self.ai_review_mode_thread_ids.contains(thread_id.as_str());
        self.ai_handle_terminal_thread_change(previous_terminal_thread_id, Some(thread_id.clone()), cx);
        self.invalidate_ai_visible_frame_state_with_reason("thread");
        self.flush_ai_timeline_scroll_request();
        self.sync_ai_session_selection_from_state();
        self.sync_ai_followup_prompt_state_for_selected_thread(Some(thread_id.as_str()));
        None
    }
}

fn terminal_tool_tab_id(
    tab_id: Option<hunk_codex::terminal_tools::TerminalTabId>,
) -> Option<TerminalTabId> {
    tab_id.map(|tab_id| tab_id.get())
}

fn terminal_missing_tab_response(
    params: &hunk_codex::protocol::DynamicToolCallParams,
    tab_id: TerminalTabId,
) -> hunk_codex::protocol::DynamicToolCallResponse {
    crate::app::ai_terminal_dynamic_tools::terminal_tab_not_found_response(params, tab_id)
}

fn terminal_no_running_session_response(
    params: &hunk_codex::protocol::DynamicToolCallParams,
) -> hunk_codex::protocol::DynamicToolCallResponse {
    crate::app::ai_terminal_dynamic_tools::terminal_no_shell_session_response(params)
}

fn terminal_tool_keystroke(keys: &str) -> Option<gpui::Keystroke> {
    let normalized = terminal_tool_keystroke_name(keys)?;
    gpui::Keystroke::parse(normalized.as_str()).ok()
}

fn terminal_tool_keystroke_name(keys: &str) -> Option<String> {
    let key = keys.trim();
    if key.is_empty() {
        return None;
    }
    let normalized = key.replace('+', "-").replace(' ', "").to_ascii_lowercase();
    Some(match normalized.as_str() {
        "return" => "enter".to_string(),
        "escape" => "esc".to_string(),
        "arrowup" => "up".to_string(),
        "arrowdown" => "down".to_string(),
        "arrowleft" => "left".to_string(),
        "arrowright" => "right".to_string(),
        "pageup" => "pageup".to_string(),
        "pagedown" => "pagedown".to_string(),
        "shift-page-up" => "shift-pageup".to_string(),
        "shift-page-down" => "shift-pagedown".to_string(),
        _ => normalized,
    })
}
