fn ai_timeline_row_supports_inline_review(
    row: &AiTimelineRow,
    item_kind: Option<&str>,
    group_kind: Option<&str>,
) -> bool {
    match &row.source {
        AiTimelineRowSource::TurnDiff { .. } => true,
        AiTimelineRowSource::Item { .. } => item_kind == Some("fileChange"),
        AiTimelineRowSource::Group { .. } => group_kind == Some("file_change_batch"),
        AiTimelineRowSource::TurnPlan { .. } => false,
    }
}

fn ai_historical_turn_diff_key_for_row(row: &AiTimelineRow) -> String {
    hunk_codex::state::turn_storage_key(row.thread_id.as_str(), row.turn_id.as_str())
}

fn ai_latest_supported_inline_review_row_id_for_visible_rows<F>(
    visible_row_ids: &[String],
    rows_by_id: &std::collections::BTreeMap<String, AiTimelineRow>,
    mut supports_inline_review: F,
) -> Option<String>
where
    F: FnMut(&AiTimelineRow) -> bool,
{
    visible_row_ids
        .iter()
        .rev()
        .find_map(|row_id| {
            rows_by_id
                .get(row_id)
                .filter(|row| supports_inline_review(row))
                .map(|row| row.id.clone())
        })
}

fn ai_resolved_inline_review_row_id_for_visible_rows<F>(
    selected_row_id: Option<&str>,
    visible_row_ids: &[String],
    rows_by_id: &std::collections::BTreeMap<String, AiTimelineRow>,
    mut supports_inline_review: F,
) -> Option<String>
where
    F: FnMut(&AiTimelineRow) -> bool,
{
    selected_row_id
        .and_then(|row_id| rows_by_id.get(row_id))
        .filter(|row| supports_inline_review(row))
        .map(|row| row.id.clone())
        .or_else(|| {
            ai_latest_supported_inline_review_row_id_for_visible_rows(
                visible_row_ids,
                rows_by_id,
                supports_inline_review,
            )
        })
}

fn ai_inline_review_toggle_target_mode(
    is_open: bool,
    current_mode: AiInlineReviewMode,
    requested_mode: AiInlineReviewMode,
) -> Option<AiInlineReviewMode> {
    if is_open && current_mode == requested_mode {
        None
    } else {
        Some(requested_mode)
    }
}

fn ai_inline_review_uses_review_compare_session_for_surface(
    workspace_view_mode: WorkspaceViewMode,
    is_open: bool,
    current_mode: AiInlineReviewMode,
) -> bool {
    workspace_view_mode == WorkspaceViewMode::Ai
        && is_open
        && current_mode == AiInlineReviewMode::WorkingTree
}

fn ai_historical_inline_review_loaded_state(
    thread_id: &str,
    row_id: &str,
    row_last_sequence: u64,
    turn_diff_last_sequence: Option<u64>,
) -> AiInlineReviewLoadedState {
    AiInlineReviewLoadedState {
        thread_id: thread_id.to_string(),
        row_id: row_id.to_string(),
        row_last_sequence,
        turn_diff_last_sequence,
        mode: AiInlineReviewMode::Historical,
    }
}

fn normalize_ai_browser_address(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("about:") || lower.starts_with("data:") {
        return Some(trimmed.to_string());
    }

    let scheme = if lower.starts_with("localhost") || lower.starts_with("127.") {
        "http://"
    } else {
        "https://"
    };
    Some(format!("{scheme}{trimmed}"))
}

impl DiffViewer {
    fn ai_workspace_message_block_config(
        item_kind: &str,
    ) -> Option<(ai_workspace_session::AiWorkspaceBlockRole, &'static str)> {
        match item_kind {
            "userMessage" => Some((ai_workspace_session::AiWorkspaceBlockRole::User, "You")),
            "agentMessage" => Some((
                ai_workspace_session::AiWorkspaceBlockRole::Assistant,
                "Assistant",
            )),
            "plan" => Some((
                ai_workspace_session::AiWorkspaceBlockRole::Assistant,
                "Proposed Plan",
            )),
            _ => None,
        }
    }

    pub(super) fn ai_inline_review_mode_for_thread(&self, thread_id: &str) -> AiInlineReviewMode {
        self.ai_inline_review_mode_by_thread
            .get(thread_id)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn current_ai_inline_review_mode(&self) -> AiInlineReviewMode {
        self.ai_selected_thread_id
            .as_deref()
            .map(|thread_id| self.ai_inline_review_mode_for_thread(thread_id))
            .unwrap_or_default()
    }

    pub(super) fn ai_inline_review_uses_review_compare_session(&self) -> bool {
        ai_inline_review_uses_review_compare_session_for_surface(
            self.workspace_view_mode,
            self.ai_inline_review_is_open(),
            self.current_ai_inline_review_mode(),
        )
    }

    fn ai_clear_inline_review_loaded_state(&mut self) {
        self.ai_inline_review_session = None;
        self.ai_inline_review_loaded_state = None;
        self.ai_inline_review_error = None;
        self.ai_inline_review_status_message = None;
    }

    fn ai_resolve_inline_review_row_id_for_thread(
        &self,
        thread_id: &str,
    ) -> Option<String> {
        let visible_row_ids = self.ai_timeline_visible_rows_for_thread(thread_id).3;
        ai_resolved_inline_review_row_id_for_visible_rows(
            self.current_ai_inline_review_row_id_for_thread(thread_id),
            visible_row_ids.as_slice(),
            &self.ai_timeline_rows_by_id,
            |row| self.ai_row_supports_inline_review(row),
        )
    }

    pub(super) fn ai_can_open_inline_review_for_current_thread(&self) -> bool {
        self.current_ai_thread_id()
            .as_deref()
            .and_then(|thread_id| self.ai_resolve_inline_review_row_id_for_thread(thread_id))
            .is_some()
    }

    pub(super) fn ai_sync_historical_inline_review_session_if_needed(&mut self) {
        if self.workspace_view_mode != WorkspaceViewMode::Ai
            || !self.ai_inline_review_is_open()
            || self.current_ai_inline_review_mode() != AiInlineReviewMode::Historical
        {
            return;
        }

        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            self.ai_clear_inline_review_loaded_state();
            return;
        };
        let Some(row_id) = self
            .current_ai_inline_review_row_id_for_thread(thread_id.as_str())
            .map(str::to_string)
        else {
            self.ai_clear_inline_review_loaded_state();
            return;
        };
        let Some(row) = self.ai_timeline_row(row_id.as_str()).cloned() else {
            self.ai_clear_inline_review_loaded_state();
            return;
        };

        let turn_key = ai_historical_turn_diff_key_for_row(&row);
        let turn_diff_last_sequence = self.ai_state_snapshot.turn_diff_sequence(turn_key.as_str());
        let next_loaded_state = ai_historical_inline_review_loaded_state(
            thread_id.as_str(),
            row_id.as_str(),
            row.last_sequence,
            turn_diff_last_sequence,
        );
        if self.ai_inline_review_loaded_state.as_ref() == Some(&next_loaded_state) {
            return;
        }

        let preserve_scroll = self
            .ai_inline_review_loaded_state
            .as_ref()
            .is_some_and(|loaded| {
                loaded.thread_id == next_loaded_state.thread_id
                    && loaded.row_id == next_loaded_state.row_id
                    && loaded.mode == next_loaded_state.mode
            });
        if preserve_scroll {
            self.ai_inline_review_surface.invalidate_geometry();
        } else {
            self.ai_inline_review_surface.clear_runtime_state();
        }

        let Some(diff) = self.ai_state_snapshot.turn_diffs.get(turn_key.as_str()) else {
            self.ai_inline_review_session = None;
            self.ai_inline_review_loaded_state = Some(next_loaded_state);
            self.ai_inline_review_error = None;
            self.ai_inline_review_status_message = Some(
                "Historical AI diff is not loaded for this turn yet. This can happen after restart or before the thread has replayed its patch snapshot. Try Working Tree for the current repo state."
                    .to_string(),
            );
            return;
        };

        let snapshot = crate::app::ai_inline_review_snapshot::compare_snapshot_from_turn_diff(diff);
        if snapshot.files.is_empty() {
            self.ai_inline_review_session = None;
            self.ai_inline_review_loaded_state = Some(next_loaded_state);
            self.ai_inline_review_error = None;
            self.ai_inline_review_status_message =
                Some("This AI turn did not capture any file changes.".to_string());
            return;
        }

        let stream = crate::app::data::build_diff_stream_from_patch_map(
            &snapshot.files,
            &std::collections::BTreeSet::new(),
            &snapshot.file_line_stats,
            &snapshot.patches_by_path,
            &std::collections::BTreeSet::new(),
        );
        match crate::app::review_workspace_session::ReviewWorkspaceSession::from_compare_snapshot(
            &snapshot,
            &std::collections::BTreeSet::new(),
        ) {
            Ok(session) => {
                self.ai_inline_review_session = Some(session.with_render_stream(&stream));
                self.ai_inline_review_loaded_state = Some(next_loaded_state);
                self.ai_inline_review_error = None;
                self.ai_inline_review_status_message = None;
            }
            Err(err) => {
                self.ai_inline_review_session = None;
                self.ai_inline_review_loaded_state = Some(next_loaded_state);
                self.ai_inline_review_error = Some(err.to_string());
                self.ai_inline_review_status_message = None;
            }
        }
    }

