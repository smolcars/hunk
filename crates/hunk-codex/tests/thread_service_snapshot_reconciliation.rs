use std::collections::BTreeSet;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use codex_app_server_protocol::ApprovalsReviewer;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SandboxPolicy;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput;
use hunk_codex::api;
use hunk_codex::api::InitializeOptions;
use hunk_codex::state::TurnStatus as StateTurnStatus;
use hunk_codex::threads::RolloutFallbackItem;
use hunk_codex::threads::RolloutFallbackTurn;
use hunk_codex::threads::ThreadService;
use hunk_codex::ws_client::JsonRpcSession;
use hunk_codex::ws_client::WebSocketEndpoint;
use serde_json::Value;
use tungstenite::Message;
use tungstenite::WebSocket;
use tungstenite::accept;

const WORKSPACE_CWD: &str = "/repo-a";
const TIMEOUT: Duration = Duration::from_secs(2);

#[test]
fn authoritative_snapshots_replace_turn_items_after_fallback_and_follow_up_reads() {
    let server = TestServer::spawn(Scenario::ReplaceTurnItemsAfterFollowUpRead);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service.ingest_rollout_fallback_history(
        "external-thread".to_string(),
        &[RolloutFallbackTurn {
            turn_id: "resume-turn-1".to_string(),
            completed: true,
            items: vec![
                RolloutFallbackItem {
                    kind: "userMessage".to_string(),
                    content: "weather in CA".to_string(),
                },
                RolloutFallbackItem {
                    kind: "agentMessage".to_string(),
                    content: "You're asking for current weather in California.".to_string(),
                },
                RolloutFallbackItem {
                    kind: "agentMessage".to_string(),
                    content: "California varies a lot by city.".to_string(),
                },
            ],
        }],
    );

    service
        .resume_thread(
            &mut session,
            ThreadResumeParams {
                thread_id: "external-thread".to_string(),
                ..ThreadResumeParams::default()
            },
            TIMEOUT,
        )
        .expect("thread/resume should succeed");

    assert_eq!(
        turn_status_for(service.state(), "external-thread", "resume-turn-1"),
        StateTurnStatus::Completed
    );
    assert_eq!(
        item_ids_for_turn(service.state(), "external-thread", "resume-turn-1"),
        string_set(&["resume-agent-1", "resume-user-1"])
    );

    service
        .read_thread(&mut session, "external-thread".to_string(), true, TIMEOUT)
        .expect("thread/read should succeed");

    assert_eq!(
        turn_status_for(service.state(), "external-thread", "resume-turn-1"),
        StateTurnStatus::Completed
    );
    assert_eq!(
        item_ids_for_turn(service.state(), "external-thread", "resume-turn-1"),
        string_set(&["read-agent-1", "read-agent-2", "read-user-1"])
    );

    server.join();
}

#[test]
fn authoritative_snapshots_replace_existing_agent_item_content_for_same_item_id() {
    let server = TestServer::spawn(Scenario::ReplaceAgentContentForSameItemId);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .resume_thread(
            &mut session,
            ThreadResumeParams {
                thread_id: "external-thread".to_string(),
                ..ThreadResumeParams::default()
            },
            TIMEOUT,
        )
        .expect("thread/resume should succeed");

    assert_eq!(
        turn_status_for(service.state(), "external-thread", "resume-turn-1"),
        StateTurnStatus::InProgress
    );
    assert_eq!(
        item_content_for(
            service.state(),
            "external-thread",
            "resume-turn-1",
            "resume-agent-1"
        ),
        Some("assistant chunk one")
    );

    service
        .read_thread(&mut session, "external-thread".to_string(), true, TIMEOUT)
        .expect("thread/read should succeed");

    assert_eq!(
        turn_status_for(service.state(), "external-thread", "resume-turn-1"),
        StateTurnStatus::InProgress
    );
    assert_eq!(
        item_content_for(
            service.state(),
            "external-thread",
            "resume-turn-1",
            "resume-agent-1"
        ),
        Some("assistant chunk two after reconnect")
    );

    server.join();
}

fn item_ids_for_turn(
    state: &hunk_codex::state::AiState,
    thread_id: &str,
    turn_id: &str,
) -> BTreeSet<String> {
    state
        .items
        .values()
        .filter(|item| item.thread_id == thread_id && item.turn_id == turn_id)
        .map(|item| item.id.clone())
        .collect()
}

fn turn_status_for(
    state: &hunk_codex::state::AiState,
    thread_id: &str,
    turn_id: &str,
) -> StateTurnStatus {
    state
        .turns
        .values()
        .find(|turn| turn.thread_id == thread_id && turn.id == turn_id)
        .expect("turn should exist")
        .status
}

