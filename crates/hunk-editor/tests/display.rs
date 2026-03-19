use hunk_editor::{
    DisplayRowKind, EditorCommand, EditorState, OverlayDescriptor, OverlayKind, Viewport,
};
use hunk_text::{BufferId, TextBuffer, TextPosition};

fn sample_editor(text: &str) -> EditorState {
    EditorState::new(TextBuffer::new(BufferId::new(1), text))
}

#[test]
fn wrapping_projects_single_line_into_multiple_rows() {
    let mut editor = sample_editor("abcdefghij");
    editor.apply(EditorCommand::SetWrapWidth(Some(4)));
    editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 0,
        visible_row_count: 10,
        horizontal_offset: 0,
    }));

    let display = editor.display_snapshot();
    assert_eq!(display.total_display_rows, 3);
    assert_eq!(display.visible_rows[0].text, "abcd");
    assert_eq!(display.visible_rows[1].text, "efgh");
    assert_eq!(display.visible_rows[2].text, "ij");
}

#[test]
fn folded_regions_create_placeholder_rows() {
    let mut editor = sample_editor("one\ntwo\nthree\nfour\n");
    editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 0,
        visible_row_count: 10,
        horizontal_offset: 0,
    }));
    editor.apply(EditorCommand::FoldLines {
        start_line: 1,
        end_line: 3,
    });

    let display = editor.display_snapshot();
    assert_eq!(display.total_display_rows, 3);
    assert!(matches!(
        display.visible_rows[1].kind,
        DisplayRowKind::FoldPlaceholder {
            hidden_line_count: 2
        }
    ));
}

#[test]
fn move_up_and_down_follow_wrapped_rows() {
    let mut editor = sample_editor("abcdefghij\n");
    editor.apply(EditorCommand::SetWrapWidth(Some(4)));
    editor.apply(EditorCommand::SetSelection(hunk_text::Selection::caret(
        TextPosition::new(0, 6),
    )));

    editor.apply(EditorCommand::MoveUp);
    let status = editor.status_snapshot();
    assert_eq!(status.cursor_line, 1);
    assert_eq!(status.cursor_column, 3);

    editor.apply(EditorCommand::MoveDown);
    let status = editor.status_snapshot();
    assert_eq!(status.cursor_column, 7);
}

#[test]
fn search_and_overlays_are_projected_into_visible_rows() {
    let mut editor = sample_editor("alpha beta alpha\n");
    editor.apply(EditorCommand::SetWrapWidth(Some(20)));
    editor.apply(EditorCommand::SetSearchQuery(Some("alpha".to_string())));
    editor.apply(EditorCommand::SetOverlays(vec![OverlayDescriptor {
        line: 0,
        kind: OverlayKind::DiffModification,
        message: None,
    }]));

    let display = editor.display_snapshot();
    assert_eq!(display.visible_rows[0].search_highlights.len(), 2);
    assert_eq!(display.visible_rows[0].overlays.len(), 1);
}

#[test]
fn multiline_search_matches_project_with_line_relative_columns() {
    let mut editor = sample_editor("zero\nalpha beta\nomega alpha\n");
    editor.apply(EditorCommand::SetWrapWidth(Some(40)));
    editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 0,
        visible_row_count: 10,
        horizontal_offset: 0,
    }));
    editor.apply(EditorCommand::SetSearchQuery(Some("alpha".to_string())));

    let display = editor.display_snapshot();

    assert_eq!(display.visible_rows[1].search_highlights.len(), 1);
    assert_eq!(display.visible_rows[1].search_highlights[0].start_column, 0);
    assert_eq!(display.visible_rows[1].search_highlights[0].end_column, 5);

    assert_eq!(display.visible_rows[2].search_highlights.len(), 1);
    assert_eq!(display.visible_rows[2].search_highlights[0].start_column, 6);
    assert_eq!(display.visible_rows[2].search_highlights[0].end_column, 11);
}

#[test]
fn copy_cut_and_paste_flow_through_editor_commands() {
    let mut editor = sample_editor("hello world");
    editor.apply(EditorCommand::SetSelection(hunk_text::Selection::new(
        TextPosition::new(0, 6),
        TextPosition::new(0, 11),
    )));

    let copied = editor.apply(EditorCommand::CopySelection);
    assert_eq!(copied.copied_text.as_deref(), Some("world"));

    let cut = editor.apply(EditorCommand::CutSelection);
    assert_eq!(cut.copied_text.as_deref(), Some("world"));
    assert_eq!(editor.buffer().text(), "hello ");

    editor.apply(EditorCommand::Paste("there".to_string()));
    assert_eq!(editor.buffer().text(), "hello there");
}

#[test]
fn display_rows_preserve_raw_to_display_offsets_for_tab_expansion() {
    let mut editor = sample_editor("\tword\n");
    editor.apply(EditorCommand::SetWrapWidth(Some(16)));

    let display = editor.display_snapshot();
    assert_eq!(display.visible_rows[0].text, "    word");
    assert_eq!(display.visible_rows[0].raw_start_column, 0);
    assert_eq!(display.visible_rows[0].raw_end_column, 5);
    assert_eq!(
        display.visible_rows[0].raw_column_offsets,
        vec![0, 4, 5, 6, 7, 8]
    );
}

#[test]
fn blank_lines_project_to_one_display_row() {
    let mut editor = sample_editor("one\n\ntwo");
    editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 0,
        visible_row_count: 10,
        horizontal_offset: 0,
    }));

    let display = editor.display_snapshot();
    assert_eq!(display.total_display_rows, 3);
    assert_eq!(display.visible_rows[0].source_line, 0);
    assert_eq!(display.visible_rows[1].source_line, 1);
    assert_eq!(display.visible_rows[2].source_line, 2);
    assert_eq!(display.visible_rows[1].text, "");
}

#[test]
fn wrapped_content_keeps_blank_lines_single_row() {
    let mut editor = sample_editor("abcdefghij\n\nxy");
    editor.apply(EditorCommand::SetWrapWidth(Some(4)));
    editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 0,
        visible_row_count: 10,
        horizontal_offset: 0,
    }));

    let display = editor.display_snapshot();
    assert_eq!(display.total_display_rows, 5);
    let blank_rows = display
        .visible_rows
        .iter()
        .filter(|row| row.source_line == 1)
        .collect::<Vec<_>>();
    assert_eq!(blank_rows.len(), 1);
    assert_eq!(blank_rows[0].text, "");
}

#[test]
fn viewport_changes_slice_cached_rows_without_rebuilding_behavior() {
    let mut editor = sample_editor("one\ntwo\nthree\nfour\n");
    editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 0,
        visible_row_count: 2,
        horizontal_offset: 0,
    }));

    let first = editor.display_snapshot();
    assert_eq!(
        first
            .visible_rows
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two"]
    );

    editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 2,
        visible_row_count: 2,
        horizontal_offset: 0,
    }));
    let second = editor.display_snapshot();
    assert_eq!(
        second
            .visible_rows
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        vec!["three", "four"]
    );
}

#[test]
fn search_query_changes_invalidate_cached_display_rows() {
    let mut editor = sample_editor("alpha beta alpha\n");
    editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 0,
        visible_row_count: 5,
        horizontal_offset: 0,
    }));

    let before = editor.display_snapshot();
    assert!(before.visible_rows[0].search_highlights.is_empty());

    editor.apply(EditorCommand::SetSearchQuery(Some("alpha".to_string())));
    let after = editor.display_snapshot();
    assert_eq!(after.visible_rows[0].search_highlights.len(), 2);
}
