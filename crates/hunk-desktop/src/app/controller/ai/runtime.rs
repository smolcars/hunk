#[derive(Debug)]
struct AiPreparedThreadWorkspace {
    branch_name: String,
    workspace_root: PathBuf,
    workspace_target_id: Option<String>,
    status_message: String,
}

include!("runtime_events.rs");

impl DiffViewer {
    fn should_ignore_transient_visible_ai_snapshot(
        &self,
        next_state: &hunk_codex::state::AiState,
        next_active_thread_id: Option<&str>,
    ) -> bool {
        if self.ai_bootstrap_loading || self.ai_connection_state != AiConnectionState::Ready {
            return false;
        }

        let Some(current_thread_id) = self.current_ai_thread_id() else {
            return false;
        };
        if self.ai_state_snapshot.threads.is_empty() {
            return false;
        }
        if !self
            .ai_state_snapshot
            .threads
            .contains_key(current_thread_id.as_str())
        {
            return false;
        }
        if current_ai_renderable_visible_row_ids(self, current_thread_id.as_str()).is_empty() {
            return false;
        }
        if next_state.threads.contains_key(current_thread_id.as_str()) {
            return false;
        }
        if next_active_thread_id == Some(current_thread_id.as_str()) {
            return false;
        }
        true
    }

    fn apply_ai_snapshot(&mut self, snapshot: AiSnapshot, cx: &mut Context<Self>) {
        let previous_selected_thread = self.ai_selected_thread_id.clone();
        let previous_draft_key = self.current_ai_composer_draft_key();
        let previous_sidebar_workspace_key = self.ai_state_snapshot_workspace_key();
        let previous_workspace_key = self.ai_workspace_key();
        let visible_threads_changed =
            ai_snapshot_threads_changed(&self.ai_state_snapshot, &snapshot.state);
        let visible_threads_removed =
            ai_snapshot_removed_thread_ids(&self.ai_state_snapshot, &snapshot.state);
        let retainable_terminal_threads_removed = ai_snapshot_removed_retainable_terminal_threads(
            &self.ai_state_snapshot,
            &snapshot.state,
        );
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        let previous_selected_thread_sequence = previous_selected_thread
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
            skills,
            include_hidden_models,
            mad_max_mode,
        } = snapshot;
        if self.should_ignore_transient_visible_ai_snapshot(&state, active_thread_id.as_deref()) {
            debug!(
                selected_thread_id = previous_selected_thread.as_deref().unwrap_or("<none>"),
                current_threads = self.ai_state_snapshot.threads.len(),
                next_threads = state.threads.len(),
                next_active_thread_id = active_thread_id.as_deref().unwrap_or("<none>"),
                "ignoring transient visible AI snapshot that would evict the current thread"
            );
            return;
        }
        let changed_row_ids = previous_selected_thread
            .as_deref()
            .map(|thread_id| {
                timeline_row_ids_with_height_changes(&self.ai_state_snapshot, &state, thread_id)
            })
            .unwrap_or_default();

