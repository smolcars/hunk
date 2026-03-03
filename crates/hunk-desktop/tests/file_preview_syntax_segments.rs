#[path = "../src/app/highlight.rs"]
mod highlight;

use highlight::{SyntaxTokenKind, build_plain_line_segments};

#[test]
fn plain_line_segments_detect_language_tokens() {
    let segments = build_plain_line_segments(
        Some("src/main.rs"),
        "fn main() { let total = 1; println!(\"ok\"); }",
    );

    assert!(!segments.is_empty());
    assert!(segments.iter().all(|segment| !segment.changed));
    assert!(
        segments
            .iter()
            .any(|segment| segment.syntax == SyntaxTokenKind::Keyword)
    );
}

#[test]
fn plain_line_segments_fallback_to_plain_for_unknown_extension() {
    let segments = build_plain_line_segments(Some("data.unknownext"), "just some words");

    assert!(!segments.is_empty());
    assert!(
        segments
            .iter()
            .all(|segment| segment.syntax == SyntaxTokenKind::Plain)
    );
    assert!(segments.iter().all(|segment| !segment.changed));
}

#[test]
fn plain_line_segments_highlight_toml_assignments() {
    let segments = build_plain_line_segments(Some("Cargo.toml"), "name = \"hunk\" # app name");

    assert!(!segments.is_empty());
    assert!(
        segments
            .iter()
            .any(|segment| segment.syntax != SyntaxTokenKind::Plain)
    );
    assert!(
        segments
            .iter()
            .any(|segment| segment.syntax == SyntaxTokenKind::String)
    );
    assert!(
        segments
            .iter()
            .any(|segment| segment.syntax == SyntaxTokenKind::Comment)
    );
}