    fn sync_ai_workspace_session_for_timeline(
        &mut self,
        selected_thread_id: Option<&str>,
        visible_row_ids: &[String],
    ) {
        let Some(thread_id) = selected_thread_id else {
            self.ai_workspace_session = None;
            self.ai_workspace_selection = None;
            return;
        };
        let rebuild_started_at = std::time::Instant::now();
        let source_rows = visible_row_ids
            .iter()
            .filter_map(|row_id| self.ai_workspace_source_row(row_id.as_str()))
            .collect::<Vec<_>>();

        if self
            .ai_workspace_session
            .as_ref()
            .is_some_and(|session| session.matches_source(thread_id, source_rows.as_slice()))
        {
            return;
        }

        let blocks_by_source_row = visible_row_ids
            .iter()
            .map(|row_id| self.ai_workspace_blocks_for_row(row_id.as_str()))
            .collect::<Vec<_>>();
        if self
            .ai_workspace_selection
            .as_ref()
            .is_some_and(|selection| {
                !blocks_by_source_row
                    .iter()
                    .flatten()
                    .any(|block| block.id == selection.block_id)
            })
        {
            self.ai_workspace_selection = None;
        }
        self.ai_workspace_session = Some(ai_workspace_session::AiWorkspaceSession::new(
            thread_id.to_string(),
            Arc::<[ai_workspace_session::AiWorkspaceSourceRow]>::from(source_rows),
            blocks_by_source_row,
        ));
        self.record_ai_workspace_session_rebuild_timing(rebuild_started_at.elapsed());
    }