fn string_set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn item_content_for<'a>(
    state: &'a hunk_codex::state::AiState,
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
) -> Option<&'a str> {
    state
        .items
        .values()
        .find(|item| item.thread_id == thread_id && item.turn_id == turn_id && item.id == item_id)
        .map(|item| item.content.as_str())
}

#[derive(Clone, Copy)]
enum Scenario {
    ReplaceTurnItemsAfterFollowUpRead,
    ReplaceAgentContentForSameItemId,
}

struct TestServer {
    port: u16,
    join: thread::JoinHandle<()>,
}

impl TestServer {
    fn spawn(scenario: Scenario) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind should succeed");
            let port = listener
                .local_addr()
                .expect("local addr should exist")
                .port();
            tx.send(port).expect("port should be sent");

            let (stream, _) = listener.accept().expect("accept should succeed");
            let mut socket = accept(stream).expect("websocket handshake should succeed");
            run_initialize_handshake(&mut socket);
            match scenario {
                Scenario::ReplaceTurnItemsAfterFollowUpRead => {
                    run_resume_then_read_authoritative_snapshot(&mut socket)
                }
                Scenario::ReplaceAgentContentForSameItemId => {
                    run_resume_then_read_same_item_snapshot(&mut socket)
                }
            }
        });

        let port = rx.recv().expect("port should be received");
        Self { port, join }
    }

    fn join(self) {
        self.join
            .join()
            .expect("test server thread should complete");
    }
}

fn run_initialize_handshake(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, api::method::INITIALIZE);
    send_success_response(socket, initialize.id, initialize_response_json());
    expect_notification(socket, api::method::INITIALIZED);
}

fn run_resume_then_read_authoritative_snapshot(socket: &mut WebSocket<TcpStream>) {
    let resume = expect_request(socket, api::method::THREAD_RESUME);
    let resume_params = resume
        .params
        .expect("thread/resume params should be present");
    assert_eq!(
        param_string(&resume_params, "threadId"),
        Some("external-thread".to_string())
    );
    assert_eq!(
        param_string(&resume_params, "cwd"),
        Some(WORKSPACE_CWD.to_string())
    );
    send_typed_success_response(
        socket,
        resume.id,
        &thread_resume_response(thread(
            "external-thread",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![turn_with_items(
                "resume-turn-1",
                TurnStatus::Completed,
                vec![
                    user_message("resume-user-1", "weather in CA"),
                    agent_message(
                        "resume-agent-1",
                        "You're asking for current weather in California.",
                    ),
                ],
            )],
        )),
    );

    let read = expect_request(socket, api::method::THREAD_READ);
    let read_params = read.params.expect("thread/read params should be present");
    assert_eq!(
        param_string(&read_params, "threadId"),
        Some("external-thread".to_string())
    );
    assert_eq!(
        read_params.get("includeTurns"),
        Some(&serde_json::json!(true))
    );
    send_typed_success_response(
        socket,
        read.id,
        &ThreadReadResponse {
            thread: thread(
                "external-thread",
                WORKSPACE_CWD,
                ThreadStatus::Idle,
                vec![turn_with_items(
                    "resume-turn-1",
                    TurnStatus::Completed,
                    vec![
                        user_message("read-user-1", "weather in CA"),
                        agent_message(
                            "read-agent-1",
                            "You're asking for current weather in California.",
                        ),
                        agent_message("read-agent-2", "California varies a lot by city."),
                    ],
                )],
            ),
        },
    );
}

fn run_resume_then_read_same_item_snapshot(socket: &mut WebSocket<TcpStream>) {
    let resume = expect_request(socket, api::method::THREAD_RESUME);
    let resume_params = resume
        .params
        .expect("thread/resume params should be present");
    assert_eq!(
        param_string(&resume_params, "threadId"),
        Some("external-thread".to_string())
    );
    send_typed_success_response(
        socket,
        resume.id,
        &thread_resume_response(thread(
            "external-thread",
            WORKSPACE_CWD,
            ThreadStatus::Active {
                active_flags: Vec::new(),
            },
            vec![turn_with_items(
                "resume-turn-1",
                TurnStatus::InProgress,
                vec![
                    user_message("resume-user-1", "keep going"),
                    agent_message("resume-agent-1", "assistant chunk one"),
                ],
            )],
        )),
    );

    let read = expect_request(socket, api::method::THREAD_READ);
    let read_params = read.params.expect("thread/read params should be present");
    assert_eq!(
        param_string(&read_params, "threadId"),
        Some("external-thread".to_string())
    );
    assert_eq!(
        read_params.get("includeTurns"),
        Some(&serde_json::json!(true))
    );
    send_typed_success_response(
        socket,
        read.id,
        &ThreadReadResponse {
            thread: thread(
                "external-thread",
                WORKSPACE_CWD,
                ThreadStatus::Active {
                    active_flags: Vec::new(),
                },
                vec![turn_with_items(
                    "resume-turn-1",
                    TurnStatus::InProgress,
                    vec![
                        user_message("resume-user-1", "keep going"),
                        agent_message("resume-agent-1", "assistant chunk two after reconnect"),
                    ],
                )],
            ),
        },
    );
}

