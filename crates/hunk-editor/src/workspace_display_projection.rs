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
    let end = min(
        start.saturating_add(viewport.visible_row_count),
        total_display_rows,
    );

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
        for leading_row_ix in 0..excerpt.spec.leading_rows {
            projected_rows.push(project_chrome_row(
                excerpt,
                row_index,
                leading_row_ix,
                WorkspaceRowKind::LeadingChrome,
            ));
            row_index = row_index.saturating_add(1);
        }

        for row in display_rows_for_excerpt(excerpt) {
            projected_rows.push(project_display_row(excerpt, row_index, &row));
            row_index = row_index.saturating_add(1);
        }

        for trailing_row_ix in 0..excerpt.spec.trailing_rows {
            projected_rows.push(project_chrome_row(
                excerpt,
                row_index,
                excerpt.spec.leading_rows + excerpt.spec.content_row_count() + trailing_row_ix,
                WorkspaceRowKind::TrailingChrome,
            ));
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

fn project_chrome_row(
    excerpt: &WorkspaceExcerptLayout,
    row_index: usize,
    row_in_excerpt: usize,
    row_kind: WorkspaceRowKind,
) -> WorkspaceProjectedRow {
    let raw_row_start = excerpt.global_row_range.start + row_in_excerpt;
    WorkspaceProjectedRow {
        row_index,
        workspace_row_range: Some(raw_row_start..raw_row_start.saturating_add(1)),
        location: Some(WorkspaceRowLocation {
            excerpt_id: excerpt.spec.id,
            document_id: excerpt.spec.document_id,
            row_kind,
            document_line: None,
            row_in_excerpt,
        }),
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
    }
}

fn project_display_row(
    excerpt: &WorkspaceExcerptLayout,
    row_index: usize,
    row: &DisplayRow,
) -> WorkspaceProjectedRow {
    let content_start = excerpt.content_row_range().start;
    let source_row_start = content_start
        + row
            .source_line
            .saturating_sub(excerpt.spec.line_range.start);
    let hidden_rows = match row.kind {
        DisplayRowKind::Text => 1,
        DisplayRowKind::FoldPlaceholder { hidden_line_count } => {
            hidden_line_count.saturating_add(1)
        }
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
            row_in_excerpt: row
                .source_line
                .saturating_sub(excerpt.spec.line_range.start),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DisplayRow, DisplayRowKind, WorkspaceDocument, WorkspaceDocumentId, WorkspaceExcerptId,
        WorkspaceExcerptKind, WorkspaceExcerptSpec, WorkspaceLayout,
    };
    use hunk_text::BufferId;

    #[test]
    fn projected_snapshot_includes_excerpt_chrome_rows() {
        let layout = WorkspaceLayout::new(
            vec![WorkspaceDocument::new(
                WorkspaceDocumentId::new(1),
                "src/main.rs",
                BufferId::new(1),
                3,
            )],
            vec![
                WorkspaceExcerptSpec::new(
                    WorkspaceExcerptId::new(1),
                    WorkspaceDocumentId::new(1),
                    WorkspaceExcerptKind::DiffHunk,
                    0..3,
                )
                .with_chrome_rows(1, 1),
            ],
            0,
        )
        .expect("layout");

        let snapshot = build_workspace_projected_snapshot(
            &layout,
            Viewport {
                first_visible_row: 0,
                visible_row_count: usize::MAX,
                horizontal_offset: 0,
            },
            |_| {
                vec![
                    DisplayRow {
                        row_index: 0,
                        source_line: 0,
                        kind: DisplayRowKind::Text,
                        raw_start_column: 0,
                        raw_end_column: 1,
                        raw_column_offsets: vec![0, 1],
                        start_column: 0,
                        end_column: 1,
                        text: "a".to_string(),
                        is_wrapped: false,
                        whitespace_markers: Vec::new(),
                        search_highlights: Vec::new(),
                        overlays: Vec::new(),
                    },
                    DisplayRow {
                        row_index: 1,
                        source_line: 1,
                        kind: DisplayRowKind::Text,
                        raw_start_column: 0,
                        raw_end_column: 1,
                        raw_column_offsets: vec![0, 1],
                        start_column: 0,
                        end_column: 1,
                        text: "b".to_string(),
                        is_wrapped: false,
                        whitespace_markers: Vec::new(),
                        search_highlights: Vec::new(),
                        overlays: Vec::new(),
                    },
                    DisplayRow {
                        row_index: 2,
                        source_line: 2,
                        kind: DisplayRowKind::Text,
                        raw_start_column: 0,
                        raw_end_column: 1,
                        raw_column_offsets: vec![0, 1],
                        start_column: 0,
                        end_column: 1,
                        text: "c".to_string(),
                        is_wrapped: false,
                        whitespace_markers: Vec::new(),
                        search_highlights: Vec::new(),
                        overlays: Vec::new(),
                    },
                ]
            },
        );

        assert_eq!(snapshot.total_display_rows, 5);
        assert_eq!(
            snapshot
                .visible_rows
                .iter()
                .map(|row| row.workspace_row_range.clone())
                .collect::<Vec<_>>(),
            vec![Some(0..1), Some(1..2), Some(2..3), Some(3..4), Some(4..5),]
        );
        assert_eq!(
            snapshot
                .visible_rows
                .iter()
                .map(|row| row.location.as_ref().map(|location| location.row_kind))
                .collect::<Vec<_>>(),
            vec![
                Some(WorkspaceRowKind::LeadingChrome),
                Some(WorkspaceRowKind::Content),
                Some(WorkspaceRowKind::Content),
                Some(WorkspaceRowKind::Content),
                Some(WorkspaceRowKind::TrailingChrome),
            ]
        );
    }
}
