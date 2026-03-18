#[allow(dead_code)]
#[path = "../src/app/native_files_editor.rs"]
mod native_files_editor;

use gpui::Keystroke;
use hunk_editor::Viewport;
use hunk_text::{Selection, TextPosition};
use native_files_editor::FilesEditor;
use std::path::PathBuf;

#[test]
fn select_all_shortcut_and_backspace_clear_the_buffer() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    editor
        .open_document(path.as_path(), "hello world")
        .expect("document should open");

    assert!(editor.handle_keystroke(&primary_shortcut_keystroke("a")));
    assert!(editor.handle_keystroke(&Keystroke::parse("backspace").expect("valid key")));
    assert_eq!(editor.current_text().as_deref(), Some(""));
    assert!(editor.is_dirty());
}

#[test]
fn enter_preserves_existing_indentation() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    editor
        .open_document(path.as_path(), "    hello")
        .expect("document should open");
    editor.set_selection_for_test(Selection::caret(TextPosition::new(0, 4)));

    assert!(editor.handle_keystroke(&Keystroke::parse("enter").expect("valid key")));
    assert_eq!(editor.current_text().as_deref(), Some("    \n    hello"));
}

#[test]
fn reopening_same_file_restores_selection_and_viewport() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    let contents = "one\ntwo\nthree\nfour\nfive\nsix\n";
    editor
        .open_document(path.as_path(), contents)
        .expect("document should open");
    editor.set_selection_for_test(Selection::new(
        TextPosition::new(4, 1),
        TextPosition::new(4, 3),
    ));
    editor.set_viewport_for_test(Viewport {
        first_visible_row: 3,
        visible_row_count: 4,
        horizontal_offset: 0,
    });

    editor
        .open_document(path.as_path(), contents)
        .expect("document should reopen");

    assert_eq!(
        editor.selection_for_test(),
        Selection::new(TextPosition::new(4, 1), TextPosition::new(4, 3))
    );
    assert_eq!(editor.viewport_for_test().first_visible_row, 3);
}

#[test]
fn search_navigation_selects_next_match() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    editor
        .open_document(path.as_path(), "alpha\nneedle one\nbeta\nneedle two\n")
        .expect("document should open");

    editor.set_search_query(Some("needle"));
    assert_eq!(editor.search_match_count(), 2);
    assert!(editor.select_next_search_match(true));
    assert_eq!(
        editor.selection_for_test(),
        Selection::new(TextPosition::new(1, 0), TextPosition::new(1, 6))
    );
    assert!(editor.select_next_search_match(true));
    assert_eq!(
        editor.selection_for_test(),
        Selection::new(TextPosition::new(3, 0), TextPosition::new(3, 6))
    );
}

#[test]
fn reopening_same_file_restores_fold_and_view_toggles() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    let contents = "fn main() {\n    if true {\n        println!(\"hi\");\n    }\n}\n";
    editor
        .open_document(path.as_path(), contents)
        .expect("document should open");

    assert!(editor.toggle_fold_at_line(0));
    assert!(editor.toggle_show_whitespace());
    assert!(editor.toggle_soft_wrap());

    editor
        .open_document(path.as_path(), contents)
        .expect("document should reopen");

    assert_eq!(editor.folded_region_count_for_test(), 1);
    assert!(editor.show_whitespace_for_test());
    assert!(editor.soft_wrap_enabled_for_test());
}

fn primary_shortcut_keystroke(key: &str) -> Keystroke {
    let shortcut = if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    };
    Keystroke::parse(shortcut.as_str()).expect("valid shortcut")
}
