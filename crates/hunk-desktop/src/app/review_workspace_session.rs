use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use hunk_domain::db::{CommentLineSide, compute_comment_anchor_hash};
use hunk_domain::diff::SideBySideRow;
use hunk_domain::diff::{
    DiffCellKind, DiffDocument, DiffHunk, DiffLineKind, DiffRowKind, parse_patch_document,
};
use hunk_editor::{
    Viewport, WorkspaceDisplayRow, WorkspaceDocument, WorkspaceDocumentId, WorkspaceExcerptId,
    WorkspaceExcerptKind, WorkspaceExcerptSpec, WorkspaceLayout, WorkspaceLayoutError,
    build_workspace_display_snapshot,
};
use hunk_git::compare::CompareSnapshot;
use hunk_git::git::{FileStatus, LineStats};
use hunk_text::BufferId;

use crate::app::data::{DiffSegmentQuality, DiffStream, DiffStreamRowKind};
use crate::app::native_files_editor::WorkspaceEditorSession;
use crate::app::{DiffRowSegmentCache, DiffStreamRowMeta};

const FILE_HEADER_SURFACE_ROWS: usize = 1;
const HUNK_HEADER_SURFACE_ROWS: usize = 1;
pub(crate) const REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX: usize = 26;
pub(crate) const REVIEW_SURFACE_HUNK_DIVIDER_HEIGHT_PX: usize = 6;

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

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspaceViewportSection {
    pub(crate) section_index: usize,
    pub(crate) pixel_range: Range<usize>,
    pub(crate) visible_row_range: Range<usize>,
    pub(crate) top_spacer_height_px: usize,
    pub(crate) bottom_spacer_height_px: usize,
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
    pub(crate) text: String,
    pub(crate) left_cell_kind: DiffCellKind,
    pub(crate) left_line: Option<u32>,
    pub(crate) right_cell_kind: DiffCellKind,
    pub(crate) right_line: Option<u32>,
    pub(crate) local_top_px: usize,
    pub(crate) height_px: usize,
    pub(crate) left_display_row: WorkspaceDisplayRow,
    pub(crate) right_display_row: WorkspaceDisplayRow,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspaceViewportSnapshot {
    pub(crate) total_surface_height_px: usize,
    pub(crate) sections: Vec<ReviewWorkspaceViewportSection>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspaceSurfaceSnapshot {
    pub(crate) scroll_top_px: usize,
    pub(crate) viewport_height_px: usize,
    pub(crate) viewport: ReviewWorkspaceViewportSnapshot,
    pub(crate) visible_state: ReviewWorkspaceVisibleState,
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
    left_document_lines: BTreeMap<WorkspaceDocumentId, Vec<String>>,
    right_document_lines: BTreeMap<WorkspaceDocumentId, Vec<String>>,
    rows: Vec<SideBySideRow>,
    row_metadata: Vec<DiffStreamRowMeta>,
    row_segments: Vec<Option<DiffRowSegmentCache>>,
    row_top_offsets: Vec<usize>,
    section_pixel_ranges: Vec<Range<usize>>,
    total_surface_height_px: usize,
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
            left_document_lines: BTreeMap::new(),
            right_document_lines: BTreeMap::new(),
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
        self.rebuild_document_line_text();
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

    pub(crate) fn build_viewport_snapshot(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_sections: usize,
        overscan_rows: usize,
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
            let left_display_rows = self.build_display_snapshot_for_side(
                visible_row_range.clone(),
                ReviewWorkspaceDisplaySide::Left,
            );
            let right_display_rows = self.build_display_snapshot_for_side(
                visible_row_range.clone(),
                ReviewWorkspaceDisplaySide::Right,
            );
            let top_spacer_height_px = self
                .row_boundary_offset_px(visible_row_range.start)
                .unwrap_or(pixel_range.start)
                .saturating_sub(pixel_range.start);
            let bottom_spacer_height_px = pixel_range.end.saturating_sub(
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
                    Some(ReviewWorkspaceViewportRow {
                        row_index,
                        stable_id: row_metadata
                            .map(|meta| meta.stable_id)
                            .unwrap_or(row_index as u64),
                        row_kind: row.kind,
                        stream_kind: row_metadata
                            .map(|meta| meta.kind)
                            .unwrap_or_else(|| review_stream_row_kind_for_row(row.kind)),
                        file_path,
                        file_status,
                        text: row.text.clone(),
                        left_cell_kind: row.left.kind,
                        left_line: row.left.line,
                        right_cell_kind: row.right.kind,
                        right_line: row.right.line,
                        local_top_px: self
                            .row_top_offset_px(row_index)
                            .unwrap_or(visible_start_px)
                            .saturating_sub(visible_start_px),
                        height_px: self.surface_row_height_px(row_index),
                        left_display_row,
                        right_display_row,
                    })
                })
                .collect();
            sections.push(ReviewWorkspaceViewportSection {
                section_index: section_ix,
                pixel_range,
                visible_row_range,
                top_spacer_height_px,
                bottom_spacer_height_px,
                rows,
            });
        }

        ReviewWorkspaceViewportSnapshot {
            total_surface_height_px: self.total_surface_height_px(),
            sections,
        }
    }

    pub(crate) fn build_surface_snapshot(
        &self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_sections: usize,
        overscan_rows: usize,
    ) -> ReviewWorkspaceSurfaceSnapshot {
        let viewport = self.build_viewport_snapshot(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
            overscan_rows,
        );
        let visible_state = self.build_visible_state(scroll_top_px, viewport_height_px);

        ReviewWorkspaceSurfaceSnapshot {
            scroll_top_px,
            viewport_height_px,
            viewport,
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
        self.build_viewport_snapshot(
            scroll_top_px,
            viewport_height_px,
            overscan_sections,
            overscan_rows,
        )
        .sections
        .into_iter()
        .flat_map(|section| section.rows.into_iter().map(|row| row.row_index))
        .collect()
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

    fn rebuild_document_line_text(&mut self) {
        self.left_document_lines = self
            .layout
            .documents()
            .iter()
            .map(|document| (document.id, vec![String::new(); document.line_count.max(1)]))
            .collect();
        self.right_document_lines = self
            .layout
            .documents()
            .iter()
            .map(|document| (document.id, vec![String::new(); document.line_count.max(1)]))
            .collect();

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
            if let Some(lines) = self.left_document_lines.get_mut(&location.document_id)
                && let Some(slot) = lines.get_mut(document_line)
            {
                *slot = row.left.text.clone();
            }
            if let Some(lines) = self.right_document_lines.get_mut(&location.document_id)
                && let Some(slot) = lines.get_mut(document_line)
            {
                *slot = row.right.text.clone();
            }
        }
    }

    fn build_display_snapshot_for_side(
        &self,
        visible_row_range: Range<usize>,
        side: ReviewWorkspaceDisplaySide,
    ) -> Vec<WorkspaceDisplayRow> {
        if visible_row_range.is_empty() {
            return Vec::new();
        }

        build_workspace_display_snapshot(
            &self.layout,
            Viewport {
                first_visible_row: visible_row_range.start,
                visible_row_count: visible_row_range.len(),
                horizontal_offset: 0,
            },
            4,
            false,
            |document_id, document_line| self.document_line_text(side, document_id, document_line),
        )
        .visible_rows
    }

    fn document_line_text(
        &self,
        side: ReviewWorkspaceDisplaySide,
        document_id: WorkspaceDocumentId,
        document_line: usize,
    ) -> Option<String> {
        let lines = match side {
            ReviewWorkspaceDisplaySide::Left => &self.left_document_lines,
            ReviewWorkspaceDisplaySide::Right => &self.right_document_lines,
        };
        lines
            .get(&document_id)
            .and_then(|document_lines| document_lines.get(document_line))
            .cloned()
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

fn prioritized_prefetch_row_indices_for_rows(
    mut row_indices: Vec<usize>,
    anchor_row: usize,
) -> Vec<usize> {
    row_indices.sort_unstable();
    row_indices.dedup();
    row_indices.sort_by_key(|row_ix| (anchor_row.abs_diff(*row_ix), *row_ix));
    row_indices
}

#[derive(Clone, Copy)]
enum ReviewWorkspaceDisplaySide {
    Left,
    Right,
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
