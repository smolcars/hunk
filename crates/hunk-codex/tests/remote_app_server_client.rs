use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use hunk_codex::app_server_client::AppServerClient;
use hunk_codex::app_server_client::AppServerEvent;
use hunk_codex::app_server_client::RemoteAppServerClient;
use tungstenite::Message;
use tungstenite::WebSocket;
use tungstenite::accept;

#[test]
fn remote_client_receives_idle_notification_event() {
    let server = TestServer::spawn(Scenario::IdleNotification);
    let mut client = RemoteAppServerClient::connect_loopback(server.port, Duration::from_secs(2))
        .expect("client should connect");

    let event = client
        .next_event(Duration::from_secs(2))
        .expect("event read should succeed")
        .expect("event should arrive");

    match event {
        AppServerEvent::ServerNotification(notification) => match notification {
            codex_app_server_protocol::ServerNotification::TurnDiffUpdated(notification) => {
                assert_eq!(notification.thread_id, "thread-live");
                assert_eq!(notification.turn_id, "turn-live");
                assert_eq!(notification.diff, "diff --git a/a b/a");
            }
            other => panic!("unexpected notification: {other:?}"),
        },
        other => panic!("unexpected event: {other:?}"),
    }

    server.join();
}

#[test]
#[ignore = "server-request roundtrip fixture needs an async websocket harness"]
fn remote_client_round_trips_server_request_responses() {
    let server = TestServer::spawn(Scenario::CommandApprovalRequestRoundTrip);
    let mut client = RemoteAppServerClient::connect_loopback(server.port, Duration::from_secs(2))
        .expect("client should connect");

    let event = client
        .next_event(Duration::from_secs(2))
        .expect("event read should succeed")
        .expect("event should arrive");

    let request_id = match event {
        AppServerEvent::ServerRequest(
            codex_app_server_protocol::ServerRequest::CommandExecutionRequestApproval {
                request_id,
                params,
            },
        ) => {
            assert_eq!(params.thread_id, "thread-live");
            assert_eq!(params.turn_id, "turn-live");
            assert_eq!(params.item_id, "item-live");
            request_id
        }
        other => panic!("unexpected event: {other:?}"),
    };

    client
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
fn remote_client_surfaces_disconnect_events() {
    let server = TestServer::spawn(Scenario::DisconnectAfterInitialize);
    let mut client = RemoteAppServerClient::connect_loopback(server.port, Duration::from_secs(2))
        .expect("client should connect");

    let event = client
        .next_event(Duration::from_secs(2))
        .expect("event read should succeed")
        .expect("disconnect event should arrive");

    match event {
        AppServerEvent::Disconnected { message } => {
            assert!(message.contains("disconnected") || message.contains("closed"));
        }
        other => panic!("unexpected event: {other:?}"),
    }

    server.join();
}

#[derive(Clone)]
enum Scenario {
    IdleNotification,
    CommandApprovalRequestRoundTrip,
    DisconnectAfterInitialize,
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
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("read timeout should be set");
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .expect("write timeout should be set");
            let mut socket = accept(stream).expect("websocket handshake should succeed");

            run_initialize_handshake(&mut socket);

            match scenario {
                Scenario::IdleNotification => run_idle_notification(&mut socket),
                Scenario::CommandApprovalRequestRoundTrip => {
                    run_command_approval_request_round_trip(&mut socket)
                }
                Scenario::DisconnectAfterInitialize => run_disconnect_after_initialize(&mut socket),
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
    let initialize = expect_request(socket, "initialize");
    assert!(initialize.params.is_some());
    send_response(
        socket,
        initialize.id,
        serde_json::json!({
            "userAgent": "hunk-test-server"
        }),
    );
    expect_notification(socket, "initialized");
}

fn run_idle_notification(socket: &mut WebSocket<TcpStream>) {
    send_notification(
        socket,
        "turn/diff/updated",
        serde_json::json!({
            "threadId": "thread-live",
            "turnId": "turn-live",
            "diff": "diff --git a/a b/a"
        }),
    );
    thread::sleep(Duration::from_millis(100));
    let _ = socket.close(None);
}

fn run_command_approval_request_round_trip(socket: &mut WebSocket<TcpStream>) {
    send_request(
        socket,
        RequestId::String("approval-1".to_string()),
        "item/commandExecution/requestApproval",
        serde_json::json!({
            "threadId": "thread-live",
            "turnId": "turn-live",
            "itemId": "item-live",
            "command": "echo hi",
            "reason": "Need approval",
            "cwd": null
        }),
    );

    let response = expect_response(socket);
    assert_eq!(response.id, RequestId::String("approval-1".to_string()));
    assert_eq!(response.result["decision"], serde_json::json!("accept"));
    thread::sleep(Duration::from_millis(100));
    let _ = socket.close(None);
}

fn run_disconnect_after_initialize(socket: &mut WebSocket<TcpStream>) {
    socket.close(None).expect("socket should close");
}

fn expect_request(socket: &mut WebSocket<TcpStream>, method: &str) -> JSONRPCRequest {
    match expect_message(socket) {
        JSONRPCMessage::Request(request) => {
            assert_eq!(request.method, method);
            request
        }
        other => panic!("expected request `{method}`, got {other:?}"),
    }
}

fn expect_notification(socket: &mut WebSocket<TcpStream>, method: &str) -> JSONRPCNotification {
    match expect_message(socket) {
        JSONRPCMessage::Notification(notification) => {
            assert_eq!(notification.method, method);
            notification
        }
        other => panic!("expected notification `{method}`, got {other:?}"),
    }
}

fn expect_response(socket: &mut WebSocket<TcpStream>) -> JSONRPCResponse {
    match expect_message(socket) {
        JSONRPCMessage::Response(response) => response,
        other => panic!("expected response, got {other:?}"),
    }
}

fn expect_message(socket: &mut WebSocket<TcpStream>) -> JSONRPCMessage {
    let message = socket.read().expect("message should be readable");
    match message {
        Message::Text(payload) => serde_json::from_str(&payload).expect("valid json-rpc text"),
        Message::Binary(payload) => {
            serde_json::from_slice(&payload).expect("valid json-rpc binary")
        }
        other => panic!("unexpected websocket message: {other:?}"),
    }
}

fn send_response(socket: &mut WebSocket<TcpStream>, id: RequestId, result: serde_json::Value) {
    send_message(
        socket,
        JSONRPCMessage::Response(JSONRPCResponse { id, result }),
    );
}

fn send_request(
    socket: &mut WebSocket<TcpStream>,
    id: RequestId,
    method: &str,
    params: serde_json::Value,
) {
    send_message(
        socket,
        JSONRPCMessage::Request(JSONRPCRequest {
            id,
            method: method.to_string(),
            params: Some(params),
            trace: None,
        }),
    );
}

fn send_notification(socket: &mut WebSocket<TcpStream>, method: &str, params: serde_json::Value) {
    send_message(
        socket,
        JSONRPCMessage::Notification(JSONRPCNotification {
            method: method.to_string(),
            params: Some(params),
        }),
    );
}

fn send_message(socket: &mut WebSocket<TcpStream>, message: JSONRPCMessage) {
    let payload = serde_json::to_string(&message).expect("json-rpc should serialize");
    socket
        .send(Message::Text(payload.into()))
        .expect("message should be sent");
}
