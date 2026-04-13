const AI_WORKSPACE_STREAMING_REVEAL_TICK_MS: u64 = 16;
const AI_WORKSPACE_STREAMING_REVEAL_TARGET_MS: u64 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiWorkspaceStreamingPreview {
    target_preview: String,
    bytes_per_tick: usize,
}

impl AiWorkspaceSession {
    pub(crate) fn has_pending_streaming_preview(&self) -> bool {
        !self.streaming_previews_by_block_id.is_empty()
    }

    pub(crate) fn replace_source_row(
        &mut self,
        source_row: AiWorkspaceSourceRow,
        next_blocks: Vec<AiWorkspaceBlock>,
        smooth_preview: bool,
    ) -> bool {
        let Some(range) = self.source_row_block_ranges.get(source_row.row_id.as_str()).cloned() else {
            return false;
        };
        if range.len() != next_blocks.len() {
            return false;
        }

        let row_index = self
            .source_rows
            .iter()
            .position(|entry| entry.row_id == source_row.row_id);
        let Some(row_index) = row_index else {
            return false;
        };

        let mut changed_block_indexes = Vec::new();
        let mut selection_surfaces_changed = false;

        for (offset, mut next_block) in next_blocks.into_iter().enumerate() {
            let block_index = range.start + offset;
            let Some(current_block) = self.blocks.get(block_index).cloned() else {
                return false;
            };
            if current_block.id != next_block.id {
                return false;
            }

            let pending_preview = smooth_preview
                .then(|| ai_workspace_streaming_preview_update(&current_block, &next_block))
                .flatten();
            if let Some(pending_preview) = pending_preview {
                let copy_tracks_preview =
                    next_block.copy_text.as_deref() == Some(next_block.preview.as_str());
                next_block.preview = current_block.preview.clone();
                if copy_tracks_preview {
                    next_block.copy_text = Some(current_block.preview.clone());
                }
                self.streaming_previews_by_block_id
                    .insert(next_block.id.clone(), pending_preview);
            } else {
                self.streaming_previews_by_block_id.remove(next_block.id.as_str());
            }

            if current_block != next_block {
                selection_surfaces_changed |= current_block.title != next_block.title
                    || current_block.preview != next_block.preview;
                self.blocks[block_index] = next_block;
                changed_block_indexes.push(block_index);
            }
        }

        if let Some(source_rows) = Arc::get_mut(&mut self.source_rows) {
            source_rows[row_index] = source_row;
        } else {
            let mut source_rows = self.source_rows.to_vec();
            source_rows[row_index] = source_row;
            self.source_rows = Arc::<[AiWorkspaceSourceRow]>::from(source_rows);
        }

        if changed_block_indexes.is_empty() {
            return true;
        }

        self.update_geometry_for_block_changes(changed_block_indexes.as_slice());
        if selection_surfaces_changed {
            self.selection_surfaces_by_width_bucket.clear();
        }
        true
    }

    pub(crate) fn reveal_pending_streaming_preview_step(&mut self) -> bool {
        let pending_block_ids = self
            .streaming_previews_by_block_id
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut changed_block_indexes = Vec::new();
        let mut selection_surfaces_changed = false;

        for block_id in pending_block_ids {
            let Some(block_index) = self.block_index(block_id.as_str()) else {
                self.streaming_previews_by_block_id.remove(block_id.as_str());
                continue;
            };
            let Some(pending_preview) = self
                .streaming_previews_by_block_id
                .get(block_id.as_str())
                .cloned()
            else {
                continue;
            };
            let Some(current_block) = self.blocks.get(block_index).cloned() else {
                self.streaming_previews_by_block_id.remove(block_id.as_str());
                continue;
            };

            let next_preview =
                ai_workspace_streaming_reveal_preview(current_block.preview.as_str(), &pending_preview);
            if next_preview == current_block.preview {
                continue;
            }

            let reveal_completed = next_preview == pending_preview.target_preview;
            let mut next_block = current_block.clone();
            let copy_tracks_preview =
                next_block.copy_text.as_deref() == Some(current_block.preview.as_str());
            next_block.preview = next_preview;
            if copy_tracks_preview {
                next_block.copy_text = Some(next_block.preview.clone());
            }
            selection_surfaces_changed |= reveal_completed;
            self.blocks[block_index] = next_block;
            changed_block_indexes.push(block_index);

            if reveal_completed {
                self.streaming_previews_by_block_id.remove(block_id.as_str());
            }
        }

        if changed_block_indexes.is_empty() {
            return false;
        }

        self.update_geometry_for_block_changes(changed_block_indexes.as_slice());
        if selection_surfaces_changed {
            self.selection_surfaces_by_width_bucket.clear();
        }
        true
    }

