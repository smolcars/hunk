#[derive(Debug)]
struct AiPreparedThreadWorkspace {
    branch_name: String,
    workspace_target_id: Option<String>,
    status_message: String,
}

impl DiffViewer {
    fn begin_ai_view_activation_trace(&mut self) {
        self.ai_view_activation_started_at = Some(Instant::now());
        let workspace_key = self
            .ai_workspace_key()
            .unwrap_or_else(|| "<none>".to_string());
        let hidden_runtime_available = self
            .ai_hidden_runtimes
            .contains_key(workspace_key.as_str());
        tracing::info!(
            workspace_key = workspace_key.as_str(),
            connection_state = ?self.ai_connection_state,
            bootstrap_loading = self.ai_bootstrap_loading,
            hidden_runtime_available,
            selected_thread_id = ?self.current_ai_thread_id(),
            "ai instrumentation: AI view activation started"
        );
    }

    fn complete_ai_view_activation_trace(&mut self, outcome: &'static str) {
        let Some(started_at) = self.ai_view_activation_started_at.take() else {
            return;
        };
        let workspace_key = self
            .ai_workspace_key()
            .unwrap_or_else(|| "<none>".to_string());
        let thread_count = self.ai_visible_threads().len();
        tracing::info!(
            workspace_key = workspace_key.as_str(),
            outcome,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            connection_state = ?self.ai_connection_state,
            bootstrap_loading = self.ai_bootstrap_loading,
            thread_count,
            selected_thread_id = ?self.current_ai_thread_id(),
            "ai instrumentation: AI view activation ready"
        );
    }

    fn fail_ai_view_activation_trace(
        &mut self,
        outcome: &'static str,
        message: Option<&str>,
    ) {
        let Some(started_at) = self.ai_view_activation_started_at.take() else {
            return;
        };
        let workspace_key = self
            .ai_workspace_key()
            .unwrap_or_else(|| "<none>".to_string());
        tracing::warn!(
            workspace_key = workspace_key.as_str(),
            outcome,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            connection_state = ?self.ai_connection_state,
            bootstrap_loading = self.ai_bootstrap_loading,
            message = message.unwrap_or(""),
            "ai instrumentation: AI view activation failed"
        );
    }

