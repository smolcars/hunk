use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use hunk_editor::WorkspaceDisplayRow;
use hunk_language::{
    HighlightCapture, SemanticToken, SyntaxSession, merge_highlight_layers, semantic_token_captures,
};
use hunk_text::{BufferId, TextPosition, TextSnapshot};

use super::{FilesEditor, RowSyntaxSpan, VisibleHighlightCache};

pub(super) struct WorkspaceDocumentSyntaxState {
    buffer_id: Option<BufferId>,
    buffer_version: u64,
    syntax: SyntaxSession,
    syntax_highlights: Vec<HighlightCapture>,
    visible_highlight_cache: Option<VisibleHighlightCache>,
}

impl WorkspaceDocumentSyntaxState {
    pub(super) fn new() -> Self {
        Self {
            buffer_id: None,
            buffer_version: 0,
            syntax: SyntaxSession::new(),
            syntax_highlights: Vec::new(),
            visible_highlight_cache: None,
        }
    }

    fn ensure_parsed(
        &mut self,
        registry: &hunk_language::LanguageRegistry,
        path: &Path,
        snapshot: &TextSnapshot,
    ) {
        if self.buffer_id == Some(snapshot.buffer_id) && self.buffer_version == snapshot.version {
            return;
        }

        let source = snapshot.text();
        let _ = self.syntax.parse_for_path(registry, path, &source);
        self.buffer_id = Some(snapshot.buffer_id);
        self.buffer_version = snapshot.version;
        self.syntax_highlights.clear();
        self.visible_highlight_cache = None;
    }
}

impl FilesEditor {
    pub(crate) fn workspace_display_segments_by_row(
        &mut self,
        visible_rows: &[WorkspaceDisplayRow],
    ) -> Option<BTreeMap<usize, Vec<RowSyntaxSpan>>> {
        let layout = self.workspace_session.layout()?.clone();
        let mut rows_by_path = BTreeMap::<PathBuf, Vec<(usize, WorkspaceDisplayRow)>>::new();
        for row in visible_rows {
            let Some(location) = row.location.as_ref() else {
                continue;
            };
            let Some(document_line) = location.document_line else {
                continue;
            };
            let Some(document) = layout.document(location.document_id) else {
                continue;
            };
            rows_by_path
                .entry(document.path.clone())
                .or_default()
                .push((document_line, row.clone()));
        }

        let mut segments_by_row = BTreeMap::new();
        let registry = self.registry.clone();
        let active_path = self.active_path_buf().map(|path| path.to_path_buf());

        for (path, document_rows) in rows_by_path {
            let snapshot = match self.workspace_document_snapshot(path.as_path()) {
                Some(snapshot) => snapshot,
                None => continue,
            };
            if active_path.as_deref() == Some(path.as_path()) {
                refresh_workspace_document_visible_highlights(
                    &registry,
                    &snapshot,
                    self.editor.semantic_tokens(),
                    self.semantic_highlight_revision,
                    &mut self.syntax,
                    &mut self.visible_highlight_cache,
                    &mut self.syntax_highlights,
                    &document_rows,
                );
                self.invalidate_row_syntax_cache();

                for (document_line, row) in &document_rows {
                    let spans = workspace_row_syntax_spans(
                        row,
                        *document_line,
                        &self.syntax_highlights,
                        &snapshot,
                    );
                    segments_by_row.insert(row.row_index, spans);
                }
                continue;
            }

            let state = self
                .workspace_syntax_by_path
                .entry(path.clone())
                .or_insert_with(WorkspaceDocumentSyntaxState::new);
            state.ensure_parsed(&registry, path.as_path(), &snapshot);
            refresh_workspace_document_visible_highlights(
                &registry,
                &snapshot,
                &[],
                0,
                &mut state.syntax,
                &mut state.visible_highlight_cache,
                &mut state.syntax_highlights,
                &document_rows,
            );

            for (document_line, row) in &document_rows {
                let spans = workspace_row_syntax_spans(
                    row,
                    *document_line,
                    &state.syntax_highlights,
                    &snapshot,
                );
                segments_by_row.insert(row.row_index, spans);
            }
        }

        Some(segments_by_row)
    }

