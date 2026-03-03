use std::collections::HashMap;
use std::sync::OnceLock;

use markdown::{
    ParseOptions,
    mdast::{self, Node},
};
use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownCodeSpan {
    pub text: String,
    pub token: MarkdownCodeTokenKind,
}

#[derive(Debug, Default)]
struct MarkdownReferences {
    links: HashMap<String, String>,
}

impl MarkdownReferences {
    fn from_root(root: &mdast::Root) -> Self {
        let mut links = HashMap::new();
        collect_definitions(&root.children, &mut links);
        Self { links }
    }

    fn resolve_link(&self, identifier: &str) -> Option<String> {
        self.links
            .get(&normalize_reference_identifier(identifier))
            .cloned()
    }
}

pub fn parse_markdown_preview(markdown: &str) -> Vec<MarkdownPreviewBlock> {
    if markdown.trim().is_empty() {
        return Vec::new();
    }

    let root = match markdown::to_mdast(markdown, &ParseOptions::gfm()) {
        Ok(Node::Root(root)) => root,
        Ok(other) => mdast::Root {
            children: vec![other],
            position: None,
        },
        Err(_) => {
            return vec![MarkdownPreviewBlock::Paragraph(vec![
                MarkdownInlineSpan::plain(markdown.to_owned()),
            ])];
        }
    };

    let references = MarkdownReferences::from_root(&root);
    let mut blocks = Vec::new();
    for node in &root.children {
        parse_flow_node(node, &references, &mut blocks);
    }
    blocks
}

fn parse_flow_node(
    node: &Node,
    references: &MarkdownReferences,
    out: &mut Vec<MarkdownPreviewBlock>,
) {
    match node {
        Node::Heading(heading) => {
            let spans = parse_inline_nodes(&heading.children, references);
            if !spans.is_empty() {
                out.push(MarkdownPreviewBlock::Heading {
                    level: heading.depth,
                    spans,
                });
            }
        }
        Node::Paragraph(paragraph) => {
            let spans = parse_inline_nodes(&paragraph.children, references);
            if !spans.is_empty() {
                out.push(MarkdownPreviewBlock::Paragraph(spans));
            }
        }
        Node::List(list) => {
            let mut number = list.start.unwrap_or(1) as usize;
            for child in &list.children {
                if let Node::ListItem(item) = child {
                    let spans = parse_container_nodes_as_inline(&item.children, references);
                    if spans.is_empty() {
                        continue;
                    }
                    if list.ordered {
                        out.push(MarkdownPreviewBlock::OrderedListItem { number, spans });
                        number = number.saturating_add(1);
                    } else {
                        out.push(MarkdownPreviewBlock::UnorderedListItem(spans));
                    }
                }
            }
        }
        Node::Blockquote(blockquote) => {
            let spans = parse_container_nodes_as_inline(&blockquote.children, references);
            if !spans.is_empty() {
                out.push(MarkdownPreviewBlock::BlockQuote(spans));
            }
        }
        Node::Code(code) => {
            out.push(MarkdownPreviewBlock::CodeBlock {
                language: code.lang.clone(),
                lines: highlight_code_lines(code.lang.as_deref(), code.value.as_str()),
            });
        }
        Node::Math(math) => {
            out.push(MarkdownPreviewBlock::CodeBlock {
                language: Some("math".to_string()),
                lines: highlight_code_lines(None, math.value.as_str()),
            });
        }
        Node::ThematicBreak(_) => out.push(MarkdownPreviewBlock::ThematicBreak),
        Node::Html(html) => {
            if !html.value.trim().is_empty() {
                out.push(MarkdownPreviewBlock::Paragraph(vec![
                    MarkdownInlineSpan::plain(html.value.clone()),
                ]));
            }
        }
        Node::Table(table) => parse_table_as_blocks(table, references, out),
        Node::Definition(_) => {}
        _ => {
            let spans = parse_inline_nodes(std::slice::from_ref(node), references);
            if !spans.is_empty() {
                out.push(MarkdownPreviewBlock::Paragraph(spans));
            }
        }
    }
}

fn parse_table_as_blocks(
    table: &mdast::Table,
    references: &MarkdownReferences,
    out: &mut Vec<MarkdownPreviewBlock>,
) {
    for row in &table.children {
        let Node::TableRow(row) = row else {
            continue;
        };
        let mut row_spans = Vec::new();
        for (cell_ix, cell) in row.children.iter().enumerate() {
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
            let Node::TableCell(cell) = cell else {
                continue;
            };
            let cell_spans = parse_container_nodes_as_inline(&cell.children, references);
            for span in cell_spans {
                push_inline_span(&mut row_spans, span.text.as_str(), &span.style);
            }
        }
        if !row_spans.is_empty() {
            out.push(MarkdownPreviewBlock::Paragraph(row_spans));
        }
    }
}

