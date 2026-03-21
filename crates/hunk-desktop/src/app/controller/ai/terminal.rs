const AI_TERMINAL_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(33);
const AI_TERMINAL_MAX_TRANSCRIPT_BYTES: usize = 256 * 1024;
const AI_TERMINAL_MIN_HEIGHT_PX: f32 = 160.0;
const AI_TERMINAL_MAX_HEIGHT_PX: f32 = 420.0;
const AI_TERMINAL_HEIGHT_STEP_PX: f32 = 56.0;

impl DiffViewer {
    fn sync_ai_visible_terminal_input_to_state(&mut self, cx: &Context<Self>) {
        self.ai_terminal_input_draft = self.ai_terminal_input_state.read(cx).value().to_string();
    }

    fn restore_ai_visible_terminal_input(&mut self, cx: &mut Context<Self>) {
        let value = self.ai_terminal_input_draft.clone();
        let input_state = self.ai_terminal_input_state.clone();
        if let Err(error) = Self::update_any_window(cx, move |window, cx| {
            input_state.update(cx, |state, cx| {
                state.set_value(value.clone(), window, cx);
            });
        }) {
            error!("failed to restore AI terminal input after workspace change: {error:#}");
        }
    }

    fn ai_terminal_runtime_is_current(&self, workspace_key: &str, generation: usize) -> bool {
        self.ai_terminal_runtime.as_ref().is_some_and(|runtime| {
            runtime.workspace_key == workspace_key && runtime.generation == generation
        })
    }

    fn next_ai_terminal_runtime_generation(&mut self) -> usize {
        self.ai_terminal_runtime_generation = self.ai_terminal_runtime_generation.saturating_add(1);
        self.ai_terminal_runtime_generation
    }

    pub(crate) fn current_ai_terminal_can_run(&self) -> bool {
        self.ai_workspace_cwd().is_some()
    }

