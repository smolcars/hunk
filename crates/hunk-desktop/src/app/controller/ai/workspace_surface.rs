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
            return;
        }

        let blocks = visible_row_ids
            .iter()
            .filter_map(|row_id| self.ai_workspace_block_for_row(row_id.as_str()))
            .collect::<Vec<_>>();
        if self
            .ai_workspace_selection
            .as_ref()
            .is_some_and(|selection| !blocks.iter().any(|block| block.id == selection.block_id))
        {
            self.ai_workspace_selection = None;
        }
        self.ai_workspace_session = Some(ai_workspace_session::AiWorkspaceSession::new(
            thread_id.to_string(),
            Arc::<[ai_workspace_session::AiWorkspaceSourceRow]>::from(source_rows),
            blocks,
        ));
        self.record_ai_workspace_session_rebuild_timing(rebuild_started_at.elapsed());
    }

    fn ai_workspace_block_for_row(
        &self,
        row_id: &str,
    ) -> Option<ai_workspace_session::AiWorkspaceBlock> {
        if let Some(pending) = self.ai_pending_steer_for_row_id(row_id) {
            return Some(ai_workspace_session::AiWorkspaceBlock {
                id: row_id.to_string(),
                source_row_id: row_id.to_string(),
                role: ai_workspace_session::AiWorkspaceBlockRole::User,
                kind: ai_workspace_session::AiWorkspaceBlockKind::Message,
                expandable: false,
                expanded: true,
                title: "You".to_string(),
                preview: ai_workspace_prompt_preview(
                    pending.prompt.as_str(),
                    pending.local_images.as_slice(),
                ),
                last_sequence: ai_workspace_pending_steer_signature(&pending),
            });
        }
        if let Some(queued) = self.ai_queued_message_for_row_id(row_id) {
            return Some(ai_workspace_session::AiWorkspaceBlock {
                id: row_id.to_string(),
                source_row_id: row_id.to_string(),
                role: ai_workspace_session::AiWorkspaceBlockRole::User,
                kind: ai_workspace_session::AiWorkspaceBlockKind::Message,
                expandable: false,
                expanded: true,
                title: match queued.status {
                    AiQueuedUserMessageStatus::Queued => "Queued".to_string(),
                    AiQueuedUserMessageStatus::PendingConfirmation { .. } => {
                        "Pending Confirmation".to_string()
                    }
                },
                preview: ai_workspace_prompt_preview(
                    queued.prompt.as_str(),
                    queued.local_images.as_slice(),
                ),
                last_sequence: ai_workspace_queued_message_signature(&queued),
            });
        }

        let row = self.ai_timeline_row(row_id)?;
        let expanded = self.ai_workspace_row_is_expanded(row.id.as_str());
        match &row.source {
            AiTimelineRowSource::Item { item_key } => {
                let item = self.ai_state_snapshot.items.get(item_key.as_str())?;
                let (kind, role, expandable) =
                    ai_workspace_block_kind_and_role_for_item_kind(item.kind.as_str());
                Some(ai_workspace_session::AiWorkspaceBlock {
                    id: row.id.clone(),
                    source_row_id: row.id.clone(),
                    role,
                    kind,
                    expandable,
                    expanded,
                    title: ai_workspace_item_title(item.kind.as_str()).to_string(),
                    preview: ai_workspace_item_preview_text(item, expanded),
                    last_sequence: row.last_sequence,
                })
            }
            AiTimelineRowSource::Group { group_id } => {
                let group = self.ai_timeline_group(group_id.as_str())?;
                Some(ai_workspace_session::AiWorkspaceBlock {
                    id: row.id.clone(),
                    source_row_id: row.id.clone(),
                    role: ai_workspace_session::AiWorkspaceBlockRole::Tool,
                    kind: ai_workspace_session::AiWorkspaceBlockKind::Group,
                    expandable: false,
                    expanded: false,
                    title: group.title.clone(),
                    preview: group
                        .summary
                        .as_deref()
                        .map(ai_workspace_collapsed_preview_text)
                        .unwrap_or_default(),
                    last_sequence: row.last_sequence,
                })
            }
            AiTimelineRowSource::TurnDiff { turn_key } => {
                let diff = self.ai_state_snapshot.turn_diffs.get(turn_key.as_str())?;
                Some(ai_workspace_session::AiWorkspaceBlock {
                    id: row.id.clone(),
                    source_row_id: row.id.clone(),
                    role: ai_workspace_session::AiWorkspaceBlockRole::Tool,
                    kind: ai_workspace_session::AiWorkspaceBlockKind::DiffSummary,
                    expandable: false,
                    expanded: false,
                    title: "Code Changes".to_string(),
                    preview: ai_workspace_diff_preview(diff),
                    last_sequence: row.last_sequence,
                })
            }
            AiTimelineRowSource::TurnPlan { turn_key } => {
                let plan = self.ai_state_snapshot.turn_plans.get(turn_key.as_str())?;
                Some(ai_workspace_session::AiWorkspaceBlock {
                    id: row.id.clone(),
                    source_row_id: row.id.clone(),
                    role: ai_workspace_session::AiWorkspaceBlockRole::Assistant,
                    kind: ai_workspace_session::AiWorkspaceBlockKind::Plan,
                    expandable: false,
                    expanded: true,
                    title: "Updated Plan".to_string(),
                    preview: ai_workspace_plan_preview(plan),
                    last_sequence: row.last_sequence,
                })
            }
        }
    }

    pub(super) fn ai_select_workspace_selection(
        &mut self,
        selection: ai_workspace_session::AiWorkspaceSelection,
        cx: &mut Context<Self>,
    ) {
        let block_kind = selection.block_kind;
        let selected_block_id = selection.block_id.clone();
        self.ai_workspace_selection = Some(selection);
        self.ai_text_selection = None;
        if block_kind == ai_workspace_session::AiWorkspaceBlockKind::DiffSummary {
            self.ai_open_inline_review_for_row(selected_block_id, cx);
        }
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

    pub(super) fn ai_select_all_workspace_block_text(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((block_id, surfaces)) = self.ai_workspace_selected_block().map(|block| {
            (block.id.clone(), ai_workspace_selection_surfaces(block))
        }) else {
            return false;
        };
        if surfaces.is_empty() {
            return false;
        }

        self.ai_select_all_text_for_surfaces(block_id.as_str(), surfaces, cx)
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
                last_sequence: ai_workspace_row_signature(
                    row.last_sequence,
                    self.ai_workspace_row_is_expanded(row.id.as_str()),
                ),
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

    pub(super) fn ai_open_inline_review_for_row(&mut self, row_id: String, cx: &mut Context<Self>) {
        let Some(thread_id) = self.ai_selected_thread_id.clone() else {
            return;
        };
        let Some(row) = self.ai_timeline_row(row_id.as_str()) else {
            return;
        };
        if !matches!(row.source, AiTimelineRowSource::TurnDiff { .. }) {
            return;
        }

        self.ai_inline_review_selected_row_id_by_thread
            .insert(thread_id, row_id);
        self.ai_sync_review_compare_to_selected_thread(cx);
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

fn ai_workspace_block_kind_and_role_for_item_kind(
    kind: &str,
) -> (
    ai_workspace_session::AiWorkspaceBlockKind,
    ai_workspace_session::AiWorkspaceBlockRole,
    bool,
) {
    match kind {
        "userMessage" => (
            ai_workspace_session::AiWorkspaceBlockKind::Message,
            ai_workspace_session::AiWorkspaceBlockRole::User,
            false,
        ),
        "agentMessage" => (
            ai_workspace_session::AiWorkspaceBlockKind::Message,
            ai_workspace_session::AiWorkspaceBlockRole::Assistant,
            false,
        ),
        "reasoning" => (
            ai_workspace_session::AiWorkspaceBlockKind::Status,
            ai_workspace_session::AiWorkspaceBlockRole::Assistant,
            true,
        ),
        "plan" => (
            ai_workspace_session::AiWorkspaceBlockKind::Plan,
            ai_workspace_session::AiWorkspaceBlockRole::Assistant,
            false,
        ),
        "webSearch"
        | "dynamicToolCall"
        | "mcpToolCall"
        | "collabAgentToolCall"
        | "commandExecution"
        | "fileChange" => (
            ai_workspace_session::AiWorkspaceBlockKind::Tool,
            ai_workspace_session::AiWorkspaceBlockRole::Tool,
            true,
        ),
        _ => (
            ai_workspace_session::AiWorkspaceBlockKind::Status,
            ai_workspace_session::AiWorkspaceBlockRole::System,
            true,
        ),
    }
}

fn ai_workspace_item_title(kind: &str) -> &'static str {
    match kind {
        "userMessage" => "You",
        "agentMessage" => "Assistant",
        "reasoning" => "Thinking",
        "plan" => "Plan",
        "webSearch" => "Search",
        "dynamicToolCall" | "mcpToolCall" | "collabAgentToolCall" => "Tool",
        "commandExecution" => "Command",
        "fileChange" => "Code Changes",
        _ => "Update",
    }
}

fn ai_workspace_item_preview_text(
    item: &hunk_codex::state::ItemSummary,
    expanded: bool,
) -> String {
    match item.kind.as_str() {
        "userMessage" | "agentMessage" => item
            .content
            .trim()
            .is_empty()
            .then(|| {
                item.display_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.summary.as_deref())
                    .map(ai_workspace_full_preview_text)
            })
            .flatten()
            .unwrap_or_else(|| ai_workspace_full_preview_text(item.content.as_str())),
        "reasoning"
        | "webSearch"
        | "dynamicToolCall"
        | "mcpToolCall"
        | "collabAgentToolCall"
        | "commandExecution"
        | "fileChange" => {
            if expanded {
                (!item.content.trim().is_empty())
                    .then(|| ai_workspace_expanded_tool_text(item.content.as_str()))
                    .or_else(|| {
                        item.display_metadata
                            .as_ref()
                            .and_then(|metadata| metadata.summary.as_deref())
                            .map(ai_workspace_full_preview_text)
                    })
                    .unwrap_or_else(|| ai_workspace_item_title(item.kind.as_str()).to_string())
            } else {
                item.display_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.summary.as_deref())
                    .map(ai_workspace_collapsed_preview_text)
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        (!item.content.trim().is_empty())
                            .then(|| ai_workspace_collapsed_preview_text(item.content.as_str()))
                    })
                    .unwrap_or_else(|| ai_workspace_item_title(item.kind.as_str()).to_string())
            }
        }
        _ => item
            .display_metadata
            .as_ref()
            .and_then(|metadata| metadata.summary.as_deref())
            .map(ai_workspace_collapsed_preview_text)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                (!item.content.trim().is_empty())
                    .then(|| ai_workspace_collapsed_preview_text(item.content.as_str()))
            })
            .unwrap_or_else(|| ai_workspace_item_title(item.kind.as_str()).to_string()),
    }
}

