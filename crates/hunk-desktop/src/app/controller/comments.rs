pub(super) type RowCommentAnchor = crate::app::review_workspace_session::ReviewCommentAnchor;

#[derive(Debug, Clone)]
struct FuzzyCommentKey {
    line_side: CommentLineSide,
    old_line: Option<u32>,
    new_line: Option<u32>,
    line_text: String,
    line_core: String,
    hunk_header: String,
    context_before_line: String,
    context_after_line: String,
}

impl DiffViewer {
    fn invalidate_review_comment_surface_snapshot(&mut self) {
        if self.uses_review_workspace_sections_surface() {
            self.review_surface.clear_workspace_surface_snapshot();
        }
    }

    fn reset_comment_row_match_cache(&mut self) {
        self.comment_row_matches.clear();
        self.comment_open_row_counts.clear();
        self.invalidate_review_comment_surface_snapshot();
    }

    fn rebuild_comment_row_match_cache(&mut self) {
        self.comment_row_matches.clear();
        let row_count = self.active_diff_row_count();
        self.comment_open_row_counts = vec![0; row_count];
        self.invalidate_review_comment_surface_snapshot();
        if row_count == 0 || self.comments_cache.is_empty() {
            return;
        }
        let (row_anchor_index, rows_by_path) = self.build_comment_row_anchor_index();

        for comment in self
            .comments_cache
            .iter()
            .filter(|comment| comment.status == CommentStatus::Open)
        {
            if let Some(row_ix) =
                self.find_matching_row_for_comment_with_index(comment, &row_anchor_index, &rows_by_path)
            {
                self.comment_row_matches.insert(comment.id.clone(), row_ix);
                if let Some(count) = self.comment_open_row_counts.get_mut(row_ix) {
                    *count += 1;
                }
            }
        }
    }

    fn build_comment_row_anchor_index(
        &self,
    ) -> (BTreeMap<usize, RowCommentAnchor>, BTreeMap<String, Vec<usize>>) {
        self.review_workspace_session
            .as_ref()
            .map(|session| session.build_comment_anchor_index(COMMENT_CONTEXT_RADIUS_ROWS))
            .unwrap_or_default()
    }

    fn load_database_store() -> Option<DatabaseStore> {
        match DatabaseStore::new() {
            Ok(store) => Some(store),
            Err(err) => {
                error!("failed to initialize sqlite database path: {err:#}");
                None
            }
        }
    }

    fn clear_comment_ui_state(&mut self) {
        self.hovered_comment_row = None;
        self.active_comment_editor_row = None;
        self.comments_preview_open = false;
        self.invalidate_review_comment_surface_snapshot();
    }

    fn auto_show_non_open_if_open_empty(&mut self) {
        if self.comments_show_non_open {
            return;
        }
        if !self.comments_cache.is_empty() && self.comments_open_count() == 0 {
            self.comments_show_non_open = true;
        }
    }

    fn clamp_comment_rows_to_diff(&mut self) {
        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            self.hovered_comment_row = None;
            self.active_comment_editor_row = None;
            self.invalidate_review_comment_surface_snapshot();
            return;
        }