    pub(crate) fn ai_terminal_status_label(&self) -> &'static str {
        match self.ai_terminal_session.status {
            AiTerminalSessionStatus::Idle => "Idle",
            AiTerminalSessionStatus::Running => "Running",
            AiTerminalSessionStatus::Completed => "Completed",
            AiTerminalSessionStatus::Failed => "Failed",
            AiTerminalSessionStatus::Stopped => "Stopped",
        }
    }

    fn ai_terminal_set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.ai_terminal_open == open {
            return;
        }
        self.ai_terminal_open = open;
        cx.notify();
    }

    pub(super) fn ai_toggle_terminal_drawer_action(&mut self, cx: &mut Context<Self>) {
        self.ai_terminal_set_open(!self.ai_terminal_open, cx);
    }

    pub(super) fn ai_increase_terminal_height_action(&mut self, cx: &mut Context<Self>) {
        self.ai_terminal_height_px = (self.ai_terminal_height_px + AI_TERMINAL_HEIGHT_STEP_PX)
            .clamp(AI_TERMINAL_MIN_HEIGHT_PX, AI_TERMINAL_MAX_HEIGHT_PX);
        cx.notify();
    }

    pub(super) fn ai_decrease_terminal_height_action(&mut self, cx: &mut Context<Self>) {
        self.ai_terminal_height_px = (self.ai_terminal_height_px - AI_TERMINAL_HEIGHT_STEP_PX)
            .clamp(AI_TERMINAL_MIN_HEIGHT_PX, AI_TERMINAL_MAX_HEIGHT_PX);
        cx.notify();
    }

    pub(super) fn ai_clear_terminal_session_action(&mut self, cx: &mut Context<Self>) {
        self.ai_terminal_session.transcript.clear();
        self.ai_terminal_session.status_message = None;
        self.ai_terminal_session.exit_code = None;
        if self.ai_terminal_runtime.is_none() {
            self.ai_terminal_session.status = AiTerminalSessionStatus::Idle;
        }
        cx.notify();
    }

    pub(super) fn ai_stop_terminal_command_action(&mut self, cx: &mut Context<Self>) {
        let Some(runtime) = self.ai_terminal_runtime.as_ref() else {
            return;
        };
        if let Err(error) = runtime.handle.kill() {
            self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
            self.ai_terminal_session.status_message = Some(error.to_string());
            cx.notify();
            return;
        }
        self.ai_terminal_stop_requested = true;
        self.ai_terminal_session.status_message = Some("Stopping command...".to_string());
        cx.notify();
    }

    pub(crate) fn stop_ai_terminal_runtime(&mut self, reason: &str) {
        self.ai_terminal_stop_requested = false;
        self.ai_terminal_event_task = Task::ready(());
        if let Some(runtime) = self.ai_terminal_runtime.take()
            && let Err(error) = runtime.handle.kill()
        {
            error!("failed to stop AI terminal runtime during {reason}: {error:#}");
        }
    }

    pub(super) fn ai_rerun_terminal_command_action(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self.ai_terminal_session.last_command.clone() else {
            self.ai_terminal_session.status_message = Some("No command to rerun.".to_string());
            cx.notify();
            return;
        };
        self.ai_terminal_input_draft = command;
        self.restore_ai_visible_terminal_input(cx);
        self.ai_run_terminal_command_action(cx);
    }

    pub(super) fn ai_run_terminal_command_action(&mut self, cx: &mut Context<Self>) {
        self.sync_ai_visible_terminal_input_to_state(cx);
        let command = self.ai_terminal_input_draft.trim().to_string();
        if command.is_empty() {
            self.ai_terminal_session.status_message = Some("Command cannot be empty.".to_string());
            self.ai_terminal_session.status = AiTerminalSessionStatus::Idle;
            cx.notify();
            return;
        }

        let Some(cwd) = self.ai_workspace_cwd() else {
            self.ai_terminal_session.status_message =
                Some("Open a workspace before using the terminal.".to_string());
            self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return;
        };

        self.stop_ai_terminal_runtime("starting terminal command");

        let workspace_key = cwd.to_string_lossy().to_string();
        let request = TerminalSpawnRequest::new(cwd.clone(), command.clone());
        match spawn_terminal_session(request) {
            Ok((handle, event_rx)) => {
                self.ai_terminal_open = true;
                self.ai_terminal_stop_requested = false;
                self.ai_terminal_session.cwd = Some(cwd);
                self.ai_terminal_session.last_command = Some(command.clone());
                self.ai_terminal_session.status = AiTerminalSessionStatus::Running;
                self.ai_terminal_session.exit_code = None;
                self.ai_terminal_session.status_message = Some("Running command...".to_string());
                append_ai_terminal_transcript(
                    &mut self.ai_terminal_session.transcript,
                    format!("$ {command}\n"),
                );
                let generation = self.next_ai_terminal_runtime_generation();
                self.ai_terminal_runtime = Some(AiTerminalRuntimeHandle {
                    workspace_key: workspace_key.clone(),
                    handle,
                    generation,
                });
                self.start_ai_terminal_event_listener(event_rx, workspace_key, generation, cx);
                cx.notify();
            }
            Err(error) => {
                self.ai_terminal_open = true;
                self.ai_terminal_session.cwd = Some(cwd);
                self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                self.ai_terminal_session.exit_code = None;
                self.ai_terminal_session.status_message =
                    Some("Failed to start terminal command.".to_string());
                append_ai_terminal_transcript(
                    &mut self.ai_terminal_session.transcript,
                    format!("$ {command}\n[terminal error] {error}\n"),
                );
                cx.notify();
            }
        }
    }

    fn start_ai_terminal_event_listener(
        &mut self,
        event_rx: std::sync::mpsc::Receiver<TerminalEvent>,
        workspace_key: String,
        generation: usize,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_event_task = cx.spawn(async move |this, cx| {
            loop {
                let (buffered_events, event_stream_disconnected) =
                    drain_ai_terminal_events(&event_rx);

                if buffered_events.is_empty() && !event_stream_disconnected {
                    cx.background_executor()
                        .timer(AI_TERMINAL_EVENT_POLL_INTERVAL)
                        .await;
                    continue;
                }

                let Some(this) = this.upgrade() else {
                    return;
                };
                let mut listener_is_current = true;
                this.update(cx, |this, cx| {
                    if !this.ai_terminal_runtime_is_current(workspace_key.as_str(), generation) {
                        listener_is_current = false;
                        return;
                    }
                    for event in buffered_events {
                        this.apply_ai_terminal_event(event);
                    }
                    if event_stream_disconnected && this.ai_terminal_runtime_is_current(workspace_key.as_str(), generation) {
                        this.ai_terminal_runtime = None;
                    }
                    cx.notify();
                });
                if !listener_is_current || event_stream_disconnected {
                    return;
                }
            }
        });
    }

    fn apply_ai_terminal_event(&mut self, event: TerminalEvent) {
        match event {
            TerminalEvent::Output(output) => {
                let sanitized = sanitize_ai_terminal_output(output.as_str());
                if sanitized.is_empty() {
                    return;
                }
                append_ai_terminal_transcript(&mut self.ai_terminal_session.transcript, sanitized);
            }
            TerminalEvent::Exit { exit_code } => {
                let stopped = self.ai_terminal_stop_requested;
                self.ai_terminal_stop_requested = false;
                self.ai_terminal_runtime = None;
                self.ai_terminal_session.exit_code = exit_code;
                if stopped {
                    self.ai_terminal_session.status = AiTerminalSessionStatus::Stopped;
                    self.ai_terminal_session.status_message = Some("Command stopped.".to_string());
                    append_ai_terminal_transcript(
                        &mut self.ai_terminal_session.transcript,
                        "[stopped]\n".to_string(),
                    );
                } else if exit_code == Some(0) {
                    self.ai_terminal_session.status = AiTerminalSessionStatus::Completed;
                    self.ai_terminal_session.status_message = Some("Command completed.".to_string());
                    append_ai_terminal_transcript(
                        &mut self.ai_terminal_session.transcript,
                        "[exit 0]\n".to_string(),
                    );
                } else {
                    self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                    self.ai_terminal_session.status_message = Some(
                        exit_code
                            .map(|code| format!("Command failed with exit code {code}."))
                            .unwrap_or_else(|| "Command failed.".to_string()),
                    );
                    append_ai_terminal_transcript(
                        &mut self.ai_terminal_session.transcript,
                        format!("[exit {}]\n", exit_code.unwrap_or(-1)),
                    );
                }
            }
            TerminalEvent::Failed(message) => {
                self.ai_terminal_runtime = None;
                self.ai_terminal_stop_requested = false;
                self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                self.ai_terminal_session.status_message = Some(message.clone());
                append_ai_terminal_transcript(
                    &mut self.ai_terminal_session.transcript,
                    format!("[terminal error] {message}\n"),
                );
            }
        }
    }
}

