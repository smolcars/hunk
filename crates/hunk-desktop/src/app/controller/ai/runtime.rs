impl DiffViewer {
    fn start_ai_event_listener(
        &mut self,
        event_rx: std::sync::mpsc::Receiver<AiWorkerEvent>,
        epoch: usize,
        cx: &mut Context<Self>,
    ) {
        let event_rx = event_rx;
        self.ai_event_task = cx.spawn(async move |this, cx| {
            loop {
                let mut buffered_events = Vec::new();
                loop {
                    match event_rx.try_recv() {
                        Ok(event) => {
                            buffered_events.push(event);
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            if let Some(this) = this.upgrade() {
                                this.update(cx, |this, cx| {
                                    if this.ai_event_epoch != epoch {
                                        return;
                                    }
                                    this.ai_command_tx = None;
                                    this.join_ai_worker_thread("event stream disconnect");
                                    this.ai_pending_approvals.clear();
                                    this.ai_pending_user_inputs.clear();
                                    this.ai_pending_user_input_answers.clear();
                                    this.ai_account = None;
                                    this.ai_requires_openai_auth = false;
                                    this.ai_rate_limits = None;
                                    this.ai_pending_chatgpt_login_id = None;
                                    this.ai_pending_chatgpt_auth_url = None;
                                    this.ai_models.clear();
                                    this.ai_experimental_features.clear();
                                    this.ai_collaboration_modes.clear();
                                    if this.ai_error_message.is_none() {
                                        this.ai_connection_state = AiConnectionState::Disconnected;
                                        this.ai_status_message = Some(
                                            "Codex worker disconnected.".to_string(),
                                        );
                                    } else {
                                        this.ai_connection_state = AiConnectionState::Failed;
                                    }
                                    cx.notify();
                                });
                            }
                            return;
                        }
                    }
                }

                if buffered_events.is_empty() {
                    cx.background_executor()
                        .timer(Self::AI_EVENT_POLL_INTERVAL)
                        .await;
                    continue;
                }

                if let Some(this) = this.upgrade() {
                    this.update(cx, |this, cx| {
                        if this.ai_event_epoch != epoch {
                            return;
                        }
                        for event in buffered_events {
                            this.apply_ai_worker_event(event, cx);
                        }
                        cx.notify();
                    });
                } else {
                    return;
                }
            }
        });
    }

    fn apply_ai_worker_event(&mut self, event: AiWorkerEvent, cx: &mut Context<Self>) {
        match event {
            AiWorkerEvent::Snapshot(snapshot) => {
                self.apply_ai_snapshot(*snapshot);
                self.ai_connection_state = AiConnectionState::Ready;
                self.ai_error_message = None;
            }
            AiWorkerEvent::Status(message) => {
                self.ai_status_message = Some(message);
            }
            AiWorkerEvent::Error(message) => {
                self.ai_error_message = Some(message.clone());
                self.ai_status_message = Some(message);
            }
            AiWorkerEvent::Fatal(message) => {
                self.ai_connection_state = AiConnectionState::Failed;
                self.ai_error_message = Some(message.clone());
                self.ai_status_message = Some("Codex integration failed".to_string());
                self.ai_command_tx = None;
                self.join_ai_worker_thread("fatal worker event");
                self.ai_pending_approvals.clear();
                self.ai_pending_user_inputs.clear();
                self.ai_pending_user_input_answers.clear();
                self.ai_account = None;
                self.ai_requires_openai_auth = false;
                self.ai_rate_limits = None;
                self.ai_pending_chatgpt_login_id = None;
                self.ai_pending_chatgpt_auth_url = None;
                self.ai_models.clear();
                self.ai_experimental_features.clear();
                self.ai_collaboration_modes.clear();
                Self::push_error_notification(format!("Codex AI failed: {message}"), cx);
            }
        }
    }

    fn apply_ai_snapshot(&mut self, snapshot: AiSnapshot) {
        let previous_selected_thread = self.ai_selected_thread_id.clone();
        let previous_selected_thread_sequence =
            previous_selected_thread
                .as_deref()
                .map(|thread_id| thread_latest_timeline_sequence(&self.ai_state_snapshot, thread_id))
                .unwrap_or(0);
        let previous_active_thread_for_workspace = self
            .ai_workspace_key()
            .as_deref()
            .and_then(|workspace| self.ai_state_snapshot.active_thread_for_cwd(workspace))
            .map(ToOwned::to_owned);
        let AiSnapshot {
            state,
            active_thread_id,
            pending_approvals,
            pending_user_inputs,
            account,
            requires_openai_auth,
            pending_chatgpt_login_id,
            pending_chatgpt_auth_url,
            rate_limits,
            models,
            experimental_features,
            collaboration_modes,
            include_hidden_models,
            mad_max_mode,
        } = snapshot;

        self.ai_state_snapshot = state;
        self.ai_pending_approvals = pending_approvals;
        self.ai_pending_user_inputs = pending_user_inputs;
        self.sync_ai_pending_user_input_answers();
        self.ai_account = account;
        self.ai_requires_openai_auth = requires_openai_auth;
        self.ai_pending_chatgpt_login_id = pending_chatgpt_login_id;
        self.ai_pending_chatgpt_auth_url = pending_chatgpt_auth_url;
        self.ai_rate_limits = rate_limits;
        self.ai_models = models;
        self.ai_experimental_features = experimental_features;
        self.ai_collaboration_modes = collaboration_modes;
        self.ai_include_hidden_models = include_hidden_models;
        self.ai_mad_max_mode = mad_max_mode;
        self.ai_timeline_visible_turn_limit_by_thread
            .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));

        if should_sync_selected_thread_from_active_thread(
            self.ai_selected_thread_id.as_deref(),
            active_thread_id.as_deref(),
            previous_active_thread_for_workspace.as_deref(),
            &self.ai_state_snapshot,
        ) {
            self.ai_selected_thread_id = active_thread_id;
        }

        if self.ai_selected_thread_id.as_ref().is_some_and(|selected| {
            self.ai_state_snapshot
                .threads
                .get(selected)
                .is_none_or(|thread| thread.status == ThreadLifecycleStatus::Archived)
        }) {
            self.ai_selected_thread_id = None;
        }

        if self.ai_selected_thread_id.is_none() {
            self.ai_selected_thread_id = self.current_ai_thread_id();
        }

        if self.ai_selected_thread_id.is_none()
            && let Some(first_thread) = self.ai_visible_threads().first()
        {
            self.ai_selected_thread_id = Some(first_thread.id.clone());
        }
        if should_scroll_timeline_to_bottom_on_selection_change(
            previous_selected_thread.as_deref(),
            self.ai_selected_thread_id.as_deref(),
        ) {
            self.ai_scroll_timeline_to_bottom = true;
            self.ai_expanded_command_output_item_ids.clear();
        }
        if let Some(selected_thread_id) = self.ai_selected_thread_id.as_deref()
            && previous_selected_thread.as_deref() == Some(selected_thread_id)
        {
            let latest_sequence =
                thread_latest_timeline_sequence(&self.ai_state_snapshot, selected_thread_id);
            if latest_sequence > previous_selected_thread_sequence
                && self.ai_timeline_is_near_bottom_for_thread(selected_thread_id)
            {
                self.ai_scroll_timeline_to_bottom = true;
            }
        }
        self.ai_expanded_command_output_item_ids
            .retain(|item_id| self.ai_state_snapshot.items.contains_key(item_id));

        self.sync_ai_session_selection_from_state();
    }

    fn sync_ai_pending_user_input_answers(&mut self) {
        let existing_answers = std::mem::take(&mut self.ai_pending_user_input_answers);
        let mut next_answers = BTreeMap::new();

        for request in &self.ai_pending_user_inputs {
            let normalized = normalized_user_input_answers(
                request,
                existing_answers.get(request.request_id.as_str()),
            );
            next_answers.insert(request.request_id.clone(), normalized);
        }

        self.ai_pending_user_input_answers = next_answers;
    }

    fn current_ai_turn_session_overrides(&self) -> AiTurnSessionOverrides {
        let model = self
            .ai_selected_model
            .clone()
            .filter(|model_id| self.ai_model_by_id(model_id.as_str()).is_some());
        let effort = model.as_ref().and_then(|model_id| {
            self.ai_selected_effort
                .clone()
                .filter(|effort| self.model_supports_effort(model_id.as_str(), effort.as_str()))
        });
        let collaboration_mode = self
            .ai_selected_collaboration_mode
            .clone()
            .filter(|mode_name| {
                self.ai_collaboration_modes
                    .iter()
                    .any(|mask| mask.name == *mode_name)
            });
        AiTurnSessionOverrides {
            model,
            effort,
            collaboration_mode,
        }
    }

    fn send_current_ai_prompt(&mut self, cx: &mut Context<Self>) -> bool {
        let prompt = self.ai_composer_input_state.read(cx).value().trim().to_string();
        if prompt.is_empty() {
            self.ai_status_message = Some("Prompt cannot be empty.".to_string());
            cx.notify();
            return false;
        }

        let session_overrides = self.current_ai_turn_session_overrides();
        if let Some(thread_id) = self.current_ai_thread_id() {
            return self.send_ai_worker_command(
                AiWorkerCommand::SendPrompt {
                    thread_id,
                    prompt,
                    session_overrides,
                },
                cx,
            );
        }

        self.send_ai_worker_command(
            AiWorkerCommand::StartThread {
                prompt: Some(prompt),
                session_overrides,
            },
            cx,
        )
    }

    fn sync_ai_session_selection_from_state(&mut self) {
        let persisted = self
            .ai_workspace_key()
            .as_ref()
            .and_then(|workspace| self.state.ai_workspace_session_overrides.get(workspace).cloned())
            .unwrap_or_default();

        self.ai_selected_model = persisted.model.or_else(|| self.default_ai_model_id());
        self.ai_selected_collaboration_mode = persisted.collaboration_mode.filter(|mode_name| {
            self.ai_collaboration_modes
                .iter()
                .any(|mask| mask.name == *mode_name)
        });
        self.ai_selected_effort = persisted.effort;
        self.normalize_ai_selected_effort();
    }

    fn persist_current_ai_workspace_session(&mut self) {
        let Some(workspace) = self.ai_workspace_key() else {
            return;
        };

        let session = AiThreadSessionState {
            model: self.ai_selected_model.clone(),
            effort: self.ai_selected_effort.clone(),
            collaboration_mode: self.ai_selected_collaboration_mode.clone(),
        };

        if let Some(session) = normalized_thread_session_state(session) {
            self.state
                .ai_workspace_session_overrides
                .insert(workspace, session);
        } else {
            self.state
                .ai_workspace_session_overrides
                .remove(workspace.as_str());
        }
        self.persist_state();
    }

    fn clear_ai_composer_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai_composer_input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
    }

    fn normalize_ai_selected_effort(&mut self) {
        let Some(model_id) = self.ai_selected_model.as_ref() else {
            self.ai_selected_effort = None;
            return;
        };
        let Some(model) = self.ai_model_by_id(model_id.as_str()) else {
            self.ai_selected_effort = None;
            return;
        };

        if let Some(effort) = self.ai_selected_effort.as_ref()
            && model
                .supported_reasoning_efforts
                .iter()
                .any(|option| reasoning_effort_key(&option.reasoning_effort) == *effort)
        {
            return;
        }
        self.ai_selected_effort = Some(reasoning_effort_key(&model.default_reasoning_effort));
    }

    fn default_ai_model_id(&self) -> Option<String> {
        self.ai_models
            .iter()
            .find(|model| model.is_default)
            .or_else(|| self.ai_models.first())
            .map(|model| model.id.clone())
    }

    fn ai_model_by_id(&self, model_id: &str) -> Option<&codex_app_server_protocol::Model> {
        self.ai_models.iter().find(|model| model.id == model_id)
    }

    fn model_supports_effort(&self, model_id: &str, effort_key: &str) -> bool {
        self.ai_model_by_id(model_id).is_some_and(|model| {
            model
                .supported_reasoning_efforts
                .iter()
                .any(|option| reasoning_effort_key(&option.reasoning_effort) == effort_key)
        })
    }
}