fn parse_container_nodes_as_inline(
    nodes: &[Node],
    references: &MarkdownReferences,
) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    let mut has_any = false;
    for node in nodes {
        let child_spans = match node {
            Node::Paragraph(paragraph) => parse_inline_nodes(&paragraph.children, references),
            Node::Heading(heading) => parse_inline_nodes(&heading.children, references),
            Node::Blockquote(blockquote) => {
                parse_container_nodes_as_inline(&blockquote.children, references)
            }
            Node::List(list) => list_children_as_inline(list, references),
            Node::Code(code) => vec![MarkdownInlineSpan {
                text: code.value.clone(),
                style: MarkdownInlineStyle {
                    code: true,
                    ..MarkdownInlineStyle::default()
                },
            }],
            _ => parse_inline_nodes(std::slice::from_ref(node), references),
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

fn list_children_as_inline(
    list: &mdast::List,
    references: &MarkdownReferences,
) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    let mut number = list.start.unwrap_or(1) as usize;
    for child in &list.children {
        if !spans.is_empty() {
            spans.push(MarkdownInlineSpan::plain(" "));
        }
        let Node::ListItem(item) = child else {
            continue;
        };
        let marker = if list.ordered {
            let label = format!("{number}. ");
            number = number.saturating_add(1);
            label
        } else {
            "- ".to_string()
        };
        spans.push(MarkdownInlineSpan::plain(marker));
        spans.extend(parse_container_nodes_as_inline(&item.children, references));
    }
    compact_inline_spans(spans)
}

fn parse_inline_nodes(nodes: &[Node], references: &MarkdownReferences) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    let base = MarkdownInlineStyle::default();
    for node in nodes {
        parse_inline_node(node, &base, references, &mut spans);
    }
    compact_inline_spans(spans)
}

