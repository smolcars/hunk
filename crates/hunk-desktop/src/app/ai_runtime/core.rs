use std::any::Any;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU16;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use codex_app_server_protocol::Account;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::CancelLoginAccountStatus;
use codex_app_server_protocol::CollaborationModeMask;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::ExperimentalFeature;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_app_server_protocol::LoginAccountParams;
use codex_app_server_protocol::LoginAccountResponse;
use codex_app_server_protocol::Model;
use codex_app_server_protocol::RateLimitSnapshot;
use codex_app_server_protocol::ReadOnlyAccess;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ReviewStartParams;
use codex_app_server_protocol::SkillMetadata;
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
use codex_protocol::config_types::ServiceTier;
use codex_protocol::openai_models::ReasoningEffort;
use hunk_domain::state::AiCollaborationModeSelection;
use hunk_domain::state::AiServiceTierSelection;
use hunk_codex::api::InitializeOptions;
use hunk_codex::errors::CodexIntegrationError;
use hunk_codex::host::HostConfig;
use hunk_codex::host::SharedHostLease;
use hunk_codex::state::AiState;
use hunk_codex::state::ServerRequestDecision;
use hunk_codex::state::ThreadLifecycleStatus;
use hunk_codex::state::TurnStatus as StateTurnStatus;
use hunk_codex::threads::RolloutFallbackItem;
use hunk_codex::threads::RolloutFallbackTurn;
use hunk_codex::threads::ThreadService;
use hunk_codex::tools::DynamicToolRegistry;
use hunk_codex::ws_client::JsonRpcSession;
use hunk_codex::ws_client::WebSocketEndpoint;

use crate::app::ai_paths::default_codex_home_path;
use crate::app::ai_rollout_fallback::find_rollout_path_for_thread;
use crate::app::ai_rollout_fallback::parse_rollout_fallback;
use crate::app::AiComposerSkillBinding;
use crate::app::AiPromptSkillReference;

const HOST_START_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(20);
const NOTIFICATION_POLL_TIMEOUT: Duration = Duration::from_millis(20);
const NOTIFICATION_DRAIN_TIMEOUT: Duration = Duration::from_millis(2);
const MAX_NOTIFICATIONS_PER_POLL: usize = 256;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const HOST_BOOTSTRAP_MAX_ATTEMPTS: usize = 12;
const TRANSIENT_ROLLOUT_LOAD_MAX_RETRIES: usize = 3;
const TRANSIENT_ROLLOUT_LOAD_RETRY_DELAY: Duration = Duration::from_millis(75);
const LOOPBACK_PORT_RANGE_START: u16 = 49_152;
const LOOPBACK_PORT_RANGE_SIZE: u16 = 16_384;
static NEXT_LOOPBACK_PORT_OFFSET: AtomicU16 = AtomicU16::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiConnectionState {
    Disconnected,
    Connecting,
    Reconnecting,
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
    pub collaboration_mode: AiCollaborationModeSelection,
    pub service_tier: AiServiceTierSelection,
}

#[derive(Debug, Clone)]
pub struct AiSnapshot {
    pub state: AiState,
    pub active_thread_id: Option<String>,
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
    pub skills: Vec<SkillMetadata>,
    pub include_hidden_models: bool,
    pub mad_max_mode: bool,
}

#[derive(Debug, Clone)]
pub enum AiWorkerEventPayload {
    Snapshot(Box<AiSnapshot>),
    BootstrapCompleted,
    ThreadStarted { thread_id: String },
    SteerAccepted(AiPendingSteer),
    Reconnecting(String),
    Status(String),
    Error(String),
    Fatal(String),
}

#[derive(Debug, Clone)]
pub struct AiWorkerEvent {
    pub workspace_key: String,
    pub payload: AiWorkerEventPayload,
}

