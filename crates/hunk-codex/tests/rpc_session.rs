use std::collections::HashMap;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::DynamicToolCallOutputContentItem;
use codex_app_server_protocol::DynamicToolCallResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ToolRequestUserInputAnswer;
use codex_app_server_protocol::ToolRequestUserInputResponse;
use hunk_codex::api::InitializeOptions;
use hunk_codex::errors::CodexIntegrationError;
use hunk_codex::ws_client::JsonRpcSession;
use hunk_codex::ws_client::RequestRetryPolicy;
use hunk_codex::ws_client::WebSocketEndpoint;
use tungstenite::Message;
use tungstenite::WebSocket;
use tungstenite::accept;

#[test]
fn initialize_handshake_success_path() {
    let server = TestServer::spawn(Scenario::InitializeSuccess);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    let response = session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("initialize should succeed");

    assert_eq!(response.user_agent, "hunk-test-server");
    server.join();
}

#[test]
fn request_before_initialize_is_surfaced_as_error() {
    let server = TestServer::spawn(Scenario::RejectBeforeInitialize);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    let err = session
        .request("thread/list", None, Duration::from_secs(2))
        .expect_err("request should fail before initialize");

    match err {
        CodexIntegrationError::JsonRpcServerError { code, .. } => {
            assert_eq!(code, -32002);
        }
        other => panic!("unexpected error: {other}"),
    }

    server.join();
}

#[test]
fn duplicate_initialize_is_rejected() {
    let server = TestServer::spawn(Scenario::DuplicateInitialize);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("first initialize should succeed");

    let err = session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect_err("second initialize should fail");

    match err {
        CodexIntegrationError::JsonRpcServerError { code, .. } => {
            assert_eq!(code, -32010);
        }
        other => panic!("unexpected error: {other}"),
    }

    server.join();
}

#[test]
fn overloaded_error_retries_with_backoff() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let server = TestServer::spawn(Scenario::OverloadedThenSuccess {
        overload_attempts: 2,
        attempts: Arc::clone(&attempts),
    });

    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint)
        .expect("session should connect")
        .with_retry_policy(RequestRetryPolicy {
            max_overload_retries: 3,
            initial_backoff: Duration::from_millis(10),
        });

    let value = session
        .request("model/list", None, Duration::from_secs(2))
        .expect("request should eventually succeed");

    assert_eq!(value, serde_json::json!({"models": []}));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);

    server.join();
}

#[test]
fn poll_server_notifications_captures_idle_notifications() {
    let server = TestServer::spawn(Scenario::IdleNotification);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("initialize should succeed");

    let captured = session
        .poll_server_notifications(Duration::from_secs(2))
        .expect("poll should succeed");
    assert_eq!(captured, 1);

    let notifications = session.drain_server_notifications();
    assert_eq!(notifications.len(), 1);
    match &notifications[0] {
        ServerNotification::TurnDiffUpdated(notification) => {
            assert_eq!(notification.thread_id, "thread-live");
            assert_eq!(notification.turn_id, "turn-live");
            assert_eq!(notification.diff, "diff --git a/a b/a");
        }
        other => panic!("unexpected notification type: {other:?}"),
    }

    server.join();
}

#[test]
fn poll_captures_server_requests_and_can_respond() {
    let server = TestServer::spawn(Scenario::CommandApprovalRequestRoundTrip);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("initialize should succeed");

    let captured = session
        .poll_server_notifications(Duration::from_secs(2))
        .expect("poll should succeed");
    assert_eq!(captured, 1);

    let requests = session.drain_server_requests();
    assert_eq!(requests.len(), 1);
    let request_id = match &requests[0] {
        ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
            assert_eq!(params.thread_id, "thread-live");
            assert_eq!(params.turn_id, "turn-live");
            assert_eq!(params.item_id, "item-live");
            request_id.clone()
        }
        other => panic!("unexpected server request: {other:?}"),
    };

    session
        .respond_typed(
            request_id,
            &CommandExecutionRequestApprovalResponse {
                decision: CommandExecutionApprovalDecision::Accept,
            },
        )
        .expect("response should be sent");

    server.join();
}