        self.ai_state_snapshot = state;
        reconcile_ai_pending_steers(&mut self.ai_pending_steers, &self.ai_state_snapshot);
        let restorable_pending_steers =
            take_restorable_ai_pending_steers(&mut self.ai_pending_steers, &self.ai_state_snapshot);
        let restored_pending_steer_drafts =
            self.restore_ai_pending_steers_to_drafts(restorable_pending_steers);
        let restored_queued_message_drafts =
            self.maybe_restore_interrupted_ai_queued_messages_to_drafts();
        let worker_workspace_key = self.ai_worker_workspace_key.clone();
        self.maybe_submit_ready_ai_queued_messages(worker_workspace_key.as_deref(), cx);
        self.rebuild_ai_timeline_indexes();
        self.sync_ai_in_progress_turn_started_at();
        self.ai_composer_activity_elapsed_second =
            self.current_ai_composer_activity_elapsed_second();
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
        if self.ai_skills != skills {
            self.ai_skills_generation = self.ai_skills_generation.saturating_add(1);
            self.ai_composer_completion_sync_key = None;
        }
        self.ai_skills = skills;
        self.ai_include_hidden_models = include_hidden_models;
        self.ai_mad_max_mode = mad_max_mode;
        if visible_threads_removed {
            self.ai_thread_title_refresh_state_by_thread
                .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
            self.ai_timeline_visible_turn_limit_by_thread
                .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
            self.prune_ai_composer_statuses();
        }
        self.invalidate_ai_visible_frame_state_with_reason("runtime");

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
            self.ai_selected_thread_id = active_thread_id.clone();
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
        if self
            .ai_pending_thread_start
            .as_ref()
            .is_some_and(|pending| {
                pending.thread_id.as_ref().is_some_and(|thread_id| {
                    ai_state_has_user_message_for_thread(&self.ai_state_snapshot, thread_id)
                })
            })
        {
            self.ai_pending_thread_start = None;
        }
        let next_sidebar_workspace_key = self.ai_state_snapshot_workspace_key();
        let next_workspace_key = self.ai_workspace_key();
        if visible_threads_changed || previous_sidebar_workspace_key != next_sidebar_workspace_key {
            self.rebuild_ai_thread_sidebar_state();
        }
        self.ai_handle_terminal_thread_change(
            previous_selected_thread.clone(),
            self.ai_selected_thread_id.clone(),
            cx,
        );
        if retainable_terminal_threads_removed || previous_workspace_key != next_workspace_key {
            self.ai_prune_terminal_threads("applying AI snapshot", cx);
        }
        if should_scroll_timeline_to_bottom_on_selection_change(
            previous_selected_thread.as_deref(),
            self.ai_selected_thread_id.as_deref(),
        ) {
            self.ai_timeline_follow_output = true;
            self.ai_scroll_timeline_to_bottom = true;
            self.ai_workspace_selection = None;
            self.ai_workspace_surface_last_scroll_offset = None;
            self.ai_expanded_timeline_row_ids.clear();
            self.ai_text_selection = None;
            self.ai_text_selection_drag_pointer = None;
            self.ai_text_selection_auto_scroll_task = Task::ready(());
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
        self.flush_ai_timeline_scroll_request();

        if visible_threads_removed {
            self.prune_ai_composer_drafts();
        }
        let next_draft_key = self.current_ai_composer_draft_key();
        if previous_draft_key != next_draft_key
            || next_draft_key.as_ref().is_some_and(|key| {
                restored_pending_steer_drafts.contains(key)
                    || restored_queued_message_drafts.contains(key)
            })
        {
            self.restore_ai_visible_composer_from_current_draft(cx);
        }
        self.maybe_refresh_selected_thread_metadata(cx);
        self.sync_ai_session_selection_from_state();
        self.sync_ai_composer_completion_menus(cx);
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
            self.ai_in_progress_turn_started_at
                .entry(key)
                .or_insert(now);
        }

        self.ai_in_progress_turn_started_at
            .retain(|key, _| in_progress_turn_keys.contains(key));
    }
}

fn ai_snapshot_threads_changed(
    previous_state: &hunk_codex::state::AiState,
    next_state: &hunk_codex::state::AiState,
) -> bool {
    previous_state.threads != next_state.threads
}

fn ai_snapshot_removed_thread_ids(
    previous_state: &hunk_codex::state::AiState,
    next_state: &hunk_codex::state::AiState,
) -> bool {
    previous_state
        .threads
        .keys()
        .any(|thread_id| !next_state.threads.contains_key(thread_id))
}

fn ai_snapshot_removed_retainable_terminal_threads(
    previous_state: &hunk_codex::state::AiState,
    next_state: &hunk_codex::state::AiState,
) -> bool {
    let next_retainable = ai_retainable_terminal_thread_ids(
        next_state,
        std::iter::empty::<&hunk_codex::state::AiState>(),
    );

    previous_state
        .threads
        .values()
        .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
        .any(|thread| !next_retainable.contains(thread.cwd.as_str()))
}

include!("runtime_composer.rs");

impl DiffViewer {
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

