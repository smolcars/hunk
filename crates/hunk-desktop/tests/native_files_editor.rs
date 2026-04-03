#[allow(dead_code)]
#[path = "../src/app/native_files_editor.rs"]
mod native_files_editor;

use gpui::Keystroke;
use hunk_editor::{Viewport, WorkspaceRowKind};
use hunk_language::{CompletionTriggerKind, Diagnostic, DiagnosticSeverity};
use hunk_text::{Selection, TextPosition, TextRange};
use native_files_editor::FilesEditor;
use std::path::{Path, PathBuf};

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
fn arrow_keys_move_the_caret_across_lines() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    editor
        .open_document(path.as_path(), "alpha\nbeta\ngamma")
        .expect("document should open");
    editor.set_selection_for_test(Selection::caret(TextPosition::new(1, 2)));

    assert!(editor.handle_keystroke(&Keystroke::parse("up").expect("valid key")));
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(0, 2))
    );

    assert!(editor.handle_keystroke(&Keystroke::parse("down").expect("valid key")));
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(1, 2))
    );
}

#[test]
fn primary_shortcut_arrow_moves_to_line_boundary() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    editor
        .open_document(path.as_path(), "const answer = 42;")
        .expect("document should open");
    editor.set_selection_for_test(Selection::caret(TextPosition::new(0, 5)));

    assert!(editor.handle_keystroke(&line_boundary_keystroke(false)));
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(0, 18))
    );

    assert!(editor.handle_keystroke(&line_boundary_keystroke(true)));
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(0, 0))
    );
}

#[test]
fn primary_shortcut_shift_arrow_extends_to_line_boundary() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    editor
        .open_document(path.as_path(), "const answer = 42;")
        .expect("document should open");
    editor.set_selection_for_test(Selection::caret(TextPosition::new(0, 6)));

    assert!(editor.handle_keystroke(&line_boundary_selection_keystroke(false)));
    assert_eq!(
        editor.selection_for_test(),
        Selection::new(TextPosition::new(0, 6), TextPosition::new(0, 18))
    );
}

#[test]
fn word_navigation_moves_to_word_boundaries() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.ts");
    editor
        .open_document(path.as_path(), "const query_string = value;")
        .expect("document should open");
    editor.set_selection_for_test(Selection::caret(TextPosition::new(0, 0)));

    assert!(editor.move_word_action(true, false));
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(0, 5))
    );

    assert!(editor.move_word_action(true, false));
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(0, 18))
    );

    assert!(editor.move_word_action(false, false));
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(0, 6))
    );
}

#[test]
fn document_boundary_navigation_moves_to_top_and_bottom() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.ts");
    editor
        .open_document(path.as_path(), "first line\nsecond line\nthird")
        .expect("document should open");
    editor.set_selection_for_test(Selection::caret(TextPosition::new(1, 4)));

    assert!(editor.move_to_document_boundary_action(true, false));
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(0, 0))
    );

    assert!(editor.move_to_document_boundary_action(false, false));
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(2, 5))
    );
}

#[test]
fn document_boundary_selection_extends_from_anchor() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.ts");
    editor
        .open_document(path.as_path(), "first line\nsecond line\nthird")
        .expect("document should open");
    editor.set_selection_for_test(Selection::caret(TextPosition::new(1, 2)));

    assert!(editor.move_to_document_boundary_action(false, true));
    assert_eq!(
        editor.selection_for_test(),
        Selection::new(TextPosition::new(1, 2), TextPosition::new(2, 5))
    );
}

#[test]
fn double_click_selects_the_containing_word() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.ts");
    editor
        .open_document(path.as_path(), "const queryString = value;")
        .expect("document should open");

    assert!(editor.begin_pointer_selection_for_test(TextPosition::new(0, 8), false, 2,));
    assert_eq!(
        editor.selection_for_test(),
        Selection::new(TextPosition::new(0, 6), TextPosition::new(0, 17))
    );
}

#[test]
fn double_click_drag_extends_by_word() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.ts");
    editor
        .open_document(path.as_path(), "const queryString = sampleValue;")
        .expect("document should open");

    assert!(editor.begin_pointer_selection_for_test(TextPosition::new(0, 8), false, 2));
    assert!(editor.drag_pointer_selection_for_test(TextPosition::new(0, 24)));
    assert_eq!(
        editor.selection_for_test(),
        Selection::new(TextPosition::new(0, 6), TextPosition::new(0, 31))
    );
}

#[test]
fn triple_click_selects_the_entire_line() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.ts");
    editor
        .open_document(path.as_path(), "const value = 1;\nnext line")
        .expect("document should open");

    assert!(editor.begin_pointer_selection_for_test(TextPosition::new(0, 7), false, 3,));
    assert_eq!(
        editor.selection_for_test(),
        Selection::new(TextPosition::new(0, 0), TextPosition::new(0, 16))
    );
}

