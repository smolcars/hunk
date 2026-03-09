use std::collections::BTreeMap;
use std::time::Duration;

use crate::app::ai_paths::resolve_codex_home_path;
use hunk_domain::state::AiCollaborationModeSelection;
use hunk_domain::state::AiServiceTierSelection;
use hunk_domain::state::AiThreadSessionState;
use hunk_codex::state::ThreadLifecycleStatus;
use hunk_codex::state::ThreadSummary;
use hunk_codex::state::TurnStatus;
use hunk_domain::state::AppState;

impl DiffViewer {
    const AI_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(33);
    const AI_THREAD_INLINE_TOAST_DURATION: Duration = Duration::from_millis(2200);

    pub(super) fn ensure_ai_runtime_started(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.ai_workspace_cwd() else {
            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_bootstrap_loading = false;
            self.ai_error_message = Some("Open a workspace before using AI.".to_string());
            cx.notify();
            return;
        };
        let worker_workspace_key = cwd.to_string_lossy().to_string();
        if self.ai_command_tx.is_some()
            && self.ai_worker_workspace_key.as_deref() == Some(worker_workspace_key.as_str())
        {
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
            cx.notify();
            return;
        };

        let codex_executable = Self::resolve_codex_executable_path();
        if let Err(error) = Self::validate_codex_executable_path(codex_executable.as_path()) {
            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_bootstrap_loading = false;
            self.ai_error_message = Some(error);
            cx.notify();
            return;
        }
        let (command_tx, command_rx) = std::sync::mpsc::channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let mut start_config = AiWorkerStartConfig::new(cwd, codex_executable, codex_home);
        start_config.mad_max_mode = self.ai_mad_max_mode;
        start_config.include_hidden_models = self.ai_include_hidden_models;

        let worker = spawn_ai_worker(start_config, command_rx, event_tx);

        self.ai_connection_state = AiConnectionState::Connecting;
        self.ai_bootstrap_loading = true;
        self.ai_error_message = None;
        self.ai_status_message = Some("Starting Codex App Server...".to_string());
        let listener_workspace_key = worker_workspace_key.clone();
        self.ai_command_tx = Some(command_tx);
        self.ai_worker_thread = Some(worker);
        self.ai_worker_workspace_key = Some(worker_workspace_key);

        let epoch = self.next_ai_event_epoch();
        self.start_ai_event_listener(event_rx, listener_workspace_key, epoch, cx);
        cx.notify();
    }

