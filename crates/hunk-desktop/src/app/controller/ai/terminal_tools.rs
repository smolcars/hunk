type TerminalAutoReviewResult = std::result::Result<
    crate::app::ai_terminal_safety::TerminalAutoReviewAssessment,
    crate::app::ai_terminal_safety::TerminalAutoReviewParseError,
>;

struct AiTerminalReviewCompletion {
    request_id: String,
    params: hunk_codex::protocol::DynamicToolCallParams,
    review_request: crate::app::ai_terminal_safety::TerminalAutoReviewRequest,
    fallback_risk_level: crate::app::ai_terminal_dynamic_tools::TerminalRiskLevel,
    review_result: TerminalAutoReviewResult,
    response_tx: std::sync::mpsc::Sender<hunk_codex::protocol::DynamicToolCallResponse>,
}

impl DiffViewer {
    const TERMINAL_AUTO_REVIEW_DENIAL_CIRCUIT_BREAKER_LIMIT: u8 = 3;

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

        let safety_context = self.ai_terminal_safety_context(&params);
        let parsed_request =
            hunk_codex::terminal_tools::parse_terminal_dynamic_tool_request(&params);
        let Ok(request) = parsed_request else {
            let response = self.ai_execute_terminal_dynamic_tool_with_safety(
                &params,
                crate::app::ai_terminal_dynamic_tools::TerminalToolSafetyMode::Enforce,
                cx,
            );
            self.ai_send_terminal_tool_response(response_tx, response, cx);
            return;
        };
        let auto_review_mode = self.ai_terminal_auto_review_mode();

