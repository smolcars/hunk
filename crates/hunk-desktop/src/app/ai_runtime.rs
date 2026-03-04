use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use codex_app_server_protocol::Account;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::CancelLoginAccountStatus;
use codex_app_server_protocol::CollaborationModeMask;
use codex_app_server_protocol::CommandExecParams;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::DynamicToolCallOutputContentItem;
use codex_app_server_protocol::ExperimentalFeature;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::LoginAccountParams;
use codex_app_server_protocol::LoginAccountResponse;
use codex_app_server_protocol::Model;
use codex_app_server_protocol::RateLimitSnapshot;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ReviewStartParams;
use codex_app_server_protocol::ReviewTarget;
use codex_app_server_protocol::SandboxMode;
use codex_app_server_protocol::SandboxPolicy;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ToolRequestUserInputAnswer;
use codex_app_server_protocol::ToolRequestUserInputQuestion;
use codex_app_server_protocol::ToolRequestUserInputResponse;
use codex_app_server_protocol::TurnInterruptParams;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnSteerParams;
use codex_app_server_protocol::UserInput;
use codex_protocol::config_types::CollaborationMode;
use codex_protocol::config_types::ModeKind;
use codex_protocol::config_types::Settings;
use codex_protocol::openai_models::ReasoningEffort;
use hunk_codex::api::InitializeOptions;
use hunk_codex::errors::CodexIntegrationError;
use hunk_codex::host::HostConfig;
use hunk_codex::host::HostRuntime;
use hunk_codex::state::AiState;
use hunk_codex::state::ServerRequestDecision;
use hunk_codex::state::TurnStatus as StateTurnStatus;
use hunk_codex::threads::RolloutFallbackItem;
use hunk_codex::threads::RolloutFallbackTurn;
use hunk_codex::threads::ThreadService;
use hunk_codex::tools::DynamicToolRegistry;
use hunk_codex::ws_client::JsonRpcSession;
use hunk_codex::ws_client::WebSocketEndpoint;

