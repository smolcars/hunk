use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::PathBuf;

use hunk_domain::db::{CommentLineSide, compute_comment_anchor_hash};
use hunk_domain::diff::SideBySideRow;
use hunk_domain::diff::{
    DiffCellKind, DiffDocument, DiffHunk, DiffLineKind, DiffRowKind, parse_patch_document,
};
use hunk_editor::{
    WorkspaceDisplayRow, WorkspaceDocument, WorkspaceDocumentId, WorkspaceExcerptId,
    WorkspaceExcerptKind, WorkspaceExcerptSpec, WorkspaceLayout, WorkspaceLayoutError,
};
use hunk_git::compare::CompareSnapshot;
use hunk_git::git::{FileStatus, LineStats};
use hunk_text::{BufferId, TextBuffer};

#[allow(clippy::duplicate_mod)]
#[path = "review_workspace_session_search.rs"]
mod search_impl;
#[allow(unused_imports)]
pub(crate) use search_impl::ReviewWorkspaceSearchTarget;
#[allow(clippy::duplicate_mod)]
#[path = "workspace_display_buffers.rs"]
mod workspace_display_buffers;

use crate::app::data::{
    CachedStyledSegment, DiffSegmentQuality, DiffStream, DiffStreamRowKind,
    apply_search_highlights_to_cached_segments, cached_runtime_fallback_segments,
    compact_cached_segments_for_render, merge_cached_segments_with_changed_flags,
};
use crate::app::highlight::SyntaxTokenKind;
use crate::app::native_files_editor::WorkspaceEditorSession;
use crate::app::native_files_editor::paint::RowSyntaxSpan;
use crate::app::{DiffRowSegmentCache, DiffStreamRowMeta};
#[cfg(test)]
use hunk_editor::Viewport;
#[cfg(test)]
use hunk_text::TextSnapshot;
#[cfg(test)]
use workspace_display_buffers::build_workspace_display_snapshot_from_document_snapshots;