impl AiWorkerEvent {
    fn new(workspace_key: String, payload: AiWorkerEventPayload) -> Self {
        Self {
            workspace_key,
            payload,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AiWorkerCommand {
    RefreshThreads,
    RefreshThreadMetadata {
        thread_id: String,
    },
    SetIncludeHiddenModels {
        enabled: bool,
    },
    StartThread {
        prompt: Option<String>,
        local_image_paths: Vec<PathBuf>,
        selected_skills: Vec<AiPromptSkillReference>,
        skill_bindings: Vec<AiComposerSkillBinding>,
        session_overrides: AiTurnSessionOverrides,
    },
    SelectThread {
        thread_id: String,
    },
    ArchiveThread {
        thread_id: String,
    },
    SendPrompt {
        thread_id: String,
        prompt: Option<String>,
        local_image_paths: Vec<PathBuf>,
        selected_skills: Vec<AiPromptSkillReference>,
        skill_bindings: Vec<AiComposerSkillBinding>,
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
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct AiWorkerStartConfig {
    pub cwd: PathBuf,
    pub host_working_directory: PathBuf,
    pub workspace_key: String,
    pub codex_executable: PathBuf,
    pub codex_home: PathBuf,
    pub request_timeout: Duration,
    pub mad_max_mode: bool,
    pub include_hidden_models: bool,
}

impl AiWorkerStartConfig {
    pub fn new(cwd: PathBuf, codex_executable: PathBuf, codex_home: PathBuf) -> Self {
        let workspace_key = cwd.to_string_lossy().to_string();
        let host_working_directory = shared_ai_host_working_directory(cwd.as_path());
        Self {
            cwd,
            host_working_directory,
            workspace_key,
            codex_executable,
            codex_home,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            mad_max_mode: false,
            include_hidden_models: true,
        }
    }
}

fn shared_ai_host_working_directory(workspace_root: &Path) -> PathBuf {
    hunk_git::worktree::primary_repo_root(workspace_root)
        .unwrap_or_else(|_| workspace_root.to_path_buf())
}

pub fn spawn_ai_worker(
    config: AiWorkerStartConfig,
    command_rx: Receiver<AiWorkerCommand>,
    event_tx: Sender<AiWorkerEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let workspace_key = config.workspace_key.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_ai_worker(config, command_rx, &event_tx)
        }));
        dispatch_ai_worker_result(result, workspace_key.as_str(), &event_tx);
    })
}

fn dispatch_ai_worker_result(
    result: std::thread::Result<Result<(), CodexIntegrationError>>,
    workspace_key: &str,
    event_tx: &Sender<AiWorkerEvent>,
) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            send_ai_worker_event(
                event_tx,
                workspace_key,
                AiWorkerEventPayload::Fatal(error.to_string()),
            );
        }
        Err(payload) => {
            send_ai_worker_event(
                event_tx,
                workspace_key,
                AiWorkerEventPayload::Fatal(format!(
                    "AI worker panicked: {}",
                    panic_payload_message(payload)
                )),
            );
        }
    }
}

fn send_ai_worker_event(
    event_tx: &Sender<AiWorkerEvent>,
    workspace_key: &str,
    payload: AiWorkerEventPayload,
) {
    let _ = event_tx.send(AiWorkerEvent::new(workspace_key.to_string(), payload));
}

fn panic_payload_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "unknown panic payload".to_string(),
        },
    }
}

