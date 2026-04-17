use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::protocol::AskForApproval;
use crate::protocol::JSONRPCErrorError;
use crate::protocol::ReadOnlyAccess;
use crate::protocol::ReasoningEffort;
use crate::protocol::SandboxMode;
use crate::protocol::SandboxPolicy;
use crate::protocol::ServerNotification;
use crate::protocol::ThreadItem;
use crate::protocol::ThreadReadParams;
use crate::protocol::ThreadReadResponse;
use crate::protocol::ThreadStartParams;
use crate::protocol::ThreadStartResponse;
use crate::protocol::TurnStartParams;
use crate::protocol::TurnStartResponse;
use crate::protocol::TurnStatus;
use crate::protocol::UserInput;
use serde_json::Value;

use crate::api;
use crate::app_server_client::AppServerClient;
use crate::app_server_client::AppServerEvent;
use crate::app_server_client::EmbeddedAppServerClient;
use crate::app_server_client::EmbeddedAppServerClientStartArgs;
use crate::errors::CodexIntegrationError;
use crate::errors::Result;

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const UNSUPPORTED_SERVER_REQUEST_CODE: i64 = -32601;

pub struct StructuredGenerationRequest<'a> {
    pub codex_home: &'a Path,
    pub cwd: &'a Path,
    pub codex_executable: &'a Path,
    pub prompt: &'a str,
    pub output_schema: &'a Value,
    pub image_paths: &'a [PathBuf],
    pub model: &'a str,
    pub reasoning_effort: ReasoningEffort,
    pub client_name: &'a str,
    pub client_version: &'a str,
    pub timeout: Duration,
}