const FILE_HEADER_SURFACE_ROWS: usize = 1;
const HUNK_HEADER_SURFACE_ROWS: usize = 1;
pub(crate) const REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX: usize = 26;
pub(crate) const REVIEW_SURFACE_HUNK_DIVIDER_HEIGHT_PX: usize = 6;
const REVIEW_LINE_NUMBER_MIN_DIGITS: u32 = 3;
const REVIEW_VIEWPORT_RENDER_MAX_SEGMENTS_PER_CELL: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewCommentAnchor {
    pub(crate) file_path: String,
    pub(crate) line_side: CommentLineSide,
    pub(crate) old_line: Option<u32>,
    pub(crate) new_line: Option<u32>,
    pub(crate) hunk_header: Option<String>,
    pub(crate) line_text: String,
    pub(crate) context_before: String,
    pub(crate) context_after: String,
    pub(crate) anchor_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReviewFileAnchorReconcileState {
    Ready,
    Deferred,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceFileRange {
    pub(crate) path: String,
    pub(crate) status: FileStatus,
    pub(crate) start_row: usize,
    pub(crate) end_row: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceHunkRange {
    pub(crate) path: String,
    pub(crate) header: String,
    pub(crate) start_row: usize,
    pub(crate) end_row: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceSection {
    pub(crate) index: usize,
    pub(crate) excerpt_id: WorkspaceExcerptId,
    pub(crate) path: String,
    pub(crate) status: FileStatus,
    pub(crate) start_row: usize,
    pub(crate) end_row: usize,
    pub(crate) show_file_header: bool,
    pub(crate) hunk_header: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceVisibleFileHeader {
    pub(crate) row_index: usize,
    pub(crate) path: String,
    pub(crate) status: FileStatus,
    pub(crate) line_stats: LineStats,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspaceViewportSection {
    pub(crate) pixel_range: Range<usize>,
    pub(crate) rows: Vec<ReviewWorkspaceViewportRow>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspaceViewportRow {
    pub(crate) row_index: usize,
    pub(crate) stable_id: u64,
    pub(crate) row_kind: DiffRowKind,
    pub(crate) stream_kind: DiffStreamRowKind,
    pub(crate) file_path: Option<String>,
    pub(crate) file_status: Option<FileStatus>,
    pub(crate) file_line_stats: Option<LineStats>,
    pub(crate) file_is_collapsed: bool,
    pub(crate) can_view_file: bool,
    pub(crate) show_comment_affordance: bool,
    pub(crate) open_comment_count: usize,
    pub(crate) text: String,
    pub(crate) left_cell_kind: DiffCellKind,
    pub(crate) left_line: Option<u32>,
    pub(crate) right_cell_kind: DiffCellKind,
    pub(crate) right_line: Option<u32>,
    pub(crate) surface_top_px: usize,
    pub(crate) height_px: usize,
    pub(crate) left_segments: Vec<CachedStyledSegment>,
    pub(crate) right_segments: Vec<CachedStyledSegment>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspaceViewportSnapshot {
    pub(crate) total_surface_height_px: usize,
    pub(crate) sections: Vec<ReviewWorkspaceViewportSection>,
}

impl ReviewWorkspaceViewportSnapshot {
    pub(crate) fn visible_pixel_range(&self) -> Option<Range<usize>> {
        Some(self.sections.first()?.pixel_range.start..self.sections.last()?.pixel_range.end)
    }

    #[allow(dead_code)]
    pub(crate) fn row_by_index(&self, row_index: usize) -> Option<&ReviewWorkspaceViewportRow> {
        self.sections
            .iter()
            .flat_map(|section| section.rows.iter())
            .find(|row| row.row_index == row_index)
    }

    pub(crate) fn row_at_viewport_position(
        &self,
        viewport_origin_px: usize,
        local_y_px: usize,
    ) -> Option<&ReviewWorkspaceViewportRow> {
        let surface_y_px = viewport_origin_px.saturating_add(local_y_px);
        self.sections
            .iter()
            .flat_map(|section| section.rows.iter())
            .find(|row| {
                let top_px = row.surface_top_px;
                let bottom_px = top_px.saturating_add(row.height_px);
                surface_y_px >= top_px && surface_y_px < bottom_px
            })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspaceSurfaceSnapshot {
    pub(crate) scroll_top_px: usize,
    pub(crate) viewport_height_px: usize,
    pub(crate) viewport: ReviewWorkspaceViewportSnapshot,
    pub(crate) sticky_file_header: Option<ReviewWorkspaceVisibleFileHeader>,
    pub(crate) active_comment_editor_overlay: Option<ReviewWorkspaceFloatingOverlay>,
    pub(crate) visible_state: ReviewWorkspaceVisibleState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceSurfaceOptions {
    pub(crate) comment_affordance_rows: BTreeSet<usize>,
    pub(crate) comment_open_counts_by_row: BTreeMap<usize, usize>,
    pub(crate) active_comment_editor_row: Option<usize>,
    pub(crate) collapsed_paths: BTreeSet<String>,
    pub(crate) view_file_enabled_paths: BTreeSet<String>,
    pub(crate) search_highlight_columns_by_row: BTreeMap<usize, Vec<Range<usize>>>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ReviewWorkspaceDisplayRows {
    pub(crate) left_by_row: BTreeMap<usize, WorkspaceDisplayRow>,
    pub(crate) right_by_row: BTreeMap<usize, WorkspaceDisplayRow>,
    pub(crate) left_syntax_by_row: BTreeMap<usize, Vec<RowSyntaxSpan>>,
    pub(crate) right_syntax_by_row: BTreeMap<usize, Vec<RowSyntaxSpan>>,
}

impl ReviewWorkspaceDisplayRows {
    pub(crate) fn covers_row_range(&self, row_range: Range<usize>) -> bool {
        row_range
            .clone()
            .all(|row| self.left_by_row.contains_key(&row) && self.right_by_row.contains_key(&row))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceFloatingOverlay {
    pub(crate) row_index: usize,
    pub(crate) top_px: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceVisibleState {
    pub(crate) visible_row_range: Option<Range<usize>>,
    pub(crate) top_row: Option<usize>,
    pub(crate) visible_file_header_row: Option<usize>,
    pub(crate) visible_hunk_header_row: Option<usize>,
    pub(crate) visible_file_path: Option<String>,
    pub(crate) visible_file_status: Option<FileStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceSegmentPrefetchRow {
    pub(crate) row_index: usize,
    pub(crate) file_path: Option<String>,
    pub(crate) left_text: String,
    pub(crate) left_kind: DiffCellKind,
    pub(crate) right_text: String,
    pub(crate) right_kind: DiffCellKind,
    pub(crate) quality: DiffSegmentQuality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReviewWorkspaceSegmentPrefetchRequest {
    pub(crate) scroll_top_px: usize,
    pub(crate) viewport_height_px: usize,
    pub(crate) anchor_row: usize,
    pub(crate) overscan_rows: usize,
    pub(crate) force_upgrade: bool,
    pub(crate) recently_scrolling: bool,
    pub(crate) batch_limit: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspaceSession {
    layout: WorkspaceLayout,
    file_line_stats: BTreeMap<String, LineStats>,
    file_ranges: Vec<ReviewWorkspaceFileRange>,
    hunk_ranges: Vec<ReviewWorkspaceHunkRange>,
    sections: Vec<ReviewWorkspaceSection>,
    left_document_buffers: BTreeMap<WorkspaceDocumentId, TextBuffer>,
    right_document_buffers: BTreeMap<WorkspaceDocumentId, TextBuffer>,
    rows: Vec<SideBySideRow>,
    row_metadata: Vec<DiffStreamRowMeta>,
    row_segments: Vec<Option<DiffRowSegmentCache>>,
    row_top_offsets: Vec<usize>,
    section_pixel_ranges: Vec<Range<usize>>,
    total_surface_height_px: usize,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReviewWorkspaceEditorSide {
    Left,
    Right,
}

impl ReviewWorkspaceSession {
    pub(crate) fn from_compare_snapshot(
        snapshot: &CompareSnapshot,
        collapsed_files: &BTreeSet<String>,
    ) -> Result<Self, WorkspaceLayoutError> {
        let mut next_document_id = 1_u64;
        let mut next_excerpt_id = 1_u64;
        let mut documents = Vec::with_capacity(snapshot.files.len());
        let mut excerpt_specs = Vec::new();
        let mut excerpt_headers = BTreeMap::new();
        let mut file_plans = Vec::with_capacity(snapshot.files.len());
        let mut hunk_ranges = Vec::new();
        let mut next_surface_row = 0_usize;

        for file in &snapshot.files {
            let patch = snapshot
                .patches_by_path
                .get(file.path.as_str())
                .map(String::as_str)
                .unwrap_or_default();
            let document = parse_patch_document(patch);
            let document_id = WorkspaceDocumentId::new(next_document_id);
            next_document_id = next_document_id.saturating_add(1);

            let line_count = review_document_line_count(&document);
            documents.push(WorkspaceDocument::new(
                document_id,
                file.path.clone(),
                BufferId::new(document_id.get()),
                line_count,
            ));

            if document.hunks.is_empty() {
                let excerpt_id = WorkspaceExcerptId::new(next_excerpt_id);
                next_excerpt_id = next_excerpt_id.saturating_add(1);
                excerpt_specs.push(
                    WorkspaceExcerptSpec::new(
                        excerpt_id,
                        document_id,
                        WorkspaceExcerptKind::DiffHunk,
                        0..line_count.max(1),
                    )
                    .with_chrome_rows(FILE_HEADER_SURFACE_ROWS, 0),
                );
                excerpt_headers.insert(excerpt_id, None);
            } else {
                for (hunk_ix, hunk) in document.hunks.iter().enumerate() {
                    let excerpt_id = WorkspaceExcerptId::new(next_excerpt_id);
                    next_excerpt_id = next_excerpt_id.saturating_add(1);
                    excerpt_specs.push(
                        WorkspaceExcerptSpec::new(
                            excerpt_id,
                            document_id,
                            WorkspaceExcerptKind::DiffHunk,
                            review_hunk_line_range(hunk, line_count),
                        )
                        .with_chrome_rows(
                            usize::from(hunk_ix == 0).saturating_add(HUNK_HEADER_SURFACE_ROWS),
                            hunk.trailing_meta.len(),
                        ),
                    );
                    excerpt_headers.insert(excerpt_id, Some(hunk.header.clone()));
                }
            }

            let start_row = next_surface_row;
            if collapsed_files.contains(file.path.as_str()) || document.hunks.is_empty() {
                next_surface_row = next_surface_row.saturating_add(2);
            } else {
                let mut next_hunk_surface_row =
                    next_surface_row.saturating_add(FILE_HEADER_SURFACE_ROWS);
                for hunk in &document.hunks {
                    let hunk_row_count = surface_row_count_for_hunk(hunk);
                    hunk_ranges.push(ReviewWorkspaceHunkRange {
                        path: file.path.clone(),
                        header: hunk.header.clone(),
                        start_row: next_hunk_surface_row,
                        end_row: next_hunk_surface_row.saturating_add(hunk_row_count),
                    });
                    next_hunk_surface_row = next_hunk_surface_row.saturating_add(hunk_row_count);
                }
                next_surface_row = next_hunk_surface_row;
            }

            file_plans.push((file.path.clone(), file.status, start_row..next_surface_row));
        }

        let layout = WorkspaceLayout::new(documents, excerpt_specs, 0)?;
        let mut file_ranges = Vec::with_capacity(file_plans.len());
        let file_status_by_path = snapshot
            .files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect::<BTreeMap<_, _>>();

        for (path, status, surface_row_range) in file_plans {
            file_ranges.push(ReviewWorkspaceFileRange {
                path,
                status,
                start_row: surface_row_range.start,
                end_row: surface_row_range.end,
            });
        }

        let mut sections = Vec::with_capacity(layout.excerpts().len());
        let mut first_excerpt_by_document = BTreeSet::new();
        for (section_ix, excerpt) in layout.excerpts().iter().enumerate() {
            let Some(document) = layout.document(excerpt.spec.document_id) else {
                continue;
            };
            let path = document.path.to_string_lossy().to_string();
            let Some(status) = file_status_by_path.get(path.as_str()).copied() else {
                continue;
            };
            sections.push(ReviewWorkspaceSection {
                index: section_ix,
                excerpt_id: excerpt.spec.id,
                path,
                status,
                start_row: excerpt.global_row_range.start,
                end_row: excerpt.global_row_range.end,
                show_file_header: first_excerpt_by_document.insert(document.id),
                hunk_header: excerpt_headers.get(&excerpt.spec.id).cloned().flatten(),
            });
        }

        Ok(Self {
            layout,
            file_line_stats: snapshot.file_line_stats.clone(),
            file_ranges,
            hunk_ranges,
            sections,
            left_document_buffers: BTreeMap::new(),
            right_document_buffers: BTreeMap::new(),
            rows: Vec::new(),
            row_metadata: Vec::new(),
            row_segments: Vec::new(),
            row_top_offsets: Vec::new(),
            section_pixel_ranges: Vec::new(),
            total_surface_height_px: 0,
        })
    }

    pub(crate) fn with_render_stream(mut self, stream: &DiffStream) -> Self {
        debug_assert_eq!(self.layout.total_rows(), stream.rows.len());
        debug_assert_eq!(stream.rows.len(), stream.row_metadata.len());
        debug_assert_eq!(stream.rows.len(), stream.row_segments.len());
        self.rows = stream.rows.clone();
        self.row_metadata = stream.row_metadata.clone();
        self.row_segments = stream.row_segments.clone();
        self.rebuild_document_buffers();
        self.rebuild_surface_geometry();
        self
    }

    pub(crate) fn file_ranges(&self) -> &[ReviewWorkspaceFileRange] {
        &self.file_ranges
    }

    pub(crate) fn file_line_stats(&self) -> &BTreeMap<String, LineStats> {
        &self.file_line_stats
    }

    pub(crate) fn file_range_for_path(&self, path: &str) -> Option<&ReviewWorkspaceFileRange> {
        self.file_ranges.iter().find(|range| range.path == path)
    }

    pub(crate) fn first_file(&self) -> Option<&ReviewWorkspaceFileRange> {
        self.file_ranges.first()
    }

    pub(crate) fn first_path(&self) -> Option<&str> {
        self.first_file().map(|range| range.path.as_str())
    }

    pub(crate) fn contains_path(&self, path: &str) -> bool {
        self.file_range_for_path(path).is_some()
    }

    pub(crate) fn path_at_surface_row(&self, row: usize) -> Option<&str> {
        self.file_ranges
            .iter()
            .find(|range| range.start_row <= row && row < range.end_row)
            .map(|range| range.path.as_str())
    }

    pub(crate) fn excerpt_id_at_surface_row(&self, row: usize) -> Option<WorkspaceExcerptId> {
        self.layout
            .excerpt_at_row(row)
            .map(|excerpt| excerpt.spec.id)
    }

    pub(crate) fn file_at_or_after_surface_row(
        &self,
        row: usize,
    ) -> Option<&ReviewWorkspaceFileRange> {
        self.file_ranges
            .iter()
            .find(|range| row < range.end_row)
            .or_else(|| self.file_ranges.last())
    }

    pub(crate) fn adjacent_file(
        &self,
        current_path: Option<&str>,
        direction: isize,
    ) -> Option<&ReviewWorkspaceFileRange> {
        let current_ix = current_path
            .and_then(|path| {
                self.file_ranges
                    .iter()
                    .position(|candidate| candidate.path == path)
            })
            .unwrap_or(0);
        let max_ix = self.file_ranges.len().saturating_sub(1) as isize;
        let target_ix = (current_ix as isize + direction).clamp(0, max_ix) as usize;
        self.file_ranges.get(target_ix)
    }

    pub(crate) fn status_for_path(&self, path: &str) -> Option<FileStatus> {
        self.file_range_for_path(path).map(|range| range.status)
    }

    pub(crate) fn visible_file_header_at_surface_row(
        &self,
        row: usize,
    ) -> Option<ReviewWorkspaceVisibleFileHeader> {
        let header_row = self.visible_file_header_row(row)?;
        let file_range = self
            .file_ranges
            .iter()
            .find(|range| range.start_row == header_row)?;
        Some(ReviewWorkspaceVisibleFileHeader {
            row_index: file_range.start_row,
            path: file_range.path.clone(),
            status: file_range.status,
            line_stats: self
                .file_line_stats
                .get(file_range.path.as_str())
                .copied()
                .unwrap_or_default(),
        })
    }

    pub(crate) fn line_number_digit_widths(&self) -> (u32, u32) {
        let mut max_left_digits = REVIEW_LINE_NUMBER_MIN_DIGITS;
        let mut max_right_digits = REVIEW_LINE_NUMBER_MIN_DIGITS;

        for row in &self.rows {
            if row.kind != DiffRowKind::Code {
                continue;
            }
            if let Some(line) = row.left.line {
                max_left_digits = max_left_digits.max(review_decimal_digits(line));
            }
            if let Some(line) = row.right.line {
                max_right_digits = max_right_digits.max(review_decimal_digits(line));
            }
        }

        (max_left_digits, max_right_digits)
    }

    pub(crate) fn visible_file_header_row(&self, row: usize) -> Option<usize> {
        self.file_ranges
            .iter()
            .find(|range| range.start_row <= row && row < range.end_row)
            .map(|range| range.start_row)
    }

    pub(crate) fn hunk_ranges(&self) -> &[ReviewWorkspaceHunkRange] {
        &self.hunk_ranges
    }

    pub(crate) fn section(&self, section_ix: usize) -> Option<&ReviewWorkspaceSection> {
        self.sections.get(section_ix)
    }

    pub(crate) fn section_pixel_range(&self, section_ix: usize) -> Option<&Range<usize>> {
        self.section_pixel_ranges.get(section_ix)
    }

    pub(crate) fn total_surface_height_px(&self) -> usize {
        self.total_surface_height_px
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn build_viewport_snapshot(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_sections: usize,
        overscan_rows: usize,
        options: &ReviewWorkspaceSurfaceOptions,
    ) -> ReviewWorkspaceViewportSnapshot {
        let display_rows = self.build_display_rows_for_viewport_projection(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
            overscan_rows,
        );
        self.build_viewport_snapshot_with_display_rows(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
            overscan_rows,
            options,
            &display_rows,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_viewport_snapshot_with_display_rows(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_sections: usize,
        overscan_rows: usize,
        options: &ReviewWorkspaceSurfaceOptions,
        display_rows: &ReviewWorkspaceDisplayRows,
    ) -> ReviewWorkspaceViewportSnapshot {
        let mut sections = Vec::new();
        for section_ix in self.visible_section_range_for_viewport(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
        ) {
            let Some(section) = self.section(section_ix) else {
                continue;
            };
            let Some(pixel_range) = self.section_pixel_range(section_ix).cloned() else {
                continue;
            };
            let visible_row_range = self
                .section_visible_row_range(
                    section_ix,
                    scroll_top_px,
                    viewport_height_px,
                    overscan_rows,
                )
                .unwrap_or(section.start_row..section.end_row);
            debug_assert!(display_rows.covers_row_range(visible_row_range.clone()));
            let left_display_rows = self
                .build_display_rows_for_side(visible_row_range.clone(), &display_rows.left_by_row);
            let right_display_rows = self
                .build_display_rows_for_side(visible_row_range.clone(), &display_rows.right_by_row);
            let _top_spacer_height_px = self
                .row_boundary_offset_px(visible_row_range.start)
                .unwrap_or(pixel_range.start)
                .saturating_sub(pixel_range.start);
            let _bottom_spacer_height_px = pixel_range.end.saturating_sub(
                self.row_boundary_offset_px(visible_row_range.end)
                    .unwrap_or(pixel_range.end),
            );
            let rows = visible_row_range
                .clone()
                .zip(left_display_rows.into_iter())
                .zip(right_display_rows.into_iter())
                .filter_map(|((row_index, left_display_row), right_display_row)| {
                    let visible_start_px = self
                        .row_boundary_offset_px(visible_row_range.start)
                        .unwrap_or(pixel_range.start);
                    let row = self.row(row_index)?;
                    let row_metadata = self.row_metadata(row_index);
                    let file_path = row_metadata
                        .and_then(|meta| meta.file_path.clone())
                        .or_else(|| self.row_file_path(row_index).map(ToString::to_string));
                    let file_status =
                        row_metadata.and_then(|meta| meta.file_status).or_else(|| {
                            file_path
                                .as_deref()
                                .and_then(|path| self.status_for_path(path))
                        });
                    let file_is_collapsed = file_path
                        .as_deref()
                        .is_some_and(|path| options.collapsed_paths.contains(path));
                    let can_view_file = file_path
                        .as_deref()
                        .is_some_and(|path| options.view_file_enabled_paths.contains(path));
                    let row_segment_cache = self.row_segment_cache(row_index);
                    let surface_top_px = self
                        .row_top_offset_px(row_index)
                        .unwrap_or(visible_start_px);
                    let right_search_highlights = options
                        .search_highlight_columns_by_row
                        .get(&row_index)
                        .map(|ranges| {
                            review_project_search_highlights_for_display_row(
                                &right_display_row,
                                ranges,
                            )
                        })
                        .unwrap_or_default();
                    Some(ReviewWorkspaceViewportRow {
                        row_index,
                        stable_id: row_metadata
                            .map(|meta| meta.stable_id)
                            .unwrap_or(row_index as u64),
                        row_kind: row.kind,
                        stream_kind: row_metadata
                            .map(|meta| meta.kind)
                            .unwrap_or_else(|| review_stream_row_kind_for_row(row.kind)),
                        file_line_stats: file_path
                            .as_deref()
                            .and_then(|path| self.file_line_stats.get(path).copied()),
                        file_path,
                        file_status,
                        file_is_collapsed,
                        can_view_file,
                        show_comment_affordance: options
                            .comment_affordance_rows
                            .contains(&row_index),
                        open_comment_count: options
                            .comment_open_counts_by_row
                            .get(&row_index)
                            .copied()
                            .unwrap_or_default(),
                        text: row.text.clone(),
                        left_cell_kind: row.left.kind,
                        left_line: row.left.line,
                        right_cell_kind: row.right.kind,
                        right_line: row.right.line,
                        surface_top_px,
                        height_px: self.surface_row_height_px(row_index),
                        left_segments: review_viewport_render_segments(
                            display_rows.left_syntax_by_row.get(&row_index),
                            row_segment_cache.map(|cache| &cache.left),
                            left_display_row.text.as_str(),
                            &[],
                        ),
                        right_segments: review_viewport_render_segments(
                            display_rows.right_syntax_by_row.get(&row_index),
                            row_segment_cache.map(|cache| &cache.right),
                            right_display_row.text.as_str(),
                            right_search_highlights.as_slice(),
                        ),
                    })
                })
                .collect();
            sections.push(ReviewWorkspaceViewportSection { pixel_range, rows });
        }

        ReviewWorkspaceViewportSnapshot {
            total_surface_height_px: self.total_surface_height_px(),
            sections,
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn build_surface_snapshot(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_sections: usize,
        overscan_rows: usize,
        options: &ReviewWorkspaceSurfaceOptions,
    ) -> ReviewWorkspaceSurfaceSnapshot {
        let display_rows = self.build_display_rows_for_viewport_projection(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
            overscan_rows,
        );
        self.build_surface_snapshot_with_display_rows(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
            overscan_rows,
            options,
            &display_rows,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_surface_snapshot_with_display_rows(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_sections: usize,
        overscan_rows: usize,
        options: &ReviewWorkspaceSurfaceOptions,
        display_rows: &ReviewWorkspaceDisplayRows,
    ) -> ReviewWorkspaceSurfaceSnapshot {
        let viewport = self.build_viewport_snapshot_with_display_rows(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
            overscan_rows,
            options,
            display_rows,
        );
        let visible_state = self.build_visible_state(scroll_top_px, viewport_height_px);
        let sticky_file_header = visible_state.top_row.and_then(|top_row| {
            let header = self.visible_file_header_at_surface_row(top_row)?;
            (header.row_index != top_row).then_some(header)
        });
        let active_comment_editor_overlay =
            options.active_comment_editor_row.and_then(|row_index| {
                let row_top_px = self.row_top_offset_px(row_index)?;
                let top_px = crate::app::comment_overlay::review_comment_overlay_top_px(
                    row_top_px,
                    scroll_top_px,
                    viewport_height_px,
                    REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX,
                )
                .round() as usize;
                Some(ReviewWorkspaceFloatingOverlay { row_index, top_px })
            });

        ReviewWorkspaceSurfaceSnapshot {
            scroll_top_px,
            viewport_height_px,
            viewport,
            sticky_file_header,
            active_comment_editor_overlay,
            visible_state,
        }
    }

    pub(crate) fn viewport_row_indices(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_sections: usize,
        overscan_rows: usize,
    ) -> Vec<usize> {
        let mut row_indices = Vec::new();
        for section_ix in self.visible_section_range_for_viewport(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
        ) {
            let Some(section) = self.section(section_ix) else {
                continue;
            };
            let visible_row_range = self
                .section_visible_row_range(
                    section_ix,
                    scroll_top_px,
                    viewport_height_px,
                    overscan_rows,
                )
                .unwrap_or(section.start_row..section.end_row);
            row_indices.extend(visible_row_range);
        }
        row_indices
    }

    pub(crate) fn build_segment_prefetch_rows(
        &self,
        request: ReviewWorkspaceSegmentPrefetchRequest,
    ) -> Vec<ReviewWorkspaceSegmentPrefetchRow> {
        let prioritized_rows = prioritized_prefetch_row_indices_for_rows(
            self.viewport_row_indices(
                request.scroll_top_px,
                request.viewport_height_px,
                1,
                request.overscan_rows,
            ),
            request.anchor_row,
        );
        let max_rows = if request.force_upgrade {
            prioritized_rows.len()
        } else {
            request.batch_limit.min(prioritized_rows.len())
        };

        let mut pending_rows = Vec::with_capacity(max_rows);
        for row_ix in prioritized_rows {
            if pending_rows.len() >= max_rows {
                break;
            }

            let Some(row) = self.row(row_ix) else {
                continue;
            };
            if row.kind != DiffRowKind::Code {
                continue;
            }

            let file_path = self.row_file_path(row_ix).map(ToString::to_string);
            let base_quality = file_path
                .as_deref()
                .and_then(|path| self.file_line_stats.get(path).copied())
                .map(review_base_segment_quality_for_file)
                .unwrap_or(DiffSegmentQuality::Detailed);
            let target_quality =
                review_effective_segment_quality(base_quality, request.recently_scrolling);

            if self
                .row_segment_cache(row_ix)
                .is_some_and(|cache| cache.quality >= target_quality)
            {
                continue;
            }

            pending_rows.push(ReviewWorkspaceSegmentPrefetchRow {
                row_index: row_ix,
                file_path,
                left_text: row.left.text.clone(),
                left_kind: row.left.kind,
                right_text: row.right.text.clone(),
                right_kind: row.right.kind,
                quality: target_quality,
            });
        }

        pending_rows
    }

    pub(crate) fn build_visible_state(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
    ) -> ReviewWorkspaceVisibleState {
        let visible_row_range =
            self.visible_row_range_for_viewport(scroll_top_px, viewport_height_px);
        let top_row = visible_row_range.as_ref().map(|range| range.start);
        let visible_file_header_row = visible_row_range.as_ref().and_then(|range| {
            self.file_ranges
                .iter()
                .find(|file| file.start_row < range.end && range.start < file.end_row)
                .map(|file| file.start_row)
        });
        let visible_hunk_header_row = visible_row_range.as_ref().and_then(|range| {
            self.hunk_ranges
                .iter()
                .find(|hunk| hunk.start_row < range.end && range.start < hunk.end_row)
                .map(|hunk| hunk.start_row)
        });
        let visible_file_path = top_row
            .and_then(|row| self.path_at_surface_row(row))
            .map(ToString::to_string);
        let visible_file_status = visible_file_path
            .as_deref()
            .and_then(|path| self.status_for_path(path));

        ReviewWorkspaceVisibleState {
            visible_file_header_row,
            visible_hunk_header_row,
            visible_file_path,
            visible_file_status,
            visible_row_range,
            top_row,
        }
    }

    pub(crate) fn visible_hunk_header_row(&self, row: usize) -> Option<usize> {
        self.hunk_ranges
            .iter()
            .find(|range| range.start_row <= row && row < range.end_row)
            .map(|range| range.start_row)
    }

    pub(crate) fn hunk_header_at_surface_row(&self, row: usize) -> Option<&str> {
        let header_row = self.visible_hunk_header_row(row)?;
        self.hunk_ranges
            .iter()
            .find(|range| range.start_row == header_row)
            .map(|range| range.header.as_str())
    }

    pub(crate) fn row_count(&self) -> usize {
        self.layout.total_rows()
    }

    pub(crate) fn row_top_offset_px(&self, row_ix: usize) -> Option<usize> {
        self.row_top_offsets.get(row_ix).copied()
    }

    pub(crate) fn row_boundary_offset_px(&self, boundary_ix: usize) -> Option<usize> {
        self.row_top_offsets.get(boundary_ix).copied()
    }

    pub(crate) fn visible_row_range_for_viewport(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
    ) -> Option<Range<usize>> {
        let row_count = self.row_count();
        if row_count == 0 || self.row_top_offsets.is_empty() {
            return None;
        }

        let start = self.row_index_for_pixel(scroll_top_px);
        let viewport_bottom = scroll_top_px
            .saturating_add(viewport_height_px.max(REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX));
        let end = self
            .row_index_for_pixel(viewport_bottom.saturating_sub(1))
            .saturating_add(1)
            .min(row_count);
        Some(
            start.min(row_count.saturating_sub(1))..end.max(start.saturating_add(1)).min(row_count),
        )
    }

    pub(crate) fn visible_section_range_for_viewport(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_sections: usize,
    ) -> Range<usize> {
        if self.section_pixel_ranges.is_empty() {
            return 0..0;
        }

        let viewport_bottom = scroll_top_px
            .saturating_add(viewport_height_px.max(REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX));
        let first_visible = self
            .section_pixel_ranges
            .partition_point(|range| range.end <= scroll_top_px)
            .min(self.section_pixel_ranges.len().saturating_sub(1));
        let last_visible_exclusive = self
            .section_pixel_ranges
            .partition_point(|range| range.start < viewport_bottom)
            .max(first_visible.saturating_add(1))
            .min(self.section_pixel_ranges.len());

        first_visible.saturating_sub(overscan_sections)
            ..last_visible_exclusive
                .saturating_add(overscan_sections)
                .min(self.section_pixel_ranges.len())
    }

    pub(crate) fn section_visible_row_range(
        &self,
        section_ix: usize,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_rows: usize,
    ) -> Option<Range<usize>> {
        let section = self.section(section_ix)?;
        let overscan_rows = overscan_rows.max(1);
        let visible = self.visible_row_range_for_viewport(scroll_top_px, viewport_height_px)?;

        if visible.end <= section.start_row {
            let end = section
                .start_row
                .saturating_add(overscan_rows)
                .min(section.end_row);
            return Some(section.start_row..end.max(section.start_row.saturating_add(1)));
        }

        if section.end_row <= visible.start {
            let start = section
                .end_row
                .saturating_sub(overscan_rows)
                .max(section.start_row);
            return Some(start..section.end_row);
        }

        let start = visible
            .start
            .saturating_sub(overscan_rows)
            .max(section.start_row);
        let end = visible
            .end
            .saturating_add(overscan_rows)
            .min(section.end_row);
        Some(start..end.max(start.saturating_add(1)).min(section.end_row))
    }

    pub(crate) fn row(&self, row_ix: usize) -> Option<&SideBySideRow> {
        if row_ix >= self.layout.total_rows() {
            return None;
        }
        self.rows.get(row_ix)
    }

    pub(crate) fn row_metadata(&self, row_ix: usize) -> Option<&DiffStreamRowMeta> {
        if row_ix >= self.layout.total_rows() {
            return None;
        }
        self.row_metadata.get(row_ix)
    }

    pub(crate) fn row_segment_cache(&self, row_ix: usize) -> Option<&DiffRowSegmentCache> {
        if row_ix >= self.layout.total_rows() {
            return None;
        }
        self.row_segments.get(row_ix).and_then(Option::as_ref)
    }

    pub(crate) fn set_row_segment_cache_if_better(
        &mut self,
        row_ix: usize,
        row_cache: DiffRowSegmentCache,
    ) -> bool {
        let Some(slot) = self.row_segments.get_mut(row_ix) else {
            return false;
        };
        let should_replace = slot
            .as_ref()
            .map(|cached| row_cache.quality > cached.quality)
            .unwrap_or(true);
        if should_replace {
            *slot = Some(row_cache);
            return true;
        }
        false
    }

    pub(crate) fn layout(&self) -> &WorkspaceLayout {
        &self.layout
    }

    #[allow(dead_code)]
    pub(crate) fn build_editor_session(
        &self,
        preferred_path: Option<&str>,
    ) -> WorkspaceEditorSession {
        let mut session = WorkspaceEditorSession::new();
        session.open_workspace_layout(
            self.layout.clone(),
            preferred_path.map(std::path::Path::new),
        );
        session
    }

    #[allow(dead_code)]
    pub(crate) fn editor_documents(
        &self,
        side: ReviewWorkspaceEditorSide,
    ) -> Vec<(PathBuf, String)> {
        let buffers = match side {
            ReviewWorkspaceEditorSide::Left => &self.left_document_buffers,
            ReviewWorkspaceEditorSide::Right => &self.right_document_buffers,
        };
        self.layout
            .documents()
            .iter()
            .map(|document| {
                let text = buffers
                    .get(&document.id)
                    .map(TextBuffer::text)
                    .unwrap_or_else(|| blank_workspace_document_text(document.line_count.max(1)));
                (document.path.clone(), text)
            })
            .collect()
    }

    pub(crate) fn file_anchor_reconcile_state(
        &self,
        file_path: &str,
        patch_loading: bool,
    ) -> ReviewFileAnchorReconcileState {
        let mut has_anchor_rows = false;
        let mut saw_rows_for_file = false;

        for row in &self.row_metadata {
            if row.file_path.as_deref() != Some(file_path) {
                continue;
            }
            saw_rows_for_file = true;
            match row.kind {
                DiffStreamRowKind::CoreCode
                | DiffStreamRowKind::CoreHunkHeader
                | DiffStreamRowKind::CoreMeta
                | DiffStreamRowKind::CoreEmpty => {
                    has_anchor_rows = true;
                }
                DiffStreamRowKind::FileLoading | DiffStreamRowKind::FileCollapsed => {
                    return ReviewFileAnchorReconcileState::Deferred;
                }
                DiffStreamRowKind::FileError => {
                    return ReviewFileAnchorReconcileState::Unavailable;
                }
                DiffStreamRowKind::FileHeader | DiffStreamRowKind::EmptyState => {}
            }
        }

        if has_anchor_rows {
            ReviewFileAnchorReconcileState::Ready
        } else if patch_loading || saw_rows_for_file {
            ReviewFileAnchorReconcileState::Deferred
        } else {
            ReviewFileAnchorReconcileState::Unavailable
        }
    }

    pub(crate) fn row_supports_comments(&self, row_ix: usize) -> bool {
        let Some(row) = self.row(row_ix) else {
            return false;
        };
        if !matches!(
            row.kind,
            DiffRowKind::Code | DiffRowKind::Meta | DiffRowKind::Empty
        ) {
            return false;
        }

        self.row_metadata(row_ix).is_some_and(|meta| {
            matches!(
                meta.kind,
                DiffStreamRowKind::CoreCode
                    | DiffStreamRowKind::CoreMeta
                    | DiffStreamRowKind::CoreEmpty
            )
        })
    }

    pub(crate) fn row_file_path(&self, row_ix: usize) -> Option<&str> {
        self.row_metadata(row_ix)
            .and_then(|meta| meta.file_path.as_deref())
            .or_else(|| self.path_at_surface_row(row_ix))
    }

    pub(crate) fn row_hunk_header(&self, row_ix: usize) -> Option<&str> {
        self.hunk_header_at_surface_row(row_ix)
    }

    pub(crate) fn build_comment_anchor(
        &self,
        row_ix: usize,
        context_radius_rows: usize,
    ) -> Option<ReviewCommentAnchor> {
        if !self.row_supports_comments(row_ix) {
            return None;
        }

        let row = self.row(row_ix)?;
        let file_path = self.row_file_path(row_ix)?.to_string();
        let hunk_header = self.row_hunk_header(row_ix).map(ToString::to_string);
        let line_text = Self::row_diff_lines(row).join("\n");

        let (line_side, old_line, new_line) = if row.kind == DiffRowKind::Code {
            if row.right.kind != DiffCellKind::None {
                (CommentLineSide::Right, row.left.line, row.right.line)
            } else if row.left.kind != DiffCellKind::None {
                (CommentLineSide::Left, row.left.line, row.right.line)
            } else {
                (CommentLineSide::Meta, None, None)
            }
        } else {
            (CommentLineSide::Meta, None, None)
        };

        let context_before = self.collect_row_context(row_ix, true, context_radius_rows);
        let context_after = self.collect_row_context(row_ix, false, context_radius_rows);
        let anchor_hash = compute_comment_anchor_hash(
            file_path.as_str(),
            hunk_header.as_deref(),
            line_text.as_str(),
            context_before.as_str(),
            context_after.as_str(),
        );

        Some(ReviewCommentAnchor {
            file_path,
            line_side,
            old_line,
            new_line,
            hunk_header,
            line_text,
            context_before,
            context_after,
            anchor_hash,
        })
    }

    pub(crate) fn build_comment_anchor_index(
        &self,
        context_radius_rows: usize,
    ) -> (
        BTreeMap<usize, ReviewCommentAnchor>,
        BTreeMap<String, Vec<usize>>,
    ) {
        let mut row_anchor_index = BTreeMap::new();
        let mut rows_by_path = BTreeMap::<String, Vec<usize>>::new();

        for row_ix in 0..self.row_count() {
            let Some(anchor) = self.build_comment_anchor(row_ix, context_radius_rows) else {
                continue;
            };
            rows_by_path
                .entry(anchor.file_path.clone())
                .or_default()
                .push(row_ix);
            row_anchor_index.insert(row_ix, anchor);
        }

        (row_anchor_index, rows_by_path)
    }

    fn collect_row_context(
        &self,
        row_ix: usize,
        before: bool,
        context_radius_rows: usize,
    ) -> String {
        let row_count = self.row_count();
        if row_count == 0 {
            return String::new();
        }

        let anchor_path = self.row_file_path(row_ix).map(ToString::to_string);
        let range = if before {
            let start = row_ix.saturating_sub(context_radius_rows);
            start..row_ix
        } else {
            let start = row_ix.saturating_add(1);
            let end = start.saturating_add(context_radius_rows).min(row_count);
            start..end
        };

        let mut lines = Vec::new();
        for ix in range {
            let Some(row) = self.row(ix) else {
                continue;
            };
            if anchor_path.is_some() && self.row_file_path(ix) != anchor_path.as_deref() {
                continue;
            }
            lines.extend(Self::row_diff_lines(row));
        }
        lines.join("\n")
    }

    fn row_diff_lines(row: &SideBySideRow) -> Vec<String> {
        let mut lines = Vec::new();
        match row.kind {
            DiffRowKind::Code => {
                if row.left.kind == DiffCellKind::Removed {
                    lines.push(format!("-{}", row.left.text));
                }
                if row.right.kind == DiffCellKind::Added {
                    lines.push(format!("+{}", row.right.text));
                }
                if row.left.kind == DiffCellKind::Context {
                    lines.push(format!(" {}", row.left.text));
                }
                if row.left.kind == DiffCellKind::None
                    && row.right.kind == DiffCellKind::None
                    && !row.text.is_empty()
                {
                    lines.push(row.text.clone());
                }
            }
            DiffRowKind::HunkHeader => {}
            DiffRowKind::Meta | DiffRowKind::Empty => {
                lines.push(row.text.clone());
            }
        }
        lines
    }

    fn rebuild_surface_geometry(&mut self) {
        let row_count = self.row_count();
        self.row_top_offsets = Vec::with_capacity(row_count.saturating_add(1));
        self.row_top_offsets.push(0);
        let mut next_offset = 0usize;
        for row_ix in 0..row_count {
            next_offset = next_offset.saturating_add(self.surface_row_height_px(row_ix));
            self.row_top_offsets.push(next_offset);
        }
        self.total_surface_height_px = next_offset;
        self.section_pixel_ranges = self
            .sections
            .iter()
            .map(|section| {
                let start = self
                    .row_top_offsets
                    .get(section.start_row)
                    .copied()
                    .unwrap_or(0);
                let end = self
                    .row_top_offsets
                    .get(section.end_row)
                    .copied()
                    .unwrap_or(start);
                start..end
            })
            .collect();
    }

    fn rebuild_document_buffers(&mut self) {
        let mut left_document_lines = self
            .layout
            .documents()
            .iter()
            .map(|document| (document.id, vec![String::new(); document.line_count.max(1)]))
            .collect::<BTreeMap<_, _>>();
        let mut right_document_lines = self
            .layout
            .documents()
            .iter()
            .map(|document| (document.id, vec![String::new(); document.line_count.max(1)]))
            .collect::<BTreeMap<_, _>>();

        for row_ix in 0..self.layout.total_rows().min(self.rows.len()) {
            let Some(location) = self.layout.locate_row(row_ix) else {
                continue;
            };
            let Some(document_line) = location.document_line else {
                continue;
            };
            let Some(row) = self.rows.get(row_ix) else {
                continue;
            };
            if let Some(lines) = left_document_lines.get_mut(&location.document_id)
                && let Some(slot) = lines.get_mut(document_line)
            {
                *slot = row.left.text.clone();
            }
            if let Some(lines) = right_document_lines.get_mut(&location.document_id)
                && let Some(slot) = lines.get_mut(document_line)
            {
                *slot = row.right.text.clone();
            }
        }

        self.left_document_buffers = self.build_document_buffers_from_lines(&left_document_lines);
        self.right_document_buffers = self.build_document_buffers_from_lines(&right_document_lines);
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn build_display_snapshot_for_side(
        &self,
        visible_row_range: Range<usize>,
        side: ReviewWorkspaceEditorSide,
    ) -> Vec<WorkspaceDisplayRow> {
        if visible_row_range.is_empty() {
            return Vec::new();
        }

        let document_snapshots = self.document_snapshots_for_side(side);
        build_workspace_display_snapshot_from_document_snapshots(
            &self.layout,
            Viewport {
                first_visible_row: visible_row_range.start,
                visible_row_count: visible_row_range.len(),
                horizontal_offset: 0,
            },
            4,
            false,
            &document_snapshots,
        )
        .visible_rows
    }

    fn build_display_rows_for_side(
        &self,
        visible_row_range: Range<usize>,
        display_rows_by_index: &BTreeMap<usize, WorkspaceDisplayRow>,
    ) -> Vec<WorkspaceDisplayRow> {
        if visible_row_range.is_empty() {
            return Vec::new();
        }

        visible_row_range
            .filter_map(|row_index| display_rows_by_index.get(&row_index).cloned())
            .collect()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn build_display_rows_for_viewport_projection(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_sections: usize,
        overscan_rows: usize,
    ) -> ReviewWorkspaceDisplayRows {
        let mut left_by_row = BTreeMap::new();
        let mut right_by_row = BTreeMap::new();
        for section_ix in self.visible_section_range_for_viewport(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
        ) {
            let Some(section) = self.section(section_ix) else {
                continue;
            };
            let visible_row_range = self
                .section_visible_row_range(
                    section_ix,
                    scroll_top_px,
                    viewport_height_px,
                    overscan_rows,
                )
                .unwrap_or(section.start_row..section.end_row);
            for row in self.build_display_snapshot_for_side(
                visible_row_range.clone(),
                ReviewWorkspaceEditorSide::Left,
            ) {
                left_by_row.insert(row.row_index, row);
            }
            for row in self.build_display_snapshot_for_side(
                visible_row_range.clone(),
                ReviewWorkspaceEditorSide::Right,
            ) {
                right_by_row.insert(row.row_index, row);
            }
        }
        ReviewWorkspaceDisplayRows {
            left_by_row,
            right_by_row,
            left_syntax_by_row: BTreeMap::new(),
            right_syntax_by_row: BTreeMap::new(),
        }
    }

    pub(crate) fn build_search_highlight_columns_by_row(
        &self,
        matches: &[ReviewWorkspaceSearchTarget],
    ) -> BTreeMap<usize, Vec<Range<usize>>> {
        let mut highlights = BTreeMap::<usize, Vec<Range<usize>>>::new();
        for target in matches {
            let Some(range) = target.raw_column_range.clone() else {
                continue;
            };
            highlights.entry(target.row_index).or_default().push(range);
        }
        highlights
    }

    fn build_document_buffers_from_lines(
        &self,
        document_lines: &BTreeMap<WorkspaceDocumentId, Vec<String>>,
    ) -> BTreeMap<WorkspaceDocumentId, TextBuffer> {
        self.layout
            .documents()
            .iter()
            .map(|document| {
                let text = document_lines
                    .get(&document.id)
                    .map(|lines| lines.join("\n"))
                    .unwrap_or_else(|| blank_workspace_document_text(document.line_count.max(1)));
                (
                    document.id,
                    TextBuffer::new(document.buffer_id, text.as_str()),
                )
            })
            .collect()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn document_snapshots_for_side(
        &self,
        side: ReviewWorkspaceEditorSide,
    ) -> BTreeMap<WorkspaceDocumentId, TextSnapshot> {
        let buffers = match side {
            ReviewWorkspaceEditorSide::Left => &self.left_document_buffers,
            ReviewWorkspaceEditorSide::Right => &self.right_document_buffers,
        };
        buffers
            .iter()
            .map(|(document_id, buffer)| (*document_id, buffer.snapshot()))
            .collect()
    }

    fn surface_row_height_px(&self, row_ix: usize) -> usize {
        match self.rows.get(row_ix).map(|row| row.kind) {
            Some(DiffRowKind::HunkHeader) => REVIEW_SURFACE_HUNK_DIVIDER_HEIGHT_PX,
            Some(DiffRowKind::Code | DiffRowKind::Meta | DiffRowKind::Empty) | None => {
                REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX
            }
        }
    }

    fn row_index_for_pixel(&self, pixel_offset: usize) -> usize {
        let row_count = self.row_count();
        if row_count == 0 || self.row_top_offsets.len() < 2 {
            return 0;
        }

        match self.row_top_offsets.binary_search(&pixel_offset) {
            Ok(ix) => ix.min(row_count.saturating_sub(1)),
            Err(ix) => ix.saturating_sub(1).min(row_count.saturating_sub(1)),
        }
    }
}

fn review_stream_row_kind_for_row(row_kind: DiffRowKind) -> DiffStreamRowKind {
    match row_kind {
        DiffRowKind::Code => DiffStreamRowKind::CoreCode,
        DiffRowKind::HunkHeader => DiffStreamRowKind::CoreHunkHeader,
        DiffRowKind::Meta => DiffStreamRowKind::CoreMeta,
        DiffRowKind::Empty => DiffStreamRowKind::CoreEmpty,
    }
}

fn blank_workspace_document_text(line_count: usize) -> String {
    vec![String::new(); line_count.max(1)].join("\n")
}

fn review_base_segment_quality_for_file(line_stats: LineStats) -> DiffSegmentQuality {
    if line_stats.changed() <= 8_000 {
        DiffSegmentQuality::Detailed
    } else {
        DiffSegmentQuality::SyntaxOnly
    }
}

fn review_effective_segment_quality(
    base_quality: DiffSegmentQuality,
    recently_scrolling: bool,
) -> DiffSegmentQuality {
    if !recently_scrolling {
        return base_quality;
    }

    match base_quality {
        DiffSegmentQuality::Detailed => DiffSegmentQuality::SyntaxOnly,
        DiffSegmentQuality::SyntaxOnly => DiffSegmentQuality::Plain,
        DiffSegmentQuality::Plain => DiffSegmentQuality::Plain,
    }
}

fn review_viewport_render_segments(
    editor_syntax_spans: Option<&Vec<RowSyntaxSpan>>,
    cached_segments: Option<&Vec<CachedStyledSegment>>,
    display_text: &str,
    search_highlights: &[Range<usize>],
) -> Vec<CachedStyledSegment> {
    apply_search_highlights_to_cached_segments(
        editor_syntax_spans
            .map(|spans: &Vec<RowSyntaxSpan>| {
                review_cached_segments_from_editor_syntax(display_text, spans.as_slice())
            })
            .map(|segments| {
                merge_cached_segments_with_changed_flags(segments, cached_segments, display_text)
            })
            .or_else(|| cached_segments.cloned())
            .map(|segments| {
                compact_cached_segments_for_render(
                    segments,
                    REVIEW_VIEWPORT_RENDER_MAX_SEGMENTS_PER_CELL,
                )
            })
            .unwrap_or_else(|| cached_runtime_fallback_segments(display_text)),
        search_highlights,
    )
}

fn review_cached_segments_from_editor_syntax(
    display_text: &str,
    spans: &[RowSyntaxSpan],
) -> Vec<CachedStyledSegment> {
    let total_columns = display_text.chars().count();
    if total_columns == 0 {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut cursor = 0usize;
    for span in spans {
        let start = span.start_column.min(total_columns);
        let end = span.end_column.min(total_columns);
        if cursor < start {
            review_push_cached_syntax_segment(
                &mut segments,
                SyntaxTokenKind::Plain,
                review_display_text_slice(display_text, cursor, start),
            );
        }
        if start < end {
            review_push_cached_syntax_segment(
                &mut segments,
                review_syntax_token_for_style_key(span.style_key.as_str()),
                review_display_text_slice(display_text, start, end),
            );
        }
        cursor = end;
    }

    if cursor < total_columns {
        review_push_cached_syntax_segment(
            &mut segments,
            SyntaxTokenKind::Plain,
            review_display_text_slice(display_text, cursor, total_columns),
        );
    }

    if segments.is_empty() {
        review_push_cached_syntax_segment(
            &mut segments,
            SyntaxTokenKind::Plain,
            display_text.to_string(),
        );
    }

    segments
}

fn review_push_cached_syntax_segment(
    segments: &mut Vec<CachedStyledSegment>,
    syntax: SyntaxTokenKind,
    text: String,
) {
    if text.is_empty() {
        return;
    }

    if let Some(previous) = segments.last_mut()
        && previous.syntax == syntax
        && !previous.changed
        && !previous.search_match
    {
        previous.plain_text = format!("{}{}", previous.plain_text.as_ref(), text).into();
        return;
    }

    segments.push(CachedStyledSegment {
        plain_text: text.into(),
        syntax,
        changed: false,
        search_match: false,
    });
}

fn review_display_text_slice(text: &str, start_column: usize, end_column: usize) -> String {
    text.chars()
        .skip(start_column)
        .take(end_column.saturating_sub(start_column))
        .collect()
}

fn review_syntax_token_for_style_key(style_key: &str) -> SyntaxTokenKind {
    match style_key.split('.').next().unwrap_or_default() {
        "keyword" => SyntaxTokenKind::Keyword,
        "string" => SyntaxTokenKind::String,
        "number" => SyntaxTokenKind::Number,
        "comment" => SyntaxTokenKind::Comment,
        "function" => SyntaxTokenKind::Function,
        "type" | "constructor" | "tag" => SyntaxTokenKind::TypeName,
        "constant" | "attribute" | "boolean" => SyntaxTokenKind::Constant,
        "variable" | "property" | "parameter" => SyntaxTokenKind::Variable,
        "operator" | "punctuation" => SyntaxTokenKind::Operator,
        _ => SyntaxTokenKind::Plain,
    }
}

fn review_project_search_highlights_for_display_row(
    row: &WorkspaceDisplayRow,
    raw_ranges: &[Range<usize>],
) -> Vec<Range<usize>> {
    raw_ranges
        .iter()
        .filter_map(|range| {
            let start = range.start.max(row.raw_start_column);
            let end = range.end.min(row.raw_end_column);
            if start >= end {
                return None;
            }

            Some(
                review_workspace_display_column_for_raw(row, start)
                    ..review_workspace_display_column_for_raw(row, end),
            )
        })
        .collect()
}

fn review_workspace_display_column_for_raw(row: &WorkspaceDisplayRow, raw_column: usize) -> usize {
    if row.raw_column_offsets.is_empty() {
        return 0;
    }

    let relative_raw = raw_column
        .saturating_sub(row.raw_start_column)
        .min(row.raw_column_offsets.len().saturating_sub(1));
    row.raw_column_offsets[relative_raw]
}

fn prioritized_prefetch_row_indices_for_rows(
    mut row_indices: Vec<usize>,
    anchor_row: usize,
) -> Vec<usize> {
    row_indices.sort_unstable();
    row_indices.dedup();
    row_indices.sort_by_key(|row_ix| (anchor_row.abs_diff(*row_ix), *row_ix));
    row_indices
}

fn review_decimal_digits(value: u32) -> u32 {
    if value == 0 { 1 } else { value.ilog10() + 1 }
}

fn review_document_line_count(document: &DiffDocument) -> usize {
    let max_old_line = document
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .filter_map(|line| line.old_line)
        .max()
        .unwrap_or(0) as usize;
    let max_new_line = document
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .filter_map(|line| line.new_line)
        .max()
        .unwrap_or(0) as usize;
    let fallback_lines = document
        .hunks
        .iter()
        .map(|hunk| hunk.lines.len())
        .max()
        .unwrap_or(0);

    max_old_line.max(max_new_line).max(fallback_lines).max(1)
}

fn review_hunk_line_range(hunk: &DiffHunk, line_count: usize) -> Range<usize> {
    let first_line = hunk
        .lines
        .iter()
        .filter_map(|line| line.new_line.or(line.old_line))
        .min()
        .or(hunk.new_start)
        .or(hunk.old_start)
        .unwrap_or(1) as usize;
    let last_line = hunk
        .lines
        .iter()
        .filter_map(|line| line.new_line.or(line.old_line))
        .max()
        .or(hunk.new_start)
        .or(hunk.old_start)
        .unwrap_or(1) as usize;

    let start = first_line
        .saturating_sub(1)
        .min(line_count.saturating_sub(1));
    let mut end = last_line.max(first_line).min(line_count.max(1));
    if end <= start {
        end = (start + 1).min(line_count.max(1));
    }

    start..end
}

fn surface_row_count_for_hunk(hunk: &DiffHunk) -> usize {
    HUNK_HEADER_SURFACE_ROWS
        .saturating_add(surface_code_row_count_for_hunk(hunk))
        .saturating_add(hunk.trailing_meta.len())
}

fn surface_code_row_count_for_hunk(hunk: &DiffHunk) -> usize {
    let mut ix = 0_usize;
    let mut rows = 0_usize;

    while ix < hunk.lines.len() {
        match hunk.lines[ix].kind {
            DiffLineKind::Context | DiffLineKind::Added => {
                rows = rows.saturating_add(1);
                ix += 1;
            }
            DiffLineKind::Removed => {
                let removed_start = ix;
                while ix < hunk.lines.len() && hunk.lines[ix].kind == DiffLineKind::Removed {
                    ix += 1;
                }
                let added_start = ix;
                while ix < hunk.lines.len() && hunk.lines[ix].kind == DiffLineKind::Added {
                    ix += 1;
                }
                rows = rows.saturating_add(
                    ix.saturating_sub(added_start)
                        .max(added_start.saturating_sub(removed_start)),
                );
            }
        }
    }

    rows
}
