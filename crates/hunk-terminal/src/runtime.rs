use std::ffi::OsString;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::backend::TerminalVt;
use crate::input::TerminalPointerInput;
use crate::snapshot::{TerminalScreenSnapshot, TerminalScroll};

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
    shell_args_override: Option<Vec<OsString>>,
    env_overrides: Vec<(OsString, OsString)>,
}

impl TerminalSpawnRequest {
    pub fn new(cwd: PathBuf, command: String) -> Self {
        Self {
            cwd,
            command,
            rows: 24,
            cols: 120,
            shell_program_override: None,
            shell_args_override: None,
            env_overrides: Vec::new(),
        }
    }

    pub fn shell(cwd: PathBuf) -> Self {
        Self::new(cwd, String::new())
    }

    pub fn with_shell_program(mut self, shell_program: impl Into<OsString>) -> Self {
        self.shell_program_override = Some(shell_program.into());
        self
    }

    pub fn with_shell_args<I, S>(mut self, shell_args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.shell_args_override = Some(shell_args.into_iter().map(Into::into).collect());
        self
    }

    pub fn with_env_var(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env_overrides.push((key.into(), value.into()));
        self
    }

    pub fn with_env_vars<I, K, V>(mut self, env_vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.env_overrides.extend(
            env_vars
                .into_iter()
                .map(|(key, value)| (key.into(), value.into())),
        );
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
    WritePaste(String),
    ReportFocus(bool),
    WritePointerInput(TerminalPointerInput),
}

enum TerminalActorInput {
    Control(TerminalControl),
    PtyOutput(Vec<u8>),
    PtyReadFailed(String),
    PtyClosed,
}

pub struct TerminalSessionHandle {
    actor_tx: Sender<TerminalActorInput>,
}

impl TerminalSessionHandle {
    pub fn kill(&self) -> Result<()> {
        self.send_control(TerminalControl::Kill)
            .context("send terminal kill command")
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.send_control(TerminalControl::Resize { rows, cols })
            .context("send terminal resize command")
    }

    pub fn write_input(&self, input: &[u8]) -> Result<()> {
        self.send_control(TerminalControl::WriteInput(input.to_vec()))
            .context("send terminal input")
    }

    pub fn write_paste(&self, text: &str) -> Result<()> {
        self.send_control(TerminalControl::WritePaste(text.to_string()))
            .context("send terminal paste")
    }

    pub fn report_focus(&self, focused: bool) -> Result<()> {
        self.send_control(TerminalControl::ReportFocus(focused))
            .context("send terminal focus event")
    }

    pub fn write_pointer_input(&self, input: TerminalPointerInput) -> Result<()> {
        self.send_control(TerminalControl::WritePointerInput(input))
            .context("send terminal pointer input")
    }

    pub fn scroll_display(&self, scroll: TerminalScroll) -> Result<()> {
        self.send_control(TerminalControl::Scroll(scroll))
            .context("send terminal scroll")
    }

    fn send_control(&self, control: TerminalControl) -> Result<()> {
        self.actor_tx.send(TerminalActorInput::Control(control))?;
        Ok(())
    }
}

impl Drop for TerminalSessionHandle {
    fn drop(&mut self) {
        let _ = self
            .actor_tx
            .send(TerminalActorInput::Control(TerminalControl::Kill));
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
        request.shell_args_override.as_deref(),
    );
    command.cwd(request.cwd.as_os_str());
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    for (key, value) in request.env_overrides {
        command.env(key, value);
    }

    let mut child = pair
        .slave
        .spawn_command(command)
        .context("spawn terminal command")?;
    drop(pair.slave);

    let master = pair.master;
    let mut reader = master.try_clone_reader().context("clone PTY reader")?;
    let writer = master.take_writer().context("take PTY writer")?;
    let (event_tx, event_rx) = mpsc::channel();
    let (actor_tx, actor_rx) = mpsc::channel();

    let output_tx = actor_tx.clone();
    thread::spawn(move || {
        let mut buffer = [0_u8; READ_BUFFER_BYTES];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if output_tx
                        .send(TerminalActorInput::PtyOutput(buffer[..count].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    let _ = output_tx.send(TerminalActorInput::PtyReadFailed(format!(
                        "Failed to read terminal output: {error}"
                    )));
                    break;
                }
            }
        }

        let _ = output_tx.send(TerminalActorInput::PtyClosed);
    });

