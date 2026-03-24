use std::collections::BTreeMap;

use gpui::{Font, FontStyle, FontWeight, Hsla, rgb};
use hunk_editor::{DisplayRow, DisplayRowKind, OverlayDescriptor};

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScrollDirection {
    Forward,
    Backward,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct FilesEditorPalette {
    background: Hsla,
    active_line_background: Hsla,
    line_number: Hsla,
    current_line_number: Hsla,
    border: Hsla,
    default_foreground: Hsla,
    muted_foreground: Hsla,
    selection_background: Hsla,
    cursor: Hsla,
    invisible: Hsla,
    indent_guide: Hsla,
    fold_marker: Hsla,
    current_scope: Hsla,
    bracket_match: Hsla,
    diagnostic_error: Hsla,
    diagnostic_warning: Hsla,
    diagnostic_info: Hsla,
    diff_addition: Hsla,
    diff_deletion: Hsla,
    diff_modification: Hsla,
}

#[path = "../src/app/native_files_editor_paint.rs"]
mod native_files_editor_paint;

use native_files_editor_paint::{ResolvedSyntaxStyle, RowSyntaxSpan, build_text_runs_for_row};

#[test]
fn overlapping_markdown_inline_spans_flatten_into_valid_text_runs() {
    let row_text = "`crates/hunk-domain` tail".to_string();
    let code_span_end = "`crates/hunk-domain`".chars().count();
    let row = DisplayRow {
        row_index: 0,
        kind: DisplayRowKind::Text,
        source_line: 0,
        raw_start_column: 0,
        raw_end_column: row_text.chars().count(),
        raw_column_offsets: (0..=row_text.chars().count()).collect(),
        start_column: 0,
        end_column: row_text.chars().count(),
        text: row_text.clone(),
        is_wrapped: false,
        whitespace_markers: Vec::new(),
        search_highlights: Vec::new(),
        overlays: Vec::<OverlayDescriptor>::new(),
    };

    let syntax_spans = vec![
        RowSyntaxSpan {
            start_column: 0,
            end_column: code_span_end,
            style_key: "text.literal".to_string(),
        },
        RowSyntaxSpan {
            start_column: 0,
            end_column: 1,
            style_key: "punctuation.delimiter".to_string(),
        },
        RowSyntaxSpan {
            start_column: code_span_end - 1,
            end_column: code_span_end,
            style_key: "punctuation.delimiter".to_string(),
        },
    ];

    let mut syntax_styles = BTreeMap::new();
    syntax_styles.insert(
        "text.literal".to_string(),
        ResolvedSyntaxStyle {
            color: Some(rgb(0x445566).into()),
            background_color: None,
            underline: None,
            strikethrough: None,
            font_weight: Some(FontWeight::MEDIUM),
            font_style: None,
        },
    );
    syntax_styles.insert(
        "punctuation.delimiter".to_string(),
        ResolvedSyntaxStyle {
            color: Some(rgb(0xaa5500).into()),
            background_color: None,
            underline: None,
            strikethrough: None,
            font_weight: Some(FontWeight::BOLD),
            font_style: Some(FontStyle::Italic),
        },
    );

    let runs = build_text_runs_for_row(
        &row,
        &syntax_spans,
        &syntax_styles,
        Font::default(),
        rgb(0xffffff).into(),
        rgb(0x999999).into(),
    );

    assert_eq!(
        runs.iter().map(|run| run.len).sum::<usize>(),
        row_text.len()
    );
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].len, 1);
    assert_eq!(runs[1].len, code_span_end - 1);
    assert_eq!(runs[2].len, row_text.len() - code_span_end);
    assert_eq!(runs[0].color, rgb(0xaa5500).into());
    assert_eq!(runs[1].color, rgb(0x445566).into());
    assert_eq!(runs[2].color, rgb(0xffffff).into());
}