fn parse_inline_node(
    node: &Node,
    style: &MarkdownInlineStyle,
    references: &MarkdownReferences,
    out: &mut Vec<MarkdownInlineSpan>,
) {
    match node {
        Node::Text(text) => push_inline_span(out, text.value.as_str(), style),
        Node::InlineCode(code) => {
            let mut next_style = style.clone();
            next_style.code = true;
            push_inline_span(out, code.value.as_str(), &next_style);
        }
        Node::InlineMath(math) => {
            let mut next_style = style.clone();
            next_style.code = true;
            push_inline_span(out, math.value.as_str(), &next_style);
        }
        Node::Emphasis(node) => {
            let mut next_style = style.clone();
            next_style.italic = true;
            for child in &node.children {
                parse_inline_node(child, &next_style, references, out);
            }
        }
        Node::Strong(node) => {
            let mut next_style = style.clone();
            next_style.bold = true;
            for child in &node.children {
                parse_inline_node(child, &next_style, references, out);
            }
        }
        Node::Delete(node) => {
            let mut next_style = style.clone();
            next_style.strikethrough = true;
            for child in &node.children {
                parse_inline_node(child, &next_style, references, out);
            }
        }
        Node::Link(link) => {
            let mut next_style = style.clone();
            next_style.link = Some(link.url.clone());
            for child in &link.children {
                parse_inline_node(child, &next_style, references, out);
            }
        }
        Node::LinkReference(link_reference) => {
            let mut next_style = style.clone();
            next_style.link = references.resolve_link(link_reference.identifier.as_str());
            for child in &link_reference.children {
                parse_inline_node(child, &next_style, references, out);
            }
        }
        Node::Image(image) => {
            let mut next_style = style.clone();
            next_style.link = Some(image.url.clone());
            let label = if image.alt.is_empty() {
                "image"
            } else {
                image.alt.as_str()
            };
            push_inline_span(out, format!("[{label}]").as_str(), &next_style);
        }
        Node::ImageReference(image_reference) => {
            let mut next_style = style.clone();
            next_style.link = references.resolve_link(image_reference.identifier.as_str());
            let label = if image_reference.alt.is_empty() {
                "image"
            } else {
                image_reference.alt.as_str()
            };
            push_inline_span(out, format!("[{label}]").as_str(), &next_style);
        }
        Node::FootnoteReference(footnote_reference) => {
            push_inline_span(
                out,
                format!("[^{}]", footnote_reference.identifier).as_str(),
                style,
            );
        }
        Node::Break(_) => push_hard_break_span(out, style),
        Node::Html(html) => push_inline_span(out, html.value.as_str(), style),
        Node::MdxTextExpression(expr) => push_inline_span(out, expr.value.as_str(), style),
        _ => {
            if let Some(children) = node.children() {
                for child in children {
                    parse_inline_node(child, style, references, out);
                }
            } else {
                let text = node.to_string();
                if !text.is_empty() {
                    push_inline_span(out, text.as_str(), style);
                }
            }
        }
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
    let mut next_style = style.clone();
    next_style.hard_break = true;
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

fn collect_definitions(nodes: &[Node], links: &mut HashMap<String, String>) {
    for node in nodes {
        if let Node::Definition(definition) = node {
            links.insert(
                normalize_reference_identifier(definition.identifier.as_str()),
                definition.url.clone(),
            );
        }

        if let Some(children) = node.children() {
            collect_definitions(children, links);
        }
    }
}

fn normalize_reference_identifier(identifier: &str) -> String {
    identifier
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn highlight_code_lines(language: Option<&str>, code: &str) -> Vec<Vec<MarkdownCodeSpan>> {
    let syntax_set = syntax_set();
    let syntax = syntax_for_language(syntax_set, language);
    let mut rows = Vec::new();

    match syntax {
        Some(syntax) => {
            let mut parse_state = ParseState::new(syntax);
            let mut scope_stack = ScopeStack::new();
            for line in code.lines() {
                rows.push(highlight_code_line(
                    line,
                    syntax_set,
                    &mut parse_state,
                    &mut scope_stack,
                ));
            }
        }
        None => {
            for line in code.lines() {
                rows.push(vec![MarkdownCodeSpan {
                    text: line.to_owned(),
                    token: MarkdownCodeTokenKind::Plain,
                }]);
            }
        }
    }

    if rows.is_empty() {
        rows.push(vec![MarkdownCodeSpan {
            text: String::new(),
            token: MarkdownCodeTokenKind::Plain,
        }]);
    }

    rows
}

fn highlight_code_line(
    line: &str,
    syntax_set: &SyntaxSet,
    parse_state: &mut ParseState,
    scope_stack: &mut ScopeStack,
) -> Vec<MarkdownCodeSpan> {
    let chars = line.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return vec![MarkdownCodeSpan {
            text: String::new(),
            token: MarkdownCodeTokenKind::Plain,
        }];
    }

    let mut token_map = vec![MarkdownCodeTokenKind::Plain; chars.len()];
    let Ok(ops) = parse_state.parse_line(line, syntax_set) else {
        return vec![MarkdownCodeSpan {
            text: line.to_owned(),
            token: MarkdownCodeTokenKind::Plain,
        }];
    };

    let mut start = 0usize;
    for (region, op) in ScopeRegionIterator::new(&ops, line) {
        let end = (start + region.chars().count()).min(token_map.len());
        let token = if scope_stack.apply(op).is_ok() {
            syntax_token_from_scope_stack(scope_stack)
        } else {
            MarkdownCodeTokenKind::Plain
        };
        for kind in token_map.iter_mut().take(end).skip(start) {
            *kind = token;
        }
        start = end;
        if start >= token_map.len() {
            break;
        }
    }

    merge_code_spans(&chars, &token_map)
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

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_nonewlines)
}

fn syntax_for_language<'a>(
    syntax_set: &'a SyntaxSet,
    language: Option<&str>,
) -> Option<&'a SyntaxReference> {
    let hint = language?.trim();
    if hint.is_empty() {
        return None;
    }

    let lower = hint.to_ascii_lowercase();
    if let Some(syntax) = syntax_set.find_syntax_by_token(lower.as_str()) {
        return Some(syntax);
    }
    if let Some(syntax) = syntax_set.find_syntax_by_extension(lower.as_str()) {
        return Some(syntax);
    }
    if let Some(tokens) = language_tokens_for_hint(lower.as_str())
        && let Some(syntax) = find_first_syntax_by_tokens(syntax_set, tokens)
    {
        return Some(syntax);
    }

    None
}

fn find_first_syntax_by_tokens<'a>(
    syntax_set: &'a SyntaxSet,
    tokens: &[&str],
) -> Option<&'a SyntaxReference> {
    tokens
        .iter()
        .find_map(|token| syntax_set.find_syntax_by_token(token))
}

