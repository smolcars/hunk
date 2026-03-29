#[path = "../src/app/diff_viewport.rs"]
mod diff_viewport;

use diff_viewport::{DiffViewportAnchor, resolve_viewport_anchor, viewport_anchor_for_top_row};
use gpui::{ListOffset, px};

#[test]
fn viewport_anchor_captures_visible_row_identity() {
    let row_stable_ids = [11, 22, 33];
    let anchor = viewport_anchor_for_top_row(
        &row_stable_ids,
        ListOffset {
            item_ix: 1,
            offset_in_item: px(14.),
        },
    )
    .expect("row anchor");

    assert_eq!(
        anchor,
        DiffViewportAnchor {
            row_stable_id: 22,
            fallback_item_ix: 1,
            offset_in_item: px(14.),
        }
    );
}

#[test]
fn viewport_anchor_restores_by_stable_row_identity() {
    let restored = resolve_viewport_anchor(
        DiffViewportAnchor {
            row_stable_id: 22,
            fallback_item_ix: 1,
            offset_in_item: px(18.),
        },
        &[44, 22, 55],
    );

    assert_eq!(restored.item_ix, 1);
    assert_eq!(restored.offset_in_item, px(18.));
}

#[test]
fn viewport_anchor_falls_back_to_previous_index_when_row_disappears() {
    let restored = resolve_viewport_anchor(
        DiffViewportAnchor {
            row_stable_id: 99,
            fallback_item_ix: 3,
            offset_in_item: px(9.),
        },
        &[10, 11],
    );

    assert_eq!(restored.item_ix, 1);
    assert_eq!(restored.offset_in_item, px(9.));
}