    fn validated_current_ai_prompt(&mut self, cx: &mut Context<Self>) -> Option<AiValidatedPrompt> {
        let prompt = self.ai_composer_input_state.read(cx).value().to_string();
        let local_image_paths = self.current_ai_composer_local_images();
        let raw_skill_bindings = self.current_ai_composer_skill_bindings();
        let (prompt, skill_bindings) =
            crate::app::ai_composer_completion::trim_prompt_with_skill_bindings(
                prompt.as_str(),
                raw_skill_bindings.as_slice(),
            );
        let selected_skills = crate::app::ai_composer_completion::selected_skills_from_bindings(
            skill_bindings.as_slice(),
            self.ai_skills.as_slice(),
        );
        if prompt.is_empty() && local_image_paths.is_empty() {
            self.set_current_ai_composer_status("Prompt cannot be empty.", cx);
            cx.notify();
            return None;
        }
        if !local_image_paths.is_empty() && !self.current_ai_model_supports_image_inputs() {
            self.set_current_ai_composer_status(
                "Selected model does not support image attachments. Remove attachments or switch models.",
                cx,
            );
            cx.notify();
            return None;
        }
        Some(AiValidatedPrompt {
            prompt,
            local_images: local_image_paths,
            selected_skills,
            skill_bindings,
        })
    }

