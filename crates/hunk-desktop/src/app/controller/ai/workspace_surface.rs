impl DiffViewer {
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
            self.record_ai_workspace_session_cache_hit();
            return;
        }

        let blocks = visible_row_ids
            .iter()
            .flat_map(|row_id| self.ai_workspace_blocks_for_row(row_id.as_str()))
            .collect::<Vec<_>>();
        if self
            .ai_workspace_selection
            .as_ref()
            .is_some_and(|selection| !blocks.iter().any(|block| block.id == selection.block_id))
        {
            self.ai_workspace_selection = None;
        }
        let source_rows = Arc::<[ai_workspace_session::AiWorkspaceSourceRow]>::from(source_rows);
        match self.ai_workspace_session.as_mut() {
            Some(session) if session.belongs_to_thread(thread_id) => {
                session.update_source(thread_id.to_string(), source_rows, blocks);
                self.record_ai_workspace_session_refresh_timing(rebuild_started_at.elapsed());
            }
            _ => {
                self.ai_workspace_session = Some(ai_workspace_session::AiWorkspaceSession::new(
                    thread_id.to_string(),
                    source_rows,
                    blocks,
                ));
                self.record_ai_workspace_session_rebuild_timing(rebuild_started_at.elapsed());
            }
        }
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
                open_side_diff_pane: false,
                expandable: false,
                expanded: true,
                title: "You  Waiting to steer running turn...".to_string(),
                preview: ai_workspace_prompt_preview(
                    pending.prompt.as_str(),
                    pending.local_images.as_slice(),
                ),
                preferred_review_path: None,
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
                open_side_diff_pane: false,
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
                preferred_review_path: None,
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
                    .map(|diff| {
                        let summary =
                            crate::app::ai_workspace_timeline_projection::ai_workspace_turn_diff_summary(
                                diff,
                            );
                        vec![ai_workspace_diff_block(
                            row.id.clone(),
                            row.id.clone(),
                            row.last_sequence,
                            &summary,
                            summary.files.first().map(|file| file.path.clone()),
                            false,
                        )]
                    })
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
                    open_side_diff_pane: false,
                    expandable: false,
                    expanded: true,
                    title: "Updated Plan".to_string(),
                    preview: ai_workspace_plan_preview(plan),
                    preferred_review_path: None,
                    action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
                    copy_text: None,
                    copy_tooltip: None,
                    copy_success_message: None,
                    run_in_terminal_command: None,
                    run_in_terminal_cwd: None,
                    status_label: None,
                    status_color_role: None,
                    last_sequence: row.last_sequence,
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
            let last_sequence = match &row.source {
                AiTimelineRowSource::Group { group_id } => self
                    .ai_timeline_group(group_id.as_str())
                    .map(|group| self.ai_workspace_group_source_signature(row, group))
                    .unwrap_or_else(|| {
                        ai_workspace_row_signature(
                            row.last_sequence,
                            self.ai_workspace_row_is_expanded(row.id.as_str()),
                        )
                    }),
                _ => ai_workspace_row_signature(
                    row.last_sequence,
                    self.ai_workspace_row_is_expanded(row.id.as_str()),
                ),
            };
            return Some(ai_workspace_session::AiWorkspaceSourceRow {
                row_id: row.id.clone(),
                last_sequence,
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

    fn ai_preferred_review_path_for_row(&self, row_id: &str) -> Option<String> {
        let row = self.ai_timeline_row(row_id)?;
        match &row.source {
            AiTimelineRowSource::Item { item_key } => self
                .ai_state_snapshot
                .items
                .get(item_key.as_str())
                .and_then(crate::app::ai_workspace_timeline_projection::ai_workspace_file_change_summary)
                .and_then(|summary| summary.files.first().map(|file| file.path.clone())),
            AiTimelineRowSource::Group { group_id } => self
                .ai_timeline_group(group_id.as_str())
                .and_then(|group| ai_workspace_file_change_group_summary(self, group))
                .and_then(|summary| summary.files.first().map(|file| file.path.clone())),
            AiTimelineRowSource::TurnDiff { turn_key } => self
                .ai_state_snapshot
                .turn_diffs
                .get(turn_key.as_str())
                .map(|diff| {
                    crate::app::ai_workspace_timeline_projection::ai_workspace_turn_diff_summary(
                        diff,
                    )
                })
                .and_then(|summary| summary.files.first().map(|file| file.path.clone())),
            _ => None,
        }
    }

    fn ai_focus_review_for_row(&mut self, row_id: &str, cx: &mut Context<Self>) {
        self.ai_sync_review_compare_to_selected_thread(cx);
        if let Some(path) = self.ai_preferred_review_path_for_row(row_id) {
            self.set_review_selected_file(Some(path.clone()), None);
            self.scroll_to_file_start(path.as_str());
        }
    }

    pub(super) fn ai_open_inline_review_for_row(&mut self, row_id: String, cx: &mut Context<Self>) {
        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            return;
        };
        if self.ai_timeline_row(row_id.as_str()).is_none() {
            return;
        }

        self.ai_inline_review_selected_row_id_by_thread
            .insert(thread_id, row_id.clone());
        self.ai_focus_review_for_row(row_id.as_str(), cx);
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        cx.notify();
    }

    pub(super) fn ai_open_review_tab_for_row(&mut self, row_id: String, cx: &mut Context<Self>) {
        self.ai_focus_review_for_row(row_id.as_str(), cx);
        self.set_workspace_view_mode(WorkspaceViewMode::Diff, cx);
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
        match item.kind.as_str() {
            "userMessage" | "agentMessage" => {
                let preview = ai_workspace_message_preview(item);
                Some(ai_workspace_session::AiWorkspaceBlock {
                id: row.id.clone(),
                source_row_id: row.id.clone(),
                role: if item.kind == "userMessage" {
                    ai_workspace_session::AiWorkspaceBlockRole::User
                } else {
                    ai_workspace_session::AiWorkspaceBlockRole::Assistant
                },
                kind: ai_workspace_session::AiWorkspaceBlockKind::Message,
                nested,
                mono_preview: false,
                open_side_diff_pane: false,
                expandable: false,
                expanded: true,
                title: if item.kind == "userMessage" {
                    "You".to_string()
                } else {
                    "Assistant".to_string()
                },
                preview: preview.clone(),
                preferred_review_path: None,
                action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
                copy_text: Some(preview),
                copy_tooltip: Some("Copy message"),
                copy_success_message: Some("Copied message."),
                run_in_terminal_command: None,
                run_in_terminal_cwd: None,
                status_label: None,
                status_color_role: None,
                last_sequence: row.last_sequence,
                })
            }
            "fileChange" => crate::app::ai_workspace_timeline_projection::ai_workspace_file_change_summary(item)
                .map(|summary| {
                    ai_workspace_diff_block(
                        row.id.clone(),
                        row.id.clone(),
                        row.last_sequence,
                        &summary,
                        summary.files.first().map(|file| file.path.clone()),
                        nested,
                    )
                }),
            "commandExecution" => {
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
                    open_side_diff_pane: false,
                    expandable: has_details,
                    expanded,
                    title,
                    preview,
                    preferred_review_path: None,
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
                    last_sequence: row.last_sequence,
                })
            }
            "reasoning" | "webSearch" | "dynamicToolCall" | "mcpToolCall"
            | "collabAgentToolCall" => {
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
                    open_side_diff_pane: false,
                    expandable: has_details,
                    expanded,
                    title: ai_workspace_tool_header_line(item, item.content.trim()),
                    preview,
                    preferred_review_path: None,
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
                    last_sequence: row.last_sequence,
                })
            }
            _ => Some(ai_workspace_session::AiWorkspaceBlock {
                id: row.id.clone(),
                source_row_id: row.id.clone(),
                role: ai_workspace_session::AiWorkspaceBlockRole::System,
                kind: ai_workspace_session::AiWorkspaceBlockKind::Status,
                nested,
                mono_preview: false,
                open_side_diff_pane: false,
                expandable: false,
                expanded: true,
                title: ai_workspace_tool_header_line(item, item.content.trim()),
                preview: String::new(),
                preferred_review_path: None,
                action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
                copy_text: None,
                copy_tooltip: None,
                copy_success_message: None,
                run_in_terminal_command: None,
                run_in_terminal_cwd: None,
                status_label: None,
                status_color_role: None,
                last_sequence: row.last_sequence,
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
            let expanded = self.ai_workspace_row_is_expanded(row.id.as_str());
            let mut blocks = vec![ai_workspace_file_change_batch_group_block(
                row.id.clone(),
                row.id.clone(),
                row.last_sequence,
                &summary,
                expanded,
            )];
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
            return blocks;
        }

        let expanded = self.ai_workspace_row_is_expanded(row.id.as_str());
        let mut blocks = vec![ai_workspace_session::AiWorkspaceBlock {
            id: row.id.clone(),
            source_row_id: row.id.clone(),
            role: ai_workspace_session::AiWorkspaceBlockRole::Tool,
            kind: ai_workspace_session::AiWorkspaceBlockKind::Group,
            nested: false,
            mono_preview: false,
            open_side_diff_pane: false,
            expandable: true,
            expanded,
            title: crate::app::ai_workspace_timeline_projection::ai_workspace_format_header_line(
                group.title.as_str(),
                group.summary.as_deref(),
                None,
            ),
            preview: String::new(),
            preferred_review_path: None,
            action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
            copy_text: None,
            copy_tooltip: None,
            copy_success_message: None,
            run_in_terminal_command: None,
            run_in_terminal_cwd: None,
            status_label: None,
            status_color_role: None,
            last_sequence: row.last_sequence,
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
                row.last_sequence,
                self.ai_workspace_row_is_expanded(row.id.as_str()),
            ),
            &mut hasher,
        );

        if self.ai_workspace_row_is_expanded(row.id.as_str()) {
            for child_row_id in &group.child_row_ids {
                if let Some(child_row) = self.ai_timeline_row(child_row_id.as_str()) {
                    std::hash::Hash::hash(
                        &ai_workspace_row_signature(
                            child_row.last_sequence,
                            self.ai_workspace_row_is_expanded(child_row.id.as_str()),
                        ),
                        &mut hasher,
                    );
                }
            }
        }

        std::hash::Hasher::finish(&hasher)
    }
}
