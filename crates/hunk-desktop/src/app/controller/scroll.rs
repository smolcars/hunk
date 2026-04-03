impl DiffViewer {
    fn scroll_selected_file_to_top(&mut self) {
        let target_path = if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.current_review_file_range().map(|range| range.path)
        } else {
            self.selected_path.clone()
        };
        let Some(path) = target_path else {
            return;
        };
        self.scroll_to_file_start(&path);
    }

    fn scroll_to_file_start(&mut self, path: &str) {
        let start_row = self
            .active_diff_file_range_for_path(path)
            .map(|range| range.start_row);
        let Some(start_row) = start_row else {
            return;
        };

        if self.uses_review_workspace_sections_surface()
            && let Some(session) = self.review_workspace_session.as_ref()
            && let Some(section_ix) = session.section_index_for_path(path)
        {
            self.review_surface
                .diff_scroll_handle
                .scroll_to_top_of_item(section_ix);
        } else {
            self.review_surface.diff_list_state.scroll_to(ListOffset {
                item_ix: start_row,
                offset_in_item: px(0.),
            });
        }
        self.review_surface.last_diff_scroll_offset = None;
        self.last_scroll_activity_at = Instant::now();
    }

    pub(super) fn sync_selected_file_from_visible_row(
        &mut self,
        row_ix: usize,
        cx: &mut Context<Self>,
    ) {
        if self.review_surface.last_visible_row_start == Some(row_ix) {
            return;
        }
        self.review_surface.last_visible_row_start = Some(row_ix);
        self.request_visible_row_segment_prefetch(row_ix, false, cx);

        let Some((next_path, next_status)) = (if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.review_workspace_session
                .as_ref()
                .and_then(|session| {
                    session
                        .path_at_surface_row(row_ix)
                        .and_then(|path| {
                            session.file_range_for_path(path).map(|range| {
                                (range.path.clone(), range.status)
                            })
                        })
                        .or_else(|| {
                            self.active_diff_file_range_at_or_after_row(row_ix)
                                .map(|range| (range.path, range.status))
                        })
                })
                .or_else(|| {
                    self.selected_file_from_row_metadata(row_ix).or_else(|| {
                        self.active_diff_file_range_at_or_after_row(row_ix)
                            .map(|range| (range.path, range.status))
                    })
                })
        } else {
            self.selected_file_from_row_metadata(row_ix).or_else(|| {
                self.active_diff_file_range_at_or_after_row(row_ix)
                    .map(|range| (range.path, range.status))
            })
        })
        else {
            return;
        };

        if self.selected_path.as_deref() == Some(next_path.as_str()) {
            return;
        }

        self.selected_path = Some(next_path);
        self.selected_status = Some(next_status);
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.review_last_selected_path = self.selected_path.clone();
        }
        self.sync_review_workspace_editor_active_path();
        cx.notify();
    }

    fn request_visible_row_segment_prefetch(
        &mut self,
        visible_row: usize,
        force_upgrade: bool,
        cx: &mut Context<Self>,
    ) {
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            return;
        }

        if self.workspace_view_mode != WorkspaceViewMode::Diff
            && self.diff_row_segment_cache.len() != row_count
        {
            self.diff_row_segment_cache.resize(row_count, None);
        }

        if !force_upgrade
            && let Some(anchor_row) = self.segment_prefetch_anchor_row
            && anchor_row.abs_diff(visible_row) < DIFF_SEGMENT_PREFETCH_STEP_ROWS
        {
            return;
        }

        self.segment_prefetch_anchor_row = Some(visible_row);
        let start = visible_row.saturating_sub(DIFF_SEGMENT_PREFETCH_RADIUS_ROWS);
        let end = visible_row
            .saturating_add(DIFF_SEGMENT_PREFETCH_RADIUS_ROWS.saturating_add(1))
            .min(row_count);

        let batch_limit = if force_upgrade {
            end.saturating_sub(start)
        } else {
            DIFF_SEGMENT_PREFETCH_BATCH_ROWS.min(end.saturating_sub(start))
        };
        let mut pending_rows = Vec::with_capacity(batch_limit);
        let recently_scrolling = self.recently_scrolling();
        for row_ix in prioritized_prefetch_row_indices(start, end, visible_row) {
            if pending_rows.len() >= batch_limit {
                break;
            }

            let Some(row) = self.active_diff_row(row_ix) else {
                continue;
            };
            if row.kind != DiffRowKind::Code {
                continue;
            }

            let file_path = self
                .active_diff_row_metadata(row_ix)
                .and_then(|meta| meta.file_path.clone());
            let base_quality = file_path
                .as_deref()
                .and_then(|path| self.active_diff_file_line_stats().get(path).copied())
                .map(base_segment_quality_for_file)
                .unwrap_or(DiffSegmentQuality::Detailed);
            let target_quality = effective_segment_quality(base_quality, recently_scrolling);

            if self
                .active_diff_row_segment_cache(row_ix)
                .is_some_and(|cache| cache.quality >= target_quality)
            {
                continue;
            }

            pending_rows.push((
                row_ix,
                row.left.text.clone(),
                row.left.kind,
                row.right.text.clone(),
                row.right.kind,
                file_path,
                target_quality,
            ));
        }

        if pending_rows.is_empty() {
            return;
        }

        let epoch = self.next_segment_prefetch_epoch();
        self.segment_prefetch_task = cx.spawn(async move |this, cx| {
            let computed_rows = cx
                .background_executor()
                .spawn(async move {
                    pending_rows
                        .into_iter()
                        .map(
                            |(
                                row_ix,
                                left_text,
                                left_kind,
                                right_text,
                                right_kind,
                                file_path,
                                quality,
                            )| {
                            (
                                row_ix,
                                build_diff_row_segment_cache_from_cells(
                                    file_path.as_deref(),
                                    left_text.as_str(),
                                    left_kind,
                                    right_text.as_str(),
                                    right_kind,
                                    quality,
                                ),
                            )
                        },
                        )
                        .collect::<Vec<_>>()
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.segment_prefetch_epoch {
                        return;
                    }

                    let mut inserted = false;
                    let update_review_session = this.workspace_view_mode == WorkspaceViewMode::Diff
                        && this.review_workspace_session.is_some();
                    for (row_ix, row_cache) in computed_rows {
                        if update_review_session {
                            if let Some(session) = this.review_workspace_session.as_mut()
                                && session.set_row_segment_cache_if_better(row_ix, row_cache)
                            {
                                inserted = true;
                            }
                        } else if let Some(slot) = this.diff_row_segment_cache.get_mut(row_ix) {
                            let should_replace = slot
                                .as_ref()
                                .map(|cached: &DiffRowSegmentCache| row_cache.quality > cached.quality)
                                .unwrap_or(true);
                            if should_replace {
                                *slot = Some(row_cache);
                                inserted = true;
                            }
                        }
                    }

                    if inserted {
                        cx.notify();
                    }
                });
            }
        });
    }

    fn selected_file_from_row_metadata(&self, row_ix: usize) -> Option<(String, FileStatus)> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff
            && let Some(session) = self.review_workspace_session.as_ref()
            && let Some(path) = session.path_at_surface_row(row_ix)
        {
            let status = session
                .status_for_path(path)
                .or_else(|| self.status_for_path(path))?;
            return Some((path.to_string(), status));
        }

        let row = self.diff_row_metadata.get(row_ix)?;
        if row.kind == DiffStreamRowKind::EmptyState {
            return None;
        }

        let path = row.file_path.clone()?;
        let status = row
            .file_status
            .or_else(|| self.status_for_path(path.as_str()))?;

        Some((path, status))
    }

    pub(super) fn on_diff_list_scroll_wheel(
        &mut self,
        _: &ScrollWheelEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.last_scroll_activity_at = Instant::now();
    }

    fn prime_diff_surface_visible_state(
        &mut self,
        force_reprime: bool,
        cx: &mut Context<Self>,
    ) {
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            return;
        }

        let visible_row = self
            .current_review_surface_top_row()
            .unwrap_or(0)
            .min(row_count.saturating_sub(1));
        if force_reprime {
            self.review_surface.last_visible_row_start = None;
        }
        self.sync_selected_file_from_visible_row(visible_row, cx);
    }

    fn reset_diff_surface_rows(&mut self, rows: Vec<SideBySideRow>) {
        self.diff_rows = rows;
        self.diff_row_metadata.clear();
        self.diff_row_segment_cache.clear();
        self.invalidate_segment_prefetch();
        self.review_surface.diff_visible_file_header_lookup.clear();
        self.review_surface.diff_visible_hunk_header_lookup.clear();
        self.file_row_ranges.clear();
        self.selection_anchor_row = None;
        self.selection_head_row = None;
        self.drag_selecting_rows = false;
        self.sync_diff_list_state();
        self.recompute_diff_layout();
    }

    fn apply_loaded_diff_surface_stream(
        &mut self,
        stream: DiffStream,
    ) -> BTreeMap<String, LineStats> {
        let DiffStream {
            rows,
            row_metadata,
            row_segments,
            file_ranges,
            file_line_stats,
        } = stream;

        self.invalidate_segment_prefetch();
        self.diff_rows = rows;
        self.diff_row_metadata = row_metadata;
        self.diff_row_segment_cache = row_segments;
        self.clamp_comment_rows_to_diff();
        self.clamp_selection_to_rows();
        self.drag_selecting_rows = false;
        self.sync_diff_list_state();
        self.file_row_ranges = file_ranges;
        self.recompute_diff_layout();
        self.review_surface.last_visible_row_start = None;
        self.recompute_diff_visible_header_lookup();
        file_line_stats
    }

    fn apply_loaded_review_workspace_surface(&mut self) {
        self.invalidate_segment_prefetch();
        self.clamp_comment_rows_to_diff();
        self.clamp_selection_to_rows();
        self.drag_selecting_rows = false;
        self.sync_diff_list_state();
        self.recompute_diff_layout();
        self.review_surface.last_visible_row_start = None;
        self.recompute_diff_visible_header_lookup();
    }

    fn recompute_diff_layout(&mut self) {
        let mut max_left_line_digits = DIFF_LINE_NUMBER_MIN_DIGITS;
        let mut max_right_line_digits = DIFF_LINE_NUMBER_MIN_DIGITS;

        for row_ix in 0..self.active_diff_row_count() {
            let Some(row) = self.active_diff_row(row_ix) else {
                continue;
            };
            if row.kind != DiffRowKind::Code {
                continue;
            }
            if let Some(line) = row.left.line {
                max_left_line_digits = max_left_line_digits.max(decimal_digits(line));
            }
            if let Some(line) = row.right.line {
                max_right_line_digits = max_right_line_digits.max(decimal_digits(line));
            }
        }

        self.review_surface.diff_left_line_number_width =
            line_number_column_width(max_left_line_digits);
        self.review_surface.diff_right_line_number_width =
            line_number_column_width(max_right_line_digits);
    }

    fn sync_diff_list_state(&self) {
        if self.uses_review_workspace_sections_surface() {
            return;
        }

        let previous_top = self.review_surface.diff_list_state.logical_scroll_top();
        let row_count = self.active_diff_row_count();
        self.review_surface.diff_list_state.reset(row_count);
        let clamped_item_ix = if row_count == 0 {
            0
        } else {
            previous_top
                .item_ix
                .min(row_count.saturating_sub(1))
        };
        let offset_in_item = if row_count == 0 || clamped_item_ix != previous_top.item_ix {
            px(0.)
        } else {
            previous_top.offset_in_item
        };
        self.review_surface.diff_list_state.scroll_to(ListOffset {
            item_ix: clamped_item_ix,
            offset_in_item,
        });
    }
}

fn prioritized_prefetch_row_indices(start: usize, end: usize, anchor_row: usize) -> Vec<usize> {
    if start >= end {
        return Vec::new();
    }

    let anchor = anchor_row.clamp(start, end.saturating_sub(1));
    let mut rows = Vec::with_capacity(end.saturating_sub(start));
    rows.push(anchor);

    let mut step = 1usize;
    while rows.len() < end.saturating_sub(start) {
        let mut inserted = false;

        if let Some(right) = anchor.checked_add(step)
            && right < end
        {
            rows.push(right);
            inserted = true;
        }

        if let Some(left) = anchor.checked_sub(step)
            && left >= start
        {
            rows.push(left);
            inserted = true;
        }

        if !inserted {
            break;
        }
        step = step.saturating_add(1);
    }

    rows
}
