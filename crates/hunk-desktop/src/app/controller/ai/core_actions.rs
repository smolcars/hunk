impl DiffViewer {
    const AI_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(33);
    const AI_EVENT_IDLE_FOREGROUND_INTERVAL: Duration = Duration::from_secs(1);

    pub(super) fn preload_ai_runtime_on_startup(&mut self, cx: &mut Context<Self>) {
        if self.ai_workspace_key().is_none() {
            return;
        }
        self.refresh_ai_repo_thread_catalog(cx);
        self.ensure_ai_runtime_started(cx);
    }

    pub(super) fn ensure_ai_runtime_started(&mut self, cx: &mut Context<Self>) {
        let workspace_key = self.ai_workspace_key();
        self.ensure_ai_runtime_started_for_workspace_key(workspace_key.as_deref(), cx);
    }

    pub(super) fn ensure_ai_runtime_started_for_workspace_key(
        &mut self,
        workspace_key: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let cwd = workspace_key
            .map(std::path::PathBuf::from)
            .or_else(|| self.ai_workspace_cwd());
        let Some(cwd) = cwd else {
            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_bootstrap_loading = false;
            self.ai_error_message = Some("Open a workspace before using AI.".to_string());
            self.invalidate_ai_visible_frame_state_with_reason("runtime");
            cx.notify();
            return;
        };
        let worker_workspace_key = cwd.to_string_lossy().to_string();
        if self.ai_command_tx.is_some()
            && self.ai_worker_workspace_key.as_deref() == Some(worker_workspace_key.as_str())
        {
            return;
        }
        if self.ai_runtime_start_is_in_flight_for_workspace(worker_workspace_key.as_str()) {
            return;
        }
        if self.ai_command_tx.is_some() {
            let visible_workspace_key = self.ai_worker_workspace_key.clone();
            self.store_current_ai_workspace_state(visible_workspace_key.as_deref());
            self.park_visible_ai_runtime();
        }
        self.join_ai_worker_thread_if_finished("starting AI runtime");

        self.sync_ai_workspace_preferences_from_state();

        if self.promote_hidden_ai_runtime(worker_workspace_key.as_str()) {
            cx.notify();
            return;
        }

        let Some(codex_home) = resolve_codex_home_path() else {
            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_bootstrap_loading = false;
            self.ai_error_message = Some("Unable to resolve the Codex home directory.".to_string());
            self.invalidate_ai_visible_frame_state_with_reason("runtime");
            cx.notify();
            return;
        };

        let codex_executable = Self::resolve_codex_executable_path();
        if let Err(error) = Self::validate_codex_executable_path(codex_executable.as_path()) {
            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_bootstrap_loading = false;
            self.ai_error_message = Some(error);
            self.invalidate_ai_visible_frame_state_with_reason("runtime");
            cx.notify();
            return;
        }
        let (command_tx, command_rx) = std::sync::mpsc::channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let mut start_config = AiWorkerStartConfig::new(cwd, codex_executable, codex_home);
        start_config.mad_max_mode = self.ai_mad_max_mode;
        start_config.include_hidden_models = self.ai_include_hidden_models;

        let worker = spawn_ai_worker(start_config, command_rx, event_tx);
        self.mark_ai_runtime_start_in_flight(worker_workspace_key.as_str());

        self.ai_connection_state = AiConnectionState::Connecting;
        self.ai_bootstrap_loading = true;
        self.ai_error_message = None;
        self.ai_status_message = Some("Starting Codex App Server...".to_string());
        self.invalidate_ai_visible_frame_state_with_reason("runtime");
        let listener_workspace_key = worker_workspace_key.clone();
        self.ai_command_tx = Some(command_tx);
        self.ai_worker_thread = Some(worker);
        self.ai_worker_workspace_key = Some(worker_workspace_key);

        let epoch = self.next_ai_event_epoch();
        self.start_ai_event_listener(event_rx, listener_workspace_key, epoch, cx);
        cx.notify();
    }

    pub(super) fn ai_refresh_threads(&mut self, cx: &mut Context<Self>) {
        self.refresh_ai_repo_thread_catalog(cx);
        self.send_ai_worker_command(AiWorkerCommand::RefreshThreads, cx);
    }

    pub(super) fn ai_start_chatgpt_login_action(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::StartChatgptLogin, cx);
    }

    pub(super) fn ai_cancel_chatgpt_login_action(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::CancelChatgptLogin, cx);
    }

    pub(super) fn ai_logout_account_action(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::LogoutAccount, cx);
    }

    pub(super) fn ai_create_thread_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous_workspace_key = self.ai_workspace_key();
        let draft_start_mode = self.ai_new_thread_start_mode;
        let draft_worktree_base_branch_name = self.ai_worktree_base_branch_name.clone();
        self.sync_ai_workspace_target_from_catalog(cx);
        let next_workspace_key = self.ai_workspace_key_for_draft();
        let previous_draft_key = self.current_ai_composer_draft_key();
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        if let Some(workspace_key) = self.workspace_ai_composer_draft_key() {
            self.ai_composer_drafts
                .insert(workspace_key.clone(), Default::default());
            self.ai_composer_status_by_draft.remove(&workspace_key);
            self.ai_composer_status_generation_by_key
                .remove(&AiComposerStatusKey::Draft(workspace_key));
        }
        self.ai_handle_workspace_change_to(previous_workspace_key, next_workspace_key, cx);
        self.ai_new_thread_start_mode = draft_start_mode;
        self.ai_worktree_base_branch_name = draft_worktree_base_branch_name;
        self.ai_new_thread_draft_active = true;
        self.ai_pending_new_thread_selection = false;
        self.ai_pending_thread_start = None;
        self.ai_selected_thread_id = None;
        self.ai_review_mode_active = false;
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = false;
        self.ai_expanded_timeline_row_ids.clear();
        self.ai_text_selection = None;
        self.ai_text_selection_drag_pointer = None;
        self.ai_text_selection_auto_scroll_task = Task::ready(());
        self.invalidate_ai_visible_frame_state_with_reason("thread");
        if previous_draft_key != self.current_ai_composer_draft_key() {
            self.restore_ai_visible_composer_from_current_draft_in_window(window, cx);
        } else {
            self.clear_ai_composer_input(window, cx);
        }
        self.focus_ai_composer_input(window, cx);
        cx.notify();
    }

    pub(super) fn ai_new_thread_action(
        &mut self,
        _: &AiNewThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            return;
        }
        self.ai_start_thread_draft(AiNewThreadStartMode::Local, window, cx);
    }

    pub(super) fn ai_new_worktree_thread_shortcut_action(
        &mut self,
        _: &AiNewWorktreeThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            return;
        }
        if self.current_ai_workspace_kind() == AiWorkspaceKind::Chats {
            self.set_current_ai_composer_status(
                "Worktree threads are unavailable in Chats.",
                cx,
            );
            cx.notify();
            return;
        }
        self.ai_start_thread_draft(AiNewThreadStartMode::Worktree, window, cx);
    }

    pub(super) fn ai_open_working_tree_diff_viewer_action(
        &mut self,
        _: &AiOpenWorkingTreeDiffViewer,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            return;
        }
        if self.current_ai_workspace_kind() == AiWorkspaceKind::Chats {
            self.set_current_ai_composer_status(
                "Diff review is unavailable in Chats.",
                cx,
            );
            cx.notify();
            return;
        }
        self.ai_toggle_inline_review_for_current_thread_in_mode(
            AiInlineReviewMode::WorkingTree,
            cx,
        );
    }

    pub(super) fn ai_select_new_thread_start_mode_action(
        &mut self,
        start_mode: AiNewThreadStartMode,
        cx: &mut Context<Self>,
    ) {
        if self.current_ai_workspace_kind() == AiWorkspaceKind::Chats {
            return;
        }
        if !self.ai_new_thread_draft_active || self.ai_pending_new_thread_selection {
            return;
        }
        if self.ai_new_thread_start_mode == start_mode {
            return;
        }
        self.ai_new_thread_start_mode = start_mode;
        self.ai_draft_workspace_target_id = self.primary_workspace_target_id().or_else(|| {
            self.workspace_targets
                .first()
                .map(|target| target.id.clone())
        });
        self.sync_ai_worktree_base_branch_from_repo();
        self.sync_ai_worktree_base_branch_picker_state(cx);
        cx.notify();
    }

    fn ai_start_thread_draft(
        &mut self,
        start_mode: AiNewThreadStartMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current_ai_workspace_kind() == AiWorkspaceKind::Chats {
            self.ai_start_chat_thread_draft(window, cx);
            return;
        }
        let Some(project_root) = self
            .ai_visible_project_root()
            .or_else(|| self.primary_repo_root())
        else {
            self.ai_new_thread_start_mode = start_mode;
            self.ai_draft_workspace_root_override = None;
            self.ai_draft_workspace_target_id = self.primary_workspace_target_id().or_else(|| {
                self.workspace_targets
                    .first()
                    .map(|target| target.id.clone())
            });
            self.sync_ai_worktree_base_branch_from_repo();
            self.sync_ai_worktree_base_branch_picker_state(cx);
            self.sync_ai_workspace_target_from_catalog(cx);
            self.ai_create_thread_action(window, cx);
            return;
        };
        self.ai_start_thread_draft_for_project_root(project_root, start_mode, window, cx);
    }

    pub(super) fn ai_start_thread_draft_for_project_root(
        &mut self,
        project_root: PathBuf,
        start_mode: AiNewThreadStartMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_new_thread_start_mode = start_mode;
        self.ai_draft_workspace_root_override = Some(project_root.clone());
        self.ai_draft_workspace_target_id =
            hunk_git::worktree::list_workspace_targets(project_root.as_path())
                .ok()
                .and_then(|targets| {
                    targets
                        .into_iter()
                        .find(|target| {
                            matches!(
                                target.kind,
                                hunk_git::worktree::WorkspaceTargetKind::PrimaryCheckout
                            )
                        })
                        .map(|target| target.id)
                });
        if start_mode == AiNewThreadStartMode::Worktree {
            let project_matches_non_ai_root = self
                .primary_repo_root()
                .is_some_and(|root| root == project_root);
            if project_matches_non_ai_root {
                self.sync_ai_worktree_base_branch_from_repo();
            } else {
                self.ai_worktree_base_branch_name =
                    resolve_default_base_branch_name(project_root.as_path())
                        .ok()
                        .flatten();
            }
        } else {
            self.ai_worktree_base_branch_name = None;
        }
        self.sync_ai_worktree_base_branch_picker_state(cx);
        self.sync_ai_workspace_target_from_catalog(cx);
        self.ai_create_thread_action(window, cx);
    }

    pub(super) fn ai_start_chat_thread_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chats_root) = self.ai_chats_root_path() else {
            self.set_current_ai_composer_status(
                "Failed to resolve the Chats workspace.",
                cx,
            );
            cx.notify();
            return;
        };

        self.ai_new_thread_start_mode = AiNewThreadStartMode::Local;
        self.ai_draft_workspace_root_override = Some(chats_root);
        self.ai_draft_workspace_target_id = None;
        self.ai_worktree_base_branch_name = None;
        self.sync_ai_worktree_base_branch_picker_state(cx);
        self.sync_ai_workspace_target_from_catalog(cx);
        self.ai_create_thread_action(window, cx);
    }

    pub(super) fn ai_send_prompt_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_review_mode_active {
            self.ai_start_review_action(window, cx);
            return;
        }
        if self.send_current_ai_prompt(cx) {
            self.clear_ai_composer_input(window, cx);
        }
    }

    pub(super) fn ai_send_prompt_action_from_keyboard(&mut self, cx: &mut Context<Self>) {
        if self.ai_review_mode_active {
            if !self.start_current_ai_review(cx) {
                return;
            }
        } else if !self.send_current_ai_prompt(cx) {
            return;
        }
        let ai_composer_state = self.ai_composer_input_state.clone();
        self.clear_current_ai_composer_status();
        if let Some(draft) = self.current_ai_composer_draft_mut() {
            draft.prompt.clear();
            draft.local_images.clear();
            draft.skill_bindings.clear();
        }
        self.invalidate_ai_visible_frame_state_with_reason("thread");
        if let Err(error) = Self::update_any_window(cx, |window, cx| {
            ai_composer_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        }) {
            error!("failed to clear AI composer input after keyboard send: {error:#}");
        }
    }

    pub(super) fn ai_handle_composer_shortcut_keystroke(
        &mut self,
        shortcut: AiComposerShortcut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            return;
        }

        let composer_focus_handle =
            gpui::Focusable::focus_handle(self.ai_composer_input_state.read(cx), cx);
        if !composer_focus_handle.is_focused(window) {
            return;
        }

        window.prevent_default();
        cx.stop_propagation();
        match shortcut {
            AiComposerShortcut::QueuePrompt => {
                self.ai_queue_prompt_action(&AiQueuePrompt, window, cx)
            }
            AiComposerShortcut::EditLastQueuedPrompt => {
                self.ai_edit_last_queued_prompt_action(&AiEditLastQueuedPrompt, window, cx)
            }
        }
    }

    pub(super) fn ai_queue_prompt_action(
        &mut self,
        _: &AiQueuePrompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            return;
        }

        let Some(thread_id) = self.current_ai_thread_id() else {
            self.ai_send_prompt_action(window, cx);
            return;
        };

        if self
            .current_ai_in_progress_turn_id(thread_id.as_str())
            .is_none()
        {
            self.ai_send_prompt_action(window, cx);
            return;
        }

        let Some(validated) = self.validated_current_ai_prompt(cx) else {
            return;
        };
        let AiValidatedPrompt {
            prompt,
            local_images,
            selected_skills,
            skill_bindings,
        } = validated;

        self.queue_current_ai_prompt_for_thread(
            thread_id.clone(),
            prompt,
            local_images,
            selected_skills,
            skill_bindings,
        );
        self.clear_ai_composer_input(window, cx);
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = true;
        self.flush_ai_timeline_scroll_request();
        cx.notify();
    }

    pub(super) fn ai_edit_last_queued_prompt_action(
        &mut self,
        _: &AiEditLastQueuedPrompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            return;
        }

        let Some(thread_id) = self.current_ai_thread_id() else {
            return;
        };
        let Some(queued) = self.edit_last_ai_queued_message_for_thread(thread_id.as_str()) else {
            return;
        };

        self.clear_current_ai_composer_status();
        if let Some(draft) = self.current_ai_composer_draft_mut() {
            draft.prompt = queued.prompt.clone();
            draft.local_images = queued.local_images.clone();
            draft.skill_bindings = queued.skill_bindings.clone();
        }
        self.invalidate_ai_visible_frame_state_with_reason("thread");
        self.ai_composer_input_state.update(cx, |state, cx| {
            state.set_value(queued.prompt, window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn ai_composer_paste_action(
        &mut self,
        _: &gpui_component::input::Paste,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(clipboard) = cx.read_from_clipboard() else {
            cx.propagate();
            return;
        };

        let attachments = match crate::app::ai_composer_clipboard::ai_composer_clipboard_attachments(
            &clipboard,
            crate::app::ai_composer_clipboard::ai_composer_pasted_image_dir().as_path(),
        ) {
            Ok(Some(attachments)) => attachments,
            Ok(None) => {
                cx.propagate();
                return;
            }
            Err(error) => {
                cx.stop_propagation();
                self.set_current_ai_composer_status(
                    format!("Failed to attach pasted image: {error:#}"),
                    cx,
                );
                cx.notify();
                return;
            }
        };

        if !self.current_ai_model_supports_image_inputs() {
            cx.stop_propagation();
            self.set_current_ai_composer_status(
                "Selected model does not support image attachments. Remove attachments or switch models.",
                cx,
            );
            cx.notify();
            return;
        }

        cx.stop_propagation();
        let added = self.ai_add_composer_local_images(attachments.paths);
        if added > 0 {
            self.invalidate_ai_visible_frame_state_with_reason("thread");
        }
        if let Some(message) = ai_attachment_status_message(attachments.item_count, added) {
            self.set_current_ai_composer_status(message, cx);
        }
        cx.notify();
    }

    pub(super) fn ai_open_attachment_picker_action(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach Images".into()),
        });

        self.ai_attachment_picker_task = cx.spawn(async move |this, cx| {
            let selection = match prompt.await {
                Ok(selection) => selection,
                Err(err) => {
                    error!("ai attachment picker prompt channel closed: {err}");
                    return;
                }
            };

            let selected_paths = match selection {
                Ok(Some(paths)) => paths,
                Ok(None) => return,
                Err(err) => {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.set_current_ai_composer_status(
                                format!("Failed to open image picker: {err:#}"),
                                cx,
                            );
                            cx.notify();
                        });
                    }
                    return;
                }
            };

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    let selected_count = selected_paths.len();
                    let added = this.ai_add_composer_local_images(selected_paths);
                    if added > 0 {
                        this.invalidate_ai_visible_frame_state_with_reason("thread");
                    }
                    if let Some(message) = ai_attachment_status_message(selected_count, added) {
                        this.set_current_ai_composer_status(message, cx);
                    }
                    cx.notify();
                });
            }
        });
    }

    pub(super) fn ai_remove_composer_attachment_action(
        &mut self,
        path: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        let mut removed = false;
        if let Some(draft) = self.current_ai_composer_draft_mut() {
            let before = draft.local_images.len();
            draft.local_images.retain(|existing| existing != &path);
            removed = draft.local_images.len() != before;
        }
        if removed {
            self.invalidate_ai_visible_frame_state_with_reason("thread");
            cx.notify();
        }
    }

    pub(super) fn ai_add_dropped_composer_paths_action(
        &mut self,
        dropped_paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if dropped_paths.is_empty() {
            return;
        }

        if !self.current_ai_model_supports_image_inputs() {
            self.set_current_ai_composer_status(
                "Selected model does not support image attachments. Remove attachments or switch models.",
                cx,
            );
            cx.notify();
            return;
        }

        let dropped_count = dropped_paths.len();
        let added = self.ai_add_composer_local_images(dropped_paths);
        if added > 0 {
            self.invalidate_ai_visible_frame_state_with_reason("thread");
        }
        if let Some(message) = ai_attachment_status_message(dropped_count, added) {
            self.set_current_ai_composer_status(message, cx);
        }
        self.focus_ai_composer_input(window, cx);
        cx.notify();
    }

    pub(super) fn ai_start_review_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.start_current_ai_review(cx) {
            return;
        }
        self.clear_ai_composer_input(window, cx);
    }

    fn start_current_ai_review(&mut self, cx: &mut Context<Self>) -> bool {
        if self.current_ai_workspace_kind() == AiWorkspaceKind::Chats {
            self.set_current_ai_composer_status(
                "Review mode is unavailable in Chats.",
                cx,
            );
            cx.notify();
            return false;
        }
        if let Some(reason) = self.ai_review_blocker() {
            self.set_current_ai_composer_status(reason, cx);
            cx.notify();
            return false;
        }

        let Some(thread_id) = self.current_ai_thread_id() else {
            self.set_current_ai_composer_status("Select a thread before starting review.", cx);
            cx.notify();
            return false;
        };

        let instructions = self
            .ai_composer_input_state
            .read(cx)
            .value()
            .trim()
            .to_string();
        let instructions = if instructions.is_empty() {
            "Review the current working-copy changes for correctness and regressions.".to_string()
        } else {
            instructions
        };

        if self.send_ai_worker_command(
            AiWorkerCommand::StartReview {
                thread_id,
                instructions,
            },
            cx,
        ) {
            self.clear_current_ai_composer_status();
            return true;
        }
        false
    }

    pub(super) fn ai_review_blocker(&self) -> Option<String> {
        let Some(thread_id) = self.current_ai_thread_id() else {
            return Some("Select a thread before starting review.".to_string());
        };
        if self
            .current_ai_in_progress_turn_id(thread_id.as_str())
            .is_some()
        {
            return Some("Wait for the current run to finish or interrupt it first.".to_string());
        }
        None
    }

    pub(super) fn ai_interrupt_turn_action(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_ai_thread_id() else {
            self.set_current_ai_composer_status("Select a thread before interrupting a turn.", cx);
            cx.notify();
            return;
        };

        let Some(turn_id) = self.current_ai_in_progress_turn_id(thread_id.as_str()) else {
            self.set_current_ai_composer_status("No in-progress turn to interrupt.", cx);
            cx.notify();
            return;
        };

        if self.send_ai_worker_command(
            AiWorkerCommand::InterruptTurn {
                thread_id: thread_id.clone(),
                turn_id,
            },
            cx,
        ) {
            self.ai_interrupt_restore_queued_thread_ids
                .insert(thread_id);
            self.set_current_ai_composer_status("Interrupted", cx);
            cx.notify();
        }
    }

    pub(super) fn ai_interrupt_selected_turn_action(
        &mut self,
        _: &AiInterruptSelectedTurn,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            return;
        }
        let Some(thread_id) = self.current_ai_thread_id() else {
            return;
        };
        if self
            .current_ai_in_progress_turn_id(thread_id.as_str())
            .is_none()
        {
            return;
        }
        self.ai_interrupt_turn_action(cx);
    }

    pub(super) fn ai_set_mad_max_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let Some(workspace_key) = self.ai_workspace_key() else {
            self.set_current_ai_composer_status(
                "Open a workspace before changing the approval policy.",
                cx,
            );
            return;
        };

        set_workspace_mad_max_mode(&mut self.state, workspace_key.as_str(), enabled);
        self.persist_state();
        self.ai_mad_max_mode = enabled;
        self.send_ai_worker_command_if_running(AiWorkerCommand::SetMadMaxMode { enabled }, cx);
        self.set_current_ai_composer_status(
            if enabled {
                "Approval policy set to Full access."
            } else {
                "Approval policy set to Ask for approvals."
            },
            cx,
        );
    }

    pub(super) fn ai_select_model_action(
        &mut self,
        model_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_selected_model = model_id;
        self.normalize_ai_selected_effort();
        self.persist_current_ai_workspace_session();
        self.invalidate_ai_visible_frame_state_with_reason("settings");
        cx.notify();
    }

    pub(super) fn ai_select_effort_action(
        &mut self,
        effort: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_selected_effort = effort;
        self.normalize_ai_selected_effort();
        self.persist_current_ai_workspace_session();
        cx.notify();
    }

    pub(super) fn ai_select_service_tier_action(
        &mut self,
        service_tier: AiServiceTierSelection,
        cx: &mut Context<Self>,
    ) {
        self.ai_selected_service_tier = service_tier;
        self.persist_current_ai_workspace_session();
        cx.notify();
    }

    pub(super) fn ai_select_collaboration_mode_action(
        &mut self,
        selection: AiCollaborationModeSelection,
        cx: &mut Context<Self>,
    ) {
        if self.current_ai_workspace_kind() == AiWorkspaceKind::Chats {
            self.ai_selected_collaboration_mode = AiCollaborationModeSelection::Default;
            self.ai_review_mode_active = false;
            self.persist_current_ai_workspace_session();
            self.set_current_ai_composer_status(
                "Mode switching is unavailable in Chats.",
                cx,
            );
            self.invalidate_ai_visible_frame_state_with_reason("settings");
            cx.notify();
            return;
        }
        let current_thread_id = self.current_ai_thread_id();
        if let Some(thread_id) = current_thread_id.as_ref() {
            self.ai_review_mode_thread_ids.remove(thread_id.as_str());
        }
        self.ai_review_mode_active = false;
        self.ai_selected_collaboration_mode = selection;
        if let Some(mask) = ai_collaboration_mode_mask(&self.ai_collaboration_modes, selection) {
            if let Some(model) = mask.model.as_ref() {
                self.ai_selected_model = Some(model.clone());
            }
            if let Some(reasoning_effort) = mask.reasoning_effort.unwrap_or(None) {
                self.ai_selected_effort = Some(reasoning_effort_key(&reasoning_effort));
            }
        }
        self.normalize_ai_selected_effort();
        self.persist_current_ai_workspace_session();
        self.sync_ai_followup_prompt_state_for_selected_thread(current_thread_id.as_deref());
        self.invalidate_ai_visible_frame_state_with_reason("settings");
        cx.notify();
    }

    pub(super) fn ai_select_review_mode_action(&mut self, cx: &mut Context<Self>) {
        if self.current_ai_workspace_kind() == AiWorkspaceKind::Chats {
            self.set_current_ai_composer_status(
                "Review mode is unavailable in Chats.",
                cx,
            );
            cx.notify();
            return;
        }
        let Some(thread_id) = self.current_ai_thread_id() else {
            self.set_current_ai_composer_status(
                "Select a thread before switching to review mode.",
                cx,
            );
            cx.notify();
            return;
        };
        self.ai_selected_collaboration_mode = AiCollaborationModeSelection::Default;
        if let Some(mask) = ai_collaboration_mode_mask(
            &self.ai_collaboration_modes,
            AiCollaborationModeSelection::Default,
        ) {
            if let Some(model) = mask.model.as_ref() {
                self.ai_selected_model = Some(model.clone());
            }
            if let Some(reasoning_effort) = mask.reasoning_effort.unwrap_or(None) {
                self.ai_selected_effort = Some(reasoning_effort_key(&reasoning_effort));
            }
        }
        self.normalize_ai_selected_effort();
        self.ai_review_mode_thread_ids.insert(thread_id);
        self.ai_review_mode_active = true;
        self.persist_current_ai_workspace_session();
        let current_thread_id = self.current_ai_thread_id();
        self.sync_ai_followup_prompt_state_for_selected_thread(current_thread_id.as_deref());
        cx.notify();
    }

    pub(super) fn ai_open_usage_overlay_action(&mut self, cx: &mut Context<Self>) {
        self.ai_usage_popover_open = true;
        cx.notify();
    }

    pub(super) fn ai_close_usage_overlay_action(&mut self, cx: &mut Context<Self>) {
        if !self.ai_usage_popover_open {
            return;
        }
        self.ai_usage_popover_open = false;
        cx.notify();
    }

    pub(super) fn ai_resolve_pending_approval_action(
        &mut self,
        request_id: String,
        decision: AiApprovalDecision,
        cx: &mut Context<Self>,
    ) {
        let approval = self.ai_pending_approval(request_id.as_str());
        let status_target = approval
            .as_ref()
            .map(|approval| AiComposerDraftKey::Thread(approval.thread_id.clone()));
        let workspace_key = approval.as_ref().and_then(|approval| {
            self.ai_thread_workspace_root(approval.thread_id.as_str())
                .map(|root| root.to_string_lossy().to_string())
        });
        if self.send_ai_worker_command_for_workspace(
            workspace_key.as_deref(),
            AiWorkerCommand::ResolveApproval {
                request_id,
                decision,
            },
            true,
            cx,
        ) {
            let message = match decision {
                AiApprovalDecision::Accept => "Approval accepted.".to_string(),
                AiApprovalDecision::Decline => "Approval declined.".to_string(),
            };
            self.set_ai_composer_status_for_target(status_target, message, cx);
            cx.notify();
        }
    }

    pub(super) fn ai_select_thread(
        &mut self,
        thread_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_workspace_key = self.ai_workspace_key();
        let next_workspace_key = self
            .ai_thread_workspace_root(thread_id.as_str())
            .map(|root| root.to_string_lossy().to_string())
            .or_else(|| previous_workspace_key.clone());
        let previous_draft_key = self.current_ai_composer_draft_key();
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.ai_handle_workspace_change_to(previous_workspace_key, next_workspace_key, cx);
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = true;
        self.ai_workspace_selection = None;
        self.ai_workspace_surface_last_scroll_offset = None;
        self.ai_expanded_timeline_row_ids.clear();
        self.ai_text_selection = None;
        self.ai_text_selection_drag_pointer = None;
        self.ai_text_selection_auto_scroll_task = Task::ready(());
        self.ai_new_thread_draft_active = false;
        self.ai_pending_new_thread_selection = false;
        let previous_terminal_thread_id = self.current_ai_thread_id();
        self.ai_selected_thread_id = Some(thread_id.clone());
        self.ai_review_mode_active = self.ai_review_mode_thread_ids.contains(thread_id.as_str());
        self.ai_handle_terminal_thread_change(
            previous_terminal_thread_id,
            Some(thread_id.clone()),
            cx,
        );
        self.invalidate_ai_visible_frame_state_with_reason("thread");
        if previous_draft_key != self.current_ai_composer_draft_key() {
            self.restore_ai_visible_composer_from_current_draft_in_window(window, cx);
        }
        self.flush_ai_timeline_scroll_request();
        self.sync_ai_session_selection_from_state();
        self.sync_ai_followup_prompt_state_for_selected_thread(Some(thread_id.as_str()));
        self.send_ai_worker_command(AiWorkerCommand::SelectThread { thread_id }, cx);
        cx.notify();
    }

    pub(super) fn ai_scroll_timeline_to_bottom_action(&mut self, cx: &mut Context<Self>) {
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = true;
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        self.flush_ai_timeline_scroll_request();
        cx.notify();
    }

    pub(super) fn ai_archive_thread_action(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let workspace_key = self
            .ai_thread_workspace_root(thread_id.as_str())
            .map(|root| root.to_string_lossy().to_string());
        if !self.send_ai_worker_command_for_workspace(
            workspace_key.as_deref(),
            AiWorkerCommand::ArchiveThread {
                thread_id: thread_id.clone(),
            },
            true,
            cx,
        ) {}
    }

    pub(super) fn ai_toggle_thread_bookmark(&mut self, thread_id: String, cx: &mut Context<Self>) {
        self.ai_toggle_thread_bookmark_action(thread_id, cx);
    }

    #[allow(dead_code)]
    pub(super) fn ai_toggle_timeline_row_expansion_action(
        &mut self,
        row_id: String,
        cx: &mut Context<Self>,
    ) {
        let changed_row_id = self
            .ai_timeline_container_row_id(row_id.as_str())
            .unwrap_or_else(|| row_id.clone());
        let changed_row_ids = [changed_row_id.clone()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        self.ai_clear_text_selection_for_rows(&changed_row_ids, cx);
        if self.ai_expanded_timeline_row_ids.contains(row_id.as_str()) {
            self.ai_expanded_timeline_row_ids.remove(row_id.as_str());
        } else {
            self.ai_expanded_timeline_row_ids.insert(row_id);
        }
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        cx.notify();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AiComposerShortcut {
    QueuePrompt,
    EditLastQueuedPrompt,
}

pub(super) fn ai_composer_shortcut_for_keystroke(
    keystroke: &gpui::Keystroke,
) -> Option<AiComposerShortcut> {
    let modifiers = &keystroke.modifiers;
    match keystroke.key.as_str() {
        "tab" if !modifiers.modified() => Some(AiComposerShortcut::QueuePrompt),
        "up" if modifiers.control
            && !modifiers.alt
            && modifiers.shift
            && !modifiers.platform
            && !modifiers.function =>
        {
            Some(AiComposerShortcut::EditLastQueuedPrompt)
        }
        _ => None,
    }
}
