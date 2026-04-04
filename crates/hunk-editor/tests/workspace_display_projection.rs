use hunk_editor::{
    DisplayRow, DisplayRowKind, EditorCommand, EditorState, Viewport, WorkspaceDocument,
    WorkspaceDocumentId, WorkspaceExcerptId, WorkspaceExcerptKind, WorkspaceExcerptSpec,
    WorkspaceLayout, build_workspace_projected_snapshot,
};
use hunk_text::{BufferId, TextBuffer};

fn sample_editor(text: &str) -> EditorState {
    EditorState::new(TextBuffer::new(BufferId::new(1), text))
}

#[test]
fn workspace_projected_snapshot_tracks_folded_and_wrapped_excerpt_rows() {
    let mut left = sample_editor("one\ntwo\nthree\nfour\nfive");
    left.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 0,
        visible_row_count: 20,
        horizontal_offset: 0,
    }));
    left.apply(EditorCommand::FoldLines {
        start_line: 1,
        end_line: 3,
    });
    let left_rows = left.display_snapshot().visible_rows;

    let mut right = sample_editor("alpha beta");
    right.apply(EditorCommand::SetWrapWidth(Some(5)));
    right.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 0,
        visible_row_count: 20,
        horizontal_offset: 0,
    }));
    let right_rows = right.display_snapshot().visible_rows;

    let left_document_id = WorkspaceDocumentId::new(1);
    let right_document_id = WorkspaceDocumentId::new(2);
    let layout = WorkspaceLayout::new(
        vec![
            WorkspaceDocument::new(left_document_id, "left.rs", BufferId::new(11), 5),
            WorkspaceDocument::new(right_document_id, "right.rs", BufferId::new(21), 1),
        ],
        vec![
            WorkspaceExcerptSpec::new(
                WorkspaceExcerptId::new(1),
                left_document_id,
                WorkspaceExcerptKind::FullFile,
                0..5,
            ),
            WorkspaceExcerptSpec::new(
                WorkspaceExcerptId::new(2),
                right_document_id,
                WorkspaceExcerptKind::FullFile,
                0..1,
            ),
        ],
        1,
    )
    .expect("workspace layout should build");

    let snapshot = build_workspace_projected_snapshot(
        &layout,
        Viewport {
            first_visible_row: 0,
            visible_row_count: 10,
            horizontal_offset: 0,
        },
        |excerpt| match excerpt.spec.document_id {
            id if id == left_document_id => left_rows.clone(),
            id if id == right_document_id => right_rows.clone(),
            _ => Vec::new(),
        },
    );

    assert_eq!(snapshot.total_display_rows, 6);
    assert_eq!(
        snapshot
            .visible_rows
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two  … 2 hidden lines", "five", "", "alpha", " beta"]
    );

    let folded = &snapshot.visible_rows[1];
    assert!(matches!(
        folded.kind,
        DisplayRowKind::FoldPlaceholder {
            hidden_line_count: 2
        }
    ));
    assert_eq!(folded.workspace_row_range, Some(1..4));

    let wrapped_first = &snapshot.visible_rows[4];
    let wrapped_second = &snapshot.visible_rows[5];
    assert_eq!(wrapped_first.workspace_row_range, Some(6..7));
    assert_eq!(wrapped_second.workspace_row_range, Some(6..7));
    assert!(wrapped_second.is_wrapped);
}

#[test]
fn workspace_projected_snapshot_preserves_search_highlights_across_excerpts() {
    let document_id = WorkspaceDocumentId::new(1);
    let other_document_id = WorkspaceDocumentId::new(2);
    let layout = WorkspaceLayout::new(
        vec![
            WorkspaceDocument::new(document_id, "main.rs", BufferId::new(11), 1),
            WorkspaceDocument::new(other_document_id, "lib.rs", BufferId::new(21), 1),
        ],
        vec![
            WorkspaceExcerptSpec::new(
                WorkspaceExcerptId::new(1),
                document_id,
                WorkspaceExcerptKind::FullFile,
                0..1,
            ),
            WorkspaceExcerptSpec::new(
                WorkspaceExcerptId::new(2),
                other_document_id,
                WorkspaceExcerptKind::FullFile,
                0..1,
            ),
        ],
        0,
    )
    .expect("workspace layout should build");

    let first = DisplayRow {
        row_index: 0,
        kind: DisplayRowKind::Text,
        source_line: 0,
        raw_start_column: 0,
        raw_end_column: 11,
        raw_column_offsets: (0..=11).collect(),
        start_column: 0,
        end_column: 11,
        text: "needle main".to_string(),
        is_wrapped: false,
        whitespace_markers: Vec::new(),
        search_highlights: vec![hunk_editor::SearchHighlight {
            start_column: 0,
            end_column: 6,
        }],
        overlays: Vec::new(),
    };
    let second = DisplayRow {
        row_index: 0,
        kind: DisplayRowKind::Text,
        source_line: 0,
        raw_start_column: 0,
        raw_end_column: 10,
        raw_column_offsets: (0..=10).collect(),
        start_column: 0,
        end_column: 10,
        text: "lib needle".to_string(),
        is_wrapped: false,
        whitespace_markers: Vec::new(),
        search_highlights: vec![hunk_editor::SearchHighlight {
            start_column: 4,
            end_column: 10,
        }],
        overlays: Vec::new(),
    };

    let snapshot = build_workspace_projected_snapshot(
        &layout,
        Viewport {
            first_visible_row: 0,
            visible_row_count: 2,
            horizontal_offset: 0,
        },
        |excerpt| match excerpt.spec.document_id {
            id if id == document_id => vec![first.clone()],
            id if id == other_document_id => vec![second.clone()],
            _ => Vec::new(),
        },
    );

    assert_eq!(snapshot.total_display_rows, 2);
    assert_eq!(snapshot.visible_rows[0].search_highlights, first.search_highlights);
    assert_eq!(snapshot.visible_rows[1].search_highlights, second.search_highlights);
}