fn ai_workspace_plan_preview(plan: &hunk_codex::state::TurnPlanSummary) -> String {
    let mut sections = Vec::new();
    if let Some(explanation) = plan
        .explanation
        .as_deref()
        .map(ai_workspace_full_preview_text)
        .filter(|value| !value.is_empty())
    {
        sections.push(explanation);
    }
    if !plan.steps.is_empty() {
        sections.extend(plan.steps.iter().map(|step| {
            format!(
                "{} {}",
                ai_workspace_plan_step_marker(step.status),
                step.step.trim()
            )
        }));
    }

    if sections.is_empty() {
        "Plan pending".to_string()
    } else {
        sections.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiWorkspaceDiffFileSummary {
    path: String,
    added: usize,
    removed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiWorkspaceDiffSummary {
    files: Vec<AiWorkspaceDiffFileSummary>,
    total_added: usize,
    total_removed: usize,
}

fn ai_workspace_turn_diff_file_header_paths(line: &str) -> Option<(String, String)> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("diff"), Some("--git"), Some(old_path), Some(new_path)) => {
            Some((old_path.to_string(), new_path.to_string()))
        }
        _ => None,
    }
}

fn ai_workspace_turn_diff_display_path(old_path: &str, new_path: &str) -> String {
    let normalized_new = new_path.strip_prefix("b/").unwrap_or(new_path);
    if normalized_new != "/dev/null" {
        return normalized_new.to_string();
    }

    let normalized_old = old_path.strip_prefix("a/").unwrap_or(old_path);
    if normalized_old != "/dev/null" {
        return normalized_old.to_string();
    }

    "changes".to_string()
}

