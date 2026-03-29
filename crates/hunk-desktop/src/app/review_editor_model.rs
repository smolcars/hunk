use std::collections::BTreeSet;

use hunk_domain::diff::{DiffCellKind, DiffRowKind, SideBySideRow};
use hunk_editor::{OverlayDescriptor, OverlayKind};

pub(crate) fn build_review_editor_overlays(
    rows: &[SideBySideRow],
) -> (Vec<OverlayDescriptor>, Vec<OverlayDescriptor>) {
    let mut left = BTreeSet::new();
    let mut right = BTreeSet::new();

    for row in rows {
        if row.kind != DiffRowKind::Code {
            continue;
        }

        match (row.left.line, row.left.kind, row.right.line, row.right.kind) {
            (Some(left_line), DiffCellKind::Removed, Some(right_line), DiffCellKind::Added) => {
                left.insert((
                    left_line.saturating_sub(1) as usize,
                    OverlayKind::DiffModification,
                ));
                right.insert((
                    right_line.saturating_sub(1) as usize,
                    OverlayKind::DiffModification,
                ));
            }
            (Some(left_line), DiffCellKind::Removed, _, _) => {
                left.insert((
                    left_line.saturating_sub(1) as usize,
                    OverlayKind::DiffDeletion,
                ));
            }
            (_, _, Some(right_line), DiffCellKind::Added) => {
                right.insert((
                    right_line.saturating_sub(1) as usize,
                    OverlayKind::DiffAddition,
                ));
            }
            _ => {}
        }
    }

    (overlays_from_entries(left), overlays_from_entries(right))
}

fn overlays_from_entries(entries: BTreeSet<(usize, OverlayKind)>) -> Vec<OverlayDescriptor> {
    entries
        .into_iter()
        .map(|(line, kind)| OverlayDescriptor {
            line,
            kind,
            message: None,
        })
        .collect()
}
