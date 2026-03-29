use gpui::{ListOffset, Pixels, px};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DiffViewportAnchor {
    pub(super) row_stable_id: u64,
    pub(super) fallback_item_ix: usize,
    pub(super) offset_in_item: Pixels,
}

pub(super) fn viewport_anchor_for_top_row(
    row_stable_ids: &[u64],
    scroll_top: ListOffset,
) -> Option<DiffViewportAnchor> {
    let row_stable_id = *row_stable_ids.get(scroll_top.item_ix)?;
    Some(DiffViewportAnchor {
        row_stable_id,
        fallback_item_ix: scroll_top.item_ix,
        offset_in_item: scroll_top.offset_in_item,
    })
}

pub(super) fn resolve_viewport_anchor(
    anchor: DiffViewportAnchor,
    row_stable_ids: &[u64],
) -> ListOffset {
    let item_ix = row_stable_ids
        .iter()
        .position(|stable_id| *stable_id == anchor.row_stable_id)
        .unwrap_or_else(|| {
            anchor
                .fallback_item_ix
                .min(row_stable_ids.len().saturating_sub(1))
        });

    ListOffset {
        item_ix,
        offset_in_item: if row_stable_ids.is_empty() {
            px(0.)
        } else {
            anchor.offset_in_item
        },
    }
}
