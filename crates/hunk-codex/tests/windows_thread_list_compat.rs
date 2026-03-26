#![cfg(windows)]

use std::net::TcpListener;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadStatus;
use hunk_codex::api;
use hunk_codex::api::InitializeOptions;
use hunk_codex::threads::ThreadService;
use hunk_codex::ws_client::JsonRpcSession;
use hunk_codex::ws_client::WebSocketEndpoint;
use serde::Serialize;
use serde_json::Value;
use tungstenite::Message;
use tungstenite::WebSocket;
use tungstenite::accept;

const TIMEOUT: Duration = Duration::from_secs(2);
const WINDOWS_WORKSPACE_CWD: &str = r"C:\Users\nites\Documents\hunk";
const WINDOWS_WORKSPACE_CWD_VERBATIM: &str = r"\\?\C:\Users\nites\Documents\hunk";

#[test]
fn list_threads_loads_legacy_windows_verbatim_cwd_threads() {
    let server = TestServer::spawn(TestScenario::AliasFallback);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(PathBuf::from(WINDOWS_WORKSPACE_CWD));

    let response = service
        .list_threads(&mut session, None, Some(50), TIMEOUT)
        .expect("thread/list should succeed across Windows cwd aliases");

    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].id, "legacy-thread");
    assert_eq!(
        service
            .state()
            .threads
            .get("legacy-thread")
            .expect("legacy thread should be ingested")
            .cwd,
        WINDOWS_WORKSPACE_CWD
    );

    server.join();
}

#[test]
fn list_threads_merges_windows_alias_results_by_newest_updated_at() {
    let server = TestServer::spawn(TestScenario::AliasMergeSorted);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(PathBuf::from(WINDOWS_WORKSPACE_CWD));

    let response = service
        .list_threads(&mut session, None, Some(1), TIMEOUT)
        .expect("thread/list should merge and sort Windows cwd aliases");

    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].id, "legacy-newest-thread");
    assert_eq!(
        service
            .state()
            .threads
            .get("legacy-newest-thread")
            .expect("legacy thread should be ingested")
            .cwd,
        WINDOWS_WORKSPACE_CWD
    );
    assert!(
        service
            .state()
            .threads
            .contains_key("normalized-older-thread"),
        "merged responses should still ingest matching normalized threads"
    );

    server.join();
}

struct TestServer {
    port: u16,
    join: thread::JoinHandle<()>,
}

#[derive(Clone, Copy)]
enum TestScenario {
    AliasFallback,
    AliasMergeSorted,
}