    fn ai_workspace_blocks_for_row(
        &self,
        row_id: &str,
    ) -> Vec<ai_workspace_session::AiWorkspaceBlock> {
        if let Some(pending) = self.ai_pending_steer_for_row_id(row_id) {
            return vec![ai_workspace_session::AiWorkspaceBlock {
                id: row_id.to_string(),
                source_row_id: row_id.to_string(),
                role: ai_workspace_session::AiWorkspaceBlockRole::User,
                kind: ai_workspace_session::AiWorkspaceBlockKind::Message,
                nested: false,
                mono_preview: false,
                markdown_preview: false,
                open_review_tab: false,
                expandable: false,
                expanded: true,
                title: "You  Waiting to steer running turn...".to_string(),
                preview: ai_workspace_prompt_preview(
                    pending.prompt.as_str(),
                    pending.local_images.as_slice(),
                ),
                action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
                copy_text: Some(ai_workspace_prompt_preview(
                    pending.prompt.as_str(),
                    pending.local_images.as_slice(),
                )),
                copy_tooltip: Some("Copy message"),
                copy_success_message: Some("Copied message."),
                run_in_terminal_command: None,
                run_in_terminal_cwd: None,
                status_label: Some("Waiting to steer running turn...".to_string()),
                status_color_role: Some(ai_workspace_session::AiWorkspacePreviewColorRole::Accent),
                last_sequence: ai_workspace_pending_steer_signature(&pending),
            }];
        }
        if let Some(queued) = self.ai_queued_message_for_row_id(row_id) {
            let preview = ai_workspace_prompt_preview(
                queued.prompt.as_str(),
                queued.local_images.as_slice(),
            );
            return vec![ai_workspace_session::AiWorkspaceBlock {
                id: row_id.to_string(),
                source_row_id: row_id.to_string(),
                role: ai_workspace_session::AiWorkspaceBlockRole::User,
                kind: ai_workspace_session::AiWorkspaceBlockKind::Message,
                nested: false,
                mono_preview: false,
                markdown_preview: false,
                open_review_tab: false,
                expandable: false,
                expanded: true,
                title: match queued.status {
                    AiQueuedUserMessageStatus::Queued => {
                        "You  queued, waiting for current turn to finish.".to_string()
                    }
                    AiQueuedUserMessageStatus::PendingConfirmation { .. } => {
                        "You  Pending Confirmation".to_string()
                    }
                },
                preview: preview.clone(),
                action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
                copy_text: Some(preview),
                copy_tooltip: Some("Copy message"),
                copy_success_message: Some("Copied message."),
                run_in_terminal_command: None,
                run_in_terminal_cwd: None,
                status_label: Some(match queued.status {
                    AiQueuedUserMessageStatus::Queued => {
                        "queued, waiting for current turn to finish.".to_string()
                    }
                    AiQueuedUserMessageStatus::PendingConfirmation { .. } => {
                        "Pending Confirmation".to_string()
                    }
                }),
                status_color_role: Some(ai_workspace_session::AiWorkspacePreviewColorRole::Accent),
                last_sequence: ai_workspace_queued_message_signature(&queued),
            }];
        }

        let Some(row) = self.ai_timeline_row(row_id) else {
            return Vec::new();
        };
        match &row.source {
            AiTimelineRowSource::Item { item_key } => {
                self.ai_state_snapshot
                    .items
                    .get(item_key.as_str())
                    .and_then(|item| self.ai_workspace_block_for_item_row(row, item, false))
                    .into_iter()
                    .collect()
            }
            AiTimelineRowSource::Group { group_id } => {
                self.ai_timeline_group(group_id.as_str())
                    .map(|group| self.ai_workspace_blocks_for_group_row(row, group))
                    .unwrap_or_default()
            }
            AiTimelineRowSource::TurnDiff { turn_key } => {
                self.ai_state_snapshot
                    .turn_diffs
                    .get(turn_key.as_str())
                    .map(|diff| vec![ai_workspace_diff_block(
                        row.id.clone(),
                        row.id.clone(),
                        self.ai_workspace_turn_diff_last_sequence(turn_key.as_str(), row),
                        &crate::app::ai_workspace_timeline_projection::ai_workspace_turn_diff_summary(
                            diff,
                        ),
                        false,
                    )])
                    .unwrap_or_default()
            }
            AiTimelineRowSource::TurnPlan { turn_key } => {
                self.ai_state_snapshot
                    .turn_plans
                    .get(turn_key.as_str())
                    .map(|plan| vec![ai_workspace_session::AiWorkspaceBlock {
                    id: row.id.clone(),
                    source_row_id: row.id.clone(),
                    role: ai_workspace_session::AiWorkspaceBlockRole::Assistant,
                    kind: ai_workspace_session::AiWorkspaceBlockKind::Plan,
                    nested: false,
                    mono_preview: false,
                    markdown_preview: false,
                    open_review_tab: false,
                    expandable: false,
                    expanded: true,
                    title: "Updated Plan".to_string(),
                    preview: ai_workspace_plan_preview(plan),
                    action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
                    copy_text: None,
                    copy_tooltip: None,
                    copy_success_message: None,
                    run_in_terminal_command: None,
                    run_in_terminal_cwd: None,
                    status_label: None,
                    status_color_role: None,
                    last_sequence: plan.last_sequence,
                    }])
                    .unwrap_or_default()
            }
        }
    }

    pub(super) fn ai_select_workspace_selection(
        &mut self,
        selection: ai_workspace_session::AiWorkspaceSelection,
        cx: &mut Context<Self>,
    ) {
        self.ai_workspace_selection = Some(selection);
        self.ai_text_selection = None;
        self.ai_text_selection_drag_pointer = None;
        self.ai_text_selection_auto_scroll_task = Task::ready(());
        cx.notify();
    }

    fn ai_workspace_selected_block(&self) -> Option<&ai_workspace_session::AiWorkspaceBlock> {
        let selection = self.ai_workspace_selection.as_ref()?;
        self.ai_workspace_session
            .as_ref()
            .and_then(|session| session.block(selection.block_id.as_str()))
    }

    pub(super) fn current_ai_workspace_selected_text(&self) -> Option<String> {
        let block = self.ai_workspace_selected_block()?;
        let mut sections = Vec::with_capacity(2);
        if !block.title.trim().is_empty() {
            sections.push(block.title.trim().to_string());
        }
        if !block.preview.trim().is_empty() {
            sections.push(block.preview.trim().to_string());
        }
        (!sections.is_empty()).then(|| sections.join("\n"))
    }

    pub(super) fn ai_select_all_workspace_thread_text(&mut self, cx: &mut Context<Self>) -> bool {
        let viewport_width_px = self
            .ai_workspace_surface_scroll_handle
            .bounds()
            .size
            .width
            .max(Pixels::ZERO)
            .as_f32()
            .round() as usize;
        let Some((selection_scope_id, selection_surfaces)) =
            self.ai_workspace_session.as_mut().map(|session| {
                (
                    session.selection_scope_id().to_string(),
                    session.selection_surfaces_for_width(viewport_width_px.max(1)),
                )
            })
        else {
            return false;
        };
        self.ai_select_all_text_for_surfaces(
            selection_scope_id.as_str(),
            selection_surfaces,
            cx,
        )
    }

    pub(super) fn ai_move_workspace_selection_by(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(next_selection) = self.ai_workspace_session.as_ref().and_then(|session| {
            if session.block_count() == 0 {
                return None;
            }

            let current_index = self
                .ai_workspace_selection
                .as_ref()
                .and_then(|selection| session.block_index(selection.block_id.as_str()));
            let next_index =
                ai_workspace_selection_index(current_index, session.block_count(), delta)?;
            let block = session.block_at(next_index)?;
            Some(ai_workspace_session::AiWorkspaceSelection {
                block_id: block.id.clone(),
                block_kind: block.kind,
                line_index: None,
                region: ai_workspace_session::AiWorkspaceSelectionRegion::Block,
            })
        }) else {
            return false;
        };

        let selected_block_id = next_selection.block_id.clone();
        self.ai_select_workspace_selection(next_selection, cx);
        self.ai_reveal_workspace_block_if_needed(selected_block_id.as_str());
        true
    }

    fn ai_workspace_source_row(
        &self,
        row_id: &str,
    ) -> Option<ai_workspace_session::AiWorkspaceSourceRow> {
        if let Some(row) = self.ai_timeline_row(row_id) {
            return Some(ai_workspace_session::AiWorkspaceSourceRow {
                row_id: row.id.clone(),
                last_sequence: self.ai_workspace_source_signature_for_row(row),
            });
        }
        if let Some(pending) = self.ai_pending_steer_for_row_id(row_id) {
            return Some(ai_workspace_session::AiWorkspaceSourceRow {
                row_id: row_id.to_string(),
                last_sequence: ai_workspace_pending_steer_signature(&pending),
            });
        }
        if let Some(queued) = self.ai_queued_message_for_row_id(row_id) {
            return Some(ai_workspace_session::AiWorkspaceSourceRow {
                row_id: row_id.to_string(),
                last_sequence: ai_workspace_queued_message_signature(&queued),
            });
        }

        None
    }

    pub(super) fn current_ai_inline_review_row_id_for_thread(
        &self,
        thread_id: &str,
    ) -> Option<&str> {
        self.ai_inline_review_selected_row_id_by_thread
            .get(thread_id)
            .map(String::as_str)
    }

