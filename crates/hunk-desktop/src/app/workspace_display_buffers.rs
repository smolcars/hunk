use std::collections::BTreeMap;

use hunk_editor::{
    Viewport, WorkspaceDisplaySnapshot, WorkspaceDocumentId, WorkspaceLayout,
    build_workspace_display_snapshot,
};
use hunk_text::TextSnapshot;

pub(crate) fn build_workspace_display_snapshot_from_document_snapshots(
    layout: &WorkspaceLayout,
    viewport: Viewport,
    tab_width: usize,
    show_whitespace: bool,
    document_snapshots: &BTreeMap<WorkspaceDocumentId, TextSnapshot>,
) -> WorkspaceDisplaySnapshot {
    build_workspace_display_snapshot(
        layout,
        viewport,
        tab_width,
        show_whitespace,
        |document_id, line| {
            let snapshot = document_snapshots.get(&document_id)?;
            Some(snapshot_line_text(snapshot, line))
        },
    )
}

pub(crate) fn snapshot_line_text(snapshot: &TextSnapshot, line: usize) -> String {
    let start = snapshot.line_to_byte(line).unwrap_or(0);
    let end = if line + 1 < snapshot.line_count() {
        snapshot
            .line_to_byte(line + 1)
            .unwrap_or(snapshot.byte_len())
    } else {
        snapshot.byte_len()
    };
    snapshot
        .slice(start..end)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}
