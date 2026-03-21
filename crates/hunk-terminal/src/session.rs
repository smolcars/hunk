use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::vt::{TerminalScreenSnapshot, TerminalScroll, TerminalVt};

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

    pub fn shell(cwd: PathBuf) -> Self {
        Self::new(cwd, String::new())
    }
}

#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Output(Vec<u8>),
    Screen(Arc<TerminalScreenSnapshot>),
    Exit { exit_code: Option<i32> },
    Failed(String),
}

enum TerminalControl {
    Kill,
    Resize { rows: u16, cols: u16 },
    Scroll(TerminalScroll),
    WriteInput(Vec<u8>),
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

    pub fn write_input(&self, input: &[u8]) -> Result<()> {
        self.control_tx
            .send(TerminalControl::WriteInput(input.to_vec()))
            .context("send terminal input")
    }

    pub fn scroll_display(&self, scroll: TerminalScroll) -> Result<()> {
        self.control_tx
            .send(TerminalControl::Scroll(scroll))
            .context("send terminal scroll")
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
    command.env("COLORTERM", "truecolor");

    let mut child = pair
        .slave
        .spawn_command(command)
        .context("spawn terminal command")?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().context("clone PTY reader")?;
    let mut writer = pair.master.take_writer().context("take PTY writer")?;
    let vt = Arc::new(Mutex::new(TerminalVt::new(request.rows, request.cols)));
    let (event_tx, event_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();

    if let Ok(mut vt) = vt.lock() {
        let _ = event_tx.send(TerminalEvent::Screen(vt.snapshot()));
    }

    let output_tx = event_tx.clone();
    let output_vt = Arc::clone(&vt);
    thread::spawn(move || {
        let mut buffer = [0_u8; READ_BUFFER_BYTES];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let bytes = buffer[..count].to_vec();
                    let snapshot = match output_vt.lock() {
                        Ok(mut vt) => Some(vt.advance(bytes.as_slice())),
                        Err(_) => None,
                    };
                    if output_tx.send(TerminalEvent::Output(bytes)).is_err() {
                        break;
                    }
                    if let Some(snapshot) = snapshot
                        && output_tx.send(TerminalEvent::Screen(snapshot)).is_err()
                    {
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

    let control_vt = Arc::clone(&vt);
    thread::spawn(move || {
        let master = pair.master;
        let mut child_exit_reported = false;
        loop {
            match control_rx.recv_timeout(CONTROL_POLL_INTERVAL) {
                Ok(TerminalControl::Kill) => {
                    if child_exit_reported {
                        break;
                    }
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
                    } else if let Ok(mut vt) = control_vt.lock() {
                        let _ = event_tx.send(TerminalEvent::Screen(vt.resize(rows, cols)));
                    }
                }
                Ok(TerminalControl::Scroll(scroll)) => {
                    if let Ok(mut vt) = control_vt.lock() {
                        let _ = event_tx.send(TerminalEvent::Screen(vt.scroll_display(scroll)));
                    }
                }
                Ok(TerminalControl::WriteInput(input)) => {
                    if child_exit_reported {
                        continue;
                    }
                    if let Err(error) = writer.write_all(input.as_slice()) {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to write terminal input: {error}"
                        )));
                    }
                    if let Err(error) = writer.flush() {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to flush terminal input: {error}"
                        )));
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if child_exit_reported {
                        break;
                    }
                    let _ = child.kill();
                }
            }

            match child.try_wait() {
                Ok(Some(status)) => {
                    if !child_exit_reported {
                        child_exit_reported = true;
                        let _ = event_tx.send(TerminalEvent::Exit {
                            exit_code: i32::try_from(status.exit_code()).ok(),
                        });
                    }
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
        let is_cmd = shell_is_cmd(shell.as_os_str());
        let mut builder = CommandBuilder::new(shell);
        if command.trim().is_empty() {
            if !is_cmd {
                builder.arg("-NoLogo");
            }
        } else {
            if is_cmd {
                builder.arg("/D");
                builder.arg("/C");
            } else {
                builder.arg("-NoLogo");
                builder.arg("-NoProfile");
                builder.arg("-Command");
            }
            builder.arg(command);
        }
        builder.cwd(cwd.as_os_str());
        builder
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/bash"));
        let mut builder = CommandBuilder::new(shell);
        if command.trim().is_empty() {
            builder.arg("-i");
        } else {
            builder.arg("-lc");
            builder.arg(command);
        }
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
