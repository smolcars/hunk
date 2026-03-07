use std::env;
use std::fs;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

use hunk_codex::host::HostConfig;
use hunk_codex::host::HostLifecycleState;
use hunk_codex::host::HostRuntime;
#[cfg(unix)]
use hunk_codex::host::cleanup_tracked_hosts_for_shutdown;
use tempfile::TempDir;
use tungstenite::Message;
use tungstenite::accept;

const HELPER_ENV: &str = "HUNK_CODEX_TEST_HELPER";
const HELPER_PORT_ENV: &str = "HUNK_CODEX_FIXTURE_PORT";
const HELPER_MODE_ENV: &str = "HUNK_CODEX_TEST_HELPER_MODE";
const HELPER_ORPHAN_PID_PATH_ENV: &str = "HUNK_CODEX_TEST_HELPER_ORPHAN_PID_PATH";
const HELPER_MODE_ORPHAN_STDERR: &str = "orphan-stderr";

#[test]
fn fixture_server_entrypoint() {
    if env::var(HELPER_ENV).ok().as_deref() != Some("1") {
        return;
    }

    if env::var(HELPER_MODE_ENV).ok().as_deref() == Some(HELPER_MODE_ORPHAN_STDERR) {
        run_fixture_orphan_stderr_server();
        return;
    }

    run_fixture_websocket_server();
}

#[test]
fn default_codex_arguments_use_websocket_listen_url() {
    let _guard = host_runtime_test_guard();
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
    let _guard = host_runtime_test_guard();
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
    let _guard = host_runtime_test_guard();
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
    let _guard = host_runtime_test_guard();
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

#[cfg(unix)]
#[test]
fn stop_returns_when_helper_leaves_descendant_holding_stderr() {
    let _guard = host_runtime_test_guard();
    let setup = TestSetup::new_with_mode(Some(HELPER_MODE_ORPHAN_STDERR));
    let mut runtime = HostRuntime::new(setup.host_config());

    runtime
        .start(Duration::from_secs(5))
        .expect("host should start with orphan fixture");
    let started_at = Instant::now();
    runtime.stop().expect("stop should succeed");
    let elapsed = started_at.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "stop should not block waiting for stderr reader; elapsed={elapsed:?}"
    );
    setup.kill_orphan_if_present();
}

#[cfg(unix)]
#[test]
fn shutdown_cleanup_terminates_tracked_host_processes() {
    let _guard = host_runtime_test_guard();
    let setup = TestSetup::new();
    let mut runtime = HostRuntime::new(setup.host_config());

    runtime
        .start(Duration::from_secs(5))
        .expect("host should start");
    let process_id = runtime.pid().expect("host pid should be available");

    std::mem::forget(runtime);
    assert!(process_group_exists(process_id));

    cleanup_tracked_hosts_for_shutdown();

    assert!(
        wait_for_process_group_exit(process_id, Duration::from_secs(5)),
        "tracked host process group should exit after shutdown cleanup"
    );
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

#[allow(clippy::zombie_processes)]
fn run_fixture_orphan_stderr_server() {
    let port: u16 = env::var(HELPER_PORT_ENV)
        .expect("helper port env must be set")
        .parse()
        .expect("helper port must be valid u16");
    let orphan_pid_path =
        env::var(HELPER_ORPHAN_PID_PATH_ENV).expect("helper orphan pid path env must be set");
    let _listener = TcpListener::bind(("127.0.0.1", port)).expect("fixture must bind port");

    let child = Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; while true; do sleep 1; done")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("orphan child should spawn");
    fs::write(orphan_pid_path, child.id().to_string()).expect("orphan pid file should be written");

    loop {
        thread::sleep(Duration::from_secs(1));
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
    helper_mode: Option<String>,
    orphan_pid_path: PathBuf,
}

impl TestSetup {
    fn new() -> Self {
        Self::new_with_mode(None)
    }

    fn new_with_mode(helper_mode: Option<&str>) -> Self {
        let test_executable =
            std::env::current_exe().expect("current test executable should be available");
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let port = free_port();
        let orphan_pid_path = temp_dir.path().join("orphan.pid");

        Self {
            test_executable,
            temp_dir,
            port,
            helper_mode: helper_mode.map(str::to_string),
            orphan_pid_path,
        }
    }

    fn host_config(&self) -> HostConfig {
        let workspace_dir = self.temp_dir.path().join("workspace");
        let codex_home = self.temp_dir.path().join(".codex");

        fs::create_dir_all(&workspace_dir).expect("workspace dir must exist");
        fs::create_dir_all(&codex_home).expect("codex home dir must exist");
        let mut environment = vec![
            (HELPER_ENV.to_string(), "1".to_string()),
            (HELPER_PORT_ENV.to_string(), self.port.to_string()),
        ];
        if let Some(mode) = self.helper_mode.as_ref() {
            environment.push((HELPER_MODE_ENV.to_string(), mode.clone()));
            environment.push((
                HELPER_ORPHAN_PID_PATH_ENV.to_string(),
                self.orphan_pid_path.display().to_string(),
            ));
        }

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
            environment,
        }
    }

    #[cfg(unix)]
    fn kill_orphan_if_present(&self) {
        let Ok(pid) = fs::read_to_string(&self.orphan_pid_path) else {
            return;
        };
        let pid = pid.trim();
        if pid.is_empty() {
            return;
        }
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(pid)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("port probe bind must succeed")
        .local_addr()
        .expect("probe local addr must exist")
        .port()
}

fn host_runtime_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static HOST_RUNTIME_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    HOST_RUNTIME_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("host runtime test mutex poisoned")
}

#[cfg(unix)]
fn process_group_exists(process_id: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg("--")
        .arg(format!("-{process_id}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn wait_for_process_group_exit(process_id: u32, timeout: Duration) -> bool {
    let started_at = Instant::now();
    loop {
        if !process_group_exists(process_id) {
            return true;
        }
        if started_at.elapsed() >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(25));
    }
}
