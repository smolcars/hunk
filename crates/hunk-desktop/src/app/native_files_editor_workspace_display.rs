use std::collections::BTreeMap;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
use hunk_editor::DisplayRow;
#[cfg(test)]
use hunk_editor::{
    EditorCommand, EditorState, WorkspaceProjectedSnapshot, build_workspace_projected_snapshot,
};
use hunk_editor::{
    SearchHighlight, WorkspaceDisplaySnapshot, WorkspaceDocumentId, WorkspaceExcerptId,
    WorkspaceLayout, WorkspaceRowLocation,
};
use hunk_editor::{Viewport, WorkspaceDisplayRow};
use hunk_text::TextBuffer;
use hunk_text::TextSnapshot;
#[cfg(test)]
use hunk_text::{Selection, TextPosition};

#[allow(clippy::duplicate_mod)]
#[path = "workspace_display_buffers.rs"]
mod workspace_display_buffers;

use workspace_display_buffers::{
    WorkspaceSearchMatch, build_workspace_display_snapshot_from_document_snapshots,
    find_workspace_search_matches,
};

use super::FilesEditor;
use super::RowSyntaxSpan;
#[cfg(test)]
use super::{FilesEditorViewState, default_show_whitespace_for_path};

#[cfg(test)]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct WorkspaceProjectedRenderSnapshot {
    pub(crate) projection: WorkspaceProjectedSnapshot,
    pub(crate) visible_display_rows: Vec<WorkspaceDisplayRow>,
    pub(crate) syntax_by_display_row: BTreeMap<usize, Vec<RowSyntaxSpan>>,
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) line_number_digits: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceVisibleRenderSnapshot {
    pub(crate) rows_by_display_row: BTreeMap<usize, WorkspaceDisplayRow>,
    pub(crate) syntax_by_display_row: BTreeMap<usize, Vec<RowSyntaxSpan>>,
}

impl FilesEditor {
    pub(crate) fn build_workspace_display_snapshot(
        &self,
        viewport: Viewport,
        tab_width: usize,
        show_whitespace: bool,
    ) -> Option<WorkspaceDisplaySnapshot> {
        let layout = self.workspace_session.layout()?;
        let document_snapshots = layout
            .documents()
            .iter()
            .filter_map(|document| {
                self.workspace_buffer_for_document(document.id)
                    .map(|buffer| (document.id, buffer.snapshot()))
            })
            .collect::<BTreeMap<_, _>>();
        let mut snapshot = build_workspace_display_snapshot_from_document_snapshots(
            layout,
            viewport,
            tab_width,
            show_whitespace,
            &document_snapshots,
        );
        if let Some(query) = self.search_query.as_deref() {
            apply_workspace_search_highlights(
                layout,
                &mut snapshot.visible_rows,
                query,
                &document_snapshots,
            );
        }
        Some(snapshot)
    }

