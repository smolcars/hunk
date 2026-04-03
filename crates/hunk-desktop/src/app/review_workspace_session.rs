use std::collections::BTreeSet;
use std::ops::Range;

use hunk_domain::diff::SideBySideRow;
use hunk_domain::diff::{DiffDocument, DiffHunk, DiffLineKind, parse_patch_document};
use hunk_editor::{
    WorkspaceDocument, WorkspaceDocumentId, WorkspaceExcerptId, WorkspaceExcerptKind,
    WorkspaceExcerptSpec, WorkspaceLayout, WorkspaceLayoutError,
};
use hunk_git::compare::CompareSnapshot;
use hunk_git::git::FileStatus;
use hunk_text::BufferId;

use crate::app::data::DiffStream;
use crate::app::{DiffRowSegmentCache, DiffStreamRowMeta};

const FILE_HEADER_SURFACE_ROWS: usize = 1;
const HUNK_HEADER_SURFACE_ROWS: usize = 1;

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

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspaceSession {
    layout: WorkspaceLayout,
    file_ranges: Vec<ReviewWorkspaceFileRange>,
    hunk_ranges: Vec<ReviewWorkspaceHunkRange>,
    rows: Vec<SideBySideRow>,
    row_metadata: Vec<DiffStreamRowMeta>,
    row_segments: Vec<Option<DiffRowSegmentCache>>,
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

        for (path, status, surface_row_range) in file_plans {
            file_ranges.push(ReviewWorkspaceFileRange {
                path,
                status,
                start_row: surface_row_range.start,
                end_row: surface_row_range.end,
            });
        }

        Ok(Self {
            layout,
            file_ranges,
            hunk_ranges,
            rows: Vec::new(),
            row_metadata: Vec::new(),
            row_segments: Vec::new(),
        })
    }

    pub(crate) fn with_render_stream(mut self, stream: &DiffStream) -> Self {
        self.rows = stream.rows.clone();
        self.row_metadata = stream.row_metadata.clone();
        self.row_segments = stream.row_segments.clone();
        self
    }

    pub(crate) fn file_ranges(&self) -> &[ReviewWorkspaceFileRange] {
        &self.file_ranges
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
        self.rows.len()
    }

    pub(crate) fn row(&self, row_ix: usize) -> Option<&SideBySideRow> {
        self.rows.get(row_ix)
    }

    pub(crate) fn row_metadata(&self, row_ix: usize) -> Option<&DiffStreamRowMeta> {
        self.row_metadata.get(row_ix)
    }

    pub(crate) fn row_segment_cache(&self, row_ix: usize) -> Option<&DiffRowSegmentCache> {
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