fn language_tokens_for_hint(hint: &str) -> Option<&'static [&'static str]> {
    match hint {
        "js" | "jsx" | "javascript" => Some(&["js", "javascript"]),
        "ts" | "tsx" | "typescript" => Some(&["ts", "typescript", "js"]),
        "rs" | "rust" => Some(&["rs", "rust"]),
        "py" | "python" => Some(&["py", "python"]),
        "go" => Some(&["go"]),
        "json" | "jsonc" => Some(&["json", "js"]),
        "yml" | "yaml" => Some(&["yaml", "yml"]),
        "toml" => Some(&["toml"]),
        "bash" | "sh" | "zsh" | "shell" => Some(&["bash", "sh"]),
        "c" | "h" => Some(&["c", "cpp"]),
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "c++" => Some(&["cpp", "c++", "c"]),
        "java" => Some(&["java"]),
        "kotlin" | "kt" | "kts" => Some(&["kotlin", "java"]),
        "swift" => Some(&["swift"]),
        "markdown" | "md" => Some(&["markdown", "md"]),
        _ => None,
    }
}

fn syntax_token_from_scope_stack(scope_stack: &ScopeStack) -> MarkdownCodeTokenKind {
    for scope in scope_stack.as_slice().iter().rev() {
        let scope_name = scope.build_string();
        if is_comment_scope(&scope_name) {
            return MarkdownCodeTokenKind::Comment;
        }
        if is_string_scope(&scope_name) {
            return MarkdownCodeTokenKind::String;
        }
        if is_number_scope(&scope_name) {
            return MarkdownCodeTokenKind::Number;
        }
        if is_function_scope(&scope_name) {
            return MarkdownCodeTokenKind::Function;
        }
        if is_type_scope(&scope_name) {
            return MarkdownCodeTokenKind::TypeName;
        }
        if is_constant_scope(&scope_name) {
            return MarkdownCodeTokenKind::Constant;
        }
        if is_keyword_scope(&scope_name) {
            return MarkdownCodeTokenKind::Keyword;
        }
        if is_variable_scope(&scope_name) {
            return MarkdownCodeTokenKind::Variable;
        }
        if is_operator_scope(&scope_name) {
            return MarkdownCodeTokenKind::Operator;
        }
    }
    MarkdownCodeTokenKind::Plain
}

fn is_comment_scope(scope_name: &str) -> bool {
    scope_name.starts_with("comment")
        || scope_name.contains(".comment.")
        || scope_name.ends_with(".comment")
}

fn is_string_scope(scope_name: &str) -> bool {
    scope_name.starts_with("string")
        || scope_name.contains(".string.")
        || scope_name.ends_with(".string")
}

fn is_number_scope(scope_name: &str) -> bool {
    scope_name.starts_with("constant.numeric")
        || scope_name.contains(".constant.numeric.")
        || scope_name.contains(".number.")
        || scope_name.ends_with(".number")
        || scope_name.ends_with(".numeric")
}

fn is_function_scope(scope_name: &str) -> bool {
    scope_name.starts_with("entity.name.function")
        || scope_name.contains(".entity.name.function.")
        || scope_name.starts_with("support.function")
        || scope_name.contains(".support.function.")
        || scope_name.starts_with("variable.function")
        || scope_name.contains(".variable.function.")
        || scope_name.starts_with("meta.function")
}

fn is_type_scope(scope_name: &str) -> bool {
    scope_name.starts_with("entity.name.type")
        || scope_name.contains(".entity.name.type.")
        || scope_name.starts_with("entity.name.class")
        || scope_name.contains(".entity.name.class.")
        || scope_name.starts_with("support.type")
        || scope_name.contains(".support.type.")
        || scope_name.starts_with("storage.type")
        || scope_name.contains(".storage.type.")
}

fn is_constant_scope(scope_name: &str) -> bool {
    scope_name.starts_with("constant")
        || scope_name.contains(".constant.")
        || scope_name.ends_with(".constant")
}

fn is_keyword_scope(scope_name: &str) -> bool {
    scope_name.starts_with("keyword")
        || scope_name.contains(".keyword.")
        || scope_name.ends_with(".keyword")
        || scope_name.starts_with("storage.modifier")
        || scope_name.contains(".storage.modifier.")
        || scope_name.starts_with("storage.control")
        || scope_name.contains(".storage.control.")
}

fn is_variable_scope(scope_name: &str) -> bool {
    scope_name.starts_with("variable")
        || scope_name.contains(".variable.")
        || scope_name.starts_with("entity.name.variable")
        || scope_name.contains(".entity.name.variable.")
        || scope_name.starts_with("support.variable")
        || scope_name.contains(".support.variable.")
}

fn is_operator_scope(scope_name: &str) -> bool {
    scope_name.starts_with("keyword.operator")
        || scope_name.contains(".keyword.operator.")
        || scope_name.starts_with("punctuation")
        || scope_name.contains(".punctuation.")
}
