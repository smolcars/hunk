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
                                    this.ai_in_progress_turn_started_at.clear();
                                    this.ai_composer_activity_elapsed_second = None;
                                    this.ai_account = None;
                                    this.ai_requires_openai_auth = false;
                                    this.ai_rate_limits = None;
                                    this.ai_pending_chatgpt_login_id = None;
                                    this.ai_pending_chatgpt_auth_url = None;
                                    this.ai_models.clear();
                                    this.ai_experimental_features.clear();
                                    this.ai_collaboration_modes.clear();
                                    this.ai_bootstrap_loading = false;
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
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            if this.ai_event_epoch != epoch {
                                return;
                            }
                            if this.sync_ai_composer_activity_elapsed_second() {
                                cx.notify();
                            }
                        });
                    } else {
                        return;
                    }
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
                self.apply_ai_snapshot(*snapshot, cx);
                self.ai_connection_state = AiConnectionState::Ready;
                self.ai_error_message = None;
            }
            AiWorkerEvent::BootstrapCompleted => {
                self.ai_bootstrap_loading = false;
            }
            AiWorkerEvent::Reconnecting(message) => {
                self.ai_connection_state = AiConnectionState::Reconnecting;
                self.ai_bootstrap_loading = false;
                self.ai_error_message = None;
                self.ai_status_message = Some(message);
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
                self.ai_in_progress_turn_started_at.clear();
                self.ai_composer_activity_elapsed_second = None;
                self.ai_account = None;
                self.ai_requires_openai_auth = false;
                self.ai_rate_limits = None;
                self.ai_pending_chatgpt_login_id = None;
                self.ai_pending_chatgpt_auth_url = None;
                self.ai_models.clear();
                self.ai_experimental_features.clear();
                self.ai_collaboration_modes.clear();
                self.ai_bootstrap_loading = false;
                Self::push_error_notification(format!("Codex AI failed: {message}"), cx);
            }
        }
    }

    fn apply_ai_snapshot(&mut self, snapshot: AiSnapshot, cx: &mut Context<Self>) {
        let previous_selected_thread = self.ai_selected_thread_id.clone();
        let previous_draft_key = self.current_ai_composer_draft_key();
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        let previous_visible_row_ids = previous_selected_thread
            .as_deref()
            .map(|thread_id| current_ai_renderable_visible_row_ids(self, thread_id))
            .unwrap_or_default();
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
        let changed_row_ids = previous_selected_thread
            .as_deref()
            .map(|thread_id| {
                timeline_row_ids_with_height_changes(&self.ai_state_snapshot, &state, thread_id)
            })
            .unwrap_or_default();

        self.ai_state_snapshot = state;
        self.rebuild_ai_timeline_indexes();
        self.sync_ai_in_progress_turn_started_at();
        self.ai_composer_activity_elapsed_second = self.current_ai_composer_activity_elapsed_second();
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
            self.ai_timeline_follow_output = true;
            self.ai_scroll_timeline_to_bottom = true;
            self.ai_expanded_timeline_row_ids.clear();
            self.ai_text_selection = None;
        }
        if let Some(selected_thread_id) = self.ai_selected_thread_id.as_deref()
            && previous_selected_thread.as_deref() == Some(selected_thread_id)
        {
            let latest_sequence =
                thread_latest_timeline_sequence(&self.ai_state_snapshot, selected_thread_id);
            if should_scroll_timeline_to_bottom_on_new_activity(
                latest_sequence,
                previous_selected_thread_sequence,
                self.ai_timeline_follow_output,
            ) {
                self.ai_scroll_timeline_to_bottom = true;
            }
        }
        self.ai_expanded_timeline_row_ids
            .retain(|row_id| self.ai_timeline_rows_by_id.contains_key(row_id));

        let changed_row_ids = changed_row_ids
            .into_iter()
            .filter_map(|row_id| self.ai_timeline_container_row_id(row_id.as_str()))
            .collect::<BTreeSet<_>>();
        self.ai_clear_text_selection_for_rows(&changed_row_ids, cx);

        let next_visible_row_ids = self
            .ai_selected_thread_id
            .as_deref()
            .map(|thread_id| current_ai_renderable_visible_row_ids(self, thread_id))
            .unwrap_or_default();
        if should_reset_ai_timeline_measurements(
            previous_selected_thread.as_deref(),
            self.ai_selected_thread_id.as_deref(),
            previous_visible_row_ids.as_slice(),
            next_visible_row_ids.as_slice(),
            self.ai_timeline_list_row_count,
        ) {
            reset_ai_timeline_list_measurements(self, next_visible_row_ids.len());
        } else {
            invalidate_ai_timeline_row_measurements(
                self,
                next_visible_row_ids.as_slice(),
                &changed_row_ids,
            );
        }

        self.prune_ai_composer_drafts();
        if previous_draft_key != self.current_ai_composer_draft_key() {
            self.restore_ai_visible_composer_from_current_draft(cx);
        }
        self.sync_ai_session_selection_from_state();
    }

    fn sync_ai_in_progress_turn_started_at(&mut self) {
        let now = Instant::now();
        let mut in_progress_turn_keys = BTreeSet::new();

        for turn in self
            .ai_state_snapshot
            .turns
            .values()
            .filter(|turn| turn.status == TurnStatus::InProgress)
        {
            let key = ai_in_progress_turn_tracking_key(turn.thread_id.as_str(), turn.id.as_str());
            in_progress_turn_keys.insert(key.clone());
            self.ai_in_progress_turn_started_at.entry(key).or_insert(now);
        }

        self.ai_in_progress_turn_started_at
            .retain(|key, _| in_progress_turn_keys.contains(key));
    }

    fn current_ai_composer_draft_key(&self) -> Option<AiComposerDraftKey> {
        let current_thread_id = self.current_ai_thread_id();
        let workspace_key = self.ai_workspace_key();
        ai_composer_draft_key(current_thread_id.as_deref(), workspace_key.as_deref())
    }

    fn current_ai_composer_draft(&self) -> Option<&AiComposerDraft> {
        let key = self.current_ai_composer_draft_key()?;
        self.ai_composer_drafts.get(&key)
    }

    fn current_ai_composer_draft_mut(&mut self) -> Option<&mut AiComposerDraft> {
        let key = self.current_ai_composer_draft_key()?;
        Some(self.ai_composer_drafts.entry(key).or_default())
    }

    pub(crate) fn current_ai_composer_local_images(&self) -> Vec<PathBuf> {
        self.current_ai_composer_draft()
            .map(|draft| draft.local_images.clone())
            .unwrap_or_default()
    }

    fn sync_ai_visible_composer_prompt_to_draft(&mut self, cx: &Context<Self>) {
        let prompt = self.ai_composer_input_state.read(cx).value().to_string();
        if let Some(draft) = self.current_ai_composer_draft_mut() {
            draft.prompt = prompt;
        }
    }

    fn restore_ai_visible_composer_from_current_draft(&mut self, cx: &mut Context<Self>) {
        let prompt = ai_composer_prompt_for_target(
            &self.ai_composer_drafts,
            self.current_ai_composer_draft_key().as_ref(),
        );
        let ai_composer_state = self.ai_composer_input_state.clone();
        let Some(window_handle) = cx.windows().into_iter().next() else {
            return;
        };
        if let Err(error) = cx.update_window(window_handle, move |_, window, cx| {
            ai_composer_state.update(cx, |state, cx| {
                state.set_value(prompt.clone(), window, cx);
            });
        }) {
            error!("failed to restore AI composer input after thread change: {error:#}");
        }
    }

    fn restore_ai_visible_composer_from_current_draft_in_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = ai_composer_prompt_for_target(
            &self.ai_composer_drafts,
            self.current_ai_composer_draft_key().as_ref(),
        );
        self.ai_composer_input_state.update(cx, |state, cx| {
            state.set_value(prompt, window, cx);
        });
    }

    fn prune_ai_composer_drafts(&mut self) {
        let thread_ids = self
            .ai_state_snapshot
            .threads
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.ai_composer_drafts.retain(|key, _| match key {
            AiComposerDraftKey::Thread(thread_id) => thread_ids.contains(thread_id),
            AiComposerDraftKey::Workspace(_) => true,
        });
    }

    fn current_ai_composer_activity_elapsed_second(&self) -> Option<u64> {
        let thread_id = self.current_ai_thread_id()?;
        let turn_id = self.current_ai_in_progress_turn_id(thread_id.as_str())?;
        let tracking_key = format!("{thread_id}::{turn_id}");
        self.ai_in_progress_turn_started_at
            .get(tracking_key.as_str())
            .map(|started_at| started_at.elapsed().as_secs())
    }

    fn sync_ai_composer_activity_elapsed_second(&mut self) -> bool {
        let next = self.current_ai_composer_activity_elapsed_second();
        if self.ai_composer_activity_elapsed_second == next {
            return false;
        }
        self.ai_composer_activity_elapsed_second = next;
        true
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
        AiTurnSessionOverrides {
            model,
            effort,
            collaboration_mode: self.ai_selected_collaboration_mode,
            service_tier: self.ai_selected_service_tier,
        }
    }

    fn send_current_ai_prompt(&mut self, cx: &mut Context<Self>) -> bool {
        let prompt = self.ai_composer_input_state.read(cx).value().trim().to_string();
        let local_image_paths = self.current_ai_composer_local_images();
        if prompt.is_empty() && local_image_paths.is_empty() {
            self.ai_status_message = Some("Prompt cannot be empty.".to_string());
            cx.notify();
            return false;
        }
        if !local_image_paths.is_empty() && !self.current_ai_model_supports_image_inputs() {
            self.ai_status_message = Some(
                "Selected model does not support image attachments. Remove attachments or switch models."
                    .to_string(),
            );
            cx.notify();
            return false;
        }
        let prompt = (!prompt.is_empty()).then_some(prompt);

        let session_overrides = self.current_ai_turn_session_overrides();
        if let Some(thread_id) = self.current_ai_thread_id() {
            let sent = self.send_ai_worker_command(
                AiWorkerCommand::SendPrompt {
                    thread_id,
                    prompt,
                    local_image_paths,
                    session_overrides,
                },
                cx,
            );
            if sent {
                self.ai_status_message = None;
            }
            return sent;
        }

        let sent = self.send_ai_worker_command(
            AiWorkerCommand::StartThread {
                prompt,
                local_image_paths,
                session_overrides,
            },
            cx,
        );
        if sent {
            self.ai_status_message = None;
        }
        sent
    }

    fn sync_ai_session_selection_from_state(&mut self) {
        let persisted = self
            .ai_workspace_key()
            .as_ref()
            .and_then(|workspace| self.state.ai_workspace_session_overrides.get(workspace).cloned())
            .unwrap_or_default();

        self.ai_selected_model = persisted.model.or_else(|| self.default_ai_model_id());
        self.ai_selected_collaboration_mode = persisted.collaboration_mode;
        self.ai_selected_effort = persisted.effort;
        self.ai_selected_service_tier = persisted.service_tier.unwrap_or_default();
        self.normalize_ai_selected_effort();
    }

    fn persist_current_ai_workspace_session(&mut self) {
        let Some(workspace) = self.ai_workspace_key() else {
            return;
        };

        let session = AiThreadSessionState {
            model: self.ai_selected_model.clone(),
            effort: self.ai_selected_effort.clone(),
            collaboration_mode: self.ai_selected_collaboration_mode,
            service_tier: normalized_ai_service_tier_selection(self.ai_selected_service_tier),
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

    fn clear_ai_composer_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self.current_ai_composer_draft_mut() {
            draft.prompt.clear();
            draft.local_images.clear();
        }
        self.ai_composer_input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
    }

    fn focus_ai_composer_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai_composer_input_state.update(cx, |state, cx| {
            state.focus(window, cx);
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

    pub(crate) fn current_ai_model_supports_image_inputs(&self) -> bool {
        self.current_ai_model_for_input_modalities()
            .is_none_or(|model| {
                model
                    .input_modalities
                    .contains(&codex_protocol::openai_models::InputModality::Image)
            })
    }

    fn current_ai_model_for_input_modalities(&self) -> Option<&codex_app_server_protocol::Model> {
        self.ai_selected_model
            .as_deref()
            .and_then(|model_id| self.ai_model_by_id(model_id))
            .or_else(|| self.ai_models.iter().find(|model| model.is_default))
            .or_else(|| self.ai_models.first())
    }
}
