const AI_TERMINAL_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(33);
const AI_TERMINAL_MAX_TRANSCRIPT_BYTES: usize = 256 * 1024;
const AI_TERMINAL_MIN_HEIGHT_PX: f32 = 140.0;
const AI_TERMINAL_MAX_HEIGHT_PX: f32 = 520.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiTerminalShellFamily {
    Posix,
    Cmd,
    PowerShell,
}

const AI_TERMINAL_SHELL_WRAPPERS: &[(&str, AiTerminalShellFamily)] = &[
    ("/usr/bin/env zsh -lc ", AiTerminalShellFamily::Posix),
    ("env zsh -lc ", AiTerminalShellFamily::Posix),
    ("/bin/zsh -lc ", AiTerminalShellFamily::Posix),
    ("zsh -lc ", AiTerminalShellFamily::Posix),
    ("/usr/bin/env bash -lc ", AiTerminalShellFamily::Posix),
    ("env bash -lc ", AiTerminalShellFamily::Posix),
    ("/bin/bash -lc ", AiTerminalShellFamily::Posix),
    ("bash -lc ", AiTerminalShellFamily::Posix),
    ("/usr/bin/env sh -lc ", AiTerminalShellFamily::Posix),
    ("env sh -lc ", AiTerminalShellFamily::Posix),
    ("/bin/sh -lc ", AiTerminalShellFamily::Posix),
    ("sh -lc ", AiTerminalShellFamily::Posix),
    ("/usr/bin/env bash -c ", AiTerminalShellFamily::Posix),
    ("env bash -c ", AiTerminalShellFamily::Posix),
    ("/bin/bash -c ", AiTerminalShellFamily::Posix),
    ("bash -c ", AiTerminalShellFamily::Posix),
    ("/usr/bin/env sh -c ", AiTerminalShellFamily::Posix),
    ("env sh -c ", AiTerminalShellFamily::Posix),
    ("/bin/sh -c ", AiTerminalShellFamily::Posix),
    ("sh -c ", AiTerminalShellFamily::Posix),
    ("cmd /D /C ", AiTerminalShellFamily::Cmd),
    ("cmd /C ", AiTerminalShellFamily::Cmd),
    ("cmd.exe /D /C ", AiTerminalShellFamily::Cmd),
    ("cmd.exe /C ", AiTerminalShellFamily::Cmd),
    ("powershell -Command ", AiTerminalShellFamily::PowerShell),
    ("powershell -command ", AiTerminalShellFamily::PowerShell),
    ("powershell.exe -Command ", AiTerminalShellFamily::PowerShell),
    ("powershell.exe -command ", AiTerminalShellFamily::PowerShell),
    ("pwsh -Command ", AiTerminalShellFamily::PowerShell),
    ("pwsh -command ", AiTerminalShellFamily::PowerShell),
    ("pwsh.exe -Command ", AiTerminalShellFamily::PowerShell),
    ("pwsh.exe -command ", AiTerminalShellFamily::PowerShell),
];

impl DiffViewer {
    pub(crate) fn workspace_terminal_surface_focused(
        &self,
        kind: WorkspaceTerminalKind,
    ) -> bool {
        match kind {
            WorkspaceTerminalKind::Ai => self.ai_terminal_surface_focused,
            WorkspaceTerminalKind::Files => self.files_terminal_surface_focused,
        }
    }

    pub(crate) fn workspace_terminal_cursor_blink_visible(
        &self,
        kind: WorkspaceTerminalKind,
    ) -> bool {
        match kind {
            WorkspaceTerminalKind::Ai => self.ai_terminal_cursor_blink_visible,
            WorkspaceTerminalKind::Files => self.files_terminal_cursor_blink_visible,
        }
    }

    pub(crate) fn workspace_terminal_cursor_output_suppressed(
        &self,
        kind: WorkspaceTerminalKind,
    ) -> bool {
        match kind {
            WorkspaceTerminalKind::Ai => self.ai_terminal_cursor_output_suppressed,
            WorkspaceTerminalKind::Files => self.files_terminal_cursor_output_suppressed,
        }
    }

    pub(crate) fn workspace_terminal_open(&self, kind: WorkspaceTerminalKind) -> bool {
        match kind {
            WorkspaceTerminalKind::Ai => self.ai_terminal_open,
            WorkspaceTerminalKind::Files => self.files_terminal_open,
        }
    }

