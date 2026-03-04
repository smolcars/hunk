use std::io::BufRead;
use std::io::BufReader;
use std::net::SocketAddr;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

use tracing::warn;

use crate::errors::CodexIntegrationError;
use crate::errors::Result;

const STDERR_BUFFER_LIMIT: usize = 256;
const READY_PROBE_INTERVAL: Duration = Duration::from_millis(75);
const READY_PROBE_CONNECT_TIMEOUT: Duration = Duration::from_millis(200);

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
        for (key, value) in &self.environment {
            command.env(key, value);
        }
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
}

impl HostRuntime {
    pub fn new(config: HostConfig) -> Self {
        Self {
            config,
            state: HostLifecycleState::Stopped,
            child: None,
            stderr_lines: Arc::new(Mutex::new(Vec::new())),
            stderr_reader: None,
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

        self.state = HostLifecycleState::Starting;

        let mut command = self.config.build_command();
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(CodexIntegrationError::HostProcessIo)?;
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
        self.stop_internal(HostLifecycleState::Stopped)
    }

    pub fn force_kill(&mut self) -> Result<()> {
        self.stop_internal(HostLifecycleState::Failed)
    }

    fn stop_internal(&mut self, final_state: HostLifecycleState) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            if let Err(error) = child.kill() {
                let already_exited = matches!(child.try_wait(), Ok(Some(_)));
                if !already_exited {
                    return Err(CodexIntegrationError::HostProcessIo(error));
                }
            }
            let _ = child.wait();
        }
        self.join_stderr_reader();
        self.state = final_state;
        Ok(())
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
                self.child = None;
                self.join_stderr_reader();
                self.state = HostLifecycleState::Failed;
                return Err(CodexIntegrationError::HostExitedBeforeReady {
                    status: exit_status.to_string(),
                });
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
            self.child = None;
            self.join_stderr_reader();
            self.state = HostLifecycleState::Failed;
        }
    }

    fn spawn_stderr_reader(&mut self, child: &mut Child) {
        let Some(stderr) = child.stderr.take() else {
            return;
        };

        let lines = Arc::clone(&self.stderr_lines);
        self.stderr_reader = Some(thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if line.trim().is_empty() {
                    continue;
                }
                warn!("codex host stderr: {line}");

                let mut guard = lines.lock().expect("stderr buffer mutex poisoned");
                guard.push(line);
                if guard.len() > STDERR_BUFFER_LIMIT {
                    let keep_from = guard.len() - STDERR_BUFFER_LIMIT;
                    guard.drain(0..keep_from);
                }
            }
        }));
    }

    fn join_stderr_reader(&mut self) {
        if let Some(join_handle) = self.stderr_reader.take()
            && let Err(error) = join_handle.join()
        {
            warn!("stderr reader join failed: {error:?}");
        }
    }
}

impl Drop for HostRuntime {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