fn ai_workspace_turn_diff_fallback_file(
    files: &mut Vec<AiWorkspaceDiffFileSummary>,
) -> &mut AiWorkspaceDiffFileSummary {
    if files.is_empty() {
        files.push(AiWorkspaceDiffFileSummary {
            path: "changes".to_string(),
            added: 0,
            removed: 0,
        });
    }

    files
        .last_mut()
        .expect("fallback diff file must exist after initialization")
}

fn ai_workspace_turn_diff_summary(diff_text: &str) -> AiWorkspaceDiffSummary {
    let mut files = Vec::new();
    let mut total_added = 0usize;
    let mut total_removed = 0usize;

    for line in diff_text.lines() {
        if let Some((old_path, new_path)) = ai_workspace_turn_diff_file_header_paths(line) {
            files.push(AiWorkspaceDiffFileSummary {
                path: ai_workspace_turn_diff_display_path(old_path.as_str(), new_path.as_str()),
                added: 0,
                removed: 0,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("+++ ") {
            let path = path.strip_prefix("b/").unwrap_or(path);
            let file = ai_workspace_turn_diff_fallback_file(&mut files);
            if file.path == "changes" && path != "/dev/null" {
                file.path = path.to_string();
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("--- ") {
            let path = path.strip_prefix("a/").unwrap_or(path);
            let file = ai_workspace_turn_diff_fallback_file(&mut files);
            if file.path == "changes" && path != "/dev/null" {
                file.path = path.to_string();
            }
            continue;
        }

        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }

        if line.starts_with('+') {
            let file = ai_workspace_turn_diff_fallback_file(&mut files);
            file.added = file.added.saturating_add(1);
            total_added = total_added.saturating_add(1);
            continue;
        }

        if line.starts_with('-') {
            let file = ai_workspace_turn_diff_fallback_file(&mut files);
            file.removed = file.removed.saturating_add(1);
            total_removed = total_removed.saturating_add(1);
        }
    }

    if files.is_empty() && !diff_text.trim().is_empty() {
        files.push(AiWorkspaceDiffFileSummary {
            path: "changes".to_string(),
            added: total_added,
            removed: total_removed,
        });
    }

    AiWorkspaceDiffSummary {
        files,
        total_added,
        total_removed,
    }
}

fn ai_workspace_diff_preview(diff: &str) -> String {
    const AI_WORKSPACE_DIFF_VISIBLE_FILE_LIMIT: usize = 4;

    let summary = ai_workspace_turn_diff_summary(diff);
    if summary.files.is_empty() {
        return "Diff ready".to_string();
    }

    let mut lines = summary
        .files
        .iter()
        .take(AI_WORKSPACE_DIFF_VISIBLE_FILE_LIMIT)
        .map(|file| format!("Edited {}  +{} -{}", file.path, file.added, file.removed))
        .collect::<Vec<_>>();
    let hidden_file_count = summary
        .files
        .len()
        .saturating_sub(AI_WORKSPACE_DIFF_VISIBLE_FILE_LIMIT);
    if hidden_file_count > 0 {
        lines.push(format!("+{hidden_file_count} more files"));
    }
    let file_count_label = if summary.files.len() == 1 {
        "1 file changed".to_string()
    } else {
        format!("{} files changed", summary.files.len())
    };
    lines.push(format!(
        "{file_count_label}, +{} -{}",
        summary.total_added, summary.total_removed
    ));
    lines.join("\n")
}

fn ai_workspace_collapsed_preview_text(value: &str) -> String {
    let normalized = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(8)
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    truncate_ai_workspace_preview(normalized.as_str(), 480)
}

fn ai_workspace_full_preview_text(value: &str) -> String {
    let normalized = value
        .replace("\r\n", "\n")
        .lines()
        .take(160)
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    truncate_ai_workspace_preview(normalized.as_str(), 12_000)
}

fn ai_workspace_expanded_tool_text(value: &str) -> String {
    let normalized = value
        .replace("\r\n", "\n")
        .lines()
        .take(96)
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    truncate_ai_workspace_preview(normalized.as_str(), 8_000)
}

fn truncate_ai_workspace_preview(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    let mut end = max_len;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let trimmed = value[..end].trim_end();
    format!("{trimmed}...")
}

fn ai_workspace_prompt_preview(prompt: &str, local_images: &[PathBuf]) -> String {
    let prompt = prompt.trim();
    let image_names = local_images
        .iter()
        .map(|path| ai_pending_steer_local_image_name(path.as_path()))
        .collect::<Vec<_>>();

    let mut content = String::new();
    if !prompt.is_empty() {
        content.push_str(prompt);
    }
    if !image_names.is_empty() {
        if !content.is_empty() {
            content.push('\n');
        }
        let prefix = if image_names.len() == 1 {
            "[image] "
        } else {
            "[images] "
        };
        content.push_str(prefix);
        content.push_str(image_names.join(", ").as_str());
    }
    if content.is_empty() {
        return "Message pending".to_string();
    }

    ai_workspace_full_preview_text(content.as_str())
}

fn ai_workspace_selection_surfaces(
    block: &ai_workspace_session::AiWorkspaceBlock,
) -> Arc<[AiTextSelectionSurfaceSpec]> {
    let mut surfaces = Vec::with_capacity(2);
    if !block.title.is_empty() {
        surfaces.push(AiTextSelectionSurfaceSpec::new(
            format!("ai-workspace:{}:title", block.id),
            block.title.clone(),
        ));
    }
    if !block.preview.is_empty() {
        let surface = AiTextSelectionSurfaceSpec::new(
            format!("ai-workspace:{}:preview", block.id),
            block.preview.clone(),
        );
        surfaces.push(if surfaces.is_empty() {
            surface
        } else {
            surface.with_separator_before("\n")
        });
    }

    Arc::<[AiTextSelectionSurfaceSpec]>::from(surfaces)
}

fn ai_workspace_selection_index(
    current_index: Option<usize>,
    block_count: usize,
    delta: isize,
) -> Option<usize> {
    if block_count == 0 {
        return None;
    }

    let baseline = current_index.unwrap_or_else(|| {
        if delta.is_negative() {
            block_count.saturating_sub(1)
        } else {
            0
        }
    });
    let next_index = baseline.saturating_add_signed(delta);
    Some(next_index.min(block_count.saturating_sub(1)))
}

fn ai_workspace_pending_steer_signature(pending: &AiPendingSteer) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&pending.thread_id, &mut hasher);
    std::hash::Hash::hash(&pending.turn_id, &mut hasher);
    std::hash::Hash::hash(&pending.prompt, &mut hasher);
    std::hash::Hash::hash(&pending.accepted_after_sequence, &mut hasher);
    for image in &pending.local_images {
        std::hash::Hash::hash(&image, &mut hasher);
    }
    std::hash::Hasher::finish(&hasher)
}

fn ai_workspace_queued_message_signature(queued: &AiQueuedUserMessage) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&queued.thread_id, &mut hasher);
    std::hash::Hash::hash(&queued.prompt, &mut hasher);
    for image in &queued.local_images {
        std::hash::Hash::hash(&image, &mut hasher);
    }
    match queued.status {
        AiQueuedUserMessageStatus::Queued => std::hash::Hash::hash(&0u64, &mut hasher),
        AiQueuedUserMessageStatus::PendingConfirmation {
            accepted_after_sequence,
        } => std::hash::Hash::hash(&accepted_after_sequence, &mut hasher),
    }
    std::hash::Hasher::finish(&hasher)
}

fn ai_workspace_row_signature(last_sequence: u64, expanded: bool) -> u64 {
    last_sequence
        .wrapping_shl(1)
        .wrapping_add(u64::from(expanded))
}

fn ai_workspace_plan_step_marker(status: hunk_codex::state::TurnPlanStepStatus) -> &'static str {
    match status {
        hunk_codex::state::TurnPlanStepStatus::Pending => "[ ]",
        hunk_codex::state::TurnPlanStepStatus::InProgress => "[>]",
        hunk_codex::state::TurnPlanStepStatus::Completed => "[x]",
    }
}