    pub(super) fn ai_inline_review_is_open(&self) -> bool {
        self.ai_selected_thread_id
            .as_deref()
            .and_then(|thread_id| self.current_ai_inline_review_row_id_for_thread(thread_id))
            .is_some()
    }

    pub(super) fn current_ai_right_pane_mode_for_thread(
        &self,
        thread_id: &str,
    ) -> Option<AiWorkspaceRightPaneMode> {
        let inline_review_open = self
            .current_ai_inline_review_row_id_for_thread(thread_id)
            .is_some();
        let browser_open = self.ai_browser_open_thread_ids.contains(thread_id);

        match self.ai_right_pane_mode_by_thread.get(thread_id).copied() {
            Some(AiWorkspaceRightPaneMode::InlineReview) if inline_review_open => {
                Some(AiWorkspaceRightPaneMode::InlineReview)
            }
            Some(AiWorkspaceRightPaneMode::Browser) if browser_open => {
                Some(AiWorkspaceRightPaneMode::Browser)
            }
            _ if inline_review_open => Some(AiWorkspaceRightPaneMode::InlineReview),
            _ if browser_open => Some(AiWorkspaceRightPaneMode::Browser),
            _ => None,
        }
    }

    pub(super) fn current_ai_right_pane_mode(&self) -> Option<AiWorkspaceRightPaneMode> {
        self.ai_selected_thread_id
            .as_deref()
            .and_then(|thread_id| self.current_ai_right_pane_mode_for_thread(thread_id))
    }

    pub(super) fn ai_browser_is_open(&self) -> bool {
        self.ai_selected_thread_id
            .as_deref()
            .is_some_and(|thread_id| self.ai_browser_open_thread_ids.contains(thread_id))
    }

