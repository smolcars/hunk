impl DiffViewer {
    fn current_files_terminal_owner_key(&self) -> Option<String> {
        self.current_workspace_project_key()
    }

    fn files_capture_visible_terminal_state(&self) -> FilesProjectTerminalState {
        FilesProjectTerminalState {
            open: self.files_terminal_open,
            active_tab_id: self.files_terminal_active_tab_id,
            next_tab_id: self.files_terminal_next_tab_id,
            tabs: self.files_visible_terminal_tabs_snapshot(),
            restore_target: self.files_terminal_restore_target,
        }
    }

    fn files_apply_visible_terminal_state(&mut self, state: FilesProjectTerminalState) {
        self.files_terminal_open = state.open;
        self.files_terminal_active_tab_id = state.active_tab_id;
        self.files_terminal_next_tab_id = state.next_tab_id;
        self.files_terminal_tabs = state.tabs;
        self.files_apply_visible_terminal_tab();
        self.files_terminal_restore_target = state.restore_target;
        self.files_terminal_surface_focused = false;
        self.files_terminal_cursor_blink_generation =
            self.files_terminal_cursor_blink_generation.saturating_add(1);
        self.files_terminal_cursor_blink_visible = true;
        self.files_terminal_cursor_blink_active = false;
        self.files_terminal_cursor_blink_task = Task::ready(());
        self.files_terminal_cursor_output_generation =
            self.files_terminal_cursor_output_generation.saturating_add(1);
        self.files_terminal_cursor_output_suppressed = false;
        self.files_terminal_cursor_output_task = Task::ready(());
    }

    pub(super) fn files_visible_terminal_tabs_snapshot(&self) -> Vec<TerminalTabState> {
        let mut tabs = self.files_terminal_tabs.clone();
        if let Some(tab) = tabs
            .iter_mut()
            .find(|tab| tab.id == self.files_terminal_active_tab_id)
        {
            tab.follow_output = self.files_terminal_follow_output;
            tab.session = self.files_terminal_session.clone();
            tab.pending_input = self.files_terminal_pending_input.clone();
        } else {
            tabs.push(TerminalTabState {
                id: self.files_terminal_active_tab_id,
                title: format!("Shell {}", self.files_terminal_active_tab_id),
                follow_output: self.files_terminal_follow_output,
                session: self.files_terminal_session.clone(),
                pending_input: self.files_terminal_pending_input.clone(),
            });
            tabs.sort_by_key(|tab| tab.id);
        }
        tabs
    }

    fn files_save_visible_terminal_tab(&mut self) {
        let active_tab_id = self.files_terminal_active_tab_id;
        if let Some(tab) = self
            .files_terminal_tabs
            .iter_mut()
            .find(|tab| tab.id == active_tab_id)
        {
            tab.follow_output = self.files_terminal_follow_output;
            tab.session = self.files_terminal_session.clone();
            tab.pending_input = self.files_terminal_pending_input.clone();
            return;
        }

        self.files_terminal_tabs.push(TerminalTabState {
            id: active_tab_id,
            title: format!("Shell {active_tab_id}"),
            follow_output: self.files_terminal_follow_output,
            session: self.files_terminal_session.clone(),
            pending_input: self.files_terminal_pending_input.clone(),
        });
        self.files_terminal_tabs.sort_by_key(|tab| tab.id);
    }

    fn files_apply_visible_terminal_tab(&mut self) {
        if self.files_terminal_tabs.is_empty() {
            self.files_terminal_tabs = default_terminal_tabs();
            self.files_terminal_active_tab_id = 1;
            self.files_terminal_next_tab_id = 2;
        }
        if !self
            .files_terminal_tabs
            .iter()
            .any(|tab| tab.id == self.files_terminal_active_tab_id)
            && let Some(first_tab) = self.files_terminal_tabs.first()
        {
            self.files_terminal_active_tab_id = first_tab.id;
        }

        let tab = self
            .files_terminal_tabs
            .iter()
            .find(|tab| tab.id == self.files_terminal_active_tab_id)
            .cloned()
            .unwrap_or_default();
        self.files_terminal_follow_output = tab.follow_output;
        self.files_terminal_session = tab.session;
        self.files_terminal_pending_input = tab.pending_input;
        self.files_terminal_grid_size = self
            .files_terminal_session
            .screen
            .as_ref()
            .map(|screen| (screen.rows, screen.cols));
    }

    fn files_store_visible_terminal_state_for_project(&mut self, project_key: Option<&str>) {
        let state = self.files_capture_visible_terminal_state();
        store_visible_terminal_state(
            &mut self.files_terminal_states_by_project,
            project_key,
            state,
        );
    }

    fn files_restore_visible_terminal_state_for_project(&mut self, project_key: Option<&str>) {
        let state =
            restore_visible_terminal_state(&self.files_terminal_states_by_project, project_key);
        self.files_apply_visible_terminal_state(state);
    }

    fn files_park_visible_terminal_runtime_for_project(&mut self, project_key: Option<&str>) {
        let Some(project_key) = project_key else {
            return;
        };
        let tab_id = self
            .files_terminal_runtime
            .as_ref()
            .map(|runtime| runtime.tab_id)
            .unwrap_or(self.files_terminal_active_tab_id);
        let runtime_key = terminal_runtime_tab_key(project_key, tab_id);
        park_visible_terminal_runtime(
            Some(runtime_key.as_str()),
            &mut self.files_terminal_runtime,
            &mut self.files_terminal_event_task,
            &mut self.files_hidden_terminal_runtimes,
        );
    }

    fn files_promote_hidden_terminal_runtime_for_project(
        &mut self,
        project_key: &str,
        tab_id: TerminalTabId,
    ) -> bool {
        let runtime_key = terminal_runtime_tab_key(project_key, tab_id);
        promote_hidden_terminal_runtime(
            runtime_key.as_str(),
            &mut self.files_terminal_runtime,
            &mut self.files_terminal_event_task,
            &mut self.files_hidden_terminal_runtimes,
        )
    }

    pub(super) fn files_handle_project_change(
        &mut self,
        previous_project_key: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let previous_terminal_owner_key = self
            .files_terminal_runtime
            .as_ref()
            .map(|runtime| runtime.project_key.clone())
            .or(previous_project_key);
        let next_terminal_owner_key = self.current_files_terminal_owner_key();
        if previous_terminal_owner_key == next_terminal_owner_key {
            return;
        }

        self.files_store_visible_terminal_state_for_project(previous_terminal_owner_key.as_deref());
        self.files_park_visible_terminal_runtime_for_project(previous_terminal_owner_key.as_deref());
        self.files_restore_visible_terminal_state_for_project(next_terminal_owner_key.as_deref());

        if let Some(project_key) = next_terminal_owner_key.as_deref() {
            if !self.files_promote_hidden_terminal_runtime_for_project(
                project_key,
                self.files_terminal_active_tab_id,
            )
                && self.files_terminal_open
            {
                self.ensure_files_terminal_session(cx);
            }
        } else {
            self.files_terminal_open = false;
            self.files_terminal_surface_focused = false;
        }

        self.files_sync_terminal_cursor_blink(cx);
        cx.notify();
    }

    pub(crate) fn files_terminal_is_running(&self) -> bool {
        self.files_terminal_session.status == AiTerminalSessionStatus::Running
    }

    pub(crate) fn files_terminal_selection_active(&self) -> bool {
        self.ai_text_selection
            .as_ref()
            .is_some_and(|selection| selection.row_id == crate::app::FILES_TERMINAL_TEXT_SELECTION_ROW_ID)
    }

    fn clear_files_terminal_text_selection(&mut self, cx: &mut Context<Self>) {
        if self.files_terminal_selection_active() {
            self.ai_clear_text_selection(cx);
        }
    }

    fn focus_files_terminal_surface(&mut self, cx: &mut Context<Self>) {
        let focus_handle = self.files_terminal_focus_handle.clone();
        if let Err(error) = Self::update_any_window(cx, move |window, cx| {
            focus_handle.focus(window, cx);
        }) {
            error!("failed to focus Files terminal surface: {error:#}");
        }
    }

    fn defer_files_terminal_interaction_focus(&self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let terminal_focus_handle = self.files_terminal_focus_handle.clone();
        cx.defer(move |cx| {
            let result = cx.update_window(window_handle, |_, window, cx| {
                terminal_focus_handle.focus(window, cx);
            });
            if let Err(err) = result
                && !Self::is_window_not_found_error(&err)
            {
                error!("failed to defer Files terminal focus: {err:#}");
            }
        });
    }

    fn defer_files_editor_focus(&self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let focus_handle = self.files_editor_focus_handle.clone();
        cx.defer(move |cx| {
            let result = cx.update_window(window_handle, |_, window, cx| {
                focus_handle.focus(window, cx);
            });
            if let Err(err) = result
                && !Self::is_window_not_found_error(&err)
            {
                error!("failed to defer Files editor focus: {err:#}");
            }
        });
    }

    fn files_terminal_restore_target_for_window(&self, window: &Window) -> FilesTerminalRestoreTarget {
        if self.editor_path.is_some()
            && !self.editor_markdown_preview
            && self.files_editor_focus_handle.is_focused(window)
        {
            FilesTerminalRestoreTarget::Editor
        } else {
            FilesTerminalRestoreTarget::WorkspaceRoot
        }
    }

    fn capture_files_terminal_restore_target(
        &mut self,
        window: Option<&Window>,
        cx: &mut Context<Self>,
    ) {
        if let Some(window) = window {
            self.files_terminal_restore_target =
                self.files_terminal_restore_target_for_window(window);
            return;
        }

        let mut restore_target = FilesTerminalRestoreTarget::WorkspaceRoot;
        let editor_focus_handle = self.files_editor_focus_handle.clone();
        let editor_open = self.editor_path.is_some() && !self.editor_markdown_preview;
        if editor_open
            && let Err(err) = Self::update_any_window(cx, |window, _| {
                if editor_focus_handle.is_focused(window) {
                    restore_target = FilesTerminalRestoreTarget::Editor;
                }
            })
        {
            error!("failed to capture Files terminal restore target: {err:#}");
        }
        self.files_terminal_restore_target = restore_target;
    }

    fn defer_files_focus_restore_after_terminal_close(&self, cx: &mut Context<Self>) {
        if self.workspace_view_mode != WorkspaceViewMode::Files {
            self.defer_root_focus(cx);
            return;
        }

        if self.files_terminal_restore_target == FilesTerminalRestoreTarget::Editor
            && self.editor_path.is_some()
            && !self.editor_markdown_preview
        {
            self.defer_files_editor_focus(cx);
            return;
        }

        self.defer_root_focus(cx);
    }

    fn files_terminal_runtime_is_current(
        &self,
        project_key: &str,
        tab_id: TerminalTabId,
        generation: usize,
    ) -> bool {
        if self.files_terminal_runtime.as_ref().is_some_and(|runtime| {
            runtime.project_key == project_key
                && runtime.tab_id == tab_id
                && runtime.generation == generation
        }) {
            return true;
        }

        self.files_hidden_terminal_runtimes
            .values()
            .any(|hidden| {
                hidden.runtime.project_key == project_key
                    && hidden.runtime.tab_id == tab_id
                    && hidden.runtime.generation == generation
            })
    }

    fn next_files_terminal_runtime_generation(&mut self) -> usize {
        self.files_terminal_runtime_generation =
            self.files_terminal_runtime_generation.saturating_add(1);
        self.files_terminal_runtime_generation
    }

    fn files_terminal_set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.files_terminal_open == open {
            return;
        }
        self.files_terminal_open = open;
        if !open {
            self.files_terminal_surface_focused = false;
            self.files_clear_terminal_cursor_output_suppression(cx);
            self.defer_files_focus_restore_after_terminal_close(cx);
        }
        self.files_sync_terminal_cursor_blink(cx);
        cx.notify();
    }

    fn toggle_files_terminal_drawer(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let next_open = !self.files_terminal_open;
        if next_open {
            self.capture_files_terminal_restore_target(window.as_deref(), cx);
        }
        self.files_terminal_set_open(next_open, cx);
        if next_open {
            self.ensure_files_terminal_session(cx);
            self.defer_files_terminal_interaction_focus(cx);
        }
    }

    fn ensure_files_terminal_session(&mut self, cx: &mut Context<Self>) {
        let Some(project_key) = self.current_files_terminal_owner_key() else {
            return;
        };
        if let Some(active_runtime_project_key) = self
            .files_terminal_runtime
            .as_ref()
            .map(|runtime| runtime.project_key.clone())
        {
            let active_runtime_tab_id = self
                .files_terminal_runtime
                .as_ref()
                .map(|runtime| runtime.tab_id)
                .unwrap_or(self.files_terminal_active_tab_id);
            if active_runtime_project_key == project_key
                && active_runtime_tab_id == self.files_terminal_active_tab_id
            {
                return;
            }

            self.files_park_visible_terminal_runtime_for_project(Some(
                active_runtime_project_key.as_str(),
            ));
        }
        if self.files_promote_hidden_terminal_runtime_for_project(
            project_key.as_str(),
            self.files_terminal_active_tab_id,
        ) {
            return;
        }
        if self.files_terminal_session.screen.is_some() {
            return;
        }

        let Some(cwd) = self.primary_repo_root() else {
            self.files_terminal_session.status_message =
                Some("Open a repository before using the terminal.".to_string());
            self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return;
        };

        self.start_default_files_terminal_session(cwd, project_key, cx);
    }

    pub(crate) fn stop_files_terminal_runtime(&mut self, reason: &str) {
        self.files_terminal_stop_requested = false;
        self.files_terminal_event_task = Task::ready(());
        if let Some(runtime) = self.files_terminal_runtime.take()
            && self.files_terminal_is_running()
            && let Err(error) = runtime.handle.kill()
        {
            error!("failed to stop Files terminal runtime during {reason}: {error:#}");
        }
    }

    pub(crate) fn stop_all_files_terminal_runtimes(&mut self, reason: &str) {
        self.stop_files_terminal_runtime(reason);
        for (project_key, hidden) in std::mem::take(&mut self.files_hidden_terminal_runtimes) {
            if let Err(error) = hidden.runtime.handle.kill() {
                error!(
                    "failed to stop hidden Files terminal runtime for project {project_key} during {reason}: {error:#}"
                );
            }
        }
    }

    pub(crate) fn discard_files_terminal_state_for_project(
        &mut self,
        project_root: &std::path::Path,
        reason: &str,
    ) {
        let project_key = project_root.to_string_lossy().to_string();
        self.files_terminal_states_by_project.remove(project_key.as_str());
        let mut retained_hidden_runtimes = BTreeMap::new();
        for (runtime_key, hidden) in std::mem::take(&mut self.files_hidden_terminal_runtimes) {
            if hidden.runtime.project_key != project_key {
                retained_hidden_runtimes.insert(runtime_key, hidden);
                continue;
            }
            if let Err(error) = hidden.runtime.handle.kill() {
                error!(
                    "failed to stop hidden Files terminal runtime for project {project_key} during {reason}: {error:#}"
                );
            }
        }
        self.files_hidden_terminal_runtimes = retained_hidden_runtimes;
    }

    pub(super) fn files_toggle_terminal_drawer_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_files_terminal_drawer(Some(window), cx);
    }

    pub(super) fn files_new_terminal_tab_action(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let Some(project_key) = self.current_files_terminal_owner_key() else {
            return;
        };
        if !self.files_terminal_open {
            self.capture_files_terminal_restore_target(window.as_deref(), cx);
        }
        self.files_save_visible_terminal_tab();
        self.files_park_visible_terminal_runtime_for_project(Some(project_key.as_str()));
        let tab_id = self.files_terminal_next_tab_id.max(1);
        self.files_terminal_next_tab_id = tab_id.saturating_add(1);
        self.files_terminal_tabs.push(TerminalTabState::new(tab_id));
        self.files_terminal_tabs.sort_by_key(|tab| tab.id);
        self.files_terminal_active_tab_id = tab_id;
        self.files_apply_visible_terminal_tab();
        self.files_terminal_set_open(true, cx);
        self.ensure_files_terminal_session(cx);
        self.defer_files_terminal_interaction_focus(cx);
        cx.notify();
    }

    fn files_select_relative_terminal_tab(
        &mut self,
        delta: isize,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.files_terminal_tabs.len() < 2 {
            return;
        }
        let Some(project_key) = self.current_files_terminal_owner_key() else {
            return;
        };
        if !self.files_terminal_open {
            self.capture_files_terminal_restore_target(window.as_deref(), cx);
        }
        self.files_save_visible_terminal_tab();
        self.files_park_visible_terminal_runtime_for_project(Some(project_key.as_str()));
        let tabs = self.files_terminal_tabs.clone();
        let current_index = tabs
            .iter()
            .position(|tab| tab.id == self.files_terminal_active_tab_id)
            .unwrap_or(0);
        let next_index =
            (current_index as isize + delta).rem_euclid(tabs.len() as isize) as usize;
        self.files_terminal_active_tab_id = tabs[next_index].id;
        self.files_apply_visible_terminal_tab();
        if self.files_terminal_open
            && !self.files_promote_hidden_terminal_runtime_for_project(
                project_key.as_str(),
                self.files_terminal_active_tab_id,
            )
        {
            self.ensure_files_terminal_session(cx);
        }
        self.defer_files_terminal_interaction_focus(cx);
        cx.notify();
    }

    pub(super) fn files_select_terminal_tab(
        &mut self,
        tab_id: TerminalTabId,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.files_terminal_active_tab_id == tab_id {
            self.defer_files_terminal_interaction_focus(cx);
            return;
        }
        if !self.files_terminal_tabs.iter().any(|tab| tab.id == tab_id) {
            return;
        }
        let Some(project_key) = self.current_files_terminal_owner_key() else {
            return;
        };
        if !self.files_terminal_open {
            self.capture_files_terminal_restore_target(window.as_deref(), cx);
        }
        self.files_save_visible_terminal_tab();
        self.files_park_visible_terminal_runtime_for_project(Some(project_key.as_str()));
        self.files_terminal_active_tab_id = tab_id;
        self.files_apply_visible_terminal_tab();
        if self.files_terminal_open
            && !self.files_promote_hidden_terminal_runtime_for_project(
                project_key.as_str(),
                self.files_terminal_active_tab_id,
            )
        {
            self.ensure_files_terminal_session(cx);
        }
        self.defer_files_terminal_interaction_focus(cx);
        cx.notify();
    }

    fn files_close_terminal_tab_action(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let Some(project_key) = self.current_files_terminal_owner_key() else {
            return;
        };
        if !self.files_terminal_open {
            self.capture_files_terminal_restore_target(window.as_deref(), cx);
        }
        let closed_tab_id = self.files_terminal_active_tab_id;
        if self.files_terminal_runtime.as_ref().is_some_and(|runtime| {
            runtime.project_key == project_key && runtime.tab_id == closed_tab_id
        }) {
            self.stop_files_terminal_runtime("closing terminal tab");
        } else {
            let runtime_key = terminal_runtime_tab_key(project_key.as_str(), closed_tab_id);
            if let Some(hidden) = self.files_hidden_terminal_runtimes.remove(runtime_key.as_str())
                && let Err(error) = hidden.runtime.handle.kill()
            {
                error!("failed to stop hidden Files terminal tab during close: {error:#}");
            }
        }
        self.files_terminal_tabs.retain(|tab| tab.id != closed_tab_id);
        if self.files_terminal_tabs.is_empty() {
            self.files_terminal_tabs = default_terminal_tabs();
            self.files_terminal_active_tab_id = 1;
            self.files_terminal_next_tab_id = 2;
            self.files_apply_visible_terminal_tab();
            self.files_terminal_set_open(false, cx);
            return;
        }

        self.files_terminal_active_tab_id = self
            .files_terminal_tabs
            .first()
            .map(|tab| tab.id)
            .unwrap_or(1);
        self.files_apply_visible_terminal_tab();
        if self.files_terminal_open
            && !self.files_promote_hidden_terminal_runtime_for_project(
                project_key.as_str(),
                self.files_terminal_active_tab_id,
            )
        {
            self.ensure_files_terminal_session(cx);
        }
        self.defer_files_terminal_interaction_focus(cx);
        cx.notify();
    }

    pub(super) fn files_clear_terminal_session_action(&mut self, cx: &mut Context<Self>) {
        if !self.files_terminal_is_running() {
            self.stop_files_terminal_runtime("clearing terminal session");
        }
        self.files_terminal_pending_input = None;
        self.files_terminal_session.transcript.clear();
        self.files_terminal_session.screen = None;
        self.files_terminal_session.status_message = None;
        self.files_terminal_session.exit_code = None;
        self.files_terminal_follow_output = true;
        if !self.files_terminal_is_running() {
            self.files_terminal_session.status = AiTerminalSessionStatus::Idle;
        }
        if self.files_terminal_open {
            self.ensure_files_terminal_session(cx);
        } else {
            self.files_clear_terminal_cursor_output_suppression(cx);
        }
        self.files_sync_terminal_cursor_blink(cx);
        cx.notify();
    }

    pub(super) fn files_rerun_terminal_command_action(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self.files_terminal_session.last_command.clone() else {
            self.files_terminal_session.status_message = Some("No command to rerun.".to_string());
            cx.notify();
            return;
        };
        self.files_run_command_in_terminal(command, cx);
    }

    pub(super) fn files_focus_terminal_surface_action(&mut self, cx: &mut Context<Self>) {
        if self.files_terminal_runtime.is_some() {
            self.focus_files_terminal_surface(cx);
        }
    }

    pub(super) fn files_terminal_surface_focus_in(&mut self, cx: &mut Context<Self>) {
        if !self.files_terminal_surface_focused {
            self.files_terminal_surface_focused = true;
            cx.notify();
        }
        self.files_sync_terminal_cursor_blink(cx);
        self.files_report_terminal_focus_change(true, cx);
    }

    pub(super) fn files_terminal_surface_focus_out(&mut self, cx: &mut Context<Self>) {
        if self.files_terminal_surface_focused {
            self.files_terminal_surface_focused = false;
            cx.notify();
        }
        self.files_sync_terminal_cursor_blink(cx);
        self.files_report_terminal_focus_change(false, cx);
    }

    pub(super) fn files_terminal_surface_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        line: i32,
        column: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let mode = self.files_terminal_session.screen.as_ref().map(|screen| screen.mode);
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if event.button == MouseButton::Middle && !mode.unwrap_or_default().mouse_mode {
            return self.files_paste_terminal_from_primary_selection(cx);
        }

        let point = AiTerminalGridPoint { line, column };
        let Some(input) =
            ai_terminal_mouse_button_input(point, event.button, event.modifiers, true, mode)
        else {
            return false;
        };
        self.files_write_terminal_pointer_input(input, cx)
    }

    pub(super) fn files_terminal_surface_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        line: i32,
        column: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let mode = self.files_terminal_session.screen.as_ref().map(|screen| screen.mode);
        let point = AiTerminalGridPoint { line, column };
        let Some(input) =
            ai_terminal_mouse_move_input(point, event.pressed_button, event.modifiers, mode)
        else {
            return false;
        };
        self.files_write_terminal_pointer_input(input, cx)
    }

    pub(super) fn files_terminal_surface_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        line: i32,
        column: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let mode = self.files_terminal_session.screen.as_ref().map(|screen| screen.mode);
        let point = AiTerminalGridPoint { line, column };
        let Some(input) =
            ai_terminal_mouse_button_input(point, event.button, event.modifiers, false, mode)
        else {
            return false;
        };
        self.files_write_terminal_pointer_input(input, cx)
    }

    pub(super) fn files_terminal_surface_key_down(
        &mut self,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.files_terminal_runtime.is_none()
            || !self.files_terminal_focus_handle.is_focused(window)
        {
            return false;
        }

        if ai_terminal_uses_copy_shortcut(keystroke) && self.files_terminal_selection_active() {
            return self.ai_copy_selected_text(cx);
        }

        let terminal_mode = self.files_terminal_session.screen.as_ref().map(|screen| screen.mode);

        if let Some(scroll) = ai_terminal_viewport_scroll_for_keystroke(keystroke, terminal_mode) {
            return self.files_scroll_terminal_viewport(scroll, cx);
        }

        if !self.files_terminal_is_running() {
            return false;
        }

        if ai_terminal_uses_desktop_clipboard_shortcut(keystroke) {
            if keystroke.key == "v" {
                return self.files_paste_terminal_from_clipboard(cx);
            }
            return false;
        }

        if ai_terminal_uses_insert_paste_shortcut(keystroke) {
            return self.files_paste_terminal_from_clipboard(cx);
        }

        let Some(input) = ai_terminal_key_input_for_keystroke(keystroke, terminal_mode) else {
            return false;
        };
        self.files_write_terminal_key_input(input, cx)
    }

    fn files_terminal_dispatch_synthesized_keystroke(
        &mut self,
        keystroke: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(keystroke) = gpui::Keystroke::parse(keystroke) else {
            error!("failed to parse synthesized Files terminal keystroke: {keystroke}");
            return;
        };

        if self.files_terminal_surface_key_down(&keystroke, window, cx) {
            cx.stop_propagation();
        }
    }

    pub(super) fn files_terminal_send_ctrl_c_action(
        &mut self,
        _: &AiTerminalSendCtrlC,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("ctrl-c", window, cx);
    }

    pub(super) fn files_terminal_send_ctrl_a_action(
        &mut self,
        _: &AiTerminalSendCtrlA,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("ctrl-a", window, cx);
    }

    pub(super) fn files_terminal_send_tab_action(
        &mut self,
        _: &AiTerminalSendTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("tab", window, cx);
    }

    pub(super) fn files_terminal_send_back_tab_action(
        &mut self,
        _: &AiTerminalSendBackTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("shift-tab", window, cx);
    }

    pub(super) fn files_terminal_send_up_action(
        &mut self,
        _: &AiTerminalSendUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("up", window, cx);
    }

    pub(super) fn files_terminal_send_down_action(
        &mut self,
        _: &AiTerminalSendDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("down", window, cx);
    }

    pub(super) fn files_terminal_send_left_action(
        &mut self,
        _: &AiTerminalSendLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("left", window, cx);
    }

    pub(super) fn files_terminal_send_right_action(
        &mut self,
        _: &AiTerminalSendRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("right", window, cx);
    }

    pub(super) fn files_terminal_send_home_action(
        &mut self,
        _: &AiTerminalSendHome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("home", window, cx);
    }

    pub(super) fn files_terminal_send_end_action(
        &mut self,
        _: &AiTerminalSendEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_dispatch_synthesized_keystroke("end", window, cx);
    }

    pub(super) fn files_terminal_surface_scroll_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        line: i32,
        column: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.files_terminal_runtime.is_none() {
            return false;
        }

        let line_height = px(16.0);
        let Some(scroll_lines) = ai_terminal_scroll_lines_from_event(event, line_height) else {
            return false;
        };

        let mode = self.files_terminal_session.screen.as_ref().map(|screen| screen.mode);
        let point = AiTerminalGridPoint { line, column };
        if let Some(input) = ai_terminal_wheel_input(point, scroll_lines, event.modifiers) {
            let fallback_scroll =
                ai_terminal_viewport_scroll_for_wheel_delta(scroll_lines, mode);
            return self.files_write_terminal_wheel_input(input, fallback_scroll, cx);
        } else if mode.is_some_and(|mode| mode.alt_screen) {
            return true;
        }

        self.files_scroll_terminal_viewport(TerminalScroll::Delta(-scroll_lines), cx)
    }

    pub(super) fn files_scroll_terminal_to_bottom_action(&mut self, cx: &mut Context<Self>) {
        let _ = self.files_scroll_terminal_viewport(TerminalScroll::Bottom, cx);
    }

    pub(super) fn files_update_terminal_panel_bounds(
        &mut self,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let bounds_changed = self.files_terminal_panel_bounds.is_none_or(|current| {
            (current.origin.x - bounds.origin.x).abs() > px(0.5)
                || (current.origin.y - bounds.origin.y).abs() > px(0.5)
                || (current.size.width - bounds.size.width).abs() > px(0.5)
                || (current.size.height - bounds.size.height).abs() > px(0.5)
        });
        if !bounds_changed {
            return;
        }
        self.files_terminal_panel_bounds = Some(bounds);
        cx.notify();
    }

    pub(super) fn files_resize_terminal_height_from_position(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.files_terminal_panel_bounds else {
            return;
        };
        let next_height = (bounds.bottom() - position.y).max(px(AI_TERMINAL_MIN_HEIGHT_PX));
        let clamped_height = next_height
            .min(px(AI_TERMINAL_MAX_HEIGHT_PX))
            .max(px(AI_TERMINAL_MIN_HEIGHT_PX));
        let next_height_px: f32 = clamped_height.into();
        if (next_height_px - self.files_terminal_height_px).abs() <= 0.5 {
            return;
        }
        self.files_terminal_height_px = next_height_px;
        cx.notify();
    }

    pub(super) fn files_resize_terminal_surface(
        &mut self,
        rows: u16,
        cols: u16,
        cx: &mut Context<Self>,
    ) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        if self.files_terminal_grid_size == Some((rows, cols)) {
            return;
        }
        self.files_terminal_grid_size = Some((rows, cols));

        let Some(runtime) = self.files_terminal_runtime.as_ref() else {
            return;
        };
        if let Err(error) = runtime.handle.resize(rows, cols) {
            self.files_terminal_session.status_message = Some(error.to_string());
            self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
        }
    }

    fn files_scroll_terminal_viewport(
        &mut self,
        scroll: TerminalScroll,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .files_terminal_session
            .screen
            .as_ref()
            .is_some_and(|screen| screen.mode.alt_screen)
        {
            return false;
        }

        self.clear_files_terminal_text_selection(cx);

        let Some(runtime) = self.files_terminal_runtime.as_ref() else {
            return false;
        };

        if let Err(error) = runtime.handle.scroll_display(scroll) {
            self.files_terminal_session.status_message = Some(error.to_string());
            self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return true;
        }

        true
    }

    fn files_write_terminal_pointer_input(
        &mut self,
        input: hunk_terminal::TerminalPointerInput,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.files_terminal_is_running() {
            return false;
        }
        let Some(runtime) = self.files_terminal_runtime.as_ref() else {
            return false;
        };

        if let Err(error) = runtime.handle.write_pointer_input(input) {
            self.files_terminal_session.status_message = Some(error.to_string());
            self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return true;
        }

        self.files_terminal_session.status_message = None;
        true
    }

    fn files_write_terminal_wheel_input(
        &mut self,
        input: hunk_terminal::TerminalWheelInput,
        fallback_scroll: Option<TerminalScroll>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.files_terminal_is_running() {
            return false;
        }
        let Some(runtime) = self.files_terminal_runtime.as_ref() else {
            return false;
        };

        match runtime.handle.write_wheel_input(input, fallback_scroll) {
            Ok(()) => {
                self.files_terminal_session.status_message = None;
                true
            }
            Err(error) => {
                self.files_terminal_session.status_message = Some(error.to_string());
                self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
                cx.notify();
                true
            }
        }
    }

    fn files_write_terminal_key_input(
        &mut self,
        input: hunk_terminal::TerminalKeyInput,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.files_terminal_is_running() {
            return false;
        }
        let Some(runtime) = self.files_terminal_runtime.as_ref() else {
            return false;
        };

        if let Err(error) = runtime.handle.write_key_input(input) {
            self.files_terminal_session.status_message = Some(error.to_string());
            self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return true;
        }

        self.files_terminal_session.status_message = None;
        true
    }

    fn files_report_terminal_focus_change(&mut self, focused: bool, cx: &mut Context<Self>) {
        if !self.files_terminal_is_running() {
            return;
        }
        let Some(runtime) = self.files_terminal_runtime.as_ref() else {
            return;
        };
        if let Err(error) = runtime.handle.report_focus(focused) {
            self.files_terminal_session.status_message = Some(error.to_string());
            self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
        }
    }

    fn files_paste_terminal_from_clipboard(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        if text.is_empty() {
            return false;
        }

        self.files_paste_terminal_text(text.as_str(), cx)
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn files_paste_terminal_from_primary_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = cx.read_from_primary().and_then(|item| item.text()) else {
            return false;
        };
        if text.is_empty() {
            return false;
        }

        self.files_paste_terminal_text(text.as_str(), cx)
    }

    fn files_paste_terminal_text(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        if !self.files_terminal_is_running() {
            return false;
        }
        if text.contains(['\r', '\n']) {
            self.files_temporarily_suppress_terminal_cursor(cx);
        }
        let Some(runtime) = self.files_terminal_runtime.as_ref() else {
            return false;
        };
        if let Err(error) = runtime.handle.write_paste(text) {
            self.files_terminal_session.status_message = Some(error.to_string());
            self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return true;
        }

        self.files_terminal_session.status_message = None;
        true
    }

    pub(super) fn files_run_command_in_terminal(
        &mut self,
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

        let Some(project_key) = self.current_files_terminal_owner_key() else {
            return;
        };

        let Some(target_cwd) = self.primary_repo_root() else {
            self.files_terminal_session.status_message =
                Some("Open a repository before using the terminal.".to_string());
            self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
            self.capture_files_terminal_restore_target(None, cx);
            self.files_terminal_set_open(true, cx);
            cx.notify();
            return;
        };

        if !self.files_terminal_open {
            self.capture_files_terminal_restore_target(None, cx);
        }
        self.files_terminal_set_open(true, cx);
        self.files_terminal_session.last_command = Some(command.clone());
        self.files_terminal_pending_input = Some(command);

        let session_cwd_matches = self
            .files_terminal_session
            .cwd
            .as_ref()
            .is_some_and(|cwd| cwd == &target_cwd);

        if self.files_terminal_is_running() && session_cwd_matches {
            self.flush_files_terminal_pending_input(cx);
            self.defer_files_terminal_interaction_focus(cx);
            return;
        }

        self.start_default_files_terminal_session(target_cwd, project_key, cx);
    }

    fn start_default_files_terminal_session(
        &mut self,
        cwd: PathBuf,
        project_key: String,
        cx: &mut Context<Self>,
    ) {
        let tab_id = self.files_terminal_active_tab_id;
        if let Some(active_runtime_project_key) = self
            .files_terminal_runtime
            .as_ref()
            .map(|runtime| runtime.project_key.clone())
        {
            let active_runtime_tab_id = self
                .files_terminal_runtime
                .as_ref()
                .map(|runtime| runtime.tab_id)
                .unwrap_or(tab_id);
            if active_runtime_project_key == project_key && active_runtime_tab_id == tab_id {
                self.stop_files_terminal_runtime("starting default terminal shell");
            } else {
                self.files_park_visible_terminal_runtime_for_project(Some(
                    active_runtime_project_key.as_str(),
                ));
            }
        }

        let resolved_shell = crate::terminal_env::resolve_terminal_shell(&self.config.terminal);
        let request = TerminalSpawnRequest::shell(cwd.clone())
            .with_shell_program(resolved_shell.program().to_os_string())
            .with_shell_args(
                resolved_shell.interactive_shell_args(self.config.terminal.inherit_login_environment),
            );
        match spawn_terminal_session(request) {
            Ok((handle, event_rx)) => {
                self.files_terminal_open = true;
                self.files_terminal_stop_requested = false;
                self.files_terminal_session.cwd = Some(cwd);
                if self.files_terminal_pending_input.is_none() {
                    self.files_terminal_session.last_command = None;
                }
                self.files_terminal_session.status = AiTerminalSessionStatus::Running;
                self.files_terminal_session.exit_code = None;
                self.files_terminal_session.screen = None;
                self.files_terminal_grid_size = None;
                self.files_terminal_follow_output = true;
                self.files_terminal_session.status_message = None;
                self.files_clear_terminal_cursor_output_suppression(cx);
                self.files_sync_terminal_cursor_blink(cx);
                let generation = self.next_files_terminal_runtime_generation();
                self.files_terminal_runtime = Some(FilesTerminalRuntimeHandle {
                    project_key: project_key.clone(),
                    tab_id,
                    handle,
                    generation,
                });
                self.start_files_terminal_event_listener(event_rx, project_key, tab_id, generation, cx);
                self.defer_files_terminal_interaction_focus(cx);
            }
            Err(error) => {
                self.files_terminal_open = true;
                self.files_terminal_session.cwd = Some(cwd);
                self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
                self.files_terminal_session.exit_code = None;
                self.files_terminal_session.screen = None;
                self.files_terminal_grid_size = None;
                self.files_terminal_session.status_message =
                    Some("Failed to start terminal shell.".to_string());
                self.files_clear_terminal_cursor_output_suppression(cx);
                self.files_sync_terminal_cursor_blink(cx);
                append_ai_terminal_transcript(
                    &mut self.files_terminal_session.transcript,
                    format!("[terminal error] {error}\n"),
                );
            }
        }
    }

    fn start_files_terminal_event_listener(
        &mut self,
        event_rx: std::sync::mpsc::Receiver<TerminalEvent>,
        project_key: String,
        tab_id: TerminalTabId,
        generation: usize,
        cx: &mut Context<Self>,
    ) {
        self.files_terminal_event_task = cx.spawn(async move |this, cx| {
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
                    if !this.files_terminal_runtime_is_current(project_key.as_str(), tab_id, generation) {
                        listener_is_current = false;
                        return;
                    }
                    for event in buffered_events {
                        let visible_project_tab_matches = this
                            .files_terminal_runtime
                            .as_ref()
                            .is_some_and(|runtime| {
                                runtime.project_key == project_key && runtime.tab_id == tab_id
                            });
                        if visible_project_tab_matches {
                            this.apply_files_terminal_event(event, cx);
                        } else {
                            this.apply_hidden_files_terminal_event(project_key.as_str(), tab_id, event);
                        }
                    }
                    if event_stream_disconnected
                        && this.files_terminal_runtime_is_current(project_key.as_str(), tab_id, generation)
                    {
                        if this
                            .files_terminal_runtime
                            .as_ref()
                            .is_some_and(|runtime| {
                                runtime.project_key == project_key && runtime.tab_id == tab_id
                            })
                        {
                            this.files_terminal_runtime = None;
                        } else {
                            let runtime_key = terminal_runtime_tab_key(project_key.as_str(), tab_id);
                            this.files_hidden_terminal_runtimes.remove(runtime_key.as_str());
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

    fn apply_files_terminal_event(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        match event {
            TerminalEvent::Output(output) => {
                let sanitized = sanitize_ai_terminal_output(output.as_slice());
                if sanitized.is_empty() {
                    return;
                }
                self.files_temporarily_suppress_terminal_cursor(cx);
                append_ai_terminal_transcript(&mut self.files_terminal_session.transcript, sanitized);
            }
            TerminalEvent::Screen(screen) => {
                if self.files_terminal_is_running() {
                    self.clear_files_terminal_text_selection(cx);
                }
                self.files_terminal_follow_output = screen.display_offset == 0;
                self.files_terminal_session.screen = Some(screen);
                self.files_sync_terminal_cursor_blink(cx);
                self.flush_files_terminal_pending_input(cx);
            }
            TerminalEvent::Exit { exit_code } => {
                let stopped = self.files_terminal_stop_requested;
                self.files_terminal_stop_requested = false;
                self.files_terminal_runtime = None;
                self.files_terminal_session.exit_code = exit_code;
                if stopped {
                    self.files_terminal_session.status = AiTerminalSessionStatus::Stopped;
                } else if exit_code == Some(0) {
                    self.files_terminal_session.status = AiTerminalSessionStatus::Completed;
                } else {
                    self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
                }
                self.files_close_terminal_after_exit(cx);
            }
            TerminalEvent::Failed(message) => {
                self.files_terminal_stop_requested = false;
                self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
                self.files_terminal_session.status_message = Some(message.clone());
                self.files_clear_terminal_cursor_output_suppression(cx);
                self.files_sync_terminal_cursor_blink(cx);
                append_ai_terminal_transcript(
                    &mut self.files_terminal_session.transcript,
                    format!("[terminal error] {message}\n"),
                );
            }
        }
    }

    fn apply_hidden_files_terminal_event(
        &mut self,
        project_key: &str,
        tab_id: TerminalTabId,
        event: TerminalEvent,
    ) {
        match event {
            TerminalEvent::Output(output) => {
                let sanitized = sanitize_ai_terminal_output(output.as_slice());
                if sanitized.is_empty() {
                    return;
                }
                let state = self
                    .files_terminal_states_by_project
                    .entry(project_key.to_string())
                    .or_default();
                let tab = terminal_tab_state_mut(&mut state.tabs, tab_id);
                append_ai_terminal_transcript(
                    &mut tab.session.transcript,
                    sanitized,
                );
            }
            TerminalEvent::Screen(screen) => {
                let state = self
                    .files_terminal_states_by_project
                    .entry(project_key.to_string())
                    .or_default();
                let tab = terminal_tab_state_mut(&mut state.tabs, tab_id);
                tab.follow_output = screen.display_offset == 0;
                tab.session.screen = Some(screen);
                tab.session.status = AiTerminalSessionStatus::Running;
                self.flush_hidden_files_terminal_pending_input(project_key, tab_id);
            }
            TerminalEvent::Exit { .. } => {
                let runtime_key = terminal_runtime_tab_key(project_key, tab_id);
                self.files_hidden_terminal_runtimes.remove(runtime_key.as_str());
            }
            TerminalEvent::Failed(message) => {
                let state = self
                    .files_terminal_states_by_project
                    .entry(project_key.to_string())
                    .or_default();
                let tab = terminal_tab_state_mut(&mut state.tabs, tab_id);
                tab.session.status = AiTerminalSessionStatus::Failed;
                tab.session.status_message = Some(message.clone());
                append_ai_terminal_transcript(
                    &mut tab.session.transcript,
                    format!("[terminal error] {message}\n"),
                );
            }
        }
    }

    fn flush_files_terminal_pending_input(&mut self, cx: &mut Context<Self>) {
        if !self.files_terminal_is_running() {
            return;
        }
        let Some(runtime) = self.files_terminal_runtime.as_ref() else {
            return;
        };
        let Some(mut input) = self.files_terminal_pending_input.take() else {
            return;
        };

        if !input.ends_with('\n') {
            input.push('\n');
        }

        if let Err(error) = runtime.handle.write_input(input.as_bytes()) {
            self.files_terminal_pending_input = Some(input.trim_end_matches('\n').to_string());
            self.files_terminal_session.status_message = Some(error.to_string());
            self.files_terminal_session.status = AiTerminalSessionStatus::Failed;
            cx.notify();
            return;
        }

        self.files_terminal_session.status_message = None;
        cx.notify();
    }

    fn flush_hidden_files_terminal_pending_input(&mut self, project_key: &str, tab_id: TerminalTabId) {
        let input = self
            .files_terminal_states_by_project
            .get_mut(project_key)
            .and_then(|state| {
                state
                    .tabs
                    .iter_mut()
                    .find(|tab| tab.id == tab_id)
                    .and_then(|tab| tab.pending_input.take())
            });
        let Some(mut input) = input else {
            return;
        };

        if !input.ends_with('\n') {
            input.push('\n');
        }

        let runtime_key = terminal_runtime_tab_key(project_key, tab_id);
        let Some(hidden) = self.files_hidden_terminal_runtimes.get(runtime_key.as_str()) else {
            let state = self
                .files_terminal_states_by_project
                .entry(project_key.to_string())
                .or_default();
            terminal_tab_state_mut(&mut state.tabs, tab_id).pending_input =
                Some(input.trim_end_matches('\n').to_string());
            return;
        };

        if let Err(error) = hidden.runtime.handle.write_input(input.as_bytes()) {
            let state = self
                .files_terminal_states_by_project
                .entry(project_key.to_string())
                .or_default();
            let tab = terminal_tab_state_mut(&mut state.tabs, tab_id);
            tab.pending_input = Some(input.trim_end_matches('\n').to_string());
            tab.session.status = AiTerminalSessionStatus::Failed;
            tab.session.status_message = Some(error.to_string());
        }
    }

    fn files_close_terminal_after_exit(&mut self, cx: &mut Context<Self>) {
        if self.files_terminal_tabs.len() > 1 {
            let closed_tab_id = self.files_terminal_active_tab_id;
            self.files_terminal_tabs.retain(|tab| tab.id != closed_tab_id);
            self.files_terminal_active_tab_id = self
                .files_terminal_tabs
                .first()
                .map(|tab| tab.id)
                .unwrap_or(1);
            self.files_apply_visible_terminal_tab();
            if let Some(project_key) = self.current_files_terminal_owner_key()
                && self.files_terminal_open
                && !self.files_promote_hidden_terminal_runtime_for_project(
                    project_key.as_str(),
                    self.files_terminal_active_tab_id,
                )
            {
                self.ensure_files_terminal_session(cx);
            }
            self.files_sync_terminal_cursor_blink(cx);
            self.defer_files_terminal_interaction_focus(cx);
            cx.notify();
            return;
        }
        self.files_terminal_open = false;
        self.files_terminal_surface_focused = false;
        self.files_terminal_cursor_blink_visible = true;
        self.files_terminal_follow_output = true;
        self.files_terminal_pending_input = None;
        self.files_terminal_session = AiTerminalSessionState::default();
        self.files_terminal_tabs = default_terminal_tabs();
        self.files_terminal_active_tab_id = 1;
        self.files_terminal_next_tab_id = 2;
        self.files_terminal_restore_target = FilesTerminalRestoreTarget::default();
        self.files_terminal_grid_size = None;
        self.files_clear_terminal_cursor_output_suppression(cx);
        self.files_sync_terminal_cursor_blink(cx);
        self.defer_files_focus_restore_after_terminal_close(cx);
        cx.notify();
    }

    fn files_terminal_cursor_should_blink(&self) -> bool {
        self.files_terminal_open
            && self.files_terminal_surface_focused
            && !self.files_terminal_cursor_output_suppressed
            && self
                .files_terminal_session
                .screen
                .as_ref()
                .is_some_and(|screen| {
                    screen.mode.show_cursor
                        && crate::app::terminal_cursor::ai_terminal_cursor_shape_blinks(
                            screen.cursor.shape,
                        )
                })
    }

    fn files_clear_terminal_cursor_output_suppression(&mut self, cx: &mut Context<Self>) {
        self.files_terminal_cursor_output_generation =
            self.files_terminal_cursor_output_generation.saturating_add(1);
        self.files_terminal_cursor_output_task = Task::ready(());
        if self.files_terminal_cursor_output_suppressed {
            self.files_terminal_cursor_output_suppressed = false;
            self.files_sync_terminal_cursor_blink(cx);
            cx.notify();
        }
    }

    fn files_temporarily_suppress_terminal_cursor(&mut self, cx: &mut Context<Self>) {
        self.files_terminal_cursor_output_generation =
            self.files_terminal_cursor_output_generation.saturating_add(1);
        let generation = self.files_terminal_cursor_output_generation;
        let state_changed = !self.files_terminal_cursor_output_suppressed;
        self.files_terminal_cursor_output_suppressed = true;
        self.files_sync_terminal_cursor_blink(cx);
        if state_changed {
            cx.notify();
        }
        self.files_terminal_cursor_output_task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(crate::app::terminal_cursor::AI_TERMINAL_CURSOR_OUTPUT_QUIET_INTERVAL)
                .await;

            let Some(this) = this.upgrade() else {
                return;
            };

            this.update(cx, |this, cx| {
                if this.files_terminal_cursor_output_generation != generation {
                    return;
                }
                this.files_terminal_cursor_output_task = Task::ready(());
                if !this.files_terminal_cursor_output_suppressed {
                    return;
                }
                this.files_terminal_cursor_output_suppressed = false;
                this.files_sync_terminal_cursor_blink(cx);
                cx.notify();
            });
        });
    }

    fn files_stop_terminal_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.files_terminal_cursor_blink_generation =
            self.files_terminal_cursor_blink_generation.saturating_add(1);
        self.files_terminal_cursor_blink_active = false;
        self.files_terminal_cursor_blink_task = Task::ready(());
        if !self.files_terminal_cursor_blink_visible {
            self.files_terminal_cursor_blink_visible = true;
            cx.notify();
        }
    }

    fn files_start_terminal_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.files_terminal_cursor_blink_generation =
            self.files_terminal_cursor_blink_generation.saturating_add(1);
        let generation = self.files_terminal_cursor_blink_generation;
        self.files_terminal_cursor_blink_active = true;
        if !self.files_terminal_cursor_blink_visible {
            self.files_terminal_cursor_blink_visible = true;
            cx.notify();
        }
        self.files_terminal_cursor_blink_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(crate::app::terminal_cursor::AI_TERMINAL_CURSOR_BLINK_INTERVAL)
                    .await;

                let Some(this) = this.upgrade() else {
                    return;
                };

                let mut keep_running = true;
                this.update(cx, |this, cx| {
                    if this.files_terminal_cursor_blink_generation != generation
                        || !this.files_terminal_cursor_should_blink()
                    {
                        this.files_terminal_cursor_blink_active = false;
                        if !this.files_terminal_cursor_blink_visible {
                            this.files_terminal_cursor_blink_visible = true;
                            cx.notify();
                        }
                        keep_running = false;
                        return;
                    }

                    this.files_terminal_cursor_blink_visible =
                        !this.files_terminal_cursor_blink_visible;
                    cx.notify();
                });
                if !keep_running {
                    return;
                }
            }
        });
    }

    pub(super) fn files_sync_terminal_cursor_blink(&mut self, cx: &mut Context<Self>) {
        if self.files_terminal_cursor_should_blink() {
            if !self.files_terminal_cursor_blink_active {
                self.files_start_terminal_cursor_blink(cx);
            }
            return;
        }

        if self.files_terminal_cursor_blink_active || !self.files_terminal_cursor_blink_visible {
            self.files_stop_terminal_cursor_blink(cx);
        }
    }
}
