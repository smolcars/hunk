type DiffSegmentPrefetchJob = (
    usize,
    String,
    DiffCellKind,
    String,
    DiffCellKind,
    Option<String>,
    DiffSegmentQuality,
);

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
        if let Some(session) = self.review_workspace_session.as_ref()
            && let Some(top_offset_px) = session
                .file_range_for_path(path)
                .and_then(|range| session.row_top_offset_px(range.start_row))
        {
            self.review_surface
                .diff_scroll_handle
                .set_offset(point(px(0.), -px(top_offset_px as f32)));
        }
        self.review_surface.last_diff_scroll_offset = None;
        self.last_scroll_activity_at = Instant::now();
    }

    pub(super) fn sync_selected_file_from_visible_row(
        &mut self,
        row_ix: usize,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.sync_review_workspace_editor_selection_for_row(row_ix);
        }

        if self.uses_review_workspace_sections_surface()
            && let Some(visible_range) = self.current_review_visible_row_range()
        {
            self.request_visible_row_range_segment_prefetch(visible_range, false, cx);
        }

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

        let current_selected_path = if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.current_review_path()
        } else {
            self.selected_path.clone()
        };

        if current_selected_path.as_deref() == Some(next_path.as_str()) {
            return;
        }

        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.set_review_selected_file(Some(next_path), Some(next_status));
        } else {
            self.selected_path = Some(next_path);
            self.selected_status = Some(next_status);
        }
        cx.notify();
    }

    fn request_review_visible_row_range_segment_prefetch(
        &mut self,
        visible_range: std::ops::Range<usize>,
        force_upgrade: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.review_workspace_session.as_ref() else {
            return;
        };

        let row_count = session.row_count();
        if row_count == 0 || visible_range.start >= row_count {
            return;
        }

        let clamped_range = visible_range.start.min(row_count)..visible_range.end.min(row_count);
        if clamped_range.is_empty() {
            return;
        }

        if !force_upgrade
            && self
                .review_surface
                .last_prefetched_visible_row_range
                .as_ref()
                .is_some_and(|previous| {
                    previous.start.abs_diff(clamped_range.start) < DIFF_SEGMENT_PREFETCH_STEP_ROWS
                        && previous.end.abs_diff(clamped_range.end)
                            < DIFF_SEGMENT_PREFETCH_STEP_ROWS
                })
        {
            return;
        }

        self.review_surface.last_prefetched_visible_row_range = Some(clamped_range.clone());
        let anchor_row =
            clamped_range.start + (clamped_range.end.saturating_sub(clamped_range.start) / 2);

        let pending_rows = session
            .build_segment_prefetch_rows(
                review_workspace_session::ReviewWorkspaceSegmentPrefetchRequest {
                    scroll_top_px: self.current_review_surface_scroll_top_px(),
                    viewport_height_px: self
                        .review_surface
                        .diff_scroll_handle
                        .bounds()
                        .size
                        .height
                        .max(Pixels::ZERO)
                        .as_f32()
                        .round() as usize,
                    anchor_row,
                    overscan_rows: DIFF_SEGMENT_PREFETCH_RADIUS_ROWS,
                    force_upgrade,
                    recently_scrolling: self.recently_scrolling(),
                    batch_limit: DIFF_SEGMENT_PREFETCH_BATCH_ROWS,
                },
            )
            .into_iter()
            .map(|row| {
                (
                    row.row_index,
                    row.left_text,
                    row.left_kind,
                    row.right_text,
                    row.right_kind,
                    row.file_path,
                    row.quality,
                )
            })
            .collect::<Vec<_>>();

        self.spawn_review_segment_prefetch_task(pending_rows, cx);
    }

    fn spawn_review_segment_prefetch_task(
        &mut self,
        pending_rows: Vec<DiffSegmentPrefetchJob>,
        cx: &mut Context<Self>,
    ) {
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
                    for (row_ix, row_cache) in computed_rows {
                        if let Some(session) = this.review_workspace_session.as_mut()
                            && session.set_row_segment_cache_if_better(row_ix, row_cache)
                        {
                            inserted = true;
                        }
                    }

                    if inserted {
                        cx.notify();
                    }
                });
            }
        });
    }

    fn request_visible_row_range_segment_prefetch(
        &mut self,
        visible_range: std::ops::Range<usize>,
        force_upgrade: bool,
        cx: &mut Context<Self>,
    ) {
        self.request_review_visible_row_range_segment_prefetch(visible_range, force_upgrade, cx);
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

        let row = self.active_diff_row_metadata(row_ix)?;
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
            self.review_surface.clear_workspace_surface_snapshot();
            self.review_surface.last_prefetched_visible_row_range = None;
        }
        self.sync_selected_file_from_visible_row(visible_row, cx);
    }

    fn reset_review_surface_runtime_state(&mut self) {
        self.invalidate_segment_prefetch();
        self.review_surface.clear_row_selection();
        self.drag_selecting_rows = false;
        self.recompute_diff_layout();
        self.review_surface.clear_workspace_surface_snapshot();
        self.review_surface.last_prefetched_visible_row_range = None;
    }

    fn apply_loaded_review_workspace_surface(&mut self) {
        self.invalidate_segment_prefetch();
        self.clamp_comment_rows_to_diff();
        self.clamp_selection_to_rows();
        self.drag_selecting_rows = false;
        self.recompute_diff_layout();
        self.review_surface.clear_workspace_surface_snapshot();
        self.review_surface.last_prefetched_visible_row_range = None;
    }

    fn recompute_diff_layout(&mut self) {
        if self.uses_review_workspace_sections_surface()
            && let Some(session) = self.review_workspace_session.as_ref()
        {
            let (max_left_line_digits, max_right_line_digits) =
                session.line_number_digit_widths();
            self.review_surface.diff_left_line_number_width =
                line_number_column_width(max_left_line_digits);
            self.review_surface.diff_right_line_number_width =
                line_number_column_width(max_right_line_digits);
            return;
        }
        self.review_surface.diff_left_line_number_width =
            line_number_column_width(DIFF_LINE_NUMBER_MIN_DIGITS);
        self.review_surface.diff_right_line_number_width =
            line_number_column_width(DIFF_LINE_NUMBER_MIN_DIGITS);
    }

}