    pub(super) fn ai_open_browser_for_current_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_ai_thread_id() else {
            return;
        };
        if self.ai_selected_thread_id.as_deref() != Some(thread_id.as_str()) {
            self.ai_selected_thread_id = Some(thread_id.clone());
        }
        if self.ai_browser_runtime.status() == hunk_browser::BrowserRuntimeStatus::Ready {
            if let Err(err) = self.ai_browser_runtime.ensure_backend_session(thread_id.clone()) {
                error!("failed to create embedded browser session: {err:#}");
                self.ai_browser_runtime
                    .ensure_session(thread_id.clone())
                    .set_load_error(err.to_string());
            }
        } else {
            self.ai_browser_runtime.ensure_session(thread_id.clone());
        }
        self.ai_browser_open_thread_ids.insert(thread_id.clone());
        self.ai_right_pane_mode_by_thread
            .insert(thread_id, AiWorkspaceRightPaneMode::Browser);
        self.ai_sync_browser_pump(cx);
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        cx.notify();
    }

    pub(super) fn ai_toggle_browser_for_current_thread(&mut self, cx: &mut Context<Self>) {
        if self.current_ai_right_pane_mode() == Some(AiWorkspaceRightPaneMode::Browser) {
            self.ai_close_browser_action(cx);
        } else {
            self.ai_open_browser_for_current_thread(cx);
        }
    }

    pub(super) fn ai_close_browser_action(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            return;
        };
        let changed = self.ai_browser_open_thread_ids.remove(thread_id.as_str());
        if self.ai_right_pane_mode_by_thread.get(thread_id.as_str()).copied()
            == Some(AiWorkspaceRightPaneMode::Browser)
        {
            if self
                .current_ai_inline_review_row_id_for_thread(thread_id.as_str())
                .is_some()
            {
                self.ai_right_pane_mode_by_thread
                    .insert(thread_id.clone(), AiWorkspaceRightPaneMode::InlineReview);
            } else {
                self.ai_right_pane_mode_by_thread.remove(thread_id.as_str());
            }
        }
        if changed {
            self.ai_sync_browser_pump(cx);
            self.invalidate_ai_visible_frame_state_with_reason("timeline");
            cx.notify();
        }
    }

    fn ai_sync_browser_pump(&mut self, cx: &mut Context<Self>) {
        let should_run = self.ai_browser_runtime.status() == hunk_browser::BrowserRuntimeStatus::Ready
            && !self.ai_browser_open_thread_ids.is_empty();
        if should_run {
            self.ai_start_browser_pump(cx);
        } else {
            self.ai_stop_browser_pump();
        }
    }

    fn ai_start_browser_pump(&mut self, cx: &mut Context<Self>) {
        if self.ai_browser_pump_active {
            return;
        }
        self.ai_browser_pump_generation = self.ai_browser_pump_generation.saturating_add(1);
        let generation = self.ai_browser_pump_generation;
        self.ai_browser_pump_active = true;
        self.ai_browser_pump_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;

                let Some(this) = this.upgrade() else {
                    return;
                };

                let mut keep_running = true;
                this.update(cx, |this, cx| {
                    if this.ai_browser_pump_generation != generation
                        || this.ai_browser_runtime.status()
                            != hunk_browser::BrowserRuntimeStatus::Ready
                        || this.ai_browser_open_thread_ids.is_empty()
                    {
                        this.ai_browser_pump_active = false;
                        this.ai_browser_pump_task = Task::ready(());
                        keep_running = false;
                        return;
                    }

                    match this.ai_browser_runtime.pump_backend() {
                        Ok(changed) => {
                            if changed {
                                cx.notify();
                            }
                        }
                        Err(err) => {
                            error!("embedded browser pump failed: {err:#}");
                            this.ai_browser_pump_active = false;
                            this.ai_browser_pump_task = Task::ready(());
                            keep_running = false;
                        }
                    }
                });
                if !keep_running {
                    return;
                }
            }
        });
    }

    fn ai_stop_browser_pump(&mut self) {
        self.ai_browser_pump_generation = self.ai_browser_pump_generation.saturating_add(1);
        self.ai_browser_pump_active = false;
        self.ai_browser_pump_task = Task::ready(());
    }

    pub(super) fn ai_apply_browser_action_for_current_thread(
        &mut self,
        action: hunk_browser::BrowserAction,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            return;
        };
        if self.ai_browser_runtime.status() == hunk_browser::BrowserRuntimeStatus::Ready {
            if let Err(err) = self
                .ai_browser_runtime
                .apply_backend_action(thread_id.as_str(), &action)
            {
                error!("embedded browser action failed: {err:#}");
                self.ai_browser_runtime
                    .ensure_session(thread_id)
                    .set_load_error(err.to_string());
            }
            self.ai_sync_browser_pump(cx);
        } else if let Err(err) = self
            .ai_browser_runtime
            .apply_state_only_action(thread_id.as_str(), &action)
        {
            self.ai_browser_runtime
                .ensure_session(thread_id)
                .set_load_error(err.to_string());
        }
        cx.notify();
    }

    pub(super) fn ai_submit_browser_address(&mut self, cx: &mut Context<Self>) {
        let value = self.ai_browser_address_input_state.read(cx).value();
        let Some(url) = normalize_ai_browser_address(value.as_ref()) else {
            return;
        };
        self.ai_apply_browser_action_for_current_thread(
            hunk_browser::BrowserAction::Navigate { url },
            cx,
        );
    }

    pub(super) fn ai_set_right_pane_mode(
        &mut self,
        mode: AiWorkspaceRightPaneMode,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            return;
        };
        let mode_available = match mode {
            AiWorkspaceRightPaneMode::InlineReview => self
                .current_ai_inline_review_row_id_for_thread(thread_id.as_str())
                .is_some(),
            AiWorkspaceRightPaneMode::Browser => self.ai_browser_open_thread_ids.contains(&thread_id),
        };
        if !mode_available {
            return;
        }
        if self
            .ai_right_pane_mode_by_thread
            .insert(thread_id, mode)
            != Some(mode)
        {
            self.invalidate_ai_visible_frame_state_with_reason("timeline");
            cx.notify();
        }
    }

    pub(super) fn ai_row_supports_inline_review(&self, row: &AiTimelineRow) -> bool {
        let item_kind = match &row.source {
            AiTimelineRowSource::Item { item_key } => self
                .ai_state_snapshot
                .items
                .get(item_key.as_str())
                .map(|item| item.kind.as_str()),
            _ => None,
        };
        let group_kind = match &row.source {
            AiTimelineRowSource::Group { group_id } => self
                .ai_timeline_group(group_id.as_str())
                .map(|group| group.kind.as_str()),
            _ => None,
        };

        ai_timeline_row_supports_inline_review(row, item_kind, group_kind)
    }

    pub(super) fn ai_open_inline_review_for_row(&mut self, row_id: String, cx: &mut Context<Self>) {
        self.ai_open_inline_review_for_row_in_mode(row_id, AiInlineReviewMode::Historical, cx);
    }

    fn ai_open_inline_review_for_row_in_mode(
        &mut self,
        row_id: String,
        mode: AiInlineReviewMode,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            return;
        };
        let Some(row) = self.ai_timeline_row(row_id.as_str()) else {
            return;
        };
        if !self.ai_row_supports_inline_review(row) {
            return;
        }

        let changed_row = self
            .ai_inline_review_selected_row_id_by_thread
            .get(thread_id.as_str())
            .is_none_or(|current| current != &row_id);
        let changed_mode = self.ai_inline_review_mode_for_thread(thread_id.as_str()) != mode;
        self.ai_inline_review_selected_row_id_by_thread
            .insert(thread_id.clone(), row_id.clone());
        self.ai_inline_review_mode_by_thread
            .insert(thread_id.clone(), mode);
        self.ai_right_pane_mode_by_thread
            .insert(thread_id.clone(), AiWorkspaceRightPaneMode::InlineReview);
        if changed_row || changed_mode {
            self.ai_inline_review_surface.clear_runtime_state();
        }
        if changed_row || changed_mode {
            self.ai_clear_inline_review_loaded_state();
        }
        self.ai_sync_review_compare_to_selected_thread(cx);
        self.ai_sync_historical_inline_review_session_if_needed();
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        cx.notify();
    }

    pub(super) fn ai_open_inline_review_for_current_thread_in_mode(
        &mut self,
        mode: AiInlineReviewMode,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = self.current_ai_thread_id() else {
            return;
        };
        if self.ai_selected_thread_id.as_deref() != Some(thread_id.as_str()) {
            self.ai_selected_thread_id = Some(thread_id.clone());
        }
        let Some(row_id) = self.ai_resolve_inline_review_row_id_for_thread(thread_id.as_str()) else {
            return;
        };
        self.ai_open_inline_review_for_row_in_mode(row_id, mode, cx);
    }

    pub(super) fn ai_toggle_inline_review_for_current_thread_in_mode(
        &mut self,
        mode: AiInlineReviewMode,
        cx: &mut Context<Self>,
    ) {
        let Some(target_mode) = ai_inline_review_toggle_target_mode(
            self.ai_inline_review_is_open(),
            self.current_ai_inline_review_mode(),
            mode,
        ) else {
            self.ai_close_inline_review_action(cx);
            return;
        };

        self.ai_open_inline_review_for_current_thread_in_mode(target_mode, cx);
    }

    pub(super) fn ai_set_inline_review_mode(
        &mut self,
        mode: AiInlineReviewMode,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            return;
        };
        if self.ai_inline_review_mode_for_thread(thread_id.as_str()) == mode {
            return;
        }
        self.ai_inline_review_mode_by_thread
            .insert(thread_id, mode);
        if let Some(thread_id) = self.ai_selected_thread_id.clone() {
            self.ai_right_pane_mode_by_thread
                .insert(thread_id, AiWorkspaceRightPaneMode::InlineReview);
        }
        self.ai_inline_review_surface.clear_runtime_state();
        self.ai_clear_inline_review_loaded_state();
        self.ai_sync_review_compare_to_selected_thread(cx);
        self.ai_sync_historical_inline_review_session_if_needed();
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        cx.notify();
    }

    pub(super) fn ai_close_inline_review_action(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.ai_selected_thread_id.as_deref() else {
            return;
        };
        if self
            .ai_inline_review_selected_row_id_by_thread
            .remove(thread_id)
            .is_some()
        {
            if self.ai_right_pane_mode_by_thread.get(thread_id).copied()
                == Some(AiWorkspaceRightPaneMode::InlineReview)
            {
                if self.ai_browser_open_thread_ids.contains(thread_id) {
                    self.ai_right_pane_mode_by_thread
                        .insert(thread_id.to_string(), AiWorkspaceRightPaneMode::Browser);
                } else {
                    self.ai_right_pane_mode_by_thread.remove(thread_id);
                }
            }
            self.ai_clear_inline_review_loaded_state();
            self.ai_inline_review_surface.clear_runtime_state();
            self.invalidate_ai_visible_frame_state_with_reason("timeline");
            cx.notify();
        }
    }

    pub(super) fn current_ai_workspace_surface_scroll_offset(&self) -> Point<Pixels> {
        if self.workspace_view_mode == WorkspaceViewMode::Ai && self.ai_workspace_session.is_some()
        {
            return self.ai_workspace_surface_scroll_handle.offset();
        }

        point(px(0.), px(0.))
    }

    pub(super) fn current_ai_workspace_surface_scroll_top_px(&self) -> usize {
        self.ai_workspace_surface_scroll_handle
            .offset()
            .y
            .min(Pixels::ZERO)
            .abs()
            .as_f32()
            .round() as usize
    }

    pub(super) fn refresh_ai_timeline_follow_output_from_surface_scroll(&mut self) {
        let block_count = self
            .ai_workspace_session
            .as_ref()
            .map(|session| session.block_count())
            .unwrap_or(0);
        let scroll_offset_y = self.ai_workspace_surface_scroll_handle.offset().y.as_f32();
        let max_scroll_offset_y = self
            .ai_workspace_surface_scroll_handle
            .max_offset()
            .y
            .max(Pixels::ZERO)
            .as_f32();
        self.ai_timeline_follow_output =
            should_follow_timeline_output(block_count, scroll_offset_y, max_scroll_offset_y);
    }

    pub(super) fn ai_auto_scroll_workspace_text_selection_drag(
        &mut self,
        pointer_position: gpui::Point<gpui::Pixels>,
        viewport_bounds: gpui::Bounds<gpui::Pixels>,
    ) -> bool {
        const EDGE_SCROLL_THRESHOLD_PX: f32 = 28.0;
        const MIN_EDGE_SCROLL_STEP_PX: f32 = 14.0;
        const MAX_EDGE_SCROLL_STEP_PX: f32 = 42.0;

        let viewport_top = viewport_bounds.origin.y;
        let viewport_bottom = viewport_bounds.origin.y + viewport_bounds.size.height;
        let delta_px = if pointer_position.y < viewport_top + px(EDGE_SCROLL_THRESHOLD_PX) {
            let intensity = ((viewport_top + px(EDGE_SCROLL_THRESHOLD_PX) - pointer_position.y)
                .as_f32()
                / EDGE_SCROLL_THRESHOLD_PX)
                .clamp(0.0, 1.0);
            -(MIN_EDGE_SCROLL_STEP_PX
                + (MAX_EDGE_SCROLL_STEP_PX - MIN_EDGE_SCROLL_STEP_PX) * intensity)
        } else if pointer_position.y > viewport_bottom - px(EDGE_SCROLL_THRESHOLD_PX) {
            let intensity = ((pointer_position.y - (viewport_bottom - px(EDGE_SCROLL_THRESHOLD_PX)))
                .as_f32()
                / EDGE_SCROLL_THRESHOLD_PX)
                .clamp(0.0, 1.0);
            MIN_EDGE_SCROLL_STEP_PX
                + (MAX_EDGE_SCROLL_STEP_PX - MIN_EDGE_SCROLL_STEP_PX) * intensity
        } else {
            0.0
        };

        if delta_px == 0.0 {
            return false;
        }

        let current_scroll_top_px = self.current_ai_workspace_surface_scroll_top_px() as f32;
        let max_scroll_top_px = self
            .ai_workspace_surface_scroll_handle
            .max_offset()
            .y
            .max(Pixels::ZERO)
            .as_f32();
        let next_scroll_top_px = (current_scroll_top_px + delta_px).clamp(0.0, max_scroll_top_px);
        if (next_scroll_top_px - current_scroll_top_px).abs() < f32::EPSILON {
            return false;
        }

        self.ai_workspace_surface_scroll_handle
            .set_offset(point(px(0.), -px(next_scroll_top_px)));
        self.refresh_ai_timeline_follow_output_from_surface_scroll();
        true
    }

    pub(super) fn ai_drive_workspace_text_selection_auto_scroll(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pointer_position) = self.ai_text_selection_drag_pointer else {
            return false;
        };
        let viewport_bounds = self.ai_workspace_surface_scroll_handle.bounds();
        if !self.ai_auto_scroll_workspace_text_selection_drag(pointer_position, viewport_bounds) {
            return false;
        }

        let viewport_height_px = viewport_bounds
            .size
            .height
            .max(Pixels::ZERO)
            .as_f32()
            .round() as usize;
        let viewport_width_px = viewport_bounds
            .size
            .width
            .max(Pixels::ZERO)
            .as_f32()
            .round() as usize;
        let scroll_top_px = self.current_ai_workspace_surface_scroll_top_px();
        let Some(snapshot) = self.ai_workspace_session.as_mut().map(|session| {
            session
                .surface_snapshot_with_stats(
                    scroll_top_px,
                    viewport_height_px.max(1),
                    viewport_width_px.max(1),
                )
                .snapshot
        }) else {
            return false;
        };
        let workspace_root = self
            .ai_workspace_cwd()
            .or_else(|| self.selected_git_workspace_root())
            .or_else(|| self.repo_root.clone());
        let Some(text_hit) = crate::app::ai_workspace_render::ai_workspace_drag_text_hit(
            &snapshot,
            pointer_position,
            viewport_bounds,
            workspace_root.as_deref(),
        ) else {
            return false;
        };
        self.ai_update_text_selection(text_hit.surface_id.as_str(), text_hit.index, cx);
        true
    }

    pub(super) fn ai_schedule_workspace_text_selection_auto_scroll(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.ai_text_selection_auto_scroll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                let keep_running = this
                    .update(cx, |this, cx| {
                        this.ai_text_selection.as_ref().is_some_and(|selection| selection.dragging)
                            && this.ai_drive_workspace_text_selection_auto_scroll(cx)
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        });
    }

    fn ai_reveal_workspace_block_if_needed(&mut self, block_id: &str) {
        let viewport_bounds = self.ai_workspace_surface_scroll_handle.bounds();
        let viewport_height_px = viewport_bounds
            .size
            .height
            .max(Pixels::ZERO)
            .as_f32()
            .round() as usize;
        let viewport_width_px = viewport_bounds
            .size
            .width
            .max(Pixels::ZERO)
            .as_f32()
            .round() as usize;
        let scroll_top_px = self.current_ai_workspace_surface_scroll_top_px();
        let Some(geometry) = self
            .ai_workspace_session
            .as_mut()
            .and_then(|session| session.block_geometry(block_id, viewport_width_px.max(1)))
        else {
            return;
        };

        let viewport_bottom_px = scroll_top_px.saturating_add(viewport_height_px);
        let next_scroll_top_px = if geometry.top_px < scroll_top_px {
            Some(geometry.top_px)
        } else if geometry.bottom_px() > viewport_bottom_px {
            Some(geometry.bottom_px().saturating_sub(viewport_height_px))
        } else {
            None
        };
        let Some(next_scroll_top_px) = next_scroll_top_px else {
            return;
        };

        self.ai_workspace_surface_scroll_handle
            .set_offset(point(px(0.), -px(next_scroll_top_px as f32)));
        self.refresh_ai_timeline_follow_output_from_surface_scroll();
    }

    pub(super) fn ai_workspace_toggle_row_expansion(
        &mut self,
        row_id: String,
        cx: &mut Context<Self>,
    ) {
        self.ai_toggle_timeline_row_expansion_action(row_id, cx);
    }

    fn ai_workspace_row_is_expanded(&self, row_id: &str) -> bool {
        self.ai_expanded_timeline_row_ids.contains(row_id)
    }
}