    pub(crate) fn build_workspace_visible_render_snapshot(
        &mut self,
        viewport: Viewport,
        tab_width: usize,
    ) -> Option<WorkspaceVisibleRenderSnapshot> {
        let snapshot = self.build_workspace_display_snapshot(viewport, tab_width, false)?;
        let syntax_by_display_row =
            self.workspace_display_segments_by_row(&snapshot.visible_rows)?;
        let rows_by_display_row = snapshot
            .visible_rows
            .into_iter()
            .map(|row| (row.row_index, row))
            .collect();
        Some(WorkspaceVisibleRenderSnapshot {
            rows_by_display_row,
            syntax_by_display_row,
        })
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn build_workspace_projected_snapshot(
        &self,
        viewport: Viewport,
        tab_width: usize,
    ) -> Option<WorkspaceProjectedSnapshot> {
        let layout = self.workspace_session.layout()?;
        let display_rows_by_path = layout
            .documents()
            .iter()
            .filter_map(|document| {
                self.workspace_document_display_rows(document.path(), tab_width.max(1))
                    .map(|rows| (document.path.clone(), rows))
            })
            .collect::<BTreeMap<PathBuf, Vec<DisplayRow>>>();

        Some(build_workspace_projected_snapshot(
            layout,
            viewport,
            |excerpt| {
                let Some(document) = layout.document(excerpt.spec.document_id) else {
                    return Vec::new();
                };
                let Some(rows) = display_rows_by_path.get(document.path()) else {
                    return Vec::new();
                };
                rows.iter()
                    .filter(|row| excerpt.spec.line_range.contains(&row.source_line))
                    .cloned()
                    .collect()
            },
        ))
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn build_workspace_projected_render_snapshot(
        &mut self,
        viewport: Viewport,
        tab_width: usize,
    ) -> Option<WorkspaceProjectedRenderSnapshot> {
        let projection = self.build_workspace_projected_snapshot(viewport, tab_width)?;
        let visible_display_rows =
            workspace_display_rows_from_projected_snapshot(&projection.visible_rows)?;
        let syntax_by_display_row =
            self.workspace_display_segments_by_row(&visible_display_rows)?;
        Some(WorkspaceProjectedRenderSnapshot {
            projection,
            visible_display_rows,
            syntax_by_display_row,
            #[cfg(test)]
            line_number_digits: self
                .workspace_session
                .layout()?
                .documents()
                .iter()
                .map(|document| document.line_count.max(1).to_string().len())
                .max()
                .unwrap_or(1),
        })
    }

    fn workspace_buffer_for_document(
        &self,
        document_id: WorkspaceDocumentId,
    ) -> Option<&TextBuffer> {
        let layout = self.workspace_session.layout()?;
        let document = layout.document(document_id)?;
        if self.active_path() == Some(document.path()) {
            return Some(self.editor.buffer());
        }
        self.workspace_buffers.get(document.path())
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn workspace_document_display_rows(
        &self,
        path: &Path,
        tab_width: usize,
    ) -> Option<Vec<DisplayRow>> {
        let snapshot = self.workspace_document_snapshot(path)?;
        let mut editor = EditorState::new(TextBuffer::new(snapshot.buffer_id, &snapshot.text()));
        let state = self.workspace_document_view_state(path);
        editor.apply(EditorCommand::SetShowWhitespace(state.show_whitespace));
        editor.apply(EditorCommand::SetWrapWidth(state.soft_wrap.then_some(80)));
        for region in state.folded_regions {
            editor.apply(EditorCommand::FoldLines {
                start_line: region.start_line,
                end_line: region.end_line,
            });
        }
        editor.apply(EditorCommand::SetSearchQuery(self.search_query.clone()));
        editor.apply(EditorCommand::SetViewport(Viewport {
            first_visible_row: 0,
            visible_row_count: usize::MAX,
            horizontal_offset: 0,
        }));
        let _ = tab_width;
        Some(editor.display_snapshot().visible_rows)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn workspace_document_view_state(&self, path: &Path) -> FilesEditorViewState {
        if self.active_path() == Some(path) {
            return FilesEditorViewState {
                selection: self.editor.selection(),
                viewport: self.editor.viewport(),
                folded_regions: self.editor.folded_regions().to_vec(),
                soft_wrap: self.soft_wrap_enabled(),
                show_whitespace: self.show_whitespace(),
            };
        }

        self.view_state_by_path
            .get(path)
            .cloned()
            .unwrap_or_else(|| FilesEditorViewState {
                selection: Selection::caret(TextPosition::new(0, 0)),
                viewport: Viewport::default(),
                folded_regions: Vec::new(),
                soft_wrap: false,
                show_whitespace: default_show_whitespace_for_path(path),
            })
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn workspace_display_rows_from_projected_snapshot(
    projected_rows: &[hunk_editor::WorkspaceProjectedRow],
) -> Option<Vec<WorkspaceDisplayRow>> {
    projected_rows
        .iter()
        .map(|row| {
            if row
                .workspace_row_range
                .as_ref()
                .is_some_and(|workspace_row_range| workspace_row_range.is_empty())
            {
                return None;
            }

            Some(WorkspaceDisplayRow {
                row_index: row.row_index,
                location: row.location.clone(),
                raw_start_column: row.raw_start_column,
                raw_end_column: row.raw_end_column,
                raw_column_offsets: row.raw_column_offsets.clone(),
                text: row.text.clone(),
                whitespace_markers: row.whitespace_markers.clone(),
                search_highlights: row.search_highlights.clone(),
            })
        })
        .collect()
}

fn apply_workspace_search_highlights(
    layout: &WorkspaceLayout,
    visible_rows: &mut [WorkspaceDisplayRow],
    query: &str,
    document_snapshots: &BTreeMap<WorkspaceDocumentId, TextSnapshot>,
) {
    if query.trim().is_empty() {
        return;
    }

    let matches = find_workspace_search_matches(layout, query, document_snapshots);
    if matches.is_empty() {
        return;
    }

    let mut matches_by_excerpt =
        BTreeMap::<(WorkspaceDocumentId, WorkspaceExcerptId), Vec<WorkspaceSearchMatch>>::new();
    for found in matches {
        matches_by_excerpt
            .entry((found.document_id, found.excerpt_id))
            .or_default()
            .push(found);
    }

    for row in visible_rows {
        let Some(location) = row.location.as_ref() else {
            continue;
        };
        let Some(document_line) = location.document_line else {
            continue;
        };
        let Some(snapshot) = document_snapshots.get(&location.document_id) else {
            continue;
        };
        let Some(matches) = matches_by_excerpt.get(&(location.document_id, location.excerpt_id))
        else {
            continue;
        };
        row.search_highlights =
            workspace_search_highlights_for_row(row, location, document_line, matches, snapshot);
    }
}

fn workspace_search_highlights_for_row(
    row: &WorkspaceDisplayRow,
    location: &WorkspaceRowLocation,
    document_line: usize,
    matches: &[WorkspaceSearchMatch],
    snapshot: &TextSnapshot,
) -> Vec<SearchHighlight> {
    let mut highlights = Vec::new();
    for found in matches {
        if found.excerpt_id != location.excerpt_id {
            continue;
        }
        let Ok(start) = snapshot.byte_to_position(found.byte_range.start) else {
            continue;
        };
        let Ok(end) = snapshot.byte_to_position(found.byte_range.end) else {
            continue;
        };
        if document_line < start.line || document_line > end.line {
            continue;
        }

        let start_raw_column = if document_line == start.line {
            start.column
        } else {
            row.raw_start_column
        };
        let end_raw_column = if document_line == end.line {
            end.column
        } else {
            row.raw_end_column
        };
        let start_column = workspace_display_column_for_raw(row, start_raw_column);
        let end_column = workspace_display_column_for_raw(row, end_raw_column);
        if start_column < end_column {
            highlights.push(SearchHighlight {
                start_column,
                end_column,
            });
        }
    }
    highlights
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
