use std::ops::Range;

use hunk_domain::diff::{DiffRowKind, SideBySideRow};

use super::{
    REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX, REVIEW_SURFACE_HUNK_DIVIDER_HEIGHT_PX,
    ReviewWorkspaceDisplayRows, ReviewWorkspaceSection,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceDisplayGeometry {
    row_display_boundaries: Vec<usize>,
    row_top_offsets_px: Vec<usize>,
    section_display_row_ranges: Vec<Range<usize>>,
    section_pixel_ranges: Vec<Range<usize>>,
    total_display_rows: usize,
    total_surface_height_px: usize,
}

impl ReviewWorkspaceDisplayGeometry {
    pub(crate) fn build(
        rows: &[SideBySideRow],
        sections: &[ReviewWorkspaceSection],
        display_rows: Option<&ReviewWorkspaceDisplayRows>,
    ) -> Self {
        let mut display_row_counts = vec![1usize; rows.len()];
        if let Some(display_rows) = display_rows {
            let mut counts_by_raw_row = vec![0usize; rows.len()];
            let mut covered_rows = vec![false; rows.len()];
            for entry in &display_rows.rows {
                let start = entry.raw_row_range.start.min(rows.len());
                let end = entry.raw_row_range.end.min(rows.len());
                if start >= end {
                    continue;
                }
                if let Some(count) = counts_by_raw_row.get_mut(start) {
                    *count = count.saturating_add(1);
                }
                for row_ix in start..end {
                    if let Some(covered) = covered_rows.get_mut(row_ix) {
                        *covered = true;
                    }
                }
            }
            for (raw_row, covered) in covered_rows.into_iter().enumerate() {
                if !covered {
                    continue;
                }
                let count = counts_by_raw_row[raw_row];
                if count > 0 {
                    display_row_counts[raw_row] = count;
                } else {
                    display_row_counts[raw_row] = 0;
                }
            }
        }

        let mut row_display_boundaries = Vec::with_capacity(rows.len().saturating_add(1));
        let mut row_top_offsets_px = Vec::with_capacity(rows.len().saturating_add(1));
        row_display_boundaries.push(0);
        row_top_offsets_px.push(0);

        let mut next_display_row = 0usize;
        let mut next_pixel_offset = 0usize;
        for (row_ix, row) in rows.iter().enumerate() {
            let display_row_count = display_row_counts[row_ix];
            next_display_row = next_display_row.saturating_add(display_row_count);
            next_pixel_offset = next_pixel_offset.saturating_add(
                display_row_count.saturating_mul(surface_row_height_for_kind(row.kind)),
            );
            row_display_boundaries.push(next_display_row);
            row_top_offsets_px.push(next_pixel_offset);
        }

        let section_display_row_ranges = sections
            .iter()
            .map(|section| {
                let start = row_display_boundaries
                    .get(section.start_row)
                    .copied()
                    .unwrap_or(0);
                let end = row_display_boundaries
                    .get(section.end_row)
                    .copied()
                    .unwrap_or(start);
                start..end
            })
            .collect::<Vec<_>>();
        let section_pixel_ranges = sections
            .iter()
            .map(|section| {
                let start = row_top_offsets_px
                    .get(section.start_row)
                    .copied()
                    .unwrap_or(0);
                let end = row_top_offsets_px
                    .get(section.end_row)
                    .copied()
                    .unwrap_or(start);
                start..end
            })
            .collect::<Vec<_>>();

        Self {
            row_display_boundaries,
            row_top_offsets_px,
            section_display_row_ranges,
            section_pixel_ranges,
            total_display_rows: next_display_row,
            total_surface_height_px: next_pixel_offset,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn row_display_range(&self, row_ix: usize) -> Option<Range<usize>> {
        let start = *self.row_display_boundaries.get(row_ix)?;
        let end = *self.row_display_boundaries.get(row_ix.saturating_add(1))?;
        Some(start..end)
    }

    #[allow(dead_code)]
    pub(crate) fn row_display_boundary(&self, boundary_ix: usize) -> Option<usize> {
        self.row_display_boundaries.get(boundary_ix).copied()
    }

    #[allow(dead_code)]
    pub(crate) fn row_top_offset_px(&self, row_ix: usize) -> Option<usize> {
        self.row_top_offsets_px.get(row_ix).copied()
    }

    #[allow(dead_code)]
    pub(crate) fn row_boundary_offset_px(&self, boundary_ix: usize) -> Option<usize> {
        self.row_top_offsets_px.get(boundary_ix).copied()
    }

    #[allow(dead_code)]
    pub(crate) fn section_display_row_range(&self, section_ix: usize) -> Option<&Range<usize>> {
        self.section_display_row_ranges.get(section_ix)
    }

    #[allow(dead_code)]
    pub(crate) fn section_pixel_range(&self, section_ix: usize) -> Option<&Range<usize>> {
        self.section_pixel_ranges.get(section_ix)
    }

    #[allow(dead_code)]
    pub(crate) fn total_display_rows(&self) -> usize {
        self.total_display_rows
    }

    #[allow(dead_code)]
    pub(crate) fn total_surface_height_px(&self) -> usize {
        self.total_surface_height_px
    }
}

fn surface_row_height_for_kind(row_kind: DiffRowKind) -> usize {
    match row_kind {
        DiffRowKind::HunkHeader => REVIEW_SURFACE_HUNK_DIVIDER_HEIGHT_PX,
        DiffRowKind::Code | DiffRowKind::Meta | DiffRowKind::Empty => {
            REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX
        }
    }
}
