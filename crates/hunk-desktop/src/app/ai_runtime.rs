use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::CommandExecParams;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ReviewStartParams;
use codex_app_server_protocol::ReviewTarget;
use codex_app_server_protocol::SandboxMode;
use codex_app_server_protocol::SandboxPolicy;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::TurnInterruptParams;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnSteerParams;
use codex_app_server_protocol::UserInput;
use hunk_codex::api::InitializeOptions;
use hunk_codex::errors::CodexIntegrationError;
use hunk_codex::host::HostConfig;
use hunk_codex::host::HostRuntime;
use hunk_codex::state::AiState;
use hunk_codex::state::ServerRequestDecision;
use hunk_codex::state::TurnStatus as StateTurnStatus;
use hunk_codex::threads::ThreadService;
use hunk_codex::ws_client::JsonRpcSession;
use hunk_codex::ws_client::WebSocketEndpoint;

const HOST_START_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const NOTIFICATION_POLL_TIMEOUT: Duration = Duration::from_millis(25);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiConnectionState {
    Disconnected,
    Connecting,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiApprovalDecision {
    Accept,
    Decline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiApprovalKind {
    CommandExecution,
    FileChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPendingApproval {
    pub request_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub kind: AiApprovalKind,
    pub reason: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<PathBuf>,
    pub grant_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct AiSnapshot {
    pub state: AiState,
    pub active_thread_id: Option<String>,
    pub last_command_result: Option<String>,
    pub pending_approvals: Vec<AiPendingApproval>,
    pub mad_max_mode: bool,
}

#[derive(Debug)]
pub enum AiWorkerEvent {
    Snapshot(Box<AiSnapshot>),
    Status(String),
    Error(String),
    Fatal(String),
}

#[derive(Debug)]
pub enum AiWorkerCommand {
    RefreshThreads,
    StartThread {
        prompt: Option<String>,
    },
    SelectThread {
        thread_id: String,
    },
    SendPrompt {
        thread_id: String,
        prompt: String,
    },
    InterruptTurn {
        thread_id: String,
        turn_id: String,
    },
    StartReview {
        thread_id: String,
        instructions: String,
    },
    CommandExec {
        command_line: String,
    },
    ResolveApproval {
        request_id: String,
        decision: AiApprovalDecision,
    },
    SetMadMaxMode {
        enabled: bool,
    },
}

#[derive(Debug, Clone)]
pub struct AiWorkerStartConfig {
    pub cwd: PathBuf,
    pub codex_executable: PathBuf,
    pub codex_home: PathBuf,
    pub request_timeout: Duration,
    pub mad_max_mode: bool,
}

impl AiWorkerStartConfig {
    pub fn new(cwd: PathBuf, codex_executable: PathBuf, codex_home: PathBuf) -> Self {
        Self {
            cwd,
            codex_executable,
            codex_home,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            mad_max_mode: false,
        }
    }
}

pub fn spawn_ai_worker(
    config: AiWorkerStartConfig,
    command_rx: Receiver<AiWorkerCommand>,
    event_tx: Sender<AiWorkerEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        if let Err(error) = run_ai_worker(config, command_rx, &event_tx) {
            let _ = event_tx.send(AiWorkerEvent::Fatal(error.to_string()));
        }
    })
}

struct AiWorkerRuntime {
    host: HostRuntime,
    session: JsonRpcSession,
    service: ThreadService,
    cwd_key: String,
    request_timeout: Duration,
    last_command_result: Option<String>,
    mad_max_mode: bool,
    pending_approvals: BTreeMap<String, PendingApproval>,
    next_approval_sequence: u64,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    request_id: RequestId,
    approval: AiPendingApproval,
    sequence: u64,
}

fn run_ai_worker(
    config: AiWorkerStartConfig,
    command_rx: Receiver<AiWorkerCommand>,
    event_tx: &Sender<AiWorkerEvent>,
) -> Result<(), CodexIntegrationError> {
    let mut runtime = AiWorkerRuntime::bootstrap(config)?;

    let _ = event_tx.send(AiWorkerEvent::Status(
        "Codex App Server connected over WebSocket".to_string(),
    ));
    runtime.refresh_thread_list()?;
    runtime.emit_snapshot_after_sync(event_tx)?;

    loop {
        match command_rx.recv_timeout(POLL_INTERVAL) {
            Ok(command) => {
                if let Err(error) = runtime.handle_command(command, event_tx) {
                    let _ = event_tx.send(AiWorkerEvent::Error(error.to_string()));
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                runtime.poll_notifications(event_tx)?;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = runtime.host.stop();
    Ok(())
}

impl AiWorkerRuntime {
    fn bootstrap(config: AiWorkerStartConfig) -> Result<Self, CodexIntegrationError> {
        std::fs::create_dir_all(&config.codex_home)
            .map_err(CodexIntegrationError::HostProcessIo)?;

        let port = allocate_loopback_port()?;
        let cwd_key = config.cwd.to_string_lossy().to_string();
        let host_config = HostConfig::codex_app_server(
            config.codex_executable,
            config.cwd.clone(),
            config.codex_home,
            port,
        );
        let mut host = HostRuntime::new(host_config);
        host.start(HOST_START_TIMEOUT)?;

        let endpoint = WebSocketEndpoint::loopback(port);
        let mut session = JsonRpcSession::connect(&endpoint)?;
        session.initialize(InitializeOptions::default(), config.request_timeout)?;

        Ok(Self {
            host,
            session,
            service: ThreadService::new(config.cwd),
            cwd_key,
            request_timeout: config.request_timeout,
            last_command_result: None,
            mad_max_mode: config.mad_max_mode,
            pending_approvals: BTreeMap::new(),
            next_approval_sequence: 1,
        })
    }

    fn handle_command(
        &mut self,
        command: AiWorkerCommand,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<(), CodexIntegrationError> {
        match command {
            AiWorkerCommand::RefreshThreads => {
                self.refresh_thread_list()?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::StartThread { prompt } => {
                let mut params = ThreadStartParams::default();
                apply_thread_start_policy(self.mad_max_mode, &mut params);
                let response =
                    self.service
                        .start_thread(&mut self.session, params, self.request_timeout)?;
                self.service
                    .state_mut()
                    .set_active_thread_for_cwd(self.cwd_key.clone(), response.thread.id.clone());
                if let Some(prompt) = prompt {
                    self.send_prompt(response.thread.id, prompt)?;
                }
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::SelectThread { thread_id } => {
                self.service
                    .state_mut()
                    .set_active_thread_for_cwd(self.cwd_key.clone(), thread_id);
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::SendPrompt { thread_id, prompt } => {
                self.send_prompt(thread_id, prompt)?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::InterruptTurn { thread_id, turn_id } => {
                self.service.interrupt_turn(
                    &mut self.session,
                    TurnInterruptParams { thread_id, turn_id },
                    self.request_timeout,
                )?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::StartReview {
                thread_id,
                instructions,
            } => {
                self.service.start_review(
                    &mut self.session,
                    ReviewStartParams {
                        thread_id,
                        target: ReviewTarget::Custom { instructions },
                        delivery: None,
                    },
                    self.request_timeout,
                )?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::CommandExec { command_line } => {
                let command = split_command_line(command_line.as_str());
                if command.is_empty() {
                    let _ =
                        event_tx.send(AiWorkerEvent::Error("Command cannot be empty".to_string()));
                    return Ok(());
                }

                let sandbox_policy = command_exec_sandbox_policy(self.mad_max_mode);
                let response = self.service.command_exec(
                    &mut self.session,
                    CommandExecParams {
                        command,
                        timeout_ms: None,
                        cwd: None,
                        sandbox_policy,
                    },
                    self.request_timeout,
                )?;
                let stderr = response.stderr.trim();
                let stdout = response.stdout.trim();
                self.last_command_result = Some(format!(
                    "exit {}\n{}{}",
                    response.exit_code,
                    stdout,
                    if stderr.is_empty() {
                        "".to_string()
                    } else if stdout.is_empty() {
                        stderr.to_string()
                    } else {
                        format!("\n{stderr}")
                    }
                ));
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::ResolveApproval {
                request_id,
                decision,
            } => {
                self.resolve_pending_approval(request_id.as_str(), decision)?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::SetMadMaxMode { enabled } => {
                self.mad_max_mode = enabled;
                self.emit_snapshot_after_sync(event_tx)?;
            }
        }

        Ok(())
    }

    fn send_prompt(
        &mut self,
        thread_id: String,
        prompt: String,
    ) -> Result<(), CodexIntegrationError> {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        self.service
            .state_mut()
            .set_active_thread_for_cwd(self.cwd_key.clone(), thread_id.clone());

        if let Some(in_progress_turn_id) = self.in_progress_turn_id(thread_id.as_str()) {
            self.service.steer_turn(
                &mut self.session,
                TurnSteerParams {
                    thread_id,
                    input: vec![UserInput::Text {
                        text: trimmed.to_string(),
                        text_elements: Vec::new(),
                    }],
                    expected_turn_id: in_progress_turn_id,
                },
                self.request_timeout,
            )?;
            return Ok(());
        }

        let mut params = TurnStartParams {
            thread_id,
            input: vec![UserInput::Text {
                text: trimmed.to_string(),
                text_elements: Vec::new(),
            }],
            ..TurnStartParams::default()
        };
        apply_turn_start_policy(self.mad_max_mode, &mut params);
        self.service
            .start_turn(&mut self.session, params, self.request_timeout)?;
        Ok(())
    }

    fn in_progress_turn_id(&self, thread_id: &str) -> Option<String> {
        self.service
            .state()
            .turns
            .values()
            .filter(|turn| {
                turn.thread_id == thread_id && turn.status == StateTurnStatus::InProgress
            })
            .max_by_key(|turn| turn.last_sequence)
            .map(|turn| turn.id.clone())
    }

    fn refresh_thread_list(&mut self) -> Result<(), CodexIntegrationError> {
        let response =
            self.service
                .list_threads(&mut self.session, None, Some(200), self.request_timeout)?;

        if self.service.active_thread_for_workspace().is_none()
            && let Some(first_thread) = response.data.first()
        {
            self.service
                .state_mut()
                .set_active_thread_for_cwd(self.cwd_key.clone(), first_thread.id.clone());
        }
        Ok(())
    }

    fn poll_notifications(
        &mut self,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<(), CodexIntegrationError> {
        let captured = self
            .session
            .poll_server_notifications(NOTIFICATION_POLL_TIMEOUT)?;
        if captured > 0 {
            self.service.apply_queued_notifications(&mut self.session);
        }

        let approvals_changed = self.sync_server_requests()?;
        if captured == 0 && !approvals_changed {
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

    fn sync_server_requests(&mut self) -> Result<bool, CodexIntegrationError> {
        let mut changed = false;
        if self.mad_max_mode && !self.pending_approvals.is_empty() {
            let queued = self.pending_approvals.keys().cloned().collect::<Vec<_>>();
            for request_id in queued {
                self.resolve_pending_approval(request_id.as_str(), AiApprovalDecision::Accept)?;
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

                    let sequence = self.request_sequence_for_key(request_id_key.as_str());
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

                    let sequence = self.request_sequence_for_key(request_id_key.as_str());
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

    fn request_sequence_for_key(&mut self, request_id_key: &str) -> u64 {
        if let Some(existing) = self.pending_approvals.get(request_id_key) {
            return existing.sequence;
        }

        let sequence = self.next_approval_sequence;
        self.next_approval_sequence = self.next_approval_sequence.saturating_add(1);
        sequence
    }

    fn emit_snapshot(&self, event_tx: &Sender<AiWorkerEvent>) {
        let pending_approvals = ordered_pending_approvals(&self.pending_approvals);
        let _ = event_tx.send(AiWorkerEvent::Snapshot(Box::new(AiSnapshot {
            state: self.service.state().clone(),
            active_thread_id: self
                .service
                .active_thread_for_workspace()
                .map(ToOwned::to_owned),
            last_command_result: self.last_command_result.clone(),
            pending_approvals,
            mad_max_mode: self.mad_max_mode,
        })));
    }
}

fn split_command_line(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(ToOwned::to_owned).collect()
}

fn map_command_approval_decision(decision: AiApprovalDecision) -> CommandExecutionApprovalDecision {
    match decision {
        AiApprovalDecision::Accept => CommandExecutionApprovalDecision::Accept,
        AiApprovalDecision::Decline => CommandExecutionApprovalDecision::Decline,
    }
}

fn map_file_change_approval_decision(decision: AiApprovalDecision) -> FileChangeApprovalDecision {
    match decision {
        AiApprovalDecision::Accept => FileChangeApprovalDecision::Accept,
        AiApprovalDecision::Decline => FileChangeApprovalDecision::Decline,
    }
}

fn map_server_request_decision(decision: AiApprovalDecision) -> ServerRequestDecision {
    match decision {
        AiApprovalDecision::Accept => ServerRequestDecision::Accept,
        AiApprovalDecision::Decline => ServerRequestDecision::Decline,
    }
}

fn apply_thread_start_policy(mad_max_mode: bool, params: &mut ThreadStartParams) {
    if mad_max_mode {
        params.approval_policy = Some(AskForApproval::Never);
        params.sandbox = Some(SandboxMode::DangerFullAccess);
    } else if params.approval_policy.is_none() {
        params.approval_policy = Some(AskForApproval::OnRequest);
    }
}

fn apply_turn_start_policy(mad_max_mode: bool, params: &mut TurnStartParams) {
    if mad_max_mode {
        params.approval_policy = Some(AskForApproval::Never);
        params.sandbox_policy = Some(SandboxPolicy::DangerFullAccess);
    } else if params.approval_policy.is_none() {
        params.approval_policy = Some(AskForApproval::OnRequest);
    }
}

fn command_exec_sandbox_policy(mad_max_mode: bool) -> Option<SandboxPolicy> {
    if mad_max_mode {
        return Some(SandboxPolicy::DangerFullAccess);
    }
    None
}

fn request_id_key(request_id: &RequestId) -> String {
    match request_id {
        RequestId::String(value) => value.clone(),
        RequestId::Integer(value) => value.to_string(),
    }
}

fn ordered_pending_approvals(
    pending_approvals: &BTreeMap<String, PendingApproval>,
) -> Vec<AiPendingApproval> {
    let mut approvals = pending_approvals.values().cloned().collect::<Vec<_>>();
    approvals.sort_by_key(|pending| pending.sequence);
    approvals
        .into_iter()
        .map(|pending| pending.approval)
        .collect::<Vec<_>>()
}

fn allocate_loopback_port() -> Result<u16, CodexIntegrationError> {
    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(CodexIntegrationError::HostProcessIo)?;
    let port = listener
        .local_addr()
        .map_err(CodexIntegrationError::HostProcessIo)?
        .port();
    drop(listener);
    Ok(port)
}

#[cfg(test)]
mod ai_tests {
    use codex_app_server_protocol::AskForApproval;
    use codex_app_server_protocol::SandboxMode;
    use codex_app_server_protocol::SandboxPolicy;
    use codex_app_server_protocol::ThreadStartParams;
    use codex_app_server_protocol::TurnStartParams;

    use super::AiApprovalDecision;
    use super::apply_thread_start_policy;
    use super::apply_turn_start_policy;
    use super::command_exec_sandbox_policy;
    use super::map_command_approval_decision;
    use super::map_file_change_approval_decision;
    use super::split_command_line;

    #[test]
    fn split_command_line_handles_repeated_whitespace() {
        let command = split_command_line("cargo    test  -p hunk-codex");
        assert_eq!(command, vec!["cargo", "test", "-p", "hunk-codex"]);
    }

    #[test]
    fn thread_policy_defaults_to_on_request_when_not_mad_max() {
        let mut params = ThreadStartParams::default();
        apply_thread_start_policy(false, &mut params);
        assert_eq!(params.approval_policy, Some(AskForApproval::OnRequest));
        assert_eq!(params.sandbox, None);
    }

    #[test]
    fn thread_policy_switches_to_never_and_danger_in_mad_max() {
        let mut params = ThreadStartParams::default();
        apply_thread_start_policy(true, &mut params);
        assert_eq!(params.approval_policy, Some(AskForApproval::Never));
        assert_eq!(params.sandbox, Some(SandboxMode::DangerFullAccess));
    }

    #[test]
    fn turn_policy_switches_to_never_and_danger_in_mad_max() {
        let mut params = TurnStartParams::default();
        apply_turn_start_policy(true, &mut params);
        assert_eq!(params.approval_policy, Some(AskForApproval::Never));
        assert_eq!(params.sandbox_policy, Some(SandboxPolicy::DangerFullAccess));
    }

    #[test]
    fn command_exec_policy_is_dangerous_only_in_mad_max() {
        assert_eq!(command_exec_sandbox_policy(false), None);
        assert_eq!(
            command_exec_sandbox_policy(true),
            Some(SandboxPolicy::DangerFullAccess)
        );
    }

    #[test]
    fn approval_decision_mapping_is_stable() {
        assert_eq!(
            map_command_approval_decision(AiApprovalDecision::Accept),
            codex_app_server_protocol::CommandExecutionApprovalDecision::Accept
        );
        assert_eq!(
            map_file_change_approval_decision(AiApprovalDecision::Decline),
            codex_app_server_protocol::FileChangeApprovalDecision::Decline
        );
    }
}