fn connect_initialized_session(port: u16) -> JsonRpcSession {
    let endpoint = WebSocketEndpoint::loopback(port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");
    session
        .initialize(InitializeOptions::default(), TIMEOUT)
        .expect("initialize should succeed");
    session
}

fn initialize_response_json() -> Value {
    serde_json::json!({
        "userAgent": "hunk-thread-service-reconcile-test-server",
        "codexHome": "/tmp/hunk-codex-test-home",
        "platformFamily": "windows",
        "platformOs": "windows"
    })
}

fn thread(id: &str, cwd: &str, status: ThreadStatus, turns: Vec<Turn>) -> Thread {
    Thread {
        id: id.to_string(),
        preview: format!("preview-{id}"),
        ephemeral: false,
        model_provider: "openai".to_string(),
        created_at: 1,
        updated_at: 2,
        status,
        path: Some(format!("/tmp/.codex/threads/{id}.jsonl").into()),
        cwd: cwd.into(),
        cli_version: "0.1.0".to_string(),
        source: SessionSource::AppServer,
        agent_nickname: None,
        agent_role: None,
        forked_from_id: None,
        git_info: None,
        name: Some(format!("Thread {id}")),
        turns,
    }
}

fn turn_with_items(id: &str, status: TurnStatus, items: Vec<ThreadItem>) -> Turn {
    Turn {
        id: id.to_string(),
        items,
        status,
        error: None,
        started_at: None,
        completed_at: None,
        duration_ms: None,
    }
}

fn user_message(id: &str, text: &str) -> ThreadItem {
    ThreadItem::UserMessage {
        id: id.to_string(),
        content: vec![UserInput::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        }],
    }
}

fn agent_message(id: &str, text: &str) -> ThreadItem {
    ThreadItem::AgentMessage {
        id: id.to_string(),
        text: text.to_string(),
        phase: None,
        memory_citation: None,
    }
}

fn thread_resume_response(thread: Thread) -> ThreadResumeResponse {
    ThreadResumeResponse {
        cwd: thread.cwd.clone(),
        thread,
        model: "gpt-5-codex".to_string(),
        model_provider: "openai".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::OnRequest,
        approvals_reviewer: ApprovalsReviewer::User,
        sandbox: SandboxPolicy::DangerFullAccess,
        reasoning_effort: None,
    }
}

fn expect_request(socket: &mut WebSocket<TcpStream>, method: &str) -> JSONRPCRequest {
    match read_jsonrpc(socket) {
        JSONRPCMessage::Request(request) => {
            assert_eq!(request.method, method, "unexpected method");
            request
        }
        other => panic!("expected request, got: {other:?}"),
    }
}

fn expect_notification(socket: &mut WebSocket<TcpStream>, method: &str) -> JSONRPCNotification {
    match read_jsonrpc(socket) {
        JSONRPCMessage::Notification(notification) => {
            assert_eq!(
                notification.method, method,
                "unexpected notification method"
            );
            notification
        }
        other => panic!("expected notification, got: {other:?}"),
    }
}

fn send_typed_success_response<T: serde::Serialize>(
    socket: &mut WebSocket<TcpStream>,
    id: RequestId,
    result: &T,
) {
    let value = serde_json::to_value(result).expect("response serialization should succeed");
    send_success_response(socket, id, value);
}

fn send_success_response(socket: &mut WebSocket<TcpStream>, id: RequestId, result: Value) {
    send_jsonrpc(
        socket,
        JSONRPCMessage::Response(JSONRPCResponse { id, result }),
    );
}

fn send_jsonrpc(socket: &mut WebSocket<TcpStream>, message: JSONRPCMessage) {
    let payload = serde_json::to_string(&message).expect("serialize should succeed");
    socket
        .send(Message::Text(payload.into()))
        .expect("socket send should succeed");
}

fn read_jsonrpc(socket: &mut WebSocket<TcpStream>) -> JSONRPCMessage {
    loop {
        let frame = socket.read().expect("socket read should succeed");
        match frame {
            Message::Text(text) => {
                return serde_json::from_str(text.as_ref()).expect("json parse should succeed");
            }
            Message::Binary(bytes) => {
                return serde_json::from_slice(bytes.as_ref()).expect("json parse should succeed");
            }
            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .expect("pong send should succeed");
            }
            Message::Pong(_) | Message::Frame(_) => {}
            Message::Close(_) => panic!("unexpected socket close"),
        }
    }
}

fn param_string(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}
