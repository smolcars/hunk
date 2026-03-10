#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FileAnchorReconcileState {
    Ready,
    Deferred,
    Unavailable,
}

impl DiffViewer {
    pub(super) fn file_anchor_reconcile_state(&self, file_path: &str) -> FileAnchorReconcileState {
        if self.diff_row_metadata.len() != self.diff_rows.len() {
            return FileAnchorReconcileState::Deferred;
        }

        let mut has_anchor_rows = false;
        let mut saw_rows_for_file = false;
        for row in self
            .diff_row_metadata
            .iter()
            .filter(|row| row.file_path.as_deref() == Some(file_path))
        {
            saw_rows_for_file = true;
            match row.kind {
                DiffStreamRowKind::CoreCode
                | DiffStreamRowKind::CoreHunkHeader
                | DiffStreamRowKind::CoreMeta
                | DiffStreamRowKind::CoreEmpty => {
                    has_anchor_rows = true;
                }
                DiffStreamRowKind::FileLoading
                | DiffStreamRowKind::FileCollapsed => return FileAnchorReconcileState::Deferred,
                DiffStreamRowKind::FileError => return FileAnchorReconcileState::Unavailable,
                DiffStreamRowKind::FileHeader | DiffStreamRowKind::EmptyState => {}
            }
        }

        if has_anchor_rows {
            FileAnchorReconcileState::Ready
        } else if self.patch_loading || saw_rows_for_file {
            FileAnchorReconcileState::Deferred
        } else {
            FileAnchorReconcileState::Unavailable
        }
    }

    fn find_matching_row_for_comment(&self, comment: &CommentRecord) -> Option<usize> {
        let (row_anchor_index, rows_by_path) = self.build_comment_row_anchor_index();
        self.find_matching_row_for_comment_with_index(comment, &row_anchor_index, &rows_by_path)
    }

    fn find_matching_row_for_comment_with_index(
        &self,
        comment: &CommentRecord,
        row_anchor_index: &BTreeMap<usize, RowCommentAnchor>,
        rows_by_path: &BTreeMap<String, Vec<usize>>,
    ) -> Option<usize> {
        let mut hash_fallback = None;
        let mut fuzzy_fallback = None::<(usize, i32)>;
        let mut rename_fuzzy_fallback = None::<(usize, i32)>;
        let key = Self::build_fuzzy_comment_key(comment);

        if let Some(row_ixs) = rows_by_path.get(comment.file_path.as_str()) {
            for row_ix in row_ixs {
                let row_ix = *row_ix;
                if self.row_exact_anchor_match(row_ix, comment) {
                    return Some(row_ix);
                }

                let Some(anchor) = row_anchor_index.get(&row_ix) else {
                    continue;
                };
                let score = Self::fuzzy_anchor_match_score(&key, anchor);
                if hash_fallback.is_none() && anchor.anchor_hash == comment.anchor_hash {
                    hash_fallback = Some(row_ix);
                }

                if score >= COMMENT_FUZZY_MATCH_MIN_SCORE {
                    let should_replace = fuzzy_fallback
                        .as_ref()
                        .map(|(_, best)| score > *best)
                        .unwrap_or(true);
                    if should_replace {
                        fuzzy_fallback = Some((row_ix, score));
                    }
                }
            }
        }

        for (row_ix, anchor) in row_anchor_index {
            if anchor.file_path == comment.file_path {
                continue;
            }

            let score = Self::fuzzy_anchor_match_score(&key, anchor);
            if score >= COMMENT_FUZZY_RENAME_MATCH_MIN_SCORE {
                let should_replace = rename_fuzzy_fallback
                    .as_ref()
                    .map(|(_, best)| score > *best)
                    .unwrap_or(true);
                if should_replace {
                    rename_fuzzy_fallback = Some((*row_ix, score));
                }
            }
        }

        hash_fallback
            .or_else(|| fuzzy_fallback.map(|(row_ix, _)| row_ix))
            .or_else(|| rename_fuzzy_fallback.map(|(row_ix, _)| row_ix))
    }

    fn build_fuzzy_comment_key(comment: &CommentRecord) -> FuzzyCommentKey {
        FuzzyCommentKey {
            line_side: comment.line_side,
            old_line: comment.old_line,
            new_line: comment.new_line,
            line_text: Self::normalize_text_for_fuzzy(comment.line_text.as_str()),
            line_core: Self::normalize_diff_line_body(comment.line_text.as_str()),
            hunk_header: Self::normalize_text_for_fuzzy(comment.hunk_header.as_deref().unwrap_or("")),
            context_before_line: Self::normalize_diff_line_body(
                Self::last_non_empty_line(comment.context_before.as_str()),
            ),
            context_after_line: Self::normalize_diff_line_body(
                Self::first_non_empty_line(comment.context_after.as_str()),
            ),
        }
    }

