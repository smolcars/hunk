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
        if self.ai_command_tx.is_some() {
            return;
        }
        self.join_ai_worker_thread_if_finished("starting AI runtime");

        self.sync_ai_workspace_preferences_from_state();

        let Some(cwd) = self.ai_workspace_cwd() else {
            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_bootstrap_loading = false;
            self.ai_error_message = Some("Open a workspace before using AI.".to_string());
            cx.notify();
            return;
        };
        let worker_workspace_key = cwd.to_string_lossy().to_string();

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
        self.ai_command_tx = Some(command_tx);
        self.ai_worker_thread = Some(worker);
        self.ai_worker_workspace_key = Some(worker_workspace_key);

        let epoch = self.next_ai_event_epoch();
        self.start_ai_event_listener(event_rx, epoch, cx);
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
        let prompt = self.ai_composer_input_state.read(cx).value().trim().to_string();
        let prompt = (!prompt.is_empty()).then_some(prompt);
        let local_image_paths = self.current_ai_composer_local_images();
        if !local_image_paths.is_empty() && !self.current_ai_model_supports_image_inputs() {
            self.ai_status_message = Some(
                "Selected model does not support image attachments. Remove attachments or switch models."
                    .to_string(),
            );
            cx.notify();
            return;
        }

        let session_overrides = self.current_ai_turn_session_overrides();
        if self.send_ai_worker_command(
            AiWorkerCommand::StartThread {
                prompt,
                local_image_paths,
                session_overrides,
            },
            cx,
        ) {
            self.ai_status_message = None;
            self.clear_ai_composer_input(window, cx);
        }
        self.focus_ai_composer_input(window, cx);
    }

    pub(super) fn ai_new_thread_action(
        &mut self,
        _: &AiNewThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.set_workspace_view_mode(WorkspaceSwitchAction::Ai.target_mode(), cx);
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
                            this.ai_status_message =
                                Some(format!("Failed to open image picker: {err:#}"));
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
                        this.ai_status_message = Some(message);
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
            self.ai_status_message = Some(
                "Selected model does not support image attachments. Remove attachments or switch models."
                    .to_string(),
            );
            cx.notify();
            return;
        }

        let dropped_count = dropped_paths.len();
        let added = self.ai_add_composer_local_images(dropped_paths);
        if let Some(message) = ai_attachment_status_message(dropped_count, added) {
            self.ai_status_message = Some(message);
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
            self.ai_status_message = Some("Select a thread before starting review.".to_string());
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
            self.ai_status_message = None;
            self.clear_ai_composer_input(window, cx);
        }
    }

    pub(super) fn ai_interrupt_turn_action(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_ai_thread_id() else {
            self.ai_status_message = Some("Select a thread before interrupting a turn.".to_string());
            cx.notify();
            return;
        };

        let Some(turn_id) = self.current_ai_in_progress_turn_id(thread_id.as_str()) else {
            self.ai_status_message = Some("No in-progress turn to interrupt.".to_string());
            cx.notify();
            return;
        };

        if self.send_ai_worker_command(
            AiWorkerCommand::InterruptTurn { thread_id, turn_id },
            cx,
        ) {
            self.ai_status_message = Some("Interrupted".to_string());
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
        if self.send_ai_worker_command(
            AiWorkerCommand::ResolveApproval {
                request_id,
                decision,
            },
            cx,
        ) {
            self.ai_status_message = Some(match decision {
                AiApprovalDecision::Accept => "Approval accepted.".to_string(),
                AiApprovalDecision::Decline => "Approval declined.".to_string(),
            });
            cx.notify();
        }
    }

    pub(super) fn ai_select_thread(
        &mut self,
        thread_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_draft_key = self.current_ai_composer_draft_key();
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = true;
        self.ai_expanded_timeline_row_ids.clear();
        self.ai_text_selection = None;
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

    pub(super) fn ai_visible_threads(&self) -> Vec<ThreadSummary> {
        sorted_threads(&self.ai_state_snapshot)
            .into_iter()
            .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
            .collect()
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
        self.ai_pending_approvals.clone()
    }

    pub(super) fn ai_visible_pending_user_inputs(&self) -> Vec<AiPendingUserInputRequest> {
        self.ai_pending_user_inputs.clone()
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
        let Some(request) = self
            .ai_pending_user_inputs
            .iter()
            .find(|request| request.request_id == request_id)
        else {
            return;
        };

        let answers = self
            .ai_pending_user_input_answers
            .entry(request_id)
            .or_insert_with(|| normalized_user_input_answers(request, None));
        answers.insert(question_id, vec![option]);
        cx.notify();
    }

    pub(super) fn ai_submit_pending_user_input_action(
        &mut self,
        request_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self
            .ai_pending_user_inputs
            .iter()
            .find(|request| request.request_id == request_id)
        else {
            self.ai_status_message = Some("User input request no longer exists.".to_string());
            cx.notify();
            return;
        };

        let answers = self
            .ai_pending_user_input_answers
            .get(request_id.as_str())
            .cloned()
            .unwrap_or_else(|| normalized_user_input_answers(request, None));

        if self.send_ai_worker_command(
            AiWorkerCommand::SubmitUserInput {
                request_id: request_id.clone(),
                answers,
            },
            cx,
        ) {
            self.ai_status_message = Some(format!("Submitted user input for request {request_id}."));
            cx.notify();
        }
    }

    pub(super) fn current_ai_thread_id(&self) -> Option<String> {
        if let Some(selected) = self.ai_selected_thread_id.as_ref()
            && self
                .ai_state_snapshot
                .threads
                .get(selected)
                .is_some_and(|thread| thread.status != ThreadLifecycleStatus::Archived)
        {
            return Some(selected.clone());
        }

        self.ai_workspace_key().and_then(|cwd| {
            self.ai_state_snapshot
                .active_thread_for_cwd(cwd.as_str())
                .and_then(|thread_id| {
                    self.ai_state_snapshot
                        .threads
                        .get(thread_id)
                        .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
                        .map(|_| thread_id)
                })
                .map(ToOwned::to_owned)
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

    fn ai_workspace_cwd(&self) -> Option<std::path::PathBuf> {
        resolved_ai_workspace_cwd(self.project_path.as_deref(), self.repo_root.as_deref())
    }

    fn ai_workspace_key(&self) -> Option<String> {
        self.ai_workspace_cwd()
            .map(|cwd| cwd.to_string_lossy().to_string())
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

    pub(super) fn shutdown_ai_worker_blocking(&mut self) {
        if let Some(command_tx) = self.ai_command_tx.take() {
            let _ = command_tx.send(AiWorkerCommand::Shutdown);
        }
        self.ai_worker_workspace_key = None;
        self.join_ai_worker_thread("dropping DiffViewer");
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

    fn detach_ai_worker_thread_join(&mut self, reason: &'static str) {
        let Some(worker) = self.ai_worker_thread.take() else {
            return;
        };

        std::thread::spawn(move || {
            if let Err(error) = worker.join() {
                error!("failed to join AI worker thread during {reason}: {error:?}");
            }
        });
    }

    fn send_ai_worker_command(&mut self, command: AiWorkerCommand, cx: &mut Context<Self>) -> bool {
        if self.ai_command_tx.is_none() {
            self.ensure_ai_runtime_started(cx);
        }

        self.send_ai_worker_command_if_running(command, cx)
    }

    fn send_ai_worker_command_if_running(
        &mut self,
        command: AiWorkerCommand,
        cx: &mut Context<Self>,
    ) -> bool {
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
