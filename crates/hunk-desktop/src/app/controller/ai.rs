use std::time::Duration;

use hunk_domain::state::AiThreadSessionState;
use hunk_codex::state::ThreadSummary;
use hunk_codex::state::TurnStatus;
use hunk_domain::state::AppState;

impl DiffViewer {
    const AI_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(80);

    pub(super) fn ensure_ai_runtime_started(&mut self, cx: &mut Context<Self>) {
        if self.ai_command_tx.is_some() {
            return;
        }

        self.sync_ai_workspace_preferences_from_state();

        let Some(cwd) = self.ai_workspace_cwd() else {
            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_error_message = Some("Open a workspace before using AI.".to_string());
            cx.notify();
            return;
        };

        let Some(codex_home) = Self::resolve_codex_home_path() else {
            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_error_message = Some("Unable to resolve ~/.codex home directory.".to_string());
            cx.notify();
            return;
        };

        let codex_executable = Self::resolve_codex_executable_path();
        if let Err(error) = Self::validate_codex_executable_path(codex_executable.as_path()) {
            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_error_message = Some(error);
            cx.notify();
            return;
        }
        let (command_tx, command_rx) = std::sync::mpsc::channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let mut start_config = AiWorkerStartConfig::new(cwd, codex_executable, codex_home);
        start_config.mad_max_mode = self.ai_mad_max_mode;
        start_config.include_hidden_models = self.ai_include_hidden_models;

        let worker = spawn_ai_worker(start_config, command_rx, event_tx);

        self.ai_connection_state = AiConnectionState::Connecting;
        self.ai_error_message = None;
        self.ai_status_message = Some("Starting Codex App Server...".to_string());
        self.ai_command_tx = Some(command_tx);
        self.ai_worker_thread = Some(worker);

        let epoch = self.next_ai_event_epoch();
        self.start_ai_event_listener(event_rx, epoch, cx);
        cx.notify();
    }

