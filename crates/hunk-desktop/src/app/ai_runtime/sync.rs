impl AiWorkerRuntime {
    fn refresh_thread_list(&mut self) -> Result<(), CodexIntegrationError> {
        let _ = self.refresh_thread_list_contains_thread(None)?;
        Ok(())
    }

    fn refresh_thread_list_contains_thread(
        &mut self,
        thread_id: Option<&str>,
    ) -> Result<bool, CodexIntegrationError> {
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
            return Ok(());
        };
        self.load_thread_snapshot(thread_id.clone())?;
        Ok(())
    }

    fn refresh_session_metadata(&mut self) -> Result<(), CodexIntegrationError> {
        self.refresh_models()?;
        self.refresh_experimental_features()?;
        self.refresh_collaboration_modes()?;
        self.refresh_skills()?;
        Ok(())
    }

    fn refresh_models(&mut self) -> Result<(), CodexIntegrationError> {
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
        Ok(())
    }

    fn refresh_experimental_features(&mut self) -> Result<(), CodexIntegrationError> {
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
        Ok(())
    }

    fn refresh_collaboration_modes(&mut self) -> Result<(), CodexIntegrationError> {
        match self
            .service
            .list_collaboration_modes(&mut self.session, self.request_timeout)
        {
            Ok(response) => {
                self.collaboration_modes = response.data;
                Ok(())
            }
            Err(CodexIntegrationError::JsonRpcServerError { .. }) => {
                self.collaboration_modes.clear();
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn refresh_skills(&mut self) -> Result<(), CodexIntegrationError> {
        match self
            .service
            .list_skills(&mut self.session, false, self.request_timeout)
        {
            Ok(response) => {
                let workspace_cwd = std::path::PathBuf::from(self.workspace_key.as_str());
                self.skills = response
                    .data
                    .iter()
                    .find(|entry| entry.cwd == workspace_cwd)
                    .or_else(|| response.data.first())
                    .cloned()
                    .map(|entry| entry.skills)
                    .unwrap_or_default();
                Ok(())
            }
            Err(CodexIntegrationError::JsonRpcServerError { .. }) => {
                self.skills.clear();
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn refresh_account_state(&mut self) -> Result<(), CodexIntegrationError> {
        let response = self
            .service
            .read_account(&mut self.session, false, self.request_timeout)?;
        self.account = response.account;
        self.requires_openai_auth = response.requires_openai_auth;
        Ok(())
    }

    fn refresh_account_rate_limits(&mut self) -> Result<(), CodexIntegrationError> {
        match self
            .service
            .read_account_rate_limits(&mut self.session, self.request_timeout)
        {
            Ok(response) => {
                self.apply_rate_limits_response(response);
                Ok(())
            }
            Err(CodexIntegrationError::JsonRpcServerError { .. }) => {
                self.rate_limits = None;
                self.rate_limits_by_limit_id.clear();
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn poll_notifications(
        &mut self,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<(), CodexIntegrationError> {
        let drained =
            self.drain_transport_events(NOTIFICATION_POLL_TIMEOUT, MAX_NOTIFICATIONS_PER_POLL, event_tx)?;
        let session_changed =
            self.sync_session_notifications(drained.notifications.as_slice(), event_tx)?;
        let approvals_changed = self.sync_server_requests(drained.server_requests)?;
        if drained.received == 0 && drained.lagged == 0 && !approvals_changed && !session_changed {
            return Ok(());
        }

        self.emit_snapshot(event_tx);
        Ok(())
    }

    fn emit_snapshot_after_sync(
        &mut self,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<(), CodexIntegrationError> {
        let drained =
            self.drain_transport_events(Duration::ZERO, MAX_NOTIFICATIONS_PER_POLL, event_tx)?;
        self.sync_session_notifications(drained.notifications.as_slice(), event_tx)?;
        self.sync_server_requests(drained.server_requests)?;
        self.emit_snapshot(event_tx);
        Ok(())
    }

    fn drain_transport_events(
        &mut self,
        initial_wait: Duration,
        max_events: usize,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<TransportEventDrain, CodexIntegrationError> {
        let mut drained = TransportEventDrain::default();
        let mut wait = initial_wait;

        while drained.received < max_events {
            let Some(event) = self.session.next_event(wait)? else {
                break;
            };
            drained.received = drained.received.saturating_add(1);
            wait = Duration::ZERO;

            match event {
                AppServerEvent::ServerNotification(notification) => {
                    self.service.apply_server_notification(notification.clone());
                    self.mark_stream_activity_from_notification(&notification, Instant::now());
                    drained.notifications.push(notification);
                }
                AppServerEvent::ServerRequest(request) => {
                    drained.server_requests.push(request);
                }
                AppServerEvent::Lagged { skipped } => {
                    drained.lagged = drained.lagged.saturating_add(skipped);
                }
                AppServerEvent::Disconnected { message } => {
                    return Err(CodexIntegrationError::WebSocketTransport(message));
                }
            }
        }

        if drained.lagged > 0 {
            self.send_event(
                event_tx,
                AiWorkerEventPayload::Status(format!(
                    "AI event stream lagged; dropped {} non-critical updates.",
                    drained.lagged
                )),
            );
        }

        Ok(drained)
    }

    fn sync_session_notifications(
        &mut self,
        notifications: &[ServerNotification],
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<bool, CodexIntegrationError> {
        let flags = notification_refresh_flags(notifications);
        let mut changed = false;

        for notification in notifications {
            match notification {
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
                    changed = true;
                    self.send_event(event_tx, AiWorkerEventPayload::Status(message));
                }
                _ => {}
            }
        }

        if flags.refresh_account {
            self.refresh_account_state()?;
            changed = true;
        }
        if flags.refresh_rate_limits {
            self.refresh_account_rate_limits()?;
            changed = true;
        }
        if flags.refresh_skills {
            self.refresh_skills()?;
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

    fn sync_server_requests(
        &mut self,
        requests: Vec<ServerRequest>,
    ) -> Result<bool, CodexIntegrationError> {
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

        for request in requests {
            match request {
                ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
                    self.mark_stream_activity(
                        params.thread_id.as_str(),
                        params.turn_id.as_str(),
                        Instant::now(),
                    );
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
                        cwd: params.cwd.map(|cwd| cwd.to_path_buf()),
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
                    self.mark_stream_activity(
                        params.thread_id.as_str(),
                        params.turn_id.as_str(),
                        Instant::now(),
                    );
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
                    self.mark_stream_activity(
                        params.thread_id.as_str(),
                        params.turn_id.as_str(),
                        Instant::now(),
                    );
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
                    self.mark_stream_activity(
                        params.thread_id.as_str(),
                        params.turn_id.as_str(),
                        Instant::now(),
                    );
                    let response = self
                        .dynamic_tool_executor
                        .execute(self.service.cwd(), &params);
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

    fn emit_snapshot(&mut self, event_tx: &Sender<AiWorkerEvent>) {
        self.sync_stream_watches_from_state(Instant::now());
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
                skills: self.skills.clone(),
                include_hidden_models: self.include_hidden_models,
                mad_max_mode: self.mad_max_mode,
            })),
        );
    }

    fn maybe_recover_stalled_turns(
        &mut self,
        config: &AiWorkerStartConfig,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<(), CodexIntegrationError> {
        let now = Instant::now();
        self.sync_stream_watches_from_state(now);

        let stalled = self
            .turn_stream_watches
            .values()
            .filter(|watch| !self.turn_is_waiting_for_user_action(&watch.thread_id, &watch.turn_id))
            .filter(|watch| {
                now.duration_since(watch.last_meaningful_activity_at) >= STREAM_STALL_THRESHOLD
            })
            .filter(|watch| {
                watch.last_recovery_at.is_none_or(|last_recovery| {
                    now.duration_since(last_recovery) >= STREAM_STALL_RECOVERY_COOLDOWN
                })
            })
            .map(|watch| {
                (
                    watch.thread_id.clone(),
                    watch.turn_id.clone(),
                    watch.soft_recovery_attempts,
                    now.duration_since(watch.last_meaningful_activity_at),
                )
            })
            .collect::<Vec<_>>();

        let Some((thread_id, turn_id, soft_recovery_attempts, stalled_for)) =
            stalled.into_iter().next()
        else {
            return Ok(());
        };

        tracing::warn!(
            thread_id = thread_id.as_str(),
            turn_id = turn_id.as_str(),
            stalled_for_ms = stalled_for.as_millis() as u64,
            soft_recovery_attempts,
            "detected stalled AI stream"
        );

        if soft_recovery_attempts < STREAM_STALL_MAX_SOFT_RECOVERIES {
            if let Some(watch) = self.turn_stream_watches.get_mut(turn_id.as_str()) {
                watch.last_recovery_at = Some(now);
                watch.last_meaningful_activity_at = now;
                watch.soft_recovery_attempts =
                    watch.soft_recovery_attempts.saturating_add(1);
            }

            tracing::info!(
                thread_id = thread_id.as_str(),
                turn_id = turn_id.as_str(),
                stalled_for_ms = stalled_for.as_millis() as u64,
                next_soft_recovery_attempt = soft_recovery_attempts.saturating_add(1),
                "attempting stalled AI stream recovery via thread snapshot refresh"
            );
            self.send_event(
                event_tx,
                AiWorkerEventPayload::Status(format!(
                    "AI stream stalled for turn {turn_id}. Attempting recovery..."
                )),
            );
            self.load_thread_snapshot(thread_id)?;
            self.emit_snapshot_after_sync(event_tx)?;
            return Ok(());
        }

        if self.transport_kind == AppServerTransportKind::Embedded {
            if let Some(watch) = self.turn_stream_watches.get_mut(turn_id.as_str()) {
                watch.last_recovery_at = Some(now);
                watch.last_meaningful_activity_at = now;
            }

            tracing::warn!(
                thread_id = thread_id.as_str(),
                turn_id = turn_id.as_str(),
                stalled_for_ms = stalled_for.as_millis() as u64,
                soft_recovery_attempts,
                "stall recovery exhausted soft retries; refreshing snapshot without embedded runtime reboot"
            );
            self.send_event(
                event_tx,
                AiWorkerEventPayload::Status(format!(
                    "AI stream is still stalled for turn {turn_id}. Refreshing thread state without restarting the embedded runtime..."
                )),
            );
            self.load_thread_snapshot(thread_id)?;
            self.emit_snapshot_after_sync(event_tx)?;
            return Ok(());
        }

        self.send_event(
            event_tx,
            AiWorkerEventPayload::Status(format!(
                "AI stream is still stalled for turn {turn_id}. Reconnecting transport..."
            )),
        );
        tracing::warn!(
            thread_id = thread_id.as_str(),
            turn_id = turn_id.as_str(),
            stalled_for_ms = stalled_for.as_millis() as u64,
            soft_recovery_attempts,
            "stall recovery exhausted soft retries; reconnecting AI transport"
        );
        self.reconnect_after_transport_failure(config, "recovering stalled AI stream", event_tx)?;
        self.sync_stream_watches_from_state(Instant::now());
        Ok(())
    }

    fn sync_stream_watches_from_state(&mut self, now: Instant) {
        let mut active_watches = BTreeMap::new();
        for turn in self
            .service
            .state()
            .turns
            .values()
            .filter(|turn| turn.status == StateTurnStatus::InProgress)
        {
            let watch = self
                .turn_stream_watches
                .remove(turn.id.as_str())
                .unwrap_or_else(|| TurnStreamWatch {
                    thread_id: turn.thread_id.clone(),
                    turn_id: turn.id.clone(),
                    last_meaningful_activity_at: now,
                    last_recovery_at: None,
                    soft_recovery_attempts: 0,
                });
            active_watches.insert(
                turn.id.clone(),
                TurnStreamWatch {
                    thread_id: turn.thread_id.clone(),
                    turn_id: turn.id.clone(),
                    ..watch
                },
            );
        }
        self.turn_stream_watches = active_watches;
    }

    fn mark_stream_activity_from_notification(
        &mut self,
        notification: &ServerNotification,
        now: Instant,
    ) {
        let Some((thread_id, turn_id)) = notification_turn_identity(notification) else {
            return;
        };
        self.mark_stream_activity(thread_id, turn_id, now);
    }

    fn mark_stream_activity(&mut self, thread_id: &str, turn_id: &str, now: Instant) {
        let watch = self
            .turn_stream_watches
            .entry(turn_id.to_string())
            .or_insert_with(|| TurnStreamWatch {
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                last_meaningful_activity_at: now,
                last_recovery_at: None,
                soft_recovery_attempts: 0,
            });
        watch.thread_id = thread_id.to_string();
        watch.turn_id = turn_id.to_string();
        watch.last_meaningful_activity_at = now;
        watch.last_recovery_at = None;
        watch.soft_recovery_attempts = 0;
    }

    fn turn_is_waiting_for_user_action(&self, thread_id: &str, turn_id: &str) -> bool {
        self.pending_approvals.values().any(|pending| {
            pending.approval.thread_id == thread_id && pending.approval.turn_id == turn_id
        }) || self.pending_user_inputs.values().any(|pending| {
            pending.request.thread_id == thread_id && pending.request.turn_id == turn_id
        })
    }
}

#[derive(Default)]
struct TransportEventDrain {
    notifications: Vec<ServerNotification>,
    server_requests: Vec<ServerRequest>,
    received: usize,
    lagged: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NotificationRefreshFlags {
    refresh_account: bool,
    refresh_rate_limits: bool,
    refresh_skills: bool,
}

fn notification_refresh_flags(notifications: &[ServerNotification]) -> NotificationRefreshFlags {
    let mut flags = NotificationRefreshFlags::default();

    for notification in notifications {
        match notification {
            ServerNotification::AccountUpdated(_) => {
                flags.refresh_account = true;
                flags.refresh_rate_limits = true;
            }
            ServerNotification::AccountLoginCompleted(_) => {
                flags.refresh_account = true;
                flags.refresh_rate_limits = true;
            }
            ServerNotification::SkillsChanged(_) => {
                flags.refresh_skills = true;
            }
            _ => {}
        }
    }

    flags
}

fn notification_turn_identity(notification: &ServerNotification) -> Option<(&str, &str)> {
    match notification {
        ServerNotification::TurnStarted(notification) => {
            Some((notification.thread_id.as_str(), notification.turn.id.as_str()))
        }
        ServerNotification::TurnCompleted(notification) => {
            Some((notification.thread_id.as_str(), notification.turn.id.as_str()))
        }
        ServerNotification::TurnDiffUpdated(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::TurnPlanUpdated(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::ItemStarted(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::ItemCompleted(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::AgentMessageDelta(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::PlanDelta(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::ReasoningSummaryTextDelta(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::ReasoningTextDelta(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::CommandExecutionOutputDelta(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::FileChangeOutputDelta(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        ServerNotification::Error(notification) => {
            Some((notification.thread_id.as_str(), notification.turn_id.as_str()))
        }
        _ => None,
    }
}
