#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use hunk_codex::api::method;
use hunk_codex::app_server_client::{
    AppServerClient, AppServerEvent, EmbeddedAppServerClient, EmbeddedAppServerClientStartArgs,
};
use hunk_codex::protocol::{
    AskForApproval, FileChangeApprovalDecision, FileChangeOutputDeltaNotification,
    FileChangeRequestApprovalResponse, PatchApplyStatus, SandboxMode, ServerNotification,
    ServerRequest, ThreadItem, ThreadStartParams, ThreadStartResponse, TurnStartParams,
    TurnStartResponse, UserInput,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::runtime::{Builder, Runtime};
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::{method as http_method, path_regex};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const EVENT_TIMEOUT: Duration = Duration::from_secs(30);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir should have parent")
        .parent()
        .expect("workspace root should exist")
        .to_path_buf()
}

fn bundled_codex_executable() -> PathBuf {
    repo_root().join("assets/codex-runtime/macos/codex")
}

fn write_mock_responses_config_toml(codex_home: &Path, server_uri: &str) -> Result<()> {
    std::fs::write(
        codex_home.join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "on-request"
sandbox_mode = "workspace-write"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
    .context("failed to write mock Codex config")?;
    Ok(())
}

fn sse(events: Vec<Value>) -> String {
    let mut out = String::new();
    for event in events {
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .expect("event type");
        out.push_str(&format!("event: {kind}\n"));
        if event.as_object().is_none_or(|object| object.len() != 1) {
            out.push_str(&format!("data: {event}\n\n"));
        } else {
            out.push('\n');
        }
    }
    out
}

fn ev_response_created(id: &str) -> Value {
    json!({
        "type": "response.created",
        "response": { "id": id }
    })
}

fn ev_completed(id: &str) -> Value {
    json!({
        "type": "response.completed",
        "response": {
            "id": id,
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0
            }
        }
    })
}

fn ev_assistant_message(id: &str, text: &str) -> Value {
    json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "role": "assistant",
            "id": id,
            "content": [{ "type": "output_text", "text": text }]
        }
    })
}

fn ev_function_call(call_id: &str, name: &str, arguments: &str) -> Value {
    json!({
        "type": "response.output_item.done",
        "item": {
            "type": "function_call",
            "call_id": call_id,
            "name": name,
            "arguments": arguments
        }
    })
}

fn ev_apply_patch_shell_command_call_via_heredoc(call_id: &str, patch: &str) -> Value {
    let arguments = serde_json::to_string(&json!({
        "command": format!("apply_patch <<'EOF'\n{patch}\nEOF\n")
    }))
    .expect("serialize apply_patch shell command args");
    ev_function_call(call_id, "shell_command", &arguments)
}

fn create_apply_patch_sse_response(patch: &str, call_id: &str) -> String {
    sse(vec![
        ev_response_created("resp-1"),
        ev_apply_patch_shell_command_call_via_heredoc(call_id, patch),
        ev_completed("resp-1"),
    ])
}

fn create_final_assistant_message_sse_response(message: &str) -> String {
    sse(vec![
        ev_response_created("resp-2"),
        ev_assistant_message("msg-1", message),
        ev_completed("resp-2"),
    ])
}

struct SeqResponder {
    call_count: AtomicUsize,
    responses: Vec<String>,
}

impl Respond for SeqResponder {
    fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
        let call_index = self.call_count.fetch_add(1, Ordering::SeqCst);
        let body = self
            .responses
            .get(call_index)
            .unwrap_or_else(|| panic!("missing mock response for call {call_index}"));
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_raw(body.clone(), "text/event-stream")
    }
}

fn start_mock_responses_server(runtime: &Runtime, responses: Vec<String>) -> MockServer {
    runtime.block_on(async move {
        let server = MockServer::start().await;
        Mock::given(http_method("POST"))
            .and(path_regex(".*/responses$"))
            .respond_with(SeqResponder {
                call_count: AtomicUsize::new(0),
                responses,
            })
            .expect(2)
            .mount(&server)
            .await;
        server
    })
}

