use std::time::Duration;

use hunk_codex::state::ThreadSummary;
use hunk_codex::state::TurnStatus;
use hunk_domain::state::AppState;

impl DiffViewer {
    const AI_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(80);

    pub(super) fn ensure_ai_runtime_started(&mut self, cx: &mut Context<Self>) {
        if self.ai_command_tx.is_some() {
            return;
        }

        self.sync_ai_workspace_mad_max_from_state();

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
        let (command_tx, command_rx) = std::sync::mpsc::channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let mut start_config = AiWorkerStartConfig::new(cwd, codex_executable, codex_home);
        start_config.mad_max_mode = self.ai_mad_max_mode;

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

    pub(super) fn ai_create_thread_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = self.ai_composer_input_state.read(cx).value().trim().to_string();
        let prompt = (!prompt.is_empty()).then_some(prompt);

        if self.send_ai_worker_command(AiWorkerCommand::StartThread { prompt }, cx) {
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

        let sent = if let Some(thread_id) = self.current_ai_thread_id() {
            self.send_ai_worker_command(AiWorkerCommand::SendPrompt { thread_id, prompt }, cx)
        } else {
            self.send_ai_worker_command(
                AiWorkerCommand::StartThread {
                    prompt: Some(prompt),
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
        let previous = self.ai_mad_max_mode;
        self.sync_ai_workspace_mad_max_from_state();
        if previous != self.ai_mad_max_mode {
            self.send_ai_worker_command_if_running(
                AiWorkerCommand::SetMadMaxMode {
                    enabled: self.ai_mad_max_mode,
                },
                cx,
            );
            cx.notify();
        }
    }

    fn sync_ai_workspace_mad_max_from_state(&mut self) {
        self.ai_mad_max_mode = workspace_mad_max_mode(&self.state, self.ai_workspace_key().as_deref());
    }

    fn resolve_codex_executable_path() -> std::path::PathBuf {
        std::env::var_os("HUNK_CODEX_EXECUTABLE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("codex"))
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
                Self::push_error_notification(format!("Codex AI failed: {message}"), cx);
            }
        }

        cx.notify();
    }

    fn apply_ai_snapshot(&mut self, snapshot: AiSnapshot) {
        self.ai_state_snapshot = snapshot.state;
        self.ai_last_command_result = snapshot.last_command_result;
        self.ai_pending_approvals = snapshot.pending_approvals;
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
    use super::item_status_chip;
    use super::sorted_threads;
    use super::workspace_mad_max_mode;
    use hunk_codex::state::AiState;
    use hunk_codex::state::ItemStatus;
    use hunk_codex::state::ThreadLifecycleStatus;
    use hunk_codex::state::ThreadSummary;
    use hunk_domain::state::AppState;

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
        };
        assert!(workspace_mad_max_mode(&state, Some("/repo-a")));
        assert!(!workspace_mad_max_mode(&state, Some("/repo-b")));
        assert!(!workspace_mad_max_mode(&state, Some("/repo-c")));
    }
}