#[test]
fn poll_captures_file_change_approval_and_decline_response() {
    let server = TestServer::spawn(Scenario::FileChangeApprovalRequestRoundTrip);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("initialize should succeed");

    let captured = session
        .poll_server_notifications(Duration::from_secs(2))
        .expect("poll should succeed");
    assert_eq!(captured, 1);

    let requests = session.drain_server_requests();
    assert_eq!(requests.len(), 1);
    let request_id = match &requests[0] {
        ServerRequest::FileChangeRequestApproval { request_id, params } => {
            assert_eq!(params.thread_id, "thread-live");
            assert_eq!(params.turn_id, "turn-live");
            assert_eq!(params.item_id, "item-file");
            request_id.clone()
        }
        other => panic!("unexpected server request: {other:?}"),
    };

    session
        .respond_typed(
            request_id,
            &FileChangeRequestApprovalResponse {
                decision: FileChangeApprovalDecision::Decline,
            },
        )
        .expect("response should be sent");

    server.join();
}

#[test]
fn turn_start_with_mad_max_policy_does_not_wait_for_approval_requests() {
    let server = TestServer::spawn(Scenario::MadMaxTurnStartNoApprovalPrompt);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("initialize should succeed");

    let response = session
        .request(
            "turn/start",
            Some(serde_json::json!({
                "threadId": "thread-live",
                "input": [
                    {
                        "type": "text",
                        "text": "Continue"
                    }
                ],
                "approvalPolicy": "never",
                "sandboxPolicy": {
                    "type": "dangerFullAccess"
                }
            })),
            Duration::from_secs(2),
        )
        .expect("turn start should complete without an approval loop");

    assert_eq!(response["turn"]["id"], serde_json::json!("turn-live"));
    server.join();
}

#[test]
fn poll_captures_dynamic_tool_call_and_returns_structured_response() {
    let server = TestServer::spawn(Scenario::DynamicToolCallRoundTrip);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("initialize should succeed");

    let captured = session
        .poll_server_notifications(Duration::from_secs(2))
        .expect("poll should succeed");
    assert_eq!(captured, 1);

    let requests = session.drain_server_requests();
    assert_eq!(requests.len(), 1);
    let request_id = match &requests[0] {
        ServerRequest::DynamicToolCall { request_id, params } => {
            assert_eq!(params.thread_id, "thread-live");
            assert_eq!(params.turn_id, "turn-live");
            assert_eq!(params.call_id, "call-live");
            assert_eq!(params.tool, "hunk.workspace_summary");
            assert_eq!(params.arguments, serde_json::json!({ "path": "." }));
            request_id.clone()
        }
        other => panic!("unexpected server request: {other:?}"),
    };

    session
        .respond_typed(
            request_id,
            &DynamicToolCallResponse {
                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                    text: "workspace summary".to_string(),
                }],
                success: true,
            },
        )
        .expect("response should be sent");

    server.join();
}

#[test]
fn request_user_input_round_trip_allows_follow_up_notifications() {
    let server = TestServer::spawn(Scenario::ToolRequestUserInputRoundTrip);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("initialize should succeed");

    let captured = session
        .poll_server_notifications(Duration::from_secs(2))
        .expect("poll should succeed");
    assert_eq!(captured, 1);

    let requests = session.drain_server_requests();
    assert_eq!(requests.len(), 1);
    let request_id = match &requests[0] {
        ServerRequest::ToolRequestUserInput { request_id, params } => {
            assert_eq!(params.thread_id, "thread-live");
            assert_eq!(params.turn_id, "turn-live");
            assert_eq!(params.item_id, "item-input");
            assert_eq!(params.questions.len(), 1);
            assert_eq!(params.questions[0].id, "approval_mode");
            request_id.clone()
        }
        other => panic!("unexpected server request: {other:?}"),
    };

    let answers = [(
        "approval_mode".to_string(),
        ToolRequestUserInputAnswer {
            answers: vec!["Apply now".to_string()],
        },
    )]
    .into_iter()
    .collect::<HashMap<_, _>>();
    session
        .respond_typed(request_id, &ToolRequestUserInputResponse { answers })
        .expect("response should be sent");

    let follow_up = session
        .poll_server_notifications(Duration::from_secs(2))
        .expect("follow-up poll should succeed");
    assert_eq!(follow_up, 1);
    let notifications = session.drain_server_notifications();
    assert_eq!(notifications.len(), 1);
    match &notifications[0] {
        ServerNotification::TurnDiffUpdated(notification) => {
            assert_eq!(notification.thread_id, "thread-live");
            assert_eq!(notification.turn_id, "turn-live");
        }
        other => panic!("unexpected follow-up notification: {other:?}"),
    }

    server.join();
}

