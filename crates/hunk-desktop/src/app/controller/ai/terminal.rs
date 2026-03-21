const AI_TERMINAL_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(33);
const AI_TERMINAL_MAX_TRANSCRIPT_BYTES: usize = 256 * 1024;
const AI_TERMINAL_MIN_HEIGHT_PX: f32 = 140.0;
const AI_TERMINAL_MAX_HEIGHT_PX: f32 = 520.0;

impl DiffViewer {
    pub(crate) fn ai_terminal_is_running(&self) -> bool {
        self.ai_terminal_session.status == AiTerminalSessionStatus::Running
    }

    fn ai_terminal_selection_active(&self) -> bool {
        self.ai_text_selection
            .as_ref()
            .is_some_and(|selection| {
                selection.row_id == crate::app::AI_TERMINAL_TEXT_SELECTION_ROW_ID
            })
    }

    fn clear_ai_terminal_text_selection(&mut self, cx: &mut Context<Self>) {
        if self.ai_terminal_selection_active() {
            self.ai_clear_text_selection(cx);
        }
    }

    fn focus_ai_terminal_surface(&mut self, cx: &mut Context<Self>) {
        let focus_handle = self.ai_terminal_focus_handle.clone();
        if let Err(error) = Self::update_any_window(cx, move |window, cx| {
            focus_handle.focus(window, cx);
        }) {
            error!("failed to focus AI terminal surface: {error:#}");
        }
    }

    fn defer_ai_terminal_interaction_focus(&self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let focus_surface = self.ai_terminal_is_running();
        let terminal_focus_handle = self.ai_terminal_focus_handle.clone();
        let terminal_input_state = self.ai_terminal_input_state.clone();
        cx.defer(move |cx| {
            let result = cx.update_window(window_handle, |_, window, cx| {
                if focus_surface {
                    terminal_focus_handle.focus(window, cx);
                } else {
                    terminal_input_state.update(cx, |state, cx| {
                        state.focus(window, cx);
                    });
                }
            });
            if let Err(err) = result
                && !Self::is_window_not_found_error(&err)
            {
                error!("failed to defer AI terminal focus: {err:#}");
            }
        });
    }

    fn defer_ai_composer_focus(&self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let composer_input_state = self.ai_composer_input_state.clone();
        cx.defer(move |cx| {
            let result = cx.update_window(window_handle, |_, window, cx| {
                composer_input_state.update(cx, |state, cx| {
                    state.focus(window, cx);
                });
            });
            if let Err(err) = result
                && !Self::is_window_not_found_error(&err)
            {
                error!("failed to defer AI composer focus: {err:#}");
            }
        });
    }

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