impl TestServer {
    fn spawn(scenario: TestScenario) -> Self {
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
                TestScenario::AliasFallback => run_thread_list_alias_fallback(&mut socket),
                TestScenario::AliasMergeSorted => run_thread_list_alias_merge_sorted(&mut socket),
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

fn connect_initialized_session(port: u16) -> JsonRpcSession {
    let endpoint = WebSocketEndpoint::loopback(port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("connect should succeed");
    session
        .initialize(InitializeOptions::default(), TIMEOUT)
        .expect("initialize should succeed");
    session
}

fn run_initialize_handshake(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, api::method::INITIALIZE);
    send_success_response(
        socket,
        initialize.id,
        serde_json::json!({
            "userAgent": "hunk-windows-thread-list-compat-test",
            "codexHome": "/tmp/hunk-codex-test-home",
            "platformFamily": "windows",
            "platformOs": "windows",
        }),
    );
    expect_notification(socket, api::method::INITIALIZED);
}

fn run_thread_list_alias_fallback(socket: &mut WebSocket<TcpStream>) {
    let first = expect_request(socket, api::method::THREAD_LIST);
    let first_params = first.params.expect("first thread/list params should exist");
    assert_eq!(
        param_string(&first_params, "cwd"),
        Some(WINDOWS_WORKSPACE_CWD.to_string())
    );
    send_typed_success_response(
        socket,
        first.id,
        &ThreadListResponse {
            data: Vec::new(),
            next_cursor: None,
        },
    );

    let second = expect_request(socket, api::method::THREAD_LIST);
    let second_params = second
        .params
        .expect("second thread/list params should exist");
    assert_eq!(
        param_string(&second_params, "cwd"),
        Some(WINDOWS_WORKSPACE_CWD_VERBATIM.to_string())
    );
    send_typed_success_response(
        socket,
        second.id,
        &ThreadListResponse {
            data: vec![thread("legacy-thread", WINDOWS_WORKSPACE_CWD_VERBATIM)],
            next_cursor: None,
        },
    );
}

fn run_thread_list_alias_merge_sorted(socket: &mut WebSocket<TcpStream>) {
    let first = expect_request(socket, api::method::THREAD_LIST);
    let first_params = first.params.expect("first thread/list params should exist");
    assert_eq!(
        param_string(&first_params, "cwd"),
        Some(WINDOWS_WORKSPACE_CWD.to_string())
    );
    assert_eq!(param_u64(&first_params, "limit"), Some(1));
    send_typed_success_response(
        socket,
        first.id,
        &ThreadListResponse {
            data: vec![thread_with_timestamps(
                "normalized-older-thread",
                WINDOWS_WORKSPACE_CWD,
                1,
                10,
            )],
            next_cursor: None,
        },
    );

    let second = expect_request(socket, api::method::THREAD_LIST);
    let second_params = second
        .params
        .expect("second thread/list params should exist");
    assert_eq!(
        param_string(&second_params, "cwd"),
        Some(WINDOWS_WORKSPACE_CWD_VERBATIM.to_string())
    );
    assert_eq!(param_u64(&second_params, "limit"), Some(1));
    send_typed_success_response(
        socket,
        second.id,
        &ThreadListResponse {
            data: vec![thread_with_timestamps(
                "legacy-newest-thread",
                WINDOWS_WORKSPACE_CWD_VERBATIM,
                2,
                20,
            )],
            next_cursor: None,
        },
    );
}

fn expect_request(socket: &mut WebSocket<TcpStream>, method: &str) -> JSONRPCRequest {
    let message = read_message(socket);
    let JSONRPCMessage::Request(request) = message else {
        panic!("expected request for method {method}");
    };
    assert_eq!(request.method, method);
    request
}

fn expect_notification(socket: &mut WebSocket<TcpStream>, method: &str) -> JSONRPCNotification {
    let message = read_message(socket);
    let JSONRPCMessage::Notification(notification) = message else {
        panic!("expected notification for method {method}");
    };
    assert_eq!(notification.method, method);
    notification
}

fn read_message(socket: &mut WebSocket<TcpStream>) -> JSONRPCMessage {
    let message = socket.read().expect("socket read should succeed");
    let text = message.into_text().expect("message should be text");
    serde_json::from_str(text.as_ref()).expect("json-rpc message should decode")
}

fn send_success_response(socket: &mut WebSocket<TcpStream>, id: RequestId, result: Value) {
    let response = JSONRPCResponse { id, result };
    let encoded = serde_json::to_string(&response).expect("response should encode");
    socket
        .send(Message::Text(encoded.into()))
        .expect("response should send");
}

fn send_typed_success_response<T>(socket: &mut WebSocket<TcpStream>, id: RequestId, result: &T)
where
    T: Serialize,
{
    let value = serde_json::to_value(result).expect("typed response should convert");
    send_success_response(socket, id, value);
}

fn param_string(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn param_u64(params: &Value, key: &str) -> Option<u64> {
    params.get(key).and_then(Value::as_u64)
}

fn thread(id: &str, cwd: &str) -> Thread {
    thread_with_timestamps(id, cwd, 1, 2)
}

fn thread_with_timestamps(id: &str, cwd: &str, created_at: i64, updated_at: i64) -> Thread {
    Thread {
        id: id.to_string(),
        preview: format!("preview-{id}"),
        ephemeral: false,
        model_provider: "openai".to_string(),
        created_at,
        updated_at,
        status: ThreadStatus::Idle,
        path: Some(format!(r"C:\tmp\.codex\threads\{id}.jsonl").into()),
        cwd: cwd.into(),
        cli_version: "0.1.0".to_string(),
        source: SessionSource::AppServer,
        agent_nickname: None,
        agent_role: None,
        git_info: None,
        name: Some(format!("Thread {id}")),
        turns: Vec::new(),
    }
}
