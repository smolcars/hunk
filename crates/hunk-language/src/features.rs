use std::ops::Range;
use std::path::PathBuf;

use hunk_text::{TextPosition, TextRange};

use crate::{HighlightCapture, LanguageId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentContext {
    pub path: PathBuf,
    pub language_id: Option<LanguageId>,
    pub version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: TextRange,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTokenKind {
    Namespace,
    Type,
    Class,
    Enum,
    Interface,
    Struct,
    TypeParameter,
    Parameter,
    Variable,
    Property,
    EnumMember,
    Event,
    Function,
    Method,
    Macro,
    Keyword,
    Modifier,
    Comment,
    String,
    Number,
    Operator,
    Decorator,
    Label,
}

impl SemanticTokenKind {
    pub fn style_key(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Type
            | Self::Class
            | Self::Enum
            | Self::Interface
            | Self::Struct
            | Self::TypeParameter => "type",
            Self::Parameter => "variable.parameter",
            Self::Variable => "variable",
            Self::Property | Self::EnumMember => "property",
            Self::Event | Self::Label => "label",
            Self::Function | Self::Method => "function",
            Self::Macro => "function.macro",
            Self::Keyword => "keyword",
            Self::Modifier => "keyword.modifier",
            Self::Comment => "comment",
            Self::String => "string",
            Self::Number => "constant.numeric",
            Self::Operator => "operator",
            Self::Decorator => "attribute",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTokenModifier {
    Declaration,
    Definition,
    Readonly,
    Static,
    Async,
    Mutable,
    Documentation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    pub range: TextRange,
    pub kind: SemanticTokenKind,
    pub modifiers: Vec<SemanticTokenModifier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Symbol,
    Function,
    Method,
    Type,
    Property,
    Keyword,
    String,
    Number,
    Comment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolOccurrence {
    pub range: TextRange,
    pub text: String,
    pub kind: SymbolKind,
    pub node_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverRequest {
    pub document: DocumentContext,
    pub position: TextPosition,
    pub target: SymbolOccurrence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverResult {
    pub range: TextRange,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionRequest {
    pub document: DocumentContext,
    pub target: SymbolOccurrence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionLink {
    pub target_document: PathBuf,
    pub target_range: TextRange,
    pub target_selection_range: Option<TextRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionTriggerKind {
    Invoked,
    TriggerCharacter(char),
    TriggerForIncompleteItems,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionContext {
    pub replace_range: TextRange,
    pub prefix: String,
    pub trigger: CompletionTriggerKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRequest {
    pub document: DocumentContext,
    pub position: TextPosition,
    pub context: CompletionContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub replace_range: TextRange,
}

pub trait LanguageFeatureProvider {
    fn diagnostics(&self, _: &DocumentContext) -> Option<Vec<Diagnostic>> {
        None
    }

    fn semantic_tokens(&self, _: &DocumentContext) -> Option<Vec<SemanticToken>> {
        None
    }

    fn hover(&self, _: &HoverRequest) -> Option<HoverResult> {
        None
    }

    fn definitions(&self, _: &DefinitionRequest) -> Option<Vec<DefinitionLink>> {
        None
    }

    fn completions(&self, _: &CompletionRequest) -> Option<Vec<CompletionItem>> {
        None
    }
}

pub fn semantic_token_captures(
    source: &str,
    tokens: &[SemanticToken],
    visible_byte_range: Range<usize>,
) -> Vec<HighlightCapture> {
    tokens
        .iter()
        .filter_map(|token| {
            let byte_range = byte_range_for_text_range(source, token.range)?;
            let overlap = overlap_range(byte_range, &visible_byte_range)?;
            Some(HighlightCapture {
                name: token.kind.style_key().to_string(),
                style_key: token.kind.style_key().to_string(),
                byte_range: overlap,
            })
        })
        .collect()
}

pub fn merge_highlight_layers(
    syntax: &[HighlightCapture],
    semantic: &[HighlightCapture],
) -> Vec<HighlightCapture> {
    let mut merged = syntax.to_vec();
    merged.extend_from_slice(semantic);
    merged
}

pub(crate) fn byte_range_for_text_range(source: &str, range: TextRange) -> Option<Range<usize>> {
    let start = position_to_byte_in_source(source, range.start)?;
    let end = position_to_byte_in_source(source, range.end)?;
    (start <= end).then_some(start..end)
}

pub(crate) fn text_range_for_byte_range(source: &str, range: Range<usize>) -> Option<TextRange> {
    let start = byte_to_position_in_source(source, range.start)?;
    let end = byte_to_position_in_source(source, range.end)?;
    Some(TextRange::new(start, end))
}

pub(crate) fn position_to_byte_in_source(source: &str, position: TextPosition) -> Option<usize> {
    let mut line_start = 0;
    for _ in 0..position.line {
        let line_len = source.get(line_start..)?.find('\n')?;
        line_start += line_len + 1;
    }

    let line_slice = source.get(line_start..)?;
    let line_end = line_slice
        .find('\n')
        .map(|offset| line_start + offset)
        .unwrap_or(source.len());
    let line_text = source.get(line_start..line_end)?;
    if position.column > line_text.chars().count() {
        return None;
    }

    if position.column == line_text.chars().count() {
        return Some(line_end);
    }

    line_text
        .char_indices()
        .nth(position.column)
        .map(|(offset, _)| line_start + offset)
}

pub(crate) fn byte_to_position_in_source(source: &str, byte: usize) -> Option<TextPosition> {
    let clamped = byte.min(source.len());
    if !source.is_char_boundary(clamped) {
        return None;
    }

    let prefix = source.get(..clamped)?;
    let line = prefix.bytes().filter(|value| *value == b'\n').count();
    let line_start = prefix.rfind('\n').map(|offset| offset + 1).unwrap_or(0);
    let column = source.get(line_start..clamped)?.chars().count();
    Some(TextPosition::new(line, column))
}

fn overlap_range(range: Range<usize>, visible: &Range<usize>) -> Option<Range<usize>> {
    let start = range.start.max(visible.start);
    let end = range.end.min(visible.end);
    (start < end).then_some(start..end)
}