#[test]
fn chatgpt_login_lifecycle_notifications_are_emitted() {
    let server = TestServer::spawn(Scenario::AccountLoginLifecycleNotifications);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("initialize should succeed");

    let response = session
        .request(
            "account/login/start",
            Some(serde_json::json!({
                "type": "chatgpt"
            })),
            Duration::from_secs(2),
        )
        .expect("account/login/start should succeed");
    assert_eq!(response["type"], serde_json::json!("chatgpt"));
    assert_eq!(response["loginId"], serde_json::json!("login-live"));
    assert_eq!(
        response["authUrl"],
        serde_json::json!("https://auth.example/login")
    );

    let captured = session
        .poll_server_notifications(Duration::from_secs(2))
        .expect("poll should succeed");
    assert_eq!(captured, 1);
    let additional = session
        .poll_server_notifications(Duration::from_secs(2))
        .expect("second poll should succeed");
    assert!(additional <= 1);

    let notifications = session.drain_server_notifications();
    assert!(!notifications.is_empty());
    assert!(notifications.iter().any(|notification| matches!(
        notification,
        ServerNotification::AccountLoginCompleted(completed)
            if completed.success && completed.login_id.as_deref() == Some("login-live")
    )));

    server.join();
}

#[test]
fn logout_and_account_updated_notifications_propagate() {
    let server = TestServer::spawn(Scenario::AccountLogoutAndUpdatedNotification);
    let endpoint = WebSocketEndpoint::loopback(server.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");

    session
        .initialize(InitializeOptions::default(), Duration::from_secs(2))
        .expect("initialize should succeed");

    session
        .request("account/logout", None, Duration::from_secs(2))
        .expect("account/logout should succeed");

    let captured = session
        .poll_server_notifications(Duration::from_secs(2))
        .expect("poll should succeed");
    assert_eq!(captured, 1);

    let notifications = session.drain_server_notifications();
    assert_eq!(notifications.len(), 1);
    assert!(matches!(
        notifications[0],
        ServerNotification::AccountUpdated(_)
    ));

    server.join();
}

#[derive(Clone)]
enum Scenario {
    InitializeSuccess,
    RejectBeforeInitialize,
    DuplicateInitialize,
    IdleNotification,
    CommandApprovalRequestRoundTrip,
    FileChangeApprovalRequestRoundTrip,
    MadMaxTurnStartNoApprovalPrompt,
    DynamicToolCallRoundTrip,
    ToolRequestUserInputRoundTrip,
    AccountLoginLifecycleNotifications,
    AccountLogoutAndUpdatedNotification,
    OverloadedThenSuccess {
        overload_attempts: usize,
        attempts: Arc<AtomicUsize>,
    },
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

            match scenario {
                Scenario::InitializeSuccess => run_initialize_success(&mut socket),
                Scenario::RejectBeforeInitialize => run_reject_before_initialize(&mut socket),
                Scenario::DuplicateInitialize => run_duplicate_initialize(&mut socket),
                Scenario::IdleNotification => run_idle_notification(&mut socket),
                Scenario::CommandApprovalRequestRoundTrip => {
                    run_command_approval_request_round_trip(&mut socket)
                }
                Scenario::FileChangeApprovalRequestRoundTrip => {
                    run_file_change_approval_request_round_trip(&mut socket)
                }
                Scenario::MadMaxTurnStartNoApprovalPrompt => {
                    run_mad_max_turn_start_no_approval_prompt(&mut socket)
                }
                Scenario::DynamicToolCallRoundTrip => run_dynamic_tool_call_round_trip(&mut socket),
                Scenario::ToolRequestUserInputRoundTrip => {
                    run_tool_request_user_input_round_trip(&mut socket)
                }
                Scenario::AccountLoginLifecycleNotifications => {
                    run_account_login_lifecycle_notifications(&mut socket)
                }
                Scenario::AccountLogoutAndUpdatedNotification => {
                    run_account_logout_and_updated_notification(&mut socket)
                }
                Scenario::OverloadedThenSuccess {
                    overload_attempts,
                    attempts,
                } => run_overloaded_then_success(&mut socket, overload_attempts, attempts),
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

fn run_initialize_success(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, "initialize");
    send_success_response(socket, initialize.id, initialize_response_json());

    expect_notification(socket, "initialized");
}

fn run_reject_before_initialize(socket: &mut WebSocket<TcpStream>) {
    let request = expect_request(socket, "thread/list");
    send_error_response(socket, request.id, -32002, "not initialized");
}

fn run_duplicate_initialize(socket: &mut WebSocket<TcpStream>) {
    let first = expect_request(socket, "initialize");
    send_success_response(socket, first.id, initialize_response_json());

    expect_notification(socket, "initialized");

    let second = expect_request(socket, "initialize");
    send_error_response(socket, second.id, -32010, "already initialized");
}

fn run_idle_notification(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, "initialize");
    send_success_response(socket, initialize.id, initialize_response_json());
    expect_notification(socket, "initialized");

    send_notification(
        socket,
        "turn/diff/updated",
        serde_json::json!({
            "threadId": "thread-live",
            "turnId": "turn-live",
            "diff": "diff --git a/a b/a"
        }),
    );
}

fn run_command_approval_request_round_trip(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, "initialize");
    send_success_response(socket, initialize.id, initialize_response_json());
    expect_notification(socket, "initialized");

    send_jsonrpc(
        socket,
        JSONRPCMessage::Request(JSONRPCRequest {
            id: RequestId::Integer(77),
            method: "item/commandExecution/requestApproval".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-live",
                "turnId": "turn-live",
                "itemId": "item-live",
                "approvalId": null,
                "reason": "run command",
                "command": "cargo test"
            })),
            trace: None,
        }),
    );

    let response = read_jsonrpc(socket);
    match response {
        JSONRPCMessage::Response(response) => {
            assert_eq!(response.id, RequestId::Integer(77));
            assert_eq!(
                response.result,
                serde_json::json!({
                    "decision": "accept"
                })
            );
        }
        other => panic!("expected approval response, got: {other:?}"),
    }
}