    thread::spawn(move || {
        let mut writer = writer;
        let mut vt = TerminalVt::new(request.rows, request.cols);
        let mut query_responder = TerminalQueryResponder::default();
        let mut child_exit_reported = false;
        let mut pty_closed = false;

        let _ = event_tx.send(TerminalEvent::Screen(vt.snapshot()));

        loop {
            match actor_rx.recv_timeout(CONTROL_POLL_INTERVAL) {
                Ok(TerminalActorInput::Control(TerminalControl::Kill)) => {
                    if child_exit_reported {
                        break;
                    }
                    if let Err(error) = child.kill() {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to stop terminal command: {error}"
                        )));
                    }
                }
                Ok(TerminalActorInput::Control(TerminalControl::Resize { rows, cols })) => {
                    if let Err(error) = master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    }) {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to resize terminal: {error}"
                        )));
                    } else {
                        let _ = event_tx.send(TerminalEvent::Screen(vt.resize(rows, cols)));
                    }
                }
                Ok(TerminalActorInput::Control(TerminalControl::Scroll(scroll))) => {
                    let _ = event_tx.send(TerminalEvent::Screen(vt.scroll_display(scroll)));
                }
                Ok(TerminalActorInput::Control(TerminalControl::WriteInput(input))) => {
                    if child_exit_reported {
                        continue;
                    }
                    if let Err(error) = write_terminal_bytes(writer.as_mut(), input.as_slice()) {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to write terminal input: {error}"
                        )));
                    }
                }
                Ok(TerminalActorInput::Control(TerminalControl::WritePaste(text))) => {
                    if child_exit_reported {
                        continue;
                    }
                    let input = vt.paste_input_bytes(text.as_str());
                    if let Err(error) = write_terminal_bytes(writer.as_mut(), input.as_slice()) {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to write terminal paste: {error}"
                        )));
                    }
                }
                Ok(TerminalActorInput::Control(TerminalControl::ReportFocus(focused))) => {
                    if child_exit_reported {
                        continue;
                    }
                    let Some(input) = vt.focus_input_bytes(focused) else {
                        continue;
                    };
                    if let Err(error) = write_terminal_bytes(writer.as_mut(), input.as_slice()) {
                        let _ = event_tx.send(TerminalEvent::Failed(format!(
                            "Failed to write terminal focus event: {error}"
                        )));
                    }
                }
                Ok(TerminalActorInput::Control(TerminalControl::WritePointerInput(input))) => {
                    if child_exit_reported {
                        continue;
                    }
                    for report in vt.pointer_input_bytes(input) {
                        if let Err(error) = write_terminal_bytes(writer.as_mut(), report.as_slice())
                        {
                            let _ = event_tx.send(TerminalEvent::Failed(format!(
                                "Failed to write terminal pointer input: {error}"
                            )));
                            break;
                        }
                    }
                }
                Ok(TerminalActorInput::PtyOutput(bytes)) => {
                    let snapshot = vt.advance(bytes.as_slice());
                    for response in query_responder.responses(bytes.as_slice(), &snapshot) {
                        if let Err(error) =
                            write_terminal_bytes(writer.as_mut(), response.as_slice())
                        {
                            let _ = event_tx.send(TerminalEvent::Failed(format!(
                                "Failed to respond to terminal query: {error}"
                            )));
                            break;
                        }
                    }
                    if event_tx.send(TerminalEvent::Output(bytes)).is_err() {
                        break;
                    }
                    if event_tx.send(TerminalEvent::Screen(snapshot)).is_err() {
                        break;
                    }
                }
                Ok(TerminalActorInput::PtyReadFailed(error)) => {
                    let _ = event_tx.send(TerminalEvent::Failed(error));
                    pty_closed = true;
                }
                Ok(TerminalActorInput::PtyClosed) => {
                    pty_closed = true;
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

            if child_exit_reported && pty_closed {
                break;
            }
        }
    });

    Ok((TerminalSessionHandle { actor_tx }, event_rx))
}

fn write_terminal_bytes(writer: &mut dyn std::io::Write, input: &[u8]) -> Result<()> {
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
    shell_args_override: Option<&[OsString]>,
) -> CommandBuilder {
    #[cfg(target_os = "windows")]
    {
        let shell = shell_program_override
            .map(OsString::from)
            .unwrap_or_else(windows_shell_program);
        let is_cmd = shell_is_cmd(shell.as_os_str());
        let mut builder = CommandBuilder::new(shell);
        if let Some(shell_args_override) = shell_args_override {
            for arg in shell_args_override {
                builder.arg(arg);
            }
            if !command.trim().is_empty() {
                builder.arg(command);
            }
            builder.cwd(cwd.as_os_str());
            return builder;
        }
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
        if let Some(shell_args_override) = shell_args_override {
            for arg in shell_args_override {
                builder.arg(arg);
            }
            if !command.trim().is_empty() {
                builder.arg(command);
            }
            builder.cwd(cwd.as_os_str());
            return builder;
        }
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