impl DiffViewer {
    fn ai_workspace_block_for_item_row(
        &self,
        row: &AiTimelineRow,
        item: &hunk_codex::state::ItemSummary,
        nested: bool,
    ) -> Option<ai_workspace_session::AiWorkspaceBlock> {
        let expanded = self.ai_workspace_row_is_expanded(row.id.as_str());
        match Self::ai_workspace_message_block_config(item.kind.as_str()) {
            Some((role, title)) => {
                let preview = ai_workspace_message_preview(item);
                Some(ai_workspace_session::AiWorkspaceBlock {
                id: row.id.clone(),
                source_row_id: row.id.clone(),
                role,
                kind: ai_workspace_session::AiWorkspaceBlockKind::Message,
                nested,
                mono_preview: false,
                markdown_preview: ai_workspace_message_uses_markdown_preview(
                    &self.ai_state_snapshot,
                    item,
                ),
                open_review_tab: false,
                expandable: false,
                expanded: true,
                title: title.to_string(),
                preview: preview.clone(),
                action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
                copy_text: Some(preview),
                copy_tooltip: Some(if item.kind == "plan" {
                    "Copy plan"
                } else {
                    "Copy message"
                }),
                copy_success_message: Some(if item.kind == "plan" {
                    "Copied plan."
                } else {
                    "Copied message."
                }),
                run_in_terminal_command: None,
                run_in_terminal_cwd: None,
                status_label: None,
                status_color_role: None,
                last_sequence: item.last_sequence,
                })
            }
            None if item.kind == "fileChange" => crate::app::ai_workspace_timeline_projection::ai_workspace_file_change_summary(item)
                .map(|summary| {
                    ai_workspace_diff_block(
                        row.id.clone(),
                        row.id.clone(),
                        item.last_sequence,
                        &summary,
                        nested,
                    )
                }),
            None if item.kind == "commandExecution" => {
                let raw_content_text = item.content.trim_end();
                let command_details =
                    crate::app::ai_workspace_timeline_projection::ai_workspace_command_execution_display_details(item);
                let has_details = command_details.is_some() || !raw_content_text.is_empty();
                let title = ai_workspace_tool_header_line(item, raw_content_text);
                let status_label = command_details
                    .as_ref()
                    .map(|details| details.status.replace('_', " "));
                let preview = if expanded {
                    command_details
                        .as_ref()
                        .map(|details| {
                            crate::app::ai_workspace_timeline_projection::ai_workspace_command_execution_terminal_text(
                                details,
                                raw_content_text,
                                Some(
                                    crate::app::ai_workspace_timeline_projection::AI_WORKSPACE_COMMAND_PREVIEW_MAX_OUTPUT_LINES,
                                ),
                            )
                            .0
                        })
                        .unwrap_or_else(|| ai_workspace_expanded_tool_text(raw_content_text))
                } else {
                    String::new()
                };
                let copy_text = expanded.then_some(preview.clone());
                Some(ai_workspace_session::AiWorkspaceBlock {
                    id: row.id.clone(),
                    source_row_id: row.id.clone(),
                    role: ai_workspace_session::AiWorkspaceBlockRole::Tool,
                    kind: ai_workspace_session::AiWorkspaceBlockKind::Tool,
                    nested,
                    mono_preview: true,
                    markdown_preview: false,
                    open_review_tab: false,
                    expandable: has_details,
                    expanded,
                    title,
                    preview,
                    action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Preview,
                    copy_text,
                    copy_tooltip: expanded.then_some("Copy command transcript"),
                    copy_success_message: expanded.then_some("Copied command transcript."),
                    run_in_terminal_command: expanded.then(|| {
                        command_details
                            .as_ref()
                            .map(|details| details.command.trim().to_string())
                            .filter(|command| !command.is_empty())
                    }).flatten(),
                    run_in_terminal_cwd: expanded.then(|| {
                        command_details
                            .as_ref()
                            .and_then(|details| (!details.cwd.trim().is_empty()).then(|| PathBuf::from(details.cwd.clone())))
                    }).flatten(),
                    status_label,
                    status_color_role: Some(ai_workspace_command_status_color_role(
                        command_details.as_ref(),
                    )),
                    last_sequence: item.last_sequence,
                })
            }
            None
                if matches!(
                    item.kind.as_str(),
                    "reasoning"
                        | "webSearch"
                        | "dynamicToolCall"
                        | "mcpToolCall"
                        | "collabAgentToolCall"
                ) =>
            {
                let details_text = crate::app::ai_workspace_timeline_projection::ai_workspace_timeline_item_details_json(item)
                    .unwrap_or(item.content.as_str());
                let details_text = details_text.trim();
                let preview = if expanded {
                    ai_workspace_expanded_tool_text(details_text)
                } else {
                    String::new()
                };
                let has_details = !details_text.is_empty();
                Some(ai_workspace_session::AiWorkspaceBlock {
                    id: row.id.clone(),
                    source_row_id: row.id.clone(),
                    role: if item.kind == "reasoning" || item.kind == "webSearch" {
                        ai_workspace_session::AiWorkspaceBlockRole::Assistant
                    } else if item.kind == "dynamicToolCall"
                        || item.kind == "mcpToolCall"
                        || item.kind == "collabAgentToolCall"
                    {
                        ai_workspace_session::AiWorkspaceBlockRole::Tool
                    } else {
                        ai_workspace_session::AiWorkspaceBlockRole::System
                    },
                    kind: if item.kind == "reasoning" {
                        ai_workspace_session::AiWorkspaceBlockKind::Status
                    } else {
                        ai_workspace_session::AiWorkspaceBlockKind::Tool
                    },
                    nested,
                    mono_preview: item.kind != "reasoning" && item.kind != "webSearch",
                    markdown_preview: false,
                    open_review_tab: false,
                    expandable: has_details,
                    expanded,
                    title: ai_workspace_tool_header_line(item, item.content.trim()),
                    preview,
                    action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
                    copy_text: None,
                    copy_tooltip: None,
                    copy_success_message: None,
                    run_in_terminal_command: None,
                    run_in_terminal_cwd: None,
                    status_label: (item.status != hunk_codex::state::ItemStatus::Completed)
                        .then(|| {
                            crate::app::ai_workspace_timeline_projection::ai_workspace_item_status_label(
                                item.status,
                            )
                            .to_string()
                        }),
                    status_color_role: (item.status != hunk_codex::state::ItemStatus::Completed)
                        .then_some(ai_workspace_session::AiWorkspacePreviewColorRole::Accent),
                    last_sequence: item.last_sequence,
                })
            }
            None => Some(ai_workspace_session::AiWorkspaceBlock {
                id: row.id.clone(),
                source_row_id: row.id.clone(),
                role: ai_workspace_session::AiWorkspaceBlockRole::System,
                kind: ai_workspace_session::AiWorkspaceBlockKind::Status,
                nested,
                mono_preview: false,
                markdown_preview: false,
                open_review_tab: false,
                expandable: false,
                expanded: true,
                title: ai_workspace_tool_header_line(item, item.content.trim()),
                preview: String::new(),
                action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
                copy_text: None,
                copy_tooltip: None,
                copy_success_message: None,
                run_in_terminal_command: None,
                run_in_terminal_cwd: None,
                status_label: None,
                status_color_role: None,
                last_sequence: item.last_sequence,
            }),
        }
    }

