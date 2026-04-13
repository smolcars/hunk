impl DiffViewer {
    fn start_ai_event_listener(
        &mut self,
        event_rx: std::sync::mpsc::Receiver<AiWorkerEvent>,
        workspace_key: String,
        generation: usize,
        cx: &mut Context<Self>,
    ) {
        let event_rx = event_rx;
        self.ai_event_task = cx.spawn(async move |this, cx| {
            let mut last_idle_foreground_update_at =
                std::time::Instant::now() - Self::AI_EVENT_IDLE_FOREGROUND_INTERVAL;
            loop {
                let (buffered_events, event_stream_disconnected) =
                    drain_ai_worker_events(&event_rx);

                if buffered_events.is_empty() && !event_stream_disconnected {
                    if last_idle_foreground_update_at.elapsed()
                        >= Self::AI_EVENT_IDLE_FOREGROUND_INTERVAL
                    {
                        if let Some(this) = this.upgrade() {
                            let mut listener_is_current = true;
                            this.update(cx, |this, cx| {
                                if !this.ai_runtime_listener_is_current(
                                    workspace_key.as_str(),
                                    generation,
                                ) {
                                    listener_is_current = false;
                                    return;
                                }
                                if this.ai_worker_workspace_key.as_deref()
                                    != Some(workspace_key.as_str())
                                {
                                    return;
                                }
                                let activity_elapsed_second_changed =
                                    this.sync_ai_composer_activity_elapsed_second();
                                if activity_elapsed_second_changed {
                                    this.maybe_refresh_selected_thread_metadata(cx);
                                    cx.notify();
                                }
                            });
                            if !listener_is_current {
                                return;
                            }
                        } else {
                            return;
                        }
                        last_idle_foreground_update_at = std::time::Instant::now();
                    }
                    cx.background_executor()
                        .timer(Self::AI_EVENT_POLL_INTERVAL)
                        .await;
                    continue;
                }

                if let Some(this) = this.upgrade() {
                    let mut listener_is_current = true;
                    let mut should_stop = false;
                    this.update(cx, |this, cx| {
                        if !this
                            .ai_runtime_listener_is_current(workspace_key.as_str(), generation)
                        {
                            listener_is_current = false;
                            return;
                        }
                        let is_visible_runtime = this.ai_runtime_is_visible(workspace_key.as_str());

                        if is_visible_runtime {
                            for event in buffered_events {
                                if event.workspace_key.as_str() != workspace_key.as_str() {
                                    continue;
                                }
                                this.apply_ai_worker_event(event.payload, cx);
                            }
                            if event_stream_disconnected {
                                this.handle_ai_worker_event_stream_disconnect(cx);
                                should_stop = true;
                            }
                            cx.notify();
                            return;
                        }

                        let mut terminate_hidden_runtime = false;
                        for event in buffered_events {
                            if event.workspace_key.as_str() != workspace_key.as_str() {
                                continue;
                            }
                            terminate_hidden_runtime |=
                                matches!(&event.payload, AiWorkerEventPayload::Fatal(_));
                            this.handle_background_ai_worker_event(
                                workspace_key.as_str(),
                                event.payload,
                                cx,
                            );
                        }
                        if terminate_hidden_runtime || event_stream_disconnected {
                            this.handle_background_ai_worker_disconnect(workspace_key.as_str());
                            should_stop = true;
                        }
                    });
                    last_idle_foreground_update_at = std::time::Instant::now();
                    if !listener_is_current || should_stop {
                        return;
                    }
                } else {
                    return;
                }

                if event_stream_disconnected {
                    return;
                }
            }
        });
    }

    fn ai_runtime_is_visible(&self, workspace_key: &str) -> bool {
        self.ai_worker_workspace_key.as_deref() == Some(workspace_key)
    }

    fn reset_visible_ai_runtime_after_failure(
        &mut self,
        join_reason: &'static str,
        cx: &mut Context<Self>,
    ) {
        let restored_pending_steer_drafts = self.restore_all_visible_ai_pending_steers_to_drafts();
        let restored_queued_message_drafts = self.restore_all_visible_ai_queued_messages_to_drafts();
        let visible_workspace_key = self.ai_worker_workspace_key.clone();
        self.ai_command_tx = None;
        self.clear_ai_runtime_start_in_flight_for_workspace(visible_workspace_key.as_deref());
        self.clear_ai_desktop_notification_state(visible_workspace_key.as_deref());
        self.ai_worker_workspace_key = None;
        self.join_ai_worker_thread(join_reason);
        self.ai_thread_title_refresh_state_by_thread.clear();
        self.ai_pending_approvals.clear();
        self.ai_pending_user_inputs.clear();
        self.ai_pending_user_input_answers.clear();
        self.ai_in_progress_turn_started_at.clear();
        self.ai_composer_activity_elapsed_second = None;
        self.restore_ai_new_thread_draft_after_failure(cx);
        self.ai_account = None;
        self.ai_requires_openai_auth = false;
        self.ai_rate_limits = None;
        self.ai_pending_chatgpt_login_id = None;
        self.ai_pending_chatgpt_auth_url = None;
        self.ai_models.clear();
        self.ai_experimental_features.clear();
        self.ai_collaboration_modes.clear();
        if !self.ai_skills.is_empty() {
            self.ai_skills_generation = self.ai_skills_generation.saturating_add(1);
            self.ai_composer_completion_sync_key = None;
        }
        self.ai_skills.clear();
        self.ai_composer_skill_completion_menu = None;
        self.ai_composer_skill_completion_selected_ix = 0;
        self.ai_composer_skill_completion_dismissed_token = None;
        self.ai_bootstrap_loading = false;
        self.ai_connection_state = AiConnectionState::Failed;
        self.invalidate_ai_visible_frame_state_with_reason("event");
        if self
            .current_ai_composer_draft_key()
            .as_ref()
            .is_some_and(|key| {
                restored_pending_steer_drafts.contains(key)
                    || restored_queued_message_drafts.contains(key)
            })
        {
            self.restore_ai_visible_composer_from_current_draft(cx);
        }
    }

    fn handle_ai_worker_event_stream_disconnect(&mut self, cx: &mut Context<Self>) {
        self.reset_visible_ai_runtime_after_failure("event stream disconnect", cx);
        if self.ai_error_message.is_none() {
            let message = "Codex worker disconnected.".to_string();
            self.ai_error_message = Some(message.clone());
            self.ai_status_message = Some("Codex integration failed".to_string());
            Self::push_error_notification(format!("Codex AI failed: {message}"), cx);
        }
    }

    fn apply_ai_worker_event(&mut self, event: AiWorkerEventPayload, cx: &mut Context<Self>) {
        match event {
            AiWorkerEventPayload::Snapshot(snapshot) => {
                let previous_auth_message = ai_auth_required_message(
                    self.ai_account.as_ref(),
                    self.ai_requires_openai_auth,
                    self.ai_pending_chatgpt_login_id.as_deref(),
                );
                self.apply_ai_snapshot(*snapshot, cx);
                self.ai_connection_state = AiConnectionState::Ready;
                let next_auth_message = ai_auth_required_message(
                    self.ai_account.as_ref(),
                    self.ai_requires_openai_auth,
                    self.ai_pending_chatgpt_login_id.as_deref(),
                );
                self.ai_error_message = next_auth_message.clone();
                if previous_auth_message != next_auth_message
                    && let Some(message) = next_auth_message
                {
                    Self::push_error_notification(message, cx);
                }
                let visible_workspace_key = self.ai_worker_workspace_key.clone();
                self.clear_ai_runtime_start_in_flight_for_workspace(visible_workspace_key.as_deref());
            }
            AiWorkerEventPayload::BootstrapCompleted => {
                self.ai_bootstrap_loading = false;
                let visible_workspace_key = self.ai_worker_workspace_key.clone();
                self.clear_ai_runtime_start_in_flight_for_workspace(visible_workspace_key.as_deref());
            }
            AiWorkerEventPayload::ThreadStarted { thread_id } => {
                set_pending_thread_start_thread_id(&mut self.ai_pending_thread_start, thread_id);
            }
            AiWorkerEventPayload::SteerAccepted(pending) => {
                let pending_thread_id = pending.thread_id.clone();
                self.ai_pending_steers.push(pending);
                if self.current_ai_thread_id().as_deref() == Some(pending_thread_id.as_str()) {
                    self.ai_timeline_follow_output = true;
                    self.ai_scroll_timeline_to_bottom = true;
                }
            }
            AiWorkerEventPayload::Reconnecting(message) => {
                self.ai_connection_state = AiConnectionState::Reconnecting;
                self.ai_bootstrap_loading = false;
                self.ai_error_message = None;
                self.ai_status_message = Some(message);
            }
            AiWorkerEventPayload::Status(message) => {
                self.ai_status_message = Some(message);
                if let Some(error_message) = self
                    .ai_status_message
                    .as_deref()
                    .and_then(ai_prominent_worker_status_error)
                {
                    let should_notify =
                        self.ai_error_message.as_deref() != Some(error_message.as_str());
                    self.ai_error_message = Some(error_message.clone());
                    if should_notify {
                        Self::push_error_notification(error_message, cx);
                    }
                }
            }
            AiWorkerEventPayload::Error(message) => {
                self.restore_ai_new_thread_draft_after_failure(cx);
                self.ai_error_message = Some(message.clone());
                self.ai_status_message = Some(message);
            }
            AiWorkerEventPayload::Fatal(message) => {
                self.reset_visible_ai_runtime_after_failure("fatal worker event", cx);
                self.ai_error_message = Some(message.clone());
                self.ai_status_message = Some("Codex integration failed".to_string());
                Self::push_error_notification(format!("Codex AI failed: {message}"), cx);
            }
        }
        self.invalidate_ai_visible_frame_state_with_reason("event");
    }
}
