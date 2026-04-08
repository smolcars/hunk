fn ai_workspace_message_preview(item: &hunk_codex::state::ItemSummary) -> String {
    item.content
        .trim()
        .is_empty()
        .then(|| {
            item.display_metadata
                .as_ref()
                .and_then(|metadata| metadata.summary.as_deref())
                .map(ai_workspace_full_preview_text)
        })
        .flatten()
        .unwrap_or_else(|| ai_workspace_full_preview_text(item.content.as_str()))
}

fn ai_workspace_tool_header_line(
    item: &hunk_codex::state::ItemSummary,
    content_text: &str,
) -> String {
    let title = crate::app::ai_workspace_timeline_projection::ai_workspace_tool_header_title(item);
    let summary = crate::app::ai_workspace_timeline_projection::ai_workspace_tool_compact_summary(
        item,
        content_text,
    );
    let status = (item.status != hunk_codex::state::ItemStatus::Completed).then(|| {
        crate::app::ai_workspace_timeline_projection::ai_workspace_item_status_label(item.status)
    });

    crate::app::ai_workspace_timeline_projection::ai_workspace_format_header_line(
        title.as_str(),
        summary.as_deref(),
        status,
    )
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

fn ai_workspace_diff_block(
    block_id: String,
    source_row_id: String,
    last_sequence: u64,
    summary: &crate::app::ai_workspace_timeline_projection::AiWorkspaceDiffSummary,
    preferred_review_path: Option<String>,
    nested: bool,
) -> ai_workspace_session::AiWorkspaceBlock {
    let preview =
        crate::app::ai_workspace_timeline_projection::ai_workspace_diff_summary_preview(summary);
    let mut preview_lines = preview.lines();
    let title = preview_lines
        .next()
        .map(str::to_string)
        .unwrap_or_else(|| "Code Changes".to_string());
    let preview = preview_lines.collect::<Vec<_>>().join("\n");

    ai_workspace_session::AiWorkspaceBlock {
        id: block_id,
        source_row_id,
        role: ai_workspace_session::AiWorkspaceBlockRole::Tool,
        kind: ai_workspace_session::AiWorkspaceBlockKind::DiffSummary,
        nested,
        mono_preview: false,
        open_side_diff_pane: true,
        expandable: false,
        expanded: true,
        title,
        preview,
        preferred_review_path,
        action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
        copy_text: None,
        copy_tooltip: None,
        copy_success_message: None,
        run_in_terminal_command: None,
        run_in_terminal_cwd: None,
        status_label: None,
        status_color_role: None,
        last_sequence,
    }
}

fn ai_workspace_file_change_batch_group_block(
    block_id: String,
    source_row_id: String,
    last_sequence: u64,
    summary: &crate::app::ai_workspace_timeline_projection::AiWorkspaceDiffSummary,
    expanded: bool,
) -> ai_workspace_session::AiWorkspaceBlock {
    let total_files = summary.files.len();
    let title = if total_files == 1 {
        "Changed file".to_string()
    } else {
        format!("Changed files ({total_files})")
    };
    let preview = format!(
        "{}  +{}  -{}",
        if total_files == 1 {
            "1 file changed".to_string()
        } else {
            format!("{total_files} files changed")
        },
        summary.total_added,
        summary.total_removed,
    );

    ai_workspace_session::AiWorkspaceBlock {
        id: block_id,
        source_row_id,
        role: ai_workspace_session::AiWorkspaceBlockRole::Tool,
        kind: ai_workspace_session::AiWorkspaceBlockKind::Group,
        nested: false,
        mono_preview: false,
        open_side_diff_pane: true,
        expandable: true,
        expanded,
        title,
        preview,
        preferred_review_path: summary.files.first().map(|file| file.path.clone()),
        action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
        copy_text: None,
        copy_tooltip: None,
        copy_success_message: None,
        run_in_terminal_command: None,
        run_in_terminal_cwd: None,
        status_label: None,
        status_color_role: None,
        last_sequence,
    }
}

fn ai_workspace_file_change_group_summary(
    this: &DiffViewer,
    group: &AiTimelineGroup,
) -> Option<crate::app::ai_workspace_timeline_projection::AiWorkspaceDiffSummary> {
    let mut summary = crate::app::ai_workspace_timeline_projection::AiWorkspaceDiffSummary {
        files: Vec::new(),
        total_added: 0,
        total_removed: 0,
    };

    for child_row_id in &group.child_row_ids {
        let row = this.ai_timeline_row(child_row_id.as_str())?;
        let AiTimelineRowSource::Item { item_key } = &row.source else {
            continue;
        };
        let item = this.ai_state_snapshot.items.get(item_key.as_str())?;
        let item_summary =
            crate::app::ai_workspace_timeline_projection::ai_workspace_file_change_summary(item)?;
        for file in item_summary.files {
            summary.total_added = summary.total_added.saturating_add(file.added);
            summary.total_removed = summary.total_removed.saturating_add(file.removed);
            if let Some(existing) = summary.files.iter_mut().find(|entry| entry.path == file.path) {
                existing.added = existing.added.saturating_add(file.added);
                existing.removed = existing.removed.saturating_add(file.removed);
            } else {
                summary.files.push(file);
            }
        }
    }

    (!summary.files.is_empty()).then_some(summary)
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

#[cfg(test)]
fn ai_workspace_selection_surfaces(
    block: &ai_workspace_session::AiWorkspaceBlock,
) -> Arc<[AiTextSelectionSurfaceSpec]> {
    let mut surfaces = Vec::with_capacity(2);
    if !block.title.is_empty() {
        surfaces.push(AiTextSelectionSurfaceSpec::new(
            format!("ai-workspace:{}:title", block.id),
            block.title.clone(),
        )
        .with_row_id(block.source_row_id.clone()));
    }
    if !block.preview.is_empty() {
        let surface = AiTextSelectionSurfaceSpec::new(
            format!("ai-workspace:{}:preview", block.id),
            block.preview.clone(),
        )
        .with_row_id(block.source_row_id.clone());
        surfaces.push(if surfaces.is_empty() {
            surface
        } else {
            surface.with_separator_before("\n")
        });
    }

    Arc::<[AiTextSelectionSurfaceSpec]>::from(surfaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_change_batch_group_prefers_first_review_path() {
        let summary = crate::app::ai_workspace_timeline_projection::AiWorkspaceDiffSummary {
            files: vec![
                crate::app::ai_workspace_timeline_projection::AiWorkspaceDiffSummaryFile {
                    path: "src/main.rs".to_string(),
                    added: 3,
                    removed: 1,
                },
                crate::app::ai_workspace_timeline_projection::AiWorkspaceDiffSummaryFile {
                    path: "src/lib.rs".to_string(),
                    added: 2,
                    removed: 2,
                },
            ],
            total_added: 5,
            total_removed: 3,
        };

        let block = ai_workspace_file_change_batch_group_block(
            "row-group".to_string(),
            "row-group".to_string(),
            7,
            &summary,
            false,
        );

        assert_eq!(
            block.preferred_review_path.as_deref(),
            Some("src/main.rs")
        );
        assert!(block.open_side_diff_pane);
    }
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

fn ai_workspace_command_status_color_role(
    details: Option<&crate::app::ai_workspace_timeline_projection::AiWorkspaceCommandExecutionDisplayDetails>,
) -> ai_workspace_session::AiWorkspacePreviewColorRole {
    let Some(details) = details else {
        return ai_workspace_session::AiWorkspacePreviewColorRole::Muted;
    };
    match details.exit_code {
        Some(0) => ai_workspace_session::AiWorkspacePreviewColorRole::Added,
        Some(_) => ai_workspace_session::AiWorkspacePreviewColorRole::Removed,
        None => match details.status.as_str() {
            "started" | "running" | "streaming" => {
                ai_workspace_session::AiWorkspacePreviewColorRole::Accent
            }
            _ => ai_workspace_session::AiWorkspacePreviewColorRole::Muted,
        },
    }
}