    fn ai_workspace_blocks_for_group_row(
        &self,
        row: &AiTimelineRow,
        group: &AiTimelineGroup,
    ) -> Vec<ai_workspace_session::AiWorkspaceBlock> {
        if group.kind == "file_change_batch"
            && let Some(summary) = ai_workspace_file_change_group_summary(self, group)
        {
            return vec![ai_workspace_diff_block(
                row.id.clone(),
                row.id.clone(),
                self.ai_workspace_source_signature_for_row(row),
                &summary,
                false,
            )];
        }

        let expanded = self.ai_workspace_row_is_expanded(row.id.as_str());
        let (group_title, group_summary) = self.ai_workspace_group_title_and_summary(group);
        let mut blocks = vec![ai_workspace_session::AiWorkspaceBlock {
            id: row.id.clone(),
            source_row_id: row.id.clone(),
            role: ai_workspace_session::AiWorkspaceBlockRole::Tool,
            kind: ai_workspace_session::AiWorkspaceBlockKind::Group,
            nested: false,
            mono_preview: false,
            markdown_preview: false,
            open_review_tab: false,
            expandable: true,
            expanded,
            title: crate::app::ai_workspace_timeline_projection::ai_workspace_format_header_line(
                group_title.as_str(),
                group_summary.as_deref(),
                None,
            ),
            preview: String::new(),
            action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
            copy_text: None,
            copy_tooltip: None,
            copy_success_message: None,
            run_in_terminal_command: None,
            run_in_terminal_cwd: None,
            status_label: None,
            status_color_role: None,
            last_sequence: self.ai_workspace_source_signature_for_row(row),
        }];

        if !expanded {
            return blocks;
        }

        blocks.extend(group.child_row_ids.iter().filter_map(|child_row_id| {
            let child_row = self.ai_timeline_row(child_row_id.as_str())?;
            let AiTimelineRowSource::Item { item_key } = &child_row.source else {
                return None;
            };
            let item = self.ai_state_snapshot.items.get(item_key.as_str())?;
            self.ai_workspace_block_for_item_row(child_row, item, true)
        }));
        blocks
    }

