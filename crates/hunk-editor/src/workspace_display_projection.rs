use std::cmp::min;
use std::ops::Range;

use crate::{
    DisplayRow, DisplayRowKind, OverlayDescriptor, SearchHighlight, Viewport, WhitespaceMarker,
    WorkspaceExcerptLayout, WorkspaceLayout, WorkspaceRowKind, WorkspaceRowLocation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceProjectedRow {
    pub row_index: usize,
    pub workspace_row_range: Option<Range<usize>>,
    pub location: Option<WorkspaceRowLocation>,
    pub kind: DisplayRowKind,
    pub raw_start_column: usize,
    pub raw_end_column: usize,
    pub raw_column_offsets: Vec<usize>,
    pub start_column: usize,
    pub end_column: usize,
    pub text: String,
    pub is_wrapped: bool,
    pub whitespace_markers: Vec<WhitespaceMarker>,
    pub search_highlights: Vec<SearchHighlight>,
    pub overlays: Vec<OverlayDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceProjectedSnapshot {
    pub viewport: Viewport,
    pub total_display_rows: usize,
    pub visible_rows: Vec<WorkspaceProjectedRow>,
}

pub fn build_workspace_projected_snapshot<F>(
    layout: &WorkspaceLayout,
    viewport: Viewport,
    mut display_rows_for_excerpt: F,
) -> WorkspaceProjectedSnapshot
where
    F: FnMut(&WorkspaceExcerptLayout) -> Vec<DisplayRow>,
{
    let projected_rows = build_projected_rows(layout, &mut display_rows_for_excerpt);
    let total_display_rows = projected_rows.len();
    let start = viewport.first_visible_row.min(total_display_rows);
    let end = min(start.saturating_add(viewport.visible_row_count), total_display_rows);

    WorkspaceProjectedSnapshot {
        viewport,
        total_display_rows,
        visible_rows: projected_rows[start..end].to_vec(),
    }
}

fn build_projected_rows<F>(
    layout: &WorkspaceLayout,
    display_rows_for_excerpt: &mut F,
) -> Vec<WorkspaceProjectedRow>
where
    F: FnMut(&WorkspaceExcerptLayout) -> Vec<DisplayRow>,
{
    let mut row_index = 0usize;
    let mut projected_rows = Vec::new();

    for (excerpt_ix, excerpt) in layout.excerpts().iter().enumerate() {
        for row in display_rows_for_excerpt(excerpt) {
            projected_rows.push(project_display_row(excerpt, row_index, &row));
            row_index = row_index.saturating_add(1);
        }

        if layout.gap_rows() > 0 && excerpt_ix + 1 < layout.excerpts().len() {
            for _ in 0..layout.gap_rows() {
                projected_rows.push(WorkspaceProjectedRow {
                    row_index,
                    workspace_row_range: None,
                    location: None,
                    kind: DisplayRowKind::Text,
                    raw_start_column: 0,
                    raw_end_column: 0,
                    raw_column_offsets: vec![0],
                    start_column: 0,
                    end_column: 0,
                    text: String::new(),
                    is_wrapped: false,
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                    overlays: Vec::new(),
                });
                row_index = row_index.saturating_add(1);
            }
        }
    }

    projected_rows
}

fn project_display_row(
    excerpt: &WorkspaceExcerptLayout,
    row_index: usize,
    row: &DisplayRow,
) -> WorkspaceProjectedRow {
    let content_start = excerpt.content_row_range().start;
    let source_row_start = content_start + row.source_line.saturating_sub(excerpt.spec.line_range.start);
    let hidden_rows = match row.kind {
        DisplayRowKind::Text => 1,
        DisplayRowKind::FoldPlaceholder { hidden_line_count } => hidden_line_count.saturating_add(1),
    };
    let source_row_end = source_row_start.saturating_add(hidden_rows);

    WorkspaceProjectedRow {
        row_index,
        workspace_row_range: Some(source_row_start..source_row_end),
        location: Some(WorkspaceRowLocation {
            excerpt_id: excerpt.spec.id,
            document_id: excerpt.spec.document_id,
            row_kind: WorkspaceRowKind::Content,
            document_line: Some(row.source_line),
            row_in_excerpt: row.source_line.saturating_sub(excerpt.spec.line_range.start),
        }),
        kind: row.kind.clone(),
        raw_start_column: row.raw_start_column,
        raw_end_column: row.raw_end_column,
        raw_column_offsets: row.raw_column_offsets.clone(),
        start_column: row.start_column,
        end_column: row.end_column,
        text: row.text.clone(),
        is_wrapped: row.is_wrapped,
        whitespace_markers: row.whitespace_markers.clone(),
        search_highlights: row.search_highlights.clone(),
        overlays: row.overlays.clone(),
    }
}
