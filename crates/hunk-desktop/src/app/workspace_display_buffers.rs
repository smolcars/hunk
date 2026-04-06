use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use hunk_editor::{Viewport, WorkspaceDisplaySnapshot, build_workspace_display_snapshot};
use hunk_editor::{WorkspaceDocumentId, WorkspaceExcerptId, WorkspaceLayout};
use hunk_text::TextSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceSearchMatch {
    pub(crate) excerpt_id: WorkspaceExcerptId,
    pub(crate) document_id: WorkspaceDocumentId,
    pub(crate) surface_order: usize,
    pub(crate) byte_range: Range<usize>,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

pub(crate) fn find_workspace_search_matches(
    layout: &WorkspaceLayout,
    query: &str,
    document_snapshots: &BTreeMap<WorkspaceDocumentId, TextSnapshot>,
) -> Vec<WorkspaceSearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    let mut seen = BTreeSet::new();
    for (surface_order, excerpt) in layout.excerpts().iter().enumerate() {
        let Some(snapshot) = document_snapshots.get(&excerpt.spec.document_id) else {
            continue;
        };

        let start_byte = snapshot
            .line_to_byte(excerpt.spec.line_range.start)
            .unwrap_or(0);
        let end_byte = if excerpt.spec.line_range.end < snapshot.line_count() {
            snapshot
                .line_to_byte(excerpt.spec.line_range.end)
                .unwrap_or(snapshot.byte_len())
        } else {
            snapshot.byte_len()
        };
        if start_byte >= end_byte {
            continue;
        }

        let Ok(excerpt_text) = snapshot.slice(start_byte..end_byte) else {
            continue;
        };

        let mut local_start = 0;
        while let Some(offset) = excerpt_text[local_start..].find(query) {
            let match_start = start_byte + local_start + offset;
            let match_end = match_start + query.len();
            if seen.insert((excerpt.spec.document_id, match_start, match_end)) {
                matches.push(WorkspaceSearchMatch {
                    excerpt_id: excerpt.spec.id,
                    document_id: excerpt.spec.document_id,
                    surface_order,
                    byte_range: match_start..match_end,
                });
            }
            local_start += offset + query.len();
        }
    }

    matches
}