    fn workspace_document_snapshot(&self, path: &Path) -> Option<TextSnapshot> {
        if self.active_path() == Some(path) {
            return Some(self.editor.buffer().snapshot());
        }

        self.workspace_buffers
            .get(path)
            .map(|buffer| buffer.snapshot())
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn workspace_row_has_non_plain_syntax_for_test(
        &mut self,
        visible_rows: &[WorkspaceDisplayRow],
        row_index: usize,
    ) -> bool {
        self.workspace_display_segments_by_row(visible_rows)
            .and_then(|spans_by_row| spans_by_row.get(&row_index).cloned())
            .is_some_and(|spans| !spans.is_empty())
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh_workspace_document_visible_highlights(
    registry: &hunk_language::LanguageRegistry,
    snapshot: &TextSnapshot,
    semantic_tokens: &[SemanticToken],
    semantic_revision: u64,
    syntax: &mut SyntaxSession,
    visible_highlight_cache: &mut Option<VisibleHighlightCache>,
    syntax_highlights: &mut Vec<HighlightCapture>,
    document_rows: &[(usize, WorkspaceDisplayRow)],
) {
    let Some(byte_range) = workspace_document_visible_byte_range(snapshot, document_rows) else {
        syntax_highlights.clear();
        *visible_highlight_cache = None;
        return;
    };

    if visible_highlight_cache.as_ref().is_some_and(|cache| {
        cache.buffer_id == snapshot.buffer_id
            && cache.buffer_version == snapshot.version
            && cache.semantic_revision == semantic_revision
            && cache.byte_range.start <= byte_range.start
            && cache.byte_range.end >= byte_range.end
    }) {
        if let Some(cache) = visible_highlight_cache.as_ref() {
            *syntax_highlights = cache.captures.clone();
        }
        return;
    }

    let source = snapshot.text();
    let syntax_highlights_for_range = syntax
        .highlight_visible_range(registry, &source, byte_range.clone())
        .unwrap_or_default();
    let semantic_highlights = semantic_token_captures(&source, semantic_tokens, byte_range.clone());
    let captures = super::compact_highlight_captures(merge_highlight_layers(
        &syntax_highlights_for_range,
        &semantic_highlights,
    ));
    *visible_highlight_cache = Some(VisibleHighlightCache {
        buffer_id: snapshot.buffer_id,
        buffer_version: snapshot.version,
        semantic_revision,
        byte_range,
        captures: captures.clone(),
    });
    *syntax_highlights = captures;
}

fn workspace_document_visible_byte_range(
    snapshot: &TextSnapshot,
    document_rows: &[(usize, WorkspaceDisplayRow)],
) -> Option<Range<usize>> {
    let mut start = usize::MAX;
    let mut end = 0usize;

    for (document_line, row) in document_rows {
        let row_start = snapshot
            .position_to_byte(TextPosition::new(*document_line, row.raw_start_column))
            .ok()?;
        let row_end = snapshot
            .position_to_byte(TextPosition::new(*document_line, row.raw_end_column))
            .ok()?;
        start = start.min(row_start);
        end = end.max(row_end);
    }

    (start < end).then_some(start..end)
}

fn workspace_row_syntax_spans(
    row: &WorkspaceDisplayRow,
    document_line: usize,
    captures: &[HighlightCapture],
    snapshot: &TextSnapshot,
) -> Vec<RowSyntaxSpan> {
    if row.text.is_empty() {
        return Vec::new();
    }

    let Ok(row_start) =
        snapshot.position_to_byte(TextPosition::new(document_line, row.raw_start_column))
    else {
        return Vec::new();
    };
    let Ok(row_end) =
        snapshot.position_to_byte(TextPosition::new(document_line, row.raw_end_column))
    else {
        return Vec::new();
    };

    let mut spans = Vec::new();
    let mut scan_index = captures.partition_point(|capture| capture.byte_range.end <= row_start);
    while scan_index < captures.len() {
        let capture = &captures[scan_index];
        if capture.byte_range.start >= row_end {
            break;
        }

        let start = capture.byte_range.start.max(row_start);
        let end = capture.byte_range.end.min(row_end);
        if start < end
            && let (Ok(start_position), Ok(end_position)) = (
                snapshot.byte_to_position(start),
                snapshot.byte_to_position(end),
            )
        {
            let start_column = workspace_display_column_for_raw(row, start_position.column);
            let end_column = workspace_display_column_for_raw(row, end_position.column);
            if start_column < end_column {
                push_workspace_row_syntax_span(
                    &mut spans,
                    RowSyntaxSpan {
                        start_column,
                        end_column,
                        style_key: capture.style_key.clone(),
                    },
                );
            }
        }

        scan_index += 1;
    }

    spans
}

fn push_workspace_row_syntax_span(spans: &mut Vec<RowSyntaxSpan>, next: RowSyntaxSpan) {
    if let Some(previous) = spans.last_mut()
        && previous.end_column == next.start_column
        && previous.style_key == next.style_key
    {
        previous.end_column = next.end_column;
        return;
    }

    spans.push(next);
}

fn workspace_display_column_for_raw(row: &WorkspaceDisplayRow, raw_column: usize) -> usize {
    if row.raw_column_offsets.is_empty() {
        return 0;
    }

    let relative_raw = raw_column
        .saturating_sub(row.raw_start_column)
        .min(row.raw_column_offsets.len().saturating_sub(1));
    row.raw_column_offsets[relative_raw]
}