#[test]
fn embedded_turn_apply_patch_updates_workspace_file() -> Result<()> {
    let async_runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("failed to build async runtime for mock Responses server")?;

    let tempdir = TempDir::new()?;
    let codex_home = tempdir.path().join("codex_home");
    let workspace = tempdir.path().join("workspace");
    std::fs::create_dir_all(&codex_home)?;
    std::fs::create_dir_all(&workspace)?;

    let patch_target = workspace.join("README.md");
    std::fs::write(&patch_target, "alpha\n")?;

    let patch = format!(
        "*** Begin Patch\n*** Update File: {}\n@@\n-alpha\n+beta\n*** End Patch\n",
        patch_target.display()
    );
    let server = start_mock_responses_server(
        &async_runtime,
        vec![
            create_apply_patch_sse_response(&patch, "patch-call"),
            create_final_assistant_message_sse_response("patch applied"),
        ],
    );
    write_mock_responses_config_toml(&codex_home, &server.uri())?;

    let codex_executable = bundled_codex_executable();
    assert!(
        codex_executable.exists(),
        "bundled Codex runtime missing: {}",
        codex_executable.display()
    );

    let mut client = EmbeddedAppServerClient::start(EmbeddedAppServerClientStartArgs::new(
        codex_home.clone(),
        workspace.clone(),
        codex_executable,
        "hunk-test".to_string(),
        "0.0.0-test".to_string(),
    ))?;

    let thread_start: ThreadStartResponse = client.request_typed(
        method::THREAD_START,
        Some(&ThreadStartParams {
            model: Some("mock-model".to_string()),
            cwd: Some(workspace.to_string_lossy().into_owned()),
            approval_policy: Some(AskForApproval::OnRequest),
            sandbox: Some(SandboxMode::WorkspaceWrite),
            ..Default::default()
        }),
        REQUEST_TIMEOUT,
    )?;

    let turn_start: TurnStartResponse = client.request_typed(
        method::TURN_START,
        Some(&TurnStartParams {
            thread_id: thread_start.thread.id.clone(),
            input: vec![UserInput::Text {
                text: "apply patch".to_string(),
                text_elements: Vec::new(),
            }],
            cwd: Some(workspace.clone()),
            ..Default::default()
        }),
        REQUEST_TIMEOUT,
    )?;

    let deadline = Instant::now() + EVENT_TIMEOUT;
    let mut saw_file_change_started = false;
    let mut saw_file_change_completed = false;
    let mut saw_turn_completed = false;
    let mut file_change_output = String::new();
    let mut file_change_completion_status: Option<PatchApplyStatus> = None;
    let mut turn_error: Option<String> = None;

    while Instant::now() < deadline && !saw_turn_completed {
        let Some(event) = client.next_event(Duration::from_millis(250))? else {
            continue;
        };

        match event {
            AppServerEvent::ServerNotification(ServerNotification::ItemStarted(notification)) => {
                if notification.thread_id != thread_start.thread.id
                    || notification.turn_id != turn_start.turn.id
                {
                    continue;
                }
                if let ThreadItem::FileChange {
                    id,
                    status,
                    changes,
                } = notification.item
                {
                    assert_eq!(id, "patch-call");
                    assert_eq!(status, PatchApplyStatus::InProgress);
                    assert_eq!(changes.len(), 1);
                    saw_file_change_started = true;
                }
            }
            AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                request_id,
                params,
            }) => {
                assert_eq!(params.thread_id, thread_start.thread.id);
                assert_eq!(params.turn_id, turn_start.turn.id);
                assert_eq!(params.item_id, "patch-call");
                client.respond_typed(
                    request_id,
                    &FileChangeRequestApprovalResponse {
                        decision: FileChangeApprovalDecision::Accept,
                    },
                )?;
            }
            AppServerEvent::ServerNotification(ServerNotification::FileChangeOutputDelta(
                FileChangeOutputDeltaNotification {
                    thread_id,
                    turn_id,
                    item_id,
                    delta,
                },
            )) => {
                if thread_id == thread_start.thread.id
                    && turn_id == turn_start.turn.id
                    && item_id == "patch-call"
                {
                    file_change_output.push_str(&delta);
                }
            }
            AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(notification)) => {
                if notification.thread_id != thread_start.thread.id
                    || notification.turn_id != turn_start.turn.id
                {
                    continue;
                }
                if let ThreadItem::FileChange { id, status, .. } = notification.item {
                    assert_eq!(id, "patch-call");
                    file_change_completion_status = Some(status.clone());
                    if status == PatchApplyStatus::Completed {
                        saw_file_change_completed = true;
                    }
                }
            }
            AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(notification)) => {
                if notification.thread_id == thread_start.thread.id
                    && notification.turn.id == turn_start.turn.id
                {
                    saw_turn_completed = true;
                }
            }
            AppServerEvent::ServerNotification(ServerNotification::Error(notification)) => {
                if notification.thread_id == thread_start.thread.id
                    && notification.turn_id == turn_start.turn.id
                {
                    let details = notification.error.additional_details.unwrap_or_default();
                    turn_error = Some(format!("{} | {}", notification.error.message, details));
                }
            }
            AppServerEvent::Lagged { skipped } => {
                bail!(
                    "embedded client lagged during apply_patch regression: skipped {skipped} events"
                );
            }
            AppServerEvent::Disconnected { message } => {
                bail!("embedded client disconnected during apply_patch regression: {message}");
            }
            _ => {}
        }
    }

    assert!(
        saw_file_change_started,
        "did not observe file-change item start"
    );
    assert!(
        saw_file_change_completed,
        "file-change item did not complete successfully: status={:?}, output={:?}, turn_error={:?}",
        file_change_completion_status, file_change_output, turn_error,
    );
    assert!(
        saw_turn_completed,
        "did not observe turn completion before timeout: status={:?}, output={:?}, turn_error={:?}",
        file_change_completion_status, file_change_output, turn_error,
    );
    assert!(
        file_change_output.contains("Success. Updated the following files:"),
        "file-change output did not report success: {file_change_output:?}",
    );
    assert_eq!(std::fs::read_to_string(&patch_target)?, "beta\n");

    client.shutdown()?;
    drop(async_runtime);
    Ok(())
}
