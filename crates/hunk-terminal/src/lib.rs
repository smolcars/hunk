use std::ffi::OsString;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(50);
const READ_BUFFER_BYTES: usize = 8192;

#[derive(Debug, Clone)]
pub struct TerminalSpawnRequest {
    pub cwd: PathBuf,
    pub command: String,
    pub rows: u16,
    pub cols: u16,
}

impl TerminalSpawnRequest {
    pub fn new(cwd: PathBuf, command: String) -> Self {
        Self {
            cwd,
            command,
            rows: 24,
            cols: 120,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Output(String),
    Exit { exit_code: Option<i32> },
    Failed(String),
}

enum TerminalControl {
    Kill,
    Resize { rows: u16, cols: u16 },
}

pub struct TerminalSessionHandle {
    control_tx: Sender<TerminalControl>,
}

impl TerminalSessionHandle {
    pub fn kill(&self) -> Result<()> {
        self.control_tx
            .send(TerminalControl::Kill)
            .context("send terminal kill command")
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.control_tx
            .send(TerminalControl::Resize { rows, cols })
            .context("send terminal resize command")
    }
}

pub fn spawn_terminal_session(
    request: TerminalSpawnRequest,
) -> Result<(TerminalSessionHandle, Receiver<TerminalEvent>)> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: request.rows,
            cols: request.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("open pseudoterminal")?;

    let mut command = shell_command_builder(request.command.as_str(), request.cwd.as_path());
    command.cwd(request.cwd.as_os_str());
    command.env("TERM", "xterm-256color");
    command.env("NO_COLOR", "1");
    command.env("CLICOLOR", "0");

    let mut child = pair
        .slave
        .spawn_command(command)
        .context("spawn terminal command")?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().context("clone PTY reader")?;
    let (event_tx, event_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();

    let output_tx = event_tx.clone();
    thread::spawn(move || {
        let mut buffer = [0_u8; READ_BUFFER_BYTES];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let output = String::from_utf8_lossy(&buffer[..count]).to_string();
                    if output_tx.send(TerminalEvent::Output(output)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = output_tx.send(TerminalEvent::Failed(format!(
                        "Failed to read terminal output: {error}"
                    )));
                    break;
                }
            }
        }
    });

    thread::spawn(move || {
        let master = pair.master;
        loop {
            match control_rx.recv_timeout(CONTROL_POLL_INTERVAL) {
                Ok(TerminalControl::Kill) => {
                    if let Err(error) = child.kill() {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to stop terminal command: {error}"
                        )));
                    }
                }
                Ok(TerminalControl::Resize { rows, cols }) => {
                    if let Err(error) = master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    }) {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to resize terminal: {error}"
                        )));
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {}
            }

            match child.try_wait() {
                Ok(Some(status)) => {
                    let _ = event_tx.send(TerminalEvent::Exit {
                        exit_code: i32::try_from(status.exit_code()).ok(),
                    });
                    break;
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = event_tx.send(TerminalEvent::Failed(format!(
                        "Failed to poll terminal process: {error}"
                    )));
                    break;
                }
            }
        }
    });

    Ok((TerminalSessionHandle { control_tx }, event_rx))
}

fn shell_command_builder(command: &str, cwd: &Path) -> CommandBuilder {
    #[cfg(target_os = "windows")]
    {
        let shell = windows_shell_program();
        let mut builder = CommandBuilder::new(shell);
        if shell_is_cmd(shell.as_os_str()) {
            builder.arg("/D");
            builder.arg("/C");
        } else {
            builder.arg("-NoLogo");
            builder.arg("-NoProfile");
            builder.arg("-Command");
        }
        builder.arg(command);
        builder.cwd(cwd.as_os_str());
        builder
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/bash"));
        let mut builder = CommandBuilder::new(shell);
        builder.arg("-lc");
        builder.arg(command);
        builder.cwd(cwd.as_os_str());
        builder
    }
}

#[cfg(target_os = "windows")]
fn windows_shell_program() -> OsString {
    std::env::var_os("COMSPEC").unwrap_or_else(|| OsString::from("cmd.exe"))
}

#[cfg(target_os = "windows")]
fn shell_is_cmd(shell: &std::ffi::OsStr) -> bool {
    Path::new(shell)
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("cmd.exe") || value.eq_ignore_ascii_case("cmd"))
        .unwrap_or(false)
}
