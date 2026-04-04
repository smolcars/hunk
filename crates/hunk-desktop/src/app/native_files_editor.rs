use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Result;
use gpui::*;
use hunk_editor::{
    EditorCommand, EditorState, FoldRegion, OverlayDescriptor, OverlayKind, Viewport,
    WorkspaceExcerptId,
};
use hunk_language::{
    FoldCandidate, HighlightCapture, LanguageRegistry, SyntaxSession, merge_highlight_layers,
    semantic_token_captures,
};
use hunk_text::{BufferId, Selection, TextBuffer, TextPosition};
use tracing::error;

#[path = "native_files_editor_element.rs"]
mod element_impl;
#[path = "native_files_editor_input.rs"]
mod input_impl;
#[path = "native_files_editor_language.rs"]
mod language_impl;
#[path = "native_files_editor_paint.rs"]
pub(crate) mod paint;
#[path = "native_files_editor_workspace_buffers.rs"]
mod workspace_buffers_impl;
#[path = "native_files_editor_workspace_display.rs"]
mod workspace_display_impl;
#[path = "native_files_editor_workspace_search.rs"]
mod workspace_search_impl;
#[path = "native_files_editor_workspace.rs"]
mod workspace_session;
#[path = "native_files_editor_workspace_syntax.rs"]
mod workspace_syntax_impl;

use language_impl::overlay_kind_for_diagnostic_severity;
use paint::{EditorLayout, RowSyntaxSpan, build_row_syntax_spans_for_row};
#[allow(unused_imports)]
pub(crate) use workspace_search_impl::WorkspaceSearchTarget;
pub(crate) use workspace_session::WorkspaceEditorSession;

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
type FilesEditorSecondaryClickHandler =
    Rc<dyn Fn(FilesEditorSecondaryClickTarget, Point<Pixels>, &mut Window, &mut App)>;

#[derive(Clone, Copy)]
pub(crate) struct FilesEditorSecondaryClickTarget {
    pub(crate) can_cut: bool,
    pub(crate) can_copy: bool,
    pub(crate) can_paste: bool,
    pub(crate) can_select_all: bool,
}

#[derive(Clone)]
pub(crate) struct FilesEditorStatusSnapshot {
    pub(crate) language: String,
    pub(crate) position: String,
    pub(crate) selection: String,
}

pub(crate) struct FilesEditor {
    editor: EditorState,
    registry: LanguageRegistry,
    syntax: SyntaxSession,
    next_buffer_id: u64,
    workspace_session: WorkspaceEditorSession,
    workspace_buffers: BTreeMap<PathBuf, TextBuffer>,
    workspace_syntax_by_path:
        BTreeMap<PathBuf, workspace_syntax_impl::WorkspaceDocumentSyntaxState>,
    view_state_by_path: BTreeMap<PathBuf, FilesEditorViewState>,
    language_label: String,
    pointer_selection: Option<PointerSelectionState>,
    fold_candidates: Vec<FoldCandidate>,
    search_query: Option<String>,
    syntax_highlights: Vec<HighlightCapture>,
    manual_overlays: Vec<OverlayDescriptor>,
    visible_highlight_cache: Option<VisibleHighlightCache>,
    row_syntax_cache: Option<RowSyntaxSpanCache>,
    semantic_highlight_revision: u64,
    syntax_highlight_revision: u64,
}

