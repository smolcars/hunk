use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

use hunk_codex::api::InitializeOptions;
use hunk_codex::app_server_client::AppServerClient;
use hunk_codex::app_server_client::RemoteAppServerClient;
use hunk_codex::host::HostConfig;
use hunk_codex::host::HostLifecycleState;
use hunk_codex::host::HostRuntime;
use hunk_codex::threads::ThreadService;
use hunk_codex::ws_client::JsonRpcSession;
use hunk_codex::ws_client::WebSocketEndpoint;
use tempfile::TempDir;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[test]
#[ignore = "Requires a real bundled Codex runtime binary at assets/codex-runtime/macos/codex."]
fn bundled_macos_runtime_bootstraps_and_initializes() {
    if !cfg!(target_os = "macos") {
        return;
    }

    let runtime_binary = bundled_macos_runtime_path();
    assert!(
        runtime_binary.is_file(),
        "missing bundled runtime binary: {}",
        runtime_binary.display()
    );

    let setup = SmokeSetup::new(runtime_binary);
    let mut host = HostRuntime::new(setup.host_config());

    host.start(STARTUP_TIMEOUT)
        .expect("bundled runtime should start");
    assert_eq!(host.state(), HostLifecycleState::Ready);

    let endpoint = WebSocketEndpoint::loopback(setup.port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");
    let initialize = session
        .initialize(InitializeOptions::default(), REQUEST_TIMEOUT)
        .expect("initialize handshake should succeed");
    assert!(
        !initialize.user_agent.trim().is_empty(),
        "initialize response should include user agent"
    );

    host.stop().expect("runtime should stop");
}

#[test]
#[ignore = "Requires a real bundled Codex runtime binary at assets/codex-runtime/macos/codex."]
fn bundled_macos_remote_client_bootstraps_and_lists_threads() {
    if !cfg!(target_os = "macos") {
        return;
    }

    let runtime_binary = bundled_macos_runtime_path();
    assert!(
        runtime_binary.is_file(),
        "missing bundled runtime binary: {}",
        runtime_binary.display()
    );

    let setup = SmokeSetup::new(runtime_binary);
    let mut host = HostRuntime::new(setup.host_config());

    eprintln!("starting host");
    host.start(STARTUP_TIMEOUT)
        .expect("bundled runtime should start");
    assert_eq!(host.state(), HostLifecycleState::Ready);

    eprintln!("connecting remote client");
    let mut session =
        RemoteAppServerClient::connect_loopback(setup.port, REQUEST_TIMEOUT)
            .expect("remote client should connect");
    eprintln!("remote client connected");
    let mut service = ThreadService::new(setup.workspace.clone());
    eprintln!("listing threads");
    let response = service
        .list_threads(&mut session, None, Some(20), REQUEST_TIMEOUT)
        .expect("thread/list should succeed");

    assert!(response.data.is_empty());
    eprintln!("shutting down remote client");
    session.shutdown().expect("remote client should shut down");
    host.stop().expect("runtime should stop");
}

fn bundled_macos_runtime_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/codex-runtime/macos/codex")
}

#[derive(Debug)]
struct SmokeSetup {
    runtime_binary: PathBuf,
    _temp_dir: TempDir,
    workspace: PathBuf,
    codex_home: PathBuf,
    port: u16,
}

impl SmokeSetup {
    fn new(runtime_binary: PathBuf) -> Self {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace = temp_dir.path().join("workspace");
        let codex_home = temp_dir.path().join(".codex");
        std::fs::create_dir_all(&workspace).expect("workspace dir should exist");
        std::fs::create_dir_all(&codex_home).expect("codex home dir should exist");
        let port = free_port();

        Self {
            runtime_binary,
            _temp_dir: temp_dir,
            workspace,
            codex_home,
            port,
        }
    }

    fn host_config(&self) -> HostConfig {
        HostConfig::codex_app_server(
            self.runtime_binary.clone(),
            self.workspace.clone(),
            self.codex_home.clone(),
            self.port,
        )
    }
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("port probe should bind")
        .local_addr()
        .expect("probe should have local address")
        .port()
}