    fn ai_workspace_group_source_signature(
        &self,
        row: &AiTimelineRow,
        group: &AiTimelineGroup,
    ) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(
            &ai_workspace_row_signature(
                self.ai_workspace_row_content_last_sequence(row),
                self.ai_workspace_row_is_expanded(row.id.as_str()),
            ),
            &mut hasher,
        );

        if self.ai_workspace_row_is_expanded(row.id.as_str()) {
            for child_row_id in &group.child_row_ids {
                if let Some(child_row) = self.ai_timeline_row(child_row_id.as_str()) {
                    std::hash::Hash::hash(
                        &ai_workspace_row_signature(
                            self.ai_workspace_row_content_last_sequence(child_row),
                            self.ai_workspace_row_is_expanded(child_row.id.as_str()),
                        ),
                        &mut hasher,
                    );
                }
            }
        }

        std::hash::Hasher::finish(&hasher)
    }

    fn ai_workspace_group_title_and_summary(
        &self,
        group: &AiTimelineGroup,
    ) -> (String, Option<String>) {
        let mut summary = None::<AiTimelineGroupSummary>;
        for child_row_id in &group.child_row_ids {
            let Some(child_row) = self.ai_timeline_row(child_row_id.as_str()) else {
                continue;
            };
            let Some(next_summary) =
                ai_timeline_group_summary_for_row(&self.ai_state_snapshot, child_row)
            else {
                continue;
            };
            if let Some(current_summary) = summary.as_mut() {
                current_summary.merge(next_summary);
            } else {
                summary = Some(next_summary);
            }
        }

        summary
            .as_ref()
            .map(|summary| ai_timeline_group_title_and_summary(summary, group.child_row_ids.len()))
            .unwrap_or_else(|| (group.title.clone(), group.summary.clone()))
    }

    fn ai_workspace_source_signature_for_row(&self, row: &AiTimelineRow) -> u64 {
        match &row.source {
            AiTimelineRowSource::Group { group_id } => self
                .ai_timeline_group(group_id.as_str())
                .map(|group| self.ai_workspace_group_source_signature(row, group))
                .unwrap_or_else(|| {
                    ai_workspace_row_signature(
                        self.ai_workspace_row_content_last_sequence(row),
                        self.ai_workspace_row_is_expanded(row.id.as_str()),
                    )
                }),
            _ => ai_workspace_row_signature(
                self.ai_workspace_row_content_last_sequence(row),
                self.ai_workspace_row_is_expanded(row.id.as_str()),
            ),
        }
    }

    fn ai_workspace_row_content_last_sequence(&self, row: &AiTimelineRow) -> u64 {
        match &row.source {
            AiTimelineRowSource::Item { item_key } => self
                .ai_state_snapshot
                .items
                .get(item_key.as_str())
                .map(|item| item.last_sequence)
                .unwrap_or(row.last_sequence),
            AiTimelineRowSource::TurnDiff { turn_key } => {
                self.ai_workspace_turn_diff_last_sequence(turn_key.as_str(), row)
            }
            AiTimelineRowSource::TurnPlan { turn_key } => self
                .ai_state_snapshot
                .turn_plans
                .get(turn_key.as_str())
                .map(|plan| plan.last_sequence)
                .unwrap_or(row.last_sequence),
            AiTimelineRowSource::Group { .. } => row.last_sequence,
        }
    }

    fn ai_workspace_turn_diff_last_sequence(&self, turn_key: &str, row: &AiTimelineRow) -> u64 {
        self.ai_state_snapshot
            .turn_diff_sequence(turn_key)
            .unwrap_or(row.last_sequence)
    }
}

fn ai_workspace_message_uses_markdown_preview(
    state: &hunk_codex::state::AiState,
    item: &hunk_codex::state::ItemSummary,
) -> bool {
    if item.status == hunk_codex::state::ItemStatus::Completed {
        return true;
    }

    let turn_key = hunk_codex::state::turn_storage_key(item.thread_id.as_str(), item.turn_id.as_str());
    !state
        .turns
        .get(turn_key.as_str())
        .is_some_and(|turn| turn.status == hunk_codex::state::TurnStatus::InProgress)
}