    fn send_current_ai_prompt(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(validated) = self.validated_current_ai_prompt(cx) else {
            return false;
        };
        let AiValidatedPrompt {
            prompt,
            local_images: local_image_paths,
            selected_skills,
            skill_bindings,
        } = validated;
        if self.ai_command_tx.is_none() {
            self.ensure_ai_runtime_started(cx);
        }
        if self.ai_command_tx.is_none()
            || ai_prompt_send_waiting_on_connection(
                self.ai_connection_state,
                self.ai_bootstrap_loading,
            )
        {
            self.set_current_ai_composer_status("Cannot send until Codex finishes connecting.", cx);
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
                    selected_skills,
                    skill_bindings,
                    session_overrides,
                },
                cx,
            );
            if sent {
                self.clear_current_ai_composer_status();
            }
            return sent;
        }

        let pending_thread_start =
            self.ai_workspace_key_for_draft()
                .map(|workspace_key| AiPendingThreadStart {
                    workspace_key,
                    prompt: prompt.clone().unwrap_or_default(),
                    local_images: local_image_paths.clone(),
                    skill_bindings: skill_bindings.clone(),
                    started_at: Instant::now(),
                    start_mode: self.ai_new_thread_start_mode,
                    thread_id: None,
                });
        let started = self.prepare_workspace_and_start_ai_thread(
            prompt,
            local_image_paths,
            selected_skills,
            skill_bindings,
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
        selected_skills: Vec<crate::app::AiPromptSkillReference>,
        skill_bindings: Vec<crate::app::AiComposerSkillBinding>,
        session_overrides: AiTurnSessionOverrides,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.git_controls_busy() {
            self.set_current_ai_composer_status(
                "Wait for the active workspace action to finish.",
                cx,
            );
            cx.notify();
            return false;
        }

        let Some(repo_root) = self.ai_draft_workspace_root() else {
            self.set_current_ai_composer_status(
                "Open a workspace before starting an AI thread.",
                cx,
            );
            cx.notify();
            return false;
        };

        let start_mode = self.ai_new_thread_start_mode;
        let selected_base_branch_name = self
            .ai_selected_worktree_base_branch_name()
            .map(str::to_string);
        let prompt_seed = prompt.clone().unwrap_or_default();
        let fallback_branch_name = ai_branch_name_for_prompt(
            prompt_seed.as_str(),
            start_mode == AiNewThreadStartMode::Worktree,
        );
        let epoch = self.begin_git_action("Prepare AI thread", cx);
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let prompt_for_start = prompt.clone();
            let image_paths_for_start = local_image_paths.clone();
            let selected_skills_for_start = selected_skills.clone();
            let skill_bindings_for_start = skill_bindings.clone();
            let image_paths_for_rename = local_image_paths.clone();
            let prompt_seed_for_rename = prompt_seed.clone();
            let session_overrides_for_start = session_overrides.clone();
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let requested_branch_name =
                        requested_branch_name_for_new_thread(fallback_branch_name);
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
                            this.ai_draft_workspace_root_override =
                                Some(prepared.workspace_root.clone());
                            if let Some(target_id) = prepared.workspace_target_id.clone() {
                                let active_repo_matches_prepared_workspace = this
                                    .primary_repo_root()
                                    .and_then(|root| {
                                        hunk_git::worktree::primary_repo_root(
                                            prepared.workspace_root.as_path(),
                                        )
                                        .ok()
                                        .map(|prepared_root| root == prepared_root)
                                    })
                                    .unwrap_or(false);
                                this.sync_ai_visible_composer_prompt_to_draft(cx);
                                if active_repo_matches_prepared_workspace {
                                    this.refresh_workspace_targets_from_git_state(cx);
                                    this.ai_draft_workspace_target_id = Some(target_id.clone());
                                } else {
                                    this.ai_draft_workspace_target_id = None;
                                }
                                if let Some(workspace_key) = this.ai_workspace_key_for_draft() {
                                    this.seed_ai_workspace_state_for(workspace_key.as_str());
                                }
                                this.ai_handle_workspace_change(previous_workspace_key, cx);
                            } else {
                                this.request_snapshot_refresh_workflow_only(true, cx);
                                this.request_recent_commits_refresh(true, cx);
                            }
                            let pending_workspace_key = this.ai_workspace_key_for_draft();
                            if let Some(workspace_key) = pending_workspace_key.as_ref()
                                && let Some(pending) = this.ai_pending_thread_start.as_mut()
                            {
                                pending.workspace_key = workspace_key.clone();
                            }

                            if this.send_ai_worker_command(
                                AiWorkerCommand::StartThread {
                                    prompt: prompt_for_start.clone(),
                                    local_image_paths: image_paths_for_start.clone(),
                                    selected_skills: selected_skills_for_start.clone(),
                                    skill_bindings: skill_bindings_for_start.clone(),
                                    session_overrides: session_overrides_for_start.clone(),
                                },
                                cx,
                            ) {
                                if let Some(workspace_key) = pending_workspace_key.clone() {
                                    this.schedule_ai_worktree_branch_rename(
                                        start_mode,
                                        workspace_key,
                                        prepared.branch_name.clone(),
                                        prompt_seed_for_rename.clone(),
                                        image_paths_for_rename.clone(),
                                        cx,
                                    );
                                }
                                this.clear_current_ai_composer_status();
                                this.ai_pending_new_thread_selection = true;
                            } else {
                                let fallback_message =
                                    "Workspace prepared, but failed to start thread.".to_string();
                                this.set_current_ai_composer_status(fallback_message, cx);
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
                                cx,
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

    fn schedule_ai_worktree_branch_rename(
        &mut self,
        start_mode: AiNewThreadStartMode,
        workspace_key: String,
        current_branch_name: String,
        prompt_seed: String,
        local_image_paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let codex_executable = Self::resolve_codex_executable_path();
        if let Err(error) = Self::validate_codex_executable_path(codex_executable.as_path()) {
            debug!(
                "skipping AI worktree branch rename generation for {}: {}",
                workspace_key, error
            );
            return;
        }

        let workspace_root = PathBuf::from(workspace_key.as_str());
        cx.spawn(async move |this, cx| {
            let workspace_root_for_generation = workspace_root.clone();
            let current_branch_for_generation = current_branch_name.clone();
            let generated_branch_name = cx
                .background_executor()
                .spawn(async move {
                    background_branch_name_for_new_thread(
                        start_mode,
                        current_branch_for_generation.as_str(),
                        || {
                            try_ai_branch_name_for_prompt(
                                codex_executable.as_path(),
                                workspace_root_for_generation.as_path(),
                                prompt_seed.as_str(),
                                local_image_paths.as_slice(),
                                true,
                            )
                        },
                    )
                })
                .await;

            let Some(generated_branch_name) = generated_branch_name else {
                return;
            };

            const RENAME_RETRY_INTERVAL: Duration = Duration::from_millis(250);
            const RENAME_RETRY_LIMIT: usize = 120;

            let mut retry_count = 0usize;
            let epoch = loop {
                let Some(view) = this.upgrade() else {
                    return;
                };

                let mut rename_epoch = None;
                view.update(cx, |this, cx| {
                    if this.git_controls_busy() {
                        return;
                    }

                    rename_epoch = Some(this.begin_git_action("Rename AI worktree branch", cx));
                });

                if let Some(epoch) = rename_epoch {
                    break epoch;
                }

                retry_count = retry_count.saturating_add(1);
                if retry_count >= RENAME_RETRY_LIMIT {
                    debug!(
                        "skipping AI worktree branch rename for {} because the workspace stayed busy",
                        workspace_key
                    );
                    return;
                }

                cx.background_executor()
                    .timer(RENAME_RETRY_INTERVAL)
                    .await;
            };

            let rename_workspace_root = workspace_root.clone();
            let rename_current_branch_name = current_branch_name.clone();
            let rename_generated_branch_name = generated_branch_name.clone();
            let rename_started_at = Instant::now();
            let rename_result = cx
                .background_executor()
                .spawn(async move {
                    rename_branch_if_current_unpublished(
                        rename_workspace_root.as_path(),
                        rename_current_branch_name.as_str(),
                        rename_generated_branch_name.as_str(),
                    )
                })
                .await;

            let Some(view) = this.upgrade() else {
                return;
            };

            let workspace_key = workspace_key.clone();
            let current_branch_name = current_branch_name.clone();
            let generated_branch_name = generated_branch_name.clone();
            let renamed_workspace_root = workspace_root.clone();
            view.update(cx, |this, cx| {
                if epoch != this.git_action_epoch {
                    return;
                }

                this.finish_git_action();
                match rename_result {
                    Ok(RenameBranchIfSafeOutcome::Renamed) => {
                        debug!(
                            "git action complete: epoch={} action=Rename AI worktree branch exec_elapsed_ms={} workspace={} from={} to={}",
                            epoch,
                            rename_started_at.elapsed().as_millis(),
                            workspace_key,
                            current_branch_name,
                            generated_branch_name
                        );
                        this.refresh_workspace_targets_from_git_state(cx);
                        let selected_git_workspace_root = this.selected_git_workspace_root();
                        if selected_git_workspace_root.as_ref() == this.repo_root.as_ref() {
                            this.request_snapshot_refresh_workflow_only(true, cx);
                        } else if selected_git_workspace_root.as_ref()
                            == Some(&renamed_workspace_root)
                        {
                            this.request_git_workspace_refresh(false, cx);
                        }
                        cx.notify();
                    }
                    Ok(RenameBranchIfSafeOutcome::Skipped(reason)) => {
                        debug!(
                            "git action complete: epoch={} action=Rename AI worktree branch exec_elapsed_ms={} workspace={} from={} to={} skipped={:?}",
                            epoch,
                            rename_started_at.elapsed().as_millis(),
                            workspace_key,
                            current_branch_name,
                            generated_branch_name,
                            reason
                        );
                    }
                    Err(err) => {
                        debug!(
                            "git action failed: epoch={} action=Rename AI worktree branch exec_elapsed_ms={} workspace={} from={} to={} err={err:#}",
                            epoch,
                            rename_started_at.elapsed().as_millis(),
                            workspace_key,
                            current_branch_name,
                            generated_branch_name
                        );
                    }
                }
            });
        })
        .detach();
    }

    fn sync_ai_session_selection_from_state(&mut self) {
        let resolved = { self.resolve_ai_current_state() };
        let persisted = {
            resolved_ai_thread_session_state(
                &self.state,
                resolved.current_thread_id.as_deref(),
                resolved.workspace_key.as_deref(),
            )
        };
        let (selected_model, selected_effort) = {
            normalized_ai_session_selection(
                self.ai_models.as_slice(),
                persisted.model,
                persisted.effort,
            )
        };

        {
            self.ai_selected_model = selected_model;
            self.ai_selected_collaboration_mode = persisted.collaboration_mode;
            self.ai_selected_effort = selected_effort;
            self.ai_selected_service_tier = persisted.service_tier.unwrap_or_default();
            self.ai_review_mode_active = resolved
                .current_thread_id
                .as_ref()
                .is_some_and(|thread_id| self.ai_review_mode_thread_ids.contains(thread_id));
        }
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
            pending_steers: Vec::new(),
            queued_messages: Vec::new(),
            interrupt_restore_queued_thread_ids: std::collections::BTreeSet::new(),
            timeline_follow_output: current_state.timeline_follow_output,
            inline_review_selected_row_id_by_thread: std::collections::BTreeMap::new(),
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
            skills: Vec::new(),
            include_hidden_models: current_state.include_hidden_models,
            selected_model: current_state.selected_model.clone(),
            selected_effort: current_state.selected_effort.clone(),
            selected_collaboration_mode: current_state.selected_collaboration_mode,
            selected_service_tier: current_state.selected_service_tier,
            review_mode_thread_ids: std::collections::BTreeSet::new(),
            mad_max_mode: current_state.mad_max_mode,
            draft_workspace_root_override: current_state.draft_workspace_root_override.clone(),
            terminal_open: current_state.terminal_open,
            terminal_follow_output: current_state.terminal_follow_output,
            terminal_height_px: current_state.terminal_height_px,
            terminal_input_draft: current_state.terminal_input_draft.clone(),
            terminal_session: current_state.terminal_session.clone(),
        }
    }

    fn persist_ai_session_for_target(&mut self, thread_id: Option<&str>, workspace: Option<&str>) {
        let session = AiThreadSessionState {
            model: self.ai_selected_model.clone(),
            effort: self.ai_selected_effort.clone(),
            collaboration_mode: self.ai_selected_collaboration_mode,
            service_tier: normalized_ai_service_tier_selection(self.ai_selected_service_tier),
        };

        if let Some(thread_id) = thread_id {
            if let Some(session) = normalized_thread_session_state(session) {
                self.state
                    .ai_thread_session_overrides
                    .insert(thread_id.to_string(), session);
            } else {
                self.state.ai_thread_session_overrides.remove(thread_id);
            }
        } else if let Some(workspace) = workspace {
            if let Some(session) = normalized_thread_session_state(session) {
                self.state
                    .ai_workspace_session_overrides
                    .insert(workspace.to_string(), session);
            } else {
                self.state.ai_workspace_session_overrides.remove(workspace);
            }
        }
        self.persist_state();
    }

    fn seed_ai_workspace_state_for(&mut self, workspace: &str) {
        self.persist_ai_session_for_target(None, Some(workspace));
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
        let seeded_state = Self::seeded_ai_workspace_state_for_new_thread_workspace(&current_state);
        self.ai_workspace_states
            .insert(workspace.to_string(), seeded_state);
    }

    fn persist_current_ai_workspace_session(&mut self) {
        let current_thread_id = self.current_ai_thread_id();
        let workspace_key = self.ai_workspace_key();
        if current_thread_id.is_none() && workspace_key.is_none() {
            return;
        }
        self.persist_ai_session_for_target(current_thread_id.as_deref(), workspace_key.as_deref());
    }

    fn clear_ai_composer_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_current_ai_composer_status();
        if let Some(draft) = self.current_ai_composer_draft_mut() {
            draft.prompt.clear();
            draft.local_images.clear();
            draft.skill_bindings.clear();
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
                workspace_root: repo_root.to_path_buf(),
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
                    .with_context(|| {
                        format!("failed to sync base branch '{}'", base_branch_name)
                    })?;

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
                            workspace_root: created.root.clone(),
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

fn requested_branch_name_for_new_thread(fallback_branch_name: String) -> String {
    fallback_branch_name
}

fn background_branch_name_for_new_thread(
    start_mode: AiNewThreadStartMode,
    current_branch_name: &str,
    generate_branch_name: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if start_mode != AiNewThreadStartMode::Worktree {
        return None;
    }

    let generated_branch_name = generate_branch_name()?;
    let generated_branch_name = generated_branch_name.trim();
    if generated_branch_name.is_empty() || generated_branch_name == current_branch_name {
        return None;
    }

    Some(generated_branch_name.to_string())
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
