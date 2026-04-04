mod display;
mod workspace;
mod workspace_display;
mod workspace_display_projection;

use std::cell::RefCell;
use std::cmp::min;

use display::{
    ExpandedLine, VisualRow, build_fold_placeholder, line_text, overlays_for_line,
    project_search_matches, search_matches_for_line,
};
use hunk_language::{
    CompletionRequest, DefinitionRequest, Diagnostic, HoverRequest, LanguageId, ParseStatus,
    SemanticToken,
};
use hunk_text::{Selection, TextBuffer, TextPosition, Transaction};
pub use workspace::{
    WorkspaceDocument, WorkspaceDocumentId, WorkspaceExcerptId, WorkspaceExcerptKind,
    WorkspaceExcerptLayout, WorkspaceExcerptSpec, WorkspaceLayout, WorkspaceLayoutError,
    WorkspaceRowKind, WorkspaceRowLocation,
};
pub use workspace_display::{
    WorkspaceDisplayRow, WorkspaceDisplaySnapshot, build_workspace_display_snapshot,
};
pub use workspace_display_projection::{
    WorkspaceProjectedRow, WorkspaceProjectedSnapshot, build_workspace_projected_snapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub first_visible_row: usize,
    pub visible_row_count: usize,
    pub horizontal_offset: usize,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            first_visible_row: 0,
            visible_row_count: 1,
            horizontal_offset: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhitespaceKind {
    Space,
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhitespaceMarker {
    pub column: usize,
    pub kind: WhitespaceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchHighlight {
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    DiagnosticError,
    DiagnosticWarning,
    DiagnosticInfo,
    DiffAddition,
    DiffDeletion,
    DiffModification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayDescriptor {
    pub line: usize,
    pub kind: OverlayKind,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoldRegion {
    pub start_line: usize,
    pub end_line: usize,
}

impl FoldRegion {
    pub fn new(start_line: usize, end_line: usize) -> Option<Self> {
        (end_line > start_line).then_some(Self {
            start_line,
            end_line,
        })
    }

    fn contains_line(self, line: usize) -> bool {
        line >= self.start_line && line <= self.end_line
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayRowKind {
    Text,
    FoldPlaceholder { hidden_line_count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayRow {
    pub row_index: usize,
    pub kind: DisplayRowKind,
    pub source_line: usize,
    pub raw_start_column: usize,
    pub raw_end_column: usize,
    pub raw_column_offsets: Vec<usize>,
    pub start_column: usize,
    pub end_column: usize,
    pub text: String,
    pub is_wrapped: bool,
    pub whitespace_markers: Vec<WhitespaceMarker>,
    pub search_highlights: Vec<SearchHighlight>,
    pub overlays: Vec<OverlayDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplaySnapshot {
    pub viewport: Viewport,
    pub line_count: usize,
    pub total_display_rows: usize,
    pub dirty: bool,
    pub language_id: Option<LanguageId>,
    pub parse_status: ParseStatus,
    pub selection_count: usize,
    pub wrap_width: Option<usize>,
    pub folded_region_count: usize,
    pub visible_rows: Vec<DisplayRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorStatusSnapshot {
    pub line_count: usize,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub selection_count: usize,
    pub dirty: bool,
    pub language_id: Option<LanguageId>,
    pub parse_status: ParseStatus,
    pub folded_region_count: usize,
    pub wrap_width: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorCommand {
    SetViewport(Viewport),
    SetWrapWidth(Option<usize>),
    SetTabWidth(usize),
    SetShowWhitespace(bool),
    SetSelection(Selection),
    SetLanguage(Option<LanguageId>),
    SetParseStatus(ParseStatus),
    SetSearchQuery(Option<String>),
    SetOverlays(Vec<OverlayDescriptor>),
    SetDiagnostics(Vec<Diagnostic>),
    SetSemanticTokens(Vec<SemanticToken>),
    FoldLines { start_line: usize, end_line: usize },
    UnfoldAtLine { line: usize },
    RequestHover(HoverRequest),
    ClearHoverRequest,
    RequestDefinition(DefinitionRequest),
    ClearDefinitionRequest,
    RequestCompletion(CompletionRequest),
    ClearCompletionRequest,
    ReplaceAll(String),
    ReplaceSelection(String),
    InsertText(String),
    DeleteBackward,
    DeleteForward,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    CopySelection,
    CutSelection,
    Paste(String),
    Undo,
    Redo,
    MarkSaved,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOutput {
    pub copied_text: Option<String>,
    pub document_changed: bool,
    pub selection_changed: bool,
    pub viewport_changed: bool,
    pub hover_requested: bool,
    pub definition_requested: bool,
    pub completion_requested: bool,
}

#[derive(Debug, Clone)]
pub struct EditorState {
    buffer: TextBuffer,
    primary_selection: Selection,
    secondary_selections: Vec<Selection>,
    viewport: Viewport,
    dirty: bool,
    saved_text: String,
    language_id: Option<LanguageId>,
    parse_status: ParseStatus,
    wrap_width: Option<usize>,
    tab_width: usize,
    show_whitespace: bool,
    folded_regions: Vec<FoldRegion>,
    search_query: Option<String>,
    overlays: Vec<OverlayDescriptor>,
    diagnostics: Vec<Diagnostic>,
    semantic_tokens: Vec<SemanticToken>,
    pending_hover_request: Option<HoverRequest>,
    pending_definition_request: Option<DefinitionRequest>,
    pending_completion_request: Option<CompletionRequest>,
    preferred_display_column: Option<usize>,
    display_generation: u64,
    display_cache: RefCell<Option<DisplayCacheEntry>>,
}

#[derive(Debug, Clone)]
struct DisplayCacheEntry {
    generation: u64,
    rows: Vec<DisplayRow>,
}

impl EditorState {
    pub fn new(buffer: TextBuffer) -> Self {
        let saved_text = buffer.text();
        Self {
            buffer,
            primary_selection: Selection::caret(TextPosition::default()),
            secondary_selections: Vec::new(),
            viewport: Viewport::default(),
            dirty: false,
            saved_text,
            language_id: None,
            parse_status: ParseStatus::Idle,
            wrap_width: None,
            tab_width: 4,
            show_whitespace: false,
            folded_regions: Vec::new(),
            search_query: None,
            overlays: Vec::new(),
            diagnostics: Vec::new(),
            semantic_tokens: Vec::new(),
            pending_hover_request: None,
            pending_definition_request: None,
            pending_completion_request: None,
            preferred_display_column: None,
            display_generation: 0,
            display_cache: RefCell::new(None),
        }
    }

    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    pub fn into_buffer(self) -> TextBuffer {
        self.buffer
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn selection(&self) -> Selection {
        self.primary_selection
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn wrap_width(&self) -> Option<usize> {
        self.wrap_width
    }

    pub fn show_whitespace(&self) -> bool {
        self.show_whitespace
    }

    pub fn folded_regions(&self) -> &[FoldRegion] {
        &self.folded_regions
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn semantic_tokens(&self) -> &[SemanticToken] {
        &self.semantic_tokens
    }

    pub fn pending_hover_request(&self) -> Option<&HoverRequest> {
        self.pending_hover_request.as_ref()
    }

    pub fn pending_definition_request(&self) -> Option<&DefinitionRequest> {
        self.pending_definition_request.as_ref()
    }

    pub fn pending_completion_request(&self) -> Option<&CompletionRequest> {
        self.pending_completion_request.as_ref()
    }

    pub fn display_snapshot(&self) -> DisplaySnapshot {
        let generation = self.display_generation;
        let (total_display_rows, visible_rows) = {
            let mut cache = self.display_cache.borrow_mut();
            let rebuild_needed = cache
                .as_ref()
                .is_none_or(|entry| entry.generation != generation);
            if rebuild_needed {
                *cache = Some(DisplayCacheEntry {
                    generation,
                    rows: self.build_display_rows(),
                });
            }

            let rows = &cache.as_ref().expect("display cache populated").rows;
            let total_display_rows = rows.len();
            let start = self.viewport.first_visible_row.min(total_display_rows);
            let end = min(
                start.saturating_add(self.viewport.visible_row_count),
                total_display_rows,
            );
            (total_display_rows, rows[start..end].to_vec())
        };

        DisplaySnapshot {
            viewport: self.viewport,
            line_count: self.buffer.line_count(),
            total_display_rows,
            dirty: self.dirty,
            language_id: self.language_id,
            parse_status: self.parse_status,
            selection_count: 1 + self.secondary_selections.len(),
            wrap_width: self.wrap_width,
            folded_region_count: self.folded_regions.len(),
            visible_rows,
        }
    }

    pub fn status_snapshot(&self) -> EditorStatusSnapshot {
        EditorStatusSnapshot {
            line_count: self.buffer.line_count(),
            cursor_line: self.primary_selection.head.line + 1,
            cursor_column: self.primary_selection.head.column + 1,
            selection_count: 1 + self.secondary_selections.len(),
            dirty: self.dirty,
            language_id: self.language_id,
            parse_status: self.parse_status,
            folded_region_count: self.folded_regions.len(),
            wrap_width: self.wrap_width,
        }
    }

    pub fn apply(&mut self, command: EditorCommand) -> CommandOutput {
        let mut output = CommandOutput::default();

        match command {
            EditorCommand::SetViewport(viewport) => {
                self.viewport = viewport;
                output.viewport_changed = true;
            }
            EditorCommand::SetWrapWidth(width) => {
                let next = width.filter(|width| *width > 0);
                if self.wrap_width != next {
                    self.wrap_width = next;
                    self.invalidate_display_cache();
                }
            }
            EditorCommand::SetTabWidth(width) => {
                let next = width.max(1);
                if self.tab_width != next {
                    self.tab_width = next;
                    self.invalidate_display_cache();
                }
            }
            EditorCommand::SetShowWhitespace(show_whitespace) => {
                if self.show_whitespace != show_whitespace {
                    self.show_whitespace = show_whitespace;
                    self.invalidate_display_cache();
                }
            }
            EditorCommand::SetSelection(selection) => {
                self.primary_selection = self.clamp_selection(selection);
                self.preferred_display_column = None;
                output.selection_changed = true;
            }
            EditorCommand::SetLanguage(language_id) => {
                self.language_id = language_id;
            }
            EditorCommand::SetParseStatus(parse_status) => {
                self.parse_status = parse_status;
            }
            EditorCommand::SetSearchQuery(query) => {
                let next = query.filter(|query| !query.is_empty());
                if self.search_query != next {
                    self.search_query = next;
                    self.invalidate_display_cache();
                }
            }
            EditorCommand::SetOverlays(overlays) => {
                if self.overlays != overlays {
                    self.overlays = overlays;
                    self.invalidate_display_cache();
                }
            }
            EditorCommand::SetDiagnostics(diagnostics) => {
                self.diagnostics = diagnostics;
            }
            EditorCommand::SetSemanticTokens(tokens) => {
                self.semantic_tokens = tokens;
            }
            EditorCommand::FoldLines {
                start_line,
                end_line,
            } => {
                if let Some(region) = FoldRegion::new(start_line, end_line)
                    && !self.folded_regions.contains(&region)
                {
                    self.folded_regions.push(region);
                    self.folded_regions.sort_by_key(|region| region.start_line);
                    self.invalidate_display_cache();
                }
            }
            EditorCommand::UnfoldAtLine { line } => {
                let previous_len = self.folded_regions.len();
                self.folded_regions
                    .retain(|region| !region.contains_line(line));
                if self.folded_regions.len() != previous_len {
                    self.invalidate_display_cache();
                }
            }
            EditorCommand::RequestHover(request) => {
                self.pending_hover_request = Some(request);
                output.hover_requested = true;
            }
            EditorCommand::ClearHoverRequest => {
                self.pending_hover_request = None;
            }
            EditorCommand::RequestDefinition(request) => {
                self.pending_definition_request = Some(request);
                output.definition_requested = true;
            }
            EditorCommand::ClearDefinitionRequest => {
                self.pending_definition_request = None;
            }
            EditorCommand::RequestCompletion(request) => {
                self.pending_completion_request = Some(request);
                output.completion_requested = true;
            }
            EditorCommand::ClearCompletionRequest => {
                self.pending_completion_request = None;
            }
            EditorCommand::ReplaceAll(text) => {
                self.buffer.set_text(&text);
                self.primary_selection = Selection::caret(TextPosition::default());
                self.folded_regions.clear();
                self.invalidate_display_cache();
                output.document_changed = true;
                output.selection_changed = true;
            }
            EditorCommand::ReplaceSelection(text) | EditorCommand::InsertText(text) => {
                let changed = self.replace_selection_text(&text);
                output.document_changed = changed;
                output.selection_changed = changed;
            }
            EditorCommand::DeleteBackward => {
                let changed = self.delete_backward();
                output.document_changed = changed;
                output.selection_changed = changed;
            }
            EditorCommand::DeleteForward => {
                let changed = self.delete_forward();
                output.document_changed = changed;
                output.selection_changed = changed;
            }
            EditorCommand::MoveLeft => {
                output.selection_changed = self.move_horizontal(-1);
            }
            EditorCommand::MoveRight => {
                output.selection_changed = self.move_horizontal(1);
            }
            EditorCommand::MoveUp => {
                output.selection_changed = self.move_vertical(-1);
            }
            EditorCommand::MoveDown => {
                output.selection_changed = self.move_vertical(1);
            }
            EditorCommand::CopySelection => {
                output.copied_text = self.selected_text();
            }
            EditorCommand::CutSelection => {
                output.copied_text = self.selected_text();
                if output.copied_text.is_some() {
                    let changed = self.replace_selection_text("");
                    output.document_changed = changed;
                    output.selection_changed = changed;
                }
            }
            EditorCommand::Paste(text) => {
                let changed = self.replace_selection_text(&text);
                output.document_changed = changed;
                output.selection_changed = changed;
            }
            EditorCommand::Undo => {
                if self.buffer.undo().unwrap_or(false) {
                    self.primary_selection = Selection::caret(TextPosition::default());
                    output.document_changed = true;
                    output.selection_changed = true;
                }
            }
            EditorCommand::Redo => {
                if self.buffer.redo().unwrap_or(false) {
                    self.primary_selection = Selection::caret(TextPosition::default());
                    output.document_changed = true;
                    output.selection_changed = true;
                }
            }
            EditorCommand::MarkSaved => {
                self.saved_text = self.buffer.text();
            }
        }

        if output.selection_changed {
            self.viewport = self.viewport_for_selection();
            output.viewport_changed = true;
        }
        if output.document_changed {
            self.clamp_after_document_change();
            self.clear_language_intelligence();
            self.invalidate_display_cache();
        }
        self.update_dirty();
        output
    }

    fn update_dirty(&mut self) {
        self.dirty = self.buffer.text() != self.saved_text;
    }

    fn clamp_after_document_change(&mut self) {
        self.primary_selection = self.clamp_selection(self.primary_selection);
        self.folded_regions
            .retain(|region| region.start_line < self.buffer.line_count());
        for region in &mut self.folded_regions {
            region.end_line = region
                .end_line
                .min(self.buffer.line_count().saturating_sub(1));
        }
    }

    fn clear_language_intelligence(&mut self) {
        self.diagnostics.clear();
        self.semantic_tokens.clear();
        self.pending_hover_request = None;
        self.pending_definition_request = None;
        self.pending_completion_request = None;
    }

    fn invalidate_display_cache(&mut self) {
        self.display_generation = self.display_generation.saturating_add(1);
        self.display_cache.borrow_mut().take();
    }

    fn clamp_selection(&self, selection: Selection) -> Selection {
        Selection::new(
            self.clamp_position(selection.anchor),
            self.clamp_position(selection.head),
        )
    }

    fn clamp_position(&self, position: TextPosition) -> TextPosition {
        let snapshot = self.buffer.snapshot();
        let max_line = snapshot.line_count().saturating_sub(1);
        let line = min(position.line, max_line);
        let line_text = line_text(&snapshot, line);
        let max_column = line_text.chars().count();
        TextPosition::new(line, min(position.column, max_column))
    }

    fn selected_text(&self) -> Option<String> {
        let range = self.primary_selection.range();
        if range.is_empty() {
            return None;
        }
        let snapshot = self.buffer.snapshot();
        let start = snapshot.position_to_byte(range.start).ok()?;
        let end = snapshot.position_to_byte(range.end).ok()?;
        snapshot.slice(start..end).ok()
    }

    fn replace_selection_text(&mut self, text: &str) -> bool {
        let snapshot = self.buffer.snapshot();
        let range = self.primary_selection.range();
        let start = match snapshot.position_to_byte(range.start) {
            Ok(byte) => byte,
            Err(_) => return false,
        };
        let end = match snapshot.position_to_byte(range.end) {
            Ok(byte) => byte,
            Err(_) => return false,
        };

        let transaction = Transaction::new().replace(start..end, text);
        if self.buffer.apply_transaction(transaction).is_err() {
            return false;
        }

        let new_snapshot = self.buffer.snapshot();
        let caret_byte = start + text.len();
        let caret = new_snapshot
            .byte_to_position(caret_byte)
            .unwrap_or_else(|_| TextPosition::default());
        self.primary_selection = Selection::caret(caret);
        self.preferred_display_column = None;
        true
    }

    fn delete_backward(&mut self) -> bool {
        if !self.primary_selection.is_caret() {
            return self.replace_selection_text("");
        }

        let snapshot = self.buffer.snapshot();
        let caret = self.primary_selection.head;
        let byte = match snapshot.position_to_byte(caret) {
            Ok(byte) => byte,
            Err(_) => return false,
        };
        if byte == 0 {
            return false;
        }

        let previous_char = snapshot.text()[..byte].chars().next_back();
        let Some(previous_char) = previous_char else {
            return false;
        };
        let previous_start = byte - previous_char.len_utf8();
        if self
            .buffer
            .apply_transaction(Transaction::new().replace(previous_start..byte, ""))
            .is_err()
        {
            return false;
        }

        let new_snapshot = self.buffer.snapshot();
        let caret = new_snapshot
            .byte_to_position(previous_start)
            .unwrap_or_else(|_| TextPosition::default());
        self.primary_selection = Selection::caret(caret);
        self.preferred_display_column = None;
        true
    }

    fn delete_forward(&mut self) -> bool {
        if !self.primary_selection.is_caret() {
            return self.replace_selection_text("");
        }

        let snapshot = self.buffer.snapshot();
        let caret = self.primary_selection.head;
        let start = match snapshot.position_to_byte(caret) {
            Ok(byte) => byte,
            Err(_) => return false,
        };
        if start == snapshot.byte_len() {
            return false;
        }

        let text = snapshot.text();
        let mut chars = text[start..].chars();
        let Some(next_char) = chars.next() else {
            return false;
        };
        let end = start + next_char.len_utf8();
        if self
            .buffer
            .apply_transaction(Transaction::new().replace(start..end, ""))
            .is_err()
        {
            return false;
        }

        self.primary_selection = Selection::caret(caret);
        self.preferred_display_column = None;
        true
    }

    fn move_horizontal(&mut self, direction: isize) -> bool {
        let snapshot = self.buffer.snapshot();
        let head = self.primary_selection.head;
        let byte = match snapshot.position_to_byte(head) {
            Ok(byte) => byte,
            Err(_) => return false,
        };
        let text = snapshot.text();

        let next_byte = if direction < 0 {
            if byte == 0 {
                return false;
            }
            let previous = text[..byte].chars().next_back();
            let Some(previous) = previous else {
                return false;
            };
            byte - previous.len_utf8()
        } else {
            if byte == text.len() {
                return false;
            }
            let next = text[byte..].chars().next();
            let Some(next) = next else {
                return false;
            };
            byte + next.len_utf8()
        };

        let next_position = match snapshot.byte_to_position(next_byte) {
            Ok(position) => position,
            Err(_) => return false,
        };
        self.primary_selection = Selection::caret(next_position);
        self.preferred_display_column = None;
        true
    }

    fn move_vertical(&mut self, direction: isize) -> bool {
        let rows = self.build_visual_rows();
        let Some(current_row_index) =
            self.row_index_for_position(&rows, self.primary_selection.head)
        else {
            return false;
        };

        let next_row_index = if direction < 0 {
            current_row_index.checked_sub(1)
        } else if current_row_index + 1 < rows.len() {
            Some(current_row_index + 1)
        } else {
            None
        };
        let Some(next_row_index) = next_row_index else {
            return false;
        };

        let current_row = &rows[current_row_index];
        let current_display_column =
            self.display_column_for_position(current_row, self.primary_selection.head);
        let target_display_column = self
            .preferred_display_column
            .unwrap_or(current_display_column);
        let next_row = &rows[next_row_index];
        let target_column = min(
            next_row.end_column.saturating_sub(next_row.start_column),
            target_display_column,
        );
        let raw_column = next_row
            .expanded_line
            .display_to_raw_column(next_row.start_column + target_column);
        self.primary_selection =
            Selection::caret(TextPosition::new(next_row.source_line, raw_column));
        self.preferred_display_column = Some(target_display_column);
        true
    }

    fn display_column_for_position(&self, row: &VisualRow, position: TextPosition) -> usize {
        let display_column = row.expanded_line.raw_to_display_column(position.column);
        display_column.saturating_sub(row.start_column)
    }

    fn row_index_for_position(&self, rows: &[VisualRow], position: TextPosition) -> Option<usize> {
        rows.iter().position(|row| {
            row.source_line == position.line
                && row.start_column <= row.expanded_line.raw_to_display_column(position.column)
                && row.expanded_line.raw_to_display_column(position.column) <= row.end_column
        })
    }

    fn viewport_for_selection(&self) -> Viewport {
        let rows = self.build_visual_rows();
        let Some(row_index) = self.row_index_for_position(&rows, self.primary_selection.head)
        else {
            return self.viewport;
        };

        let mut viewport = self.viewport;
        if row_index < viewport.first_visible_row {
            viewport.first_visible_row = row_index;
        } else {
            let last_visible = viewport
                .first_visible_row
                .saturating_add(viewport.visible_row_count.saturating_sub(1));
            if row_index > last_visible {
                viewport.first_visible_row =
                    row_index.saturating_sub(viewport.visible_row_count.saturating_sub(1));
            }
        }
        viewport
    }

    fn build_display_rows(&self) -> Vec<DisplayRow> {
        self.build_visual_rows()
            .into_iter()
            .map(|row| DisplayRow {
                row_index: row.row_index,
                kind: row.kind,
                source_line: row.source_line,
                raw_start_column: row.raw_start_column,
                raw_end_column: row.raw_end_column,
                raw_column_offsets: row.raw_column_offsets,
                start_column: row.start_column,
                end_column: row.end_column,
                text: row.text,
                is_wrapped: row.is_wrapped,
                whitespace_markers: row.whitespace_markers,
                search_highlights: row.search_highlights,
                overlays: row.overlays,
            })
            .collect()
    }

    fn build_visual_rows(&self) -> Vec<VisualRow> {
        let snapshot = self.buffer.snapshot();
        let search_matches = self
            .search_query
            .as_ref()
            .map(|query| snapshot.find_all(query))
            .unwrap_or_default();

        let mut row_index = 0;
        let mut rows = Vec::new();
        let mut line = 0;
        while line < snapshot.line_count() {
            if let Some(region) = self.fold_region_starting_at(line) {
                let expanded_line = ExpandedLine::from_line(
                    line_text(&snapshot, line),
                    self.tab_width,
                    self.show_whitespace,
                );
                let placeholder = build_fold_placeholder(&expanded_line.display_text, region);
                rows.push(VisualRow {
                    row_index,
                    kind: DisplayRowKind::FoldPlaceholder {
                        hidden_line_count: region.end_line - region.start_line,
                    },
                    source_line: line,
                    raw_start_column: 0,
                    raw_end_column: expanded_line.raw_len(),
                    raw_column_offsets: (0..=expanded_line.raw_len()).collect(),
                    start_column: 0,
                    end_column: placeholder.chars().count(),
                    text: placeholder,
                    is_wrapped: false,
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                    overlays: overlays_for_line(&self.overlays, line),
                    expanded_line,
                });
                row_index += 1;
                line = region.end_line + 1;
                continue;
            }

            let expanded_line = ExpandedLine::from_line(
                line_text(&snapshot, line),
                self.tab_width,
                self.show_whitespace,
            );
            let line_search_matches = search_matches_for_line(&snapshot, &search_matches, line);
            let display_len = expanded_line.display_len();
            let wrap_width = self.wrap_width.unwrap_or_else(|| display_len.max(1));
            if display_len == 0 {
                rows.push(VisualRow {
                    row_index,
                    kind: DisplayRowKind::Text,
                    source_line: line,
                    raw_start_column: 0,
                    raw_end_column: 0,
                    raw_column_offsets: vec![0],
                    start_column: 0,
                    end_column: 0,
                    text: String::new(),
                    is_wrapped: false,
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                    overlays: overlays_for_line(&self.overlays, line),
                    expanded_line,
                });
                row_index += 1;
                line += 1;
                continue;
            }
            let mut start_column = 0;
            while start_column < display_len {
                let end_column = min(start_column + wrap_width.max(1), display_len);
                rows.push(VisualRow {
                    row_index,
                    kind: DisplayRowKind::Text,
                    source_line: line,
                    raw_start_column: expanded_line.display_to_raw_column(start_column),
                    raw_end_column: expanded_line.display_to_raw_column(end_column),
                    raw_column_offsets: expanded_line
                        .raw_offsets_in_range(start_column, end_column),
                    start_column,
                    end_column,
                    text: expanded_line.segment(start_column, end_column),
                    is_wrapped: start_column > 0 || end_column < expanded_line.display_len(),
                    whitespace_markers: expanded_line.markers_in_range(start_column, end_column),
                    search_highlights: project_search_matches(
                        &expanded_line,
                        &line_search_matches,
                        start_column,
                        end_column,
                    ),
                    overlays: overlays_for_line(&self.overlays, line),
                    expanded_line: expanded_line.clone(),
                });
                row_index += 1;
                start_column = end_column;
            }
            line += 1;
        }

        if rows.is_empty() {
            rows.push(VisualRow::empty());
        }
        rows
    }

    fn fold_region_starting_at(&self, line: usize) -> Option<FoldRegion> {
        self.folded_regions
            .iter()
            .copied()
            .find(|region| region.start_line == line)
    }
}
