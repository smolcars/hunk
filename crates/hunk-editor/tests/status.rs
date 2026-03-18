use hunk_editor::{EditorCommand, EditorState, OverlayDescriptor, OverlayKind, Viewport};
use hunk_language::{LanguageId, ParseStatus};
use hunk_text::{BufferId, TextBuffer};

#[test]
fn editor_state_tracks_dirty_language_parse_status_and_overlays() {
    let buffer = TextBuffer::new(BufferId::new(3), "fn main() {}\n");
    let mut editor = EditorState::new(buffer);

    let output = editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 4,
        visible_row_count: 20,
        horizontal_offset: 2,
    }));
    assert!(output.viewport_changed);

    editor.apply(EditorCommand::SetLanguage(Some(LanguageId::new(9))));
    editor.apply(EditorCommand::SetParseStatus(ParseStatus::Parsing));
    editor.apply(EditorCommand::SetOverlays(vec![OverlayDescriptor {
        line: 0,
        kind: OverlayKind::DiagnosticWarning,
        message: Some("warn".to_string()),
    }]));
    editor.apply(EditorCommand::ReplaceAll(
        "fn answer() -> i32 { 42 }\n".to_string(),
    ));

    let display = editor.display_snapshot();
    assert_eq!(display.viewport.first_visible_row, 0);
    assert!(display.dirty);
    assert_eq!(display.language_id, Some(LanguageId::new(9)));
    assert_eq!(display.parse_status, ParseStatus::Parsing);
    assert_eq!(display.visible_rows[0].overlays.len(), 1);

    editor.apply(EditorCommand::MarkSaved);
    assert!(!editor.is_dirty());
}
