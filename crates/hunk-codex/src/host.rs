use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::net::SocketAddr;
use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

use tracing::debug;
use tracing::warn;

use crate::errors::CodexIntegrationError;
use crate::errors::Result;

const STDERR_BUFFER_LIMIT: usize = 256;
const READY_PROBE_INTERVAL: Duration = Duration::from_millis(75);
const READY_PROBE_CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
#[cfg(unix)]
const STOP_TERM_GRACE_TIMEOUT: Duration = Duration::from_millis(750);
#[cfg(unix)]
const STOP_TERM_POLL_INTERVAL: Duration = Duration::from_millis(25);
const STDERR_READER_JOIN_TIMEOUT: Duration = Duration::from_millis(750);
static TRACKED_HOST_PROCESS_IDS: OnceLock<Mutex<BTreeSet<u32>>> = OnceLock::new();
static SHARED_HOSTS: OnceLock<Mutex<BTreeMap<SharedHostKey, SharedHostEntry>>> = OnceLock::new();
static BLOCK_NEW_HOST_STARTS_FOR_SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostLifecycleState {
    Starting,
    Ready,
    Reconnecting,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostConfig {
    pub executable_path: PathBuf,
    pub working_directory: PathBuf,
    pub codex_home: PathBuf,
    pub port: u16,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub cleared_environment: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SharedHostKey {
    executable_path: PathBuf,
    codex_home: PathBuf,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    cleared_environment: Vec<String>,
}

impl SharedHostKey {
    fn from_config(config: &HostConfig) -> Self {
        let mut environment = config.environment.clone();
        environment.sort();
        let mut cleared_environment = config.cleared_environment.clone();
        cleared_environment.sort();
        Self {
            executable_path: config.executable_path.clone(),
            codex_home: config.codex_home.clone(),
            arguments: normalized_shared_host_arguments(config),
            environment,
            cleared_environment,
        }
    }
}

fn normalized_shared_host_arguments(config: &HostConfig) -> Vec<String> {
    let mut normalized = Vec::with_capacity(config.arguments.len());
    let mut index = 0usize;

    while index < config.arguments.len() {
        let argument = &config.arguments[index];
        normalized.push(argument.clone());

        if argument == "--listen"
            && let Some(value) = config.arguments.get(index + 1)
        {
            normalized.push(normalize_listen_argument(value.as_str()));
            index += 2;
            continue;
        }

        index += 1;
    }

    normalized
}

fn normalize_listen_argument(argument: &str) -> String {
    if argument
        .strip_prefix("ws://127.0.0.1:")
        .and_then(|port| port.parse::<u16>().ok())
        .is_some()
    {
        return "ws://127.0.0.1:<loopback-port>".to_string();
    }

    argument.to_string()
}

#[derive(Debug)]
struct SharedHostEntry {
    lease_count: usize,
    runtime: HostRuntime,
}

#[derive(Debug)]
pub struct SharedHostLease {
    key: SharedHostKey,
    fallback_port: u16,
}

impl SharedHostLease {
    pub fn acquire(config: HostConfig, timeout: Duration) -> Result<Self> {
        if host_shutdown_in_progress() {
            return Err(CodexIntegrationError::WebSocketTransport(
                "codex host shutdown is already in progress".to_string(),
            ));
        }

        let key = SharedHostKey::from_config(&config);
        let mut guard = shared_hosts()
            .lock()
            .expect("shared host registry mutex poisoned");

        if let Some(entry) = guard.get_mut(&key) {
            if entry.runtime.ensure_running(timeout).is_err() {
                let mut replacement = HostRuntime::new(config);
                replacement.start(timeout)?;
                entry.runtime = replacement;
            }
            entry.lease_count = entry.lease_count.saturating_add(1);
            return Ok(Self {
                key,
                fallback_port: entry.runtime.config().port,
            });
        }

        let mut runtime = HostRuntime::new(config);
        runtime.start(timeout)?;
        let port = runtime.config().port;
        guard.insert(
            key.clone(),
            SharedHostEntry {
                lease_count: 1,
                runtime,
            },
        );
        Ok(Self {
            key,
            fallback_port: port,
        })
    }

    pub fn port(&self) -> u16 {
        shared_hosts()
            .lock()
            .expect("shared host registry mutex poisoned")
            .get(&self.key)
            .map(|entry| entry.runtime.config().port)
            .unwrap_or(self.fallback_port)
    }

    pub fn pid(&self) -> Option<u32> {
        let mut guard = shared_hosts()
            .lock()
            .expect("shared host registry mutex poisoned");
        guard
            .get_mut(&self.key)
            .and_then(|entry| entry.runtime.pid())
    }

    pub fn ensure_running(&self, timeout: Duration) -> Result<()> {
        let mut guard = shared_hosts()
            .lock()
            .expect("shared host registry mutex poisoned");
        let Some(entry) = guard.get_mut(&self.key) else {
            return Err(CodexIntegrationError::WebSocketTransport(
                "shared codex host lease was released unexpectedly".to_string(),
            ));
        };
        entry.runtime.ensure_running(timeout)
    }
}

impl HostConfig {
    pub fn codex_app_server(
        executable_path: PathBuf,
        working_directory: PathBuf,
        codex_home: PathBuf,
        port: u16,
    ) -> Self {
        Self {
            executable_path,
            working_directory,
            codex_home,
            port,
            arguments: Self::default_codex_arguments(port),
            environment: Vec::new(),
            cleared_environment: Self::default_cleared_environment(),
        }
    }

    fn default_cleared_environment() -> Vec<String> {
        #[cfg(target_os = "linux")]
        {
            ["APPDIR", "APPIMAGE", "ARGV0", "LD_LIBRARY_PATH", "OWD"]
                .into_iter()
                .map(str::to_string)
                .collect()
        }
        #[cfg(not(target_os = "linux"))]
        {
            Vec::new()
        }
    }

    pub fn default_codex_arguments(port: u16) -> Vec<String> {
        let listen_url = format!("ws://127.0.0.1:{port}");
        vec!["app-server".to_string(), "--listen".to_string(), listen_url]
    }

    pub fn websocket_url(&self) -> String {
        format!("ws://127.0.0.1:{}/", self.port)
    }

    pub fn build_command(&self) -> Command {
        let mut command = Command::new(&self.executable_path);
        command.args(&self.arguments);
        command.current_dir(&self.working_directory);
        command.env("CODEX_HOME", &self.codex_home);
        for key in &self.cleared_environment {
            command.env_remove(key);
        }
        for (key, value) in &self.environment {
            command.env(key, value);
        }
        configure_background_command(&mut command);
        command
    }
}

#[derive(Debug)]
pub struct HostRuntime {
    config: HostConfig,
    state: HostLifecycleState,
    child: Option<Child>,
    stderr_lines: Arc<Mutex<Vec<String>>>,
    stderr_reader: Option<JoinHandle<()>>,
    stderr_reader_done_rx: Option<Receiver<()>>,
}

impl HostRuntime {
    pub fn new(config: HostConfig) -> Self {
        Self {
            config,
            state: HostLifecycleState::Stopped,
            child: None,
            stderr_lines: Arc::new(Mutex::new(Vec::new())),
            stderr_reader: None,
            stderr_reader_done_rx: None,
        }
    }

    pub fn state(&self) -> HostLifecycleState {
        self.state
    }

    pub fn config(&self) -> &HostConfig {
        &self.config
    }

    pub fn pid(&mut self) -> Option<u32> {
        self.refresh_state();
        self.child.as_ref().map(Child::id)
    }

    pub fn stderr_snapshot(&self) -> Vec<String> {
        self.stderr_lines
            .lock()
            .expect("stderr buffer mutex poisoned")
            .clone()
    }

    pub fn start(&mut self, timeout: Duration) -> Result<()> {
        self.refresh_state();
        if self.child.is_some() {
            return Err(CodexIntegrationError::HostAlreadyRunning);
        }
        if host_shutdown_in_progress() {
            return Err(CodexIntegrationError::WebSocketTransport(
                "codex host startup was blocked because shutdown is in progress".to_string(),
            ));
        }

        self.state = HostLifecycleState::Starting;

        let mut command = self.config.build_command();
        #[cfg(unix)]
        {
            // Run the host as its own process-group leader so we can signal descendants.
            command.process_group(0);
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(CodexIntegrationError::HostProcessIo)?;
        let process_id = child.id();
        register_tracked_host_process(process_id);
        if host_shutdown_in_progress() {
            stop_spawned_child_during_shutdown(process_id, &mut child);
            unregister_tracked_host_process(process_id);
            self.state = HostLifecycleState::Failed;
            return Err(CodexIntegrationError::WebSocketTransport(
                "codex host startup was interrupted because shutdown began".to_string(),
            ));
        }
        self.spawn_stderr_reader(&mut child);
        self.child = Some(child);
        self.wait_until_ready(timeout)
    }

    pub fn ensure_running(&mut self, timeout: Duration) -> Result<()> {
        self.refresh_state();
        if self.child.is_some() && self.state == HostLifecycleState::Ready {
            return Ok(());
        }

        self.state = HostLifecycleState::Reconnecting;
        self.start(timeout)
    }

    pub fn stop(&mut self) -> Result<()> {
        self.stop_internal(HostLifecycleState::Stopped, false)
    }

    pub fn force_kill(&mut self) -> Result<()> {
        self.stop_internal(HostLifecycleState::Failed, true)
    }

    fn stop_internal(&mut self, final_state: HostLifecycleState, force: bool) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let process_id = child.id();
            let stop_result = if force {
                self.force_stop_child(process_id, &mut child)
            } else {
                self.graceful_stop_child(process_id, &mut child)
            };
            match stop_result {
                Ok(()) => {
                    let wait_result = child.wait();
                    if wait_result.is_ok() || matches!(child.try_wait(), Ok(Some(_))) {
                        unregister_tracked_host_process(process_id);
                    }
                    #[cfg(unix)]
                    {
                        // Best-effort cleanup for any descendants that outlive the group leader.
                        let _ = signal_process_group(process_id, ProcessSignal::Kill);
                    }
                    wait_result.map_err(CodexIntegrationError::HostProcessIo)?;
                }
                Err(error) => {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        unregister_tracked_host_process(process_id);
                    }
                    return Err(error);
                }
            }
        }
        self.join_stderr_reader();
        self.state = final_state;
        Ok(())
    }

    fn graceful_stop_child(&self, process_id: u32, child: &mut Child) -> Result<()> {
        #[cfg(unix)]
        {
            if let Err(error) = signal_process_group(process_id, ProcessSignal::Term) {
                let already_exited = matches!(child.try_wait(), Ok(Some(_)));
                if !already_exited {
                    return Err(CodexIntegrationError::HostProcessIo(error));
                }
            }

            if wait_for_child_exit(child, STOP_TERM_GRACE_TIMEOUT)
                .map_err(CodexIntegrationError::HostProcessIo)?
            {
                return Ok(());
            }

            self.force_stop_child(process_id, child)
        }

        #[cfg(not(unix))]
        {
            self.force_stop_child(process_id, child)
        }
    }

    fn force_stop_child(&self, process_id: u32, child: &mut Child) -> Result<()> {
        #[cfg(unix)]
        {
            if let Err(error) = signal_process_group(process_id, ProcessSignal::Kill) {
                let already_exited = matches!(child.try_wait(), Ok(Some(_)));
                if !already_exited {
                    if let Err(kill_error) = child.kill() {
                        let already_exited = matches!(child.try_wait(), Ok(Some(_)));
                        if !already_exited {
                            return Err(CodexIntegrationError::HostProcessIo(kill_error));
                        }
                    }
                    return Err(CodexIntegrationError::HostProcessIo(error));
                }
            }
            Ok(())
        }

        #[cfg(not(unix))]
        {
            if stop_windows_process_tree(process_id)
                .map_err(CodexIntegrationError::HostProcessIo)?
            {
                return Ok(());
            }

            if let Err(error) = child.kill() {
                let already_exited = matches!(child.try_wait(), Ok(Some(_)));
                if !already_exited {
                    return Err(CodexIntegrationError::HostProcessIo(error));
                }
            }
            Ok(())
        }
    }

    fn wait_until_ready(&mut self, timeout: Duration) -> Result<()> {
        let started_at = Instant::now();
        let probe_address = SocketAddr::from(([127, 0, 0, 1], self.config.port));

        loop {
            let status = self
                .child
                .as_mut()
                .expect("child process should exist while waiting for readiness")
                .try_wait()
                .map_err(CodexIntegrationError::HostProcessIo)?;

            if let Some(exit_status) = status {
                let stderr_lines = self.stderr_snapshot();
                let status = format_host_exit_status(exit_status.to_string(), &stderr_lines);
                unregister_tracked_host_process(
                    self.child
                        .as_ref()
                        .expect("child should still exist before readiness failure")
                        .id(),
                );
                self.child = None;
                self.join_stderr_reader();
                self.state = HostLifecycleState::Failed;
                return Err(CodexIntegrationError::HostExitedBeforeReady { status });
            }

            if TcpStream::connect_timeout(&probe_address, READY_PROBE_CONNECT_TIMEOUT).is_ok() {
                self.state = HostLifecycleState::Ready;
                return Ok(());
            }

            if started_at.elapsed() >= timeout {
                let timeout_ms = timeout.as_millis().min(u128::from(u64::MAX)) as u64;
                let _ = self.force_kill();
                return Err(CodexIntegrationError::HostStartupTimedOut {
                    port: self.config.port,
                    timeout_ms,
                });
            }

            thread::sleep(READY_PROBE_INTERVAL);
        }
    }

    fn refresh_state(&mut self) {
        let maybe_status = self
            .child
            .as_mut()
            .and_then(|child| match child.try_wait() {
                Ok(status) => status,
                Err(error) => {
                    warn!("failed to inspect host process state: {error}");
                    None
                }
            });

        if let Some(status) = maybe_status {
            warn!("codex host process exited unexpectedly: {status}");
            if let Some(process_id) = self.child.as_ref().map(Child::id) {
                unregister_tracked_host_process(process_id);
            }
            self.child = None;
            self.join_stderr_reader();
            self.state = HostLifecycleState::Failed;
        }
    }

    fn spawn_stderr_reader(&mut self, child: &mut Child) {
        let Some(stderr) = child.stderr.take() else {
            self.stderr_reader_done_rx = None;
            return;
        };

        let lines = Arc::clone(&self.stderr_lines);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        self.stderr_reader_done_rx = Some(done_rx);
        self.stderr_reader = Some(thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if line.trim().is_empty() {
                    continue;
                }
                debug!("codex host stderr: {line}");

                let mut guard = lines.lock().expect("stderr buffer mutex poisoned");
                guard.push(line);
                if guard.len() > STDERR_BUFFER_LIMIT {
                    let keep_from = guard.len() - STDERR_BUFFER_LIMIT;
                    guard.drain(0..keep_from);
                }
            }
            let _ = done_tx.send(());
        }));
    }

    fn join_stderr_reader(&mut self) {
        let Some(join_handle) = self.stderr_reader.take() else {
            self.stderr_reader_done_rx = None;
            return;
        };

        let should_join = match self.stderr_reader_done_rx.take() {
            Some(done_rx) => match done_rx.recv_timeout(STDERR_READER_JOIN_TIMEOUT) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
                Err(RecvTimeoutError::Timeout) => {
                    warn!("timed out waiting for codex stderr reader; detaching reader thread");
                    false
                }
            },
            None => true,
        };
        if !should_join {
            return;
        }

        if let Err(error) = join_handle.join() {
            warn!("stderr reader join failed: {error:?}");
        }
    }
}