struct AiWorkerRuntime {
    host: SharedHostLease,
    session: JsonRpcSession,
    service: ThreadService,
    codex_home: PathBuf,
    workspace_key: String,
    request_timeout: Duration,
    mad_max_mode: bool,
    account: Option<Account>,
    requires_openai_auth: bool,
    pending_chatgpt_login_id: Option<String>,
    pending_chatgpt_auth_url: Option<String>,
    rate_limits: Option<RateLimitSnapshot>,
    rate_limits_by_limit_id: HashMap<String, RateLimitSnapshot>,
    models: Vec<Model>,
    experimental_features: Vec<ExperimentalFeature>,
    collaboration_modes: Vec<CollaborationModeMask>,
    skills: Vec<SkillMetadata>,
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

impl AiWorkerRuntime {
    fn bootstrap(config: AiWorkerStartConfig) -> Result<Self, CodexIntegrationError> {
        std::fs::create_dir_all(&config.codex_home)
            .map_err(CodexIntegrationError::HostProcessIo)?;

        let mut last_retryable_error = None;
        for _attempt in 0..HOST_BOOTSTRAP_MAX_ATTEMPTS {
            let port = allocate_loopback_port();
            match Self::bootstrap_on_port(&config, port) {
                Ok(runtime) => return Ok(runtime),
                Err(error) if should_retry_bootstrap_with_new_port(&error) => {
                    last_retryable_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }

        Err(last_retryable_error.unwrap_or(CodexIntegrationError::HostStartupTimedOut {
            port: 0,
            timeout_ms: HOST_START_TIMEOUT
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        }))
    }

    fn bootstrap_on_port(
        config: &AiWorkerStartConfig,
        port: u16,
    ) -> Result<Self, CodexIntegrationError> {
        let host_config = HostConfig::codex_app_server(
            config.codex_executable.clone(),
            config.host_working_directory.clone(),
            config.codex_home.clone(),
            port,
        );
        let host = SharedHostLease::acquire(host_config, HOST_START_TIMEOUT)?;

        let endpoint = WebSocketEndpoint::loopback(host.port());
        let mut session = JsonRpcSession::connect(&endpoint)?;
        session.initialize(InitializeOptions::default(), config.request_timeout)?;

        Ok(Self {
            host,
            session,
            service: ThreadService::new(config.cwd.clone()),
            codex_home: config.codex_home.clone(),
            workspace_key: config.workspace_key.clone(),
            request_timeout: config.request_timeout,
            mad_max_mode: config.mad_max_mode,
            account: None,
            requires_openai_auth: false,
            pending_chatgpt_login_id: None,
            pending_chatgpt_auth_url: None,
            rate_limits: None,
            rate_limits_by_limit_id: HashMap::new(),
            models: Vec::new(),
            experimental_features: Vec::new(),
            collaboration_modes: Vec::new(),
            skills: Vec::new(),
            include_hidden_models: config.include_hidden_models,
            tool_registry: DynamicToolRegistry::new(),
            pending_approvals: BTreeMap::new(),
            pending_user_inputs: BTreeMap::new(),
            next_approval_sequence: 1,
            next_user_input_sequence: 1,
        })
    }

    fn send_event(&self, event_tx: &Sender<AiWorkerEvent>, payload: AiWorkerEventPayload) {
        send_ai_worker_event(event_tx, self.workspace_key.as_str(), payload);
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
            AiWorkerCommand::RefreshThreadMetadata { thread_id } => {
                self.refresh_thread_metadata_snapshot(thread_id)?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::SetIncludeHiddenModels { enabled } => {
                self.include_hidden_models = enabled;
                self.refresh_models()?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::StartThread {
                prompt,
                local_image_paths,
                selected_skills,
                skill_bindings,
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
                    .set_active_thread_for_cwd(
                        self.workspace_key.clone(),
                        response.thread.id.clone(),
                    );
                self.send_event(
                    event_tx,
                    AiWorkerEventPayload::ThreadStarted {
                        thread_id: response.thread.id.clone(),
                    },
                );
                self.emit_snapshot_after_sync(event_tx)?;
                if prompt.as_ref().is_some_and(|value| !value.trim().is_empty())
                    || !local_image_paths.is_empty()
                {
                    if let Some(pending_steer) = self.send_prompt(
                        response.thread.id,
                        prompt,
                        local_image_paths,
                        selected_skills,
                        skill_bindings,
                        session_overrides,
                    )? {
                        self.send_event(
                            event_tx,
                            AiWorkerEventPayload::SteerAccepted(pending_steer),
                        );
                    }
                    self.emit_snapshot_after_sync(event_tx)?;
                }
            }
            AiWorkerCommand::SelectThread { thread_id } => {
                self.load_thread_snapshot(thread_id)?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::ArchiveThread { thread_id } => {
                let archive_result = self.service.archive_thread(
                    &mut self.session,
                    thread_id.clone(),
                    self.request_timeout,
                );
                match archive_result {
                    Ok(_) => {
                        self.update_active_thread_after_archive(thread_id.as_str());
                    }
                    Err(error)
                        if is_missing_thread_rollout_error(&error)
                            && self.reconcile_missing_rollout_thread_error(thread_id.as_str())? => {}
                    Err(error) => return Err(error),
                }
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::SendPrompt {
                thread_id,
                prompt,
                local_image_paths,
                selected_skills,
                skill_bindings,
                session_overrides,
            } => {
                if let Some(pending_steer) =
                    self.send_prompt(
                        thread_id,
                        prompt,
                        local_image_paths,
                        selected_skills,
                        skill_bindings,
                        session_overrides,
                    )?
                {
                    self.send_event(event_tx, AiWorkerEventPayload::SteerAccepted(pending_steer));
                }
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
                                self.send_event(
                                    event_tx,
                                    AiWorkerEventPayload::Status(
                                        "Opened browser for ChatGPT login.".to_string(),
                                    ),
                                );
                            }
                            Err(_) => {
                                self.send_event(
                                    event_tx,
                                    AiWorkerEventPayload::Status(format!(
                                        "Open this URL to continue ChatGPT login: {auth_url}"
                                    )),
                                );
                            }
                        }
                    }
                    LoginAccountResponse::ChatgptDeviceCode {
                        login_id,
                        verification_url,
                        user_code,
                    } => {
                        self.pending_chatgpt_login_id = Some(login_id.clone());
                        self.pending_chatgpt_auth_url = Some(verification_url.clone());
                        match open_url_in_system_browser(verification_url.as_str()) {
                            Ok(()) => {
                                self.send_event(
                                    event_tx,
                                    AiWorkerEventPayload::Status(format!(
                                        "Opened browser for ChatGPT login. Enter code {user_code} at {verification_url}"
                                    )),
                                );
                            }
                            Err(_) => {
                                self.send_event(
                                    event_tx,
                                    AiWorkerEventPayload::Status(format!(
                                        "Open {verification_url} and enter code {user_code} to continue ChatGPT login."
                                    )),
                                );
                            }
                        }
                    }
                    LoginAccountResponse::ApiKey { .. } => {
                        self.send_event(
                            event_tx,
                            AiWorkerEventPayload::Status(
                                "Server returned API-key login mode; expected ChatGPT login."
                                    .to_string(),
                            ),
                        );
                    }
                    LoginAccountResponse::ChatgptAuthTokens { .. } => {
                        self.send_event(
                            event_tx,
                            AiWorkerEventPayload::Status(
                                "Server returned external auth token mode.".to_string(),
                            ),
                        );
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
                    self.send_event(event_tx, AiWorkerEventPayload::Status(message));
                } else {
                    self.send_event(
                        event_tx,
                        AiWorkerEventPayload::Status(
                            "No active ChatGPT login attempt.".to_string(),
                        ),
                    );
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
                self.rate_limits_by_limit_id.clear();
                self.refresh_account_state()?;
                self.emit_snapshot_after_sync(event_tx)?;
            }
            AiWorkerCommand::Shutdown => {}
        }
        Ok(())
    }

    fn send_prompt(
        &mut self,
        thread_id: String,
        prompt: Option<String>,
        local_image_paths: Vec<PathBuf>,
        selected_skills: Vec<AiPromptSkillReference>,
        skill_bindings: Vec<AiComposerSkillBinding>,
        session_overrides: AiTurnSessionOverrides,
    ) -> Result<Option<AiPendingSteer>, CodexIntegrationError> {
        let trimmed = prompt.as_deref().map(str::trim).filter(|text| !text.is_empty());
        if trimmed.is_none() && local_image_paths.is_empty() && selected_skills.is_empty() {
            return Ok(None);
        }
        self.service
            .state_mut()
            .set_active_thread_for_cwd(self.workspace_key.clone(), thread_id.clone());

        let input = prompt_user_input_items(trimmed, local_image_paths.as_slice(), &selected_skills);
        if let Some(in_progress_turn_id) = self.in_progress_turn_id(thread_id.as_str()) {
            let pending_steer = pending_steer_with_state_baseline(
                self.service.state(),
                thread_id.clone(),
                in_progress_turn_id.clone(),
                trimmed,
                &local_image_paths,
                &selected_skills,
                &skill_bindings,
            );
            let steer_result = self.service.steer_turn(
                &mut self.session,
                TurnSteerParams {
                    thread_id: thread_id.clone(),
                    input: input.clone(),
                    responsesapi_client_metadata: None,
                    expected_turn_id: in_progress_turn_id.clone(),
                },
                self.request_timeout,
            );

            match steer_result {
                Ok(_) => return Ok(Some(pending_steer)),
                Err(error) if should_retry_stale_turn_after_steer_error(&error) => {
                    self.service.read_thread(
                        &mut self.session,
                        thread_id.clone(),
                        true,
                        self.request_timeout,
                    )?;
                    if let Some(refreshed_turn_id) = self.in_progress_turn_id(thread_id.as_str()) {
                        let pending_steer = pending_steer_with_state_baseline(
                            self.service.state(),
                            thread_id.clone(),
                            refreshed_turn_id.clone(),
                            trimmed,
                            &local_image_paths,
                            &selected_skills,
                            &skill_bindings,
                        );
                        self.service.steer_turn(
                            &mut self.session,
                            TurnSteerParams {
                                thread_id: thread_id.clone(),
                                input: input.clone(),
                                responsesapi_client_metadata: None,
                                expected_turn_id: refreshed_turn_id.clone(),
                            },
                            self.request_timeout,
                        )?;
                        return Ok(Some(pending_steer));
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
        Ok(None)
    }

    fn load_thread_snapshot(
        &mut self,
        thread_id: String,
    ) -> Result<(), CodexIntegrationError> {
        let read_thread_id = thread_id.clone();
        match retry_transient_rollout_load(
            TRANSIENT_ROLLOUT_LOAD_MAX_RETRIES,
            TRANSIENT_ROLLOUT_LOAD_RETRY_DELAY,
            || {
                self.service.resume_thread(
                    &mut self.session,
                    ThreadResumeParams {
                        thread_id: thread_id.clone(),
                        persist_extended_history: true,
                        ..ThreadResumeParams::default()
                    },
                    self.request_timeout,
                )?;
                self.service.read_thread(
                    &mut self.session,
                    read_thread_id.clone(),
                    true,
                    self.request_timeout,
                )?;
                Ok(())
            },
        ) {
            Ok(()) => {}
            Err(error) if is_transient_rollout_load_error(&error) => {
                tracing::debug!(
                    thread_id = read_thread_id.as_str(),
                    error = %error,
                    "ignoring transient rollout load error while hydrating thread snapshot"
                );
            }
            Err(error)
                if is_missing_thread_rollout_error(&error)
                    && self.reconcile_missing_rollout_thread_error(read_thread_id.as_str())? => {}
            Err(error) => return Err(error),
        }
        self.hydrate_thread_from_rollout_fallback_if_needed(read_thread_id.as_str());
        Ok(())
    }

    fn refresh_thread_metadata_snapshot(
        &mut self,
        thread_id: String,
    ) -> Result<(), CodexIntegrationError> {
        match retry_transient_rollout_load(
            TRANSIENT_ROLLOUT_LOAD_MAX_RETRIES,
            TRANSIENT_ROLLOUT_LOAD_RETRY_DELAY,
            || {
                self.service
                    .read_thread(&mut self.session, thread_id.clone(), false, self.request_timeout)?;
                Ok(())
            },
        ) {
            Ok(()) => {}
            Err(error) if is_transient_rollout_load_error(&error) => {
                tracing::debug!(
                    thread_id = thread_id.as_str(),
                    error = %error,
                    "ignoring transient rollout load error while refreshing thread metadata"
                );
            }
            Err(error)
                if is_missing_thread_rollout_error(&error)
                    && self.reconcile_missing_rollout_thread_error(thread_id.as_str())? => {}
            Err(error) => return Err(error),
        }
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
            && let Some(home_codex) = default_codex_home_path()
            && home_codex != self.codex_home
        {
            rollout_path = match find_rollout_path_for_thread(home_codex.as_path(), thread_id) {
                Ok(Some(path)) => Some(path),
                Ok(None) => None,
                Err(_) => None,
            };
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
        params.service_tier = selected_ai_service_tier(session_overrides.service_tier);
        params.effort = session_overrides
            .effort
            .as_deref()
            .and_then(parse_reasoning_effort);

        let mode_kind = match session_overrides.collaboration_mode {
            AiCollaborationModeSelection::Default => ModeKind::Default,
            AiCollaborationModeSelection::Plan => ModeKind::Plan,
        };
        let mode_mask =
            self.collaboration_modes
                .iter()
                .find(|mask| match session_overrides.collaboration_mode {
                    AiCollaborationModeSelection::Default => {
                        mask.mode == Some(ModeKind::Default)
                            || mask.name.eq_ignore_ascii_case("Default")
                    }
                    AiCollaborationModeSelection::Plan => {
                        mask.mode == Some(ModeKind::Plan)
                            || mask.name.eq_ignore_ascii_case("Plan")
                    }
                });

        let model = session_overrides
            .model
            .clone()
            .or_else(|| mode_mask.and_then(|mask| mask.model.clone()))
            .or_else(|| self.default_model_id());
        let Some(model) = model else {
            return;
        };

        let effort = session_overrides
            .effort
            .as_deref()
            .and_then(parse_reasoning_effort)
            .or_else(|| mode_mask.and_then(|mask| mask.reasoning_effort.unwrap_or(None)));

        let collaboration_mode = CollaborationMode {
            mode: mode_kind,
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
}

fn prompt_user_input_items(
    trimmed_prompt: Option<&str>,
    local_image_paths: &[PathBuf],
    selected_skills: &[AiPromptSkillReference],
) -> Vec<UserInput> {
    let mut input = local_image_paths
        .iter()
        .cloned()
        .map(|path| UserInput::LocalImage { path })
        .collect::<Vec<_>>();
    if let Some(text) = trimmed_prompt {
        input.push(UserInput::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        });
    }
    input.extend(selected_skills.iter().cloned().map(|skill| UserInput::Skill {
        name: skill.name,
        path: skill.path,
    }));
    input
}