    pub(crate) fn active_workspace_terminal_kind(&self) -> Option<WorkspaceTerminalKind> {
        match self.workspace_view_mode {
            WorkspaceViewMode::Ai => Some(WorkspaceTerminalKind::Ai),
            WorkspaceViewMode::Files => Some(WorkspaceTerminalKind::Files),
            _ => None,
        }
    }

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
        let terminal_focus_handle = self.ai_terminal_focus_handle.clone();
        cx.defer(move |cx| {
            let result = cx.update_window(window_handle, |_, window, cx| {
                terminal_focus_handle.focus(window, cx);
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

    fn ai_capture_visible_terminal_state(&self) -> AiThreadTerminalState {
        AiThreadTerminalState {
            open: self.ai_terminal_open,
            follow_output: self.ai_terminal_follow_output,
            session: self.ai_terminal_session.clone(),
            pending_input: self.ai_terminal_pending_input.clone(),
        }
    }

    fn ai_apply_visible_terminal_state(&mut self, state: AiThreadTerminalState) {
        self.ai_terminal_open = state.open;
        self.ai_terminal_follow_output = state.follow_output;
        self.ai_terminal_session = state.session;
        self.ai_terminal_pending_input = state.pending_input;
        self.ai_terminal_surface_focused = false;
        self.ai_terminal_cursor_blink_generation =
            self.ai_terminal_cursor_blink_generation.saturating_add(1);
        self.ai_terminal_cursor_blink_visible = true;
        self.ai_terminal_cursor_blink_active = false;
        self.ai_terminal_cursor_blink_task = Task::ready(());
        self.ai_terminal_cursor_output_generation =
            self.ai_terminal_cursor_output_generation.saturating_add(1);
        self.ai_terminal_cursor_output_suppressed = false;
        self.ai_terminal_cursor_output_task = Task::ready(());
        self.ai_terminal_grid_size = self
            .ai_terminal_session
            .screen
            .as_ref()
            .map(|screen| (screen.rows, screen.cols));
    }

    fn ai_store_visible_terminal_state_for_thread(&mut self, thread_id: Option<&str>) {
        let Some(thread_id) = thread_id else {
            return;
        };
        self.ai_terminal_states_by_thread
            .insert(thread_id.to_string(), self.ai_capture_visible_terminal_state());
    }

    fn ai_terminal_owner_key_for_thread(&self, thread_id: &str) -> Option<String> {
        self.ai_thread_workspace_root(thread_id)
            .map(|root| root.to_string_lossy().to_string())
    }

    fn ai_current_terminal_owner_key(&self) -> Option<String> {
        self.ai_workspace_key()
    }

    fn ai_restore_visible_terminal_state_for_thread(&mut self, thread_id: Option<&str>) {
        let state = thread_id
            .and_then(|thread_id| self.ai_terminal_states_by_thread.get(thread_id).cloned())
            .unwrap_or_default();
        self.ai_apply_visible_terminal_state(state);
    }

    fn ai_park_visible_terminal_runtime_for_thread(&mut self, thread_id: Option<&str>) {
        let Some(thread_id) = thread_id else {
            return;
        };
        let Some(runtime) = self.ai_terminal_runtime.take() else {
            return;
        };
        let event_task = std::mem::replace(&mut self.ai_terminal_event_task, Task::ready(()));
        self.ai_hidden_terminal_runtimes.insert(
            thread_id.to_string(),
            AiHiddenTerminalRuntimeHandle { runtime, event_task },
        );
    }

    fn ai_promote_hidden_terminal_runtime_for_thread(&mut self, thread_id: &str) -> bool {
        let Some(hidden) = self.ai_hidden_terminal_runtimes.remove(thread_id) else {
            return false;
        };
        self.ai_terminal_runtime = Some(hidden.runtime);
        self.ai_terminal_event_task = hidden.event_task;
        true
    }

    pub(super) fn ai_handle_terminal_thread_change(
        &mut self,
        previous_thread_id: Option<String>,
        next_thread_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let previous_terminal_owner_thread_id = self
            .ai_terminal_runtime
            .as_ref()
            .map(|runtime| runtime.thread_id.clone())
            .or_else(|| {
                previous_thread_id
                    .as_deref()
                    .and_then(|thread_id| self.ai_terminal_owner_key_for_thread(thread_id))
            });
        let next_terminal_owner_thread_id = next_thread_id
            .as_deref()
            .and_then(|thread_id| self.ai_terminal_owner_key_for_thread(thread_id))
            .or_else(|| self.ai_current_terminal_owner_key());
        if previous_terminal_owner_thread_id == next_terminal_owner_thread_id {
            return;
        }

        self.ai_store_visible_terminal_state_for_thread(previous_terminal_owner_thread_id.as_deref());
        self.ai_park_visible_terminal_runtime_for_thread(previous_terminal_owner_thread_id.as_deref());
        self.ai_restore_visible_terminal_state_for_thread(next_terminal_owner_thread_id.as_deref());

        if let Some(next_thread_id) = next_terminal_owner_thread_id.as_deref() {
            if !self.ai_promote_hidden_terminal_runtime_for_thread(next_thread_id)
                && self.ai_terminal_open
            {
                debug!(
                    thread_id = next_thread_id,
                    has_screen = self.ai_terminal_session.screen.is_some(),
                    has_runtime = self.ai_terminal_runtime.is_some(),
                    "Ensuring AI terminal session after thread selection"
                );
                self.ensure_ai_terminal_session(cx);
            }
        } else {
            self.ai_terminal_open = false;
            self.ai_terminal_surface_focused = false;
        }

        self.ai_sync_terminal_cursor_blink(cx);
        cx.notify();
    }

    pub(super) fn ai_prune_terminal_threads(&mut self, reason: &str, cx: &mut Context<Self>) {
        if self.ai_terminal_runtime.is_none()
            && self.ai_hidden_terminal_runtimes.is_empty()
            && self.ai_terminal_states_by_thread.is_empty()
        {
            return;
        }

        let mut retained_thread_ids = ai_retainable_terminal_thread_ids(
            &self.ai_state_snapshot,
            self.ai_workspace_states
                .values()
                .map(|state| &state.state_snapshot),
        );
        if let Some(workspace_key) = self.ai_workspace_key() {
            retained_thread_ids.insert(workspace_key);
        }
        retained_thread_ids.extend(self.ai_workspace_states.iter().filter_map(|(workspace_key, state)| {
            let retain = state.new_thread_draft_active
                || state.pending_new_thread_selection
                || state.terminal_open
                || state.terminal_session.screen.is_some()
                || !state.terminal_session.transcript.is_empty();
            retain.then(|| workspace_key.clone())
        }));

        let visible_runtime_removed = self.ai_terminal_runtime.as_ref().is_some_and(|runtime| {
            !retained_thread_ids.contains(runtime.thread_id.as_str())
        });
        if visible_runtime_removed {
            self.stop_ai_terminal_runtime(reason);
            self.ai_terminal_open = false;
            self.ai_terminal_surface_focused = false;
            self.ai_terminal_cursor_blink_visible = true;
            self.ai_terminal_follow_output = true;
            self.ai_terminal_pending_input = None;
            self.ai_terminal_input_draft.clear();
            self.ai_terminal_session = AiTerminalSessionState::default();
            self.ai_terminal_grid_size = None;
            self.ai_clear_terminal_cursor_output_suppression(cx);
            self.defer_ai_composer_focus(cx);
        }

        self.ai_sync_terminal_cursor_blink(cx);

        self.ai_terminal_states_by_thread
            .retain(|thread_id, _| retained_thread_ids.contains(thread_id));

        let mut retained_hidden_runtimes = BTreeMap::new();
        for (thread_id, hidden) in std::mem::take(&mut self.ai_hidden_terminal_runtimes) {
            if retained_thread_ids.contains(thread_id.as_str()) {
                retained_hidden_runtimes.insert(thread_id, hidden);
                continue;
            }

            if let Err(error) = hidden.runtime.handle.kill() {
                error!(
                    "failed to stop hidden AI terminal runtime for pruned thread {thread_id} during {reason}: {error:#}"
                );
            }
        }
        self.ai_hidden_terminal_runtimes = retained_hidden_runtimes;
    }

    fn ai_terminal_runtime_is_current(&self, thread_id: &str, generation: usize) -> bool {
        if self.ai_terminal_runtime.as_ref().is_some_and(|runtime| {
            runtime.thread_id == thread_id && runtime.generation == generation
        }) {
            return true;
        }

        self.ai_hidden_terminal_runtimes
            .get(thread_id)
            .is_some_and(|hidden| hidden.runtime.generation == generation)
    }

    fn next_ai_terminal_runtime_generation(&mut self) -> usize {
        self.ai_terminal_runtime_generation = self.ai_terminal_runtime_generation.saturating_add(1);
        self.ai_terminal_runtime_generation
    }

    fn ai_terminal_set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.ai_terminal_open == open {
            return;
        }
        self.ai_terminal_open = open;
        if !open {
            self.ai_terminal_surface_focused = false;
            self.ai_clear_terminal_cursor_output_suppression(cx);
            self.defer_ai_composer_focus(cx);
        }
        self.ai_sync_terminal_cursor_blink(cx);
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
        match self.workspace_view_mode {
            WorkspaceViewMode::Ai => self.toggle_ai_terminal_drawer(cx),
            WorkspaceViewMode::Files => self.toggle_files_terminal_drawer(Some(window), cx),
            _ => {}
        }
    }

    pub(super) fn ai_toggle_terminal_drawer_action(&mut self, cx: &mut Context<Self>) {
        self.toggle_ai_terminal_drawer(cx);
    }

    pub(super) fn ai_clear_terminal_session_action(&mut self, cx: &mut Context<Self>) {
        if !self.ai_terminal_is_running() {
            self.stop_ai_terminal_runtime("clearing terminal session");
        }
        self.ai_terminal_pending_input = None;
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
        } else {
            self.ai_clear_terminal_cursor_output_suppression(cx);
        }
        self.ai_sync_terminal_cursor_blink(cx);
        cx.notify();
    }

    fn ensure_ai_terminal_session(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.ai_current_terminal_owner_key() else {
            debug!(
                terminal_open = self.ai_terminal_open,
                pending_new_thread_selection = self.ai_pending_new_thread_selection,
                new_thread_draft_active = self.ai_new_thread_draft_active,
                "Skipping AI terminal start because no workspace is currently selected"
            );
            return;
        };
        if let Some(active_runtime_thread_id) = self
            .ai_terminal_runtime
            .as_ref()
            .map(|runtime| runtime.thread_id.clone())
        {
            if active_runtime_thread_id == thread_id {
                debug!(
                    thread_id,
                    "Skipping AI terminal start because the selected thread already owns a runtime"
                );
                return;
            }

            debug!(
                thread_id,
                active_runtime_thread_id,
                "Parking stale visible AI terminal runtime before starting a session for the selected thread"
            );
            self.ai_park_visible_terminal_runtime_for_thread(Some(active_runtime_thread_id.as_str()));
        }
        if self.ai_terminal_session.screen.is_some() {
            debug!(
                thread_id,
                status = ?self.ai_terminal_session.status,
                "Skipping AI terminal start because a terminal screen is already present"
            );
            return;
        }

        let Some(cwd) = self.ai_workspace_cwd() else {
            debug!(thread_id, "Skipping AI terminal start because no workspace cwd is available");
            return;
        };

        self.start_default_ai_terminal_session(cwd, thread_id, cx);
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

    pub(crate) fn stop_all_ai_terminal_runtimes(&mut self, reason: &str) {
        self.stop_ai_terminal_runtime(reason);
        for (thread_id, hidden) in std::mem::take(&mut self.ai_hidden_terminal_runtimes) {
            if let Err(error) = hidden.runtime.handle.kill() {
                error!(
                    "failed to stop hidden AI terminal runtime for thread {thread_id} during {reason}: {error:#}"
                );
            }
        }
    }

    pub(super) fn ai_rerun_terminal_command_action(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self.ai_terminal_session.last_command.clone() else {
            self.ai_terminal_session.status_message = Some("No command to rerun.".to_string());
            cx.notify();
            return;
        };
        self.ai_run_command_in_terminal(None, command, cx);
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
        self.ai_sync_terminal_cursor_blink(cx);
        self.ai_report_terminal_focus_change(true, cx);
    }

    pub(super) fn ai_terminal_surface_focus_out(&mut self, cx: &mut Context<Self>) {
        if self.ai_terminal_surface_focused {
            self.ai_terminal_surface_focused = false;
            cx.notify();
        }
        self.ai_sync_terminal_cursor_blink(cx);
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
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if event.button == MouseButton::Middle && !mode.unwrap_or_default().mouse_mode {
            return self.ai_paste_terminal_from_primary_selection(cx);
        }

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

        if ai_terminal_uses_insert_paste_shortcut(keystroke) {
            return self.ai_paste_terminal_from_clipboard(cx);
        }

        let Some(bytes) = ai_terminal_input_bytes_for_keystroke(keystroke, terminal_mode) else {
            return false;
        };
        self.ai_write_terminal_bytes(bytes.as_slice(), cx)
    }

    fn ai_terminal_dispatch_synthesized_keystroke(
        &mut self,
        keystroke: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(keystroke) = gpui::Keystroke::parse(keystroke) else {
            error!("failed to parse synthesized AI terminal keystroke: {keystroke}");
            return;
        };

        if self.ai_terminal_surface_key_down(&keystroke, window, cx) {
            cx.stop_propagation();
        }
    }

    pub(super) fn ai_terminal_send_ctrl_c_action(
        &mut self,
        _: &AiTerminalSendCtrlC,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("ctrl-c", window, cx);
    }

    pub(super) fn ai_terminal_send_ctrl_a_action(
        &mut self,
        _: &AiTerminalSendCtrlA,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("ctrl-a", window, cx);
    }

    pub(super) fn ai_terminal_send_tab_action(
        &mut self,
        _: &AiTerminalSendTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("tab", window, cx);
    }

    pub(super) fn ai_terminal_send_back_tab_action(
        &mut self,
        _: &AiTerminalSendBackTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("shift-tab", window, cx);
    }

    pub(super) fn ai_terminal_send_up_action(
        &mut self,
        _: &AiTerminalSendUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("up", window, cx);
    }

    pub(super) fn ai_terminal_send_down_action(
        &mut self,
        _: &AiTerminalSendDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("down", window, cx);
    }

    pub(super) fn ai_terminal_send_left_action(
        &mut self,
        _: &AiTerminalSendLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("left", window, cx);
    }

    pub(super) fn ai_terminal_send_right_action(
        &mut self,
        _: &AiTerminalSendRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("right", window, cx);
    }

    pub(super) fn ai_terminal_send_home_action(
        &mut self,
        _: &AiTerminalSendHome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("home", window, cx);
    }

    pub(super) fn ai_terminal_send_end_action(
        &mut self,
        _: &AiTerminalSendEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_terminal_dispatch_synthesized_keystroke("end", window, cx);
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
        if bytes.contains(&b'\r') || bytes.contains(&b'\n') {
            self.ai_temporarily_suppress_terminal_cursor(cx);
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
        let _ = self.ai_write_terminal_bytes(bytes.as_slice(), cx);
    }

    fn ai_paste_terminal_from_clipboard(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        if text.is_empty() {
            return false;
        }

        self.ai_paste_terminal_text(text.as_str(), cx)
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn ai_paste_terminal_from_primary_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = cx.read_from_primary().and_then(|item| item.text()) else {
            return false;
        };
        if text.is_empty() {
            return false;
        }

        self.ai_paste_terminal_text(text.as_str(), cx)
    }

    fn ai_paste_terminal_text(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let bracketed_paste = self
            .ai_terminal_session
            .screen
            .as_ref()
            .is_some_and(|screen| screen.mode.bracketed_paste);
        let bytes = ai_terminal_paste_bytes(text, bracketed_paste);
        self.ai_write_terminal_bytes(bytes.as_slice(), cx)
    }

    pub(super) fn ai_run_command_in_terminal(
        &mut self,
        cwd: Option<PathBuf>,
        command: String,
        cx: &mut Context<Self>,
    ) {
        let command = ai_terminal_command_for_shell(
            command.as_str(),
            ai_terminal_default_shell_family(&self.config),
        );
        if command.is_empty() {
            return;
        }

        let target_cwd = cwd.or_else(|| self.ai_workspace_cwd());
        let Some(target_cwd) = target_cwd else {
            self.ai_terminal_session.status_message =
                Some("Open a workspace before using the terminal.".to_string());
            self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
            self.ai_terminal_set_open(true, cx);
            cx.notify();
            return;
        };

        self.ai_terminal_set_open(true, cx);
        self.ai_terminal_session.last_command = Some(command.clone());
        self.ai_terminal_pending_input = Some(command);

        let session_cwd_matches = self
            .ai_terminal_session
            .cwd
            .as_ref()
            .is_some_and(|cwd| cwd == &target_cwd);

        if self.ai_terminal_is_running() && session_cwd_matches {
            self.flush_ai_terminal_pending_input(cx);
            self.defer_ai_terminal_interaction_focus(cx);
            return;
        }

        self.start_default_ai_terminal_session(
            target_cwd.clone(),
            target_cwd.to_string_lossy().to_string(),
            cx,
        );
    }

    fn start_default_ai_terminal_session(
        &mut self,
        cwd: PathBuf,
        thread_id: String,
        cx: &mut Context<Self>,
    ) {
        self.stop_ai_terminal_runtime("starting default terminal shell");
        debug!(
            thread_id = thread_id.as_str(),
            cwd = %cwd.display(),
            pending_input = self.ai_terminal_pending_input.is_some(),
            "Starting AI terminal session"
        );
        let resolved_shell = crate::terminal_env::resolve_terminal_shell(&self.config.terminal);
        let request = TerminalSpawnRequest::shell(cwd.clone())
            .with_shell_program(resolved_shell.program().to_os_string())
            .with_shell_args(
                resolved_shell.interactive_shell_args(self.config.terminal.inherit_login_environment),
            );
        match spawn_terminal_session(request) {
            Ok((handle, event_rx)) => {
                self.ai_terminal_open = true;
                self.ai_terminal_stop_requested = false;
                self.ai_terminal_session.cwd = Some(cwd);
                if self.ai_terminal_pending_input.is_none() {
                    self.ai_terminal_session.last_command = None;
                }
                self.ai_terminal_session.status = AiTerminalSessionStatus::Running;
                self.ai_terminal_session.exit_code = None;
                self.ai_terminal_session.screen = None;
                self.ai_terminal_grid_size = None;
                self.ai_terminal_follow_output = true;
                self.ai_terminal_session.status_message = None;
                self.ai_clear_terminal_cursor_output_suppression(cx);
                self.ai_sync_terminal_cursor_blink(cx);
                let generation = self.next_ai_terminal_runtime_generation();
                self.ai_terminal_runtime = Some(AiTerminalRuntimeHandle {
                    thread_id: thread_id.clone(),
                    handle,
                    generation,
                });
                self.start_ai_terminal_event_listener(event_rx, thread_id, generation, cx);
                self.defer_ai_terminal_interaction_focus(cx);
            }
            Err(error) => {
                self.ai_terminal_open = true;
                self.ai_terminal_session.cwd = Some(cwd);
                self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                self.ai_terminal_session.exit_code = None;
                self.ai_terminal_session.screen = None;
                self.ai_terminal_grid_size = None;
                self.ai_terminal_session.status_message =
                    Some("Failed to start terminal shell.".to_string());
                self.ai_clear_terminal_cursor_output_suppression(cx);
                self.ai_sync_terminal_cursor_blink(cx);
                append_ai_terminal_transcript(
                    &mut self.ai_terminal_session.transcript,
                    format!("[terminal error] {error}\n"),
                );
            }
        }
    }
    fn start_ai_terminal_event_listener(
        &mut self,
        event_rx: std::sync::mpsc::Receiver<TerminalEvent>,
        thread_id: String,
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
                    if !this.ai_terminal_runtime_is_current(thread_id.as_str(), generation) {
                        listener_is_current = false;
                        return;
                    }
                    for event in buffered_events {
                        this.apply_ai_terminal_event_for_thread(thread_id.as_str(), event, cx);
                    }
                    if event_stream_disconnected
                        && this.ai_terminal_runtime_is_current(thread_id.as_str(), generation)
                    {
                        if this
                            .ai_terminal_runtime
                            .as_ref()
                            .is_some_and(|runtime| runtime.thread_id == thread_id)
                        {
                            this.ai_terminal_runtime = None;
                        } else {
                            this.ai_hidden_terminal_runtimes.remove(thread_id.as_str());
                        }
                    }
                    cx.notify();
                });
                if !listener_is_current || event_stream_disconnected {
                    return;
                }
            }
        });
    }

    fn apply_ai_terminal_event_for_thread(
        &mut self,
        thread_id: &str,
        event: TerminalEvent,
        cx: &mut Context<Self>,
    ) {
        if self.ai_current_terminal_owner_key().as_deref() == Some(thread_id) {
            self.apply_ai_terminal_event(event, cx);
            return;
        }

        self.apply_hidden_ai_terminal_event_for_thread(thread_id, event);
    }

    fn apply_ai_terminal_event(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        match event {
            TerminalEvent::Output(output) => {
                let sanitized = sanitize_ai_terminal_output(output.as_slice());
                if sanitized.is_empty() {
                    return;
                }
                self.ai_temporarily_suppress_terminal_cursor(cx);
                append_ai_terminal_transcript(&mut self.ai_terminal_session.transcript, sanitized);
            }
            TerminalEvent::Screen(screen) => {
                if self.ai_terminal_is_running() {
                    self.clear_ai_terminal_text_selection(cx);
                }
                self.ai_terminal_follow_output = screen.display_offset == 0;
                self.ai_terminal_session.screen = Some(screen);
                self.ai_sync_terminal_cursor_blink(cx);
                self.flush_ai_terminal_pending_input(cx);
            }
            TerminalEvent::Exit { exit_code } => {
                let stopped = self.ai_terminal_stop_requested;
                self.ai_terminal_stop_requested = false;
                self.ai_terminal_runtime = None;
                self.ai_terminal_session.exit_code = exit_code;
                if stopped {
                    self.ai_terminal_session.status = AiTerminalSessionStatus::Stopped;
                } else if exit_code == Some(0) {
                    self.ai_terminal_session.status = AiTerminalSessionStatus::Completed;
                } else {
                    self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                }
                self.ai_close_terminal_after_exit(cx);
            }
            TerminalEvent::Failed(message) => {
                self.ai_terminal_stop_requested = false;
                self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
                self.ai_terminal_session.status_message = Some(message.clone());
                self.ai_clear_terminal_cursor_output_suppression(cx);
                self.ai_sync_terminal_cursor_blink(cx);
                append_ai_terminal_transcript(
                    &mut self.ai_terminal_session.transcript,
                    format!("[terminal error] {message}\n"),
                );
            }
        }
    }

    fn flush_ai_terminal_pending_input(&mut self, cx: &mut Context<Self>) {
        if !self.ai_terminal_is_running() {
            return;
        }
        let Some(runtime) = self.ai_terminal_runtime.as_ref() else {
            return;
        };
        let Some(mut input) = self.ai_terminal_pending_input.take() else {
            return;
        };

        if !input.ends_with('\n') {
            input.push('\n');
        }

        if let Err(error) = runtime.handle.write_input(input.as_bytes()) {
            self.ai_terminal_pending_input = Some(input.trim_end_matches('\n').to_string());
            self.ai_terminal_session.status_message = Some(error.to_string());
            self.ai_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return;
        }

        self.ai_terminal_session.status_message = None;
        cx.notify();
    }

    fn apply_hidden_ai_terminal_event_for_thread(&mut self, thread_id: &str, event: TerminalEvent) {
        match event {
            TerminalEvent::Output(output) => {
                let sanitized = sanitize_ai_terminal_output(output.as_slice());
                if sanitized.is_empty() {
                    return;
                }
                let state = self
                    .ai_terminal_states_by_thread
                    .entry(thread_id.to_string())
                    .or_default();
                append_ai_terminal_transcript(&mut state.session.transcript, sanitized);
            }
            TerminalEvent::Screen(screen) => {
                let state = self
                    .ai_terminal_states_by_thread
                    .entry(thread_id.to_string())
                    .or_default();
                state.follow_output = screen.display_offset == 0;
                state.session.screen = Some(screen);
                state.session.status = AiTerminalSessionStatus::Running;
                self.flush_hidden_ai_terminal_pending_input(thread_id);
            }
            TerminalEvent::Exit { .. } => {
                self.ai_hidden_terminal_runtimes.remove(thread_id);
                self.ai_terminal_states_by_thread.remove(thread_id);
            }
            TerminalEvent::Failed(message) => {
                let state = self
                    .ai_terminal_states_by_thread
                    .entry(thread_id.to_string())
                    .or_default();
                state.session.status = AiTerminalSessionStatus::Failed;
                state.session.status_message = Some(message.clone());
                append_ai_terminal_transcript(
                    &mut state.session.transcript,
                    format!("[terminal error] {message}\n"),
                );
            }
        }
    }

    fn flush_hidden_ai_terminal_pending_input(&mut self, thread_id: &str) {
        let input = self
            .ai_terminal_states_by_thread
            .get_mut(thread_id)
            .and_then(|state| state.pending_input.take());
        let Some(mut input) = input else {
            return;
        };

        if !input.ends_with('\n') {
            input.push('\n');
        }

        let Some(hidden) = self.ai_hidden_terminal_runtimes.get(thread_id) else {
            self.ai_terminal_states_by_thread
                .entry(thread_id.to_string())
                .or_default()
                .pending_input = Some(input.trim_end_matches('\n').to_string());
            return;
        };

        if let Err(error) = hidden.runtime.handle.write_input(input.as_bytes()) {
            let state = self
                .ai_terminal_states_by_thread
                .entry(thread_id.to_string())
                .or_default();
            state.pending_input = Some(input.trim_end_matches('\n').to_string());
            state.session.status = AiTerminalSessionStatus::Failed;
            state.session.status_message = Some(error.to_string());
        }
    }

    fn ai_close_terminal_after_exit(&mut self, cx: &mut Context<Self>) {
        self.ai_terminal_open = false;
        self.ai_terminal_surface_focused = false;
        self.ai_terminal_cursor_blink_visible = true;
        self.ai_terminal_follow_output = true;
        self.ai_terminal_pending_input = None;
        self.ai_terminal_input_draft.clear();
        self.ai_terminal_session = AiTerminalSessionState::default();
        self.ai_terminal_grid_size = None;
        self.ai_clear_terminal_cursor_output_suppression(cx);
        self.ai_sync_terminal_cursor_blink(cx);
        self.defer_ai_composer_focus(cx);
        cx.notify();
    }
}