fn configure_background_command(command: &mut Command) {
    let _ = command;
}

fn format_host_exit_status(status: String, stderr_lines: &[String]) -> String {
    if stderr_lines.is_empty() {
        return status;
    }

    let stderr_excerpt = stderr_lines
        .iter()
        .rev()
        .take(4)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
    format!("{status}; stderr: {stderr_excerpt}")
}

#[cfg(unix)]
fn wait_for_child_exit(child: &mut Child, timeout: Duration) -> io::Result<bool> {
    let started_at = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return Ok(true);
        }
        if started_at.elapsed() >= timeout {
            return Ok(false);
        }
        thread::sleep(STOP_TERM_POLL_INTERVAL);
    }
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy)]
enum ProcessSignal {
    Term,
    Kill,
}

#[cfg(unix)]
impl ProcessSignal {
    fn kill_arg(self) -> &'static str {
        match self {
            Self::Term => "-TERM",
            Self::Kill => "-KILL",
        }
    }
}

#[cfg(unix)]
fn signal_process_group(process_id: u32, signal: ProcessSignal) -> io::Result<()> {
    let status = Command::new("kill")
        .arg(signal.kill_arg())
        .arg("--")
        .arg(format!("-{process_id}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        return Ok(());
    }

    Err(io::Error::other(format!(
        "kill {} -- -{} exited with status {}",
        signal.kill_arg(),
        process_id,
        status
    )))
}

impl Drop for HostRuntime {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl Drop for SharedHostLease {
    fn drop(&mut self) {
        release_shared_host(&self.key);
    }
}

pub fn begin_host_shutdown() {
    BLOCK_NEW_HOST_STARTS_FOR_SHUTDOWN.store(true, Ordering::SeqCst);
}

pub fn cleanup_tracked_hosts_for_shutdown() {
    for mut runtime in take_shared_host_runtimes() {
        if let Err(error) = runtime.stop() {
            warn!("failed to stop shared codex host during shutdown: {error}");
        }
    }

    let tracked_process_ids = take_tracked_host_processes();
    for process_id in tracked_process_ids {
        cleanup_tracked_host_process(process_id);
    }
}

fn tracked_host_processes() -> &'static Mutex<BTreeSet<u32>> {
    TRACKED_HOST_PROCESS_IDS.get_or_init(|| Mutex::new(BTreeSet::new()))
}

fn host_shutdown_in_progress() -> bool {
    BLOCK_NEW_HOST_STARTS_FOR_SHUTDOWN.load(Ordering::SeqCst)
}

fn shared_hosts() -> &'static Mutex<BTreeMap<SharedHostKey, SharedHostEntry>> {
    SHARED_HOSTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn release_shared_host(key: &SharedHostKey) {
    let maybe_runtime = {
        let mut guard = shared_hosts()
            .lock()
            .expect("shared host registry mutex poisoned");
        let Some(entry) = guard.get_mut(key) else {
            return;
        };

        if entry.lease_count > 1 {
            entry.lease_count -= 1;
            None
        } else {
            guard.remove(key).map(|entry| entry.runtime)
        }
    };

    if let Some(mut runtime) = maybe_runtime
        && let Err(error) = runtime.stop()
    {
        warn!("failed to stop shared codex host after last lease dropped: {error}");
    }
}

fn take_shared_host_runtimes() -> Vec<HostRuntime> {
    let mut guard = shared_hosts()
        .lock()
        .expect("shared host registry mutex poisoned");
    std::mem::take(&mut *guard)
        .into_values()
        .map(|entry| entry.runtime)
        .collect()
}

fn register_tracked_host_process(process_id: u32) {
    tracked_host_processes()
        .lock()
        .expect("tracked host process mutex poisoned")
        .insert(process_id);
}

fn unregister_tracked_host_process(process_id: u32) {
    tracked_host_processes()
        .lock()
        .expect("tracked host process mutex poisoned")
        .remove(&process_id);
}

fn take_tracked_host_processes() -> Vec<u32> {
    let mut guard = tracked_host_processes()
        .lock()
        .expect("tracked host process mutex poisoned");
    let process_ids = guard.iter().copied().collect();
    guard.clear();
    process_ids
}

#[cfg(unix)]
fn cleanup_tracked_host_process(process_id: u32) {
    if !process_group_exists(process_id) {
        let _ = reap_child_process(process_id);
        return;
    }

    if let Err(error) = signal_process_group(process_id, ProcessSignal::Term) {
        if process_group_exists(process_id) {
            warn!("failed to terminate tracked codex host process group {process_id}: {error}");
        }
        return;
    }

    if wait_for_process_group_exit(process_id, STOP_TERM_GRACE_TIMEOUT) {
        return;
    }

    if let Err(error) = signal_process_group(process_id, ProcessSignal::Kill)
        && process_group_exists(process_id)
    {
        warn!("failed to kill tracked codex host process group {process_id}: {error}");
    }
    let _ = wait_for_process_group_exit(process_id, STOP_TERM_GRACE_TIMEOUT);
}

#[cfg(unix)]
fn stop_spawned_child_during_shutdown(process_id: u32, child: &mut Child) {
    let _ = signal_process_group(process_id, ProcessSignal::Kill);
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(unix))]
fn cleanup_tracked_host_process(process_id: u32) {
    if let Err(error) = stop_windows_process_tree(process_id) {
        warn!("failed to kill tracked codex host process tree {process_id}: {error}");
    }
}