fn run_file_change_approval_request_round_trip(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, "initialize");
    send_success_response(socket, initialize.id, initialize_response_json());
    expect_notification(socket, "initialized");

    send_jsonrpc(
        socket,
        JSONRPCMessage::Request(JSONRPCRequest {
            id: RequestId::Integer(88),
            method: "item/fileChange/requestApproval".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-live",
                "turnId": "turn-live",
                "itemId": "item-file",
                "reason": "write access"
            })),
            trace: None,
        }),
    );

    let response = read_jsonrpc(socket);
    match response {
        JSONRPCMessage::Response(response) => {
            assert_eq!(response.id, RequestId::Integer(88));
            assert_eq!(
                response.result,
                serde_json::json!({
                    "decision": "decline"
                })
            );
        }
        other => panic!("expected file-change approval response, got: {other:?}"),
    }
}

fn run_mad_max_turn_start_no_approval_prompt(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, "initialize");
    send_success_response(socket, initialize.id, initialize_response_json());
    expect_notification(socket, "initialized");

    let request = expect_request(socket, "turn/start");
    let params = request.params.expect("turn/start params should exist");
    assert_eq!(params["approvalPolicy"], serde_json::json!("never"));
    assert_eq!(
        params["sandboxPolicy"],
        serde_json::json!({
            "type": "dangerFullAccess"
        })
    );

    send_success_response(
        socket,
        request.id,
        serde_json::json!({
            "turn": {
                "id": "turn-live",
                "status": "inProgress"
            }
        }),
    );
}

fn run_dynamic_tool_call_round_trip(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, "initialize");
    send_success_response(socket, initialize.id, initialize_response_json());
    expect_notification(socket, "initialized");

    send_jsonrpc(
        socket,
        JSONRPCMessage::Request(JSONRPCRequest {
            id: RequestId::Integer(111),
            method: "item/tool/call".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-live",
                "turnId": "turn-live",
                "callId": "call-live",
                "tool": "hunk.workspace_summary",
                "arguments": {
                    "path": "."
                }
            })),
            trace: None,
        }),
    );

    let response = read_jsonrpc(socket);
    match response {
        JSONRPCMessage::Response(response) => {
            assert_eq!(response.id, RequestId::Integer(111));
            assert_eq!(response.result["success"], serde_json::json!(true));
            assert_eq!(
                response.result["contentItems"][0]["text"],
                serde_json::json!("workspace summary")
            );
        }
        other => panic!("expected dynamic tool response, got: {other:?}"),
    }
}