fn ai_terminal_strip_matching_outer_quotes(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0];
        let last = bytes[trimmed.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

fn ai_terminal_default_shell_family(config: &AppConfig) -> AiTerminalShellFamily {
    let shell = crate::terminal_env::resolve_terminal_shell(&config.terminal);
    ai_terminal_shell_family_from_program(
        shell.label(),
    )
}

fn ai_terminal_shell_family_from_program(program: &str) -> AiTerminalShellFamily {
    match program.to_ascii_lowercase().as_str() {
        "cmd" | "cmd.exe" => AiTerminalShellFamily::Cmd,
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => {
            AiTerminalShellFamily::PowerShell
        }
        _ => AiTerminalShellFamily::Posix,
    }
}

fn ai_terminal_command_for_shell(command: &str, shell_family: AiTerminalShellFamily) -> String {
    let trimmed = command.trim();
    let Some((inner, wrapper_family)) = ai_terminal_wrapped_command(trimmed) else {
        return trimmed.to_string();
    };
    if wrapper_family != shell_family {
        return trimmed.to_string();
    }
    ai_terminal_strip_matching_outer_quotes(inner).trim().to_string()
}

fn ai_terminal_wrapped_command(command: &str) -> Option<(&str, AiTerminalShellFamily)> {
    AI_TERMINAL_SHELL_WRAPPERS
        .iter()
        .find_map(|(prefix, family)| command.strip_prefix(prefix).map(|inner| (inner, *family)))
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

fn ai_retainable_terminal_thread_ids<'a>(
    visible_state: &hunk_codex::state::AiState,
    background_states: impl IntoIterator<Item = &'a hunk_codex::state::AiState>,
) -> std::collections::BTreeSet<String> {
    let mut retained_thread_ids = std::collections::BTreeSet::new();
    ai_extend_retainable_terminal_thread_ids(&mut retained_thread_ids, visible_state);
    for state in background_states {
        ai_extend_retainable_terminal_thread_ids(&mut retained_thread_ids, state);
    }
    retained_thread_ids
}

fn ai_extend_retainable_terminal_thread_ids(
    retained_thread_ids: &mut std::collections::BTreeSet<String>,
    state: &hunk_codex::state::AiState,
) {
    retained_thread_ids.extend(
        state
            .threads
            .values()
            .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
            .map(|thread| thread.cwd.clone()),
    );
}

#[cfg(test)]
mod terminal_output_tests {
    use super::{
        ai_retainable_terminal_thread_ids, ai_terminal_command_for_shell,
        sanitize_ai_terminal_output, strip_ansi_sequences, AiTerminalShellFamily,
    };
    use crate::app::AiWorkspaceState;
    use hunk_codex::state::{AiState, ThreadLifecycleStatus, ThreadSummary};
    use std::collections::BTreeMap;

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

    #[test]
    fn retainable_terminal_threads_include_visible_and_background_non_archived_threads() {
        let mut visible_state = AiState::default();
        visible_state.threads.insert(
            "thread-visible".to_string(),
            ThreadSummary {
                id: "thread-visible".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
        );
        visible_state.threads.insert(
            "thread-archived".to_string(),
            ThreadSummary {
                id: "thread-archived".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Archived,
                created_at: 2,
                updated_at: 2,
                last_sequence: 2,
            },
        );

        let mut background_state = AiState::default();
        background_state.threads.insert(
            "thread-background".to_string(),
            ThreadSummary {
                id: "thread-background".to_string(),
                cwd: "/repo/worktree".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                created_at: 3,
                updated_at: 3,
                last_sequence: 3,
            },
        );

        let workspace_states = BTreeMap::from([(
            "/repo/worktree".to_string(),
            AiWorkspaceState {
                state_snapshot: background_state,
                ..AiWorkspaceState::default()
            },
        )]);

        let retained = ai_retainable_terminal_thread_ids(
            &visible_state,
            workspace_states.values().map(|state| &state.state_snapshot),
        );

        assert!(retained.contains("/repo"));
        assert!(retained.contains("/repo/worktree"));
        assert!(!retained.contains("thread-archived"));
    }

    #[test]
    fn terminal_command_for_shell_unwraps_matching_posix_wrapper() {
        let command = ai_terminal_command_for_shell(
            "/bin/zsh -lc 'kill 75768 && sleep 1'",
            AiTerminalShellFamily::Posix,
        );
        assert_eq!(command, "kill 75768 && sleep 1");
    }

    #[test]
    fn terminal_command_for_shell_preserves_mismatched_windows_wrapper() {
        let command = ai_terminal_command_for_shell(
            "powershell -Command Get-ChildItem",
            AiTerminalShellFamily::Cmd,
        );
        assert_eq!(command, "powershell -Command Get-ChildItem");
    }
}
