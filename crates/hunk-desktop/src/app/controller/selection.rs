fn find_wrapped_hunk_target(
    row_count: usize,
    start_ix: usize,
    direction: isize,
    mut is_hunk_header: impl FnMut(usize) -> bool,
) -> Option<usize> {
    if row_count == 0 {
        return None;
    }

    let start_ix = start_ix.min(row_count.saturating_sub(1));

    if direction >= 0 {
        ((start_ix + 1)..row_count)
            .find(|ix| is_hunk_header(*ix))
            .or_else(|| (0..row_count).find(|ix| is_hunk_header(*ix)))
    } else {
        (0..start_ix)
            .rev()
            .find(|ix| is_hunk_header(*ix))
            .or_else(|| (0..row_count).rev().find(|ix| is_hunk_header(*ix)))
    }
}

impl DiffViewer {
    pub(super) fn toggle_file_collapsed(&mut self, path: String, cx: &mut Context<Self>) {
        if self.collapsed_files.contains(path.as_str()) {
            self.collapsed_files.remove(path.as_str());
        } else {
            self.collapsed_files.insert(path.clone());
        }

        let status = self
            .active_diff_files()
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.status);
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.set_review_selected_file(Some(path.clone()), status);
        } else {
            self.selected_path = Some(path.clone());
            self.selected_status = status;
        }
        self.scroll_selected_after_reload = true;
        self.review_surface.last_diff_scroll_offset = None;
        self.last_scroll_activity_at = Instant::now();
        self.request_selected_diff_reload(cx);
        cx.notify();
    }

    fn clamp_selection_to_rows(&mut self) {
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            self.review_surface.clear_row_selection();
            return;
        }

        let max_ix = row_count.saturating_sub(1);
        self.review_surface.selection_anchor_row =
            self.review_surface.selection_anchor_row.map(|ix| ix.min(max_ix));
        self.review_surface.selection_head_row =
            self.review_surface.selection_head_row.map(|ix| ix.min(max_ix));
    }

    pub(super) fn selected_row_range(&self) -> Option<(usize, usize)> {
        let anchor = self.review_surface.selection_anchor_row?;
        let head = self.review_surface.selection_head_row?;
        Some((anchor.min(head), anchor.max(head)))
    }

    pub(super) fn is_row_selected(&self, row_ix: usize) -> bool {
        self.selected_row_range()
            .is_some_and(|(start, end)| row_ix >= start && row_ix <= end)
    }

    pub(super) fn on_diff_row_mouse_down(
        &mut self,
        row_ix: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.drag_selecting_rows = true;
        self.select_row(row_ix, event.modifiers.shift, cx);
    }

    pub(super) fn on_diff_row_mouse_move(
        &mut self,
        row_ix: usize,
        _: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.on_diff_row_hover(row_ix, cx);
        if !self.drag_selecting_rows {
            return;
        }
        self.select_row(row_ix, true, cx);
    }

    pub(super) fn on_diff_row_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.drag_selecting_rows = false;
    }

    fn select_row(&mut self, row_ix: usize, extend_selection: bool, cx: &mut Context<Self>) {
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            self.review_surface.clear_row_selection();
            return;
        }

        let target_ix = row_ix.min(row_count.saturating_sub(1));
        if extend_selection && self.review_surface.selection_anchor_row.is_some() {
            self.review_surface.selection_head_row = Some(target_ix);
        } else {
            self.review_surface.selection_anchor_row = Some(target_ix);
            self.review_surface.selection_head_row = Some(target_ix);
        }

        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.sync_review_workspace_editor_selection_for_row(target_ix);
        }

        if let Some((path, status)) = self.selected_file_from_row_metadata(target_ix)
            && self.selected_path.as_deref() != Some(path.as_str())
        {
            if self.workspace_view_mode == WorkspaceViewMode::Diff {
                self.set_review_selected_file(Some(path), Some(status));
            } else {
                self.selected_path = Some(path);
                self.selected_status = Some(status);
            }
        }

        cx.notify();
    }

    pub(super) fn open_diff_row_context_menu(
        &mut self,
        row_ix: usize,
        position: Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.drag_selecting_rows = false;
        if !self.is_row_selected(row_ix) {
            self.select_row(row_ix, false, cx);
        }
        self.open_workspace_text_context_menu(
            WorkspaceTextContextMenuTarget::DiffRows(DiffRowsContextMenuTarget {
                can_copy: self.selected_row_range().is_some(),
                can_select_all: self.active_diff_row_count() > 0,
            }),
            position,
            cx,
        );
    }

    fn select_row_and_scroll(
        &mut self,
        row_ix: usize,
        extend_selection: bool,
        cx: &mut Context<Self>,
    ) {
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            return;
        }
        self.select_row(row_ix, extend_selection, cx);
        let target_row = row_ix.min(row_count.saturating_sub(1));
        if let Some(session) = self.review_workspace_session.as_ref()
            && let Some(top_offset_px) = session.row_top_offset_px(target_row)
        {
            self.review_surface
                .diff_scroll_handle
                .set_offset(point(px(0.), -px(top_offset_px as f32)));
        }
        self.review_surface.last_diff_scroll_offset = None;
        self.last_scroll_activity_at = Instant::now();
    }

    fn move_selection_by(&mut self, delta: isize, extend_selection: bool, cx: &mut Context<Self>) {
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            return;
        }

        let max_ix = row_count.saturating_sub(1) as isize;
        let base_ix = self
            .review_surface
            .selection_head_row
            .map(|ix| ix as isize)
            .or_else(|| self.current_review_surface_top_row().map(|ix| ix as isize))
            .unwrap_or(0);
        let target_ix = (base_ix + delta).clamp(0, max_ix) as usize;
        self.select_row_and_scroll(target_ix, extend_selection, cx);
    }

    fn select_all_rows(&mut self, cx: &mut Context<Self>) {
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            return;
        }

        self.review_surface.selection_anchor_row = Some(0);
        self.review_surface.selection_head_row = Some(row_count.saturating_sub(1));
        cx.notify();
    }

    fn select_hunk_relative(&mut self, direction: isize, cx: &mut Context<Self>) {
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            return;
        }

        if self.workspace_view_mode == WorkspaceViewMode::Diff
            && let Some(session) = self.review_workspace_session.as_ref()
            && !session.hunk_ranges().is_empty()
        {
            let start_ix = self
                .review_surface
                .selection_head_row
                .or_else(|| self.current_review_surface_top_row())
                .unwrap_or(0)
                .min(row_count.saturating_sub(1));
            let hunk_rows = session
                .hunk_ranges()
                .iter()
                .map(|range| range.start_row)
                .collect::<Vec<_>>();
            let current_ix = hunk_rows
                .iter()
                .position(|row_ix| *row_ix == start_ix)
                .unwrap_or_else(|| {
                    hunk_rows
                        .iter()
                        .position(|row_ix| *row_ix > start_ix)
                        .unwrap_or(0)
                });
            let target_ix = if direction >= 0 {
                current_ix.saturating_add(1) % hunk_rows.len()
            } else if current_ix == 0 {
                hunk_rows.len().saturating_sub(1)
            } else {
                current_ix.saturating_sub(1)
            };
            if let Some(target_row) = hunk_rows.get(target_ix).copied() {
                self.select_row_and_scroll(target_row, false, cx);
            }
            return;
        }

        let start_ix = self
            .review_surface
            .selection_head_row
            .or_else(|| self.current_review_surface_top_row())
            .unwrap_or(0)
            .min(row_count.saturating_sub(1));

        let target = find_wrapped_hunk_target(row_count, start_ix, direction, |ix| {
            self.active_diff_row(ix)
                .is_some_and(|row| row.kind == DiffRowKind::HunkHeader)
        });

        if let Some(target_ix) = target {
            self.select_row_and_scroll(target_ix, false, cx);
        }
    }

    fn select_file_relative(&mut self, direction: isize, cx: &mut Context<Self>) {
        if self.workspace_view_mode == WorkspaceViewMode::Diff
            && let Some(session) = self.review_workspace_session.as_ref()
        {
            let current_path = self
                .current_review_file_range()
                .map(|range| range.path)
                .or_else(|| self.current_review_path());
            let Some((path, status, start_row)) = session
                .adjacent_file(current_path.as_deref(), direction)
                .map(|range| (range.path.clone(), range.status, range.start_row))
            else {
                return;
            };

            self.set_review_selected_file(Some(path.clone()), Some(status));
            self.scroll_to_file_start(path.as_str());
            self.select_row(start_row, false, cx);
            cx.notify();
            return;
        }

        let file_ranges = if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.file_row_ranges
                .iter()
                .map(|range| (range.path.clone(), range.status, range.start_row))
                .collect::<Vec<_>>()
        } else {
            self.file_row_ranges
                .iter()
                .map(|range| (range.path.clone(), range.status, range.start_row))
                .collect::<Vec<_>>()
        };

        if file_ranges.is_empty() {
            return;
        }

        let current_ix = self
            .selected_path
            .as_ref()
            .and_then(|path| {
                file_ranges
                    .iter()
                    .position(|(candidate_path, _, _)| candidate_path == path)
            })
            .unwrap_or(0);
        let max_ix = file_ranges.len().saturating_sub(1) as isize;
        let target_ix = (current_ix as isize + direction).clamp(0, max_ix) as usize;
        let (path, status, start_row) = file_ranges[target_ix].clone();

        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.set_review_selected_file(Some(path.clone()), Some(status));
        } else {
            self.selected_path = Some(path.clone());
            self.selected_status = Some(status);
        }
        self.scroll_to_file_start(&path);
        self.select_row(start_row, false, cx);
        cx.notify();
    }

    fn selected_rows_as_text(&self) -> Option<String> {
        let (start, end) = self.selected_row_range()?;
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            return None;
        }

        let mut lines = Vec::new();
        for row_ix in start..=end.min(row_count.saturating_sub(1)) {
            let Some(row) = self.active_diff_row(row_ix) else {
                continue;
            };
            lines.extend(Self::row_diff_lines(row));
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    pub(super) fn select_next_line_action(
        &mut self,
        _: &SelectNextLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        self.move_selection_by(1, false, cx);
    }

    pub(super) fn select_previous_line_action(
        &mut self,
        _: &SelectPreviousLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        self.move_selection_by(-1, false, cx);
    }

    pub(super) fn extend_selection_next_line_action(
        &mut self,
        _: &ExtendSelectionNextLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        self.move_selection_by(1, true, cx);
    }

    pub(super) fn extend_selection_previous_line_action(
        &mut self,
        _: &ExtendSelectionPreviousLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        self.move_selection_by(-1, true, cx);
    }

    pub(super) fn copy_selection_action(
        &mut self,
        _: &CopySelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode == WorkspaceViewMode::Ai && self.ai_copy_selected_text(cx) {
            return;
        }
        if self.workspace_view_mode == WorkspaceViewMode::Files
            && self.files_terminal_selection_active()
            && self.ai_copy_selected_text(cx)
        {
            return;
        }
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        let Some(selection_text) = self.selected_rows_as_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(selection_text));
    }

    pub(super) fn select_all_rows_action(
        &mut self,
        _: &SelectAllDiffRows,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode == WorkspaceViewMode::Ai && self.ai_select_all_text(cx) {
            return;
        }
        if self.workspace_view_mode == WorkspaceViewMode::Files
            && self.files_terminal_selection_active()
            && self.ai_select_all_text(cx)
        {
            return;
        }
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        self.select_all_rows(cx);
    }

    pub(super) fn next_hunk_action(
        &mut self,
        _: &NextHunk,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        self.select_hunk_relative(1, cx);
    }

    pub(super) fn previous_hunk_action(
        &mut self,
        _: &PreviousHunk,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        self.select_hunk_relative(-1, cx);
    }

    pub(super) fn next_file_action(
        &mut self,
        _: &NextFile,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        self.select_file_relative(1, cx);
    }

    pub(super) fn previous_file_action(
        &mut self,
        _: &PreviousFile,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return;
        }
        self.select_file_relative(-1, cx);
    }
}
