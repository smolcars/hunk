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
}