    fn fuzzy_anchor_match_score(key: &FuzzyCommentKey, anchor: &RowCommentAnchor) -> i32 {
        let mut score = 0i32;

        if key.line_side == anchor.line_side {
            score += 2;
        } else {
            score -= 1;
        }

        let anchor_line = Self::normalize_text_for_fuzzy(anchor.line_text.as_str());
        if !key.line_text.is_empty() && key.line_text == anchor_line {
            score += 6;
        } else {
            let anchor_core = Self::normalize_diff_line_body(anchor.line_text.as_str());
            if !key.line_core.is_empty() && key.line_core == anchor_core {
                score += 5;
            } else if Self::has_substring_overlap(key.line_core.as_str(), anchor_core.as_str()) {
                score += 3;
            }
        }

        let anchor_hunk = Self::normalize_text_for_fuzzy(anchor.hunk_header.as_deref().unwrap_or(""));
        if !key.hunk_header.is_empty() && key.hunk_header == anchor_hunk {
            score += 2;
        }

        let anchor_before = Self::normalize_diff_line_body(
            Self::last_non_empty_line(anchor.context_before.as_str()),
        );
        let anchor_after = Self::normalize_diff_line_body(
            Self::first_non_empty_line(anchor.context_after.as_str()),
        );
        score += Self::context_line_score(
            key.context_before_line.as_str(),
            anchor_before.as_str(),
        );
        score += Self::context_line_score(
            key.context_after_line.as_str(),
            anchor_after.as_str(),
        );
        score += Self::line_distance_score(key.old_line, anchor.old_line);
        score += Self::line_distance_score(key.new_line, anchor.new_line);

        score
    }

    fn normalize_text_for_fuzzy(text: &str) -> String {
        text.split_whitespace()
            .map(|part| part.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn normalize_diff_line_body(text: &str) -> String {
        text.lines()
            .map(|line| {
                let trimmed = line.trim_start();
                trimmed
                    .strip_prefix('+')
                    .or_else(|| trimmed.strip_prefix('-'))
                    .or_else(|| trimmed.strip_prefix(' '))
                    .unwrap_or(trimmed)
                    .trim()
            })
            .filter(|line| !line.is_empty())
            .map(Self::normalize_text_for_fuzzy)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn first_non_empty_line(text: &str) -> &str {
        text.lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
    }

    fn last_non_empty_line(text: &str) -> &str {
        text.lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
    }

    fn has_substring_overlap(lhs: &str, rhs: &str) -> bool {
        let min_len = lhs.len().min(rhs.len());
        min_len >= 12 && (lhs.contains(rhs) || rhs.contains(lhs))
    }

    fn context_line_score(lhs: &str, rhs: &str) -> i32 {
        if lhs.is_empty() || rhs.is_empty() {
            return 0;
        }
        if lhs == rhs {
            return 2;
        }
        if Self::has_substring_overlap(lhs, rhs) {
            return 1;
        }
        0
    }

    fn line_distance_score(lhs: Option<u32>, rhs: Option<u32>) -> i32 {
        match (lhs, rhs) {
            (Some(a), Some(b)) => {
                let distance = a.abs_diff(b);
                if distance == 0 {
                    2
                } else if distance <= 2 {
                    1
                } else if distance <= 8 {
                    0
                } else {
                    -1
                }
            }
            _ => 0,
        }
    }

    fn row_exact_anchor_match(&self, row_ix: usize, comment: &CommentRecord) -> bool {
        if self.row_file_path(row_ix).as_deref() != Some(comment.file_path.as_str()) {
            return false;
        }
        let Some(row) = self.diff_rows.get(row_ix) else {
            return false;
        };

        if row.kind != DiffRowKind::Code {
            if comment.line_side != CommentLineSide::Meta {
                return false;
            }
            let line_text = Self::row_diff_lines(row).join("\n");
            return line_text == comment.line_text;
        }

        match comment.line_side {
            CommentLineSide::Left => {
                row.left.line == comment.old_line
                    && (comment.new_line.is_none() || row.right.line == comment.new_line)
            }
            CommentLineSide::Right => {
                row.right.line == comment.new_line
                    && (comment.old_line.is_none() || row.left.line == comment.old_line)
            }
            CommentLineSide::Meta => false,
        }
    }

    fn row_file_path(&self, row_ix: usize) -> Option<String> {
        if self.diff_row_metadata.len() == self.diff_rows.len() {
            return self
                .diff_row_metadata
                .get(row_ix)
                .and_then(|row| row.file_path.clone());
        }
        None
    }

    fn row_hunk_header(&self, row_ix: usize) -> Option<String> {
        let hunk_ix = self
            .diff_visible_hunk_header_lookup
            .get(row_ix)
            .copied()
            .flatten()?;
        self.diff_rows.get(hunk_ix).map(|row| row.text.clone())
    }
}
