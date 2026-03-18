use std::cmp::min;
use std::ops::Range;

use hunk_language::{LanguageId, ParseStatus};
use hunk_text::{SearchMatch, Selection, TextBuffer, TextPosition, Transaction};

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
    FoldLines { start_line: usize, end_line: usize },
    UnfoldAtLine { line: usize },
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
    preferred_display_column: Option<usize>,
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
            preferred_display_column: None,
        }
    }

    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn display_snapshot(&self) -> DisplaySnapshot {
        let rows = self.build_display_rows();
        let total_display_rows = rows.len();
        let start = self.viewport.first_visible_row.min(total_display_rows);
        let end = min(
            start.saturating_add(self.viewport.visible_row_count),
            total_display_rows,
        );

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
            visible_rows: rows[start..end].to_vec(),
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
                self.wrap_width = width.filter(|width| *width > 0);
            }
            EditorCommand::SetTabWidth(width) => {
                self.tab_width = width.max(1);
            }
            EditorCommand::SetShowWhitespace(show_whitespace) => {
                self.show_whitespace = show_whitespace;
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
                self.search_query = query.filter(|query| !query.is_empty());
            }
            EditorCommand::SetOverlays(overlays) => {
                self.overlays = overlays;
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
                }
            }
            EditorCommand::UnfoldAtLine { line } => {
                self.folded_regions
                    .retain(|region| !region.contains_line(line));
            }
            EditorCommand::ReplaceAll(text) => {
                self.buffer.set_text(&text);
                self.primary_selection = Selection::caret(TextPosition::default());
                self.folded_regions.clear();
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
            let wrap_width = self
                .wrap_width
                .unwrap_or_else(|| expanded_line.display_len().max(1));
            let display_len = expanded_line.display_len().max(1);
            let mut start_column = 0;
            while start_column < display_len {
                let end_column = min(start_column + wrap_width.max(1), display_len);
                rows.push(VisualRow {
                    row_index,
                    kind: DisplayRowKind::Text,
                    source_line: line,
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
                if expanded_line.display_len() == 0 {
                    break;
                }
            }
            if expanded_line.display_len() == 0 {
                rows.push(VisualRow {
                    row_index,
                    kind: DisplayRowKind::Text,
                    source_line: line,
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

#[derive(Debug, Clone)]
struct ExpandedLine {
    display_text: String,
    raw_to_display: Vec<usize>,
    display_to_raw: Vec<usize>,
    markers: Vec<WhitespaceMarker>,
}

impl ExpandedLine {
    fn from_line(line: String, tab_width: usize, show_whitespace: bool) -> Self {
        let mut display_text = String::new();
        let mut raw_to_display = Vec::new();
        let mut display_to_raw = Vec::new();
        let mut markers = Vec::new();

        for (raw_column, ch) in line.chars().enumerate() {
            raw_to_display.push(display_text.chars().count());
            match ch {
                '\t' => {
                    if show_whitespace {
                        markers.push(WhitespaceMarker {
                            column: display_text.chars().count(),
                            kind: WhitespaceKind::Tab,
                        });
                    }
                    let tab_stop = tab_width - (display_text.chars().count() % tab_width);
                    for _ in 0..tab_stop {
                        display_text.push(' ');
                        display_to_raw.push(raw_column);
                    }
                }
                ' ' => {
                    if show_whitespace {
                        markers.push(WhitespaceMarker {
                            column: display_text.chars().count(),
                            kind: WhitespaceKind::Space,
                        });
                    }
                    display_text.push(' ');
                    display_to_raw.push(raw_column);
                }
                other => {
                    display_text.push(other);
                    display_to_raw.push(raw_column);
                }
            }
        }

        raw_to_display.push(display_text.chars().count());
        display_to_raw.push(line.chars().count());

        Self {
            display_text,
            raw_to_display,
            display_to_raw,
            markers,
        }
    }

    fn display_len(&self) -> usize {
        self.display_text.chars().count()
    }

    fn segment(&self, start: usize, end: usize) -> String {
        self.display_text
            .chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect()
    }

    fn raw_to_display_column(&self, raw_column: usize) -> usize {
        let index = min(raw_column, self.raw_to_display.len().saturating_sub(1));
        self.raw_to_display[index]
    }

    fn display_to_raw_column(&self, display_column: usize) -> usize {
        let index = min(display_column, self.display_to_raw.len().saturating_sub(1));
        self.display_to_raw[index]
    }

    fn markers_in_range(&self, start: usize, end: usize) -> Vec<WhitespaceMarker> {
        self.markers
            .iter()
            .copied()
            .filter(|marker| marker.column >= start && marker.column < end)
            .map(|marker| WhitespaceMarker {
                column: marker.column - start,
                kind: marker.kind,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct VisualRow {
    row_index: usize,
    kind: DisplayRowKind,
    source_line: usize,
    start_column: usize,
    end_column: usize,
    text: String,
    is_wrapped: bool,
    whitespace_markers: Vec<WhitespaceMarker>,
    search_highlights: Vec<SearchHighlight>,
    overlays: Vec<OverlayDescriptor>,
    expanded_line: ExpandedLine,
}

impl VisualRow {
    fn empty() -> Self {
        Self {
            row_index: 0,
            kind: DisplayRowKind::Text,
            source_line: 0,
            start_column: 0,
            end_column: 0,
            text: String::new(),
            is_wrapped: false,
            whitespace_markers: Vec::new(),
            search_highlights: Vec::new(),
            overlays: Vec::new(),
            expanded_line: ExpandedLine::from_line(String::new(), 4, false),
        }
    }
}

fn build_fold_placeholder(prefix: &str, region: FoldRegion) -> String {
    let hidden_line_count = region.end_line - region.start_line;
    if prefix.is_empty() {
        format!("… {} hidden lines", hidden_line_count)
    } else {
        format!("{prefix}  … {} hidden lines", hidden_line_count)
    }
}

fn overlays_for_line(overlays: &[OverlayDescriptor], line: usize) -> Vec<OverlayDescriptor> {
    overlays
        .iter()
        .filter(|overlay| overlay.line == line)
        .cloned()
        .collect()
}

fn search_matches_for_line(
    snapshot: &hunk_text::TextSnapshot,
    matches: &[SearchMatch],
    line: usize,
) -> Vec<Range<usize>> {
    let line_start = snapshot.line_to_byte(line).unwrap_or(0);
    let line_end = if line + 1 < snapshot.line_count() {
        snapshot
            .line_to_byte(line + 1)
            .unwrap_or(snapshot.byte_len())
    } else {
        snapshot.byte_len()
    };

    matches
        .iter()
        .filter_map(|found| {
            let start = found.byte_range.start.max(line_start);
            let end = found.byte_range.end.min(line_end);
            (start < end).then_some(start..end)
        })
        .collect()
}

fn project_search_matches(
    expanded_line: &ExpandedLine,
    matches: &[Range<usize>],
    start_column: usize,
    end_column: usize,
) -> Vec<SearchHighlight> {
    matches
        .iter()
        .filter_map(|range| {
            let start = expanded_line.raw_to_display_column(range.start);
            let end = expanded_line.raw_to_display_column(range.end);
            let projected_start = start.max(start_column);
            let projected_end = end.min(end_column);
            (projected_start < projected_end).then_some(SearchHighlight {
                start_column: projected_start - start_column,
                end_column: projected_end - start_column,
            })
        })
        .collect()
}

fn line_text(snapshot: &hunk_text::TextSnapshot, line: usize) -> String {
    let start = snapshot.line_to_byte(line).unwrap_or(0);
    let end = if line + 1 < snapshot.line_count() {
        snapshot
            .line_to_byte(line + 1)
            .unwrap_or(snapshot.byte_len())
    } else {
        snapshot.byte_len()
    };
    snapshot
        .slice(start..end)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}