pub fn generate_structured_output(request: StructuredGenerationRequest<'_>) -> Result<Value> {
    std::fs::create_dir_all(request.codex_home).map_err(CodexIntegrationError::HostProcessIo)?;

    let mut client = EmbeddedAppServerClient::start(EmbeddedAppServerClientStartArgs::new(
        request.codex_home.to_path_buf(),
        request.cwd.to_path_buf(),
        request.codex_executable.to_path_buf(),
        request.client_name.to_string(),
        request.client_version.to_string(),
    ))?;

    let result = generate_structured_output_with_client(&mut client, &request);
    let shutdown_result = client.shutdown();

    match (result, shutdown_result) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

fn generate_structured_output_with_client(
    client: &mut EmbeddedAppServerClient,
    request: &StructuredGenerationRequest<'_>,
) -> Result<Value> {
    let thread_response: ThreadStartResponse = client.request_typed(
        api::method::THREAD_START,
        Some(&ThreadStartParams {
            model: Some(request.model.to_string()),
            model_provider: None,
            service_tier: None,
            cwd: Some(request.cwd.to_string_lossy().to_string()),
            approval_policy: Some(AskForApproval::Never),
            approvals_reviewer: None,
            sandbox: Some(SandboxMode::ReadOnly),
            config: None,
            service_name: None,
            base_instructions: None,
            developer_instructions: None,
            personality: None,
            ephemeral: Some(true),
            dynamic_tools: None,
            mock_experimental_field: None,
            experimental_raw_events: false,
            persist_extended_history: false,
            ..ThreadStartParams::default()
        }),
        request.timeout,
    )?;

    let thread_id = thread_response.thread.id;
    let turn_response: TurnStartResponse = client.request_typed(
        api::method::TURN_START,
        Some(&TurnStartParams {
            thread_id: thread_id.clone(),
            input: build_user_input(request.prompt, request.image_paths),
            responsesapi_client_metadata: None,
            cwd: Some(request.cwd.to_path_buf()),
            approval_policy: Some(AskForApproval::Never),
            approvals_reviewer: None,
            sandbox_policy: Some(SandboxPolicy::ReadOnly {
                access: ReadOnlyAccess::default(),
                network_access: false,
            }),
            model: None,
            service_tier: None,
            effort: Some(request.reasoning_effort),
            summary: None,
            personality: None,
            output_schema: Some(request.output_schema.clone()),
            collaboration_mode: None,
        }),
        request.timeout,
    )?;

    let turn_id = turn_response.turn.id;
    let buffered_agent_message =
        wait_for_turn_completion(client, &thread_id, &turn_id, request.timeout)?;

    let read_response: ThreadReadResponse = client.request_typed(
        api::method::THREAD_READ,
        Some(&ThreadReadParams {
            thread_id: thread_id.clone(),
            include_turns: true,
        }),
        request.timeout,
    )?;

    let final_message = find_turn_agent_message(&read_response, &turn_id)
        .or_else(|| (!buffered_agent_message.is_empty()).then_some(buffered_agent_message))
        .ok_or_else(|| CodexIntegrationError::WebSocketTransport(format!(
            "embedded structured output generation completed without an agent message for turn {turn_id}"
        )))?;

    serde_json::from_str(final_message.as_str()).map_err(|error| {
        CodexIntegrationError::WebSocketTransport(format!(
            "embedded structured output generation returned invalid JSON: {error}"
        ))
    })
}

fn build_user_input(prompt: &str, image_paths: &[PathBuf]) -> Vec<UserInput> {
    let mut input = Vec::with_capacity(1 + image_paths.len());
    input.push(UserInput::Text {
        text: prompt.to_string(),
        text_elements: Vec::new(),
    });
    input.extend(
        image_paths
            .iter()
            .cloned()
            .map(|path| UserInput::LocalImage { path }),
    );
    input
}

fn wait_for_turn_completion(
    client: &mut EmbeddedAppServerClient,
    thread_id: &str,
    turn_id: &str,
    timeout: Duration,
) -> Result<String> {
    let started_at = Instant::now();
    let mut buffered_agent_message = String::new();
    let mut terminal_error = None;

    loop {
        let Some(remaining) = timeout.checked_sub(started_at.elapsed()) else {
            return Err(CodexIntegrationError::RequestTimedOut {
                method: api::method::TURN_START.to_string(),
                timeout_ms: timeout.as_millis().min(u128::from(u64::MAX)) as u64,
            });
        };

        let poll_timeout = remaining.min(EVENT_POLL_INTERVAL);
        let Some(event) = client.next_event(poll_timeout)? else {
            continue;
        };

        match event {
            AppServerEvent::Lagged { .. } => {}
            AppServerEvent::Disconnected { message } => {
                return Err(CodexIntegrationError::WebSocketTransport(format!(
                    "embedded structured output generation disconnected: {message}"
                )));
            }
            AppServerEvent::ServerRequest(request) => {
                client.reject_server_request(
                    request.id().clone(),
                    JSONRPCErrorError {
                        code: UNSUPPORTED_SERVER_REQUEST_CODE,
                        data: None,
                        message:
                            "structured generation helper does not support interactive server requests"
                                .to_string(),
                    },
                )?;
            }
            AppServerEvent::ServerNotification(notification) => match notification {
                ServerNotification::AgentMessageDelta(payload)
                    if payload.thread_id == thread_id && payload.turn_id == turn_id =>
                {
                    buffered_agent_message.push_str(payload.delta.as_str());
                }
                ServerNotification::Error(payload)
                    if payload.thread_id == thread_id
                        && payload.turn_id == turn_id
                        && !payload.will_retry =>
                {
                    terminal_error = Some(payload.error.message);
                }
                ServerNotification::TurnCompleted(payload)
                    if payload.thread_id == thread_id && payload.turn.id == turn_id =>
                {
                    match payload.turn.status {
                        TurnStatus::Completed => return Ok(buffered_agent_message),
                        TurnStatus::Failed | TurnStatus::Interrupted => {
                            let error = terminal_error.unwrap_or_else(|| {
                                format!(
                                    "turn {turn_id} completed with status {:?}",
                                    payload.turn.status
                                )
                            });
                            return Err(CodexIntegrationError::WebSocketTransport(format!(
                                "embedded structured output generation failed: {error}"
                            )));
                        }
                        TurnStatus::InProgress => {}
                    }
                }
                _ => {}
            },
        }
    }
}

fn find_turn_agent_message(response: &ThreadReadResponse, turn_id: &str) -> Option<String> {
    response
        .thread
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .and_then(|turn| {
            turn.items.iter().rev().find_map(|item| match item {
                ThreadItem::AgentMessage { text, .. } => Some(text.clone()),
                _ => None,
            })
        })
}