#[test]
fn triple_click_drag_extends_by_line() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.ts");
    editor
        .open_document(path.as_path(), "const value = 1;\nnext line\nthird line")
        .expect("document should open");

    assert!(editor.begin_pointer_selection_for_test(TextPosition::new(0, 7), false, 3));
    assert!(editor.drag_pointer_selection_for_test(TextPosition::new(1, 2)));
    assert_eq!(
        editor.selection_for_test(),
        Selection::new(TextPosition::new(0, 0), TextPosition::new(1, 9))
    );
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
fn opening_a_file_builds_a_full_file_workspace_layout() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    editor
        .open_document(path.as_path(), "one\ntwo\nthree\n")
        .expect("document should open");

    let layout = editor
        .workspace_layout_for_test()
        .expect("workspace layout should exist");
    assert_eq!(layout.documents().len(), 1);
    assert_eq!(layout.excerpts().len(), 1);
    assert_eq!(layout.total_rows(), 4);
    assert!(editor.active_workspace_document_id_for_test().is_some());
    assert!(editor.active_workspace_excerpt_id_for_test().is_some());

    let first_row = layout.locate_row(0).expect("first row should resolve");
    assert_eq!(first_row.row_kind, WorkspaceRowKind::Content);
    assert_eq!(first_row.document_line, Some(0));

    let last_row = layout.locate_row(3).expect("last row should resolve");
    assert_eq!(last_row.row_kind, WorkspaceRowKind::Content);
    assert_eq!(last_row.document_line, Some(3));
}

#[test]
fn workspace_documents_switch_active_buffers_without_reloading_text() {
    let mut editor = FilesEditor::new();
    editor
        .open_workspace_documents(
            vec![
                (PathBuf::from("src/main.rs"), "fn main() {}\n".to_string()),
                (
                    PathBuf::from("src/lib.rs"),
                    "pub fn helper() {}\n".to_string(),
                ),
            ],
            Some(Path::new("src/main.rs")),
        )
        .expect("workspace documents should open");

    editor.set_selection_for_test(Selection::caret(TextPosition::new(0, 12)));
    assert!(editor.paste_text("// main"));
    editor.set_viewport_for_test(Viewport {
        first_visible_row: 2,
        visible_row_count: 4,
        horizontal_offset: 0,
    });

    assert!(
        editor
            .activate_workspace_path(Path::new("src/lib.rs"))
            .expect("workspace path switch should succeed")
    );
    assert_eq!(
        editor.current_text().as_deref(),
        Some("pub fn helper() {}\n")
    );

    editor.set_selection_for_test(Selection::caret(TextPosition::new(0, 3)));
    assert!(editor.paste_text("// lib"));

    assert!(
        editor
            .activate_workspace_path(Path::new("src/main.rs"))
            .expect("workspace path switch should succeed")
    );
    assert_eq!(
        editor.current_text().as_deref(),
        Some("fn main() {}// main\n")
    );
    assert_eq!(editor.viewport_for_test().first_visible_row, 2);
    assert_eq!(
        editor.selection_for_test(),
        Selection::caret(TextPosition::new(0, 19))
    );

    assert!(
        editor
            .activate_workspace_path(Path::new("src/lib.rs"))
            .expect("workspace path switch should succeed")
    );
    assert_eq!(
        editor.current_text().as_deref(),
        Some("pub// lib fn helper() {}\n")
    );
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

#[test]
fn language_intelligence_requests_and_diagnostics_flow_through_native_editor() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    let contents = "fn main() {\n    let answer = compute_value();\n}\n";
    editor
        .open_document(path.as_path(), contents)
        .expect("document should open");
    editor.set_selection_for_test(Selection::caret(TextPosition::new(1, 19)));

    let hover = editor
        .request_hover_at_cursor()
        .expect("hover request should exist");
    assert_eq!(hover.target.text, "compute_value");
    assert_eq!(
        editor
            .take_pending_hover_request()
            .expect("pending hover request")
            .target
            .text,
        "compute_value"
    );

    let definition = editor
        .request_definition_at_cursor()
        .expect("definition request should exist");
    assert_eq!(definition.target.text, "compute_value");
    assert_eq!(
        editor
            .take_pending_definition_request()
            .expect("pending definition request")
            .target
            .text,
        "compute_value"
    );

    editor.set_selection_for_test(Selection::caret(TextPosition::new(1, 14)));
    let completion = editor
        .trigger_completion(CompletionTriggerKind::Invoked)
        .expect("completion request should exist");
    assert_eq!(completion.context.prefix, "answer");
    assert_eq!(
        editor
            .take_pending_completion_request()
            .expect("pending completion request")
            .context
            .prefix,
        "answer"
    );

    editor.set_diagnostics(vec![Diagnostic {
        range: TextRange::new(TextPosition::new(1, 8), TextPosition::new(1, 14)),
        severity: DiagnosticSeverity::Warning,
        message: "shadowed binding".to_string(),
        source: Some("test".to_string()),
        code: Some("W1".to_string()),
    }]);
    assert_eq!(
        editor
            .display_snapshot_for_test(120, 12)
            .visible_rows
            .iter()
            .flat_map(|row| row.overlays.iter())
            .filter(|overlay| matches!(overlay.kind, hunk_editor::OverlayKind::DiagnosticWarning))
            .count(),
        1
    );
}