use crate::app::ai_rollout_fallback::find_rollout_path_for_thread;
use crate::app::ai_rollout_fallback::parse_rollout_fallback;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPendingUserInputQuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPendingUserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub is_other: bool,
    pub is_secret: bool,
    pub options: Vec<AiPendingUserInputQuestionOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPendingUserInputRequest {
    pub request_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub questions: Vec<AiPendingUserInputQuestion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AiTurnSessionOverrides {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub collaboration_mode: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AiSnapshot {
    pub state: AiState,
    pub active_thread_id: Option<String>,
    pub last_command_result: Option<String>,
    pub pending_approvals: Vec<AiPendingApproval>,
    pub pending_user_inputs: Vec<AiPendingUserInputRequest>,
    pub account: Option<Account>,
    pub requires_openai_auth: bool,
    pub pending_chatgpt_login_id: Option<String>,
    pub pending_chatgpt_auth_url: Option<String>,
    pub rate_limits: Option<RateLimitSnapshot>,
    pub models: Vec<Model>,
    pub experimental_features: Vec<ExperimentalFeature>,
    pub collaboration_modes: Vec<CollaborationModeMask>,
    pub include_hidden_models: bool,
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
    RefreshAccount,
    RefreshRateLimits,
    RefreshSessionMetadata,
    SetIncludeHiddenModels {
        enabled: bool,
    },
    StartThread {
        prompt: Option<String>,
        session_overrides: AiTurnSessionOverrides,
    },
    SelectThread {
        thread_id: String,
    },
    SendPrompt {
        thread_id: String,
        prompt: String,
        session_overrides: AiTurnSessionOverrides,
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
    SubmitUserInput {
        request_id: String,
        answers: BTreeMap<String, Vec<String>>,
    },
    SetMadMaxMode {
        enabled: bool,
    },
    StartChatgptLogin,
    CancelChatgptLogin,
    LogoutAccount,
}

#[derive(Debug, Clone)]
pub struct AiWorkerStartConfig {
    pub cwd: PathBuf,
    pub codex_executable: PathBuf,
    pub codex_home: PathBuf,
    pub request_timeout: Duration,
    pub mad_max_mode: bool,
    pub include_hidden_models: bool,
}

impl AiWorkerStartConfig {
    pub fn new(cwd: PathBuf, codex_executable: PathBuf, codex_home: PathBuf) -> Self {
        Self {
            cwd,
            codex_executable,
            codex_home,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            mad_max_mode: false,
            include_hidden_models: false,
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
    codex_home: PathBuf,
    cwd_key: String,
    request_timeout: Duration,
    last_command_result: Option<String>,
    mad_max_mode: bool,
    account: Option<Account>,
    requires_openai_auth: bool,
    pending_chatgpt_login_id: Option<String>,
    pending_chatgpt_auth_url: Option<String>,
    rate_limits: Option<RateLimitSnapshot>,
    models: Vec<Model>,
    experimental_features: Vec<ExperimentalFeature>,
    collaboration_modes: Vec<CollaborationModeMask>,
    include_hidden_models: bool,
    tool_registry: DynamicToolRegistry,
    pending_approvals: BTreeMap<String, PendingApproval>,
    pending_user_inputs: BTreeMap<String, PendingUserInput>,
    next_approval_sequence: u64,
    next_user_input_sequence: u64,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    request_id: RequestId,
    approval: AiPendingApproval,
    sequence: u64,
}

#[derive(Debug, Clone)]
struct PendingUserInput {
    request_id: RequestId,
    request: AiPendingUserInputRequest,
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
    if let Err(error) = runtime.refresh_session_metadata() {
        let _ = event_tx.send(AiWorkerEvent::Status(format!(
            "Unable to read model/session metadata: {error}"
        )));
    }
    if let Err(error) = runtime.refresh_account_state() {
        let _ = event_tx.send(AiWorkerEvent::Status(format!(
            "Unable to read account state: {error}"
        )));
    }
    if let Err(error) = runtime.refresh_account_rate_limits() {
        let _ = event_tx.send(AiWorkerEvent::Status(format!(
            "Unable to read account rate limits: {error}"
        )));
    }
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
            config.codex_home.clone(),
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
            codex_home: config.codex_home,
            cwd_key,
            request_timeout: config.request_timeout,
            last_command_result: None,
            mad_max_mode: config.mad_max_mode,
            account: None,
            requires_openai_auth: false,
            pending_chatgpt_login_id: None,
            pending_chatgpt_auth_url: None,
            rate_limits: None,
            models: Vec::new(),
            experimental_features: Vec::new(),
            collaboration_modes: Vec::new(),
            include_hidden_models: config.include_hidden_models,
            tool_registry: DynamicToolRegistry::new(),
            pending_approvals: BTreeMap::new(),
            pending_user_inputs: BTreeMap::new(),
            next_approval_sequence: 1,
            next_user_input_sequence: 1,
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
            AiWorkerCommand::RefreshAccount => {
                self.refresh_account_state()?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::RefreshRateLimits => {
                self.refresh_account_rate_limits()?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::RefreshSessionMetadata => {
                self.refresh_session_metadata()?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::SetIncludeHiddenModels { enabled } => {
                self.include_hidden_models = enabled;
                self.refresh_models()?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::StartThread {
                prompt,
                session_overrides,
            } => {
                let mut params = ThreadStartParams {
                    persist_extended_history: true,
                    ..ThreadStartParams::default()
                };
                apply_thread_start_policy(self.mad_max_mode, &mut params);
                apply_thread_start_session_overrides(&session_overrides, &mut params);
                let response =
                    self.service
                        .start_thread(&mut self.session, params, self.request_timeout)?;
                self.service
                    .state_mut()
                    .set_active_thread_for_cwd(self.cwd_key.clone(), response.thread.id.clone());
                if let Some(prompt) = prompt {
                    self.send_prompt(response.thread.id, prompt, session_overrides)?;
                }
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::SelectThread { thread_id } => {
                let selected_thread_id = thread_id.clone();
                self.service.resume_thread(
                    &mut self.session,
                    ThreadResumeParams {
                        thread_id,
                        persist_extended_history: true,
                        ..ThreadResumeParams::default()
                    },
                    self.request_timeout,
                )?;
                self.service.read_thread(
                    &mut self.session,
                    selected_thread_id.clone(),
                    true,
                    self.request_timeout,
                )?;
                self.hydrate_thread_from_rollout_fallback_if_needed(selected_thread_id.as_str());
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::SendPrompt {
                thread_id,
                prompt,
                session_overrides,
            } => {
                self.send_prompt(thread_id, prompt, session_overrides)?;
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
            AiWorkerCommand::SubmitUserInput {
                request_id,
                answers,
            } => {
                self.submit_pending_user_input(request_id.as_str(), answers)?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::SetMadMaxMode { enabled } => {
                self.mad_max_mode = enabled;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::StartChatgptLogin => {
                let response = self.service.login_account(
                    &mut self.session,
                    LoginAccountParams::Chatgpt,
                    self.request_timeout,
                )?;
                match response {
                    LoginAccountResponse::Chatgpt { login_id, auth_url } => {
                        self.pending_chatgpt_login_id = Some(login_id.clone());
                        self.pending_chatgpt_auth_url = Some(auth_url.clone());
                        match open_url_in_system_browser(auth_url.as_str()) {
                            Ok(()) => {
                                let _ = event_tx.send(AiWorkerEvent::Status(
                                    "Opened browser for ChatGPT login.".to_string(),
                                ));
                            }
                            Err(_) => {
                                let _ = event_tx.send(AiWorkerEvent::Status(format!(
                                    "Open this URL to continue ChatGPT login: {auth_url}"
                                )));
                            }
                        }
                    }
                    LoginAccountResponse::ApiKey { .. } => {
                        let _ = event_tx.send(AiWorkerEvent::Status(
                            "Server returned API-key login mode; expected ChatGPT login."
                                .to_string(),
                        ));
                    }
                    LoginAccountResponse::ChatgptAuthTokens { .. } => {
                        let _ = event_tx.send(AiWorkerEvent::Status(
                            "Server returned external auth token mode.".to_string(),
                        ));
                    }
                }
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::CancelChatgptLogin => {
                if let Some(login_id) = self.pending_chatgpt_login_id.clone() {
                    let result = self.service.cancel_account_login(
                        &mut self.session,
                        login_id.clone(),
                        self.request_timeout,
                    )?;
                    self.pending_chatgpt_login_id = None;
                    self.pending_chatgpt_auth_url = None;
                    let message = match result.status {
                        CancelLoginAccountStatus::Canceled => {
                            format!("Canceled ChatGPT login attempt {login_id}.")
                        }
                        CancelLoginAccountStatus::NotFound => {
                            "No active ChatGPT login attempt to cancel.".to_string()
                        }
                    };
                    let _ = event_tx.send(AiWorkerEvent::Status(message));
                } else {
                    let _ = event_tx.send(AiWorkerEvent::Status(
                        "No active ChatGPT login attempt.".to_string(),
                    ));
                }
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::LogoutAccount => {
                self.service
                    .logout_account(&mut self.session, self.request_timeout)?;
                self.pending_chatgpt_login_id = None;
                self.pending_chatgpt_auth_url = None;
                self.account = None;
                self.rate_limits = None;
                self.refresh_account_state()?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
        }

        Ok(())
    }

    fn send_prompt(
        &mut self,
        thread_id: String,
        prompt: String,
        session_overrides: AiTurnSessionOverrides,
    ) -> Result<(), CodexIntegrationError> {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        self.service
            .state_mut()
            .set_active_thread_for_cwd(self.cwd_key.clone(), thread_id.clone());

        let input = vec![UserInput::Text {
            text: trimmed.to_string(),
            text_elements: Vec::new(),
        }];

        if let Some(in_progress_turn_id) = self.in_progress_turn_id(thread_id.as_str()) {
            let steer_result = self.service.steer_turn(
                &mut self.session,
                TurnSteerParams {
                    thread_id: thread_id.clone(),
                    input: input.clone(),
                    expected_turn_id: in_progress_turn_id,
                },
                self.request_timeout,
            );

            match steer_result {
                Ok(_) => return Ok(()),
                Err(error) if should_retry_stale_turn_after_steer_error(&error) => {
                    self.service.read_thread(
                        &mut self.session,
                        thread_id.clone(),
                        true,
                        self.request_timeout,
                    )?;
                    if let Some(refreshed_turn_id) = self.in_progress_turn_id(thread_id.as_str()) {
                        self.service.steer_turn(
                            &mut self.session,
                            TurnSteerParams {
                                thread_id: thread_id.clone(),
                                input: input.clone(),
                                expected_turn_id: refreshed_turn_id,
                            },
                            self.request_timeout,
                        )?;
                        return Ok(());
                    }
                }
                Err(error) => return Err(error),
            }
        }

        let mut params = TurnStartParams {
            thread_id,
            input,
            ..TurnStartParams::default()
        };
        apply_turn_start_policy(self.mad_max_mode, &mut params);
        self.apply_turn_session_overrides(&mut params, &session_overrides);
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

    fn hydrate_thread_from_rollout_fallback_if_needed(&mut self, thread_id: &str) {
        let missing_turn_ids = thread_missing_item_turn_ids(self.service.state(), thread_id);
        if missing_turn_ids.is_empty() {
            return;
        }

        let mut rollout_path =
            match find_rollout_path_for_thread(self.codex_home.as_path(), thread_id) {
                Ok(Some(path)) => Some(path),
                Ok(None) => None,
                Err(_) => None,
            };
        if rollout_path.is_none()
            && let Some(home) = std::env::var_os("HOME")
        {
            let home_codex = PathBuf::from(home).join(".codex");
            if home_codex != self.codex_home {
                rollout_path = match find_rollout_path_for_thread(home_codex.as_path(), thread_id) {
                    Ok(Some(path)) => Some(path),
                    Ok(None) => None,
                    Err(_) => None,
                };
            }
        }
        let Some(rollout_path) = rollout_path else {
            return;
        };
        let parsed_turns = match parse_rollout_fallback(rollout_path.as_path()) {
            Ok(turns) => turns,
            Err(_) => return,
        };
        if parsed_turns.is_empty() {
            return;
        }

        let fallback_turns = parsed_turns
            .into_iter()
            .filter(|turn| {
                missing_turn_ids.contains(turn.turn_id.as_str()) && !turn.items.is_empty()
            })
            .map(|turn| RolloutFallbackTurn {
                turn_id: turn.turn_id,
                completed: turn.completed,
                items: turn
                    .items
                    .into_iter()
                    .map(|item| RolloutFallbackItem {
                        kind: item.kind,
                        content: item.content,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        if fallback_turns.is_empty() {
            return;
        }

        self.service
            .ingest_rollout_fallback_history(thread_id.to_string(), fallback_turns.as_slice());
    }

    fn apply_turn_session_overrides(
        &self,
        params: &mut TurnStartParams,
        session_overrides: &AiTurnSessionOverrides,
    ) {
        params.model = session_overrides.model.clone();
        params.effort = session_overrides
            .effort
            .as_deref()
            .and_then(parse_reasoning_effort);

        let Some(mode_name) = session_overrides.collaboration_mode.as_ref() else {
            return;
        };
        let Some(mode_mask) = self
            .collaboration_modes
            .iter()
            .find(|mask| &mask.name == mode_name)
        else {
            return;
        };

        let model = session_overrides
            .model
            .clone()
            .or_else(|| mode_mask.model.clone())
            .or_else(|| self.default_model_id());
        let Some(model) = model else {
            return;
        };

        let effort = session_overrides
            .effort
            .as_deref()
            .and_then(parse_reasoning_effort)
            .or_else(|| mode_mask.reasoning_effort.unwrap_or(None));

        let collaboration_mode = CollaborationMode {
            mode: mode_mask.mode.unwrap_or(ModeKind::Default),
            settings: Settings {
                model,
                reasoning_effort: effort,
                developer_instructions: None,
            },
        };
        params.collaboration_mode = Some(collaboration_mode);
        // Collaboration mode takes precedence over model/effort in the server.
        params.model = None;
        params.effort = None;
    }

    fn default_model_id(&self) -> Option<String> {
        self.models
            .iter()
            .find(|model| model.is_default)
            .or_else(|| self.models.first())
            .map(|model| model.id.clone())
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

    fn refresh_session_metadata(&mut self) -> Result<(), CodexIntegrationError> {
        self.refresh_models()?;
        self.refresh_experimental_features()?;
        self.refresh_collaboration_modes()?;
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
                self.rate_limits = Some(response.rate_limits);
                Ok(())
            }
            Err(CodexIntegrationError::JsonRpcServerError { .. }) => {
                self.rate_limits = None;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn poll_notifications(
        &mut self,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<(), CodexIntegrationError> {
        let captured = self
            .session
            .poll_server_notifications(NOTIFICATION_POLL_TIMEOUT)?;
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
                    self.rate_limits = Some(update.rate_limits.clone());
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
                    let _ = event_tx.send(AiWorkerEvent::Status(message));
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
                    let status = if response.success {
                        format!("Tool '{}' completed.", params.tool)
                    } else {
                        let failure_reason = response
                            .content_items
                            .iter()
                            .find_map(|content| match content {
                                DynamicToolCallOutputContentItem::InputText { text } => {
                                    Some(text.clone())
                                }
                                DynamicToolCallOutputContentItem::InputImage { .. } => None,
                            })
                            .unwrap_or_else(|| "tool execution failed".to_string());
                        format!("Tool '{}' failed: {failure_reason}", params.tool)
                    };
                    self.last_command_result = Some(status);
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
        let _ = event_tx.send(AiWorkerEvent::Snapshot(Box::new(AiSnapshot {
            state: self.service.state().clone(),
            active_thread_id: self
                .service
                .active_thread_for_workspace()
                .map(ToOwned::to_owned),
            last_command_result: self.last_command_result.clone(),
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
        })));
    }
}

fn split_command_line(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(ToOwned::to_owned).collect()
}

fn thread_missing_item_turn_ids(state: &AiState, thread_id: &str) -> BTreeSet<String> {
    let turn_ids = state
        .turns
        .values()
        .filter(|turn| turn.thread_id == thread_id)
        .map(|turn| turn.id.clone())
        .collect::<BTreeSet<_>>();
    if turn_ids.is_empty() {
        return BTreeSet::new();
    }

    let turns_with_items = state
        .items
        .values()
        .filter(|item| turn_ids.contains(item.turn_id.as_str()))
        .map(|item| item.turn_id.clone())
        .collect::<BTreeSet<_>>();
    turn_ids
        .into_iter()
        .filter(|turn_id| !turns_with_items.contains(turn_id))
        .collect()
}

fn should_retry_stale_turn_after_steer_error(error: &CodexIntegrationError) -> bool {
    matches!(error, CodexIntegrationError::JsonRpcServerError { .. })
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

fn apply_thread_start_session_overrides(
    session_overrides: &AiTurnSessionOverrides,
    params: &mut ThreadStartParams,
) {
    params.model = session_overrides.model.clone();
}

fn command_exec_sandbox_policy(mad_max_mode: bool) -> Option<SandboxPolicy> {
    if mad_max_mode {
        return Some(SandboxPolicy::DangerFullAccess);
    }
    None
}

fn parse_reasoning_effort(raw: &str) -> Option<ReasoningEffort> {
    serde_json::from_value::<ReasoningEffort>(serde_json::Value::String(raw.to_string())).ok()
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

fn ordered_pending_user_inputs(
    pending_user_inputs: &BTreeMap<String, PendingUserInput>,
) -> Vec<AiPendingUserInputRequest> {
    let mut requests = pending_user_inputs.values().cloned().collect::<Vec<_>>();
    requests.sort_by_key(|pending| pending.sequence);
    requests
        .into_iter()
        .map(|pending| pending.request)
        .collect::<Vec<_>>()
}

fn map_pending_user_input_question(
    question: ToolRequestUserInputQuestion,
) -> AiPendingUserInputQuestion {
    AiPendingUserInputQuestion {
        id: question.id,
        header: question.header,
        question: question.question,
        is_other: question.is_other,
        is_secret: question.is_secret,
        options: question
            .options
            .unwrap_or_default()
            .into_iter()
            .map(|option| AiPendingUserInputQuestionOption {
                label: option.label,
                description: option.description,
            })
            .collect::<Vec<_>>(),
    }
}

fn default_user_input_answers(
    questions: &[AiPendingUserInputQuestion],
) -> BTreeMap<String, Vec<String>> {
    questions
        .iter()
        .map(|question| {
            let answer = question
                .options
                .first()
                .map(|option| option.label.clone())
                .unwrap_or_default();
            (question.id.clone(), vec![answer])
        })
        .collect::<BTreeMap<_, _>>()
}

fn map_user_input_answers(
    answers: BTreeMap<String, Vec<String>>,
) -> HashMap<String, ToolRequestUserInputAnswer> {
    answers
        .into_iter()
        .map(|(question_id, answers)| (question_id, ToolRequestUserInputAnswer { answers }))
        .collect::<HashMap<_, _>>()
}

fn apply_login_completed_state(
    pending_chatgpt_login_id: &mut Option<String>,
    pending_chatgpt_auth_url: &mut Option<String>,
    completed: &codex_app_server_protocol::AccountLoginCompletedNotification,
) -> String {
    *pending_chatgpt_login_id = None;
    *pending_chatgpt_auth_url = None;
    if completed.success {
        return "ChatGPT login completed.".to_string();
    }

    completed
        .error
        .clone()
        .map(|error| format!("ChatGPT login failed: {error}"))
        .unwrap_or_else(|| "ChatGPT login failed.".to_string())
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

fn open_url_in_system_browser(url: &str) -> Result<(), CodexIntegrationError> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        command.arg("/C").arg("start").arg("").arg(url);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    let status = command
        .status()
        .map_err(CodexIntegrationError::HostProcessIo)?;
    if status.success() {
        return Ok(());
    }

    Err(CodexIntegrationError::HostProcessIo(io::Error::other(
        format!("failed to open browser for URL '{url}'"),
    )))
}

#[cfg(test)]
mod ai_tests {
    use codex_app_server_protocol::AccountLoginCompletedNotification;
    use codex_app_server_protocol::AskForApproval;
    use codex_app_server_protocol::SandboxMode;
    use codex_app_server_protocol::SandboxPolicy;
    use codex_app_server_protocol::ThreadStartParams;
    use codex_app_server_protocol::TurnStartParams;
    use hunk_codex::errors::CodexIntegrationError;
    use hunk_codex::state::AiState;
    use hunk_codex::state::ReducerEvent;
    use hunk_codex::state::StreamEvent;

    use super::AiApprovalDecision;
    use super::apply_login_completed_state;
    use super::apply_thread_start_policy;
    use super::apply_turn_start_policy;
    use super::command_exec_sandbox_policy;
    use super::map_command_approval_decision;
    use super::map_file_change_approval_decision;
    use super::should_retry_stale_turn_after_steer_error;
    use super::split_command_line;
    use super::thread_missing_item_turn_ids;

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

    #[test]
    fn login_completion_clears_pending_state_on_success() {
        let mut pending_login_id = Some("login-1".to_string());
        let mut pending_auth_url = Some("https://auth.example/login".to_string());
        let message = apply_login_completed_state(
            &mut pending_login_id,
            &mut pending_auth_url,
            &AccountLoginCompletedNotification {
                login_id: Some("login-1".to_string()),
                success: true,
                error: None,
            },
        );

        assert_eq!(message, "ChatGPT login completed.");
        assert_eq!(pending_login_id, None);
        assert_eq!(pending_auth_url, None);
    }

    #[test]
    fn login_completion_failure_prefers_server_error_message() {
        let mut pending_login_id = Some("login-2".to_string());
        let mut pending_auth_url = Some("https://auth.example/login".to_string());
        let message = apply_login_completed_state(
            &mut pending_login_id,
            &mut pending_auth_url,
            &AccountLoginCompletedNotification {
                login_id: Some("login-2".to_string()),
                success: false,
                error: Some("token expired".to_string()),
            },
        );

        assert_eq!(message, "ChatGPT login failed: token expired");
        assert_eq!(pending_login_id, None);
        assert_eq!(pending_auth_url, None);
    }

    #[test]
    fn stale_turn_retry_helper_matches_jsonrpc_server_errors_only() {
        assert!(should_retry_stale_turn_after_steer_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32602,
                message: "invalid turn".to_string(),
            }
        ));
        assert!(!should_retry_stale_turn_after_steer_error(
            &CodexIntegrationError::WebSocketTransport("closed".to_string())
        ));
    }

    #[test]
    fn thread_rollout_fallback_is_needed_when_turns_exist_without_items() {
        let mut state = AiState::default();
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 1,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
        });
        let missing_turns = thread_missing_item_turn_ids(&state, "thread-1");
        assert_eq!(
            missing_turns.into_iter().collect::<Vec<_>>(),
            vec!["turn-1".to_string()]
        );
    }

    #[test]
    fn thread_rollout_fallback_is_not_needed_when_items_exist() {
        let mut state = AiState::default();
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 1,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 2,
            dedupe_key: None,
            payload: ReducerEvent::ItemStarted {
                turn_id: "turn-1".to_string(),
                item_id: "item-1".to_string(),
                kind: "agentMessage".to_string(),
            },
        });
        assert!(thread_missing_item_turn_ids(&state, "thread-1").is_empty());
    }

    #[test]
    fn thread_rollout_fallback_targets_only_turns_missing_items() {
        let mut state = AiState::default();
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 1,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 2,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-2".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 3,
            dedupe_key: None,
            payload: ReducerEvent::ItemStarted {
                turn_id: "turn-2".to_string(),
                item_id: "item-2".to_string(),
                kind: "agentMessage".to_string(),
            },
        });

        let missing_turns = thread_missing_item_turn_ids(&state, "thread-1");
        assert_eq!(
            missing_turns.into_iter().collect::<Vec<_>>(),
            vec!["turn-1".to_string()]
        );
    }
}