        let max_ix = row_count.saturating_sub(1);
        self.hovered_comment_row = self.hovered_comment_row.map(|ix| ix.min(max_ix));
        self.active_comment_editor_row = self.active_comment_editor_row.map(|ix| ix.min(max_ix));
        self.invalidate_review_comment_surface_snapshot();
    }

    fn comment_scope_repo_root(&self) -> Option<String> {
        self.primary_repo_root()
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
    }

    fn comment_scope_branch_name(&self) -> String {
        let name = self
            .review_right_source_id
            .as_deref()
            .and_then(|source_id| self.review_compare_source_option(source_id))
            .and_then(|source| source.branch_name.as_deref())
            .or_else(|| {
                self.selected_git_workspace_target()
                    .map(|target| target.branch_name.as_str())
            })
            .or_else(|| self.primary_checked_out_branch_name())
            .unwrap_or(self.branch_name.as_str())
            .trim();
        if name.is_empty() || name == "unknown" {
            "detached".to_string()
        } else {
            name.to_string()
        }
    }

    fn refresh_comments_cache_from_store(&mut self) {
        if !self.review_comments_enabled() {
            self.comments_cache.clear();
            self.comment_miss_streaks.clear();
            self.reset_comment_row_match_cache();
            self.clear_comment_ui_state();
            self.comment_status_message = None;
            return;
        }

        let Some(store) = self.database_store.clone() else {
            self.comments_cache.clear();
            self.reset_comment_row_match_cache();
            return;
        };
        let Some(repo_root) = self.comment_scope_repo_root() else {
            self.comments_cache.clear();
            self.reset_comment_row_match_cache();
            return;
        };
        let branch_name = self.comment_scope_branch_name();

        match store.list_comments(repo_root.as_str(), branch_name.as_str(), true) {
            Ok(records) => {
                self.comments_cache = records;
                let open_ids = self
                    .comments_cache
                    .iter()
                    .filter(|comment| comment.status == CommentStatus::Open)
                    .map(|comment| comment.id.clone())
                    .collect::<BTreeSet<_>>();
                self.comment_miss_streaks
                    .retain(|comment_id, _| open_ids.contains(comment_id));
                self.auto_show_non_open_if_open_empty();
                self.rebuild_comment_row_match_cache();
                self.comment_status_message = None;
            }
            Err(err) => {
                error!(
                    "failed to load comments for repo '{}' branch '{}': {err:#}",
                    repo_root, branch_name
                );
                self.comments_cache.clear();
                self.reset_comment_row_match_cache();
                self.comment_status_message =
                    Some("Failed to load comments from local database.".to_string());
            }
        }
    }

    fn prune_expired_comments(&mut self) {
        let Some(store) = self.database_store.clone() else {
            return;
        };
        let retention_ms = COMMENT_RETENTION_DAYS.saturating_mul(24 * 60 * 60 * 1000);
        let cutoff = now_unix_ms().saturating_sub(retention_ms);
        if let Err(err) = store.prune_non_open_comments(cutoff) {
            error!("failed to prune old comments: {err:#}");
        }
    }

    pub(super) fn comments_open_count(&self) -> usize {
        self.comments_cache
            .iter()
            .filter(|comment| comment.status == CommentStatus::Open)
            .count()
    }

    pub(super) fn comments_stale_count(&self) -> usize {
        self.comments_cache
            .iter()
            .filter(|comment| comment.status == CommentStatus::Stale)
            .count()
    }

    pub(super) fn comments_resolved_count(&self) -> usize {
        self.comments_cache
            .iter()
            .filter(|comment| comment.status == CommentStatus::Resolved)
            .count()
    }

    pub(super) fn comments_preview_records(&self) -> Vec<CommentRecord> {
        self.comments_cache
            .iter()
            .filter(|comment| {
                self.comments_show_non_open || comment.status == CommentStatus::Open
            })
            .take(COMMENT_PREVIEW_MAX_ITEMS)
            .cloned()
            .collect::<Vec<_>>()
    }

    pub(super) fn set_comments_show_non_open(
        &mut self,
        show_non_open: bool,
        cx: &mut Context<Self>,
    ) {
        if self.comments_show_non_open == show_non_open {
            return;
        }
        self.comments_show_non_open = show_non_open;
        cx.notify();
    }

    pub(super) fn toggle_comments_preview(&mut self, cx: &mut Context<Self>) {
        if !self.review_comments_enabled() {
            self.comment_status_message =
                Some("Comments are only available for the default base-vs-active comparison.".to_string());
            cx.notify();
            return;
        }
        if !self.comments_preview_open {
            self.auto_show_non_open_if_open_empty();
        }
        self.comments_preview_open = !self.comments_preview_open;
        cx.notify();
    }

    pub(super) fn close_comments_preview(&mut self, cx: &mut Context<Self>) {
        if !self.comments_preview_open {
            return;
        }
        self.comments_preview_open = false;
        cx.notify();
    }

    pub(super) fn row_supports_comments(&self, row_ix: usize) -> bool {
        if !self.review_comments_enabled() {
            return false;
        }
        self.review_workspace_session
            .as_ref()
            .is_some_and(|session| session.row_supports_comments(row_ix))
    }

    pub(super) fn on_diff_row_hover(&mut self, row_ix: usize, cx: &mut Context<Self>) {
        if !self.row_supports_comments(row_ix) {
            return;
        }
        if self.hovered_comment_row == Some(row_ix) {
            return;
        }
        self.hovered_comment_row = Some(row_ix);
        self.invalidate_review_comment_surface_snapshot();
        cx.notify();
    }

    pub(super) fn open_comment_editor_for_row(
        &mut self,
        row_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.row_supports_comments(row_ix) {
            return;
        }
        self.active_comment_editor_row = Some(row_ix);
        self.comment_status_message = None;
        self.invalidate_review_comment_surface_snapshot();
        let state = self.comment_input_state.clone();
        state.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    pub(super) fn cancel_comment_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_comment_editor_row = None;
        self.invalidate_review_comment_surface_snapshot();
        let state = self.comment_input_state.clone();
        state.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    pub(super) fn save_active_comment(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.review_comments_enabled() {
            self.comment_status_message =
                Some("Comments are disabled for custom compare pairs.".to_string());
            self.active_comment_editor_row = None;
            cx.notify();
            return;
        }
        let Some(store) = self.database_store.clone() else {
            self.comment_status_message =
                Some("Comments database is unavailable on this machine.".to_string());
            cx.notify();
            return;
        };
        let Some(row_ix) = self.active_comment_editor_row else {
            return;
        };

        let comment_text = self.comment_input_state.read(cx).value().trim().to_string();
        if comment_text.is_empty() {
            self.comment_status_message = Some("Comment text cannot be empty.".to_string());
            cx.notify();
            return;
        }

        let Some(anchor) = self.build_row_comment_anchor(row_ix) else {
            self.comment_status_message =
                Some("Could not resolve a stable anchor for this diff row.".to_string());
            cx.notify();
            return;
        };
        let Some(repo_root) = self.comment_scope_repo_root() else {
            self.comment_status_message = Some("No repository is open.".to_string());
            cx.notify();
            return;
        };

        let input = NewComment {
            repo_root,
            branch_name: self.comment_scope_branch_name(),
            created_head_commit: None,
            file_path: anchor.file_path,
            line_side: anchor.line_side,
            old_line: anchor.old_line,
            new_line: anchor.new_line,
            row_stable_id: self
                .active_diff_row_metadata(row_ix)
                .map(|row| row.stable_id),
            hunk_header: anchor.hunk_header,
            line_text: anchor.line_text,
            context_before: anchor.context_before,
            context_after: anchor.context_after,
            anchor_hash: anchor.anchor_hash,
            comment_text,
        };

        match store.create_comment(&input) {
            Ok(_) => {
                self.active_comment_editor_row = None;
                self.comments_preview_open = false;
                let state = self.comment_input_state.clone();
                state.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                });
                self.refresh_comments_cache_from_store();
                self.comment_status_message = Some("Comment added.".to_string());
            }
            Err(err) => {
                error!("failed to create diff comment: {err:#}");
                self.comment_status_message = Some("Failed to save comment.".to_string());
            }
        }
        cx.notify();
    }

    pub(super) fn copy_comment_bundle_by_id(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(comment) = self.comments_cache.iter().find(|comment| comment.id == id) else {
            return;
        };
        let blob = format_comment_clipboard_blob(comment);
        cx.write_to_clipboard(ClipboardItem::new_string(blob));
        self.comment_status_message = Some("Copied comment bundle.".to_string());
        cx.notify();
    }

    pub(super) fn copy_all_open_comment_bundles(&mut self, cx: &mut Context<Self>) {
        let blobs = self
            .comments_cache
            .iter()
            .filter(|comment| comment.status == CommentStatus::Open)
            .map(format_comment_clipboard_blob)
            .collect::<Vec<_>>();
        if blobs.is_empty() {
            self.comment_status_message = Some("No open comments to copy.".to_string());
            cx.notify();
            return;
        }

        let combined = blobs.join("\n\n---\n\n");
        cx.write_to_clipboard(ClipboardItem::new_string(combined));
        self.comment_status_message = Some(format!("Copied {} comment bundles.", blobs.len()));
        cx.notify();
    }

    pub(super) fn delete_comment_by_id(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(store) = self.database_store.clone() else {
            return;
        };

        match store.delete_comment(id.as_str()) {
            Ok(_) => {
                self.comment_miss_streaks.remove(id.as_str());
                self.refresh_comments_cache_from_store();
                self.comment_status_message = Some("Comment deleted.".to_string());
            }
            Err(err) => {
                error!("failed to delete comment {id}: {err:#}");
                self.comment_status_message = Some("Failed to delete comment.".to_string());
            }
        }
        cx.notify();
    }

    pub(super) fn reopen_comment_by_id(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(store) = self.database_store.clone() else {
            return;
        };

        match store.mark_comment_status(id.as_str(), CommentStatus::Open, None, now_unix_ms()) {
            Ok(updated) => {
                if updated {
                    self.comment_miss_streaks.remove(id.as_str());
                    self.refresh_comments_cache_from_store();
                    self.comment_status_message = Some("Comment reopened.".to_string());
                }
            }
            Err(err) => {
                error!("failed to reopen comment {id}: {err:#}");
                self.comment_status_message = Some("Failed to reopen comment.".to_string());
            }
        }
        cx.notify();
    }

    pub(super) fn resolve_all_stale_comments(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.database_store.clone() else {
            return;
        };
        let stale_ids = self
            .comments_cache
            .iter()
            .filter(|comment| comment.status == CommentStatus::Stale)
            .map(|comment| comment.id.clone())
            .collect::<Vec<_>>();
        if stale_ids.is_empty() {
            self.comment_status_message = Some("No stale comments to resolve.".to_string());
            cx.notify();
            return;
        }

        let now = now_unix_ms();
        let resolved =
            match store.mark_many_comment_status(&stale_ids, CommentStatus::Resolved, None, now) {
                Ok(updated) => {
                    for id in &stale_ids {
                        self.comment_miss_streaks.remove(id.as_str());
                    }
                    updated
                }
                Err(err) => {
                    error!("failed to resolve stale comments in batch: {err:#}");
                    0
                }
            };

        self.refresh_comments_cache_from_store();
        self.comment_status_message = Some(format!("Resolved {resolved} stale comments."));
        cx.notify();
    }

    pub(super) fn reopen_all_stale_comments(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.database_store.clone() else {
            return;
        };
        let stale_ids = self
            .comments_cache
            .iter()
            .filter(|comment| comment.status == CommentStatus::Stale)
            .map(|comment| comment.id.clone())
            .collect::<Vec<_>>();
        if stale_ids.is_empty() {
            self.comment_status_message = Some("No stale comments to reopen.".to_string());
            cx.notify();
            return;
        }

        let now = now_unix_ms();
        let reopened =
            match store.mark_many_comment_status(&stale_ids, CommentStatus::Open, None, now) {
                Ok(updated) => {
                    for id in &stale_ids {
                        self.comment_miss_streaks.remove(id.as_str());
                    }
                    updated
                }
                Err(err) => {
                    error!("failed to reopen stale comments in batch: {err:#}");
                    0
                }
            };

        self.refresh_comments_cache_from_store();
        self.comment_status_message = Some(format!("Reopened {reopened} stale comments."));
        cx.notify();
    }

    pub(super) fn delete_all_resolved_comments(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.database_store.clone() else {
            return;
        };
        let resolved_ids = self
            .comments_cache
            .iter()
            .filter(|comment| comment.status == CommentStatus::Resolved)
            .map(|comment| comment.id.clone())
            .collect::<Vec<_>>();
        if resolved_ids.is_empty() {
            self.comment_status_message = Some("No resolved comments to delete.".to_string());
            cx.notify();
            return;
        }

        let deleted = match store.delete_many_comments(&resolved_ids) {
            Ok(updated) => {
                for id in &resolved_ids {
                    self.comment_miss_streaks.remove(id.as_str());
                }
                updated
            }
            Err(err) => {
                error!("failed to delete resolved comments in batch: {err:#}");
                0
            }
        };

        self.refresh_comments_cache_from_store();
        self.comment_status_message = Some(format!("Deleted {deleted} resolved comments."));
        cx.notify();
    }

    pub(super) fn jump_to_comment_by_id(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(comment) = self
            .comments_cache
            .iter()
            .find(|comment| comment.id == id)
            .cloned()
        else {
            return;
        };

        let mapped_row = self
            .comment_row_matches
            .get(comment.id.as_str())
            .copied()
            .or_else(|| self.find_matching_row_for_comment(&comment));
        if let Some(row_ix) = mapped_row {
            self.comments_preview_open = false;
            self.select_row_and_scroll(row_ix, false, cx);
            self.hovered_comment_row = Some(row_ix);
            self.comment_status_message = Some("Jumped to comment location.".to_string());
            cx.notify();
            return;
        }

        if let Some((status, start_row)) = self
            .active_diff_file_range_for_path(comment.file_path.as_str())
            .map(|range| (range.status, range.start_row))
        {
            self.comments_preview_open = false;
            if self.workspace_view_mode == WorkspaceViewMode::Diff {
                self.set_review_selected_file(Some(comment.file_path), Some(status));
            } else {
                self.selected_path = Some(comment.file_path);
                self.selected_status = Some(status);
            }
            self.select_row_and_scroll(start_row, false, cx);
            self.comment_status_message =
                Some("Comment anchor not found; jumped to file.".to_string());
            cx.notify();
            return;
        }

        self.comment_status_message = Some("Comment location is not visible in this diff.".to_string());
        cx.notify();
    }

    pub(super) fn reconcile_comments_with_loaded_diff(&mut self) {
        self.refresh_comments_cache_from_store();
        let Some(store) = self.database_store.clone() else {
            return;
        };
        if self.comments_cache.is_empty() {
            return;
        }

        let now = now_unix_ms();
        let changed_paths = self
            .active_diff_files()
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        let mut should_reload = false;
        let mut seen_ids = Vec::new();
        let mut stale_ids = Vec::new();
        let mut resolved_ids = Vec::new();

        for comment in self
            .comments_cache
            .clone()
            .into_iter()
            .filter(|comment| comment.status == CommentStatus::Open)
        {
            if self.comment_row_matches.contains_key(comment.id.as_str()) {
                self.comment_miss_streaks.remove(comment.id.as_str());
                seen_ids.push(comment.id.clone());
                continue;
            }

            let file_is_changed = changed_paths.contains(comment.file_path.as_str());
            if file_is_changed {
                match self.file_anchor_reconcile_state(comment.file_path.as_str()) {
                    FileAnchorReconcileState::Ready | FileAnchorReconcileState::Unavailable => {}
                    FileAnchorReconcileState::Deferred => continue,
                }
            }

            let next_miss_streak = self
                .comment_miss_streaks
                .get(comment.id.as_str())
                .copied()
                .unwrap_or(0)
                .saturating_add(1);
            if next_miss_streak < COMMENT_RECONCILE_MISS_THRESHOLD {
                self.comment_miss_streaks
                    .insert(comment.id.clone(), next_miss_streak);
                continue;
            }
            self.comment_miss_streaks.remove(comment.id.as_str());

            let (next_status, _) = next_status_for_unmatched_anchor(file_is_changed);
            match next_status {
                CommentStatus::Stale => stale_ids.push(comment.id.clone()),
                CommentStatus::Resolved => resolved_ids.push(comment.id.clone()),
                CommentStatus::Open => {}
            }
        }

        if let Err(err) = store.touch_many_comment_seen(&seen_ids, now) {
            error!("failed to batch update comment last_seen: {err:#}");
        }
        match store.mark_many_comment_status(&stale_ids, CommentStatus::Stale, Some("anchor_not_found"), now)
        {
            Ok(updated) => {
                should_reload |= updated > 0;
            }
            Err(err) => {
                error!("failed to batch update stale comment status: {err:#}");
            }
        }
        match store.mark_many_comment_status(&resolved_ids, CommentStatus::Resolved, None, now) {
            Ok(updated) => {
                should_reload |= updated > 0;
            }
            Err(err) => {
                error!("failed to batch update resolved comment status: {err:#}");
            }
        }

        if should_reload {
            self.refresh_comments_cache_from_store();
        }
    }

    pub(super) fn build_row_comment_anchor(&self, row_ix: usize) -> Option<RowCommentAnchor> {
        self.review_workspace_session
            .as_ref()
            .and_then(|session| session.build_comment_anchor(row_ix, COMMENT_CONTEXT_RADIUS_ROWS))
    }

    pub(super) fn row_diff_lines(row: &SideBySideRow) -> Vec<String> {
        let mut lines = Vec::new();
        match row.kind {
            DiffRowKind::Code => {
                if row.left.kind == DiffCellKind::Removed {
                    lines.push(format!("-{}", row.left.text));
                }
                if row.right.kind == DiffCellKind::Added {
                    lines.push(format!("+{}", row.right.text));
                }
                if row.left.kind == DiffCellKind::Context {
                    lines.push(format!(" {}", row.left.text));
                }
                if row.left.kind == DiffCellKind::None
                    && row.right.kind == DiffCellKind::None
                    && !row.text.is_empty()
                {
                    lines.push(row.text.clone());
                }
            }
            DiffRowKind::HunkHeader => {}
            DiffRowKind::Meta | DiffRowKind::Empty => {
                lines.push(row.text.clone());
            }
        }
        lines
    }

    pub(super) fn comment_status_label(status: CommentStatus) -> &'static str {
        match status {
            CommentStatus::Open => "open",
            CommentStatus::Stale => "stale",
            CommentStatus::Resolved => "resolved",
        }
    }
}