#[derive(Clone)]
pub(crate) struct FilesEditorElement {
    state: SharedFilesEditor,
    on_secondary_mouse_down: FilesEditorSecondaryClickHandler,
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

#[derive(Clone, Copy)]
struct PointerSelectionState {
    anchor: TextPosition,
    mode: PointerSelectionMode,
}

#[derive(Clone)]
struct VisibleHighlightCache {
    buffer_id: BufferId,
    buffer_version: u64,
    semantic_revision: u64,
    byte_range: Range<usize>,
    captures: Vec<HighlightCapture>,
}

struct RowSyntaxSpanCache {
    buffer_id: BufferId,
    buffer_version: u64,
    syntax_revision: u64,
    spans_by_signature: HashMap<VisibleRowSignature, Vec<RowSyntaxSpan>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct VisibleRowSignature {
    row_index: usize,
    source_line: usize,
    raw_start_column: usize,
    raw_end_column: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PointerSelectionMode {
    Character,
    Word,
    Line,
}

impl FilesEditor {
    pub(crate) fn new() -> Self {
        Self {
            editor: EditorState::new(TextBuffer::new(BufferId::new(1), "")),
            registry: LanguageRegistry::builtin(),
            syntax: SyntaxSession::new(),
            next_buffer_id: 2,
            workspace_session: WorkspaceEditorSession::new(),
            workspace_buffers: BTreeMap::new(),
            workspace_syntax_by_path: BTreeMap::new(),
            view_state_by_path: BTreeMap::new(),
            language_label: "text".to_string(),
            pointer_selection: None,
            fold_candidates: Vec::new(),
            search_query: None,
            syntax_highlights: Vec::new(),
            manual_overlays: Vec::new(),
            visible_highlight_cache: None,
            row_syntax_cache: None,
            semantic_highlight_revision: 0,
            syntax_highlight_revision: 0,
        }
    }

    pub(crate) fn open_document(&mut self, path: &Path, contents: &str) -> Result<()> {
        self.open_workspace_documents(vec![(path.to_path_buf(), contents.to_string())], Some(path))
    }

    pub(crate) fn clear(&mut self) {
        self.workspace_session.clear();
        self.workspace_buffers.clear();
        self.workspace_syntax_by_path.clear();
        self.view_state_by_path.clear();
        self.editor = EditorState::new(TextBuffer::new(BufferId::new(self.next_buffer_id), ""));
        self.next_buffer_id = self.next_buffer_id.saturating_add(1);
        self.language_label = "text".to_string();
        self.pointer_selection = None;
        self.fold_candidates.clear();
        self.clear_syntax_highlights();
        self.manual_overlays.clear();
        self.visible_highlight_cache = None;
        self.row_syntax_cache = None;
        self.semantic_highlight_revision = 0;
        self.syntax_highlight_revision = 0;
    }

    pub(crate) fn shutdown(&mut self) {
        self.clear();
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.editor.is_dirty()
    }

    pub(crate) fn current_text(&self) -> Option<String> {
        self.active_path()?;
        Some(self.editor.buffer().text())
    }

    pub(crate) fn status_snapshot(&self) -> Option<FilesEditorStatusSnapshot> {
        self.active_path()?;
        let status = self.editor.status_snapshot();
        let selection = self.editor.selection().range();
        Some(FilesEditorStatusSnapshot {
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
        self.active_path()?;
        let output = self.apply_editor_command(EditorCommand::CutSelection);
        output.copied_text
    }

    pub(crate) fn paste_text(&mut self, text: &str) -> bool {
        if text.is_empty() || self.active_path().is_none() {
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
        let Some(query) = self.search_query.as_ref() else {
            return 0;
        };
        self.workspace_search_matches(query)
            .map(|matches| matches.len())
            .unwrap_or_else(|| self.editor.buffer().snapshot().find_all(query).len())
    }

    pub(crate) fn select_next_search_match(&mut self, forward: bool) -> bool {
        let Some(query) = self.search_query.clone() else {
            return false;
        };
        if let Some(matches) = self.workspace_search_matches(query.as_str()) {
            return self.select_next_workspace_search_match(&matches, forward);
        }
        let snapshot = self.editor.buffer().snapshot();
        let matches = snapshot.find_all(query.as_str());
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

    pub(crate) fn replace_selected_search_match(&mut self, replacement: &str) -> bool {
        let Some(query) = self.search_query.clone() else {
            return false;
        };
        if self.active_path().is_none() {
            return false;
        }

        let snapshot = self.editor.buffer().snapshot();
        let matches = snapshot.find_all(query.as_str());
        if matches.is_empty() {
            return false;
        }

        let selection = self.editor.selection().range();
        let Ok(selection_start) = snapshot.position_to_byte(selection.start) else {
            return false;
        };
        let Ok(selection_end) = snapshot.position_to_byte(selection.end) else {
            return false;
        };

        let target = matches
            .iter()
            .find(|found| {
                found.byte_range.start == selection_start && found.byte_range.end == selection_end
            })
            .or_else(|| {
                matches
                    .iter()
                    .find(|found| found.byte_range.start >= selection_end)
            })
            .or_else(|| matches.first());
        let Some(target) = target else {
            return false;
        };

        let Ok(start) = snapshot.byte_to_position(target.byte_range.start) else {
            return false;
        };
        let Ok(end) = snapshot.byte_to_position(target.byte_range.end) else {
            return false;
        };

        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(start, end)));
        let output =
            self.apply_editor_command(EditorCommand::ReplaceSelection(replacement.to_string()));
        self.editor
            .apply(EditorCommand::SetSearchQuery(self.search_query.clone()));
        output.document_changed
    }

    pub(crate) fn replace_all_search_matches(&mut self, replacement: &str) -> bool {
        let Some(query) = self.search_query.clone() else {
            return false;
        };
        if self.active_path().is_none() {
            return false;
        }

        let current_text = self.editor.buffer().text();
        if query.is_empty() || !current_text.contains(query.as_str()) {
            return false;
        }

        let next_text = current_text.replace(query.as_str(), replacement);
        let output = self.apply_editor_command(EditorCommand::ReplaceAll(next_text));
        self.editor
            .apply(EditorCommand::SetSearchQuery(self.search_query.clone()));
        output.document_changed
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

    fn capture_active_view_state(&mut self) {
        let Some(path) = self.active_path_buf() else {
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
            .apply(EditorCommand::SetShowWhitespace(state.show_whitespace));
        self.editor
            .apply(EditorCommand::SetWrapWidth(state.soft_wrap.then_some(80)));
        for region in state.folded_regions {
            self.editor.apply(EditorCommand::FoldLines {
                start_line: region.start_line,
                end_line: region.end_line,
            });
        }
        self.editor
            .apply(EditorCommand::SetSelection(state.selection));
        self.editor
            .apply(EditorCommand::SetViewport(state.viewport));
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

    fn apply_editor_command(&mut self, command: EditorCommand) -> hunk_editor::CommandOutput {
        let output = self.editor.apply(command);
        if output.document_changed
            && let Err(err) = self.refresh_syntax_state()
        {
            error!("failed to refresh native editor syntax state: {err:#}");
            self.fold_candidates.clear();
            self.clear_syntax_highlights();
            self.visible_highlight_cache = None;
            self.row_syntax_cache = None;
            self.semantic_highlight_revision = self.semantic_highlight_revision.saturating_add(1);
            self.editor.apply(EditorCommand::SetParseStatus(
                hunk_language::ParseStatus::Failed,
            ));
            self.sync_overlays();
        }
        output
    }

    fn refresh_syntax_state(&mut self) -> Result<()> {
        let Some(path) = self.active_path_buf() else {
            self.fold_candidates.clear();
            self.clear_syntax_highlights();
            self.visible_highlight_cache = None;
            self.row_syntax_cache = None;
            self.semantic_highlight_revision = self.semantic_highlight_revision.saturating_add(1);
            return Ok(());
        };

        let source = self.editor.buffer().text();
        let syntax = self.syntax.parse_for_path(&self.registry, &path, &source)?;
        self.fold_candidates = self.syntax.fold_candidates(&self.registry, &source);
        self.editor
            .apply(EditorCommand::SetLanguage(syntax.language_id));
        self.editor
            .apply(EditorCommand::SetParseStatus(syntax.parse_status));
        self.visible_highlight_cache = None;
        self.row_syntax_cache = None;
        self.semantic_highlight_revision = self.semantic_highlight_revision.saturating_add(1);
        self.sync_overlays();
        Ok(())
    }

    fn active_path(&self) -> Option<&Path> {
        self.workspace_session.active_path()
    }

    fn active_path_buf(&self) -> Option<PathBuf> {
        self.workspace_session.active_path_buf()
    }

    pub(crate) fn active_workspace_path_buf(&self) -> Option<PathBuf> {
        self.active_path_buf()
    }

    pub(crate) fn active_workspace_excerpt_id(&self) -> Option<WorkspaceExcerptId> {
        self.workspace_session.active_excerpt_id()
    }

    fn refresh_visible_syntax_highlights(
        &mut self,
        display_snapshot: &hunk_editor::DisplaySnapshot,
    ) {
        let Some(first_row) = display_snapshot.visible_rows.first() else {
            self.clear_syntax_highlights();
            return;
        };
        let Some(last_row) = display_snapshot.visible_rows.last() else {
            self.clear_syntax_highlights();
            return;
        };

        let snapshot = self.editor.buffer().snapshot();
        let Ok(visible_start) = snapshot.position_to_byte(TextPosition::new(
            first_row.source_line,
            first_row.raw_start_column,
        )) else {
            self.clear_syntax_highlights();
            return;
        };
        let Ok(visible_end) = snapshot.position_to_byte(TextPosition::new(
            last_row.source_line,
            last_row.raw_end_column,
        )) else {
            self.clear_syntax_highlights();
            return;
        };
        if visible_start >= visible_end {
            self.clear_syntax_highlights();
            return;
        }

        if self.visible_highlight_cache.as_ref().is_some_and(|cache| {
            cache.buffer_id == snapshot.buffer_id
                && cache.buffer_version == snapshot.version
                && cache.semantic_revision == self.semantic_highlight_revision
                && cache.byte_range.start <= visible_start
                && visible_end <= cache.byte_range.end
        }) {
            return;
        }

        let highlight_overscan_lines =
            highlight_overscan_lines(display_snapshot.visible_rows.len());
        let start_line = first_row
            .source_line
            .saturating_sub(highlight_overscan_lines);
        let end_line = last_row
            .source_line
            .saturating_add(highlight_overscan_lines)
            .min(snapshot.line_count().saturating_sub(1));
        let Ok(start) = snapshot.line_to_byte(start_line) else {
            self.clear_syntax_highlights();
            return;
        };
        let end = if end_line + 1 < snapshot.line_count() {
            snapshot
                .line_to_byte(end_line + 1)
                .unwrap_or(snapshot.byte_len())
        } else {
            snapshot.byte_len()
        };
        if start >= end {
            self.clear_syntax_highlights();
            return;
        }

        let source = snapshot.text();
        let captures = if let Some(cache) =
            self.visible_highlight_cache
                .as_ref()
                .cloned()
                .filter(|cache| {
                    cache.buffer_id == snapshot.buffer_id
                        && cache.buffer_version == snapshot.version
                        && cache.semantic_revision == self.semantic_highlight_revision
                        && ranges_overlap_or_touch(&cache.byte_range, &(start..end))
                }) {
            let mut merged_captures = Vec::new();
            let merged_start = cache.byte_range.start.min(start);
            let merged_end = cache.byte_range.end.max(end);

            if start < cache.byte_range.start {
                merged_captures.extend(
                    self.highlight_captures_for_range(&source, start..cache.byte_range.start),
                );
            }

            merged_captures.extend(
                cache
                    .captures
                    .iter()
                    .filter(|capture| {
                        capture.byte_range.start < merged_end
                            && merged_start < capture.byte_range.end
                    })
                    .cloned(),
            );

            if cache.byte_range.end < end {
                merged_captures
                    .extend(self.highlight_captures_for_range(&source, cache.byte_range.end..end));
            }

            self.visible_highlight_cache = Some(VisibleHighlightCache {
                buffer_id: snapshot.buffer_id,
                buffer_version: snapshot.version,
                semantic_revision: self.semantic_highlight_revision,
                byte_range: merged_start..merged_end,
                captures: merged_captures.clone(),
            });
            merged_captures
        } else {
            let fresh_captures = self.highlight_captures_for_range(&source, start..end);
            self.visible_highlight_cache = Some(VisibleHighlightCache {
                buffer_id: snapshot.buffer_id,
                buffer_version: snapshot.version,
                semantic_revision: self.semantic_highlight_revision,
                byte_range: start..end,
                captures: fresh_captures.clone(),
            });
            fresh_captures
        };
        self.set_syntax_highlights(captures);
    }

    fn highlight_captures_for_range(
        &mut self,
        source: &str,
        range: Range<usize>,
    ) -> Vec<HighlightCapture> {
        if range.start >= range.end {
            return Vec::new();
        }

        let syntax_highlights = self
            .syntax
            .highlight_visible_range(&self.registry, source, range.clone())
            .unwrap_or_default();
        let semantic_highlights =
            semantic_token_captures(source, self.editor.semantic_tokens(), range);
        compact_highlight_captures(merge_highlight_layers(
            &syntax_highlights,
            &semantic_highlights,
        ))
    }

    pub(crate) fn row_syntax_spans(
        &mut self,
        visible_rows: &[hunk_editor::DisplayRow],
    ) -> Rc<BTreeMap<usize, Vec<RowSyntaxSpan>>> {
        let snapshot = self.editor.buffer().snapshot();
        let rebuild_needed = self.row_syntax_cache.as_ref().is_none_or(|cache| {
            cache.buffer_id != snapshot.buffer_id
                || cache.buffer_version != snapshot.version
                || cache.syntax_revision != self.syntax_highlight_revision
        });

        if rebuild_needed {
            self.row_syntax_cache = Some(RowSyntaxSpanCache {
                buffer_id: snapshot.buffer_id,
                buffer_version: snapshot.version,
                syntax_revision: self.syntax_highlight_revision,
                spans_by_signature: HashMap::new(),
            });
        }

        let cache = self
            .row_syntax_cache
            .as_mut()
            .expect("row syntax cache populated");
        let mut spans_by_row = BTreeMap::new();
        for row in visible_rows {
            let signature = VisibleRowSignature {
                row_index: row.row_index,
                source_line: row.source_line,
                raw_start_column: row.raw_start_column,
                raw_end_column: row.raw_end_column,
            };
            let spans = cache
                .spans_by_signature
                .entry(signature)
                .or_insert_with(|| {
                    build_row_syntax_spans_for_row(row, &self.syntax_highlights, &snapshot)
                });
            if !spans.is_empty() {
                spans_by_row.insert(row.row_index, spans.clone());
            }
        }

        Rc::new(spans_by_row)
    }

    fn invalidate_row_syntax_cache(&mut self) {
        self.syntax_highlight_revision = self.syntax_highlight_revision.saturating_add(1);
        self.row_syntax_cache = None;
    }

    fn set_syntax_highlights(&mut self, captures: Vec<HighlightCapture>) {
        if self.syntax_highlights != captures {
            self.syntax_highlights = captures;
            self.invalidate_row_syntax_cache();
        }
    }

    fn clear_syntax_highlights(&mut self) {
        if !self.syntax_highlights.is_empty() {
            self.syntax_highlights.clear();
            self.invalidate_row_syntax_cache();
        }
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

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn visible_highlight_range_for_test(&self) -> Option<Range<usize>> {
        self.visible_highlight_cache
            .as_ref()
            .map(|cache| cache.byte_range.clone())
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn workspace_layout_for_test(&self) -> Option<&hunk_editor::WorkspaceLayout> {
        self.workspace_session.layout()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn active_workspace_document_id_for_test(
        &self,
    ) -> Option<hunk_editor::WorkspaceDocumentId> {
        self.workspace_session.active_document_id()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn active_workspace_excerpt_id_for_test(
        &self,
    ) -> Option<hunk_editor::WorkspaceExcerptId> {
        self.workspace_session.active_excerpt_id()
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

fn highlight_overscan_lines(visible_row_count: usize) -> usize {
    let dynamic_rows = visible_row_count.saturating_mul(8);
    dynamic_rows.clamp(192, 512)
}

fn ranges_overlap_or_touch(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start <= right.end && right.start <= left.end
}

fn compact_highlight_captures(captures: Vec<HighlightCapture>) -> Vec<HighlightCapture> {
    let mut compacted: Vec<HighlightCapture> = Vec::with_capacity(captures.len());
    for capture in captures {
        if let Some(previous) = compacted.last_mut()
            && previous.style_key == capture.style_key
            && previous.name == capture.name
            && previous.byte_range.end >= capture.byte_range.start
        {
            previous.byte_range.end = previous.byte_range.end.max(capture.byte_range.end);
            continue;
        }
        compacted.push(capture);
    }
    compacted
}
