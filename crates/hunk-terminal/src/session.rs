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
const TERMINAL_CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";

#[derive(Debug, Clone)]
pub struct TerminalSpawnRequest {
    pub cwd: PathBuf,
    pub command: String,
    pub rows: u16,
    pub cols: u16,
    shell_program_override: Option<OsString>,
}

impl TerminalSpawnRequest {
    pub fn new(cwd: PathBuf, command: String) -> Self {
        Self {
            cwd,
            command,
            rows: 24,
            cols: 120,
            shell_program_override: None,
        }
    }

    pub fn shell(cwd: PathBuf) -> Self {
        Self::new(cwd, String::new())
    }

    pub fn with_shell_program(mut self, shell_program: impl Into<OsString>) -> Self {
        self.shell_program_override = Some(shell_program.into());
        self
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

type SharedTerminalWriter = Arc<Mutex<Box<dyn std::io::Write + Send>>>;

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

    let mut command = shell_command_builder(
        request.command.as_str(),
        request.cwd.as_path(),
        request.shell_program_override.as_deref(),
    );
    command.cwd(request.cwd.as_os_str());
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");

    let mut child = pair
        .slave
        .spawn_command(command)
        .context("spawn terminal command")?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().context("clone PTY reader")?;
    let writer: SharedTerminalWriter = Arc::new(Mutex::new(
        pair.master.take_writer().context("take PTY writer")?,
    ));
    let vt = Arc::new(Mutex::new(TerminalVt::new(request.rows, request.cols)));
    let (event_tx, event_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();

    if let Ok(mut vt) = vt.lock() {
        let _ = event_tx.send(TerminalEvent::Screen(vt.snapshot()));
    }

    let output_tx = event_tx.clone();
    let output_vt = Arc::clone(&vt);
    let output_writer = Arc::clone(&writer);
    thread::spawn(move || {
        let mut buffer = [0_u8; READ_BUFFER_BYTES];
        let mut query_responder = TerminalQueryResponder::default();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let bytes = buffer[..count].to_vec();
                    let snapshot = match output_vt.lock() {
                        Ok(mut vt) => Some(vt.advance(bytes.as_slice())),
                        Err(_) => None,
                    };
                    if let Some(snapshot) = snapshot.as_ref() {
                        // ConPTY can emit a cursor-position query during startup; answer it
                        // from the current VT cursor so Windows shells don't stall on launch.
                        for response in query_responder.responses(bytes.as_slice(), snapshot) {
                            if let Err(error) =
                                write_terminal_bytes(&output_writer, response.as_slice())
                            {
                                let _ = output_tx.send(TerminalEvent::Failed(format!(
                                    "Failed to respond to terminal query: {error}"
                                )));
                                return;
                            }
                        }
                    }
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
    let control_writer = Arc::clone(&writer);
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
                    if let Err(error) = write_terminal_bytes(&control_writer, input.as_slice()) {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to write terminal input: {error}"
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

fn write_terminal_bytes(writer: &SharedTerminalWriter, input: &[u8]) -> Result<()> {
    let mut writer = writer
        .lock()
        .map_err(|_| anyhow::anyhow!("terminal writer poisoned"))?;
    writer.write_all(input).context("write terminal bytes")?;
    writer.flush().context("flush terminal bytes")?;
    Ok(())
}

#[derive(Default)]
struct TerminalQueryResponder {
    trailing_bytes: Vec<u8>,
}

impl TerminalQueryResponder {
    fn responses(&mut self, bytes: &[u8], screen: &TerminalScreenSnapshot) -> Vec<Vec<u8>> {
        let mut combined = Vec::with_capacity(self.trailing_bytes.len() + bytes.len());
        combined.extend_from_slice(self.trailing_bytes.as_slice());
        combined.extend_from_slice(bytes);

        let mut responses = Vec::new();
        let mut index = 0usize;
        while index + TERMINAL_CURSOR_POSITION_QUERY.len() <= combined.len() {
            if combined[index..].starts_with(TERMINAL_CURSOR_POSITION_QUERY) {
                responses.push(terminal_cursor_position_response(screen));
                index += TERMINAL_CURSOR_POSITION_QUERY.len();
                continue;
            }
            index += 1;
        }

        let keep = combined
            .len()
            .min(TERMINAL_CURSOR_POSITION_QUERY.len().saturating_sub(1));
        self.trailing_bytes.clear();
        self.trailing_bytes
            .extend_from_slice(&combined[combined.len().saturating_sub(keep)..]);

        responses
    }
}

fn terminal_cursor_position_response(screen: &TerminalScreenSnapshot) -> Vec<u8> {
    let row = screen.cursor.line.max(0) as usize + 1;
    let column = screen.cursor.column + 1;
    format!("\x1b[{row};{column}R").into_bytes()
}

fn shell_command_builder(
    command: &str,
    cwd: &Path,
    shell_program_override: Option<&std::ffi::OsStr>,
) -> CommandBuilder {
    #[cfg(target_os = "windows")]
    {
        let shell = shell_program_override
            .map(OsString::from)
            .unwrap_or_else(windows_shell_program);
        let is_cmd = shell_is_cmd(shell.as_os_str());
        let mut builder = CommandBuilder::new(shell);
        if is_cmd && std::env::var_os("PROMPT").is_none() {
            builder.env("PROMPT", "$P$G$S");
        }
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
        let shell = shell_program_override
            .map(OsString::from)
            .unwrap_or_else(unix_shell_program);
        let mut builder = CommandBuilder::new(shell.clone());
        if command.trim().is_empty() {
            builder.arg("-i");
        } else {
            if unix_shell_supports_login_flag(shell.as_os_str()) {
                builder.arg("-l");
            }
            builder.arg("-c");
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

#[cfg(not(target_os = "windows"))]
fn unix_shell_program() -> OsString {
    if let Some(shell) = std::env::var_os("SHELL")
        .filter(|shell| !shell.is_empty())
        .filter(|shell| Path::new(shell).exists())
    {
        return shell;
    }

    ["/bin/bash", "/bin/sh"]
        .into_iter()
        .find(|path| Path::new(path).exists())
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("/bin/sh"))
}

#[cfg(not(target_os = "windows"))]
fn unix_shell_supports_login_flag(shell: &std::ffi::OsStr) -> bool {
    Path::new(shell)
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| {
            matches!(
                value,
                "bash" | "zsh" | "fish" | "ksh" | "mksh" | "nu" | "nushell"
            )
        })
        .unwrap_or(false)
}
