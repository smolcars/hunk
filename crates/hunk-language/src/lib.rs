mod assets;
mod features;

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use hunk_text::{TextPosition, TextRange};
use tree_sitter::{Language, Parser, Point, Tree};
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

pub use assets::CANONICAL_HIGHLIGHT_NAMES;
pub use features::{
    CompletionContext, CompletionItem, CompletionRequest, CompletionTriggerKind, DefinitionLink,
    DefinitionRequest, Diagnostic, DiagnosticSeverity, DocumentContext, HoverRequest, HoverResult,
    LanguageFeatureProvider, SemanticToken, SemanticTokenKind, SemanticTokenModifier, SymbolKind,
    SymbolOccurrence, merge_highlight_layers, semantic_token_captures,
};
use features::{position_to_byte_in_source, text_range_for_byte_range};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LanguageId(u16);

impl LanguageId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatcher {
    pub extensions: Vec<String>,
    pub file_names: Vec<String>,
}

impl FileMatcher {
    pub fn matches_path(&self, path: &Path) -> bool {
        if let Some(file_name) = path.file_name().and_then(|value| value.to_str())
            && self
                .file_names
                .iter()
                .any(|candidate| candidate == file_name)
        {
            return true;
        }

        path.extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| {
                self.extensions
                    .iter()
                    .any(|candidate| candidate == extension)
            })
    }
}

pub struct LanguageDefinition {
    pub id: LanguageId,
    pub name: String,
    pub scope_name: String,
    pub file_matcher: FileMatcher,
    pub grammar_name: String,
    pub highlight_query: String,
    pub injection_query: String,
    pub locals_query: String,
    pub fold_node_kinds: Vec<String>,
    pub injection_names: Vec<String>,
    language_loader: fn() -> Language,
    highlight_config: Arc<HighlightConfiguration>,
}