        match crate::app::ai_terminal_safety::evaluate_terminal_action_prefilter(
            &request,
            &safety_context,
            Some(params.turn_id.as_str()),
        ) {
            crate::app::ai_terminal_safety::TerminalPrefilterDecision::Allow {
                summary,
                evidence,
            } => {
                if auto_review_mode == TerminalAutoReviewMode::StrictConfirmation
                    && !terminal_request_is_read_only(&request)
                {
                    let confirmation = terminal_manual_confirmation(
                        summary,
                        "Strict terminal approval mode requires confirmation for this action.",
                        evidence,
                    );
                    self.ai_enqueue_pending_terminal_approval(
                        params,
                        confirmation,
                        AiPendingTerminalApprovalStatus::NeedsUserConfirmation,
                        response_tx,
                        cx,
                    );
                    return;
                }
                let response = self.ai_execute_terminal_dynamic_tool_with_safety(
                    &params,
                    crate::app::ai_terminal_dynamic_tools::TerminalToolSafetyMode::Enforce,
                cx,
                );
                self.ai_send_terminal_tool_response(response_tx, response, cx);
            }
            crate::app::ai_terminal_safety::TerminalPrefilterDecision::ReviewRequired {
                request,
                fallback_risk_level,
                evidence,
            } => {
                if auto_review_mode != TerminalAutoReviewMode::AutoReview {
                    let confirmation = self.ai_terminal_auto_review_unavailable_confirmation(
                        &request,
                        fallback_risk_level,
                        evidence,
                        auto_review_mode.confirmation_reason(),
                    );
                    self.ai_enqueue_pending_terminal_approval(
                        params,
                        confirmation,
                        AiPendingTerminalApprovalStatus::NeedsUserConfirmation,
                        response_tx,
                        cx,
                    );
                    return;
                }
                self.ai_start_terminal_auto_review(
                    params,
                    *request,
                    fallback_risk_level,
                    evidence,
                    response_tx,
                    cx,
                );
            }
            crate::app::ai_terminal_safety::TerminalPrefilterDecision::Reject(rejection) => {
                let response =
                    crate::app::ai_terminal_dynamic_tools::terminal_safety_rejected_response(
                        &params, &rejection,
                    );
                self.ai_send_terminal_tool_response(response_tx, response, cx);
            }
        }
    }

    fn ai_terminal_auto_review_mode(&self) -> TerminalAutoReviewMode {
        TerminalAutoReviewMode::from_environment().unwrap_or(TerminalAutoReviewMode::AutoReview)
    }

    fn ai_start_terminal_auto_review(
        &mut self,
        params: hunk_codex::protocol::DynamicToolCallParams,
        review_request: crate::app::ai_terminal_safety::TerminalAutoReviewRequest,
        fallback_risk_level: crate::app::ai_terminal_dynamic_tools::TerminalRiskLevel,
        fallback_evidence: Vec<String>,
        response_tx: std::sync::mpsc::Sender<hunk_codex::protocol::DynamicToolCallResponse>,
        cx: &mut Context<Self>,
    ) {
        let request_id = params.call_id.clone();
        if self.ai_terminal_request_is_pending(request_id.as_str()) {
            let response = crate::app::ai_terminal_dynamic_tools::terminal_unavailable_response(
                &params,
                "This terminal action is already waiting for review or user confirmation.",
            );
            self.ai_send_terminal_tool_response(response_tx, response, cx);
            return;
        }

        let Some(codex_home) = crate::app::ai_paths::resolve_codex_home_path() else {
            let confirmation = self.ai_terminal_auto_review_unavailable_confirmation(
                &review_request,
                fallback_risk_level,
                fallback_evidence,
                "unable to resolve Codex home",
            );
            self.ai_enqueue_pending_terminal_approval(
                params,
                confirmation,
                AiPendingTerminalApprovalStatus::AutoReviewFailed,
                response_tx,
                cx,
            );
            return;
        };
        let Some(cwd) = self.ai_workspace_cwd() else {
            let confirmation = self.ai_terminal_auto_review_unavailable_confirmation(
                &review_request,
                fallback_risk_level,
                fallback_evidence,
                "unable to resolve workspace cwd",
            );
            self.ai_enqueue_pending_terminal_approval(
                params,
                confirmation,
                AiPendingTerminalApprovalStatus::AutoReviewFailed,
                response_tx,
                cx,
            );
            return;
        };
        let codex_executable = Self::resolve_codex_executable_path();
        if let Err(error) = Self::validate_codex_executable_path(codex_executable.as_path()) {
            let confirmation = self.ai_terminal_auto_review_unavailable_confirmation(
                &review_request,
                fallback_risk_level,
                fallback_evidence,
                error.as_str(),
            );
            self.ai_enqueue_pending_terminal_approval(
                params,
                confirmation,
                AiPendingTerminalApprovalStatus::AutoReviewFailed,
                response_tx,
                cx,
            );
            return;
        }

        self.ai_status_message = Some("Reviewing terminal action...".to_string());
        cx.notify();

        let task_request_id = request_id.clone();
        let task_params = params.clone();
        let task_review_request = review_request;
        let task_response_tx = response_tx.clone();
        let task_fallback_model = self.ai_selected_model.clone();
        let task = cx.spawn(async move |this, cx| {
            let background_review_request = task_review_request.clone();
            let review_result = cx
                .background_executor()
                .spawn(async move {
                    crate::app::ai_terminal_review::run_terminal_auto_review(
                        codex_home.as_path(),
                        cwd.as_path(),
                        codex_executable.as_path(),
                        &background_review_request,
                        task_fallback_model.as_deref(),
                    )
                })
                .await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |this, cx| {
                this.ai_finish_terminal_auto_review(
                    AiTerminalReviewCompletion {
                        request_id: task_request_id,
                        params: task_params,
                        review_request: task_review_request,
                        fallback_risk_level,
                        review_result,
                        response_tx: task_response_tx,
                    },
                    cx,
                );
            });
        });
        self.ai_pending_terminal_reviews.insert(
            request_id,
            AiPendingTerminalReview {
                params,
                response_tx,
                _task: task,
            },
        );
    }

    fn ai_finish_terminal_auto_review(
        &mut self,
        completion: AiTerminalReviewCompletion,
        cx: &mut Context<Self>,
    ) {
        let AiTerminalReviewCompletion {
            request_id,
            params,
            review_request,
            fallback_risk_level,
            review_result,
            response_tx,
        } = completion;
        self.ai_pending_terminal_reviews.remove(request_id.as_str());
        match review_result {
            Ok(assessment) => {
                match crate::app::ai_terminal_safety::TerminalAutoReviewPolicy::decide(
                    &review_request,
                    assessment,
                ) {
                    crate::app::ai_terminal_safety::TerminalAutoReviewPolicyDecision::Execute {
                        ..
                    } => {
                        let response = self.ai_execute_terminal_dynamic_tool_with_safety(
                            &params,
                            crate::app::ai_terminal_dynamic_tools::TerminalToolSafetyMode::AllowSensitiveOnce,
                            cx,
                        );
                        self.ai_status_message = Some("Terminal action auto-approved.".to_string());
                        self.ai_send_terminal_tool_response(response_tx, response, cx);
                    }
                    crate::app::ai_terminal_safety::TerminalAutoReviewPolicyDecision::Confirm(
                        confirmation,
                    ) => {
                        self.ai_enqueue_pending_terminal_approval(
                            params,
                            confirmation,
                            AiPendingTerminalApprovalStatus::NeedsUserConfirmation,
                            response_tx,
                            cx,
                        );
                    }
                    crate::app::ai_terminal_safety::TerminalAutoReviewPolicyDecision::Reject(
                        rejection,
                    ) => {
                        let denial_count =
                            self.ai_record_terminal_auto_review_denial(params.thread_id.as_str());
                        if denial_count >= Self::TERMINAL_AUTO_REVIEW_DENIAL_CIRCUIT_BREAKER_LIMIT
                        {
                            self.ai_interrupt_terminal_turn_after_repeated_denials(
                                params.thread_id.as_str(),
                                cx,
                            );
                            let mut rejection = rejection;
                            rejection.evidence.push(format!(
                                "terminal auto-review denied {denial_count} action(s) in this thread"
                            ));
                            let response =
                                crate::app::ai_terminal_dynamic_tools::terminal_safety_rejected_response(
                                    &params, &rejection,
                                );
                            self.ai_status_message = Some(
                                "Terminal action rejected after repeated auto-review denials."
                                    .to_string(),
                            );
                            self.ai_send_terminal_tool_response(response_tx, response, cx);
                        } else {
                            let confirmation = terminal_confirmation_from_rejection(rejection);
                            self.ai_enqueue_pending_terminal_approval(
                                params,
                                confirmation,
                                AiPendingTerminalApprovalStatus::AutoReviewDenied,
                                response_tx,
                                cx,
                            );
                        }
                    }
                }
            }
            Err(error) => {
                let confirmation =
                    crate::app::ai_terminal_safety::terminal_auto_review_parse_failure_confirmation(
                        &review_request,
                        fallback_risk_level,
                        &error,
                    );
                self.ai_enqueue_pending_terminal_approval(
                    params,
                    confirmation,
                    terminal_auto_review_failure_status(&error),
                    response_tx,
                    cx,
                );
            }
        }
    }

    fn ai_record_terminal_auto_review_denial(&mut self, thread_id: &str) -> u8 {
        let count = self
            .ai_terminal_auto_review_denials_by_thread
            .entry(thread_id.to_string())
            .or_insert(0);
        *count = count.saturating_add(1);
        *count
    }

    fn ai_interrupt_terminal_turn_after_repeated_denials(
        &mut self,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(turn_id) = self.current_ai_in_progress_turn_id(thread_id) {
            let _ = self.send_ai_worker_command(
                AiWorkerCommand::InterruptTurn {
                    thread_id: thread_id.to_string(),
                    turn_id,
                },
                cx,
            );
        }
        self.ai_cancel_pending_terminal_approvals_for_thread(
            Some(thread_id),
            "The embedded terminal confirmation was cancelled after repeated auto-review denials.",
        );
    }

    fn ai_terminal_request_is_pending(&self, request_id: &str) -> bool {
        self.ai_pending_terminal_reviews.contains_key(request_id)
            || self
                .ai_pending_terminal_approvals
                .iter()
                .any(|approval| approval.request_id == request_id)
    }

    fn ai_cancel_pending_terminal_reviews_for_thread(
        &mut self,
        thread_id: Option<&str>,
        message: &str,
    ) {
        let request_ids = self
            .ai_pending_terminal_reviews
            .iter()
            .filter(|(_, review)| {
                thread_id.is_none_or(|thread_id| review.params.thread_id == thread_id)
            })
            .map(|(request_id, _)| request_id.clone())
            .collect::<Vec<_>>();
        for request_id in request_ids {
            let Some(review) = self.ai_pending_terminal_reviews.remove(request_id.as_str()) else {
                continue;
            };
            let response = crate::app::ai_terminal_dynamic_tools::terminal_unavailable_response(
                &review.params,
                message,
            );
            let _ = review.response_tx.send(response);
        }
    }

    fn ai_cancel_pending_terminal_approvals_for_thread(
        &mut self,
        thread_id: Option<&str>,
        message: &str,
    ) {
        let mut index = 0;
        while index < self.ai_pending_terminal_approvals.len() {
            if thread_id.is_some_and(|thread_id| {
                self.ai_pending_terminal_approvals[index].params.thread_id != thread_id
            }) {
                index += 1;
                continue;
            }
            let pending = self.ai_pending_terminal_approvals.remove(index);
            let response = crate::app::ai_terminal_dynamic_tools::terminal_unavailable_response(
                &pending.params,
                message,
            );
            let _ = pending.response_tx.send(response);
        }
    }

    fn ai_terminal_auto_review_unavailable_confirmation(
        &self,
        review_request: &crate::app::ai_terminal_safety::TerminalAutoReviewRequest,
        fallback_risk_level: crate::app::ai_terminal_dynamic_tools::TerminalRiskLevel,
        mut evidence: Vec<String>,
        reason: &str,
    ) -> crate::app::ai_terminal_dynamic_tools::TerminalToolConfirmation {
        evidence.push(format!("terminal auto-review unavailable: {reason}"));
        crate::app::ai_terminal_dynamic_tools::TerminalToolConfirmation {
            risk_level: fallback_risk_level,
            user_authorization:
                crate::app::ai_terminal_dynamic_tools::TerminalUserAuthorization::Unknown,
            outcome: crate::app::ai_terminal_dynamic_tools::TerminalAutoReviewOutcome::Confirm,
            summary: crate::app::ai_terminal_dynamic_tools::redact_terminal_tool_text(
                review_request.summary.as_str(),
            ),
            rationale: "Terminal auto-review is unavailable; user confirmation is required."
                .to_string(),
            evidence,
        }
    }

    fn ai_enqueue_pending_terminal_approval(
        &mut self,
        params: hunk_codex::protocol::DynamicToolCallParams,
        confirmation: crate::app::ai_terminal_dynamic_tools::TerminalToolConfirmation,
        status: AiPendingTerminalApprovalStatus,
        response_tx: std::sync::mpsc::Sender<hunk_codex::protocol::DynamicToolCallResponse>,
        cx: &mut Context<Self>,
    ) {
        let request_id = params.call_id.clone();
        if self.ai_terminal_request_is_pending(request_id.as_str()) {
            let response = crate::app::ai_terminal_dynamic_tools::terminal_unavailable_response(
                &params,
                "This terminal action is already waiting for review or user confirmation.",
            );
            self.ai_send_terminal_tool_response(response_tx, response, cx);
            return;
        }

        self.ai_pending_terminal_approvals
            .push(AiPendingTerminalApproval {
                request_id,
                params,
                status,
                risk_level: confirmation.risk_level,
                user_authorization: confirmation.user_authorization,
                outcome: confirmation.outcome,
                summary: confirmation.summary,
                rationale: confirmation.rationale,
                evidence: confirmation.evidence,
                response_tx,
            });
        self.ai_status_message = Some("Terminal action needs confirmation.".to_string());
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        cx.notify();
    }

    fn ai_send_terminal_tool_response(
        &mut self,
        response_tx: std::sync::mpsc::Sender<hunk_codex::protocol::DynamicToolCallResponse>,
        response: hunk_codex::protocol::DynamicToolCallResponse,
        cx: &mut Context<Self>,
    ) {
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
            AiApprovalDecision::Accept => {
                self.ai_terminal_auto_review_denials_by_thread
                    .remove(pending.params.thread_id.as_str());
                self.ai_execute_terminal_dynamic_tool_with_safety(
                    &pending.params,
                    crate::app::ai_terminal_dynamic_tools::TerminalToolSafetyMode::AllowSensitiveOnce,
                    cx,
                )
            }
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

        if safety_mode == crate::app::ai_terminal_dynamic_tools::TerminalToolSafetyMode::Enforce {
            let context = self.ai_terminal_safety_context(params);
            if let Some(preflight) =
                crate::app::ai_terminal_dynamic_tools::terminal_dynamic_tool_preflight(
                    params,
                    &context,
                )
            {
                match preflight {
                    crate::app::ai_terminal_dynamic_tools::TerminalToolPreflight::Confirm(
                        confirmation,
                    ) => {
                        return crate::app::ai_terminal_dynamic_tools::terminal_confirmation_required_response(
                            params,
                            &confirmation,
                        );
                    }
                    crate::app::ai_terminal_dynamic_tools::TerminalToolPreflight::Reject(
                        rejection,
                    ) => {
                        return crate::app::ai_terminal_dynamic_tools::terminal_safety_rejected_response(
                            params,
                            &rejection,
                        );
                    }
                }
            }
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

    fn ai_terminal_safety_context(
        &self,
        params: &hunk_codex::protocol::DynamicToolCallParams,
    ) -> crate::app::ai_terminal_dynamic_tools::TerminalSafetyContext {
        let tabs = self.ai_visible_terminal_tabs_snapshot();
        let requested_tab_id = hunk_codex::terminal_tools::parse_terminal_dynamic_tool_request(params)
            .ok()
            .and_then(|request| terminal_tool_request_tab_id(&request));
        let target_tab_id = requested_tab_id.unwrap_or(self.ai_terminal_active_tab_id);
        let target_tab = tabs.iter().find(|tab| tab.id == target_tab_id);
        let visible_snapshot = target_tab
            .and_then(|tab| tab.session.screen.as_deref())
            .map(|screen| {
                crate::app::ai_terminal_dynamic_tools::terminal_visible_text(screen)
                    .join("\n")
            });
        crate::app::ai_terminal_dynamic_tools::TerminalSafetyContext {
            thread_id: params.thread_id.clone(),
            workspace_key: self.ai_workspace_key(),
            cwd: self.ai_workspace_cwd().map(|cwd| cwd.display().to_string()),
            tab_id: Some(target_tab_id),
            active_tab_id: Some(self.ai_terminal_active_tab_id),
            available_tab_ids: tabs.iter().map(|tab| tab.id).collect(),
            target_tab_title: target_tab.map(|tab| tab.title.clone()),
            target_tab_status: target_tab.map(|tab| {
                crate::app::ai_terminal_dynamic_tools::terminal_status_label(tab.session.status)
                    .to_string()
            }),
            target_tab_exit_code: target_tab.and_then(|tab| tab.session.exit_code),
            target_tab_last_command: target_tab
                .and_then(|tab| tab.session.last_command.as_ref())
                .cloned(),
            shell_session_available: target_tab
                .is_some_and(|tab| matches!(tab.session.status, AiTerminalSessionStatus::Running)),
            shell: match ai_terminal_default_shell_family(&self.config) {
                AiTerminalShellFamily::Posix => {
                    crate::app::ai_terminal_dynamic_tools::TerminalShellKind::Posix
                }
                AiTerminalShellFamily::PowerShell => {
                    crate::app::ai_terminal_dynamic_tools::TerminalShellKind::PowerShell
                }
                AiTerminalShellFamily::Cmd => {
                    crate::app::ai_terminal_dynamic_tools::TerminalShellKind::Cmd
                }
            },
            platform: ai_terminal_safety_platform(),
            user_intent: None,
            visible_snapshot,
            recent_logs: ai_terminal_review_log_entries(
                target_tab
                    .map(|tab| tab.session.transcript.as_str())
                    .unwrap_or(self.ai_terminal_session.transcript.as_str()),
            ),
            recent_thread_context: ai_terminal_recent_thread_context(
                &self.ai_state_snapshot,
                params.thread_id.as_str(),
            ),
            browser_context: self.ai_terminal_browser_review_context(params.thread_id.as_str()),
        }
    }

    fn ai_terminal_browser_review_context(&self, thread_id: &str) -> Option<String> {
        if !self.ai_browser_open_thread_ids.contains(thread_id) {
            return None;
        }
        let session = self.ai_browser_runtime.session(thread_id)?;
        let active_tab_id = session.active_tab_id();
        let snapshot = session.latest_snapshot();
        let elements = snapshot
            .elements
            .iter()
            .take(20)
            .map(|element| {
                serde_json::json!({
                    "index": element.index,
                    "role": ai_terminal_review_truncate_text(element.role.as_str(), 80),
                    "label": ai_terminal_review_truncate_text(element.label.as_str(), 160),
                    "text": ai_terminal_review_truncate_text(element.text.as_str(), 240),
                })
            })
            .collect::<Vec<_>>();
        let console_entries = session
            .recent_console_entries_for_tab(active_tab_id, None, None, 20)
            .into_iter()
            .map(|entry| {
                serde_json::json!({
                    "sequence": entry.sequence,
                    "level": format!("{:?}", entry.level),
                    "message": ai_terminal_review_truncate_text(entry.message.as_str(), 400),
                    "source": entry.source.map(|source| ai_terminal_review_truncate_text(source.as_str(), 240)),
                    "line": entry.line,
                })
            })
            .collect::<Vec<_>>();
        let context = serde_json::json!({
            "activeTabId": active_tab_id,
            "tabs": session.tab_summaries(),
            "snapshot": {
                "epoch": snapshot.epoch,
                "url": snapshot.url.as_ref(),
                "title": snapshot.title.as_ref(),
                "elements": elements,
            },
            "console": console_entries,
        });
        serde_json::to_string(&context)
            .ok()
            .map(|context| ai_terminal_review_truncate_text(context.as_str(), 12_000))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalAutoReviewMode {
    AutoReview,
    UserConfirmationOnly,
    StrictConfirmation,
}

impl TerminalAutoReviewMode {
    fn from_environment() -> Option<Self> {
        let mode = std::env::var("HUNK_TERMINAL_AUTO_REVIEW_MODE").ok()?;
        match mode.trim().to_ascii_lowercase().as_str() {
            "auto" | "auto-review" | "autoreview" => Some(Self::AutoReview),
            "user" | "user-only" | "confirm" | "confirmation" => {
                Some(Self::UserConfirmationOnly)
            }
            "strict" | "strict-confirmation" => Some(Self::StrictConfirmation),
            _ => None,
        }
    }

    const fn confirmation_reason(self) -> &'static str {
        match self {
            Self::AutoReview => "terminal auto-review is enabled",
            Self::UserConfirmationOnly => {
                "terminal auto-review is disabled by approval mode"
            }
            Self::StrictConfirmation => {
                "strict terminal approval mode requires user confirmation"
            }
        }
    }
}

fn terminal_tool_request_tab_id(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
) -> Option<TerminalTabId> {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

    match request {
        TerminalDynamicToolRequest::Open { tab_id }
        | TerminalDynamicToolRequest::Snapshot { tab_id, .. }
        | TerminalDynamicToolRequest::Logs { tab_id, .. }
        | TerminalDynamicToolRequest::Run { tab_id, .. }
        | TerminalDynamicToolRequest::Type { tab_id, .. }
        | TerminalDynamicToolRequest::Paste { tab_id, .. }
        | TerminalDynamicToolRequest::Press { tab_id, .. }
        | TerminalDynamicToolRequest::Scroll { tab_id, .. }
        | TerminalDynamicToolRequest::Resize { tab_id, .. }
        | TerminalDynamicToolRequest::Kill { tab_id } => tab_id.map(|id| id.get()),
        TerminalDynamicToolRequest::SelectTab { tab_id }
        | TerminalDynamicToolRequest::CloseTab { tab_id } => Some(tab_id.get()),
        TerminalDynamicToolRequest::Tabs | TerminalDynamicToolRequest::NewTab { .. } => None,
    }
}

fn terminal_request_is_read_only(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
) -> bool {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;
    matches!(
        request,
        TerminalDynamicToolRequest::Tabs
            | TerminalDynamicToolRequest::Snapshot { .. }
            | TerminalDynamicToolRequest::Logs { .. }
    )
}

fn ai_terminal_review_log_entries(
    transcript: &str,
) -> Vec<crate::app::ai_terminal_dynamic_tools::TerminalAutoReviewLogEntry> {
    transcript
        .lines()
        .rev()
        .take(50)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .enumerate()
        .map(|(index, line)| crate::app::ai_terminal_dynamic_tools::TerminalAutoReviewLogEntry {
            sequence: index as u64 + 1,
            text: line.to_string(),
        })
        .collect()
}

fn ai_terminal_recent_thread_context(
    state: &hunk_codex::state::AiState,
    thread_id: &str,
) -> Vec<crate::app::ai_terminal_dynamic_tools::TerminalAutoReviewLogEntry> {
    let mut entries = state
        .items
        .values()
        .filter(|item| item.thread_id == thread_id)
        .filter_map(|item| {
            let content = if item.content.trim().is_empty() {
                item.display_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.summary.as_ref())
                    .map(String::as_str)
                    .unwrap_or("")
            } else {
                item.content.as_str()
            };
            let content = content.trim();
            if content.is_empty() {
                return None;
            }
            Some(crate::app::ai_terminal_dynamic_tools::TerminalAutoReviewLogEntry {
                sequence: item.last_sequence,
                text: format!(
                    "{} {:?}: {}",
                    item.kind,
                    item.status,
                    ai_terminal_review_truncate_text(content, 800)
                ),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.sequence);
    if entries.len() > 12 {
        entries.drain(0..entries.len() - 12);
    }
    entries
}

fn ai_terminal_review_truncate_text(text: &str, max_chars: usize) -> String {
    let mut truncated = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        truncated.push_str("...");
    }
    truncated
}

fn terminal_confirmation_from_rejection(
    rejection: crate::app::ai_terminal_dynamic_tools::TerminalToolRejection,
) -> crate::app::ai_terminal_dynamic_tools::TerminalToolConfirmation {
    crate::app::ai_terminal_dynamic_tools::TerminalToolConfirmation {
        risk_level: rejection.risk_level,
        user_authorization: rejection.user_authorization,
        outcome: rejection.outcome,
        summary: rejection.summary,
        rationale: rejection.rationale,
        evidence: rejection.evidence,
    }
}

fn terminal_manual_confirmation(
    summary: String,
    rationale: &str,
    evidence: Vec<String>,
) -> crate::app::ai_terminal_dynamic_tools::TerminalToolConfirmation {
    crate::app::ai_terminal_dynamic_tools::TerminalToolConfirmation {
        risk_level: crate::app::ai_terminal_dynamic_tools::TerminalRiskLevel::Low,
        user_authorization:
            crate::app::ai_terminal_dynamic_tools::TerminalUserAuthorization::Unknown,
        outcome: crate::app::ai_terminal_dynamic_tools::TerminalAutoReviewOutcome::Confirm,
        summary: crate::app::ai_terminal_dynamic_tools::redact_terminal_tool_text(
            summary.as_str(),
        ),
        rationale: rationale.to_string(),
        evidence: evidence
            .into_iter()
            .map(|item| crate::app::ai_terminal_dynamic_tools::redact_terminal_tool_text(&item))
            .collect(),
    }
}

fn terminal_auto_review_failure_status(
    error: &crate::app::ai_terminal_safety::TerminalAutoReviewParseError,
) -> AiPendingTerminalApprovalStatus {
    let message = error.message.to_ascii_lowercase();
    if message.contains("timeout") || message.contains("timed out") {
        AiPendingTerminalApprovalStatus::AutoReviewTimedOut
    } else {
        AiPendingTerminalApprovalStatus::AutoReviewFailed
    }
}

fn ai_terminal_safety_platform() -> crate::app::ai_terminal_dynamic_tools::TerminalPlatform {
    if cfg!(target_os = "macos") {
        crate::app::ai_terminal_dynamic_tools::TerminalPlatform::MacOS
    } else if cfg!(target_os = "linux") {
        crate::app::ai_terminal_dynamic_tools::TerminalPlatform::Linux
    } else if cfg!(target_os = "windows") {
        crate::app::ai_terminal_dynamic_tools::TerminalPlatform::Windows
    } else {
        crate::app::ai_terminal_dynamic_tools::TerminalPlatform::Unknown
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
