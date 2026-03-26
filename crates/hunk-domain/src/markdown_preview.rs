use comrak::nodes::{ListType, NodeCodeBlock, NodeLink, NodeList, NodeValue};
use hunk_language::preview_highlight_spans_for_language_hint;
use std::time::{Duration, Instant};

type ComrakNode<'a> = comrak::nodes::Node<'a>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownPreviewBlock {
    Heading {
        level: u8,
        spans: Vec<MarkdownInlineSpan>,
    },
    Paragraph(Vec<MarkdownInlineSpan>),
    UnorderedListItem(Vec<MarkdownInlineSpan>),
    OrderedListItem {
        number: usize,
        spans: Vec<MarkdownInlineSpan>,
    },
    BlockQuote(Vec<MarkdownInlineSpan>),
    CodeBlock {
        language: Option<String>,
        lines: Vec<Vec<MarkdownCodeSpan>>,
    },
    ThematicBreak,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarkdownInlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub hard_break: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownInlineSpan {
    pub text: String,
    pub style: MarkdownInlineStyle,
}

impl MarkdownInlineSpan {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: MarkdownInlineStyle::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownCodeTokenKind {
    Plain,
    Keyword,
    String,
    Number,
    Comment,
    Function,
    TypeName,
    Constant,
    Variable,
    Operator,
}

impl From<hunk_language::PreviewSyntaxToken> for MarkdownCodeTokenKind {
    fn from(value: hunk_language::PreviewSyntaxToken) -> Self {
        match value {
            hunk_language::PreviewSyntaxToken::Plain => Self::Plain,
            hunk_language::PreviewSyntaxToken::Keyword => Self::Keyword,
            hunk_language::PreviewSyntaxToken::String => Self::String,
            hunk_language::PreviewSyntaxToken::Number => Self::Number,
            hunk_language::PreviewSyntaxToken::Comment => Self::Comment,
            hunk_language::PreviewSyntaxToken::Function => Self::Function,
            hunk_language::PreviewSyntaxToken::TypeName => Self::TypeName,
            hunk_language::PreviewSyntaxToken::Constant => Self::Constant,
            hunk_language::PreviewSyntaxToken::Variable => Self::Variable,
            hunk_language::PreviewSyntaxToken::Operator => Self::Operator,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownCodeSpan {
    pub text: String,
    pub token: MarkdownCodeTokenKind,
}

#[derive(Debug, Clone, Default)]
pub struct MarkdownPreviewParseStats {
    pub comrak_parse: Duration,
    pub transform: Duration,
    pub code_highlight: Duration,
    pub code_block_count: usize,
    pub code_char_count: usize,
}

pub fn parse_markdown_preview(markdown: &str) -> Vec<MarkdownPreviewBlock> {
    parse_markdown_preview_with_stats(markdown).0
}

pub fn parse_markdown_preview_with_stats(
    markdown: &str,
) -> (Vec<MarkdownPreviewBlock>, MarkdownPreviewParseStats) {
    if markdown.trim().is_empty() {
        return (Vec::new(), MarkdownPreviewParseStats::default());
    }

    let arena = comrak::Arena::new();
    let options = markdown_parse_options();
    let comrak_parse_started_at = Instant::now();
    let root = comrak::parse_document(&arena, markdown, &options);
    let comrak_parse = comrak_parse_started_at.elapsed();

    let transform_started_at = Instant::now();
    let mut stats = MarkdownPreviewParseStats {
        comrak_parse,
        ..MarkdownPreviewParseStats::default()
    };
    let mut blocks = Vec::new();
    for node in root.children() {
        parse_flow_node(node, &mut blocks, &mut stats);
    }
    stats.transform = transform_started_at.elapsed();
    (blocks, stats)
}

fn markdown_parse_options() -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options
}

fn parse_flow_node(
    node: ComrakNode<'_>,
    out: &mut Vec<MarkdownPreviewBlock>,
    stats: &mut MarkdownPreviewParseStats,
) {
    let data = node.data();
    match &data.value {
        NodeValue::Heading(heading) => {
            let spans = parse_inline_nodes(node);
            if !spans.is_empty() {
                out.push(MarkdownPreviewBlock::Heading {
                    level: heading.level,
                    spans,
                });
            }
        }
        NodeValue::Paragraph => {
            let spans = parse_inline_nodes(node);
            if !spans.is_empty() {
                out.push(MarkdownPreviewBlock::Paragraph(spans));
            }
        }
        NodeValue::List(list) => {
            let list = *list;
            drop(data);
            parse_list_block(node, list, out);
        }
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) | NodeValue::Alert(_) => {
            let spans = parse_container_nodes_as_inline(node);
            if !spans.is_empty() {
                out.push(MarkdownPreviewBlock::BlockQuote(spans));
            }
        }
        NodeValue::CodeBlock(code) => {
            let language = code_block_language_hint(code);
            let literal = code.literal.as_str();
            out.push(MarkdownPreviewBlock::CodeBlock {
                language: language.clone(),
                lines: highlight_code_lines(language.as_deref(), literal, stats),
            });
        }
        NodeValue::Math(math) => {
            let literal = math.literal.as_str();
            out.push(MarkdownPreviewBlock::CodeBlock {
                language: Some("math".to_string()),
                lines: highlight_code_lines(None, literal, stats),
            });
        }
        NodeValue::ThematicBreak => out.push(MarkdownPreviewBlock::ThematicBreak),
        NodeValue::HtmlBlock(html) => {
            if !html.literal.trim().is_empty() {
                out.push(MarkdownPreviewBlock::Paragraph(vec![
                    MarkdownInlineSpan::plain(html.literal.clone()),
                ]));
            }
        }
        NodeValue::Table(..) => {
            drop(data);
            parse_table_as_blocks(node, out);
        }
        _ => {
            let spans = parse_container_nodes_as_inline(node);
            if !spans.is_empty() {
                out.push(MarkdownPreviewBlock::Paragraph(spans));
            }
        }
    }
}

fn parse_list_block(
    list_node: ComrakNode<'_>,
    list: NodeList,
    out: &mut Vec<MarkdownPreviewBlock>,
) {
    let mut number = list.start;
    for child in list_node.children() {
        let child_data = child.data();
        if !matches!(
            child_data.value,
            NodeValue::Item(..) | NodeValue::TaskItem(..)
        ) {
            continue;
        }
        drop(child_data);

        let spans = parse_container_nodes_as_inline(child);
        if spans.is_empty() {
            continue;
        }

        if list.list_type == ListType::Ordered {
            out.push(MarkdownPreviewBlock::OrderedListItem { number, spans });
            number = number.saturating_add(1);
        } else {
            out.push(MarkdownPreviewBlock::UnorderedListItem(spans));
        }
    }
}

fn code_block_language_hint(code: &NodeCodeBlock) -> Option<String> {
    code.info
        .split_whitespace()
        .next()
        .map(str::trim)
        .filter(|hint| !hint.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_table_as_blocks(node: ComrakNode<'_>, out: &mut Vec<MarkdownPreviewBlock>) {
    for row in node.children() {
        let row_data = row.data();
        if !matches!(row_data.value, NodeValue::TableRow(..)) {
            continue;
        }
        drop(row_data);

        let mut row_spans = Vec::new();
        for (cell_ix, cell) in row.children().enumerate() {
            if cell_ix > 0 {
                push_inline_span(
                    &mut row_spans,
                    " | ",
                    &MarkdownInlineStyle {
                        code: true,
                        ..MarkdownInlineStyle::default()
                    },
                );
            }

            let cell_data = cell.data();
            if !matches!(cell_data.value, NodeValue::TableCell) {
                continue;
            }
            drop(cell_data);

            let cell_spans = parse_container_nodes_as_inline(cell);
            for span in cell_spans {
                push_inline_span(&mut row_spans, span.text.as_str(), &span.style);
            }
        }

        if !row_spans.is_empty() {
            out.push(MarkdownPreviewBlock::Paragraph(row_spans));
        }
    }
}

fn parse_container_nodes_as_inline(node: ComrakNode<'_>) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    let mut has_any = false;

    for child in node.children() {
        let child_data = child.data();
        let child_spans = match &child_data.value {
            NodeValue::Paragraph
            | NodeValue::Heading(..)
            | NodeValue::TableCell
            | NodeValue::DescriptionTerm
            | NodeValue::Subtext => {
                drop(child_data);
                parse_inline_nodes(child)
            }
            NodeValue::BlockQuote
            | NodeValue::MultilineBlockQuote(_)
            | NodeValue::Alert(_)
            | NodeValue::DescriptionDetails => {
                drop(child_data);
                parse_container_nodes_as_inline(child)
            }
            NodeValue::List(list) => {
                let list = *list;
                drop(child_data);
                list_children_as_inline(child, list)
            }
            NodeValue::CodeBlock(code) => {
                let literal = code.literal.clone();
                drop(child_data);
                vec![MarkdownInlineSpan {
                    text: literal,
                    style: MarkdownInlineStyle {
                        code: true,
                        ..MarkdownInlineStyle::default()
                    },
                }]
            }
            NodeValue::HtmlBlock(html) => {
                let literal = html.literal.clone();
                drop(child_data);
                vec![MarkdownInlineSpan::plain(literal)]
            }
            _ => {
                drop(child_data);
                parse_inline_nodes(child)
            }
        };

        if child_spans.is_empty() {
            continue;
        }
        if has_any && !spans_end_with_whitespace(&spans) {
            spans.push(MarkdownInlineSpan::plain(" "));
        }
        spans.extend(child_spans);
        has_any = true;
    }

    compact_inline_spans(spans)
}

fn list_children_as_inline(list_node: ComrakNode<'_>, list: NodeList) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    let mut number = list.start;

    for child in list_node.children() {
        if !spans.is_empty() {
            spans.push(MarkdownInlineSpan::plain(" "));
        }

        let child_data = child.data();
        if !matches!(
            child_data.value,
            NodeValue::Item(..) | NodeValue::TaskItem(..)
        ) {
            continue;
        }
        drop(child_data);

        let marker = if list.list_type == ListType::Ordered {
            let label = format!("{number}. ");
            number = number.saturating_add(1);
            label
        } else {
            "- ".to_string()
        };
        spans.push(MarkdownInlineSpan::plain(marker));
        spans.extend(parse_container_nodes_as_inline(child));
    }

    compact_inline_spans(spans)
}

fn parse_inline_nodes(node: ComrakNode<'_>) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    let base = MarkdownInlineStyle::default();
    for child in node.children() {
        parse_inline_node(child, &base, &mut spans);
    }
    compact_inline_spans(spans)
}

fn parse_inline_node(
    node: ComrakNode<'_>,
    style: &MarkdownInlineStyle,
    out: &mut Vec<MarkdownInlineSpan>,
) {
    let data = node.data();
    match &data.value {
        NodeValue::Text(text) => push_inline_span(out, text.as_ref(), style),
        NodeValue::Code(code) => {
            let next_style = updated_inline_style(style, |next| next.code = true);
            push_inline_span(out, code.literal.as_str(), &next_style);
        }
        NodeValue::Math(math) => {
            let next_style = updated_inline_style(style, |next| next.code = true);
            push_inline_span(out, math.literal.as_str(), &next_style);
        }
        NodeValue::Emph => {
            let next_style = updated_inline_style(style, |next| next.italic = true);
            drop(data);
            push_inline_children(node, &next_style, out);
        }
        NodeValue::Strong => {
            let next_style = updated_inline_style(style, |next| next.bold = true);
            drop(data);
            push_inline_children(node, &next_style, out);
        }
        NodeValue::Strikethrough => {
            let next_style = updated_inline_style(style, |next| next.strikethrough = true);
            drop(data);
            push_inline_children(node, &next_style, out);
        }
        NodeValue::Link(link) => {
            let next_style = updated_inline_style(style, |next| next.link = Some(link.url.clone()));
            drop(data);
            push_inline_children(node, &next_style, out);
        }
        NodeValue::WikiLink(link) => {
            let next_style = updated_inline_style(style, |next| next.link = Some(link.url.clone()));
            drop(data);
            push_inline_children(node, &next_style, out);
        }
        NodeValue::Image(image) => parse_image_inline(node, image.as_ref(), style, out),
        NodeValue::FootnoteReference(footnote_reference) => {
            push_inline_span(
                out,
                format!("[^{}]", footnote_reference.name).as_str(),
                style,
            );
        }
        NodeValue::LineBreak => push_hard_break_span(out, style),
        NodeValue::SoftBreak => push_inline_span(out, " ", style),
        NodeValue::HtmlInline(html) => push_inline_span(out, html.as_str(), style),
        NodeValue::Raw(raw) => push_inline_span(out, raw.as_str(), style),
        NodeValue::FrontMatter(front_matter) => push_inline_span(out, front_matter.as_str(), style),
        NodeValue::EscapedTag(tag) => push_inline_span(out, tag, style),
        _ => {
            drop(data);
            for child in node.children() {
                parse_inline_node(child, style, out);
            }
        }
    }
}

fn updated_inline_style(
    style: &MarkdownInlineStyle,
    update: impl FnOnce(&mut MarkdownInlineStyle),
) -> MarkdownInlineStyle {
    let mut next_style = style.clone();
    update(&mut next_style);
    next_style
}

fn parse_image_inline(
    node: ComrakNode<'_>,
    image: &NodeLink,
    style: &MarkdownInlineStyle,
    out: &mut Vec<MarkdownInlineSpan>,
) {
    let next_style = updated_inline_style(style, |next| next.link = Some(image.url.clone()));
    let mut image_spans = Vec::new();
    for child in node.children() {
        parse_inline_node(child, &next_style, &mut image_spans);
    }
    if image_spans.is_empty() {
        push_inline_span(out, "[image]", &next_style);
    } else {
        out.extend(image_spans);
    }
}

fn push_inline_children(
    node: ComrakNode<'_>,
    style: &MarkdownInlineStyle,
    out: &mut Vec<MarkdownInlineSpan>,
) {
    for child in node.children() {
        parse_inline_node(child, style, out);
    }
}

fn push_inline_span(out: &mut Vec<MarkdownInlineSpan>, text: &str, style: &MarkdownInlineStyle) {
    if text.is_empty() {
        return;
    }

    if let Some(last) = out.last_mut()
        && last.style == *style
    {
        last.text.push_str(text);
        return;
    }

    out.push(MarkdownInlineSpan {
        text: text.to_owned(),
        style: style.clone(),
    });
}

fn push_hard_break_span(out: &mut Vec<MarkdownInlineSpan>, style: &MarkdownInlineStyle) {
    let next_style = updated_inline_style(style, |next| next.hard_break = true);
    out.push(MarkdownInlineSpan {
        text: String::new(),
        style: next_style,
    });
}

fn compact_inline_spans(spans: Vec<MarkdownInlineSpan>) -> Vec<MarkdownInlineSpan> {
    let mut compacted: Vec<MarkdownInlineSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if span.style.hard_break {
            compacted.push(span);
            continue;
        }
        if span.text.is_empty() {
            continue;
        }
        if let Some(last) = compacted.last_mut()
            && last.style == span.style
        {
            last.text.push_str(span.text.as_str());
            continue;
        }
        compacted.push(span);
    }
    compacted
}

fn spans_end_with_whitespace(spans: &[MarkdownInlineSpan]) -> bool {
    if spans.last().is_some_and(|span| span.style.hard_break) {
        return true;
    }
    spans
        .last()
        .and_then(|span| span.text.chars().last())
        .is_some_and(char::is_whitespace)
}

fn highlight_code_lines(
    language: Option<&str>,
    code: &str,
    stats: &mut MarkdownPreviewParseStats,
) -> Vec<Vec<MarkdownCodeSpan>> {
    stats.code_block_count = stats.code_block_count.saturating_add(1);
    stats.code_char_count = stats.code_char_count.saturating_add(code.len());

    let line_texts = code.lines().collect::<Vec<_>>();
    if line_texts.is_empty() {
        return vec![vec![MarkdownCodeSpan {
            text: String::new(),
            token: MarkdownCodeTokenKind::Plain,
        }]];
    }

    let mut rows = line_texts
        .iter()
        .map(|line| vec![MarkdownCodeTokenKind::Plain; line.chars().count()])
        .collect::<Vec<_>>();
    let line_offsets = line_byte_offsets(&line_texts);
    let line_char_offsets = line_texts
        .iter()
        .zip(line_offsets.iter())
        .map(|(line, (line_start, _))| char_byte_offsets(line, *line_start))
        .collect::<Vec<_>>();

    let highlight_started_at = Instant::now();
    for span in preview_highlight_spans_for_language_hint(language, code) {
        let token = MarkdownCodeTokenKind::from(span.token);
        for (line_index, (line_start, line_end)) in line_offsets.iter().enumerate() {
            let overlap_start = span.byte_range.start.max(*line_start);
            let overlap_end = span.byte_range.end.min(*line_end);
            if overlap_start >= overlap_end {
                continue;
            }
            mark_code_range(
                &line_char_offsets[line_index],
                &mut rows[line_index],
                overlap_start,
                overlap_end,
                token,
            );
        }
    }
    stats.code_highlight = stats
        .code_highlight
        .saturating_add(highlight_started_at.elapsed());

    line_texts
        .into_iter()
        .zip(rows)
        .map(|(line, token_map)| {
            let chars = line.chars().collect::<Vec<_>>();
            if chars.is_empty() {
                vec![MarkdownCodeSpan {
                    text: String::new(),
                    token: MarkdownCodeTokenKind::Plain,
                }]
            } else {
                merge_code_spans(&chars, &token_map)
            }
        })
        .collect()
}

fn merge_code_spans(chars: &[char], token_map: &[MarkdownCodeTokenKind]) -> Vec<MarkdownCodeSpan> {
    if chars.is_empty() {
        return vec![MarkdownCodeSpan {
            text: String::new(),
            token: MarkdownCodeTokenKind::Plain,
        }];
    }

    let mut spans = Vec::new();
    let mut start = 0usize;
    let mut current = token_map[0];
    for index in 1..=chars.len() {
        let boundary = index == chars.len() || token_map[index] != current;
        if !boundary {
            continue;
        }
        spans.push(MarkdownCodeSpan {
            text: chars[start..index].iter().collect::<String>(),
            token: current,
        });
        if index < chars.len() {
            start = index;
            current = token_map[index];
        }
    }
    spans
}

fn line_byte_offsets(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut start = 0;
    lines
        .iter()
        .map(|line| {
            let end = start + line.len();
            let range = (start, end);
            start = end + 1;
            range
        })
        .collect()
}

fn char_byte_offsets(line: &str, line_start: usize) -> Vec<usize> {
    line.char_indices()
        .map(|(offset, _)| line_start + offset)
        .chain(std::iter::once(line_start + line.len()))
        .collect()
}

fn mark_code_range(
    char_offsets: &[usize],
    token_map: &mut [MarkdownCodeTokenKind],
    start_byte: usize,
    end_byte: usize,
    token: MarkdownCodeTokenKind,
) {
    if start_byte >= end_byte {
        return;
    }

    for (index, kind) in token_map.iter_mut().enumerate() {
        let char_start = char_offsets[index];
        let char_end = char_offsets[index + 1];
        if char_end <= start_byte {
            continue;
        }
        if char_start >= end_byte {
            break;
        }
        *kind = token;
    }
}
