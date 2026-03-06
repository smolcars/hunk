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

        self.selected_path = Some(path.clone());
        self.selected_status = self
            .files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.status);
        self.scroll_selected_after_reload = true;
        self.last_diff_scroll_offset = None;
        self.last_scroll_activity_at = Instant::now();
        self.request_selected_diff_reload(cx);
        cx.notify();
    }

    fn clamp_selection_to_rows(&mut self) {
        if self.diff_rows.is_empty() {
            self.selection_anchor_row = None;
            self.selection_head_row = None;
            return;
        }

        let max_ix = self.diff_rows.len().saturating_sub(1);
        self.selection_anchor_row = self.selection_anchor_row.map(|ix| ix.min(max_ix));
        self.selection_head_row = self.selection_head_row.map(|ix| ix.min(max_ix));
    }

    pub(super) fn selected_row_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor_row?;
        let head = self.selection_head_row?;
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
        if self.diff_rows.is_empty() {
            self.selection_anchor_row = None;
            self.selection_head_row = None;
            return;
        }

        let target_ix = row_ix.min(self.diff_rows.len().saturating_sub(1));
        if extend_selection && self.selection_anchor_row.is_some() {
            self.selection_head_row = Some(target_ix);
        } else {
            self.selection_anchor_row = Some(target_ix);
            self.selection_head_row = Some(target_ix);
        }

        if let Some((path, status)) = self.selected_file_from_row_metadata(target_ix)
            && self.selected_path.as_deref() != Some(path.as_str())
        {
            self.selected_path = Some(path);
            self.selected_status = Some(status);
        }

        cx.notify();
    }

    fn select_row_and_scroll(
        &mut self,
        row_ix: usize,
        extend_selection: bool,
        cx: &mut Context<Self>,
    ) {
        self.select_row(row_ix, extend_selection, cx);
        self.diff_list_state.scroll_to(ListOffset {
            item_ix: row_ix.min(self.diff_rows.len().saturating_sub(1)),
            offset_in_item: px(0.),
        });
        self.last_diff_scroll_offset = None;
        self.last_scroll_activity_at = Instant::now();
    }

    fn move_selection_by(&mut self, delta: isize, extend_selection: bool, cx: &mut Context<Self>) {
        if self.diff_rows.is_empty() {
            return;
        }

        let max_ix = self.diff_rows.len().saturating_sub(1) as isize;
        let base_ix = self
            .selection_head_row
            .map(|ix| ix as isize)
            .unwrap_or(self.diff_list_state.logical_scroll_top().item_ix as isize);
        let target_ix = (base_ix + delta).clamp(0, max_ix) as usize;
        self.select_row_and_scroll(target_ix, extend_selection, cx);
    }

    fn select_all_rows(&mut self, cx: &mut Context<Self>) {
        if self.diff_rows.is_empty() {
            return;
        }

        self.selection_anchor_row = Some(0);
        self.selection_head_row = Some(self.diff_rows.len().saturating_sub(1));
        cx.notify();
    }

    fn select_hunk_relative(&mut self, direction: isize, cx: &mut Context<Self>) {
        if self.diff_rows.is_empty() {
            return;
        }

        let start_ix = self
            .selection_head_row
            .unwrap_or(self.diff_list_state.logical_scroll_top().item_ix)
            .min(self.diff_rows.len().saturating_sub(1));

        let target = find_wrapped_hunk_target(self.diff_rows.len(), start_ix, direction, |ix| {
            self.diff_rows[ix].kind == DiffRowKind::HunkHeader
        });

        if let Some(target_ix) = target {
            self.select_row_and_scroll(target_ix, false, cx);
        }
    }

    fn select_file_relative(&mut self, direction: isize, cx: &mut Context<Self>) {
        if self.file_row_ranges.is_empty() {
            return;
        }

        let current_ix = self
            .selected_path
            .as_ref()
            .and_then(|path| {
                self.file_row_ranges
                    .iter()
                    .position(|range| range.path == *path)
            })
            .unwrap_or(0);
        let max_ix = self.file_row_ranges.len().saturating_sub(1) as isize;
        let target_ix = (current_ix as isize + direction).clamp(0, max_ix) as usize;
        let (path, status, start_row) = {
            let range = &self.file_row_ranges[target_ix];
            (range.path.clone(), range.status, range.start_row)
        };

        self.selected_path = Some(path.clone());
        self.selected_status = Some(status);
        self.scroll_to_file_start(&path);
        self.select_row(start_row, false, cx);
        cx.notify();
    }

    fn selected_rows_as_text(&self) -> Option<String> {
        let (start, end) = self.selected_row_range()?;
        if self.diff_rows.is_empty() {
            return None;
        }

        let mut lines = Vec::new();
        for row_ix in start..=end.min(self.diff_rows.len().saturating_sub(1)) {
            let row = &self.diff_rows[row_ix];
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
        self.move_selection_by(1, false, cx);
    }

    pub(super) fn select_previous_line_action(
        &mut self,
        _: &SelectPreviousLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_selection_by(-1, false, cx);
    }

    pub(super) fn extend_selection_next_line_action(
        &mut self,
        _: &ExtendSelectionNextLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_selection_by(1, true, cx);
    }

    pub(super) fn extend_selection_previous_line_action(
        &mut self,
        _: &ExtendSelectionPreviousLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        self.select_all_rows(cx);
    }

    pub(super) fn next_hunk_action(
        &mut self,
        _: &NextHunk,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_hunk_relative(1, cx);
    }

    pub(super) fn previous_hunk_action(
        &mut self,
        _: &PreviousHunk,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_hunk_relative(-1, cx);
    }

    pub(super) fn next_file_action(
        &mut self,
        _: &NextFile,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_file_relative(1, cx);
    }

    pub(super) fn previous_file_action(
        &mut self,
        _: &PreviousFile,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_file_relative(-1, cx);
    }
}