#[test]
fn scrolling_large_file_extends_visible_highlight_cache_range() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("example.rs");
    let contents = (0..3000)
        .map(|index| format!("const VALUE_{index}: usize = {index};"))
        .collect::<Vec<_>>()
        .join("\n");
    editor
        .open_document(path.as_path(), contents.as_str())
        .expect("document should open");

    editor.set_viewport_for_test(Viewport {
        first_visible_row: 0,
        visible_row_count: 35,
        horizontal_offset: 0,
    });
    editor.display_snapshot_for_test(120, 35);
    let initial_range = editor
        .visible_highlight_range_for_test()
        .expect("initial highlight cache");

    editor.set_viewport_for_test(Viewport {
        first_visible_row: 500,
        visible_row_count: 35,
        horizontal_offset: 0,
    });
    editor.display_snapshot_for_test(120, 35);
    let extended_range = editor
        .visible_highlight_range_for_test()
        .expect("extended highlight cache");

    assert_eq!(extended_range.start, initial_range.start);
    assert!(extended_range.end > initial_range.end);
}

#[test]
fn markdown_edits_keep_native_editor_layout_and_syntax_caches_consistent() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("README.md");
    editor
        .open_document(
            path.as_path(),
            "# Hunk\n\n- item\n\n```rust\nfn main() {}\n```\n",
        )
        .expect("document should open");

    let initial_snapshot = editor.display_snapshot_for_test(120, 20);
    let initial_spans = editor.row_syntax_spans(&initial_snapshot.visible_rows);
    assert!(
        !initial_spans.is_empty(),
        "markdown syntax spans should exist before edits"
    );

    assert!(editor.handle_keystroke(&Keystroke::parse("enter").expect("valid key")));
    let after_enter_snapshot = editor.display_snapshot_for_test(120, 20);
    let after_enter_spans = editor.row_syntax_spans(&after_enter_snapshot.visible_rows);
    assert!(
        !after_enter_spans.is_empty(),
        "markdown syntax spans should exist after enter"
    );

    assert!(editor.handle_keystroke(&Keystroke::parse("shift-g->G").expect("valid key")));
    let after_shift_g_snapshot = editor.display_snapshot_for_test(120, 20);
    let after_shift_g_spans = editor.row_syntax_spans(&after_shift_g_snapshot.visible_rows);
    assert!(
        !after_shift_g_spans.is_empty(),
        "markdown syntax spans should exist after inserting uppercase text"
    );
}

#[test]
fn wrapped_markdown_rows_preserve_text_around_inline_code() {
    let mut editor = FilesEditor::new();
    let path = PathBuf::from("README.md");
    let line = "The immediate problem is not the PTY or VT surface. Hunk already has a PTY-backed terminal surface in `crates/hunk-terminal`. The compatibility gap is in:";
    editor
        .open_document(path.as_path(), format!("{line}\n").as_str())
        .expect("document should open");

    let snapshot = editor.display_snapshot_for_test(40, 20);
    let rejoined = snapshot
        .visible_rows
        .iter()
        .filter(|row| row.source_line == 0)
        .map(|row| row.text.as_str())
        .collect::<String>();

    assert_eq!(rejoined, line);
}

fn primary_shortcut_keystroke(key: &str) -> Keystroke {
    let shortcut = if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    };
    Keystroke::parse(shortcut.as_str()).expect("valid shortcut")
}

fn line_boundary_keystroke(start: bool) -> Keystroke {
    let shortcut = if cfg!(target_os = "macos") {
        if start { "cmd-left" } else { "cmd-right" }
    } else if start {
        "home"
    } else {
        "end"
    };
    Keystroke::parse(shortcut).expect("valid line boundary shortcut")
}

fn line_boundary_selection_keystroke(start: bool) -> Keystroke {
    let shortcut = if cfg!(target_os = "macos") {
        if start {
            "shift-cmd-left"
        } else {
            "shift-cmd-right"
        }
    } else if start {
        "shift-home"
    } else {
        "shift-end"
    };
    Keystroke::parse(shortcut).expect("valid line boundary selection shortcut")
}