    pub(super) fn ai_refresh_threads(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::RefreshThreads, cx);
    }

    pub(super) fn ai_refresh_account(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::RefreshAccount, cx);
        self.send_ai_worker_command(AiWorkerCommand::RefreshRateLimits, cx);
    }

    pub(super) fn ai_refresh_session_metadata(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::RefreshSessionMetadata, cx);
    }

    pub(super) fn ai_set_include_hidden_models_action(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_key) = self.ai_workspace_key() else {
            self.ai_status_message = Some("Open a workspace before changing model visibility.".to_string());
            cx.notify();
            return;
        };
        self.ai_include_hidden_models = enabled;
        if enabled {
            self.state
                .ai_workspace_include_hidden_models
                .insert(workspace_key, true);
        } else {
            self.state
                .ai_workspace_include_hidden_models
                .remove(workspace_key.as_str());
        }
        self.persist_state();
        self.send_ai_worker_command_if_running(
            AiWorkerCommand::SetIncludeHiddenModels { enabled },
            cx,
        );
        cx.notify();
    }

    pub(super) fn ai_start_chatgpt_login_action(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::StartChatgptLogin, cx);
    }

    pub(super) fn ai_cancel_chatgpt_login_action(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::CancelChatgptLogin, cx);
    }

    pub(super) fn ai_logout_account_action(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::LogoutAccount, cx);
    }

    pub(super) fn ai_create_thread_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = self.ai_composer_input_state.read(cx).value().trim().to_string();
        let prompt = (!prompt.is_empty()).then_some(prompt);

        let session_overrides = self.current_ai_turn_session_overrides();
        if self.send_ai_worker_command(
            AiWorkerCommand::StartThread {
                prompt,
                session_overrides,
            },
            cx,
        ) {
            self.ai_composer_input_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        }
    }

    pub(super) fn ai_send_prompt_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = self.ai_composer_input_state.read(cx).value().trim().to_string();
        if prompt.is_empty() {
            self.ai_status_message = Some("Prompt cannot be empty.".to_string());
            cx.notify();
            return;
        }

        let session_overrides = self.current_ai_turn_session_overrides();
        let sent = if let Some(thread_id) = self.current_ai_thread_id() {
            self.send_ai_worker_command(
                AiWorkerCommand::SendPrompt {
                    thread_id,
                    prompt,
                    session_overrides,
                },
                cx,
            )
        } else {
            self.send_ai_worker_command(
                AiWorkerCommand::StartThread {
                    prompt: Some(prompt),
                    session_overrides,
                },
                cx,
            )
        };

        if sent {
            self.ai_composer_input_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        }
    }

    pub(super) fn ai_start_review_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = self.current_ai_thread_id() else {
            self.ai_status_message = Some("Select a thread before starting review.".to_string());
            cx.notify();
            return;
        };

        let instructions = self.ai_review_input_state.read(cx).value().trim().to_string();
        let instructions = if instructions.is_empty() {
            "Review the current working-copy changes for correctness and regressions.".to_string()
        } else {
            instructions
        };

        if self.send_ai_worker_command(
            AiWorkerCommand::StartReview {
                thread_id,
                instructions,
            },
            cx,
        ) {
            self.ai_review_input_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        }
    }

    pub(super) fn ai_interrupt_turn_action(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_ai_thread_id() else {
            self.ai_status_message = Some("Select a thread before interrupting a turn.".to_string());
            cx.notify();
            return;
        };

        let Some(turn_id) = self.current_ai_in_progress_turn_id(thread_id.as_str()) else {
            self.ai_status_message = Some("No in-progress turn to interrupt.".to_string());
            cx.notify();
            return;
        };

        self.send_ai_worker_command(
            AiWorkerCommand::InterruptTurn { thread_id, turn_id },
            cx,
        );
    }

    pub(super) fn ai_run_command_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command_line = self.ai_command_input_state.read(cx).value().trim().to_string();
        if command_line.is_empty() {
            self.ai_status_message = Some("Command cannot be empty.".to_string());
            cx.notify();
            return;
        }

        if self.send_ai_worker_command(AiWorkerCommand::CommandExec { command_line }, cx) {
            self.ai_command_input_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        }
    }

    pub(super) fn ai_set_mad_max_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let Some(workspace_key) = self.ai_workspace_key() else {
            self.ai_status_message = Some("Open a workspace before changing Mad Max mode.".to_string());
            cx.notify();
            return;
        };

        if enabled {
            self.state.ai_workspace_mad_max.insert(workspace_key, true);
        } else {
            self.state.ai_workspace_mad_max.remove(workspace_key.as_str());
        }
        self.persist_state();
        self.ai_mad_max_mode = enabled;
        self.send_ai_worker_command_if_running(AiWorkerCommand::SetMadMaxMode { enabled }, cx);
        self.ai_status_message = Some(if enabled {
            "Mad Max mode enabled: approvals are auto-accepted with full sandbox access."
                .to_string()
        } else {
            "Mad Max mode disabled: command and file approvals require explicit review."
                .to_string()
        });
        cx.notify();
    }

    pub(super) fn ai_select_model_action(
        &mut self,
        model_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_selected_model = model_id;
        self.ai_selected_collaboration_mode = None;
        self.normalize_ai_selected_effort();
        self.persist_current_ai_thread_session();
        cx.notify();
    }

    pub(super) fn ai_select_effort_action(
        &mut self,
        effort: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_selected_effort = effort;
        self.ai_selected_collaboration_mode = None;
        self.normalize_ai_selected_effort();
        self.persist_current_ai_thread_session();
        cx.notify();
    }

    pub(super) fn ai_select_collaboration_mode_action(
        &mut self,
        mode_name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_selected_collaboration_mode = mode_name.clone();
        if let Some(mode_name) = mode_name
            && let Some(mask) = self
                .ai_collaboration_modes
                .iter()
                .find(|mask| mask.name == mode_name)
        {
            if let Some(model) = mask.model.as_ref() {
                self.ai_selected_model = Some(model.clone());
            }
            if let Some(reasoning_effort) = mask.reasoning_effort.unwrap_or(None) {
                self.ai_selected_effort = Some(reasoning_effort_key(&reasoning_effort));
            }
        }
        self.normalize_ai_selected_effort();
        self.persist_current_ai_thread_session();
        cx.notify();
    }

    pub(super) fn ai_resolve_pending_approval_action(
        &mut self,
        request_id: String,
        decision: AiApprovalDecision,
        cx: &mut Context<Self>,
    ) {
        if self.send_ai_worker_command(
            AiWorkerCommand::ResolveApproval {
                request_id,
                decision,
            },
            cx,
        ) {
            self.ai_status_message = Some(match decision {
                AiApprovalDecision::Accept => "Approval accepted.".to_string(),
                AiApprovalDecision::Decline => "Approval declined.".to_string(),
            });
            cx.notify();
        }
    }

    pub(super) fn ai_select_thread(
        &mut self,
        thread_id: String,
        cx: &mut Context<Self>,
    ) {
        self.ai_selected_thread_id = Some(thread_id.clone());
        self.sync_ai_session_selection_from_state();
        self.send_ai_worker_command(AiWorkerCommand::SelectThread { thread_id }, cx);
        cx.notify();
    }

    pub(super) fn ai_open_review_tab(&mut self, cx: &mut Context<Self>) {
        self.set_workspace_view_mode(WorkspaceViewMode::Diff, cx);
    }

    pub(super) fn ai_visible_threads(&self) -> Vec<ThreadSummary> {
        sorted_threads(&self.ai_state_snapshot)
    }

    pub(super) fn ai_timeline_turn_ids(&self, thread_id: &str) -> Vec<String> {
        let mut turns = self
            .ai_state_snapshot
            .turns
            .values()
            .filter(|turn| turn.thread_id == thread_id)
            .cloned()
            .collect::<Vec<_>>();
        turns.sort_by_key(|turn| turn.last_sequence);
        turns.into_iter().map(|turn| turn.id).collect()
    }

    pub(super) fn ai_timeline_item_ids(&self, turn_id: &str) -> Vec<String> {
        let mut items = self
            .ai_state_snapshot
            .items
            .values()
            .filter(|item| item.turn_id == turn_id)
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by_key(|item| item.last_sequence);
        items.into_iter().map(|item| item.id).collect()
    }

    pub(super) fn ai_visible_pending_approvals(&self) -> Vec<AiPendingApproval> {
        self.ai_pending_approvals.clone()
    }

    pub(super) fn ai_visible_pending_user_inputs(&self) -> Vec<AiPendingUserInputRequest> {
        self.ai_pending_user_inputs.clone()
    }

    pub(super) fn ai_select_pending_user_input_option_action(
        &mut self,
        request_id: String,
        question_id: String,
        option: String,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self
            .ai_pending_user_inputs
            .iter()
            .find(|request| request.request_id == request_id)
        else {
            return;
        };

        let answers = self
            .ai_pending_user_input_answers
            .entry(request_id)
            .or_insert_with(|| normalized_user_input_answers(request, None));
        answers.insert(question_id, vec![option]);
        cx.notify();
    }

    pub(super) fn ai_submit_pending_user_input_action(
        &mut self,
        request_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self
            .ai_pending_user_inputs
            .iter()
            .find(|request| request.request_id == request_id)
        else {
            self.ai_status_message = Some("User input request no longer exists.".to_string());
            cx.notify();
            return;
        };

        let answers = self
            .ai_pending_user_input_answers
            .get(request_id.as_str())
            .cloned()
            .unwrap_or_else(|| normalized_user_input_answers(request, None));

        if self.send_ai_worker_command(
            AiWorkerCommand::SubmitUserInput {
                request_id: request_id.clone(),
                answers,
            },
            cx,
        ) {
            self.ai_status_message = Some(format!("Submitted user input for request {request_id}."));
            cx.notify();
        }
    }

    pub(super) fn current_ai_thread_id(&self) -> Option<String> {
        if let Some(selected) = self.ai_selected_thread_id.as_ref()
            && self.ai_state_snapshot.threads.contains_key(selected)
        {
            return Some(selected.clone());
        }

        self.ai_workspace_key().and_then(|cwd| {
            self.ai_state_snapshot
                .active_thread_for_cwd(cwd.as_str())
                .map(ToOwned::to_owned)
        })
    }

    pub(super) fn current_ai_in_progress_turn_id(&self, thread_id: &str) -> Option<String> {
        self.ai_state_snapshot
            .turns
            .values()
            .filter(|turn| turn.thread_id == thread_id && turn.status == TurnStatus::InProgress)
            .max_by_key(|turn| turn.last_sequence)
            .map(|turn| turn.id.clone())
    }

    fn ai_workspace_cwd(&self) -> Option<std::path::PathBuf> {
        self.repo_root.clone().or_else(|| self.project_path.clone())
    }

    fn ai_workspace_key(&self) -> Option<String> {
        self.ai_workspace_cwd()
            .map(|cwd| cwd.to_string_lossy().to_string())
    }

    pub(super) fn ai_sync_workspace_preferences(&mut self, cx: &mut Context<Self>) {
        let previous_mad_max = self.ai_mad_max_mode;
        let previous_include_hidden = self.ai_include_hidden_models;
        self.sync_ai_workspace_preferences_from_state();
        if previous_mad_max != self.ai_mad_max_mode {
            self.send_ai_worker_command_if_running(
                AiWorkerCommand::SetMadMaxMode {
                    enabled: self.ai_mad_max_mode,
                },
                cx,
            );
        }
        if previous_include_hidden != self.ai_include_hidden_models {
            self.send_ai_worker_command_if_running(
                AiWorkerCommand::SetIncludeHiddenModels {
                    enabled: self.ai_include_hidden_models,
                },
                cx,
            );
        }
        self.sync_ai_session_selection_from_state();
        cx.notify();
    }

    fn sync_ai_workspace_preferences_from_state(&mut self) {
        self.ai_mad_max_mode = workspace_mad_max_mode(&self.state, self.ai_workspace_key().as_deref());
        self.ai_include_hidden_models = workspace_include_hidden_models(
            &self.state,
            self.ai_workspace_key().as_deref(),
        );
    }

    fn resolve_codex_executable_path() -> std::path::PathBuf {
        std::env::var_os("HUNK_CODEX_EXECUTABLE")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|path| resolve_bundled_codex_executable_from_exe(path.as_path()))
            })
            .unwrap_or_else(|| std::path::PathBuf::from("codex"))
    }

    fn validate_codex_executable_path(path: &std::path::Path) -> Result<(), String> {
        if is_command_name_without_path(path) {
            return Ok(());
        }
        if !path.exists() {
            return Err(format!(
                "Bundled Codex executable not found at {}",
                path.display()
            ));
        }
        if !path.is_file() {
            return Err(format!(
                "Bundled Codex executable path is not a file: {}",
                path.display()
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(path)
                .map_err(|error| format!("Unable to inspect Codex executable: {error}"))?;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(format!(
                    "Bundled Codex executable is not marked executable: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    fn resolve_codex_home_path() -> Option<std::path::PathBuf> {
        if let Some(path) = std::env::var_os("CODEX_HOME") {
            return Some(std::path::PathBuf::from(path));
        }

        std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".codex"))
    }

    fn send_ai_worker_command(&mut self, command: AiWorkerCommand, cx: &mut Context<Self>) -> bool {
        if self.ai_command_tx.is_none() {
            self.ensure_ai_runtime_started(cx);
        }

        self.send_ai_worker_command_if_running(command, cx)
    }

    fn send_ai_worker_command_if_running(
        &mut self,
        command: AiWorkerCommand,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(command_tx) = self.ai_command_tx.as_ref() else {
            return false;
        };

        if command_tx.send(command).is_ok() {
            return true;
        }

        self.ai_connection_state = AiConnectionState::Failed;
        self.ai_error_message = Some("AI worker channel disconnected.".to_string());
        self.ai_command_tx = None;
        cx.notify();
        false
    }

    fn next_ai_event_epoch(&mut self) -> usize {
        self.ai_event_epoch = self.ai_event_epoch.saturating_add(1);
        self.ai_event_epoch
    }

    fn start_ai_event_listener(
        &mut self,
        event_rx: std::sync::mpsc::Receiver<AiWorkerEvent>,
        epoch: usize,
        cx: &mut Context<Self>,
    ) {
        let event_rx = event_rx;
        self.ai_event_task = cx.spawn(async move |this, cx| {
            loop {
                let mut has_events = false;
                loop {
                    match event_rx.try_recv() {
                        Ok(event) => {
                            has_events = true;
                            if let Some(this) = this.upgrade() {
                                this.update(cx, |this, cx| {
                                    if this.ai_event_epoch != epoch {
                                        return;
                                    }
                                    this.apply_ai_worker_event(event, cx);
                                });
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            if let Some(this) = this.upgrade() {
                                this.update(cx, |this, cx| {
                                    if this.ai_event_epoch != epoch {
                                        return;
                                    }
                                    this.ai_command_tx = None;
                                    this.ai_worker_thread = None;
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

                if !has_events {
                    cx.background_executor()
                        .timer(Self::AI_EVENT_POLL_INTERVAL)
                        .await;
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
                self.ai_worker_thread = None;
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

        cx.notify();
    }

    fn apply_ai_snapshot(&mut self, snapshot: AiSnapshot) {
        self.ai_state_snapshot = snapshot.state;
        self.ai_last_command_result = snapshot.last_command_result;
        self.ai_pending_approvals = snapshot.pending_approvals;
        self.ai_pending_user_inputs = snapshot.pending_user_inputs;
        self.sync_ai_pending_user_input_answers();
        self.ai_account = snapshot.account;
        self.ai_requires_openai_auth = snapshot.requires_openai_auth;
        self.ai_pending_chatgpt_login_id = snapshot.pending_chatgpt_login_id;
        self.ai_pending_chatgpt_auth_url = snapshot.pending_chatgpt_auth_url;
        self.ai_rate_limits = snapshot.rate_limits;
        self.ai_models = snapshot.models;
        self.ai_experimental_features = snapshot.experimental_features;
        self.ai_collaboration_modes = snapshot.collaboration_modes;
        self.ai_include_hidden_models = snapshot.include_hidden_models;
        self.ai_mad_max_mode = snapshot.mad_max_mode;

        if let Some(active_thread_id) = snapshot.active_thread_id {
            self.ai_selected_thread_id = Some(active_thread_id);
        }

        if self.ai_selected_thread_id.as_ref().is_some_and(|selected| {
            !self.ai_state_snapshot.threads.contains_key(selected)
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

    fn sync_ai_session_selection_from_state(&mut self) {
        let persisted = self
            .ai_workspace_key()
            .as_ref()
            .and_then(|workspace| {
                self.current_ai_thread_id().and_then(|thread_id| {
                    self.state
                        .ai_thread_session_overrides
                        .get(workspace)
                        .and_then(|threads| threads.get(thread_id.as_str()))
                        .cloned()
                })
            })
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

    fn persist_current_ai_thread_session(&mut self) {
        let Some(workspace) = self.ai_workspace_key() else {
            return;
        };
        let Some(thread_id) = self.current_ai_thread_id() else {
            return;
        };

        let session = AiThreadSessionState {
            model: self.ai_selected_model.clone(),
            effort: self.ai_selected_effort.clone(),
            collaboration_mode: self.ai_selected_collaboration_mode.clone(),
        };

        if let Some(session) = normalized_thread_session_state(session) {
            self.state
                .ai_thread_session_overrides
                .entry(workspace)
                .or_default()
                .insert(thread_id, session);
        } else if let Some(workspace_sessions) =
            self.state.ai_thread_session_overrides.get_mut(workspace.as_str())
        {
            workspace_sessions.remove(thread_id.as_str());
            if workspace_sessions.is_empty() {
                self.state.ai_thread_session_overrides.remove(workspace.as_str());
            }
        }
        self.persist_state();
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

fn sorted_threads(state: &hunk_codex::state::AiState) -> Vec<ThreadSummary> {
    let mut threads = state.threads.values().cloned().collect::<Vec<_>>();
    threads.sort_by(|left, right| {
        right
            .last_sequence
            .cmp(&left.last_sequence)
            .then_with(|| left.id.cmp(&right.id))
    });
    threads
}

fn workspace_mad_max_mode(state: &AppState, workspace_key: Option<&str>) -> bool {
    workspace_key
        .and_then(|workspace| state.ai_workspace_mad_max.get(workspace))
        .copied()
        .unwrap_or(false)
}

fn workspace_include_hidden_models(state: &AppState, workspace_key: Option<&str>) -> bool {
    workspace_key
        .and_then(|workspace| state.ai_workspace_include_hidden_models.get(workspace))
        .copied()
        .unwrap_or(false)
}

fn reasoning_effort_key(effort: &codex_protocol::openai_models::ReasoningEffort) -> String {
    serde_json::to_value(effort)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{effort:?}").to_lowercase())
}

fn resolve_bundled_codex_executable_from_exe(current_exe: &std::path::Path) -> Option<std::path::PathBuf> {
    bundled_codex_executable_candidates(current_exe)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn bundled_codex_executable_candidates(current_exe: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Some(exe_dir) = current_exe.parent() else {
        return Vec::new();
    };

    let binary_name = codex_runtime_binary_name();
    let platform_dir = codex_runtime_platform_dir();
    let mut candidates = vec![
        exe_dir
            .join("codex-runtime")
            .join(platform_dir)
            .join(binary_name),
        exe_dir.join(binary_name),
    ];

    if cfg!(target_os = "macos")
        && let Some(contents_dir) = exe_dir.parent()
    {
        candidates.push(
            contents_dir
                .join("Resources")
                .join("codex-runtime")
                .join(platform_dir)
                .join(binary_name),
        );
    }

    candidates
}

fn codex_runtime_platform_dir() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

fn codex_runtime_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "codex.exe"
    } else {
        "codex"
    }
}

fn is_command_name_without_path(path: &std::path::Path) -> bool {
    if path.is_absolute() {
        return false;
    }
    let text = path.to_string_lossy();
    !text.contains(std::path::MAIN_SEPARATOR) && !text.contains('/')
}

fn normalized_thread_session_state(
    session: AiThreadSessionState,
) -> Option<AiThreadSessionState> {
    let is_empty =
        session.model.is_none() && session.effort.is_none() && session.collaboration_mode.is_none();
    if is_empty {
        return None;
    }
    Some(session)
}

fn normalized_user_input_answers(
    request: &AiPendingUserInputRequest,
    previous: Option<&BTreeMap<String, Vec<String>>>,
) -> BTreeMap<String, Vec<String>> {
    request
        .questions
        .iter()
        .map(|question| {
            let answer = previous
                .and_then(|answers| answers.get(question.id.as_str()))
                .cloned()
                .unwrap_or_else(|| default_user_input_question_answers(question));
            (question.id.clone(), answer)
        })
        .collect::<BTreeMap<_, _>>()
}

fn default_user_input_question_answers(question: &AiPendingUserInputQuestion) -> Vec<String> {
    question
        .options
        .first()
        .map(|option| vec![option.label.clone()])
        .unwrap_or_else(|| vec![String::new()])
}

#[cfg(test)]
fn item_status_chip(status: hunk_codex::state::ItemStatus) -> &'static str {
    match status {
        hunk_codex::state::ItemStatus::Started => "started",
        hunk_codex::state::ItemStatus::Streaming => "streaming",
        hunk_codex::state::ItemStatus::Completed => "completed",
    }
}

#[cfg(test)]
mod ai_tests {
    use super::bundled_codex_executable_candidates;
    use super::codex_runtime_binary_name;
    use super::codex_runtime_platform_dir;
    use super::item_status_chip;
    use super::is_command_name_without_path;
    use super::normalized_thread_session_state;
    use super::normalized_user_input_answers;
    use super::resolve_bundled_codex_executable_from_exe;
    use super::sorted_threads;
    use super::workspace_include_hidden_models;
    use super::workspace_mad_max_mode;
    use crate::app::ai_runtime::AiPendingUserInputQuestion;
    use crate::app::ai_runtime::AiPendingUserInputQuestionOption;
    use crate::app::ai_runtime::AiPendingUserInputRequest;
    use hunk_codex::state::AiState;
    use hunk_codex::state::ItemStatus;
    use hunk_codex::state::ThreadLifecycleStatus;
    use hunk_codex::state::ThreadSummary;
    use hunk_domain::state::AiThreadSessionState;
    use hunk_domain::state::AppState;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn sorted_threads_orders_by_latest_sequence_then_id() {
        let mut state = AiState::default();
        state.threads.insert(
            "t-older".to_string(),
            ThreadSummary {
                id: "t-older".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                last_sequence: 2,
            },
        );
        state.threads.insert(
            "t-newer".to_string(),
            ThreadSummary {
                id: "t-newer".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                last_sequence: 9,
            },
        );

        let sorted = sorted_threads(&state);
        assert_eq!(sorted[0].id, "t-newer");
        assert_eq!(sorted[1].id, "t-older");
    }

    #[test]
    fn item_status_chip_labels_are_stable() {
        assert_eq!(item_status_chip(ItemStatus::Started), "started");
        assert_eq!(item_status_chip(ItemStatus::Streaming), "streaming");
        assert_eq!(item_status_chip(ItemStatus::Completed), "completed");
    }

    #[test]
    fn workspace_mad_max_mode_defaults_to_false_when_missing() {
        let state = AppState::default();
        assert!(!workspace_mad_max_mode(&state, Some("/repo")));
        assert!(!workspace_mad_max_mode(&state, None));
    }

    #[test]
    fn workspace_mad_max_mode_reads_per_workspace_flags() {
        let state = AppState {
            last_project_path: None,
            ai_workspace_mad_max: [
                ("/repo-a".to_string(), true),
                ("/repo-b".to_string(), false),
            ]
            .into_iter()
            .collect(),
            ai_workspace_include_hidden_models: Default::default(),
            ai_thread_session_overrides: Default::default(),
        };
        assert!(workspace_mad_max_mode(&state, Some("/repo-a")));
        assert!(!workspace_mad_max_mode(&state, Some("/repo-b")));
        assert!(!workspace_mad_max_mode(&state, Some("/repo-c")));
    }

    #[test]
    fn workspace_include_hidden_models_defaults_to_false_when_missing() {
        let state = AppState::default();
        assert!(!workspace_include_hidden_models(&state, Some("/repo")));
        assert!(!workspace_include_hidden_models(&state, None));
    }

    #[test]
    fn workspace_include_hidden_models_reads_per_workspace_flags() {
        let state = AppState {
            last_project_path: None,
            ai_workspace_mad_max: Default::default(),
            ai_workspace_include_hidden_models: [
                ("/repo-a".to_string(), true),
                ("/repo-b".to_string(), false),
            ]
            .into_iter()
            .collect(),
            ai_thread_session_overrides: Default::default(),
        };
        assert!(workspace_include_hidden_models(&state, Some("/repo-a")));
        assert!(!workspace_include_hidden_models(&state, Some("/repo-b")));
        assert!(!workspace_include_hidden_models(&state, Some("/repo-c")));
    }

    #[test]
    fn normalized_thread_session_state_drops_empty_entries() {
        assert_eq!(
            normalized_thread_session_state(AiThreadSessionState::default()),
            None
        );
    }

    #[test]
    fn normalized_thread_session_state_preserves_selected_overrides() {
        let session = AiThreadSessionState {
            model: Some("gpt-5-codex".to_string()),
            effort: Some("high".to_string()),
            collaboration_mode: Some("Plan".to_string()),
        };
        assert_eq!(
            normalized_thread_session_state(session.clone()),
            Some(session),
        );
    }

    #[test]
    fn command_name_without_path_detection_is_stable() {
        assert!(is_command_name_without_path(std::path::Path::new("codex")));
        assert!(!is_command_name_without_path(std::path::Path::new("./codex")));
        assert!(!is_command_name_without_path(std::path::Path::new("/usr/bin/codex")));
    }

    #[test]
    fn bundled_codex_resolution_picks_existing_runtime_candidate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-runtime-{unique}"));
        let exe_dir = root.join("bin");
        std::fs::create_dir_all(&exe_dir).expect("exe dir should be created");
        let exe_path = exe_dir.join("hunk");
        std::fs::write(&exe_path, "").expect("fake exe should be written");

        let runtime_path = exe_dir
            .join("codex-runtime")
            .join(codex_runtime_platform_dir())
            .join(codex_runtime_binary_name());
        std::fs::create_dir_all(
            runtime_path
                .parent()
                .expect("runtime parent should exist"),
        )
        .expect("runtime dir should be created");
        std::fs::write(&runtime_path, "").expect("runtime binary should be written");

        let resolved = resolve_bundled_codex_executable_from_exe(exe_path.as_path());
        assert_eq!(resolved, Some(runtime_path));

        let candidates = bundled_codex_executable_candidates(exe_path.as_path());
        assert!(candidates.iter().any(|candidate| candidate.ends_with(PathBuf::from("codex-runtime").join(codex_runtime_platform_dir()).join(codex_runtime_binary_name()))));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn normalized_user_input_answers_defaults_to_first_option_or_blank() {
        let request = AiPendingUserInputRequest {
            request_id: "req-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: "item-1".to_string(),
            questions: vec![
                AiPendingUserInputQuestion {
                    id: "q-option".to_string(),
                    header: "Header".to_string(),
                    question: "Pick one".to_string(),
                    is_other: false,
                    is_secret: false,
                    options: vec![
                        AiPendingUserInputQuestionOption {
                            label: "first".to_string(),
                            description: "first option".to_string(),
                        },
                        AiPendingUserInputQuestionOption {
                            label: "second".to_string(),
                            description: "second option".to_string(),
                        },
                    ],
                },
                AiPendingUserInputQuestion {
                    id: "q-empty".to_string(),
                    header: "Free text".to_string(),
                    question: "Enter value".to_string(),
                    is_other: true,
                    is_secret: false,
                    options: Vec::new(),
                },
            ],
        };

        let answers = normalized_user_input_answers(&request, None);
        assert_eq!(
            answers.get("q-option"),
            Some(&vec!["first".to_string()])
        );
        assert_eq!(answers.get("q-empty"), Some(&vec![String::new()]));
    }

    #[test]
    fn normalized_user_input_answers_preserves_existing_answers() {
        let request = AiPendingUserInputRequest {
            request_id: "req-2".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: "item-2".to_string(),
            questions: vec![AiPendingUserInputQuestion {
                id: "q-option".to_string(),
                header: "Header".to_string(),
                question: "Pick one".to_string(),
                is_other: false,
                is_secret: false,
                options: vec![AiPendingUserInputQuestionOption {
                    label: "default".to_string(),
                    description: "default option".to_string(),
                }],
            }],
        };
        let previous = [("q-option".to_string(), vec!["custom".to_string()])]
            .into_iter()
            .collect();

        let answers = normalized_user_input_answers(&request, Some(&previous));
        assert_eq!(
            answers.get("q-option"),
            Some(&vec!["custom".to_string()])
        );
    }
}
