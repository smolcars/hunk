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
    assert_eq!(display.total_display_rows, 2);
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
