use hunk_domain::markdown_preview::{
    MarkdownCodeTokenKind, MarkdownPreviewBlock, parse_markdown_preview,
};

#[test]
fn parses_inline_styles_for_bold_italic_strike_and_code() {
    let markdown = "alpha **bold** *italic* ~~gone~~ `code`";
    let blocks = parse_markdown_preview(markdown);
    assert_eq!(blocks.len(), 1);

    let MarkdownPreviewBlock::Paragraph(spans) = &blocks[0] else {
        panic!("expected paragraph block");
    };

    assert!(
        spans
            .iter()
            .any(|span| span.text == "bold" && span.style.bold)
    );
    assert!(
        spans
            .iter()
            .any(|span| span.text == "italic" && span.style.italic)
    );
    assert!(
        spans
            .iter()
            .any(|span| span.text == "gone" && span.style.strikethrough)
    );
    assert!(
        spans
            .iter()
            .any(|span| span.text == "code" && span.style.code)
    );
}

#[test]
fn parses_headings_lists_and_quotes() {
    let markdown = "\
# Title

- one
2. two
> quote";
    let blocks = parse_markdown_preview(markdown);

    assert!(matches!(
        &blocks[0],
        MarkdownPreviewBlock::Heading { level: 1, .. }
    ));
    assert!(matches!(
        &blocks[1],
        MarkdownPreviewBlock::UnorderedListItem(spans)
            if spans.iter().any(|span| span.text.contains("one"))
    ));
    assert!(matches!(
        &blocks[2],
        MarkdownPreviewBlock::OrderedListItem { number: 2, spans }
            if spans.iter().any(|span| span.text.contains("two"))
    ));
    assert!(matches!(
        &blocks[3],
        MarkdownPreviewBlock::BlockQuote(spans)
            if spans.iter().any(|span| span.text.contains("quote"))
    ));
}

#[test]
fn parses_fenced_code_blocks_with_syntax_tokens() {
    let markdown = "\
```rust
fn main() {
    println!(\"hello\");
}
```";

    let blocks = parse_markdown_preview(markdown);
    assert_eq!(blocks.len(), 1);

    let MarkdownPreviewBlock::CodeBlock { language, lines } = &blocks[0] else {
        panic!("expected code block");
    };
    assert_eq!(language.as_deref(), Some("rust"));
    assert_eq!(lines.len(), 3);

    let has_non_plain = lines
        .iter()
        .flatten()
        .any(|span| span.token != MarkdownCodeTokenKind::Plain);
    assert!(has_non_plain);
}

#[test]
fn resolves_reference_links() {
    let markdown = "\
[hello][id]

[id]: https://example.com";
    let blocks = parse_markdown_preview(markdown);
    assert_eq!(blocks.len(), 1);

    let MarkdownPreviewBlock::Paragraph(spans) = &blocks[0] else {
        panic!("expected paragraph block");
    };
    assert!(spans.iter().any(|span| {
        span.text == "hello" && span.style.link.as_deref() == Some("https://example.com")
    }));
}

#[test]
fn preserves_hard_line_breaks() {
    let markdown = "first line  \nsecond line";
    let blocks = parse_markdown_preview(markdown);
    assert_eq!(blocks.len(), 1);

    let MarkdownPreviewBlock::Paragraph(spans) = &blocks[0] else {
        panic!("expected paragraph block");
    };

    assert!(spans.iter().any(|span| span.text == "first line"));
    assert!(spans.iter().any(|span| span.style.hard_break));
    assert!(spans.iter().any(|span| span.text == "second line"));
}