    pub(super) fn ai_refresh_threads(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::RefreshThreads, cx);
    }

    pub(super) fn ai_refresh_account(&mut self, cx: &mut Context<Self>) {
        self.send_ai_worker_command(AiWorkerCommand::RefreshAccount, cx);
        self.send_ai_worker_command(AiWorkerCommand::RefreshRateLimits, cx);
        self.send_ai_worker_command(AiWorkerCommand::RefreshSessionMetadata, cx);
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

    pub(super) fn ai_create_thread_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_workspace_key = self.ai_workspace_key();
        let draft_start_mode = self.ai_new_thread_start_mode;
        let draft_worktree_base_branch_name = self.ai_worktree_base_branch_name.clone();
        self.sync_ai_workspace_target_from_catalog(cx);
        let previous_draft_key = self.current_ai_composer_draft_key();
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        if let Some(workspace_key) = self.workspace_ai_composer_draft_key() {
            self.ai_composer_drafts
                .insert(workspace_key.clone(), Default::default());
            self.ai_composer_status_by_draft.remove(&workspace_key);
        }
        self.ai_new_thread_draft_active = true;
        self.ai_pending_new_thread_selection = false;
        self.ai_selected_thread_id = None;
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = false;
        self.ai_expanded_timeline_row_ids.clear();
        self.ai_text_selection = None;
        self.ai_handle_workspace_change(previous_workspace_key, cx);
        self.ai_new_thread_start_mode = draft_start_mode;
        self.ai_worktree_base_branch_name = draft_worktree_base_branch_name;
        self.ai_new_thread_draft_active = true;
        self.ai_pending_new_thread_selection = false;
        self.ai_selected_thread_id = None;
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = false;
        self.ai_expanded_timeline_row_ids.clear();
        self.ai_text_selection = None;
        if previous_draft_key != self.current_ai_composer_draft_key() {
            self.restore_ai_visible_composer_from_current_draft_in_window(window, cx);
        } else {
            self.clear_ai_composer_input(window, cx);
        }
        reset_ai_timeline_list_measurements(self, 0);
        self.focus_ai_composer_input(window, cx);
        cx.notify();
    }

    pub(super) fn ai_new_thread_action(
        &mut self,
        _: &AiNewThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.set_workspace_view_mode(WorkspaceSwitchAction::Ai.target_mode(), cx);
        self.ai_start_thread_draft(AiNewThreadStartMode::Local, window, cx);
    }

    pub(super) fn ai_new_worktree_thread_shortcut_action(
        &mut self,
        _: &AiNewWorktreeThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.set_workspace_view_mode(WorkspaceSwitchAction::Ai.target_mode(), cx);
        self.ai_start_thread_draft(AiNewThreadStartMode::Worktree, window, cx);
    }

    pub(super) fn ai_select_new_thread_start_mode_action(
        &mut self,
        start_mode: AiNewThreadStartMode,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_new_thread_draft_active || self.ai_pending_new_thread_selection {
            return;
        }
        if self.ai_new_thread_start_mode == start_mode {
            return;
        }
        self.ai_new_thread_start_mode = start_mode;
        self.ai_draft_workspace_target_id = self
            .primary_workspace_target_id()
            .or_else(|| self.workspace_targets.first().map(|target| target.id.clone()));
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
        self.ai_new_thread_start_mode = start_mode;
        self.ai_draft_workspace_target_id = self
            .primary_workspace_target_id()
            .or_else(|| self.workspace_targets.first().map(|target| target.id.clone()));
        self.sync_ai_worktree_base_branch_from_repo();
        self.sync_ai_worktree_base_branch_picker_state(cx);
        self.sync_ai_workspace_target_from_catalog(cx);
        self.ai_create_thread_action(window, cx);
    }

    pub(super) fn ai_send_prompt_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.send_current_ai_prompt(cx) {
            self.clear_ai_composer_input(window, cx);
        }
    }

    pub(super) fn ai_send_prompt_action_from_keyboard(&mut self, cx: &mut Context<Self>) {
        if !self.send_current_ai_prompt(cx) {
            return;
        }
        let ai_composer_state = self.ai_composer_input_state.clone();
        self.clear_current_ai_composer_status();
        if let Some(draft) = self.current_ai_composer_draft_mut() {
            draft.prompt.clear();
            draft.local_images.clear();
        }
        let Some(window_handle) = cx.windows().into_iter().next() else {
            return;
        };
        if let Err(error) = cx.update_window(window_handle, |_, window, cx| {
            ai_composer_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        }) {
            error!("failed to clear AI composer input after keyboard send: {error:#}");
        }
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
                            this.set_current_ai_composer_status(format!(
                                "Failed to open image picker: {err:#}"
                            ));
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
                    if let Some(message) =
                        ai_attachment_status_message(selected_count, added)
                    {
                        this.set_current_ai_composer_status(message);
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
            );
            cx.notify();
            return;
        }

        let dropped_count = dropped_paths.len();
        let added = self.ai_add_composer_local_images(dropped_paths);
        if let Some(message) = ai_attachment_status_message(dropped_count, added) {
            self.set_current_ai_composer_status(message);
        }
        self.focus_ai_composer_input(window, cx);
        cx.notify();
    }

    pub(super) fn ai_start_review_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = self.current_ai_thread_id() else {
            self.set_current_ai_composer_status("Select a thread before starting review.");
            cx.notify();
            return;
        };

        let instructions = self.ai_composer_input_state.read(cx).value().trim().to_string();
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
            self.clear_ai_composer_input(window, cx);
        }
    }

    pub(super) fn ai_interrupt_turn_action(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_ai_thread_id() else {
            self.set_current_ai_composer_status("Select a thread before interrupting a turn.");
            cx.notify();
            return;
        };

        let Some(turn_id) = self.current_ai_in_progress_turn_id(thread_id.as_str()) else {
            self.set_current_ai_composer_status("No in-progress turn to interrupt.");
            cx.notify();
            return;
        };

        if self.send_ai_worker_command(
            AiWorkerCommand::InterruptTurn { thread_id, turn_id },
            cx,
        ) {
            self.set_current_ai_composer_status("Interrupted");
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
            self.ai_status_message = Some("Open a workspace before changing Mad Max mode.".to_string());
            cx.notify();
            return;
        };

        if enabled {
            self.state.ai_workspace_mad_max.insert(workspace_key, true);
        } else {
            self.state.ai_workspace_mad_max.remove(workspace_key.as_str());
        }
        self.persist_state();
        self.ai_mad_max_mode = enabled;
        self.send_ai_worker_command_if_running(AiWorkerCommand::SetMadMaxMode { enabled }, cx);
        self.ai_status_message = Some(if enabled {
            "Mad Max mode enabled: approvals are auto-accepted with full sandbox access."
                .to_string()
        } else {
            "Mad Max mode disabled: command and file approvals require explicit review."
                .to_string()
        });
        cx.notify();
    }

    pub(super) fn ai_select_model_action(
        &mut self,
        model_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_selected_model = model_id;
        self.normalize_ai_selected_effort();
        self.persist_current_ai_workspace_session();
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
        self.ai_selected_collaboration_mode = selection;
        if let Some(mask) = ai_collaboration_mode_mask(
            &self.ai_collaboration_modes,
            selection,
        ) {
            if let Some(model) = mask.model.as_ref() {
                self.ai_selected_model = Some(model.clone());
            }
            if let Some(reasoning_effort) = mask.reasoning_effort.unwrap_or(None) {
                self.ai_selected_effort = Some(reasoning_effort_key(&reasoning_effort));
            }
        }
        self.normalize_ai_selected_effort();
        self.persist_current_ai_workspace_session();
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
            self.set_ai_composer_status_for_target(status_target, message);
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
        let previous_draft_key = self.current_ai_composer_draft_key();
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = true;
        self.ai_expanded_timeline_row_ids.clear();
        self.ai_text_selection = None;
        self.ai_new_thread_draft_active = false;
        self.ai_pending_new_thread_selection = false;
        self.ai_selected_thread_id = Some(thread_id.clone());
        self.ai_handle_workspace_change(previous_workspace_key, cx);
        self.ai_selected_thread_id = Some(thread_id.clone());
        if previous_draft_key != self.current_ai_composer_draft_key() {
            self.restore_ai_visible_composer_from_current_draft_in_window(window, cx);
        }
        let visible_row_ids = current_ai_renderable_visible_row_ids(self, thread_id.as_str());
        reset_ai_timeline_list_measurements(self, visible_row_ids.len());
        self.sync_ai_session_selection_from_state();
        self.send_ai_worker_command(AiWorkerCommand::SelectThread { thread_id }, cx);
        cx.notify();
    }

    pub(super) fn ai_scroll_timeline_to_bottom_action(&mut self, cx: &mut Context<Self>) {
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = true;
        cx.notify();
    }

    fn show_ai_thread_inline_toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.ai_thread_inline_toast_epoch = self.ai_thread_inline_toast_epoch.wrapping_add(1);
        let epoch = self.ai_thread_inline_toast_epoch;
        self.ai_thread_inline_toast = Some(message.into());
        self.ai_thread_inline_toast_task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Self::AI_THREAD_INLINE_TOAST_DURATION)
                .await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |this, cx| {
                if this.ai_thread_inline_toast_epoch != epoch {
                    return;
                }
                this.ai_thread_inline_toast = None;
                cx.notify();
            });
        });
        cx.notify();
    }

    pub(super) fn ai_archive_thread_action(&mut self, thread_id: String, cx: &mut Context<Self>) {
        if !self.send_ai_worker_command(
            AiWorkerCommand::ArchiveThread {
                thread_id: thread_id.clone(),
            },
            cx,
        ) {
            return;
        }

        if self.ai_selected_thread_id.as_deref() == Some(thread_id.as_str()) {
            self.ai_selected_thread_id = None;
            self.ai_expanded_timeline_row_ids.clear();
            self.ai_text_selection = None;
            self.ai_timeline_follow_output = true;
            self.ai_scroll_timeline_to_bottom = true;
        }
        self.show_ai_thread_inline_toast("Thread archived.", cx);
    }

    pub(super) fn ai_toggle_timeline_row_expansion_action(
        &mut self,
        row_id: String,
        cx: &mut Context<Self>,
    ) {
        let changed_row_id = self
            .ai_timeline_container_row_id(row_id.as_str())
            .unwrap_or_else(|| row_id.clone());
        let changed_row_ids = [changed_row_id.clone()].into_iter().collect::<BTreeSet<_>>();
        self.ai_clear_text_selection_for_rows(&changed_row_ids, cx);
        if self.ai_expanded_timeline_row_ids.contains(row_id.as_str()) {
            self.ai_expanded_timeline_row_ids.remove(row_id.as_str());
        } else {
            self.ai_expanded_timeline_row_ids.insert(row_id);
        }
        if let Some(selected_thread_id) = self.ai_selected_thread_id.as_deref() {
            let visible_row_ids = current_ai_renderable_visible_row_ids(self, selected_thread_id);
            invalidate_ai_timeline_row_measurements(self, visible_row_ids.as_slice(), &changed_row_ids);
        }
        cx.notify();
    }

    pub(super) fn ai_copy_thread_id_action(
        &mut self,
        thread_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(thread_id.clone()));
        let message = format!("Copied thread ID: {thread_id}");
        gpui_component::WindowExt::push_notification(
            window,
            gpui_component::notification::Notification::success(message),
            cx,
        );
        cx.notify();
    }

    pub(super) fn ai_open_review_tab(&mut self, cx: &mut Context<Self>) {
        self.set_workspace_view_mode(WorkspaceViewMode::Diff, cx);
    }

    pub(crate) fn ai_threads_for_current_workspace(&self) -> Vec<ThreadSummary> {
        sorted_threads(&self.ai_state_snapshot)
            .into_iter()
            .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
            .collect()
    }

    fn ai_thread_summary(&self, thread_id: &str) -> Option<ThreadSummary> {
        self.ai_state_snapshot
            .threads
            .get(thread_id)
            .cloned()
            .or_else(|| {
                self.ai_workspace_states
                    .values()
                    .find_map(|state| state.state_snapshot.threads.get(thread_id).cloned())
            })
    }

    fn ai_thread_workspace_root(&self, thread_id: &str) -> Option<std::path::PathBuf> {
        self.ai_thread_summary(thread_id)
            .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
            .map(|thread| std::path::PathBuf::from(thread.cwd))
    }

    fn ai_active_thread_for_workspace_key(&self, workspace_key: &str) -> Option<String> {
        self.ai_state_snapshot
            .active_thread_for_cwd(workspace_key)
            .map(ToOwned::to_owned)
            .or_else(|| {
                self.ai_workspace_states
                    .get(workspace_key)
                    .and_then(|state| {
                        state
                            .state_snapshot
                            .active_thread_for_cwd(workspace_key)
                            .map(ToOwned::to_owned)
                    })
            })
    }

    fn ai_pending_approval(&self, request_id: &str) -> Option<AiPendingApproval> {
        self.ai_pending_approvals
            .iter()
            .find(|approval| approval.request_id == request_id)
            .cloned()
            .or_else(|| {
                self.ai_workspace_states
                    .values()
                    .find_map(|state| {
                        state
                            .pending_approvals
                            .iter()
                            .find(|approval| approval.request_id == request_id)
                            .cloned()
                    })
            })
    }

    fn ai_pending_user_input_request(
        &self,
        request_id: &str,
    ) -> Option<AiPendingUserInputRequest> {
        self.ai_pending_user_inputs
            .iter()
            .find(|request| request.request_id == request_id)
            .cloned()
            .or_else(|| {
                self.ai_workspace_states
                    .values()
                    .find_map(|state| {
                        state
                            .pending_user_inputs
                            .iter()
                            .find(|request| request.request_id == request_id)
                            .cloned()
                    })
            })
    }

    fn ai_pending_user_input_answers_mut_for_workspace(
        &mut self,
        workspace_key: Option<&str>,
    ) -> Option<&mut BTreeMap<String, BTreeMap<String, Vec<String>>>> {
        let workspace_key = workspace_key?;
        if self.ai_workspace_key().as_deref() == Some(workspace_key) {
            return Some(&mut self.ai_pending_user_input_answers);
        }
        self.ai_workspace_states
            .get_mut(workspace_key)
            .map(|state| &mut state.pending_user_input_answers)
    }

    pub(super) fn ai_visible_threads(&self) -> Vec<ThreadSummary> {
        let visible_workspace_key = self.ai_workspace_key();
        let mut threads_by_id = BTreeMap::<String, ThreadSummary>::new();

        for thread in self
            .ai_state_snapshot
            .threads
            .values()
            .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
        {
            threads_by_id.insert(thread.id.clone(), thread.clone());
        }

        for (workspace_key, state) in &self.ai_workspace_states {
            if visible_workspace_key.as_deref() == Some(workspace_key.as_str()) {
                continue;
            }
            for thread in state
                .state_snapshot
                .threads
                .values()
                .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
            {
                let replace_existing = threads_by_id
                    .get(thread.id.as_str())
                    .is_none_or(|existing| {
                        (thread.updated_at, thread.created_at, thread.id.as_str())
                            > (existing.updated_at, existing.created_at, existing.id.as_str())
                    });
                if replace_existing {
                    threads_by_id.insert(thread.id.clone(), thread.clone());
                }
            }
        }

        let mut threads = threads_by_id.into_values().collect::<Vec<_>>();
        threads.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        threads
    }

    pub(super) fn ai_timeline_turn_ids(&self, thread_id: &str) -> &[String] {
        self.ai_timeline_turn_ids_by_thread
            .get(thread_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(super) fn ai_timeline_row_ids(&self, thread_id: &str) -> &[String] {
        self.ai_timeline_row_ids_by_thread
            .get(thread_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(super) fn ai_timeline_row(&self, row_id: &str) -> Option<&AiTimelineRow> {
        self.ai_timeline_rows_by_id.get(row_id)
    }

    pub(super) fn ai_timeline_group(&self, group_id: &str) -> Option<&AiTimelineGroup> {
        self.ai_timeline_groups_by_id.get(group_id)
    }

    fn ai_timeline_container_row_id(&self, row_id: &str) -> Option<String> {
        self.ai_timeline_group_parent_by_child_row_id
            .get(row_id)
            .cloned()
            .or_else(|| self.ai_timeline_rows_by_id.contains_key(row_id).then(|| row_id.to_string()))
    }

    pub(super) fn ai_timeline_visible_rows_for_thread(
        &self,
        thread_id: &str,
    ) -> (usize, usize, usize, Vec<String>) {
        let turn_ids = self.ai_timeline_turn_ids(thread_id);
        let configured_limit = self
            .ai_timeline_visible_turn_limit_by_thread
            .get(thread_id)
            .copied()
            .unwrap_or(AI_TIMELINE_DEFAULT_VISIBLE_TURNS);
        let (total_turn_count, visible_turn_count, hidden_turn_count, visible_turn_ids) =
            timeline_visible_turn_ids(turn_ids, configured_limit);
        let row_ids = self.ai_timeline_row_ids(thread_id);
        let visible_row_ids = timeline_visible_row_ids_for_turns(
            row_ids,
            &self.ai_timeline_rows_by_id,
            visible_turn_ids.as_slice(),
        );
        (
            total_turn_count,
            visible_turn_count,
            hidden_turn_count,
            visible_row_ids,
        )
    }

    fn rebuild_ai_timeline_indexes(&mut self) {
        self.ai_timeline_turn_ids_by_thread = timeline_turn_ids_by_thread(&self.ai_state_snapshot);

        let mut base_rows_by_thread = BTreeMap::<String, Vec<(u64, String)>>::new();
        let mut rows_by_id = BTreeMap::<String, AiTimelineRow>::new();
        for (item_key, item) in &self.ai_state_snapshot.items {
            let row_id = format!("item:{item_key}");
            base_rows_by_thread
                .entry(item.thread_id.clone())
                .or_default()
                .push((item.last_sequence, row_id.clone()));
            rows_by_id.insert(
                row_id.clone(),
                AiTimelineRow {
                    id: row_id,
                    thread_id: item.thread_id.clone(),
                    turn_id: item.turn_id.clone(),
                    last_sequence: item.last_sequence,
                    source: AiTimelineRowSource::Item {
                        item_key: item_key.clone(),
                    },
                },
            );
        }

        for (turn_key, turn) in &self.ai_state_snapshot.turns {
            let Some(diff) = self.ai_state_snapshot.turn_diffs.get(turn_key.as_str()) else {
                continue;
            };
            if diff.trim().is_empty() {
                continue;
            }
            let diff_row_id = format!("turn-diff:{turn_key}");
            base_rows_by_thread
                .entry(turn.thread_id.clone())
                .or_default()
                .push((turn.last_sequence, diff_row_id.clone()));
            rows_by_id.entry(diff_row_id.clone()).or_insert(AiTimelineRow {
                id: diff_row_id,
                thread_id: turn.thread_id.clone(),
                turn_id: turn.id.clone(),
                last_sequence: turn.last_sequence,
                source: AiTimelineRowSource::TurnDiff {
                    turn_key: turn_key.clone(),
                },
            });
        }

        let base_row_ids_by_thread = base_rows_by_thread
            .into_iter()
            .map(|(thread_id, mut entries)| {
                entries.sort_by(|left, right| {
                    left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1))
                });
                entries.dedup_by(|left, right| left.1 == right.1);
                let ids = entries
                    .into_iter()
                    .map(|(_, row_id)| row_id)
                    .collect::<Vec<_>>();
                (thread_id, ids)
            })
            .collect::<BTreeMap<_, _>>();

        let mut grouped_row_ids_by_thread = BTreeMap::new();
        let mut groups_by_id = BTreeMap::new();
        let mut parent_by_child_row_id = BTreeMap::new();
        for (thread_id, row_ids) in &base_row_ids_by_thread {
            let (grouped_row_ids, groups, group_parent_by_child_row_id) =
                group_ai_timeline_rows_for_thread(
                    &self.ai_state_snapshot,
                    row_ids.as_slice(),
                    &rows_by_id,
                );
            for group in groups {
                rows_by_id.insert(
                    group.id.clone(),
                    AiTimelineRow {
                        id: group.id.clone(),
                        thread_id: group.thread_id.clone(),
                        turn_id: group.turn_id.clone(),
                        last_sequence: group.last_sequence,
                        source: AiTimelineRowSource::Group {
                            group_id: group.id.clone(),
                        },
                    },
                );
                groups_by_id.insert(group.id.clone(), group);
            }
            parent_by_child_row_id.extend(group_parent_by_child_row_id);
            grouped_row_ids_by_thread.insert(thread_id.clone(), grouped_row_ids);
        }

        self.ai_timeline_row_ids_by_thread = grouped_row_ids_by_thread;
        self.ai_timeline_rows_by_id = rows_by_id;
        self.ai_timeline_groups_by_id = groups_by_id;
        self.ai_timeline_group_parent_by_child_row_id = parent_by_child_row_id;
    }

    pub(super) fn sync_ai_timeline_list_state(&mut self, row_count: usize) {
        if self.ai_timeline_list_row_count != row_count {
            reset_ai_timeline_list_measurements(self, row_count);
        }

        if self.ai_scroll_timeline_to_bottom && row_count > 0 {
            self.scroll_ai_timeline_list_to_bottom();
            self.ai_scroll_timeline_to_bottom = false;
        }
    }

    pub(super) fn sync_ai_timeline_follow_output(
        &mut self,
        row_count: usize,
        can_refresh_from_metrics: bool,
    ) {
        if !can_refresh_from_metrics {
            if row_count == 0 {
                self.ai_timeline_follow_output = true;
            }
            return;
        }

        let scroll_offset_y = self
            .ai_timeline_list_state
            .scroll_px_offset_for_scrollbar()
            .y
            .as_f32();
        let max_scroll_offset_y = self
            .ai_timeline_list_state
            .max_offset_for_scrollbar()
            .height
            .as_f32();
        self.ai_timeline_follow_output =
            should_follow_timeline_output(row_count, scroll_offset_y, max_scroll_offset_y);
    }

    fn scroll_ai_timeline_list_to_bottom(&self) {
        let row_count = self.ai_timeline_list_state.item_count();
        if row_count == 0 {
            return;
        }
        // Use an end-of-list logical offset instead of reveal-item because reveal-item relies on
        // measured row heights; immediately after a reset, rows are unmeasured (height=0).
        self.ai_timeline_list_state.scroll_to(ListOffset {
            item_ix: row_count,
            offset_in_item: px(0.),
        });
    }

    pub(super) fn ai_visible_pending_approvals(&self) -> Vec<AiPendingApproval> {
        let visible_workspace_key = self.ai_workspace_key();
        let mut approvals_by_id = BTreeMap::<String, AiPendingApproval>::new();

        for approval in &self.ai_pending_approvals {
            approvals_by_id.insert(approval.request_id.clone(), approval.clone());
        }

        for (workspace_key, state) in &self.ai_workspace_states {
            if visible_workspace_key.as_deref() == Some(workspace_key.as_str()) {
                continue;
            }
            for approval in &state.pending_approvals {
                approvals_by_id
                    .entry(approval.request_id.clone())
                    .or_insert_with(|| approval.clone());
            }
        }

        approvals_by_id.into_values().collect()
    }

    pub(super) fn ai_visible_pending_user_inputs(&self) -> Vec<AiPendingUserInputRequest> {
        let visible_workspace_key = self.ai_workspace_key();
        let mut requests_by_id = BTreeMap::<String, AiPendingUserInputRequest>::new();

        for request in &self.ai_pending_user_inputs {
            requests_by_id.insert(request.request_id.clone(), request.clone());
        }

        for (workspace_key, state) in &self.ai_workspace_states {
            if visible_workspace_key.as_deref() == Some(workspace_key.as_str()) {
                continue;
            }
            for request in &state.pending_user_inputs {
                requests_by_id
                    .entry(request.request_id.clone())
                    .or_insert_with(|| request.clone());
            }
        }

        requests_by_id.into_values().collect()
    }

    pub(super) fn ai_load_older_turns_action(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let total_turn_count = self.ai_timeline_turn_ids(thread_id.as_str()).len();
        if total_turn_count == 0 {
            return;
        }
        let current_limit = self
            .ai_timeline_visible_turn_limit_by_thread
            .get(thread_id.as_str())
            .copied()
            .unwrap_or(AI_TIMELINE_DEFAULT_VISIBLE_TURNS.min(total_turn_count));
        if current_limit == usize::MAX {
            return;
        }
        let next_limit = current_limit
            .saturating_add(AI_TIMELINE_TURN_PAGE_SIZE)
            .min(total_turn_count);
        if next_limit == current_limit {
            return;
        }
        self.ai_timeline_visible_turn_limit_by_thread
            .insert(thread_id.clone(), next_limit);
        if self.ai_selected_thread_id.as_deref() == Some(thread_id.as_str()) {
            self.ai_text_selection = None;
            let visible_row_ids = current_ai_renderable_visible_row_ids(self, thread_id.as_str());
            reset_ai_timeline_list_measurements(self, visible_row_ids.len());
        }
        cx.notify();
    }

    pub(super) fn ai_show_full_timeline_action(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let total_turn_count = self.ai_timeline_turn_ids(thread_id.as_str()).len();
        if total_turn_count == 0 {
            return;
        }
        self.ai_timeline_visible_turn_limit_by_thread
            .insert(thread_id.clone(), usize::MAX);
        if self.ai_selected_thread_id.as_deref() == Some(thread_id.as_str()) {
            self.ai_text_selection = None;
            let visible_row_ids = current_ai_renderable_visible_row_ids(self, thread_id.as_str());
            reset_ai_timeline_list_measurements(self, visible_row_ids.len());
        }
        cx.notify();
    }

    pub(super) fn ai_select_pending_user_input_option_action(
        &mut self,
        request_id: String,
        question_id: String,
        option: String,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.ai_pending_user_input_request(request_id.as_str()) else {
            return;
        };
        let workspace_key = self
            .ai_thread_workspace_root(request.thread_id.as_str())
            .map(|root| root.to_string_lossy().to_string());

        let Some(answers_by_request) =
            self.ai_pending_user_input_answers_mut_for_workspace(workspace_key.as_deref())
        else {
            return;
        };
        let answers = answers_by_request
            .entry(request_id)
            .or_insert_with(|| normalized_user_input_answers(&request, None));
        answers.insert(question_id, vec![option]);
        cx.notify();
    }

    pub(super) fn ai_submit_pending_user_input_action(
        &mut self,
        request_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.ai_pending_user_input_request(request_id.as_str()) else {
            self.set_current_ai_composer_status("User input request no longer exists.");
            cx.notify();
            return;
        };
        let workspace_key = self
            .ai_thread_workspace_root(request.thread_id.as_str())
            .map(|root| root.to_string_lossy().to_string());

        let answers = if self.ai_workspace_key().as_deref() == workspace_key.as_deref() {
            self.ai_pending_user_input_answers
                .get(request_id.as_str())
                .cloned()
        } else {
            workspace_key
                .as_deref()
                .and_then(|workspace_key| self.ai_workspace_states.get(workspace_key))
                .and_then(|state| state.pending_user_input_answers.get(request_id.as_str()).cloned())
        }
        .unwrap_or_else(|| normalized_user_input_answers(&request, None));
        let request_thread_id = request.thread_id.clone();

        if self.send_ai_worker_command_for_workspace(
            workspace_key.as_deref(),
            AiWorkerCommand::SubmitUserInput {
                request_id: request_id.clone(),
                answers,
            },
            true,
            cx,
        ) {
            self.set_ai_composer_status_for_target(
                Some(AiComposerDraftKey::Thread(request_thread_id)),
                format!("Submitted user input for request {request_id}."),
            );
            cx.notify();
        }
    }

    pub(super) fn current_ai_thread_id(&self) -> Option<String> {
        if self.ai_new_thread_draft_active || self.ai_pending_new_thread_selection {
            return None;
        }

        if let Some(selected) = self.ai_selected_thread_id.as_ref()
            && self
                .ai_thread_summary(selected)
                .is_some_and(|thread| thread.status != ThreadLifecycleStatus::Archived)
        {
            return Some(selected.clone());
        }

        self.ai_workspace_key_for_draft().and_then(|cwd| {
            self.ai_active_thread_for_workspace_key(cwd.as_str())
                .as_deref()
                .and_then(|thread_id| {
                    self.ai_thread_summary(thread_id)
                        .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
                        .map(|_| thread_id.to_string())
                })
        })
    }

    pub(super) fn current_ai_in_progress_turn_id(&self, thread_id: &str) -> Option<String> {
        self.ai_state_snapshot
            .turns
            .values()
            .filter(|turn| turn.thread_id == thread_id && turn.status == TurnStatus::InProgress)
            .max_by_key(|turn| turn.last_sequence)
            .map(|turn| turn.id.clone())
    }

    fn ai_draft_workspace_root(&self) -> Option<std::path::PathBuf> {
        if let Some(target_id) = self.ai_draft_workspace_target_id.as_deref()
            && let Some(target) = self
                .workspace_targets
                .iter()
                .find(|target| target.id == target_id)
        {
            return Some(target.root.clone());
        }

        self.primary_repo_root()
    }

    fn resolve_ai_default_worktree_base_branch_name(&self) -> Option<String> {
        let repo_root = self.primary_repo_root()?;
        let resolved_default_base_branch = resolve_default_base_branch_name(repo_root.as_path())
            .ok()
            .flatten();
        preferred_ai_worktree_base_branch_name(
            &self.branches,
            resolved_default_base_branch.as_deref(),
            self.primary_checked_out_branch_name()
                .or(Some(self.branch_name.as_str())),
        )
    }

    fn sync_ai_worktree_base_branch_from_repo(&mut self) {
        if self.ai_new_thread_start_mode != AiNewThreadStartMode::Worktree {
            self.ai_worktree_base_branch_name = None;
            return;
        }

        let resolved_default_base_branch = self.resolve_ai_default_worktree_base_branch_name();
        self.ai_worktree_base_branch_name = preferred_ai_worktree_base_branch_name(
            &self.branches,
            self.ai_worktree_base_branch_name
                .as_deref()
                .or(resolved_default_base_branch.as_deref()),
            self.primary_checked_out_branch_name()
                .or(Some(self.branch_name.as_str())),
        );
    }

    fn ai_select_worktree_base_branch(
        &mut self,
        branch_name: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai_new_thread_start_mode != AiNewThreadStartMode::Worktree {
            return;
        }
        let branch_name = branch_name.trim().to_string();
        if branch_name.is_empty() {
            return;
        }
        self.ai_worktree_base_branch_name = Some(branch_name);
        self.sync_ai_worktree_base_branch_picker_state(cx);
        cx.notify();
    }

    pub(crate) fn ai_selected_worktree_base_branch_name(&self) -> Option<&str> {
        if self.ai_new_thread_start_mode != AiNewThreadStartMode::Worktree {
            return None;
        }

        self.ai_worktree_base_branch_name.as_deref()
    }

    fn ai_workspace_key_for_draft(&self) -> Option<String> {
        self.ai_draft_workspace_root()
            .map(|path| path.to_string_lossy().to_string())
    }

    fn ai_workspace_cwd(&self) -> Option<std::path::PathBuf> {
        if self.ai_new_thread_draft_active || self.ai_pending_new_thread_selection {
            return self.ai_draft_workspace_root();
        }

        if let Some(thread_id) = self.ai_selected_thread_id.as_deref()
            && let Some(thread_root) = self.ai_thread_workspace_root(thread_id)
        {
            return Some(thread_root);
        }

        self.ai_draft_workspace_root()
    }

    fn ai_workspace_key(&self) -> Option<String> {
        self.ai_workspace_cwd()
            .map(|cwd| cwd.to_string_lossy().to_string())
    }

    fn sync_ai_workspace_target_from_catalog(&mut self, _: &mut Context<Self>) {
        let next_target_id = self
            .ai_draft_workspace_target_id
            .clone()
            .filter(|target_id| {
                self.workspace_targets
                    .iter()
                    .any(|target| target.id == *target_id)
            })
            .or_else(|| self.primary_workspace_target_id())
            .or_else(|| self.workspace_targets.first().map(|target| target.id.clone()));
        if self.ai_draft_workspace_target_id != next_target_id {
            self.ai_draft_workspace_target_id = next_target_id;
        }
    }

    pub(crate) fn ai_active_workspace_label(&self) -> String {
        if self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_new_thread_start_mode == AiNewThreadStartMode::Worktree
        {
            return "New Worktree".to_string();
        }

        let Some(workspace_root) = self.ai_workspace_cwd() else {
            return "Primary Checkout".to_string();
        };

        self.workspace_targets
            .iter()
            .find(|target| target.root == workspace_root)
            .map(|target| target.display_name.clone())
            .or_else(|| {
                workspace_root
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| workspace_root.display().to_string())
    }

    pub(crate) fn ai_thread_workspace_label(&self, thread_id: &str) -> String {
        let Some(workspace_root) = self.ai_thread_workspace_root(thread_id) else {
            return "Unknown Workspace".to_string();
        };

        self.workspace_targets
            .iter()
            .find(|target| target.root == workspace_root)
            .map(|target| target.display_name.clone())
            .or_else(|| {
                workspace_root
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| workspace_root.display().to_string())
    }

    pub(crate) fn ai_thread_start_mode(
        &self,
        thread_id: &str,
    ) -> Option<AiNewThreadStartMode> {
        let thread = self.ai_thread_summary(thread_id)?;
        ai_thread_start_mode_for_workspace(
            self.repo_root.as_deref(),
            &self.workspace_targets,
            std::path::Path::new(thread.cwd.as_str()),
        )
    }

    pub(crate) fn ai_thread_mode_picker_state(
        &self,
        selected_thread_start_mode: Option<AiNewThreadStartMode>,
    ) -> (AiNewThreadStartMode, bool) {
        resolved_ai_thread_mode_picker_state(
            selected_thread_start_mode,
            self.ai_new_thread_start_mode,
            self.ai_new_thread_draft_active,
            self.ai_pending_new_thread_selection,
        )
    }

    pub(crate) fn ai_active_workspace_branch_name(&self) -> String {
        if self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_new_thread_start_mode == AiNewThreadStartMode::Worktree
        {
            return self
                .ai_selected_worktree_base_branch_name()
                .or_else(|| self.primary_checked_out_branch_name())
                .unwrap_or(self.branch_name.as_str())
                .to_string();
        }

        let Some(workspace_root) = self.ai_workspace_cwd() else {
            return self
                .primary_checked_out_branch_name()
                .unwrap_or(self.branch_name.as_str())
                .to_string();
        };

        self.workspace_targets
            .iter()
            .find(|target| target.root == workspace_root)
            .map(|target| target.branch_name.clone())
            .unwrap_or_else(|| {
                self.primary_checked_out_branch_name()
                    .unwrap_or(self.branch_name.as_str())
                    .to_string()
            })
    }

    pub(super) fn ai_sync_workspace_preferences(&mut self, cx: &mut Context<Self>) {
        let previous_mad_max = self.ai_mad_max_mode;
        let previous_include_hidden = self.ai_include_hidden_models;
        self.sync_ai_workspace_preferences_from_state();
        if previous_mad_max != self.ai_mad_max_mode {
            self.send_ai_worker_command_if_running(
                AiWorkerCommand::SetMadMaxMode {
                    enabled: self.ai_mad_max_mode,
                },
                cx,
            );
        }
        if previous_include_hidden != self.ai_include_hidden_models {
            self.send_ai_worker_command_if_running(
                AiWorkerCommand::SetIncludeHiddenModels {
                    enabled: self.ai_include_hidden_models,
                },
                cx,
            );
        }
        self.sync_ai_session_selection_from_state();
        cx.notify();
    }

    fn sync_ai_workspace_preferences_from_state(&mut self) {
        self.ai_mad_max_mode = workspace_mad_max_mode(&self.state, self.ai_workspace_key().as_deref());
        self.ai_include_hidden_models = workspace_include_hidden_models(
            &self.state,
            self.ai_workspace_key().as_deref(),
        );
    }

    fn resolve_codex_executable_path() -> std::path::PathBuf {
        std::env::var_os("HUNK_CODEX_EXECUTABLE")
            .map(std::path::PathBuf::from)
            .map(Self::resolve_windows_codex_command_path)
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|path| resolve_bundled_codex_executable_from_exe(path.as_path()))
            })
            .or({
                #[cfg(target_os = "windows")]
                {
                    resolve_windows_command_path(std::path::Path::new("codex"))
                }
                #[cfg(not(target_os = "windows"))]
                {
                    None
                }
            })
            .unwrap_or_else(|| std::path::PathBuf::from("codex"))
    }

    fn validate_codex_executable_path(path: &std::path::Path) -> Result<(), String> {
        if is_command_name_without_path(path) {
            #[cfg(target_os = "windows")]
            {
                return Err(format!(
                    "Unable to find a spawnable Codex executable for '{}'. Install Codex so that 'codex.cmd' or 'codex.exe' is on PATH, or set HUNK_CODEX_EXECUTABLE to the full launcher path.",
                    path.display()
                ));
            }
            #[cfg(not(target_os = "windows"))]
            return Ok(());
        }
        if !path.exists() {
            return Err(format!(
                "Bundled Codex executable not found at {}",
                path.display()
            ));
        }
        if !path.is_file() {
            return Err(format!(
                "Bundled Codex executable path is not a file: {}",
                path.display()
            ));
        }
        #[cfg(target_os = "windows")]
        {
            if !windows_path_is_spawnable(path) {
                return Err(format!(
                    "Codex executable is not spawnable on Windows: {}. Point HUNK_CODEX_EXECUTABLE at a real '.cmd' or '.exe' launcher, not the Unix shim.",
                    path.display()
                ));
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(path)
                .map_err(|error| format!("Unable to inspect Codex executable: {error}"))?;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(format!(
                    "Bundled Codex executable is not marked executable: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    fn resolve_windows_codex_command_path(path: std::path::PathBuf) -> std::path::PathBuf {
        #[cfg(target_os = "windows")]
        {
            resolve_windows_command_path(path.as_path()).unwrap_or(path)
        }
        #[cfg(not(target_os = "windows"))]
        {
            path
        }
    }

    fn default_ai_workspace_state_for_workspace_key(
        &self,
        workspace_key: Option<&str>,
    ) -> AiWorkspaceState {
        let mut next = AiWorkspaceState {
            include_hidden_models: workspace_include_hidden_models(&self.state, workspace_key),
            mad_max_mode: workspace_mad_max_mode(&self.state, workspace_key),
            ..AiWorkspaceState::default()
        };
        let persisted = workspace_key
            .and_then(|workspace| self.state.ai_workspace_session_overrides.get(workspace).cloned())
            .unwrap_or_default();
        next.selected_model = persisted.model;
        next.selected_effort = persisted.effort;
        next.selected_collaboration_mode = persisted.collaboration_mode;
        next.selected_service_tier = persisted.service_tier.unwrap_or_default();
        next
    }

    fn capture_current_ai_workspace_state(&self) -> AiWorkspaceState {
        AiWorkspaceState {
            connection_state: self.ai_connection_state,
            bootstrap_loading: self.ai_bootstrap_loading,
            status_message: self.ai_status_message.clone(),
            error_message: self.ai_error_message.clone(),
            state_snapshot: self.ai_state_snapshot.clone(),
            selected_thread_id: self.ai_selected_thread_id.clone(),
            new_thread_draft_active: self.ai_new_thread_draft_active,
            new_thread_start_mode: self.ai_new_thread_start_mode,
            worktree_base_branch_name: self.ai_worktree_base_branch_name.clone(),
            pending_new_thread_selection: self.ai_pending_new_thread_selection,
            timeline_follow_output: self.ai_timeline_follow_output,
            thread_title_refresh_state_by_thread: self.ai_thread_title_refresh_state_by_thread.clone(),
            timeline_visible_turn_limit_by_thread: self.ai_timeline_visible_turn_limit_by_thread.clone(),
            in_progress_turn_started_at: self.ai_in_progress_turn_started_at.clone(),
            expanded_timeline_row_ids: self.ai_expanded_timeline_row_ids.clone(),
            pending_approvals: self.ai_pending_approvals.clone(),
            pending_user_inputs: self.ai_pending_user_inputs.clone(),
            pending_user_input_answers: self.ai_pending_user_input_answers.clone(),
            account: self.ai_account.clone(),
            requires_openai_auth: self.ai_requires_openai_auth,
            pending_chatgpt_login_id: self.ai_pending_chatgpt_login_id.clone(),
            pending_chatgpt_auth_url: self.ai_pending_chatgpt_auth_url.clone(),
            rate_limits: self.ai_rate_limits.clone(),
            models: self.ai_models.clone(),
            experimental_features: self.ai_experimental_features.clone(),
            collaboration_modes: self.ai_collaboration_modes.clone(),
            include_hidden_models: self.ai_include_hidden_models,
            selected_model: self.ai_selected_model.clone(),
            selected_effort: self.ai_selected_effort.clone(),
            selected_collaboration_mode: self.ai_selected_collaboration_mode,
            selected_service_tier: self.ai_selected_service_tier,
            mad_max_mode: self.ai_mad_max_mode,
        }
    }

    fn apply_ai_workspace_state(&mut self, state: AiWorkspaceState) {
        self.ai_connection_state = state.connection_state;
        self.ai_bootstrap_loading = state.bootstrap_loading;
        self.ai_status_message = state.status_message;
        self.ai_error_message = state.error_message;
        self.ai_state_snapshot = state.state_snapshot;
        self.ai_selected_thread_id = state.selected_thread_id;
        self.ai_new_thread_draft_active = state.new_thread_draft_active;
        self.ai_new_thread_start_mode = state.new_thread_start_mode;
        self.ai_worktree_base_branch_name = state.worktree_base_branch_name;
        self.ai_pending_new_thread_selection = state.pending_new_thread_selection;
        self.ai_scroll_timeline_to_bottom = false;
        self.ai_timeline_follow_output = state.timeline_follow_output;
        self.ai_thread_inline_toast = None;
        self.ai_thread_title_refresh_state_by_thread = state.thread_title_refresh_state_by_thread;
        self.ai_timeline_visible_turn_limit_by_thread = state.timeline_visible_turn_limit_by_thread;
        self.ai_in_progress_turn_started_at = state.in_progress_turn_started_at;
        self.ai_expanded_timeline_row_ids = state.expanded_timeline_row_ids;
        self.ai_pending_approvals = state.pending_approvals;
        self.ai_pending_user_inputs = state.pending_user_inputs;
        self.ai_pending_user_input_answers = state.pending_user_input_answers;
        self.ai_account = state.account;
        self.ai_requires_openai_auth = state.requires_openai_auth;
        self.ai_pending_chatgpt_login_id = state.pending_chatgpt_login_id;
        self.ai_pending_chatgpt_auth_url = state.pending_chatgpt_auth_url;
        self.ai_rate_limits = state.rate_limits;
        self.ai_models = state.models;
        self.ai_experimental_features = state.experimental_features;
        self.ai_collaboration_modes = state.collaboration_modes;
        self.ai_include_hidden_models = state.include_hidden_models;
        self.ai_selected_model = state.selected_model;
        self.ai_selected_effort = state.selected_effort;
        self.ai_selected_collaboration_mode = state.selected_collaboration_mode;
        self.ai_selected_service_tier = state.selected_service_tier;
        self.ai_mad_max_mode = state.mad_max_mode;
        self.ai_text_selection = None;
        self.rebuild_ai_timeline_indexes();
        self.sync_ai_in_progress_turn_started_at();
        self.ai_composer_activity_elapsed_second = self.current_ai_composer_activity_elapsed_second();
        self.ai_thread_title_refresh_state_by_thread
            .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
        self.ai_timeline_visible_turn_limit_by_thread
            .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
        self.sync_ai_pending_user_input_answers();
        self.ai_expanded_timeline_row_ids
            .retain(|row_id| self.ai_timeline_rows_by_id.contains_key(row_id));
        if self.ai_selected_thread_id.as_ref().is_some_and(|selected| {
            self.ai_state_snapshot
                .threads
                .get(selected)
                .is_none_or(|thread| thread.status == ThreadLifecycleStatus::Archived)
        }) {
            self.ai_selected_thread_id = None;
        }
        if !self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_selected_thread_id.is_none()
        {
            self.ai_selected_thread_id = self.current_ai_thread_id();
        }
        if !self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_selected_thread_id.is_none()
            && let Some(first_thread) = self.ai_threads_for_current_workspace().first()
        {
            self.ai_selected_thread_id = Some(first_thread.id.clone());
        }
        self.prune_ai_composer_drafts();
        self.prune_ai_composer_statuses();
        reset_ai_timeline_list_measurements(self, 0);
    }

    fn store_current_ai_workspace_state(&mut self, workspace_key: Option<&str>) {
        let Some(workspace_key) = workspace_key else {
            return;
        };
        self.ai_workspace_states.insert(
            workspace_key.to_string(),
            self.capture_current_ai_workspace_state(),
        );
    }

    fn restore_ai_workspace_state_for_key(&mut self, workspace_key: Option<&str>) {
        let state = workspace_key
            .and_then(|key| self.ai_workspace_states.get(key).cloned())
            .unwrap_or_else(|| self.default_ai_workspace_state_for_workspace_key(workspace_key));
        self.apply_ai_workspace_state(state);
    }

    fn park_visible_ai_runtime(&mut self) {
        let Some(workspace_key) = self.ai_worker_workspace_key.clone() else {
            return;
        };
        let Some(command_tx) = self.ai_command_tx.take() else {
            self.ai_worker_workspace_key = None;
            return;
        };
        let Some(worker_thread) = self.ai_worker_thread.take() else {
            self.ai_worker_workspace_key = None;
            return;
        };
        let event_task = std::mem::replace(&mut self.ai_event_task, Task::ready(()));
        self.ai_hidden_runtimes.insert(
            workspace_key,
            AiHiddenRuntimeHandle {
                command_tx,
                worker_thread,
                event_task,
                generation: self.ai_event_epoch,
            },
        );
        self.ai_worker_workspace_key = None;
    }

    fn promote_hidden_ai_runtime(&mut self, workspace_key: &str) -> bool {
        let Some(handle) = self.ai_hidden_runtimes.remove(workspace_key) else {
            return false;
        };
        if handle.worker_thread.is_finished() {
            if let Err(error) = handle.worker_thread.join() {
                error!(
                    "failed to join completed hidden AI worker thread while promoting {workspace_key}: {error:?}"
                );
            }
            return false;
        }
        self.ai_command_tx = Some(handle.command_tx);
        self.ai_worker_thread = Some(handle.worker_thread);
        self.ai_event_task = handle.event_task;
        self.ai_event_epoch = handle.generation;
        self.ai_worker_workspace_key = Some(workspace_key.to_string());
        true
    }

    fn ai_runtime_listener_generation(&self, workspace_key: &str) -> Option<usize> {
        if self.ai_worker_workspace_key.as_deref() == Some(workspace_key) {
            return Some(self.ai_event_epoch);
        }
        self.ai_hidden_runtimes
            .get(workspace_key)
            .map(|handle| handle.generation)
    }

    fn ai_runtime_listener_is_current(&self, workspace_key: &str, generation: usize) -> bool {
        self.ai_runtime_listener_generation(workspace_key) == Some(generation)
    }

    fn update_background_ai_workspace_state<F>(&mut self, workspace_key: &str, update: F)
    where
        F: FnOnce(&mut AiWorkspaceState),
    {
        let default_state = self.default_ai_workspace_state_for_workspace_key(Some(workspace_key));
        let state = self
            .ai_workspace_states
            .entry(workspace_key.to_string())
            .or_insert(default_state);
        update(state);
    }

    fn apply_ai_snapshot_to_workspace_state(state: &mut AiWorkspaceState, snapshot: AiSnapshot) {
        let AiSnapshot {
            state: next_snapshot,
            active_thread_id,
            pending_approvals,
            pending_user_inputs,
            account,
            requires_openai_auth,
            pending_chatgpt_login_id,
            pending_chatgpt_auth_url,
            rate_limits,
            models,
            experimental_features,
            collaboration_modes,
            include_hidden_models,
            mad_max_mode,
        } = snapshot;

        state.state_snapshot = next_snapshot;
        state.pending_approvals = pending_approvals;
        state.pending_user_inputs = pending_user_inputs;
        state.account = account;
        state.requires_openai_auth = requires_openai_auth;
        state.pending_chatgpt_login_id = pending_chatgpt_login_id;
        state.pending_chatgpt_auth_url = pending_chatgpt_auth_url;
        state.rate_limits = rate_limits;
        state.models = models;
        state.experimental_features = experimental_features;
        state.collaboration_modes = collaboration_modes;
        state.include_hidden_models = include_hidden_models;
        state.mad_max_mode = mad_max_mode;
        state.connection_state = AiConnectionState::Ready;
        state.error_message = None;

        if state.pending_new_thread_selection
            && let Some(active_thread_id) = active_thread_id.as_deref()
            && state
                .state_snapshot
                .threads
                .get(active_thread_id)
                .is_some_and(|thread| thread.status != ThreadLifecycleStatus::Archived)
        {
            state.new_thread_draft_active = false;
            state.pending_new_thread_selection = false;
            state.selected_thread_id = Some(active_thread_id.to_string());
        }

        if state.selected_thread_id.as_ref().is_some_and(|selected| {
            state
                .state_snapshot
                .threads
                .get(selected)
                .is_none_or(|thread| thread.status == ThreadLifecycleStatus::Archived)
        }) {
            state.selected_thread_id = None;
        }

        if !state.new_thread_draft_active
            && !state.pending_new_thread_selection
            && state.selected_thread_id.is_none()
        {
            state.selected_thread_id = active_thread_id;
        }

        if !state.new_thread_draft_active
            && !state.pending_new_thread_selection
            && state.selected_thread_id.is_none()
            && let Some(first_thread) = sorted_threads(&state.state_snapshot).first()
        {
            state.selected_thread_id = Some(first_thread.id.clone());
        }

        state
            .thread_title_refresh_state_by_thread
            .retain(|thread_id, _| state.state_snapshot.threads.contains_key(thread_id));
        state
            .timeline_visible_turn_limit_by_thread
            .retain(|thread_id, _| state.state_snapshot.threads.contains_key(thread_id));
    }

    fn restore_ai_workspace_state_after_failure_for_state(state: &mut AiWorkspaceState) {
        if state.pending_new_thread_selection {
            state.new_thread_draft_active = true;
        }
        state.pending_new_thread_selection = false;
    }

    fn handle_background_ai_worker_event(
        &mut self,
        workspace_key: &str,
        event: AiWorkerEventPayload,
    ) {
        self.update_background_ai_workspace_state(workspace_key, |state| match event {
            AiWorkerEventPayload::Snapshot(snapshot) => {
                Self::apply_ai_snapshot_to_workspace_state(state, *snapshot);
            }
            AiWorkerEventPayload::BootstrapCompleted => {
                state.bootstrap_loading = false;
            }
            AiWorkerEventPayload::Reconnecting(message) => {
                state.connection_state = AiConnectionState::Reconnecting;
                state.bootstrap_loading = false;
                state.error_message = None;
                state.status_message = Some(message);
            }
            AiWorkerEventPayload::Status(message) => {
                state.status_message = Some(message);
            }
            AiWorkerEventPayload::Error(message) => {
                Self::restore_ai_workspace_state_after_failure_for_state(state);
                state.error_message = Some(message.clone());
                state.status_message = Some(message);
            }
            AiWorkerEventPayload::Fatal(message) => {
                state.connection_state = AiConnectionState::Failed;
                state.bootstrap_loading = false;
                state.status_message = Some("Codex integration failed".to_string());
                state.error_message = Some(message);
                state.account = None;
                state.requires_openai_auth = false;
                state.pending_chatgpt_login_id = None;
                state.pending_chatgpt_auth_url = None;
                state.rate_limits = None;
                state.models.clear();
                state.experimental_features.clear();
                state.collaboration_modes.clear();
                state.pending_approvals.clear();
                state.pending_user_inputs.clear();
                state.pending_user_input_answers.clear();
                Self::restore_ai_workspace_state_after_failure_for_state(state);
            }
        });
    }

    fn handle_background_ai_worker_disconnect(&mut self, workspace_key: &str) {
        if let Some(hidden) = self.ai_hidden_runtimes.remove(workspace_key) {
            let AiHiddenRuntimeHandle { worker_thread, .. } = hidden;
            let workspace_key = workspace_key.to_string();
            std::thread::spawn(move || {
                if let Err(error) = worker_thread.join() {
                    error!(
                        "failed to join hidden AI worker thread during disconnect for {workspace_key}: {error:?}"
                    );
                }
            });
        }
        self.update_background_ai_workspace_state(workspace_key, |state| {
            state.connection_state = AiConnectionState::Failed;
            state.bootstrap_loading = false;
            if state.error_message.is_none() {
                let message = "Codex worker disconnected.".to_string();
                state.error_message = Some(message.clone());
                state.status_message = Some("Codex integration failed".to_string());
            }
            state.account = None;
            state.requires_openai_auth = false;
            state.pending_chatgpt_login_id = None;
            state.pending_chatgpt_auth_url = None;
            state.rate_limits = None;
            state.models.clear();
            state.experimental_features.clear();
            state.collaboration_modes.clear();
            state.pending_approvals.clear();
            state.pending_user_inputs.clear();
            state.pending_user_input_answers.clear();
            Self::restore_ai_workspace_state_after_failure_for_state(state);
        });
    }

    pub(super) fn shutdown_ai_worker_blocking(&mut self) {
        if let Some(command_tx) = self.ai_command_tx.take() {
            let _ = command_tx.send(AiWorkerCommand::Shutdown);
        }
        self.ai_worker_workspace_key = None;
        self.join_ai_worker_thread("dropping DiffViewer");
        for (_, hidden) in std::mem::take(&mut self.ai_hidden_runtimes) {
            let _ = hidden.command_tx.send(AiWorkerCommand::Shutdown);
            if let Err(error) = hidden.worker_thread.join() {
                error!("failed to join hidden AI worker thread during shutdown: {error:?}");
            }
        }
    }

    fn join_ai_worker_thread_if_finished(&mut self, reason: &str) {
        let Some(worker) = self.ai_worker_thread.take() else {
            return;
        };
        if !worker.is_finished() {
            self.ai_worker_thread = Some(worker);
            return;
        }
        if let Err(error) = worker.join() {
            error!("failed to join completed AI worker thread during {reason}: {error:?}");
        }
    }

    fn join_ai_worker_thread(&mut self, reason: &str) {
        let Some(worker) = self.ai_worker_thread.take() else {
            return;
        };
        if let Err(error) = worker.join() {
            error!("failed to join AI worker thread during {reason}: {error:?}");
        }
    }

    fn send_ai_worker_command(&mut self, command: AiWorkerCommand, cx: &mut Context<Self>) -> bool {
        let workspace_key = self.ai_workspace_key();
        self.send_ai_worker_command_for_workspace(workspace_key.as_deref(), command, true, cx)
    }

    fn send_ai_worker_command_if_running(
        &mut self,
        command: AiWorkerCommand,
        cx: &mut Context<Self>,
    ) -> bool {
        let workspace_key = self.ai_workspace_key();
        self.send_ai_worker_command_for_workspace(workspace_key.as_deref(), command, false, cx)
    }

    fn send_ai_worker_command_for_workspace(
        &mut self,
        workspace_key: Option<&str>,
        command: AiWorkerCommand,
        ensure_running: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let current_workspace_key = self.ai_workspace_key();
        let Some(workspace_key) = workspace_key.or(current_workspace_key.as_deref()) else {
            return false;
        };

        if self.ai_worker_workspace_key.as_deref() == Some(workspace_key) {
            if ensure_running && self.ai_command_tx.is_none() {
                self.ensure_ai_runtime_started(cx);
            }

            let Some(command_tx) = self.ai_command_tx.as_ref() else {
                return false;
            };
            if command_tx.send(command).is_ok() {
                return true;
            }

            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_bootstrap_loading = false;
            self.ai_error_message = Some("AI worker channel disconnected.".to_string());
            self.ai_command_tx = None;
            self.ai_worker_workspace_key = None;
            self.join_ai_worker_thread("worker channel disconnect");
            cx.notify();
            return false;
        }

        if let Some(command_tx) = self
            .ai_hidden_runtimes
            .get(workspace_key)
            .map(|runtime| runtime.command_tx.clone())
        {
            if command_tx.send(command).is_ok() {
                return true;
            }

            self.handle_background_ai_worker_disconnect(workspace_key);
            cx.notify();
            return false;
        }

        if ensure_running && current_workspace_key.as_deref() == Some(workspace_key) {
            self.ensure_ai_runtime_started(cx);
            if self.ai_worker_workspace_key.as_deref() == Some(workspace_key) {
                return self.send_ai_worker_command_for_workspace(
                    Some(workspace_key),
                    command,
                    false,
                    cx,
                );
            }
        }

        false
    }

    fn next_ai_event_epoch(&mut self) -> usize {
        self.ai_event_epoch = self.ai_event_epoch.saturating_add(1);
        self.ai_event_epoch
    }

    fn ai_add_composer_local_images<I>(&mut self, paths: I) -> usize
    where
        I: IntoIterator<Item = std::path::PathBuf>,
    {
        let mut added = 0;
        let Some(draft) = self.current_ai_composer_draft_mut() else {
            return 0;
        };

        for path in paths {
            let normalized = std::fs::canonicalize(path.as_path()).unwrap_or(path);
            if !normalized.is_file() || !is_supported_ai_image_path(normalized.as_path()) {
                continue;
            }
            if draft.local_images.iter().any(|existing| existing == &normalized) {
                continue;
            }
            draft.local_images.push(normalized);
            added += 1;
        }

        added
    }
}

fn ai_in_progress_turn_tracking_key(thread_id: &str, turn_id: &str) -> String {
    format!("{thread_id}::{turn_id}")
}

fn is_supported_ai_image_path(path: &std::path::Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tif" | "tiff"
    )
}

fn ai_attachment_status_message(file_count: usize, added_count: usize) -> Option<String> {
    if file_count == 0 || added_count == file_count {
        return None;
    }

    if added_count == 0 {
        if file_count == 1 {
            return Some("File is not a supported image or is already attached.".to_string());
        }
        return Some("No files were supported images or were already attached.".to_string());
    }

    let added_suffix = if added_count == 1 { "" } else { "s" };
    let skipped_count = file_count.saturating_sub(added_count);
    let skipped_suffix = if skipped_count == 1 { "" } else { "s" };
    Some(format!(
        "Attached {added_count} image{added_suffix}. Skipped {skipped_count} unsupported or duplicate file{skipped_suffix}."
    ))
}
