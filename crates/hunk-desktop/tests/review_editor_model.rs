#[path = "../src/app/review_editor_model.rs"]
mod review_editor_model;

use hunk_domain::diff::{DiffCell, DiffCellKind, DiffRowKind, SideBySideRow};
use hunk_editor::OverlayKind;
use review_editor_model::build_review_editor_overlays;

#[test]
fn review_editor_overlays_mark_modified_and_added_lines() {
    let rows = vec![
        SideBySideRow {
            kind: DiffRowKind::Code,
            left: DiffCell {
                line: Some(4),
                text: "before".to_string(),
                kind: DiffCellKind::Removed,
            },
            right: DiffCell {
                line: Some(4),
                text: "after".to_string(),
                kind: DiffCellKind::Added,
            },
            text: String::new(),
        },
        SideBySideRow {
            kind: DiffRowKind::Code,
            left: DiffCell {
                line: None,
                text: String::new(),
                kind: DiffCellKind::None,
            },
            right: DiffCell {
                line: Some(9),
                text: "new".to_string(),
                kind: DiffCellKind::Added,
            },
            text: String::new(),
        },
    ];

    let (left, right) = build_review_editor_overlays(&rows);

    assert_eq!(left.len(), 1);
    assert_eq!(left[0].line, 3);
    assert_eq!(left[0].kind, OverlayKind::DiffModification);
    assert_eq!(right.len(), 2);
    assert_eq!(right[0].line, 3);
    assert_eq!(right[0].kind, OverlayKind::DiffModification);
    assert_eq!(right[1].line, 8);
    assert_eq!(right[1].kind, OverlayKind::DiffAddition);
}
