use std::path::Path;

use hunk_language::{
    CompletionTriggerKind, LanguageRegistry, ParseStatus, SemanticToken, SemanticTokenKind,
    SyntaxSession, merge_highlight_layers, semantic_token_captures,
};
use hunk_text::{TextPosition, TextRange};

#[test]
fn rust_source_parses_and_highlights_keywords() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn main() {\n    let answer = 42;\n}\n";

    let snapshot = session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");
    assert_eq!(snapshot.parse_status, ParseStatus::Ready);

    let captures = session
        .highlight_visible_range(&registry, source, 0..source.len())
        .expect("highlights");
    assert!(
        captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        captures
            .iter()
            .any(|capture| { capture.style_key == "function" || capture.style_key == "variable" })
    );
}

#[test]
fn html_injection_highlights_embedded_javascript_and_css() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "<html><body><script>const answer = 42;</script><style>.card { color: red; }</style></body></html>";

    session
        .parse_for_path(&registry, Path::new("index.html"), source)
        .expect("parse html");
    let captures = session
        .highlight_visible_range(&registry, source, 0..source.len())
        .expect("html highlights");

    let const_offset = source.find("const").expect("const");
    let color_offset = source.find("color").expect("color");
    assert!(captures.iter().any(|capture| {
        capture.style_key == "keyword"
            && capture.byte_range.start <= const_offset
            && capture.byte_range.end >= const_offset + "const".len()
    }));
    assert!(captures.iter().any(|capture| {
        capture.style_key == "property"
            && capture.byte_range.start <= color_offset
            && capture.byte_range.end >= color_offset + "color".len()
    }));
}

#[test]
fn fold_candidates_cover_multiline_rust_blocks() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn main() {\n    if true {\n        println!(\"hi\");\n    }\n}\n";

    session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");
    let folds = session.fold_candidates(&registry, source);

    assert!(
        folds
            .iter()
            .any(|fold| fold.start_line == 0 && fold.end_line >= 4)
    );
    assert!(
        folds
            .iter()
            .any(|fold| fold.start_line == 1 && fold.end_line >= 3)
    );
}

#[test]
fn semantic_tokens_override_syntax_captures_when_layers_merge() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn greet(name: &str) {\n    println!(\"{name}\");\n}\n";

    session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");
    let syntax = session
        .highlight_visible_range(&registry, source, 0..source.len())
        .expect("syntax highlights");
    let semantic = semantic_token_captures(
        source,
        &[SemanticToken {
            range: TextRange::new(TextPosition::new(0, 9), TextPosition::new(0, 13)),
            kind: SemanticTokenKind::Parameter,
            modifiers: Vec::new(),
        }],
        0..source.len(),
    );
    let merged = merge_highlight_layers(&syntax, &semantic);

    let name_start = source.find("name").expect("name offset");
    let capture = merged
        .iter()
        .rev()
        .find(|capture| {
            capture.byte_range.start <= name_start && name_start < capture.byte_range.end
        })
        .expect("merged capture for semantic token");

    assert_eq!(capture.style_key, "variable.parameter");
}

#[test]
fn hover_and_definition_targets_map_the_symbol_under_cursor() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn main() {\n    let answer = compute_value();\n}\n";

    session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");

    let hover = session
        .hover_target_at(source, TextPosition::new(1, 9))
        .expect("hover target");
    assert_eq!(hover.text, "answer");
    assert_eq!(
        hover.range,
        TextRange::new(TextPosition::new(1, 8), TextPosition::new(1, 14))
    );

    let definition = session
        .definition_target_at(source, TextPosition::new(1, 18))
        .expect("definition target");
    assert_eq!(definition.text, "compute_value");
    assert_eq!(
        definition.range,
        TextRange::new(TextPosition::new(1, 17), TextPosition::new(1, 30))
    );
}

#[test]
fn completion_context_tracks_identifier_prefix_and_replace_range() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn main() {\n    answ\n}\n";

    session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");
    let completion = session
        .completion_context_at(
            source,
            TextPosition::new(1, 8),
            CompletionTriggerKind::Invoked,
        )
        .expect("completion context");

    assert_eq!(completion.prefix, "answ");
    assert_eq!(
        completion.replace_range,
        TextRange::new(TextPosition::new(1, 4), TextPosition::new(1, 8))
    );
}