    fn ai_terminal_set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.ai_terminal_open == open {
            return;
        }
        self.ai_terminal_open = open;
        if !open {
            self.ai_terminal_surface_focused = false;
            self.defer_ai_composer_focus(cx);
        }
        cx.notify();
    }

    fn toggle_ai_terminal_drawer(&mut self, cx: &mut Context<Self>) {
        let next_open = !self.ai_terminal_open;
        self.ai_terminal_set_open(next_open, cx);
        if next_open {
            self.ensure_ai_terminal_session(cx);
            self.defer_ai_terminal_interaction_focus(cx);
        }
    }

    pub(super) fn ai_toggle_terminal_drawer_shortcut_action(
        &mut self,
        _: &AiToggleTerminalDrawer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            self.activate_ai_workspace(window, cx);
            self.ai_terminal_set_open(true, cx);
            self.ensure_ai_terminal_session(cx);
            self.defer_ai_terminal_interaction_focus(cx);
            return;
        }
        self.toggle_ai_terminal_drawer(cx);
    }

    pub(super) fn ai_toggle_terminal_drawer_action(&mut self, cx: &mut Context<Self>) {
        self.toggle_ai_terminal_drawer(cx);
    }

    pub(super) fn ai_clear_terminal_session_action(&mut self, cx: &mut Context<Self>) {
        if !self.ai_terminal_is_running() {
            self.stop_ai_terminal_runtime("clearing terminal session");
        }
        self.ai_terminal_session.transcript.clear();
        self.ai_terminal_session.screen = None;
        self.ai_terminal_session.status_message = None;
        self.ai_terminal_session.exit_code = None;
        self.ai_terminal_follow_output = true;
        if !self.ai_terminal_is_running() {
            self.ai_terminal_session.status = AiTerminalSessionStatus::Idle;
        }
        if self.ai_terminal_open {
            self.ensure_ai_terminal_session(cx);
        }
        cx.notify();
    }

    fn ensure_ai_terminal_session(&mut self, cx: &mut Context<Self>) {
        if self.ai_terminal_runtime.is_some() || self.ai_terminal_session.screen.is_some() {
            return;
        }

        let Some(cwd) = self.ai_workspace_cwd() else {
            return;
        };

        self.start_default_ai_terminal_session(cwd, cx);
    }

    pub(crate) fn stop_ai_terminal_runtime(&mut self, reason: &str) {
        self.ai_terminal_stop_requested = false;
        self.ai_terminal_event_task = Task::ready(());
        if let Some(runtime) = self.ai_terminal_runtime.take()
            && self.ai_terminal_is_running()
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

    pub(super) fn ai_submit_terminal_input_action(&mut self, cx: &mut Context<Self>) {
        if self.ai_terminal_is_running() {
            self.ai_send_terminal_input_action(cx);
        } else {
            self.ai_run_terminal_command_action(cx);
        }
    }

    pub(super) fn ai_focus_terminal_surface_action(&mut self, cx: &mut Context<Self>) {
        if self.ai_terminal_runtime.is_some() {
            self.focus_ai_terminal_surface(cx);
        }
    }

    pub(super) fn ai_terminal_surface_focus_in(&mut self, cx: &mut Context<Self>) {
        if !self.ai_terminal_surface_focused {
            self.ai_terminal_surface_focused = true;
            cx.notify();
        }
        self.ai_report_terminal_focus_change(true, cx);
    }

    pub(super) fn ai_terminal_surface_focus_out(&mut self, cx: &mut Context<Self>) {
        if self.ai_terminal_surface_focused {
            self.ai_terminal_surface_focused = false;
            cx.notify();
        }
        self.ai_report_terminal_focus_change(false, cx);
    }

    pub(super) fn ai_terminal_surface_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        line: i32,
        column: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let mode = self.ai_terminal_session.screen.as_ref().map(|screen| screen.mode);
        let point = AiTerminalGridPoint { line, column };
        let Some(bytes) =
            ai_terminal_mouse_button_bytes(point, event.button, event.modifiers, true, mode)
        else {
            return false;
        };
        self.ai_write_terminal_bytes(bytes.as_slice(), cx)
    }

    pub(super) fn ai_terminal_surface_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        line: i32,
        column: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let mode = self.ai_terminal_session.screen.as_ref().map(|screen| screen.mode);
        let point = AiTerminalGridPoint { line, column };
        let Some(bytes) =
            ai_terminal_mouse_move_bytes(point, event.pressed_button, event.modifiers, mode)
        else {
            return false;
        };
        self.ai_write_terminal_bytes(bytes.as_slice(), cx)
    }

    pub(super) fn ai_terminal_surface_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        line: i32,
        column: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let mode = self.ai_terminal_session.screen.as_ref().map(|screen| screen.mode);
        let point = AiTerminalGridPoint { line, column };
        let Some(bytes) =
            ai_terminal_mouse_button_bytes(point, event.button, event.modifiers, false, mode)
        else {
            return false;
        };
        self.ai_write_terminal_bytes(bytes.as_slice(), cx)
    }

    pub(super) fn ai_terminal_surface_key_down(
        &mut self,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.ai_terminal_runtime.is_none() || !self.ai_terminal_focus_handle.is_focused(window) {
            return false;
        }

        if ai_terminal_uses_copy_shortcut(keystroke) && self.ai_terminal_selection_active() {
            return self.ai_copy_selected_text(cx);
        }

        let terminal_mode = self.ai_terminal_session.screen.as_ref().map(|screen| screen.mode);

        if let Some(scroll) = ai_terminal_viewport_scroll_for_keystroke(keystroke, terminal_mode) {
            return self.ai_scroll_terminal_viewport(scroll, cx);
        }

        if !self.ai_terminal_is_running() {
            return false;
        }

        if ai_terminal_uses_desktop_clipboard_shortcut(keystroke) {
            if keystroke.key == "v" {
                return self.ai_paste_terminal_from_clipboard(cx);
            }
            return false;
        }

        let Some(bytes) = ai_terminal_input_bytes_for_keystroke(keystroke, terminal_mode) else {
            return false;
        };
        self.ai_write_terminal_bytes(bytes.as_slice(), cx)
    }

    pub(super) fn ai_terminal_surface_scroll_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        line: i32,
        column: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.ai_terminal_runtime.is_none() {
            return false;
        }

        let line_height = px(16.0);
        let Some((direction, line_count)) =
            crate::app::native_files_editor::scroll_direction_and_count(event, line_height)
        else {
            return false;
        };

        let delta = match direction {
            crate::app::native_files_editor::ScrollDirection::Forward => -(line_count as i32),
            crate::app::native_files_editor::ScrollDirection::Backward => line_count as i32,
        };

        let mode = self.ai_terminal_session.screen.as_ref().map(|screen| screen.mode);
        let point = AiTerminalGridPoint { line, column };
        if let Some(reports) =
            ai_terminal_mouse_scroll_bytes(point, delta, event.modifiers, mode)
        {
            return self.ai_write_terminal_report_chunks(reports, cx);
        }

        if let Some(bytes) = ai_terminal_alt_scroll_bytes(delta, mode) {
            return self.ai_write_terminal_bytes(bytes.as_slice(), cx);
        }

        if mode.is_some_and(|mode| mode.alt_screen) {
            return true;
        }

        self.ai_scroll_terminal_viewport(TerminalScroll::Delta(delta), cx)
    }

    pub(super) fn ai_scroll_terminal_to_bottom_action(&mut self, cx: &mut Context<Self>) {
        let _ = self.ai_scroll_terminal_viewport(TerminalScroll::Bottom, cx);
    }

    pub(super) fn ai_update_terminal_panel_bounds(
        &mut self,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let bounds_changed = self.ai_terminal_panel_bounds.is_none_or(|current| {
            (current.origin.x - bounds.origin.x).abs() > px(0.5)
                || (current.origin.y - bounds.origin.y).abs() > px(0.5)
                || (current.size.width - bounds.size.width).abs() > px(0.5)
                || (current.size.height - bounds.size.height).abs() > px(0.5)
        });
        if !bounds_changed {
            return;
        }
        self.ai_terminal_panel_bounds = Some(bounds);
        cx.notify();
    }

    pub(super) fn ai_resize_terminal_height_from_position(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.ai_terminal_panel_bounds else {
            return;
        };
        let next_height = (bounds.bottom() - position.y).max(px(AI_TERMINAL_MIN_HEIGHT_PX));
        let clamped_height = next_height
            .min(px(AI_TERMINAL_MAX_HEIGHT_PX))
            .max(px(AI_TERMINAL_MIN_HEIGHT_PX));
        let next_height_px: f32 = clamped_height.into();
        if (next_height_px - self.ai_terminal_height_px).abs() <= 0.5 {
            return;
        }
        self.ai_terminal_height_px = next_height_px;
        cx.notify();
    }

    pub(super) fn ai_resize_terminal_surface(
        &mut self,
        rows: u16,
        cols: u16,
        cx: &mut Context<Self>,
    ) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        if self.ai_terminal_grid_size == Some((rows, cols)) {
            return;
        }
        self.ai_terminal_grid_size = Some((rows, cols));

        let Some(runtime) = self.ai_terminal_runtime.as_ref() else {
            return;
        };
        if let Err(error) = runtime.handle.resize(rows, cols) {
            self.ai_terminal_session.status_message = Some(error.to_string());
            self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
        }
    }

    fn ai_scroll_terminal_viewport(
        &mut self,
        scroll: TerminalScroll,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .ai_terminal_session
            .screen
            .as_ref()
            .is_some_and(|screen| screen.mode.alt_screen)
        {
            return false;
        }

        self.clear_ai_terminal_text_selection(cx);

        let Some(runtime) = self.ai_terminal_runtime.as_ref() else {
            return false;
        };

        if let Err(error) = runtime.handle.scroll_display(scroll) {
            self.ai_terminal_session.status_message = Some(error.to_string());
            self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return true;
        }

        true
    }

    fn ai_write_terminal_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) -> bool {
        if !self.ai_terminal_is_running() {
            return false;
        }
        let Some(runtime) = self.ai_terminal_runtime.as_ref() else {
            return false;
        };

        if let Err(error) = runtime.handle.write_input(bytes) {
            self.ai_terminal_session.status_message = Some(error.to_string());
            self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return true;
        }

        self.ai_terminal_session.status_message = None;
        true
    }

    fn ai_write_terminal_report_chunks(
        &mut self,
        reports: Vec<Vec<u8>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut handled = false;
        for report in reports {
            handled = self.ai_write_terminal_bytes(report.as_slice(), cx) || handled;
            if self.ai_terminal_session.status == AiTerminalSessionStatus::Failed {
                break;
            }
        }
        handled
    }

    fn ai_report_terminal_focus_change(&mut self, focused: bool, cx: &mut Context<Self>) {
        let mode = self.ai_terminal_session.screen.as_ref().map(|screen| screen.mode);
        let Some(bytes) = ai_terminal_focus_bytes(focused, mode) else {
            return;
        };
        let _ = self.ai_write_terminal_bytes(bytes, cx);
    }

    fn ai_paste_terminal_from_clipboard(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        if text.is_empty() {
            return false;
        }

        let bracketed_paste = self
            .ai_terminal_session
            .screen
            .as_ref()
            .is_some_and(|screen| screen.mode.bracketed_paste);
        let bytes = ai_terminal_paste_bytes(text.as_str(), bracketed_paste);
        self.ai_write_terminal_bytes(bytes.as_slice(), cx)
    }

    pub(super) fn ai_send_terminal_input_action(&mut self, cx: &mut Context<Self>) {
        self.sync_ai_visible_terminal_input_to_state(cx);
        if !self.ai_terminal_is_running() {
            self.ai_run_terminal_command_action(cx);
            return;
        }
        let Some(runtime) = self.ai_terminal_runtime.as_ref() else {
            return;
        };

        let mut input = self.ai_terminal_input_draft.clone();
        if input.is_empty() {
            return;
        }
        if !input.ends_with('\n') {
            input.push('\n');
        }

        if let Err(error) = runtime.handle.write_input(input.as_bytes()) {
            self.ai_terminal_session.status_message = Some(error.to_string());
            self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return;
        }

        self.ai_terminal_session.status_message = None;
        self.ai_terminal_input_draft.clear();
        self.restore_ai_visible_terminal_input(cx);
        cx.notify();
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

        self.start_ai_terminal_command_session(cwd, command, cx);
    }

    fn start_default_ai_terminal_session(&mut self, cwd: PathBuf, cx: &mut Context<Self>) {
        self.stop_ai_terminal_runtime("starting default terminal shell");

        let workspace_key = cwd.to_string_lossy().to_string();
        let request = TerminalSpawnRequest::shell(cwd.clone());
        match spawn_terminal_session(request) {
            Ok((handle, event_rx)) => {
                self.ai_terminal_open = true;
                self.ai_terminal_stop_requested = false;
                self.ai_terminal_session.cwd = Some(cwd);
                self.ai_terminal_session.last_command = None;
                self.ai_terminal_session.status = AiTerminalSessionStatus::Running;
                self.ai_terminal_session.exit_code = None;
                self.ai_terminal_session.screen = None;
                self.ai_terminal_follow_output = true;
                self.ai_terminal_session.status_message = None;
                let generation = self.next_ai_terminal_runtime_generation();
                self.ai_terminal_runtime = Some(AiTerminalRuntimeHandle {
                    workspace_key: workspace_key.clone(),
                    handle,
                    generation,
                });
                self.start_ai_terminal_event_listener(event_rx, workspace_key, generation, cx);
                self.defer_ai_terminal_interaction_focus(cx);
            }
            Err(error) => {
                self.ai_terminal_open = true;
                self.ai_terminal_session.cwd = Some(cwd);
                self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                self.ai_terminal_session.exit_code = None;
                self.ai_terminal_session.screen = None;
                self.ai_terminal_session.status_message =
                    Some("Failed to start terminal shell.".to_string());
                append_ai_terminal_transcript(
                    &mut self.ai_terminal_session.transcript,
                    format!("[terminal error] {error}\n"),
                );
            }
        }
    }

    fn start_ai_terminal_command_session(
        &mut self,
        cwd: PathBuf,
        command: String,
        cx: &mut Context<Self>,
    ) {
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
                self.ai_terminal_session.screen = None;
                self.ai_terminal_follow_output = true;
                self.ai_terminal_session.status_message = None;
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
                self.defer_ai_terminal_interaction_focus(cx);
                cx.notify();
            }
            Err(error) => {
                self.ai_terminal_open = true;
                self.ai_terminal_session.cwd = Some(cwd);
                self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                self.ai_terminal_session.exit_code = None;
                self.ai_terminal_session.screen = None;
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
                        this.apply_ai_terminal_event(event, cx);
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

    fn apply_ai_terminal_event(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        match event {
            TerminalEvent::Output(output) => {
                let sanitized = sanitize_ai_terminal_output(output.as_slice());
                if sanitized.is_empty() {
                    return;
                }
                append_ai_terminal_transcript(&mut self.ai_terminal_session.transcript, sanitized);
            }
            TerminalEvent::Screen(screen) => {
                if self.ai_terminal_is_running() {
                    self.clear_ai_terminal_text_selection(cx);
                }
                self.ai_terminal_follow_output = screen.display_offset == 0;
                self.ai_terminal_session.screen = Some(screen);
            }
            TerminalEvent::Exit { exit_code } => {
                let stopped = self.ai_terminal_stop_requested;
                self.ai_terminal_stop_requested = false;
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

fn sanitize_ai_terminal_output(output: &[u8]) -> String {
    let normalized = String::from_utf8_lossy(output).replace("\r\n", "\n").replace('\r', "\n");
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
mod terminal_output_tests {
    use super::{sanitize_ai_terminal_output, strip_ansi_sequences};

    #[test]
    fn strips_basic_ansi_sequences() {
        let value = strip_ansi_sequences("\u{1b}[31merror\u{1b}[0m");
        assert_eq!(value, "error");
    }

    #[test]
    fn normalizes_carriage_returns() {
        let value = sanitize_ai_terminal_output(b"hello\rworld\r\nnext");
        assert_eq!(value, "hello\nworld\nnext");
    }
}
