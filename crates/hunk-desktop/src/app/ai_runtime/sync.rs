impl AiWorkerRuntime {
    fn refresh_thread_list(&mut self) -> Result<(), CodexIntegrationError> {
        let _ = self.refresh_thread_list_contains_thread(None)?;
        Ok(())
    }

    fn refresh_thread_list_contains_thread(
        &mut self,
        thread_id: Option<&str>,
    ) -> Result<bool, CodexIntegrationError> {
        let started_at = Instant::now();
        let response =
            self.service
                .list_threads(&mut self.session, None, Some(200), self.request_timeout)?;

        if self.service.active_thread_for_workspace().is_none()
            && let Some(first_thread) = response.data.first()
        {
            self.service
                .state_mut()
                .set_active_thread_for_cwd(self.workspace_key.clone(), first_thread.id.clone());
        }
        let contains_thread = thread_id.is_some_and(|thread_id| {
            response.data.iter().any(|thread| thread.id == thread_id)
        });
        tracing::info!(
            workspace_key = self.workspace_key.as_str(),
            requested_thread_id = ?thread_id,
            thread_count = response.data.len(),
            contains_thread,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "ai instrumentation: thread list refreshed"
        );
        Ok(contains_thread)
    }

    fn update_active_thread_after_archive(&mut self, archived_thread_id: &str) {
        if self.service.active_thread_for_workspace() == Some(archived_thread_id) {
            self.service
                .state_mut()
                .active_thread_by_cwd
                .remove(self.workspace_key.as_str());
        }

        if self.service.active_thread_for_workspace().is_some() {
            return;
        }

        let replacement_thread_id = self
            .service
            .state()
            .threads
            .values()
            .filter(|thread| {
                thread.cwd == self.workspace_key
                    && thread.status != ThreadLifecycleStatus::Archived
                    && thread.id != archived_thread_id
            })
            .max_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .map(|thread| thread.id.clone());

        if let Some(next_thread_id) = replacement_thread_id {
            self.service
                .state_mut()
                .set_active_thread_for_cwd(self.workspace_key.clone(), next_thread_id);
        }
    }

    fn reconcile_missing_rollout_thread_error(
        &mut self,
        thread_id: &str,
    ) -> Result<bool, CodexIntegrationError> {
        if self
            .service
            .state()
            .threads
            .get(thread_id)
            .is_some_and(|thread| thread.status == ThreadLifecycleStatus::Archived)
        {
            self.update_active_thread_after_archive(thread_id);
            return Ok(true);
        }

        if self.refresh_thread_list_contains_thread(Some(thread_id))? {
            return Ok(false);
        }

        self.service.mark_thread_archived_if_known(thread_id.to_string());
        self.update_active_thread_after_archive(thread_id);
        Ok(true)
    }

    fn hydrate_initial_timeline(&mut self) -> Result<(), CodexIntegrationError> {
        let started_at = Instant::now();
        let thread_id = self
            .service
            .active_thread_for_workspace()
            .filter(|thread_id| {
                self.service
                    .state()
                    .threads
                    .get(*thread_id)
                    .is_some_and(|thread| {
                        thread.cwd == self.workspace_key
                            && thread.status != ThreadLifecycleStatus::Archived
                    })
            })
            .map(ToOwned::to_owned)
            .or_else(|| {
                self.service
                    .state()
                    .threads
                    .values()
                    .filter(|thread| {
                        thread.cwd == self.workspace_key
                            && thread.status != ThreadLifecycleStatus::Archived
                    })
                    .max_by(|left, right| {
                        left.updated_at
                            .cmp(&right.updated_at)
                            .then_with(|| left.created_at.cmp(&right.created_at))
                            .then_with(|| left.id.cmp(&right.id))
                    })
                    .map(|thread| thread.id.clone())
        });
        let Some(thread_id) = thread_id else {
            tracing::info!(
                workspace_key = self.workspace_key.as_str(),
                elapsed_ms = started_at.elapsed().as_millis() as u64,
                "ai instrumentation: initial timeline hydration skipped because no thread is active"
            );
            return Ok(());
        };
        self.load_thread_snapshot(thread_id.clone())?;
        let turn_count = self
            .service
            .state()
            .turns
            .values()
            .filter(|turn| turn.thread_id == thread_id)
            .count();
        let item_count = self
            .service
            .state()
            .items
            .values()
            .filter(|item| item.thread_id == thread_id)
            .count();
        tracing::info!(
            workspace_key = self.workspace_key.as_str(),
            thread_id = thread_id.as_str(),
            turn_count,
            item_count,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "ai instrumentation: initial timeline hydrated"
        );
        Ok(())
    }

    fn refresh_session_metadata(&mut self) -> Result<(), CodexIntegrationError> {
        let started_at = Instant::now();
        self.refresh_models()?;
        self.refresh_experimental_features()?;
        self.refresh_collaboration_modes()?;
        tracing::info!(
            workspace_key = self.workspace_key.as_str(),
            model_count = self.models.len(),
            experimental_feature_count = self.experimental_features.len(),
            collaboration_mode_count = self.collaboration_modes.len(),
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "ai instrumentation: session metadata refreshed"
        );
        Ok(())
    }

    fn refresh_models(&mut self) -> Result<(), CodexIntegrationError> {
        let started_at = Instant::now();
        let mut cursor: Option<String> = None;
        let mut models = Vec::new();
        let mut pages = 0_u8;
        loop {
            pages = pages.saturating_add(1);
            let response = self.service.list_models(
                &mut self.session,
                cursor.clone(),
                Some(100),
                Some(self.include_hidden_models),
                self.request_timeout,
            )?;
            models.extend(response.data);
            cursor = response.next_cursor;
            if cursor.is_none() || pages >= 20 {
                break;
            }
        }
        self.models = models;
        tracing::debug!(
            workspace_key = self.workspace_key.as_str(),
            model_count = self.models.len(),
            page_count = pages,
            include_hidden_models = self.include_hidden_models,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "ai instrumentation: model list refreshed"
        );
        Ok(())
    }

    fn refresh_experimental_features(&mut self) -> Result<(), CodexIntegrationError> {
        let started_at = Instant::now();
        let mut cursor: Option<String> = None;
        let mut features = Vec::new();
        let mut pages = 0_u8;
        loop {
            pages = pages.saturating_add(1);
            let response = self.service.list_experimental_features(
                &mut self.session,
                cursor.clone(),
                Some(100),
                self.request_timeout,
            )?;
            features.extend(response.data);
            cursor = response.next_cursor;
            if cursor.is_none() || pages >= 20 {
                break;
            }
        }
        self.experimental_features = features;
        tracing::debug!(
            workspace_key = self.workspace_key.as_str(),
            feature_count = self.experimental_features.len(),
            page_count = pages,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "ai instrumentation: experimental feature list refreshed"
        );
        Ok(())
    }

    fn refresh_collaboration_modes(&mut self) -> Result<(), CodexIntegrationError> {
        let started_at = Instant::now();
        match self
            .service
            .list_collaboration_modes(&mut self.session, self.request_timeout)
        {
            Ok(response) => {
                self.collaboration_modes = response.data;
                tracing::debug!(
                    workspace_key = self.workspace_key.as_str(),
                    collaboration_mode_count = self.collaboration_modes.len(),
                    elapsed_ms = started_at.elapsed().as_millis() as u64,
                    "ai instrumentation: collaboration modes refreshed"
                );
                Ok(())
            }
            Err(CodexIntegrationError::JsonRpcServerError { .. }) => {
                self.collaboration_modes.clear();
                tracing::debug!(
                    workspace_key = self.workspace_key.as_str(),
                    elapsed_ms = started_at.elapsed().as_millis() as u64,
                    "ai instrumentation: collaboration mode list unavailable on server"
                );
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn refresh_account_state(&mut self) -> Result<(), CodexIntegrationError> {
        let started_at = Instant::now();
        let response = self
            .service
            .read_account(&mut self.session, false, self.request_timeout)?;
        self.account = response.account;
        self.requires_openai_auth = response.requires_openai_auth;
        let account_kind = match self.account.as_ref() {
            Some(Account::ApiKey { .. }) => "api_key",
            Some(Account::Chatgpt { .. }) => "chatgpt",
            None => "none",
        };
        tracing::info!(
            workspace_key = self.workspace_key.as_str(),
            account_kind,
            requires_openai_auth = self.requires_openai_auth,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "ai instrumentation: account state refreshed"
        );
        Ok(())
    }

    fn refresh_account_rate_limits(&mut self) -> Result<(), CodexIntegrationError> {
        let started_at = Instant::now();
        match self
            .service
            .read_account_rate_limits(&mut self.session, self.request_timeout)
        {
            Ok(response) => {
                self.apply_rate_limits_response(response);
                tracing::info!(
                    workspace_key = self.workspace_key.as_str(),
                    has_rate_limits = self.rate_limits.is_some(),
                    elapsed_ms = started_at.elapsed().as_millis() as u64,
                    "ai instrumentation: account rate limits refreshed"
                );
                Ok(())
            }
            Err(CodexIntegrationError::JsonRpcServerError { .. }) => {
                self.rate_limits = None;
                self.rate_limits_by_limit_id.clear();
                tracing::info!(
                    workspace_key = self.workspace_key.as_str(),
                    has_rate_limits = false,
                    elapsed_ms = started_at.elapsed().as_millis() as u64,
                    "ai instrumentation: account rate limits unavailable on server"
                );
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn poll_notifications(
        &mut self,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<(), CodexIntegrationError> {
        let mut captured = self
            .session
            .poll_server_notifications(NOTIFICATION_POLL_TIMEOUT)?;
        if captured > 0 {
            for _ in 1..MAX_NOTIFICATIONS_PER_POLL {
                let drained = self
                    .session
                    .poll_server_notifications(NOTIFICATION_DRAIN_TIMEOUT)?;
                if drained == 0 {
                    break;
                }
                captured += drained;
            }
        }
        let mut notifications = Vec::new();
        if captured > 0 {
            notifications = self
                .service
                .drain_and_apply_queued_notifications(&mut self.session);
        }

        let account_changed =
            self.sync_account_notifications(notifications.as_slice(), event_tx)?;
        let approvals_changed = self.sync_server_requests()?;
        if captured == 0 && !approvals_changed && !account_changed {
            return Ok(());
        }

        self.emit_snapshot(event_tx);
        Ok(())
    }

    fn emit_snapshot_after_sync(
        &mut self,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<(), CodexIntegrationError> {
        self.sync_server_requests()?;
        self.emit_snapshot(event_tx);
        Ok(())
    }

    fn sync_account_notifications(
        &mut self,
        notifications: &[ServerNotification],
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<bool, CodexIntegrationError> {
        let mut changed = false;
        let mut refresh_account = false;
        let mut refresh_rate_limits = false;

        for notification in notifications {
            match notification {
                ServerNotification::AccountUpdated(_) => {
                    refresh_account = true;
                    refresh_rate_limits = true;
                }
                ServerNotification::AccountRateLimitsUpdated(update) => {
                    self.apply_rate_limits_snapshot(update.rate_limits.clone());
                    changed = true;
                }
                ServerNotification::AccountLoginCompleted(completed) => {
                    let message = apply_login_completed_state(
                        &mut self.pending_chatgpt_login_id,
                        &mut self.pending_chatgpt_auth_url,
                        completed,
                    );
                    refresh_account = true;
                    refresh_rate_limits = true;
                    changed = true;
                    self.send_event(event_tx, AiWorkerEventPayload::Status(message));
                }
                _ => {}
            }
        }

        if refresh_account {
            self.refresh_account_state()?;
            changed = true;
        }
        if refresh_rate_limits {
            self.refresh_account_rate_limits()?;
            changed = true;
        }
        Ok(changed)
    }

    fn apply_rate_limits_response(&mut self, response: GetAccountRateLimitsResponse) {
        let fallback = response.rate_limits;
        let fallback_limit_id = fallback
            .limit_id
            .clone()
            .unwrap_or_else(|| "codex".to_string());
        let mut snapshots_by_limit_id = response.rate_limits_by_limit_id.unwrap_or_default();
        snapshots_by_limit_id
            .entry(fallback_limit_id)
            .or_insert_with(|| fallback.clone());

        self.rate_limits_by_limit_id = snapshots_by_limit_id;
        self.rate_limits =
            preferred_rate_limit_snapshot(&self.rate_limits_by_limit_id, Some(&fallback));
    }

    fn apply_rate_limits_snapshot(&mut self, snapshot: RateLimitSnapshot) {
        let limit_id = snapshot
            .limit_id
            .clone()
            .unwrap_or_else(|| "codex".to_string());
        self.rate_limits_by_limit_id
            .insert(limit_id, snapshot.clone());
        self.rate_limits =
            preferred_rate_limit_snapshot(&self.rate_limits_by_limit_id, Some(&snapshot));
    }

    fn sync_server_requests(&mut self) -> Result<bool, CodexIntegrationError> {
        let mut changed = false;
        if self.mad_max_mode && !self.pending_approvals.is_empty() {
            let queued = self.pending_approvals.keys().cloned().collect::<Vec<_>>();
            for request_id in queued {
                self.resolve_pending_approval(request_id.as_str(), AiApprovalDecision::Accept)?;
            }
            changed = true;
        }
        if self.mad_max_mode && !self.pending_user_inputs.is_empty() {
            let queued = self.pending_user_inputs.keys().cloned().collect::<Vec<_>>();
            for request_id in queued {
                let answers = self
                    .pending_user_inputs
                    .get(request_id.as_str())
                    .map(|pending| default_user_input_answers(&pending.request.questions))
                    .unwrap_or_default();
                self.submit_pending_user_input(request_id.as_str(), answers)?;
            }
            changed = true;
        }

        let requests = self.service.drain_queued_server_requests(&mut self.session);
        for request in requests {
            match request {
                ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
                    let request_id_key = request_id_key(&request_id);
                    if self.mad_max_mode {
                        self.session.respond_typed(
                            request_id.clone(),
                            &CommandExecutionRequestApprovalResponse {
                                decision: CommandExecutionApprovalDecision::Accept,
                            },
                        )?;
                        self.service.record_server_request_resolved(
                            request_id,
                            Some(params.item_id),
                            ServerRequestDecision::Accept,
                        );
                        changed = true;
                        continue;
                    }

                    let sequence = self.request_sequence_for_approval(request_id_key.as_str());
                    let approval = AiPendingApproval {
                        request_id: request_id_key.clone(),
                        thread_id: params.thread_id,
                        turn_id: params.turn_id,
                        item_id: params.item_id,
                        kind: AiApprovalKind::CommandExecution,
                        reason: params.reason,
                        command: params.command,
                        cwd: params.cwd,
                        grant_root: None,
                    };
                    self.pending_approvals.insert(
                        request_id_key,
                        PendingApproval {
                            request_id,
                            approval,
                            sequence,
                        },
                    );
                    changed = true;
                }
                ServerRequest::FileChangeRequestApproval { request_id, params } => {
                    let request_id_key = request_id_key(&request_id);
                    if self.mad_max_mode {
                        self.session.respond_typed(
                            request_id.clone(),
                            &FileChangeRequestApprovalResponse {
                                decision: FileChangeApprovalDecision::Accept,
                            },
                        )?;
                        self.service.record_server_request_resolved(
                            request_id,
                            Some(params.item_id),
                            ServerRequestDecision::Accept,
                        );
                        changed = true;
                        continue;
                    }

                    let sequence = self.request_sequence_for_approval(request_id_key.as_str());
                    let approval = AiPendingApproval {
                        request_id: request_id_key.clone(),
                        thread_id: params.thread_id,
                        turn_id: params.turn_id,
                        item_id: params.item_id,
                        kind: AiApprovalKind::FileChange,
                        reason: params.reason,
                        command: None,
                        cwd: None,
                        grant_root: params.grant_root,
                    };
                    self.pending_approvals.insert(
                        request_id_key,
                        PendingApproval {
                            request_id,
                            approval,
                            sequence,
                        },
                    );
                    changed = true;
                }
                ServerRequest::ToolRequestUserInput { request_id, params } => {
                    let request_id_key = request_id_key(&request_id);
                    let mapped_questions = params
                        .questions
                        .into_iter()
                        .map(map_pending_user_input_question)
                        .collect::<Vec<_>>();
                    if self.mad_max_mode {
                        let answers = default_user_input_answers(&mapped_questions);
                        self.session.respond_typed(
                            request_id.clone(),
                            &ToolRequestUserInputResponse {
                                answers: map_user_input_answers(answers),
                            },
                        )?;
                        changed = true;
                        continue;
                    }

                    let sequence = self.request_sequence_for_user_input(request_id_key.as_str());
                    let user_input = AiPendingUserInputRequest {
                        request_id: request_id_key.clone(),
                        thread_id: params.thread_id,
                        turn_id: params.turn_id,
                        item_id: params.item_id,
                        questions: mapped_questions,
                    };
                    self.pending_user_inputs.insert(
                        request_id_key,
                        PendingUserInput {
                            request_id,
                            request: user_input,
                            sequence,
                        },
                    );
                    changed = true;
                }
                ServerRequest::DynamicToolCall { request_id, params } => {
                    let response = self.tool_registry.execute(self.service.cwd(), &params);
                    self.session.respond_typed(request_id, &response)?;
                    changed = true;
                }
                _ => {}
            }
        }

        if self.prune_resolved_approvals() {
            changed = true;
        }
        Ok(changed)
    }

    fn resolve_pending_approval(
        &mut self,
        request_id: &str,
        decision: AiApprovalDecision,
    ) -> Result<(), CodexIntegrationError> {
        let Some(pending) = self.pending_approvals.remove(request_id) else {
            return Ok(());
        };

        let request_id_value = pending.request_id.clone();
        let item_id = pending.approval.item_id.clone();
        match pending.approval.kind {
            AiApprovalKind::CommandExecution => {
                self.session.respond_typed(
                    request_id_value.clone(),
                    &CommandExecutionRequestApprovalResponse {
                        decision: map_command_approval_decision(decision),
                    },
                )?;
            }
            AiApprovalKind::FileChange => {
                self.session.respond_typed(
                    request_id_value.clone(),
                    &FileChangeRequestApprovalResponse {
                        decision: map_file_change_approval_decision(decision),
                    },
                )?;
            }
        }

        self.service.record_server_request_resolved(
            request_id_value,
            Some(item_id),
            map_server_request_decision(decision),
        );
        Ok(())
    }

    fn submit_pending_user_input(
        &mut self,
        request_id: &str,
        answers: BTreeMap<String, Vec<String>>,
    ) -> Result<(), CodexIntegrationError> {
        let Some(pending) = self.pending_user_inputs.remove(request_id) else {
            return Ok(());
        };

        self.session.respond_typed(
            pending.request_id,
            &ToolRequestUserInputResponse {
                answers: map_user_input_answers(answers),
            },
        )
    }

    fn prune_resolved_approvals(&mut self) -> bool {
        let resolved_request_ids = self
            .service
            .state()
            .server_requests
            .iter()
            .filter(|(_, summary)| !matches!(summary.decision, ServerRequestDecision::Unknown))
            .map(|(request_id, _)| request_id.clone())
            .collect::<Vec<_>>();

        if resolved_request_ids.is_empty() {
            return false;
        }

        let previous_count = self.pending_approvals.len();
        for request_id in resolved_request_ids {
            self.pending_approvals.remove(&request_id);
        }

        previous_count != self.pending_approvals.len()
    }

    fn request_sequence_for_approval(&mut self, request_id_key: &str) -> u64 {
        if let Some(existing) = self.pending_approvals.get(request_id_key) {
            return existing.sequence;
        }

        let sequence = self.next_approval_sequence;
        self.next_approval_sequence = self.next_approval_sequence.saturating_add(1);
        sequence
    }

    fn request_sequence_for_user_input(&mut self, request_id_key: &str) -> u64 {
        if let Some(existing) = self.pending_user_inputs.get(request_id_key) {
            return existing.sequence;
        }

        let sequence = self.next_user_input_sequence;
        self.next_user_input_sequence = self.next_user_input_sequence.saturating_add(1);
        sequence
    }

    fn emit_snapshot(&self, event_tx: &Sender<AiWorkerEvent>) {
        let pending_approvals = ordered_pending_approvals(&self.pending_approvals);
        let pending_user_inputs = ordered_pending_user_inputs(&self.pending_user_inputs);
        self.send_event(
            event_tx,
            AiWorkerEventPayload::Snapshot(Box::new(AiSnapshot {
                state: self.service.state().clone(),
                active_thread_id: self
                    .service
                    .active_thread_for_workspace()
                    .map(ToOwned::to_owned),
                pending_approvals,
                pending_user_inputs,
                account: self.account.clone(),
                requires_openai_auth: self.requires_openai_auth,
                pending_chatgpt_login_id: self.pending_chatgpt_login_id.clone(),
                pending_chatgpt_auth_url: self.pending_chatgpt_auth_url.clone(),
                rate_limits: self.rate_limits.clone(),
                models: self.models.clone(),
                experimental_features: self.experimental_features.clone(),
                collaboration_modes: self.collaboration_modes.clone(),
                include_hidden_models: self.include_hidden_models,
                mad_max_mode: self.mad_max_mode,
            })),
        );
    }
}