#[cfg(not(unix))]
fn stop_spawned_child_during_shutdown(process_id: u32, child: &mut Child) {
    let _ = stop_windows_process_tree(process_id);
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(unix))]
fn stop_windows_process_tree(process_id: u32) -> io::Result<bool> {
    let status = Command::new("taskkill")
        .args(["/PID", &process_id.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    Ok(status.success())
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
        match reap_child_process(process_id) {
            Ok(true) => return true,
            Ok(false) => {}
            Err(error) => {
                warn!("failed to reap tracked codex host process {process_id}: {error}");
            }
        }
        if !process_group_exists(process_id) {
            return true;
        }
        if started_at.elapsed() >= timeout {
            return false;
        }
        thread::sleep(STOP_TERM_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn reap_child_process(process_id: u32) -> io::Result<bool> {
    let process_id = i32::try_from(process_id)
        .map_err(|_| io::Error::other(format!("process id {process_id} does not fit in i32")))?;
    let mut status = 0;
    let waited = unsafe { libc::waitpid(process_id, &mut status, libc::WNOHANG) };
    if waited == process_id {
        return Ok(true);
    }
    if waited == 0 {
        return Ok(false);
    }

    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ECHILD) {
        return Ok(true);
    }
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::HostConfig;
    use super::SharedHostKey;
    use std::path::PathBuf;

    #[test]
    fn shared_host_key_ignores_loopback_listen_port_for_codex_app_server() {
        let first = HostConfig::codex_app_server(
            PathBuf::from("/tmp/codex"),
            PathBuf::from("/repo"),
            PathBuf::from("/tmp/.codex"),
            64100,
        );
        let second = HostConfig::codex_app_server(
            PathBuf::from("/tmp/codex"),
            PathBuf::from("/repo"),
            PathBuf::from("/tmp/.codex"),
            64199,
        );

        assert_eq!(
            SharedHostKey::from_config(&first),
            SharedHostKey::from_config(&second)
        );
    }

    #[test]
    fn shared_host_key_preserves_non_listen_arguments() {
        let mut first = HostConfig::codex_app_server(
            PathBuf::from("/tmp/codex"),
            PathBuf::from("/repo"),
            PathBuf::from("/tmp/.codex"),
            64100,
        );
        first.arguments.push("--verbose".to_string());

        let second = HostConfig::codex_app_server(
            PathBuf::from("/tmp/codex"),
            PathBuf::from("/repo"),
            PathBuf::from("/tmp/.codex"),
            64100,
        );

        assert_ne!(
            SharedHostKey::from_config(&first),
            SharedHostKey::from_config(&second)
        );
    }
}