fn append_ai_terminal_transcript(buffer: &mut String, text: String) {
    if text.is_empty() {
        return;
    }
    if !buffer.is_empty() && !buffer.ends_with('\n') && !text.starts_with('\n') {
        buffer.push('\n');
    }
    buffer.push_str(text.as_str());
    trim_ai_terminal_transcript(buffer);
}

fn trim_ai_terminal_transcript(buffer: &mut String) {
    if buffer.len() <= AI_TERMINAL_MAX_TRANSCRIPT_BYTES {
        return;
    }

    let target_len = AI_TERMINAL_MAX_TRANSCRIPT_BYTES / 2;
    let mut start = buffer.len().saturating_sub(target_len);
    while start < buffer.len() && !buffer.is_char_boundary(start) {
        start = start.saturating_add(1);
    }
    let trimmed = buffer[start..].to_string();
    *buffer = format!("[output truncated]\n{trimmed}");
}

fn drain_ai_terminal_events(
    event_rx: &std::sync::mpsc::Receiver<TerminalEvent>,
) -> (Vec<TerminalEvent>, bool) {
    let mut events = Vec::new();
    let mut disconnected = false;
    loop {
        match event_rx.try_recv() {
            Ok(event) => events.push(event),
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                disconnected = true;
                break;
            }
        }
    }
    (events, disconnected)
}

fn sanitize_ai_terminal_output(output: &str) -> String {
    let normalized = output.replace("\r\n", "\n").replace('\r', "\n");
    strip_ansi_sequences(normalized.as_str())
}

fn strip_ansi_sequences(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == 0x1b {
            index = skip_ansi_sequence(bytes, index);
            continue;
        }
        if byte == 0x08 {
            output.pop();
            index = index.saturating_add(1);
            continue;
        }
        output.push(byte);
        index = index.saturating_add(1);
    }

    String::from_utf8_lossy(&output).to_string()
}

fn skip_ansi_sequence(bytes: &[u8], start: usize) -> usize {
    let Some(next) = bytes.get(start.saturating_add(1)).copied() else {
        return start.saturating_add(1);
    };

    if next == b'[' {
        let mut index = start.saturating_add(2);
        while index < bytes.len() {
            let byte = bytes[index];
            if (0x40..=0x7e).contains(&byte) {
                return index.saturating_add(1);
            }
            index = index.saturating_add(1);
        }
        return bytes.len();
    }

    if next == b']' {
        let mut index = start.saturating_add(2);
        while index < bytes.len() {
            if bytes[index] == 0x07 {
                return index.saturating_add(1);
            }
            if bytes[index] == 0x1b && bytes.get(index.saturating_add(1)) == Some(&b'\\') {
                return index.saturating_add(2);
            }
            index = index.saturating_add(1);
        }
        return bytes.len();
    }

    start.saturating_add(2)
}

#[cfg(test)]
mod terminal_tests {
    use super::{sanitize_ai_terminal_output, strip_ansi_sequences};

    #[test]
    fn strips_basic_ansi_sequences() {
        let value = strip_ansi_sequences("\u{1b}[31merror\u{1b}[0m");
        assert_eq!(value, "error");
    }

    #[test]
    fn normalizes_carriage_returns() {
        let value = sanitize_ai_terminal_output("hello\rworld\r\nnext");
        assert_eq!(value, "hello\nworld\nnext");
    }
}
