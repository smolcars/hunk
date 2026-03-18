use std::cell::RefCell;
use std::cmp::min;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Result;
use gpui::*;
use hunk_editor::{
    EditorCommand, EditorState, FoldRegion, OverlayDescriptor, OverlayKind, Viewport,
};
use hunk_language::{
    FoldCandidate, HighlightCapture, LanguageRegistry, SyntaxSession, merge_highlight_layers,
    semantic_token_captures,
};
use hunk_text::{BufferId, Selection, TextBuffer, TextPosition};
use tracing::error;

#[path = "native_files_editor_element.rs"]
mod element_impl;
#[path = "native_files_editor_language.rs"]
mod language_impl;
#[path = "native_files_editor_paint.rs"]
mod paint;

use paint::{
    EditorLayout, current_line_text, last_position, raw_column_for_display, uses_primary_shortcut,
};
use language_impl::overlay_kind_for_diagnostic_severity;

pub(crate) fn scroll_direction_and_count(
    event: &ScrollWheelEvent,
    line_height: Pixels,
) -> Option<(ScrollDirection, usize)> {
    paint::scroll_direction_and_count(event, line_height)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollDirection {
    Forward,
    Backward,
}

pub(crate) type SharedFilesEditor = Rc<RefCell<FilesEditor>>;

#[derive(Clone)]
pub(crate) struct FilesEditorStatusSnapshot {
    pub(crate) mode: &'static str,
    pub(crate) language: String,
    pub(crate) position: String,
    pub(crate) selection: String,
}

pub(crate) struct FilesEditor {
    editor: EditorState,
    registry: LanguageRegistry,
    syntax: SyntaxSession,
    next_buffer_id: u64,
    active_path: Option<PathBuf>,
    view_state_by_path: BTreeMap<PathBuf, FilesEditorViewState>,
    language_label: String,
    drag_anchor: Option<TextPosition>,
    fold_candidates: Vec<FoldCandidate>,
    search_query: Option<String>,
    syntax_highlights: Vec<HighlightCapture>,
    manual_overlays: Vec<OverlayDescriptor>,
}

#[derive(Clone)]
pub(crate) struct FilesEditorElement {
    state: SharedFilesEditor,
    is_focused: bool,
    style: TextStyle,
    palette: FilesEditorPalette,
}

#[derive(Clone, Copy)]
pub(crate) struct FilesEditorPalette {
    pub(crate) background: Hsla,
    pub(crate) active_line_background: Hsla,
    pub(crate) line_number: Hsla,
    pub(crate) current_line_number: Hsla,
    pub(crate) border: Hsla,
    pub(crate) default_foreground: Hsla,
    pub(crate) muted_foreground: Hsla,
    pub(crate) selection_background: Hsla,
    pub(crate) cursor: Hsla,
    pub(crate) invisible: Hsla,
    pub(crate) indent_guide: Hsla,
    pub(crate) fold_marker: Hsla,
    pub(crate) current_scope: Hsla,
    pub(crate) bracket_match: Hsla,
    pub(crate) diagnostic_error: Hsla,
    pub(crate) diagnostic_warning: Hsla,
    pub(crate) diagnostic_info: Hsla,
    pub(crate) diff_addition: Hsla,
    pub(crate) diff_deletion: Hsla,
    pub(crate) diff_modification: Hsla,
}

#[derive(Clone, Copy)]
pub(crate) struct FilesEditorPaletteOverlay {
    pub(crate) gutter_marker: Hsla,
    pub(crate) inline_background: Hsla,
}

impl FilesEditorPalette {
    pub(crate) fn overlay_colors(self, kind: OverlayKind) -> FilesEditorPaletteOverlay {
        match kind {
            OverlayKind::DiagnosticError => FilesEditorPaletteOverlay {
                gutter_marker: self.diagnostic_error,
                inline_background: self.diagnostic_error.opacity(0.28),
            },
            OverlayKind::DiagnosticWarning => FilesEditorPaletteOverlay {
                gutter_marker: self.diagnostic_warning,
                inline_background: self.diagnostic_warning.opacity(0.24),
            },
            OverlayKind::DiagnosticInfo => FilesEditorPaletteOverlay {
                gutter_marker: self.diagnostic_info,
                inline_background: self.diagnostic_info.opacity(0.22),
            },
            OverlayKind::DiffAddition => FilesEditorPaletteOverlay {
                gutter_marker: self.diff_addition,
                inline_background: self.diff_addition.opacity(0.10),
            },
            OverlayKind::DiffDeletion => FilesEditorPaletteOverlay {
                gutter_marker: self.diff_deletion,
                inline_background: self.diff_deletion.opacity(0.10),
            },
            OverlayKind::DiffModification => FilesEditorPaletteOverlay {
                gutter_marker: self.diff_modification,
                inline_background: self.diff_modification.opacity(0.10),
            },
        }
    }
}

#[derive(Clone)]
struct FilesEditorViewState {
    selection: Selection,
    viewport: Viewport,
    folded_regions: Vec<FoldRegion>,
    soft_wrap: bool,
    show_whitespace: bool,
}

impl FilesEditor {
    pub(crate) fn new() -> Self {
        Self {
            editor: EditorState::new(TextBuffer::new(BufferId::new(1), "")),
            registry: LanguageRegistry::builtin(),
            syntax: SyntaxSession::new(),
            next_buffer_id: 2,
            active_path: None,
            view_state_by_path: BTreeMap::new(),
            language_label: "text".to_string(),
            drag_anchor: None,
            fold_candidates: Vec::new(),
            search_query: None,
            syntax_highlights: Vec::new(),
            manual_overlays: Vec::new(),
        }
    }

    pub(crate) fn open_document(&mut self, path: &Path, contents: &str) -> Result<()> {
        self.capture_active_view_state();
        let buffer = TextBuffer::new(BufferId::new(self.next_buffer_id), contents);
        self.next_buffer_id = self.next_buffer_id.saturating_add(1);
        self.editor = EditorState::new(buffer);
        self.editor.apply(EditorCommand::SetViewport(Viewport {
            first_visible_row: 0,
            visible_row_count: 1,
            horizontal_offset: 0,
        }));
        self.active_path = Some(path.to_path_buf());
        self.language_label = self
            .registry
            .language_for_path(path)
            .map(|definition| definition.name.clone())
            .unwrap_or_else(|| "text".to_string());
        self.drag_anchor = None;
        self.fold_candidates.clear();
        self.syntax_highlights.clear();
        self.apply_path_defaults(path);
        self.refresh_syntax_state()?;
        if self.search_query.is_some() {
            self.editor
                .apply(EditorCommand::SetSearchQuery(self.search_query.clone()));
        }
        self.restore_view_state(path);
        Ok(())
    }

    pub(crate) fn clear(&mut self) {
        self.active_path = None;
        self.view_state_by_path.clear();
        self.editor = EditorState::new(TextBuffer::new(BufferId::new(self.next_buffer_id), ""));
        self.next_buffer_id = self.next_buffer_id.saturating_add(1);
        self.language_label = "text".to_string();
        self.drag_anchor = None;
        self.fold_candidates.clear();
        self.syntax_highlights.clear();
        self.manual_overlays.clear();
    }

    pub(crate) fn shutdown(&mut self) {
        self.clear();
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.editor.is_dirty()
    }

    pub(crate) fn current_text(&self) -> Option<String> {
        self.active_path.as_ref()?;
        Some(self.editor.buffer().text())
    }

    pub(crate) fn status_snapshot(&self) -> Option<FilesEditorStatusSnapshot> {
        self.active_path.as_ref()?;
        let status = self.editor.status_snapshot();
        let selection = self.editor.selection().range();
        Some(FilesEditorStatusSnapshot {
            mode: "EDIT",
            language: self.language_label.clone(),
            position: format!(
                "Ln {}  Col {}  {} lines",
                status.cursor_line, status.cursor_column, status.line_count
            ),
            selection: if selection.is_empty() {
                "1 cursor".to_string()
            } else {
                "Selection".to_string()
            },
        })
    }

    pub(crate) fn mark_saved(&mut self) {
        self.editor.apply(EditorCommand::MarkSaved);
    }

    pub(crate) fn copy_selection_text(&self) -> Option<String> {
        let mut clone = self.editor.clone();
        clone.apply(EditorCommand::CopySelection).copied_text
    }

    pub(crate) fn cut_selection_text(&mut self) -> Option<String> {
        self.active_path.as_ref()?;
        let output = self.apply_editor_command(EditorCommand::CutSelection);
        output.copied_text
    }

    pub(crate) fn paste_text(&mut self, text: &str) -> bool {
        if text.is_empty() || self.active_path.is_none() {
            return false;
        }

        let output = self.apply_editor_command(EditorCommand::Paste(text.to_string()));
        output.document_changed || output.selection_changed
    }

    pub(crate) fn sync_theme(&mut self, _is_dark: bool) {}

    pub(crate) fn show_whitespace(&self) -> bool {
        self.editor.show_whitespace()
    }

    pub(crate) fn soft_wrap_enabled(&self) -> bool {
        self.editor.wrap_width().is_some()
    }

    pub(crate) fn toggle_show_whitespace(&mut self) -> bool {
        let next = !self.show_whitespace();
        self.editor.apply(EditorCommand::SetShowWhitespace(next));
        true
    }

    pub(crate) fn toggle_soft_wrap(&mut self) -> bool {
        if self.soft_wrap_enabled() {
            self.editor.apply(EditorCommand::SetWrapWidth(None));
        } else {
            self.editor.apply(EditorCommand::SetWrapWidth(Some(80)));
        }
        true
    }

    pub(crate) fn set_search_query(&mut self, query: Option<&str>) {
        self.search_query = query
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(ToOwned::to_owned);
        self.editor
            .apply(EditorCommand::SetSearchQuery(self.search_query.clone()));
    }

    pub(crate) fn search_match_count(&self) -> usize {
        self.search_query
            .as_ref()
            .map(|query| self.editor.buffer().snapshot().find_all(query).len())
            .unwrap_or(0)
    }

    pub(crate) fn select_next_search_match(&mut self, forward: bool) -> bool {
        let Some(query) = self.search_query.as_ref() else {
            return false;
        };
        let snapshot = self.editor.buffer().snapshot();
        let matches = snapshot.find_all(query);
        if matches.is_empty() {
            return false;
        }

        let selection = self.editor.selection().range();
        let Ok(caret_start) = snapshot.position_to_byte(selection.start) else {
            return false;
        };
        let Ok(caret_end) = snapshot.position_to_byte(selection.end) else {
            return false;
        };

        let next = if forward {
            matches
                .iter()
                .find(|found| found.byte_range.start > caret_end)
                .or_else(|| matches.first())
        } else {
            matches
                .iter()
                .rev()
                .find(|found| found.byte_range.end < caret_start)
                .or_else(|| matches.last())
        };
        let Some(next) = next else {
            return false;
        };

        let Ok(start) = snapshot.byte_to_position(next.byte_range.start) else {
            return false;
        };
        let Ok(end) = snapshot.byte_to_position(next.byte_range.end) else {
            return false;
        };
        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(start, end)))
            .selection_changed
    }

    pub(crate) fn toggle_fold_at_line(&mut self, line: usize) -> bool {
        if self
            .editor
            .folded_regions()
            .iter()
            .any(|region| region.start_line <= line && line <= region.end_line)
        {
            self.editor.apply(EditorCommand::UnfoldAtLine { line });
            true
        } else {
            let Some(candidate) = self
                .fold_candidates
                .iter()
                .find(|candidate| candidate.start_line == line)
            else {
                return false;
            };
            self.editor.apply(EditorCommand::FoldLines {
                start_line: candidate.start_line,
                end_line: candidate.end_line,
            });
            true
        }
    }

    pub(crate) fn handle_keystroke(&mut self, keystroke: &Keystroke) -> bool {
        if self.active_path.is_none() {
            return false;
        }

        if self.handle_shortcut(keystroke) {
            return true;
        }

        match keystroke.key.as_str() {
            "left" => self.move_horizontally(false, keystroke.modifiers.shift),
            "right" => self.move_horizontally(true, keystroke.modifiers.shift),
            "up" => self.move_vertically(false, keystroke.modifiers.shift),
            "down" => self.move_vertically(true, keystroke.modifiers.shift),
            "home" => self.move_to_line_boundary(true, keystroke.modifiers.shift),
            "end" => self.move_to_line_boundary(false, keystroke.modifiers.shift),
            "pageup" => {
                self.page_scroll(ScrollDirection::Backward);
                true
            }
            "pagedown" => {
                self.page_scroll(ScrollDirection::Forward);
                true
            }
            "backspace" => {
                self.apply_editor_command(EditorCommand::DeleteBackward)
                    .document_changed
            }
            "delete" => {
                self.apply_editor_command(EditorCommand::DeleteForward)
                    .document_changed
            }
            "escape" => self.collapse_selection_to_head(),
            "enter" => self.insert_newline_with_indent(),
            "tab" if !keystroke.modifiers.control && !keystroke.modifiers.platform => {
                self.insert_text("    ")
            }
            _ => self.insert_key_char(keystroke),
        }
    }

    pub(crate) fn scroll_lines(&mut self, line_count: usize, direction: ScrollDirection) {
        let snapshot = self.editor.display_snapshot();
        let max_first_row = snapshot
            .total_display_rows
            .saturating_sub(snapshot.viewport.visible_row_count);
        let next_first_row = match direction {
            ScrollDirection::Backward => snapshot
                .viewport
                .first_visible_row
                .saturating_sub(line_count),
            ScrollDirection::Forward => min(
                snapshot
                    .viewport
                    .first_visible_row
                    .saturating_add(line_count),
                max_first_row,
            ),
        };
        self.editor.apply(EditorCommand::SetViewport(Viewport {
            first_visible_row: next_first_row,
            visible_row_count: snapshot.viewport.visible_row_count,
            horizontal_offset: 0,
        }));
    }

    fn page_scroll(&mut self, direction: ScrollDirection) {
        let snapshot = self.editor.display_snapshot();
        let page = snapshot.viewport.visible_row_count.max(1);
        self.scroll_lines(page, direction);
    }

    fn handle_shortcut(&mut self, keystroke: &Keystroke) -> bool {
        if !uses_primary_shortcut(keystroke) {
            return false;
        }

        match keystroke.key.as_str() {
            "a" if !keystroke.modifiers.shift => self.select_all(),
            "z" if !keystroke.modifiers.shift => {
                self.editor.apply(EditorCommand::Undo).document_changed
            }
            "z" if keystroke.modifiers.shift => {
                self.editor.apply(EditorCommand::Redo).document_changed
            }
            "y" if !cfg!(target_os = "macos") => {
                self.editor.apply(EditorCommand::Redo).document_changed
            }
            _ => false,
        }
    }

    fn move_horizontally(&mut self, forward: bool, extend: bool) -> bool {
        let selection = self.editor.selection();
        if !extend && !selection.is_caret() {
            let target = if forward {
                selection.range().end
            } else {
                selection.range().start
            };
            return self
                .editor
                .apply(EditorCommand::SetSelection(Selection::caret(target)))
                .selection_changed;
        }

        let anchor = selection.anchor;
        let output = if forward {
            self.editor.apply(EditorCommand::MoveRight)
        } else {
            self.editor.apply(EditorCommand::MoveLeft)
        };
        if !extend || !output.selection_changed {
            return output.selection_changed;
        }

        let head = self.editor.selection().head;
        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(anchor, head)))
            .selection_changed
    }

    fn move_vertically(&mut self, forward: bool, extend: bool) -> bool {
        let selection = self.editor.selection();
        if !extend && !selection.is_caret() {
            let target = if forward {
                selection.range().end
            } else {
                selection.range().start
            };
            return self
                .editor
                .apply(EditorCommand::SetSelection(Selection::caret(target)))
                .selection_changed;
        }

        let anchor = selection.anchor;
        let output = if forward {
            self.editor.apply(EditorCommand::MoveDown)
        } else {
            self.editor.apply(EditorCommand::MoveUp)
        };
        if !extend || !output.selection_changed {
            return output.selection_changed;
        }

        let head = self.editor.selection().head;
        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(anchor, head)))
            .selection_changed
    }

    fn move_to_line_boundary(&mut self, start: bool, extend: bool) -> bool {
        let selection = self.editor.selection();
        let snapshot = self.editor.buffer().snapshot();
        let line_text = current_line_text(&snapshot, selection.head.line);
        let column = if start { 0 } else { line_text.chars().count() };
        let target = TextPosition::new(selection.head.line, column);
        let next_selection = if extend {
            Selection::new(selection.anchor, target)
        } else {
            Selection::caret(target)
        };
        self.editor
            .apply(EditorCommand::SetSelection(next_selection))
            .selection_changed
    }

    fn collapse_selection_to_head(&mut self) -> bool {
        let head = self.editor.selection().head;
        self.editor
            .apply(EditorCommand::SetSelection(Selection::caret(head)))
            .selection_changed
    }

    fn select_all(&mut self) -> bool {
        let snapshot = self.editor.buffer().snapshot();
        let Some(end_position) = last_position(&snapshot) else {
            return false;
        };
        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(
                TextPosition::default(),
                end_position,
            )))
            .selection_changed
    }

    fn insert_key_char(&mut self, keystroke: &Keystroke) -> bool {
        if keystroke.modifiers.control || keystroke.modifiers.platform {
            return false;
        }

        let Some(text) = keystroke.key_char.as_deref() else {
            return false;
        };
        if text.is_empty() || matches!(keystroke.key.as_str(), "enter" | "tab") {
            return false;
        }

        self.insert_text(text)
    }

    fn insert_newline_with_indent(&mut self) -> bool {
        let selection = self.editor.selection();
        let snapshot = self.editor.buffer().snapshot();
        let line_text = current_line_text(&snapshot, selection.head.line);
        let indent: String = line_text
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect();
        self.insert_text(format!("\n{indent}").as_str())
    }

    fn insert_text(&mut self, text: &str) -> bool {
        self.apply_editor_command(EditorCommand::InsertText(text.to_string()))
            .document_changed
    }

    fn capture_active_view_state(&mut self) {
        let Some(path) = self.active_path.clone() else {
            return;
        };
        self.view_state_by_path.insert(
            path,
            FilesEditorViewState {
                selection: self.editor.selection(),
                viewport: self.editor.viewport(),
                folded_regions: self.editor.folded_regions().to_vec(),
                soft_wrap: self.soft_wrap_enabled(),
                show_whitespace: self.show_whitespace(),
            },
        );
    }

    fn restore_view_state(&mut self, path: &Path) {
        let Some(state) = self.view_state_by_path.get(path).cloned() else {
            return;
        };
        self.editor
            .apply(EditorCommand::SetViewport(state.viewport));
        self.editor
            .apply(EditorCommand::SetSelection(state.selection));
        self.editor
            .apply(EditorCommand::SetShowWhitespace(state.show_whitespace));
        self.editor
            .apply(EditorCommand::SetWrapWidth(state.soft_wrap.then_some(80)));
        for region in state.folded_regions {
            self.editor.apply(EditorCommand::FoldLines {
                start_line: region.start_line,
                end_line: region.end_line,
            });
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn set_selection_for_test(&mut self, selection: Selection) {
        self.editor.apply(EditorCommand::SetSelection(selection));
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn set_viewport_for_test(&mut self, viewport: Viewport) {
        self.editor.apply(EditorCommand::SetViewport(viewport));
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn selection_for_test(&self) -> Selection {
        self.editor.selection()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn viewport_for_test(&self) -> Viewport {
        self.editor.viewport()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn folded_region_count_for_test(&self) -> usize {
        self.editor.folded_regions().len()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn show_whitespace_for_test(&self) -> bool {
        self.show_whitespace()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn soft_wrap_enabled_for_test(&self) -> bool {
        self.soft_wrap_enabled()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn display_snapshot_for_test(
        &mut self,
        columns: usize,
        visible_rows: usize,
    ) -> hunk_editor::DisplaySnapshot {
        self.apply_layout(columns, visible_rows)
    }

    fn apply_layout(
        &mut self,
        columns: usize,
        visible_rows: usize,
    ) -> hunk_editor::DisplaySnapshot {
        if self.soft_wrap_enabled() {
            self.editor
                .apply(EditorCommand::SetWrapWidth(Some(columns.max(1))));
        }
        let viewport = self.editor.viewport();
        self.editor.apply(EditorCommand::SetViewport(Viewport {
            first_visible_row: viewport.first_visible_row,
            visible_row_count: visible_rows.max(1),
            horizontal_offset: 0,
        }));
        let display_snapshot = self.editor.display_snapshot();
        self.refresh_visible_syntax_highlights(&display_snapshot);
        display_snapshot
    }

    fn handle_mouse_down(
        &mut self,
        position: Point<Pixels>,
        layout: &EditorLayout,
        shift_held: bool,
    ) -> bool {
        if self.handle_fold_toggle_click(position, layout) {
            return true;
        }
        let Some(next_position) = self.position_for_point(position, layout) else {
            return false;
        };
        let anchor = if shift_held {
            self.drag_anchor.unwrap_or(self.editor.selection().anchor)
        } else {
            next_position
        };
        self.drag_anchor = Some(anchor);
        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(
                anchor,
                next_position,
            )));
        true
    }

    fn handle_mouse_drag(&mut self, position: Point<Pixels>, layout: &EditorLayout) -> bool {
        let Some(anchor) = self.drag_anchor else {
            return false;
        };
        let Some(next_position) = self.position_for_point(position, layout) else {
            return false;
        };
        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(
                anchor,
                next_position,
            )));
        true
    }

    fn handle_mouse_up(&mut self) -> bool {
        self.drag_anchor.take().is_some()
    }

    fn position_for_point(
        &self,
        position: Point<Pixels>,
        layout: &EditorLayout,
    ) -> Option<TextPosition> {
        if !layout.hitbox.bounds.contains(&position) {
            return None;
        }
        let row = ((position.y - layout.hitbox.bounds.origin.y) / layout.line_height)
            .floor()
            .max(0.0) as usize;
        let display_row = layout.display_snapshot.visible_rows.get(row)?;
        let display_column = if position.x <= layout.content_origin_x() {
            0
        } else {
            ((position.x - layout.content_origin_x()) / layout.cell_width)
                .floor()
                .max(0.0) as usize
        };
        let raw_column = raw_column_for_display(display_row, display_column);
        Some(TextPosition::new(display_row.source_line, raw_column))
    }

    fn apply_editor_command(&mut self, command: EditorCommand) -> hunk_editor::CommandOutput {
        let output = self.editor.apply(command);
        if output.document_changed
            && let Err(err) = self.refresh_syntax_state()
        {
            error!("failed to refresh native editor syntax state: {err:#}");
            self.fold_candidates.clear();
            self.syntax_highlights.clear();
            self.editor.apply(EditorCommand::SetParseStatus(
                hunk_language::ParseStatus::Failed,
            ));
            self.sync_overlays();
        }
        output
    }

    fn refresh_syntax_state(&mut self) -> Result<()> {
        let Some(path) = self.active_path.clone() else {
            self.fold_candidates.clear();
            self.syntax_highlights.clear();
            return Ok(());
        };

        let source = self.editor.buffer().text();
        let syntax = self.syntax.parse_for_path(&self.registry, &path, &source)?;
        self.fold_candidates = self.syntax.fold_candidates(&self.registry, &source);
        self.editor
            .apply(EditorCommand::SetLanguage(syntax.language_id));
        self.editor
            .apply(EditorCommand::SetParseStatus(syntax.parse_status));
        self.sync_overlays();
        Ok(())
    }

    fn refresh_visible_syntax_highlights(
        &mut self,
        display_snapshot: &hunk_editor::DisplaySnapshot,
    ) {
        let Some(first_row) = display_snapshot.visible_rows.first() else {
            self.syntax_highlights.clear();
            return;
        };
        let Some(last_row) = display_snapshot.visible_rows.last() else {
            self.syntax_highlights.clear();
            return;
        };

        let snapshot = self.editor.buffer().snapshot();
        let Ok(start) = snapshot.position_to_byte(TextPosition::new(
            first_row.source_line,
            first_row.raw_start_column,
        )) else {
            self.syntax_highlights.clear();
            return;
        };
        let Ok(end) = snapshot.position_to_byte(TextPosition::new(
            last_row.source_line,
            last_row.raw_end_column,
        )) else {
            self.syntax_highlights.clear();
            return;
        };
        if start >= end {
            self.syntax_highlights.clear();
            return;
        }

        let source = snapshot.text();
        let syntax_highlights = self
            .syntax
            .highlight_visible_range(&self.registry, &source, start..end)
            .unwrap_or_default();
        let semantic_highlights =
            semantic_token_captures(&source, self.editor.semantic_tokens(), start..end);
        self.syntax_highlights = merge_highlight_layers(&syntax_highlights, &semantic_highlights);
    }

    fn sync_overlays(&mut self) {
        let mut overlays = self.manual_overlays.clone();
        overlays.extend(
            self.editor
                .diagnostics()
                .iter()
                .map(|diagnostic| OverlayDescriptor {
                    line: diagnostic.range.start.line,
                    kind: overlay_kind_for_diagnostic_severity(diagnostic.severity),
                    message: Some(diagnostic.message.clone()),
                }),
        );
        if matches!(
            self.editor.status_snapshot().parse_status,
            hunk_language::ParseStatus::Failed
        ) {
            overlays.push(OverlayDescriptor {
                line: 0,
                kind: OverlayKind::DiagnosticError,
                message: Some("syntax parser failed for this file".to_string()),
            });
        }
        self.editor.apply(EditorCommand::SetOverlays(overlays));
    }

    fn apply_path_defaults(&mut self, path: &Path) {
        self.editor.apply(EditorCommand::SetShowWhitespace(
            default_show_whitespace_for_path(path),
        ));
        self.editor.apply(EditorCommand::SetWrapWidth(
            default_soft_wrap_for_path(path).then_some(80),
        ));
    }

    fn handle_fold_toggle_click(&mut self, position: Point<Pixels>, layout: &EditorLayout) -> bool {
        let row = ((position.y - layout.hitbox.bounds.origin.y) / layout.line_height)
            .floor()
            .max(0.0) as usize;
        let Some(display_row) = layout.display_snapshot.visible_rows.get(row) else {
            return false;
        };
        let fold_bounds = layout.fold_marker_bounds_for_row(
            display_row.row_index,
            layout.display_snapshot.visible_rows[0].row_index,
        );
        if !fold_bounds.contains(&position) {
            return false;
        }
        self.toggle_fold_at_line(display_row.source_line)
    }

    fn active_scope(&self) -> Option<FoldRegion> {
        let current_line = self.editor.selection().head.line;
        self.fold_candidates
            .iter()
            .filter(|candidate| {
                candidate.start_line <= current_line && current_line <= candidate.end_line
            })
            .min_by_key(|candidate| candidate.end_line - candidate.start_line)
            .and_then(|candidate| FoldRegion::new(candidate.start_line, candidate.end_line))
    }

    fn is_foldable_line(&self, line: usize) -> bool {
        self.fold_candidates
            .iter()
            .any(|candidate| candidate.start_line == line)
    }

    fn is_folded_line(&self, line: usize) -> bool {
        self.editor
            .folded_regions()
            .iter()
            .any(|region| region.start_line == line)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn set_overlays_for_test(&mut self, overlays: Vec<OverlayDescriptor>) {
        self.manual_overlays = overlays;
        self.sync_overlays();
    }
}

fn default_soft_wrap_for_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension,
                "md" | "mdx" | "markdown" | "txt" | "rst" | "json" | "yaml" | "yml" | "toml"
            )
        })
}

fn default_show_whitespace_for_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "md" | "mdx" | "markdown" | "txt"))
}