impl LanguageDefinition {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: LanguageId,
        name: &str,
        scope_name: &str,
        file_matcher: FileMatcher,
        language_loader: fn() -> Language,
        highlight_query: impl Into<String>,
        injection_query: impl Into<String>,
        locals_query: impl Into<String>,
        fold_node_kinds: &[&str],
        injection_names: &[&str],
    ) -> Self {
        let highlight_query = highlight_query.into();
        let injection_query = injection_query.into();
        let locals_query = locals_query.into();
        let mut highlight_config = HighlightConfiguration::new(
            language_loader(),
            scope_name,
            &highlight_query,
            &injection_query,
            &locals_query,
        )
        .unwrap_or_else(|error| {
            panic!("failed to build highlight configuration for {scope_name}: {error}")
        });
        highlight_config.configure(CANONICAL_HIGHLIGHT_NAMES);

        Self {
            id,
            name: name.to_string(),
            scope_name: scope_name.to_string(),
            file_matcher,
            grammar_name: scope_name.to_string(),
            highlight_query,
            injection_query,
            locals_query,
            fold_node_kinds: fold_node_kinds
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            injection_names: injection_names
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            language_loader,
            highlight_config: Arc::new(highlight_config),
        }
    }

    pub fn language(&self) -> Language {
        (self.language_loader)()
    }

    pub fn highlight_config(&self) -> Arc<HighlightConfiguration> {
        Arc::clone(&self.highlight_config)
    }

    fn is_fold_node_kind(&self, kind: &str) -> bool {
        self.fold_node_kinds
            .iter()
            .any(|candidate| candidate == kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseStatus {
    Idle,
    Parsing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightCapture {
    pub name: String,
    pub byte_range: Range<usize>,
    pub style_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldKind {
    Block,
    Comment,
    Region,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldCandidate {
    pub start_line: usize,
    pub end_line: usize,
    pub kind: FoldKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxSnapshot {
    pub language_id: Option<LanguageId>,
    pub parse_status: ParseStatus,
    pub tree_revision: u64,
    pub highlight_revision: u64,
    pub root_kind: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HighlightStyleMap {
    styles: Vec<String>,
}

impl Default for HighlightStyleMap {
    fn default() -> Self {
        Self {
            styles: CANONICAL_HIGHLIGHT_NAMES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        }
    }
}

impl HighlightStyleMap {
    pub fn resolve(&self, capture_name: &str) -> Option<&str> {
        self.styles
            .iter()
            .filter(|style| {
                capture_name == style.as_str()
                    || capture_name
                        .strip_prefix(style.as_str())
                        .is_some_and(|suffix| suffix.starts_with('.'))
            })
            .max_by_key(|style| style.len())
            .map(String::as_str)
    }
}

#[derive(Default, Clone)]
pub struct LanguageRegistry {
    definitions: BTreeMap<LanguageId, Arc<LanguageDefinition>>,
    ids_by_lower_name: BTreeMap<String, LanguageId>,
    ids_by_injection_name: BTreeMap<String, LanguageId>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builtin() -> Self {
        let mut registry = Self::new();
        for definition in assets::builtin_language_definitions() {
            registry.register(definition);
        }
        registry
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    pub fn register(&mut self, definition: LanguageDefinition) -> Option<Arc<LanguageDefinition>> {
        let definition = Arc::new(definition);
        self.ids_by_lower_name
            .insert(definition.name.to_ascii_lowercase(), definition.id);
        self.ids_by_injection_name
            .insert(definition.scope_name.to_ascii_lowercase(), definition.id);
        for injection_name in &definition.injection_names {
            self.ids_by_injection_name
                .insert(injection_name.to_ascii_lowercase(), definition.id);
        }
        self.definitions.insert(definition.id, definition)
    }

    pub fn language_by_name(&self, name: &str) -> Option<&Arc<LanguageDefinition>> {
        let language_id = self.ids_by_lower_name.get(&name.to_ascii_lowercase())?;
        self.definitions.get(language_id)
    }

    pub fn language_for_path(&self, path: &Path) -> Option<&Arc<LanguageDefinition>> {
        self.definitions
            .values()
            .find(|definition| definition.file_matcher.matches_path(path))
    }

    pub fn language_for_injection_name(&self, name: &str) -> Option<&Arc<LanguageDefinition>> {
        let language_id = self.ids_by_injection_name.get(&name.to_ascii_lowercase())?;
        self.definitions.get(language_id)
    }
}

pub struct SyntaxSession {
    parser: Parser,
    highlighter: Highlighter,
    language_id: Option<LanguageId>,
    tree: Option<Tree>,
    tree_revision: u64,
    highlight_revision: u64,
    parse_status: ParseStatus,
}

impl Default for SyntaxSession {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxSession {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            highlighter: Highlighter::new(),
            language_id: None,
            tree: None,
            tree_revision: 0,
            highlight_revision: 0,
            parse_status: ParseStatus::Idle,
        }
    }

    pub fn snapshot(&self) -> SyntaxSnapshot {
        SyntaxSnapshot {
            language_id: self.language_id,
            parse_status: self.parse_status,
            tree_revision: self.tree_revision,
            highlight_revision: self.highlight_revision,
            root_kind: self
                .tree
                .as_ref()
                .map(|tree| tree.root_node().kind().to_string()),
        }
    }

    pub fn parse_for_path(
        &mut self,
        registry: &LanguageRegistry,
        path: &Path,
        source: &str,
    ) -> Result<SyntaxSnapshot, tree_sitter::LanguageError> {
        let language = registry.language_for_path(path).cloned();
        self.parse_with_language(registry, language.as_deref(), source)
    }

    pub fn parse_with_language(
        &mut self,
        _registry: &LanguageRegistry,
        language: Option<&LanguageDefinition>,
        source: &str,
    ) -> Result<SyntaxSnapshot, tree_sitter::LanguageError> {
        let Some(language) = language else {
            self.language_id = None;
            self.tree = None;
            self.parse_status = ParseStatus::Idle;
            return Ok(self.snapshot());
        };

        self.parse_status = ParseStatus::Parsing;
        self.parser.set_language(&language.language())?;
        let next_tree = self.parser.parse(source, self.tree.as_ref());
        self.tree = next_tree;
        self.language_id = Some(language.id);
        self.tree_revision = self.tree_revision.saturating_add(1);
        self.highlight_revision = self.highlight_revision.saturating_add(1);
        self.parse_status = if self.tree.is_some() {
            ParseStatus::Ready
        } else {
            ParseStatus::Failed
        };
        Ok(self.snapshot())
    }

    pub fn highlight_visible_range(
        &mut self,
        registry: &LanguageRegistry,
        source: &str,
        visible_byte_range: Range<usize>,
    ) -> Result<Vec<HighlightCapture>, tree_sitter_highlight::Error> {
        let Some(language_id) = self.language_id else {
            return Ok(Vec::new());
        };
        let Some(language) = registry.definitions.get(&language_id) else {
            return Ok(Vec::new());
        };
        let primary_config = language.highlight_config();
        let injection_configs = registry
            .definitions
            .values()
            .map(|definition| {
                (
                    definition
                        .injection_names
                        .iter()
                        .map(|name| name.to_ascii_lowercase())
                        .collect::<Vec<_>>(),
                    definition.highlight_config(),
                )
            })
            .collect::<Vec<_>>();

        let mut active_styles = Vec::<String>::new();
        let mut captures = Vec::new();
        let highlights = self.highlighter.highlight(
            primary_config.as_ref(),
            source.as_bytes(),
            None,
            |injection_name| {
                let injection_name = injection_name.to_ascii_lowercase();
                injection_configs.iter().find_map(|(names, config)| {
                    names
                        .iter()
                        .any(|candidate| candidate == &injection_name)
                        .then_some(config.as_ref())
                })
            },
        )?;

        for event in highlights {
            match event? {
                HighlightEvent::Source { start, end } => {
                    let range = overlap_range(start..end, &visible_byte_range);
                    if let Some(range) = range
                        && let Some(style_key) = active_styles.last()
                    {
                        captures.push(HighlightCapture {
                            name: style_key.clone(),
                            byte_range: range,
                            style_key: style_key.clone(),
                        });
                    }
                }
                HighlightEvent::HighlightStart(Highlight(index)) => {
                    if let Some(style_name) = CANONICAL_HIGHLIGHT_NAMES.get(index) {
                        active_styles.push((*style_name).to_string());
                    }
                }
                HighlightEvent::HighlightEnd => {
                    let _ = active_styles.pop();
                }
            }
        }

        Ok(captures)
    }

    pub fn fold_candidates(&self, registry: &LanguageRegistry, source: &str) -> Vec<FoldCandidate> {
        let Some(language_id) = self.language_id else {
            return Vec::new();
        };
        let Some(language) = registry.definitions.get(&language_id) else {
            return Vec::new();
        };
        let Some(tree) = self.tree.as_ref() else {
            return Vec::new();
        };

        let mut cursor = tree.walk();
        let mut candidates = Vec::new();
        collect_fold_candidates(&mut cursor, language, source, &mut candidates);
        candidates
    }

    pub fn hover_target_at(
        &self,
        source: &str,
        position: TextPosition,
    ) -> Option<SymbolOccurrence> {
        self.symbol_occurrence_at(source, position)
    }

    pub fn definition_target_at(
        &self,
        source: &str,
        position: TextPosition,
    ) -> Option<SymbolOccurrence> {
        self.symbol_occurrence_at(source, position)
    }

    pub fn completion_context_at(
        &self,
        source: &str,
        position: TextPosition,
        trigger: CompletionTriggerKind,
    ) -> Option<CompletionContext> {
        let line = source.lines().nth(position.line)?;
        if position.column > line.chars().count() {
            return None;
        }

        let chars = line.chars().collect::<Vec<_>>();
        let mut start = position.column;
        while start > 0 && is_completion_token_char(chars[start - 1]) {
            start -= 1;
        }

        let mut end = position.column;
        while end < chars.len() && is_completion_token_char(chars[end]) {
            end += 1;
        }

        let prefix = chars[start..position.column].iter().collect::<String>();
        Some(CompletionContext {
            replace_range: TextRange::new(
                TextPosition::new(position.line, start),
                TextPosition::new(position.line, end),
            ),
            prefix,
            trigger,
        })
    }

    fn symbol_occurrence_at(
        &self,
        source: &str,
        position: TextPosition,
    ) -> Option<SymbolOccurrence> {
        let tree = self.tree.as_ref()?;
        let byte = position_to_byte_in_source(source, position)?;
        let node = tree
            .root_node()
            .named_descendant_for_byte_range(byte, byte)
            .or_else(|| tree.root_node().descendant_for_byte_range(byte, byte))?;
        let byte_range = node.byte_range();
        let range = text_range_for_byte_range(source, byte_range.clone())?;
        let text = source.get(byte_range)?.to_string();
        let parent_kind = node.parent().map(|parent| parent.kind().to_string());
        Some(SymbolOccurrence {
            range,
            text,
            kind: classify_symbol_kind(node.kind(), parent_kind.as_deref()),
            node_kind: node.kind().to_string(),
        })
    }
}

fn collect_fold_candidates(
    cursor: &mut tree_sitter::TreeCursor<'_>,
    language: &LanguageDefinition,
    source: &str,
    candidates: &mut Vec<FoldCandidate>,
) {
    loop {
        let node = cursor.node();
        if node.is_named() && language.is_fold_node_kind(node.kind()) {
            let start_line = byte_to_point(source, node.start_byte()).row;
            let end_line = byte_to_point(source, node.end_byte()).row;
            if end_line > start_line {
                candidates.push(FoldCandidate {
                    start_line,
                    end_line,
                    kind: FoldKind::Block,
                });
            }
        }

        if cursor.goto_first_child() {
            collect_fold_candidates(cursor, language, source, candidates);
            let _ = cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn overlap_range(range: Range<usize>, visible: &Range<usize>) -> Option<Range<usize>> {
    let start = range.start.max(visible.start);
    let end = range.end.min(visible.end);
    (start < end).then_some(start..end)
}

fn byte_to_point(source: &str, byte: usize) -> Point {
    let prefix = &source.as_bytes()[..byte.min(source.len())];
    let row = prefix.iter().filter(|value| **value == b'\n').count();
    let column = prefix
        .iter()
        .rev()
        .take_while(|value| **value != b'\n')
        .count();
    Point::new(row, column)
}

fn classify_symbol_kind(node_kind: &str, parent_kind: Option<&str>) -> SymbolKind {
    if node_kind.contains("comment") {
        return SymbolKind::Comment;
    }
    if node_kind.contains("string") {
        return SymbolKind::String;
    }
    if node_kind.contains("float")
        || node_kind.contains("integer")
        || node_kind.contains("number")
        || node_kind.contains("numeric")
    {
        return SymbolKind::Number;
    }
    if node_kind.contains("keyword") {
        return SymbolKind::Keyword;
    }

    if node_kind.contains("field") || node_kind.contains("property") {
        return SymbolKind::Property;
    }

    if node_kind.contains("type")
        || parent_kind.is_some_and(|kind| {
            kind.contains("struct")
                || kind.contains("enum")
                || kind.contains("class")
                || kind.contains("interface")
                || kind.contains("trait")
                || kind.contains("type")
        })
    {
        return SymbolKind::Type;
    }

    if parent_kind.is_some_and(|kind| kind.contains("method")) {
        return SymbolKind::Method;
    }

    if parent_kind.is_some_and(|kind| {
        kind.contains("function")
            || kind.contains("call")
            || kind.contains("closure")
            || kind.contains("macro")
    }) {
        return SymbolKind::Function;
    }

    SymbolKind::Symbol
}

fn is_completion_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
