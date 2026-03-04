use std::env;
use std::fs;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use hunk_codex::host::HostConfig;
use hunk_codex::host::HostLifecycleState;
use hunk_codex::host::HostRuntime;
use tempfile::TempDir;
use tungstenite::Message;
use tungstenite::accept;

const HELPER_ENV: &str = "HUNK_CODEX_TEST_HELPER";
const HELPER_PORT_ENV: &str = "HUNK_CODEX_FIXTURE_PORT";

#[test]
fn fixture_server_entrypoint() {
    if env::var(HELPER_ENV).ok().as_deref() != Some("1") {
        return;
    }

    run_fixture_websocket_server();
}

#[test]
fn default_codex_arguments_use_websocket_listen_url() {
    let args = HostConfig::default_codex_arguments(4455);
    assert_eq!(
        args,
        vec![
            "app-server".to_string(),
            "--listen".to_string(),
            "ws://127.0.0.1:4455".to_string(),
        ]
    );
}

#[test]
fn host_boots_and_accepts_websocket_client() {
    let setup = TestSetup::new();
    let mut runtime = HostRuntime::new(setup.host_config());

    runtime
        .start(Duration::from_secs(5))
        .expect("host should start");

    assert_eq!(runtime.state(), HostLifecycleState::Ready);
    assert!(runtime.pid().is_some());

    let (mut socket, _) = tungstenite::connect(runtime.config().websocket_url())
        .expect("websocket connect should succeed");
    socket
        .send(Message::Text("ping".into()))
        .expect("send should succeed");

    let response = socket.read().expect("read should succeed");
    let text = response
        .into_text()
        .expect("response should be text message");
    assert_eq!(text.to_string(), "ping");

    runtime.stop().expect("stop should succeed");
    assert_eq!(runtime.state(), HostLifecycleState::Stopped);
}

#[test]
fn host_reconnects_after_forced_restart() {
    let setup = TestSetup::new();
    let mut runtime = HostRuntime::new(setup.host_config());

    runtime
        .start(Duration::from_secs(5))
        .expect("host should start");

    runtime.force_kill().expect("force kill should succeed");
    assert_eq!(runtime.state(), HostLifecycleState::Failed);

    runtime
        .ensure_running(Duration::from_secs(5))
        .expect("runtime should reconnect");

    assert_eq!(runtime.state(), HostLifecycleState::Ready);

    let (mut socket, _) = tungstenite::connect(runtime.config().websocket_url())
        .expect("websocket connect should succeed after reconnect");
    socket
        .send(Message::Text("pong".into()))
        .expect("send should succeed");

    let response = socket.read().expect("read should succeed");
    let text = response
        .into_text()
        .expect("response should be text message");
    assert_eq!(text.to_string(), "pong");

    runtime.stop().expect("stop should succeed");
}

#[test]
fn graceful_shutdown_leaves_no_running_process() {
    let setup = TestSetup::new();
    let mut runtime = HostRuntime::new(setup.host_config());

    runtime
        .start(Duration::from_secs(5))
        .expect("host should start");
    runtime.stop().expect("stop should succeed");

    assert_eq!(runtime.state(), HostLifecycleState::Stopped);
    assert!(runtime.pid().is_none());

    let connect_result = tungstenite::connect(runtime.config().websocket_url());
    assert!(connect_result.is_err());
}

fn run_fixture_websocket_server() {
    let port: u16 = env::var(HELPER_PORT_ENV)
        .expect("helper port env must be set")
        .parse()
        .expect("helper port must be valid u16");

    let listener = TcpListener::bind(("127.0.0.1", port)).expect("fixture must bind port");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || handle_connection(stream));
            }
            Err(error) => {
                panic!("fixture listener error: {error}");
            }
        }
    }
}

fn handle_connection(stream: TcpStream) {
    let mut websocket = match accept(stream) {
        Ok(socket) => socket,
        Err(_) => {
            return;
        }
    };

    loop {
        let message = match websocket.read() {
            Ok(message) => message,
            Err(_) => {
                return;
            }
        };

        match message {
            Message::Text(text) => {
                if websocket.send(Message::Text(text)).is_err() {
                    return;
                }
            }
            Message::Binary(bytes) => {
                if websocket.send(Message::Binary(bytes)).is_err() {
                    return;
                }
            }
            Message::Close(_) => {
                let _ = websocket.close(None);
                return;
            }
            Message::Ping(payload) => {
                if websocket.send(Message::Pong(payload)).is_err() {
                    return;
                }
            }
            Message::Pong(_) => {}
            Message::Frame(_) => {}
        }
    }
}

#[derive(Debug)]
struct TestSetup {
    test_executable: PathBuf,
    temp_dir: TempDir,
    port: u16,
}

impl TestSetup {
    fn new() -> Self {
        let test_executable =
            std::env::current_exe().expect("current test executable should be available");
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let port = free_port();

        Self {
            test_executable,
            temp_dir,
            port,
        }
    }

    fn host_config(&self) -> HostConfig {
        let workspace_dir = self.temp_dir.path().join("workspace");
        let codex_home = self.temp_dir.path().join(".codex");

        fs::create_dir_all(&workspace_dir).expect("workspace dir must exist");
        fs::create_dir_all(&codex_home).expect("codex home dir must exist");

        HostConfig {
            executable_path: self.test_executable.clone(),
            working_directory: workspace_dir,
            codex_home,
            port: self.port,
            arguments: vec![
                "--exact".to_string(),
                "fixture_server_entrypoint".to_string(),
                "--nocapture".to_string(),
            ],
            environment: vec![
                (HELPER_ENV.to_string(), "1".to_string()),
                (HELPER_PORT_ENV.to_string(), self.port.to_string()),
            ],
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("port probe bind must succeed")
        .local_addr()
        .expect("probe local addr must exist")
        .port()
}