    fn update_geometry_for_block_changes(&mut self, changed_block_indexes: &[usize]) {
        if changed_block_indexes.is_empty() {
            return;
        }

        let mut changed_block_indexes = changed_block_indexes.to_vec();
        changed_block_indexes.sort_unstable();
        changed_block_indexes.dedup();

        for (width_bucket, geometry) in &mut self.geometry_by_width_bucket {
            let mut total_height_delta = 0isize;
            for &block_index in &changed_block_indexes {
                let Some(block) = self.blocks.get(block_index) else {
                    continue;
                };
                let Some(entry) = geometry.blocks.get_mut(block_index) else {
                    continue;
                };

                let next_height_px =
                    ai_workspace_text_layout_for_block(block, *width_bucket).height_px;
                let previous_height_px = entry.height_px;
                if next_height_px == previous_height_px {
                    continue;
                }

                let height_delta = next_height_px as isize - previous_height_px as isize;
                entry.height_px = next_height_px;
                total_height_delta += height_delta;
                for shifted_entry in geometry.blocks.iter_mut().skip(block_index + 1) {
                    shifted_entry.top_px = shifted_entry.top_px.saturating_add_signed(height_delta);
                }
            }

            if total_height_delta != 0 {
                geometry.total_surface_height_px = geometry
                    .total_surface_height_px
                    .saturating_add_signed(total_height_delta);
                self.selection_surfaces_by_width_bucket.remove(width_bucket);
            }
        }
    }
}

fn ai_workspace_streaming_preview_update(
    current_block: &AiWorkspaceBlock,
    next_block: &AiWorkspaceBlock,
) -> Option<AiWorkspaceStreamingPreview> {
    if current_block.preview == next_block.preview {
        return None;
    }
    if current_block.title != next_block.title
        || current_block.kind != next_block.kind
        || current_block.role != next_block.role
        || current_block.mono_preview != next_block.mono_preview
        || current_block.expandable != next_block.expandable
        || current_block.expanded != next_block.expanded
    {
        return None;
    }
    if !next_block.preview.starts_with(current_block.preview.as_str()) {
        return None;
    }

    let remaining_bytes = next_block
        .preview
        .len()
        .saturating_sub(current_block.preview.len());
    if remaining_bytes == 0 {
        return None;
    }

    let steps = AI_WORKSPACE_STREAMING_REVEAL_TARGET_MS
        .div_ceil(AI_WORKSPACE_STREAMING_REVEAL_TICK_MS)
        .max(1) as usize;
    Some(AiWorkspaceStreamingPreview {
        target_preview: next_block.preview.clone(),
        bytes_per_tick: remaining_bytes.div_ceil(steps).max(1),
    })
}

fn ai_workspace_streaming_reveal_preview(
    current_preview: &str,
    pending_preview: &AiWorkspaceStreamingPreview,
) -> String {
    let current_len = current_preview.len();
    if current_len >= pending_preview.target_preview.len() {
        return pending_preview.target_preview.clone();
    }

    let next_len = ai_workspace_streaming_next_boundary(
        pending_preview.target_preview.as_str(),
        current_len,
        pending_preview.bytes_per_tick,
    );
    pending_preview.target_preview[..next_len].to_string()
}

fn ai_workspace_streaming_next_boundary(
    preview: &str,
    current_len: usize,
    bytes_per_tick: usize,
) -> usize {
    let target_len = preview.len().min(current_len.saturating_add(bytes_per_tick));
    if target_len >= preview.len() {
        return preview.len();
    }
    if preview.is_char_boundary(target_len) {
        return target_len;
    }

    let mut next_len = target_len;
    while next_len < preview.len() && !preview.is_char_boundary(next_len) {
        next_len += 1;
    }
    next_len.min(preview.len())
}