    fn start_ai_event_listener(
        &mut self,
        event_rx: std::sync::mpsc::Receiver<AiWorkerEvent>,
        workspace_key: String,
        generation: usize,
        cx: &mut Context<Self>,
    ) {
        let event_rx = event_rx;
        self.ai_event_task = cx.spawn(async move |this, cx| {
            loop {
                let (buffered_events, event_stream_disconnected) =
                    drain_ai_worker_events(&event_rx);

                if buffered_events.is_empty() && !event_stream_disconnected {
                    if let Some(this) = this.upgrade() {
                        let mut listener_is_current = true;
                        this.update(cx, |this, cx| {
                            if !this
                                .ai_runtime_listener_is_current(workspace_key.as_str(), generation)
                            {
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
                            );
                        }
                        if terminate_hidden_runtime || event_stream_disconnected {
                            this.handle_background_ai_worker_disconnect(workspace_key.as_str());
                            should_stop = true;
                        }
                    });
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

    fn handle_ai_worker_event_stream_disconnect(&mut self, cx: &mut Context<Self>) {
        self.ai_command_tx = None;
        self.ai_worker_workspace_key = None;
        self.join_ai_worker_thread("event stream disconnect");
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
        self.ai_bootstrap_loading = false;
        self.ai_connection_state = AiConnectionState::Failed;
        if self.ai_error_message.is_none() {
            let message = "Codex worker disconnected.".to_string();
            self.ai_error_message = Some(message.clone());
            self.ai_status_message = Some("Codex integration failed".to_string());
            Self::push_error_notification(format!("Codex AI failed: {message}"), cx);
        }
        let error_message = self.ai_error_message.clone();
        self.fail_ai_view_activation_trace("event_stream_disconnect", error_message.as_deref());
    }

    fn apply_ai_worker_event(&mut self, event: AiWorkerEventPayload, cx: &mut Context<Self>) {
        match event {
            AiWorkerEventPayload::Snapshot(snapshot) => {
                self.apply_ai_snapshot(*snapshot, cx);
                self.ai_connection_state = AiConnectionState::Ready;
                self.ai_error_message = None;
            }
            AiWorkerEventPayload::BootstrapCompleted => {
                self.ai_bootstrap_loading = false;
                self.complete_ai_view_activation_trace("bootstrap_completed");
            }
            AiWorkerEventPayload::ThreadStarted { thread_id } => {
                set_pending_thread_start_thread_id(&mut self.ai_pending_thread_start, thread_id);
            }
            AiWorkerEventPayload::Reconnecting(message) => {
                self.ai_connection_state = AiConnectionState::Reconnecting;
                self.ai_bootstrap_loading = false;
                self.ai_error_message = None;
                self.ai_status_message = Some(message);
            }
            AiWorkerEventPayload::Status(message) => {
                self.ai_status_message = Some(message);
            }
            AiWorkerEventPayload::Error(message) => {
                self.restore_ai_new_thread_draft_after_failure(cx);
                self.ai_error_message = Some(message.clone());
                self.ai_status_message = Some(message);
                let error_message = self.ai_error_message.clone();
                self.fail_ai_view_activation_trace("worker_error", error_message.as_deref());
            }
            AiWorkerEventPayload::Fatal(message) => {
                self.ai_connection_state = AiConnectionState::Failed;
                self.ai_error_message = Some(message.clone());
                self.ai_status_message = Some("Codex integration failed".to_string());
                self.ai_command_tx = None;
                self.ai_worker_workspace_key = None;
                self.join_ai_worker_thread("fatal worker event");
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
                self.ai_bootstrap_loading = false;
                self.fail_ai_view_activation_trace("worker_fatal", Some(message.as_str()));
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
        self.ai_thread_title_refresh_state_by_thread
            .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
        self.ai_timeline_visible_turn_limit_by_thread
            .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
        self.prune_ai_composer_statuses();

        if let Some(thread_id) = pending_new_thread_selection_ready_thread_id(
            self.ai_pending_new_thread_selection,
            self.ai_pending_thread_start.as_ref(),
            active_thread_id.as_deref(),
            &self.ai_state_snapshot,
        ) {
            self.ai_new_thread_draft_active = false;
            self.ai_pending_new_thread_selection = false;
            self.ai_selected_thread_id = Some(thread_id);
        }

        if should_sync_selected_thread_from_active_thread(
            self.ai_selected_thread_id.as_deref(),
            active_thread_id.as_deref(),
            self.ai_new_thread_draft_active || self.ai_pending_new_thread_selection,
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

        if !self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_selected_thread_id.is_none()
        {
            self.ai_selected_thread_id = self.current_ai_thread_id();
        }

        if !self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_selected_thread_id.is_none()
            && let Some(first_thread) = self.ai_threads_for_current_workspace().first()
        {
            self.ai_selected_thread_id = Some(first_thread.id.clone());
        }
        if self.ai_pending_thread_start.as_ref().is_some_and(|pending| {
            pending.thread_id.as_ref().is_some_and(|thread_id| {
                ai_state_has_user_message_for_thread(&self.ai_state_snapshot, thread_id)
            })
        }) {
            self.ai_pending_thread_start = None;
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
        self.maybe_refresh_selected_thread_metadata(cx);
        self.sync_ai_session_selection_from_state();
    }

    fn maybe_refresh_selected_thread_metadata(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            return;
        };
        let now = Instant::now();

        let Some((refresh_key, attempts)) = next_thread_metadata_refresh_attempt(
            &mut self.ai_thread_title_refresh_state_by_thread,
            &self.ai_state_snapshot,
            thread_id.as_str(),
            now,
        ) else {
            return;
        };

        if self.send_ai_worker_command_if_running(
            AiWorkerCommand::RefreshThreadMetadata {
                thread_id: thread_id.clone(),
            },
            cx,
        ) {
            tracing::debug!(
                thread_id = thread_id.as_str(),
                attempts,
                refresh_key = refresh_key.as_str(),
                "Polling AI thread metadata for title refresh"
            );
            self.ai_thread_title_refresh_state_by_thread.insert(
                thread_id,
                AiThreadTitleRefreshState {
                    key: refresh_key,
                    attempts,
                    in_flight: true,
                    last_attempt_at: now,
                },
            );
        }
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

    fn workspace_ai_composer_draft_key(&self) -> Option<AiComposerDraftKey> {
        let workspace_key = self.ai_workspace_key_for_draft();
        ai_composer_draft_key(None, workspace_key.as_deref())
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

    fn composer_status_message_for_target(
        &self,
        target_key: Option<&AiComposerDraftKey>,
    ) -> Option<&str> {
        target_key.and_then(|key| {
            self.ai_composer_status_by_draft
                .get(key)
                .map(String::as_str)
        })
    }

    pub(crate) fn current_ai_composer_status_message(&self) -> Option<&str> {
        self.composer_status_message_for_target(self.current_ai_composer_draft_key().as_ref())
            .or(self.ai_status_message.as_deref())
    }

    fn set_ai_composer_status_for_target(
        &mut self,
        target_key: Option<AiComposerDraftKey>,
        message: impl Into<String>,
    ) {
        let message = message.into();
        if let Some(key) = target_key {
            self.ai_composer_status_by_draft.insert(key, message);
        } else {
            self.ai_status_message = Some(message);
        }
    }

    fn set_current_ai_composer_status(&mut self, message: impl Into<String>) {
        let target_key = self.current_ai_composer_draft_key();
        self.set_ai_composer_status_for_target(target_key, message);
    }

    fn clear_ai_composer_status_for_target(&mut self, target_key: Option<&AiComposerDraftKey>) {
        if let Some(key) = target_key {
            self.ai_composer_status_by_draft.remove(key);
        } else {
            self.ai_status_message = None;
        }
    }

    fn clear_current_ai_composer_status(&mut self) {
        let target_key = self.current_ai_composer_draft_key();
        self.clear_ai_composer_status_for_target(target_key.as_ref());
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
        if let Err(error) = Self::update_any_window(cx, move |window, cx| {
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

    fn prune_ai_composer_statuses(&mut self) {
        let thread_ids = self
            .ai_state_snapshot
            .threads
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.ai_composer_status_by_draft.retain(|key, _| match key {
            AiComposerDraftKey::Thread(thread_id) => thread_ids.contains(thread_id),
            AiComposerDraftKey::Workspace(_) => true,
        });
    }

    fn restore_ai_new_thread_draft_after_failure(&mut self, cx: &mut Context<Self>) {
        if self.ai_pending_new_thread_selection {
            self.ai_new_thread_draft_active = true;
        }
        self.ai_pending_new_thread_selection = false;
        let Some(pending) = self.ai_pending_thread_start.take() else {
            return;
        };
        let current_workspace_key = self.ai_workspace_key_for_draft();
        if current_workspace_key.as_deref() != Some(pending.workspace_key.as_str()) {
            self.ai_pending_thread_start = Some(pending);
            return;
        }
        let target_key = self.workspace_ai_composer_draft_key();
        if let Some(target_key) = target_key {
            let draft = self.ai_composer_drafts.entry(target_key).or_default();
            draft.prompt = pending.prompt;
            draft.local_images = pending.local_images;
        }
        self.restore_ai_visible_composer_from_current_draft(cx);
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
            self.set_current_ai_composer_status("Prompt cannot be empty.");
            cx.notify();
            return false;
        }
        if !local_image_paths.is_empty() && !self.current_ai_model_supports_image_inputs() {
            self.set_current_ai_composer_status(
                "Selected model does not support image attachments. Remove attachments or switch models.",
            );
            cx.notify();
            return false;
        }
        if self.ai_command_tx.is_none() {
            self.ensure_ai_runtime_started(cx);
        }
        if self.ai_command_tx.is_none()
            || ai_prompt_send_waiting_on_connection(
                self.ai_connection_state,
                self.ai_bootstrap_loading,
            )
        {
            self.set_current_ai_composer_status(
                "Cannot send until Codex finishes connecting.",
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
                self.clear_current_ai_composer_status();
            }
            return sent;
        }

        let pending_thread_start = self.ai_workspace_key_for_draft().map(|workspace_key| {
            AiPendingThreadStart {
                workspace_key,
                prompt: prompt.clone().unwrap_or_default(),
                local_images: local_image_paths.clone(),
                started_at: Instant::now(),
                start_mode: self.ai_new_thread_start_mode,
                thread_id: None,
            }
        });
        let started = self.prepare_workspace_and_start_ai_thread(
            prompt,
            local_image_paths,
            session_overrides,
            cx,
        );
        if started {
            self.ai_pending_thread_start = pending_thread_start;
            cx.notify();
        }
        started
    }

    fn prepare_workspace_and_start_ai_thread(
        &mut self,
        prompt: Option<String>,
        local_image_paths: Vec<PathBuf>,
        session_overrides: AiTurnSessionOverrides,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.git_controls_busy() {
            self.set_current_ai_composer_status("Wait for the active workspace action to finish.");
            cx.notify();
            return false;
        }

        let Some(repo_root) = self.ai_draft_workspace_root() else {
            self.set_current_ai_composer_status("Open a workspace before starting an AI thread.");
            cx.notify();
            return false;
        };

        let start_mode = self.ai_new_thread_start_mode;
        let selected_base_branch_name = self.ai_selected_worktree_base_branch_name().map(str::to_string);
        let prompt_seed = prompt.clone().unwrap_or_default();
        let fallback_branch_name =
            ai_branch_name_for_prompt(prompt_seed.as_str(), start_mode == AiNewThreadStartMode::Worktree);
        let codex_executable = Self::resolve_codex_executable_path();
        let epoch = self.begin_git_action("Prepare AI thread", cx);
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let prompt_for_start = prompt.clone();
            let image_paths_for_start = local_image_paths.clone();
            let session_overrides_for_start = session_overrides.clone();
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let requested_branch_name = requested_branch_name_for_new_thread(
                        start_mode,
                        fallback_branch_name,
                        || try_ai_branch_name_for_prompt(
                            codex_executable.as_path(),
                            repo_root.as_path(),
                            prompt_seed.as_str(),
                            local_image_paths.as_slice(),
                            true,
                        )
                    );
                    let prepared = prepare_ai_thread_workspace(
                        repo_root.as_path(),
                        requested_branch_name.as_str(),
                        start_mode,
                        selected_base_branch_name,
                    );
                    (execution_started_at.elapsed(), prepared)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok(prepared) => {
                            let previous_workspace_key = this.ai_workspace_key();
                            debug!(
                                "git action complete: epoch={} action=Prepare AI thread exec_elapsed_ms={} total_elapsed_ms={} mode={:?} branch={}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis(),
                                start_mode,
                                prepared.branch_name
                            );
                            this.git_status_message = Some(prepared.status_message.clone());
                            if let Some(target_id) = prepared.workspace_target_id.clone() {
                                this.sync_ai_visible_composer_prompt_to_draft(cx);
                                this.refresh_workspace_targets_from_git_state(cx);
                                this.ai_draft_workspace_target_id = Some(target_id.clone());
                                if let Some(workspace_key) = this.ai_workspace_key_for_draft() {
                                    this.seed_ai_workspace_state_for(workspace_key.as_str());
                                }
                                this.ai_handle_workspace_change(previous_workspace_key, cx);
                            } else {
                                this.request_snapshot_refresh_workflow_only(true, cx);
                                this.request_recent_commits_refresh(true, cx);
                            }
                            let pending_workspace_key = this.ai_workspace_key_for_draft();
                            if let Some(workspace_key) = pending_workspace_key
                                && let Some(pending) = this.ai_pending_thread_start.as_mut()
                            {
                                pending.workspace_key = workspace_key;
                            }

                            if this.send_ai_worker_command(
                                AiWorkerCommand::StartThread {
                                    prompt: prompt_for_start.clone(),
                                    local_image_paths: image_paths_for_start.clone(),
                                    session_overrides: session_overrides_for_start.clone(),
                                },
                                cx,
                            ) {
                                this.clear_current_ai_composer_status();
                                this.ai_pending_new_thread_selection = true;
                            } else {
                                let fallback_message =
                                    "Workspace prepared, but failed to start thread.".to_string();
                                this.set_current_ai_composer_status(fallback_message);
                                this.restore_ai_new_thread_draft_after_failure(cx);
                            }
                        }
                        Err(err) => {
                            error!(
                                "git action failed: epoch={} action=Prepare AI thread exec_elapsed_ms={} total_elapsed_ms={} mode={:?} err={err:#}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis(),
                                start_mode
                            );
                            let summary = err.to_string();
                            this.git_status_message = Some(format!("Git error: {err:#}"));
                            Self::push_error_notification(
                                format!("Prepare AI thread failed: {summary}"),
                                cx,
                            );
                            this.set_current_ai_composer_status(
                                format!("Failed to prepare workspace: {summary}"),
                            );
                            this.restore_ai_new_thread_draft_after_failure(cx);
                        }
                    }
                    cx.notify();
                });
            }
        });
        true
    }

    fn sync_ai_session_selection_from_state(&mut self) {
        let persisted = self
            .ai_workspace_key()
            .as_ref()
            .and_then(|workspace| self.state.ai_workspace_session_overrides.get(workspace).cloned())
            .unwrap_or_default();
        let (selected_model, selected_effort) = normalized_ai_session_selection(
            self.ai_models.as_slice(),
            persisted.model,
            persisted.effort,
        );

        self.ai_selected_model = selected_model;
        self.ai_selected_collaboration_mode = persisted.collaboration_mode;
        self.ai_selected_effort = selected_effort;
        self.ai_selected_service_tier = persisted.service_tier.unwrap_or_default();
    }

    fn seeded_ai_workspace_state_for_new_thread_workspace(
        current_state: &AiWorkspaceState,
    ) -> AiWorkspaceState {
        AiWorkspaceState {
            connection_state: AiConnectionState::Disconnected,
            bootstrap_loading: false,
            status_message: None,
            error_message: None,
            state_snapshot: hunk_codex::state::AiState::default(),
            selected_thread_id: None,
            new_thread_draft_active: current_state.new_thread_draft_active,
            new_thread_start_mode: current_state.new_thread_start_mode,
            worktree_base_branch_name: current_state.worktree_base_branch_name.clone(),
            pending_new_thread_selection: current_state.pending_new_thread_selection,
            pending_thread_start: current_state.pending_thread_start.clone(),
            timeline_follow_output: current_state.timeline_follow_output,
            thread_title_refresh_state_by_thread: std::collections::BTreeMap::new(),
            timeline_visible_turn_limit_by_thread: std::collections::BTreeMap::new(),
            in_progress_turn_started_at: std::collections::BTreeMap::new(),
            expanded_timeline_row_ids: std::collections::BTreeSet::new(),
            pending_approvals: Vec::new(),
            pending_user_inputs: Vec::new(),
            pending_user_input_answers: std::collections::BTreeMap::new(),
            account: current_state.account.clone(),
            requires_openai_auth: current_state.requires_openai_auth,
            pending_chatgpt_login_id: current_state.pending_chatgpt_login_id.clone(),
            pending_chatgpt_auth_url: current_state.pending_chatgpt_auth_url.clone(),
            rate_limits: current_state.rate_limits.clone(),
            models: current_state.models.clone(),
            experimental_features: current_state.experimental_features.clone(),
            collaboration_modes: current_state.collaboration_modes.clone(),
            include_hidden_models: current_state.include_hidden_models,
            selected_model: current_state.selected_model.clone(),
            selected_effort: current_state.selected_effort.clone(),
            selected_collaboration_mode: current_state.selected_collaboration_mode,
            selected_service_tier: current_state.selected_service_tier,
            mad_max_mode: current_state.mad_max_mode,
        }
    }

    fn persist_ai_workspace_session_for(&mut self, workspace: &str) {
        let session = AiThreadSessionState {
            model: self.ai_selected_model.clone(),
            effort: self.ai_selected_effort.clone(),
            collaboration_mode: self.ai_selected_collaboration_mode,
            service_tier: normalized_ai_service_tier_selection(self.ai_selected_service_tier),
        };

        if let Some(session) = normalized_thread_session_state(session) {
            self.state
                .ai_workspace_session_overrides
                .insert(workspace.to_string(), session);
        } else {
            self.state.ai_workspace_session_overrides.remove(workspace);
        }
        self.persist_state();
    }

    fn seed_ai_workspace_state_for(&mut self, workspace: &str) {
        self.persist_ai_workspace_session_for(workspace);
        seed_ai_workspace_preferences(
            &mut self.state,
            workspace,
            self.ai_mad_max_mode,
            self.ai_include_hidden_models,
        );
        self.persist_state();

        if self.ai_workspace_states.contains_key(workspace)
            || self.ai_hidden_runtimes.contains_key(workspace)
            || self.ai_worker_workspace_key.as_deref() == Some(workspace)
        {
            return;
        }

        let current_state = self.capture_current_ai_workspace_state();
        let seeded_state =
            Self::seeded_ai_workspace_state_for_new_thread_workspace(&current_state);
        self.ai_workspace_states
            .insert(workspace.to_string(), seeded_state);
    }

    fn persist_current_ai_workspace_session(&mut self) {
        let Some(workspace) = self.ai_workspace_key() else {
            return;
        };
        self.persist_ai_workspace_session_for(workspace.as_str());
    }

    fn clear_ai_composer_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_current_ai_composer_status();
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
        let (selected_model, selected_effort) = normalized_ai_session_selection(
            self.ai_models.as_slice(),
            self.ai_selected_model.clone(),
            self.ai_selected_effort.clone(),
        );
        self.ai_selected_model = selected_model;
        self.ai_selected_effort = selected_effort;
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

fn prepare_ai_thread_workspace(
    repo_root: &std::path::Path,
    requested_branch_name: &str,
    start_mode: AiNewThreadStartMode,
    selected_base_branch_name: Option<String>,
) -> anyhow::Result<AiPreparedThreadWorkspace> {
    match start_mode {
        AiNewThreadStartMode::Local => {
            let snapshot = load_workflow_snapshot(repo_root)?;
            let branch_name = snapshot.branch_name;
            Ok(AiPreparedThreadWorkspace {
                branch_name: branch_name.clone(),
                workspace_target_id: None,
                status_message: format!("Prepared local thread on branch {branch_name}"),
            })
        }
        AiNewThreadStartMode::Worktree => {
            let base_branch_name = match selected_base_branch_name {
                Some(base_branch_name) => base_branch_name,
                None => resolve_default_base_branch_name(repo_root)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "unable to resolve a default base branch (main/master/remote default)"
                    )
                })?,
            };
            let base_branch_synced =
                sync_branch_from_remote_if_tracked(repo_root, base_branch_name.as_str())
                    .with_context(|| format!("failed to sync base branch '{}'", base_branch_name))?;

            let status_base_label = if base_branch_synced {
                format!("synced {}", base_branch_name)
            } else {
                format!("base {}", base_branch_name)
            };

            let mut attempt = 0usize;
            loop {
                attempt = attempt.saturating_add(1);
                let candidate_branch_name = if attempt == 1 {
                    requested_branch_name.to_string()
                } else {
                    format!("{requested_branch_name}-{attempt}")
                };
                let request = hunk_git::worktree::CreateWorktreeRequest {
                    branch_name: candidate_branch_name.clone(),
                    base_branch_name: Some(base_branch_name.clone()),
                };
                match hunk_git::worktree::create_managed_worktree(repo_root, &request) {
                    Ok(created) => {
                        return Ok(AiPreparedThreadWorkspace {
                            branch_name: candidate_branch_name,
                            workspace_target_id: Some(created.id),
                            status_message: format!(
                                "Prepared worktree {} from {}",
                                created.name, status_base_label
                            ),
                        });
                    }
                    Err(err) => {
                        let summary = err.to_string();
                        let branch_exists = summary.contains("already exists");
                        if branch_exists && attempt < 20 {
                            continue;
                        }
                        return Err(err);
                    }
                }
            }
        }
    }
}

fn requested_branch_name_for_new_thread(
    start_mode: AiNewThreadStartMode,
    fallback_branch_name: String,
    generate_branch_name: impl FnOnce() -> Option<String>,
) -> String {
    match start_mode {
        AiNewThreadStartMode::Local => fallback_branch_name,
        AiNewThreadStartMode::Worktree => {
            generate_branch_name().unwrap_or(fallback_branch_name)
        }
    }
}

fn drain_ai_worker_events(
    event_rx: &std::sync::mpsc::Receiver<AiWorkerEvent>,
) -> (Vec<AiWorkerEvent>, bool) {
    let mut buffered_events = Vec::new();
    loop {
        match event_rx.try_recv() {
            Ok(event) => buffered_events.push(event),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                return (buffered_events, false);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return (buffered_events, true);
            }
        }
    }
}