fn run_tool_request_user_input_round_trip(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, "initialize");
    send_success_response(socket, initialize.id, initialize_response_json());
    expect_notification(socket, "initialized");

    send_jsonrpc(
        socket,
        JSONRPCMessage::Request(JSONRPCRequest {
            id: RequestId::Integer(112),
            method: "item/tool/requestUserInput".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-live",
                "turnId": "turn-live",
                "itemId": "item-input",
                "questions": [
                    {
                        "id": "approval_mode",
                        "header": "Execution mode",
                        "question": "How should we continue?",
                        "isOther": false,
                        "isSecret": false,
                        "options": [
                            {
                                "label": "Apply now",
                                "description": "Apply edits immediately."
                            },
                            {
                                "label": "Hold",
                                "description": "Prepare patch without applying."
                            }
                        ]
                    }
                ]
            })),
            trace: None,
        }),
    );

    let response = read_jsonrpc(socket);
    match response {
        JSONRPCMessage::Response(response) => {
            assert_eq!(response.id, RequestId::Integer(112));
            assert_eq!(
                response.result["answers"]["approval_mode"]["answers"],
                serde_json::json!(["Apply now"])
            );
        }
        other => panic!("expected tool request user input response, got: {other:?}"),
    }

    send_notification(
        socket,
        "turn/diff/updated",
        serde_json::json!({
            "threadId": "thread-live",
            "turnId": "turn-live",
            "diff": "diff --git a/src/main.rs b/src/main.rs"
        }),
    );
}

fn run_account_login_lifecycle_notifications(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, "initialize");
    send_success_response(socket, initialize.id, initialize_response_json());
    expect_notification(socket, "initialized");

    let login = expect_request(socket, "account/login/start");
    let params = login
        .params
        .expect("account/login/start params should exist");
    assert_eq!(params["type"], serde_json::json!("chatgpt"));
    send_success_response(
        socket,
        login.id,
        serde_json::json!({
            "type": "chatgpt",
            "loginId": "login-live",
            "authUrl": "https://auth.example/login"
        }),
    );

    send_notification(
        socket,
        "account/login/completed",
        serde_json::json!({
            "loginId": "login-live",
            "success": true,
            "error": null
        }),
    );
    send_notification(
        socket,
        "account/updated",
        serde_json::json!({
            "authMode": null,
            "planType": null
        }),
    );
}

fn run_account_logout_and_updated_notification(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, "initialize");
    send_success_response(socket, initialize.id, initialize_response_json());
    expect_notification(socket, "initialized");

    let logout = expect_request(socket, "account/logout");
    send_success_response(socket, logout.id, serde_json::json!({}));
    send_notification(
        socket,
        "account/updated",
        serde_json::json!({
            "authMode": null,
            "planType": null
        }),
    );
}

fn run_overloaded_then_success(
    socket: &mut WebSocket<TcpStream>,
    overload_attempts: usize,
    attempts: Arc<AtomicUsize>,
) {
    loop {
        let request = expect_request(socket, "model/list");
        let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;

        if attempt <= overload_attempts {
            send_error_response(socket, request.id, -32001, "server overloaded");
            continue;
        }

        send_success_response(socket, request.id, serde_json::json!({ "models": [] }));
        break;
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

fn initialize_response_json() -> serde_json::Value {
    serde_json::json!({
        "userAgent": "hunk-test-server",
        "codexHome": "/tmp/hunk-codex-test-home",
        "platformFamily": "windows",
        "platformOs": "windows"
    })
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

fn send_success_response(
    socket: &mut WebSocket<TcpStream>,
    id: RequestId,
    result: serde_json::Value,
) {
    let message = JSONRPCMessage::Response(JSONRPCResponse { id, result });
    send_jsonrpc(socket, message);
}

fn send_error_response(socket: &mut WebSocket<TcpStream>, id: RequestId, code: i64, message: &str) {
    let error = JSONRPCMessage::Error(JSONRPCError {
        id,
        error: JSONRPCErrorError {
            code,
            data: None,
            message: message.to_string(),
        },
    });
    send_jsonrpc(socket, error);
}

fn send_notification(socket: &mut WebSocket<TcpStream>, method: &str, params: serde_json::Value) {
    let notification = JSONRPCMessage::Notification(JSONRPCNotification {
        method: method.to_string(),
        params: Some(params),
    });
    send_jsonrpc(socket, notification);
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
