#[allow(dead_code)]
#[path = "../src/app/native_files_editor_workspace.rs"]
mod workspace_editor_session;

mod app {
    use gpui::SharedString;
    use hunk_git::git::FileStatus;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SyntaxTokenKind {
        Plain,
        Keyword,
        String,
        Number,
        Comment,
        Function,
        TypeName,
        Constant,
        Variable,
        Operator,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CachedStyledSegment {
        pub plain_text: SharedString,
        pub syntax: SyntaxTokenKind,
        pub changed: bool,
        pub search_match: bool,
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
    #[allow(dead_code)]
    pub enum DiffSegmentQuality {
        #[default]
        Plain,
        SyntaxOnly,
        Detailed,
    }

    #[derive(Debug, Clone, Default)]
    pub struct DiffRowSegmentCache {
        pub quality: DiffSegmentQuality,
        pub left: Vec<CachedStyledSegment>,
        pub right: Vec<CachedStyledSegment>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[allow(dead_code)]
    pub enum DiffStreamRowKind {
        FileHeader,
        CoreCode,
        CoreHunkHeader,
        CoreMeta,
        CoreEmpty,
        FileLoading,
        FileCollapsed,
        FileError,
        EmptyState,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    pub struct DiffStreamRowMeta {
        pub stable_id: u64,
        pub file_path: Option<String>,
        pub file_status: Option<FileStatus>,
        pub kind: DiffStreamRowKind,
    }

    pub mod data {
        use super::{DiffRowSegmentCache, DiffStreamRowMeta, SyntaxTokenKind};
        use gpui::SharedString;
        use hunk_domain::diff::SideBySideRow;

        pub use super::{CachedStyledSegment, DiffSegmentQuality};

        #[derive(Debug, Clone, Default)]
        pub struct DiffStream {
            pub rows: Vec<SideBySideRow>,
            pub row_metadata: Vec<DiffStreamRowMeta>,
            pub row_segments: Vec<Option<DiffRowSegmentCache>>,
        }

        pub use super::DiffStreamRowKind;

        pub fn cached_runtime_fallback_segments(text: &str) -> Vec<CachedStyledSegment> {
            if text.is_empty() {
                return Vec::new();
            }

            vec![CachedStyledSegment {
                plain_text: SharedString::from(text.to_string()),
                syntax: SyntaxTokenKind::Plain,
                changed: false,
                search_match: false,
            }]
        }

        pub fn compact_cached_segments_for_render(
            segments: Vec<CachedStyledSegment>,
            _max_segments: usize,
        ) -> Vec<CachedStyledSegment> {
            segments
        }

        pub fn apply_search_highlights_to_cached_segments(
            segments: Vec<CachedStyledSegment>,
            highlight_columns: &[std::ops::Range<usize>],
        ) -> Vec<CachedStyledSegment> {
            if highlight_columns.is_empty() {
                return segments;
            }

            segments
                .into_iter()
                .map(|mut segment| {
                    segment.search_match = true;
                    segment
                })
                .collect()
        }

        pub fn merge_cached_segments_with_changed_flags(
            syntax_segments: Vec<CachedStyledSegment>,
            changed_segments: Option<&Vec<CachedStyledSegment>>,
            text: &str,
        ) -> Vec<CachedStyledSegment> {
            let Some(changed_segments) = changed_segments else {
                return syntax_segments;
            };
            if syntax_segments.is_empty() {
                return syntax_segments;
            }

            let total_columns = text.chars().count();
            let mut changed_by_column = Vec::with_capacity(total_columns);
            for segment in changed_segments {
                changed_by_column.extend(std::iter::repeat_n(
                    segment.changed,
                    segment.plain_text.chars().count(),
                ));
            }
            changed_by_column.resize(total_columns, false);

            let mut merged = Vec::new();
            let mut column = 0usize;
            for segment in syntax_segments {
                let column_end = (column + segment.plain_text.chars().count()).min(total_columns);
                if column >= column_end {
                    continue;
                }

                let mut run_start = column;
                while run_start < column_end {
                    let run_changed = changed_by_column[run_start];
                    let mut run_end = run_start + 1;
                    while run_end < column_end && changed_by_column[run_end] == run_changed {
                        run_end += 1;
                    }

                    merged.push(CachedStyledSegment {
                        plain_text: SharedString::from(segment_slice(
                            segment.plain_text.as_ref(),
                            run_start.saturating_sub(column),
                            run_end.saturating_sub(column),
                        )),
                        syntax: segment.syntax,
                        changed: run_changed,
                        search_match: false,
                    });
                    run_start = run_end;
                }

                column = column_end;
            }

            merged
        }

        fn segment_slice(text: &str, start_column: usize, end_column: usize) -> String {
            if start_column >= end_column {
                return String::new();
            }

            text.chars()
                .skip(start_column)
                .take(end_column.saturating_sub(start_column))
                .collect()
        }
    }

    pub mod highlight {
        pub use super::SyntaxTokenKind;
    }

    pub mod native_files_editor {
        pub(crate) use crate::workspace_editor_session::WorkspaceEditorSession;

        pub(crate) mod paint {
            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct RowSyntaxSpan {
                pub(crate) start_column: usize,
                pub(crate) end_column: usize,
                pub(crate) style_key: String,
            }
        }
    }

    pub mod comment_overlay {
        pub(crate) fn review_comment_overlay_top_px(
            row_top_px: usize,
            scroll_top_px: usize,
            viewport_height_px: usize,
            row_height_px: usize,
        ) -> f32 {
            let desired_top = row_top_px
                .saturating_sub(scroll_top_px)
                .saturating_add(row_height_px) as f32;
            let max_top = viewport_height_px.saturating_sub(176) as f32;
            desired_top.clamp(8.0, max_top.max(8.0))
        }
    }
}

#[allow(dead_code)]
#[path = "../src/app/review_workspace_session.rs"]
mod review_workspace_session;

use std::collections::{BTreeMap, BTreeSet};

use hunk_domain::diff::{
    DiffCell, DiffCellKind, DiffRowKind, SideBySideRow, parse_patch_side_by_side,
};
use hunk_editor::{SearchHighlight, WorkspaceDisplayRow};
use hunk_git::compare::CompareSnapshot;
use hunk_git::git::{ChangedFile, FileStatus, LineStats};
use review_workspace_session::{
    REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX, REVIEW_SURFACE_HUNK_DIVIDER_HEIGHT_PX,
    ReviewWorkspaceDisplayRowEntry, ReviewWorkspaceDisplayRows, ReviewWorkspaceEditorSide,
    ReviewWorkspaceSegmentPrefetchRequest, ReviewWorkspaceSession, ReviewWorkspaceSurfaceOptions,
};

fn changed_file(path: &str, status: FileStatus) -> ChangedFile {
    ChangedFile {
        path: path.to_string(),
        status,
        staged: false,
        unstaged: false,
        untracked: false,
    }
}

fn stream_row_metadata_for_rows(
    rows: &[hunk_domain::diff::SideBySideRow],
    path: &str,
    status: FileStatus,
) -> Vec<app::DiffStreamRowMeta> {
    rows.iter()
        .enumerate()
        .map(|(ix, row)| app::DiffStreamRowMeta {
            stable_id: ix as u64 + 2,
            file_path: Some(path.to_string()),
            file_status: Some(status),
            kind: match row.kind {
                DiffRowKind::Code => app::DiffStreamRowKind::CoreCode,
                DiffRowKind::HunkHeader => app::DiffStreamRowKind::CoreHunkHeader,
                DiffRowKind::Meta => app::DiffStreamRowKind::CoreMeta,
                DiffRowKind::Empty => app::DiffStreamRowKind::CoreEmpty,
            },
        })
        .collect()
}

fn file_header_row(path: &str) -> SideBySideRow {
    SideBySideRow {
        kind: DiffRowKind::Meta,
        left: DiffCell {
            line: None,
            text: String::new(),
            kind: DiffCellKind::None,
        },
        right: DiffCell {
            line: None,
            text: String::new(),
            kind: DiffCellKind::None,
        },
        text: path.to_string(),
    }
}

fn empty_file_body_row() -> SideBySideRow {
    SideBySideRow {
        kind: DiffRowKind::Empty,
        left: DiffCell {
            line: None,
            text: String::new(),
            kind: DiffCellKind::None,
        },
        right: DiffCell {
            line: None,
            text: String::new(),
            kind: DiffCellKind::None,
        },
        text: "No textual diff to display.".to_string(),
    }
}

fn review_stream_for_rows(
    core_rows: &[SideBySideRow],
    path: &str,
    status: FileStatus,
) -> app::data::DiffStream {
    let mut rows = Vec::with_capacity(core_rows.len().saturating_add(1).max(2));
    rows.push(file_header_row(path));
    if core_rows.is_empty() {
        rows.push(empty_file_body_row());
    } else {
        rows.extend(core_rows.iter().cloned());
    }

    let mut row_metadata = vec![app::DiffStreamRowMeta {
        stable_id: 1,
        file_path: Some(path.to_string()),
        file_status: Some(status),
        kind: app::DiffStreamRowKind::FileHeader,
    }];
    row_metadata.extend(stream_row_metadata_for_rows(core_rows, path, status));
    if core_rows.is_empty() {
        row_metadata.push(app::DiffStreamRowMeta {
            stable_id: 2,
            file_path: Some(path.to_string()),
            file_status: Some(status),
            kind: app::DiffStreamRowKind::CoreEmpty,
        });
    }

    app::data::DiffStream {
        row_segments: vec![None; rows.len()],
        row_metadata,
        rows,
    }
}

#[test]
fn review_workspace_session_registers_multi_file_hunk_excerpts() {
    let first_patch = "\
@@ -1,3 +1,4 @@
 alpha
-beta
+beta updated
 gamma
+delta
@@ -10,2 +11,3 @@
-old line
+new line
 context
+tail
";
    let second_patch = "";
    let snapshot = CompareSnapshot {
        files: vec![
            changed_file("src/lib.rs", FileStatus::Modified),
            changed_file("README.md", FileStatus::Added),
        ],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([
            ("src/lib.rs".to_string(), first_patch.to_string()),
            ("README.md".to_string(), second_patch.to_string()),
        ]),
    };

    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build");

    assert_eq!(session.layout().documents().len(), 2);
    assert_eq!(session.layout().excerpts().len(), 3);
    assert_eq!(session.row_count(), session.layout().total_rows());
    assert_eq!(session.first_path(), Some("src/lib.rs"));
    assert_eq!(session.file_ranges().len(), 2);
    assert_eq!(session.hunk_ranges().len(), 2);
    assert_eq!(session.path_at_surface_row(0), Some("src/lib.rs"));
    assert_eq!(
        session.path_at_surface_row(session.file_ranges()[1].start_row),
        Some("README.md")
    );
    assert_eq!(
        session.hunk_header_at_surface_row(session.hunk_ranges()[0].start_row),
        Some("@@ -1,3 +1,4 @@")
    );
}

#[test]
fn review_workspace_session_search_matches_follow_excerpt_surface_order() {
    let patch = "\
@@ -1,2 +1,2 @@
-before
+needle first
 keep
@@ -10,2 +10,3 @@
 context
-old
+needle second
+tail
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/lib.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/lib.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/lib.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("review workspace session should build")
        .with_render_stream(&stream);
    let matches = session.workspace_search_matches("needle");

    assert!(matches.len() >= 2);
    assert!(matches.iter().all(|target| target.path == "src/lib.rs"));
    assert!(
        matches
            .iter()
            .all(|target| target.raw_column_range.is_some())
    );
    assert!(
        matches
            .windows(2)
            .all(|pair| pair[0].surface_order <= pair[1].surface_order)
    );
}

#[test]
fn review_workspace_surface_snapshot_marks_visible_search_matches() {
    let patch = "\
@@ -1,2 +1,2 @@
-before
+needle first
 keep
@@ -10,2 +10,3 @@
 context
-old
+needle second
+tail
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/lib.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/lib.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/lib.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("review workspace session should build")
        .with_render_stream(&stream);
    let matches = session.workspace_search_matches("needle");
    let options = ReviewWorkspaceSurfaceOptions {
        search_highlight_columns_by_row: session.build_search_highlight_columns_by_row(&matches),
        ..ReviewWorkspaceSurfaceOptions::default()
    };

    let surface = session.build_surface_snapshot(0, 512, 1, 8, &options);
    let highlighted_rows = surface
        .viewport
        .sections
        .iter()
        .flat_map(|section| section.rows.iter())
        .filter(|row| {
            row.right_segments
                .iter()
                .any(|segment| segment.search_match)
        })
        .map(|row| row.row_index)
        .collect::<Vec<_>>();

    assert!(!highlighted_rows.is_empty());
}

#[test]
fn review_workspace_surface_snapshot_prefers_display_row_search_highlights() {
    let patch = "\
@@ -1,2 +1,2 @@
-before
+needle first
 keep
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/lib.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/lib.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/lib.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("review workspace session should build")
        .with_render_stream(&stream);
    let row_range = 0..session.row_count();
    let left_by_row = session
        .build_display_snapshot_for_side(row_range.clone(), ReviewWorkspaceEditorSide::Left)
        .into_iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let mut right_by_row = session
        .build_display_snapshot_for_side(row_range, ReviewWorkspaceEditorSide::Right)
        .into_iter()
        .map(|row| {
            let mut row = row;
            if row.text.contains("needle") {
                row.search_highlights = vec![SearchHighlight {
                    start_column: 0,
                    end_column: 6,
                }];
            }
            (row.row_index, row)
        })
        .collect::<BTreeMap<_, _>>();
    assert!(
        right_by_row
            .values()
            .any(|row| !row.search_highlights.is_empty())
    );

    let rows = left_by_row
        .iter()
        .filter_map(|(row_index, left)| {
            Some(review_workspace_session::ReviewWorkspaceDisplayRowEntry {
                display_row_index: *row_index,
                row_index: *row_index,
                raw_row_range: *row_index..row_index.saturating_add(1),
                left: left.clone(),
                right: right_by_row.get(row_index)?.clone(),
            })
        })
        .collect::<Vec<_>>();
    let display_rows = ReviewWorkspaceDisplayRows {
        rows,
        left_by_display_row: left_by_row,
        right_by_display_row: std::mem::take(&mut right_by_row),
        left_syntax_by_display_row: BTreeMap::new(),
        right_syntax_by_display_row: BTreeMap::new(),
    };
    let surface = session.build_surface_snapshot_with_display_rows(
        0,
        512,
        1,
        8,
        &ReviewWorkspaceSurfaceOptions::default(),
        &display_rows,
    );
    assert!(
        surface
            .viewport
            .sections
            .iter()
            .flat_map(|section| section.rows.iter())
            .any(|row| row
                .right_segments
                .iter()
                .any(|segment| segment.search_match))
    );
}

#[test]
fn review_workspace_display_geometry_tracks_expanded_display_rows() {
    let patch = "\
@@ -1,3 +1,3 @@
-before
+after
 keep
 tail
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/lib.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/lib.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/lib.rs", FileStatus::Modified);
    let mut session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("review workspace session should build")
        .with_render_stream(&stream);
    let base_total_height = session.total_surface_height_px();
    let base_section_pixel_range = session
        .section_pixel_range(0)
        .cloned()
        .expect("section pixel range");

    let left_by_raw_row = session
        .build_display_snapshot_for_side(0..session.row_count(), ReviewWorkspaceEditorSide::Left)
        .into_iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let right_by_raw_row = session
        .build_display_snapshot_for_side(0..session.row_count(), ReviewWorkspaceEditorSide::Right)
        .into_iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let target_code_row = (0..session.row_count())
        .find(|&row_ix| {
            session
                .row(row_ix)
                .is_some_and(|row| row.kind == DiffRowKind::Code)
        })
        .expect("code row");
    let hunk_header_row = (0..session.row_count())
        .find(|&row_ix| {
            session
                .row(row_ix)
                .is_some_and(|row| row.kind == DiffRowKind::HunkHeader)
        })
        .expect("hunk header row");

    let mut rows = Vec::new();
    let mut left_by_display_row = BTreeMap::new();
    let mut right_by_display_row = BTreeMap::new();
    let mut next_display_row = 0usize;
    for row_ix in 0..session.row_count() {
        let repeat = if row_ix == target_code_row { 3 } else { 1 };
        let left = left_by_raw_row
            .get(&row_ix)
            .cloned()
            .expect("left display row");
        let right = right_by_raw_row
            .get(&row_ix)
            .cloned()
            .expect("right display row");
        for _ in 0..repeat {
            let mut left = left.clone();
            left.row_index = next_display_row;
            let mut right = right.clone();
            right.row_index = next_display_row;
            rows.push(ReviewWorkspaceDisplayRowEntry {
                display_row_index: next_display_row,
                row_index: row_ix,
                raw_row_range: row_ix..row_ix.saturating_add(1),
                left: left.clone(),
                right: right.clone(),
            });
            left_by_display_row.insert(next_display_row, left);
            right_by_display_row.insert(next_display_row, right);
            next_display_row = next_display_row.saturating_add(1);
        }
    }

    let display_rows = ReviewWorkspaceDisplayRows {
        rows,
        left_by_display_row,
        right_by_display_row,
        left_syntax_by_display_row: BTreeMap::new(),
        right_syntax_by_display_row: BTreeMap::new(),
    };
    session.refresh_display_geometry_from_display_rows(&display_rows);

    assert_eq!(
        session.display_row_range_for_raw_row(hunk_header_row),
        Some(hunk_header_row..hunk_header_row.saturating_add(1))
    );
    assert_eq!(
        session.display_row_range_for_raw_row(target_code_row),
        Some(target_code_row..target_code_row.saturating_add(3))
    );
    assert_eq!(
        session.display_row_boundary_for_raw_row(target_code_row.saturating_add(1)),
        Some(target_code_row.saturating_add(3))
    );
    assert_eq!(
        session.display_geometry_total_display_rows(),
        session.row_count().saturating_add(2)
    );
    assert_eq!(
        session.display_geometry_total_surface_height_px(),
        base_total_height
            .saturating_add(2usize.saturating_mul(REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX))
    );
    assert_eq!(
        session.display_geometry_section_pixel_range(0),
        Some(
            &(base_section_pixel_range.start
                ..base_section_pixel_range
                    .end
                    .saturating_add(2usize.saturating_mul(REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX)))
        )
    );
    assert_eq!(
        session.display_geometry_section_display_row_range(0),
        Some(&(0..session.row_count().saturating_add(2)))
    );
}

#[test]
fn review_workspace_surface_snapshot_positions_expanded_display_rows_by_display_index() {
    let patch = "\
@@ -1,3 +1,3 @@
-before
+after
 keep
 tail
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/lib.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/lib.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/lib.rs", FileStatus::Modified);
    let mut session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("review workspace session should build")
        .with_render_stream(&stream);

    let left_by_raw_row = session
        .build_display_snapshot_for_side(0..session.row_count(), ReviewWorkspaceEditorSide::Left)
        .into_iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let right_by_raw_row = session
        .build_display_snapshot_for_side(0..session.row_count(), ReviewWorkspaceEditorSide::Right)
        .into_iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let target_code_row = (0..session.row_count())
        .find(|&row_ix| {
            session
                .row(row_ix)
                .is_some_and(|row| row.kind == DiffRowKind::Code)
        })
        .expect("code row");

    let mut rows = Vec::new();
    let mut left_by_display_row = BTreeMap::new();
    let mut right_by_display_row = BTreeMap::new();
    let mut next_display_row = 0usize;
    for row_ix in 0..session.row_count() {
        let repeat = if row_ix == target_code_row { 3 } else { 1 };
        let left = left_by_raw_row
            .get(&row_ix)
            .cloned()
            .expect("left display row");
        let right = right_by_raw_row
            .get(&row_ix)
            .cloned()
            .expect("right display row");
        for _ in 0..repeat {
            let mut left = left.clone();
            left.row_index = next_display_row;
            let mut right = right.clone();
            right.row_index = next_display_row;
            rows.push(ReviewWorkspaceDisplayRowEntry {
                display_row_index: next_display_row,
                row_index: row_ix,
                raw_row_range: row_ix..row_ix.saturating_add(1),
                left: left.clone(),
                right: right.clone(),
            });
            left_by_display_row.insert(next_display_row, left);
            right_by_display_row.insert(next_display_row, right);
            next_display_row = next_display_row.saturating_add(1);
        }
    }

    let display_rows = ReviewWorkspaceDisplayRows {
        rows,
        left_by_display_row,
        right_by_display_row,
        left_syntax_by_display_row: BTreeMap::new(),
        right_syntax_by_display_row: BTreeMap::new(),
    };
    session.refresh_display_geometry_from_display_rows(&display_rows);
    let target_row_top_px = session
        .row_top_offset_px(target_code_row)
        .expect("target row top");

    let surface = session.build_surface_snapshot_with_display_rows(
        target_row_top_px + REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX,
        REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX,
        1,
        0,
        &ReviewWorkspaceSurfaceOptions::default(),
        &display_rows,
    );
    let expanded_rows = surface
        .viewport
        .sections
        .iter()
        .flat_map(|section| section.rows.iter())
        .filter(|row| row.row_index == target_code_row)
        .collect::<Vec<_>>();

    assert_eq!(expanded_rows.len(), 3);
    assert_eq!(
        expanded_rows
            .iter()
            .map(|row| row.display_row_index)
            .collect::<Vec<_>>(),
        vec![target_code_row, target_code_row + 1, target_code_row + 2,]
    );
    assert_eq!(
        expanded_rows
            .iter()
            .map(|row| row.surface_top_px)
            .collect::<Vec<_>>(),
        vec![
            target_row_top_px,
            target_row_top_px + REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX,
            target_row_top_px + 2 * REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX,
        ]
    );
    assert!(
        expanded_rows
            .iter()
            .all(|row| row.height_px == REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX)
    );
    assert_eq!(surface.visible_state.top_row, Some(target_code_row));
    assert_eq!(
        surface.visible_state.top_display_row,
        Some(target_code_row + 1)
    );
    assert_eq!(
        surface.visible_state.visible_display_row_range,
        Some((target_code_row + 1)..(target_code_row + 2))
    );
}

#[test]
fn review_workspace_session_builds_display_viewport_for_expanded_rows() {
    let patch = "\
@@ -1,3 +1,3 @@
-before
+after
 keep
 tail
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/lib.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/lib.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/lib.rs", FileStatus::Modified);
    let mut session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("review workspace session should build")
        .with_render_stream(&stream);

    let left_by_raw_row = session
        .build_display_snapshot_for_side(0..session.row_count(), ReviewWorkspaceEditorSide::Left)
        .into_iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let right_by_raw_row = session
        .build_display_snapshot_for_side(0..session.row_count(), ReviewWorkspaceEditorSide::Right)
        .into_iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let target_code_row = (0..session.row_count())
        .find(|&row_ix| {
            session
                .row(row_ix)
                .is_some_and(|row| row.kind == DiffRowKind::Code)
        })
        .expect("code row");

    let mut rows = Vec::new();
    let mut left_by_display_row = BTreeMap::new();
    let mut right_by_display_row = BTreeMap::new();
    let mut next_display_row = 0usize;
    for row_ix in 0..session.row_count() {
        let repeat = if row_ix == target_code_row { 3 } else { 1 };
        let left = left_by_raw_row
            .get(&row_ix)
            .cloned()
            .expect("left display row");
        let right = right_by_raw_row
            .get(&row_ix)
            .cloned()
            .expect("right display row");
        for _ in 0..repeat {
            let mut left = left.clone();
            left.row_index = next_display_row;
            let mut right = right.clone();
            right.row_index = next_display_row;
            rows.push(ReviewWorkspaceDisplayRowEntry {
                display_row_index: next_display_row,
                row_index: row_ix,
                raw_row_range: row_ix..row_ix.saturating_add(1),
                left: left.clone(),
                right: right.clone(),
            });
            left_by_display_row.insert(next_display_row, left);
            right_by_display_row.insert(next_display_row, right);
            next_display_row = next_display_row.saturating_add(1);
        }
    }

    let display_rows = ReviewWorkspaceDisplayRows {
        rows,
        left_by_display_row,
        right_by_display_row,
        left_syntax_by_display_row: BTreeMap::new(),
        right_syntax_by_display_row: BTreeMap::new(),
    };
    session.refresh_display_geometry_from_display_rows(&display_rows);

    let target_display_range = session
        .display_row_range_for_raw_row(target_code_row)
        .expect("target display row range");
    let viewport = session
        .display_viewport_for_surface_viewport(
            session
                .row_top_offset_px(target_code_row)
                .expect("row top")
                .saturating_add(REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX),
            REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX,
            1,
        )
        .expect("display viewport");

    assert_eq!(
        viewport.first_visible_row,
        target_display_range.start.saturating_sub(1)
    );
    assert_eq!(
        viewport.visible_row_count,
        target_display_range
            .end
            .saturating_add(1)
            .min(session.display_geometry_total_display_rows())
            .saturating_sub(target_display_range.start.saturating_sub(1))
    );
}

#[test]
fn review_workspace_display_geometry_collapses_hidden_rows_from_multi_row_entries() {
    let patch = "\
@@ -1,4 +1,4 @@
-before
+after
 keep
 tail
 stay
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/lib.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/lib.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/lib.rs", FileStatus::Modified);
    let mut session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("review workspace session should build")
        .with_render_stream(&stream);
    let base_total_display_rows = session.display_geometry_total_display_rows();

    let first_code_row = (0..session.row_count())
        .find(|&row_ix| {
            session
                .row(row_ix)
                .is_some_and(|row| row.kind == DiffRowKind::Code)
        })
        .expect("first code row");
    let folded_range = first_code_row..first_code_row.saturating_add(3);
    let left = display_row(0, "… 2 hidden lines");
    let right = display_row(0, "… 2 hidden lines");
    let display_rows = ReviewWorkspaceDisplayRows {
        rows: vec![ReviewWorkspaceDisplayRowEntry {
            display_row_index: 0,
            row_index: folded_range.start,
            raw_row_range: folded_range.clone(),
            left: left.clone(),
            right: right.clone(),
        }],
        left_by_display_row: BTreeMap::from([(0, left)]),
        right_by_display_row: BTreeMap::from([(0, right)]),
        left_syntax_by_display_row: BTreeMap::new(),
        right_syntax_by_display_row: BTreeMap::new(),
    };

    session.refresh_display_geometry_from_display_rows(&display_rows);

    let folded_display_range = session
        .display_row_range_for_raw_row(folded_range.start)
        .expect("folded display range");
    assert_eq!(folded_display_range.len(), 1);
    assert_eq!(
        session.display_row_range_for_raw_row(folded_range.start + 1),
        Some(folded_display_range.end..folded_display_range.end)
    );
    assert_eq!(
        session.display_row_range_for_raw_row(folded_range.start + 2),
        Some(folded_display_range.end..folded_display_range.end)
    );
    assert_eq!(
        session.display_geometry_total_display_rows(),
        base_total_display_rows.saturating_sub(2)
    );
}

#[test]
fn review_workspace_session_surface_rows_match_current_side_by_side_shape() {
    let patch = "\
@@ -3,5 +3,4 @@
 keep one
-remove one
-remove two
+add one
 keep two
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/app.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/app.rs".to_string(), patch.to_string())]),
    };

    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build");
    let file_range = &session.file_ranges()[0];
    let expected_surface_rows = 1 + parse_patch_side_by_side(patch).len();

    assert_eq!(
        file_range.end_row - file_range.start_row,
        expected_surface_rows
    );
    assert_eq!(
        session.visible_file_header_row(file_range.start_row.saturating_add(1)),
        Some(file_range.start_row)
    );
    assert_eq!(
        session.visible_hunk_header_row(file_range.start_row.saturating_add(1)),
        Some(file_range.start_row.saturating_add(1))
    );
    assert_eq!(
        session.hunk_header_at_surface_row(file_range.start_row.saturating_add(2)),
        Some("@@ -3,5 +3,4 @@")
    );
}

#[test]
fn review_workspace_session_tracks_stable_file_scope_queries() {
    let first_patch = "\
@@ -1,2 +1,2 @@
-before
+after
 stay
";
    let second_patch = "\
@@ -10,0 +11,2 @@
+new
+tail
";
    let snapshot = CompareSnapshot {
        files: vec![
            changed_file("src/main.rs", FileStatus::Modified),
            changed_file("src/lib.rs", FileStatus::Added),
        ],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([
            ("src/main.rs".to_string(), first_patch.to_string()),
            ("src/lib.rs".to_string(), second_patch.to_string()),
        ]),
    };

    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build");
    let first_range = &session.file_ranges()[0];
    let second_range = &session.file_ranges()[1];

    assert!(session.contains_path("src/main.rs"));
    assert!(!session.contains_path("missing.rs"));
    assert_eq!(
        session
            .file_range_for_path("src/lib.rs")
            .map(|range| range.start_row),
        Some(second_range.start_row)
    );
    assert_eq!(
        session
            .file_at_or_after_surface_row(first_range.end_row)
            .map(|range| range.path.as_str()),
        Some("src/lib.rs")
    );
    assert_eq!(
        session
            .file_at_or_after_surface_row(usize::MAX)
            .map(|range| range.path.as_str()),
        Some("src/lib.rs")
    );
    assert_eq!(
        session
            .adjacent_file(Some("src/main.rs"), 1)
            .map(|range| range.path.as_str()),
        Some("src/lib.rs")
    );
    assert_eq!(
        session
            .adjacent_file(Some("src/lib.rs"), -1)
            .map(|range| range.path.as_str()),
        Some("src/main.rs")
    );
}

#[test]
fn review_workspace_session_exposes_excerpt_sections_for_surface_rendering() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
@@ -8,0 +10,2 @@
+tail
+more
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };

    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build");
    let first = session.section(0).expect("first section");
    let second = session.section(1).expect("second section");

    assert!(session.section(2).is_none());
    assert!(first.show_file_header);
    assert!(!second.show_file_header);
    assert_eq!(first.path, "src/main.rs");
    assert_eq!(second.path, "src/main.rs");
    assert_eq!(first.hunk_header.as_deref(), Some("@@ -1,2 +1,3 @@"));
    assert_eq!(second.hunk_header.as_deref(), Some("@@ -8,0 +10,2 @@"));
    assert_eq!(first.index, 0);
    assert_eq!(second.index, 1);
    assert_eq!(second.start_row, session.hunk_ranges()[1].start_row);
}

#[test]
fn review_workspace_session_tracks_section_pixel_geometry() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
@@ -8,0 +10,2 @@
+tail
+more
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };
    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);

    let first = session.section(0).expect("first section");
    let second = session.section(1).expect("second section");
    let first_pixels = session
        .section_pixel_range(0)
        .expect("first section pixel range")
        .clone();
    let second_pixels = session
        .section_pixel_range(1)
        .expect("second section pixel range")
        .clone();

    assert_eq!(first_pixels.start, 0);
    assert_eq!(second_pixels.start, first_pixels.end);
    assert_eq!(session.total_surface_height_px(), second_pixels.end);
    assert_eq!(
        session.row_top_offset_px(first.start_row),
        Some(first_pixels.start)
    );
    assert_eq!(
        session.row_top_offset_px(second.start_row),
        Some(second_pixels.start)
    );
    assert!(
        first_pixels.end
            >= REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX + REVIEW_SURFACE_HUNK_DIVIDER_HEIGHT_PX
    );
    assert_eq!(
        session.visible_section_range_for_viewport(0, first_pixels.end, 0),
        0..1
    );
    assert_eq!(
        session.visible_section_range_for_viewport(second_pixels.start, second_pixels.len(), 0),
        1..2
    );
}

#[test]
fn review_workspace_session_limits_section_rows_to_viewport_slice() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
@@ -8,0 +10,2 @@
+tail
+more
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };
    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);

    let first = session.section(0).expect("first section");
    let second = session.section(1).expect("second section");
    let first_pixels = session
        .section_pixel_range(0)
        .expect("first section pixel range")
        .clone();

    let first_visible = session
        .section_visible_row_range(0, 0, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 2, 1)
        .expect("first section visible rows");
    let second_visible = session
        .section_visible_row_range(1, 0, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 2, 1)
        .expect("second section visible rows");

    assert_eq!(session.row_boundary_offset_px(first.start_row), Some(0));
    assert_eq!(
        session.row_boundary_offset_px(first.end_row),
        Some(first_pixels.end)
    );
    assert_eq!(first_visible.start, first.start_row);
    assert!(first_visible.end < first.end_row);
    assert_eq!(second_visible.start, second.start_row);
    assert!(second_visible.end < second.end_row);
}

#[test]
fn review_workspace_session_builds_viewport_snapshot_from_shared_geometry() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
@@ -8,0 +10,2 @@
+tail
+more
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };
    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);
    let expected_first_visible_rows = session
        .section_visible_row_range(0, 0, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 2, 1)
        .expect("first section visible rows")
        .collect::<Vec<_>>();

    let viewport = session.build_viewport_snapshot(
        0,
        REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 2,
        1,
        1,
        &ReviewWorkspaceSurfaceOptions::default(),
    );

    assert_eq!(
        viewport.total_surface_height_px,
        session.total_surface_height_px()
    );
    assert_eq!(viewport.sections.len(), 2);
    assert_eq!(viewport.sections[0].pixel_range.start, 0);
    let first_visible_pixel_range = session
        .row_boundary_offset_px(expected_first_visible_rows[0])
        .expect("first visible row should have a top offset")
        ..session
            .row_boundary_offset_px(expected_first_visible_rows.last().copied().unwrap() + 1)
            .expect("last visible row boundary should exist");
    assert_eq!(
        viewport.sections[0]
            .rows
            .iter()
            .map(|row| row.row_index)
            .collect::<Vec<_>>(),
        expected_first_visible_rows,
    );
    assert_eq!(
        viewport.visible_pixel_range(),
        Some(
            viewport.sections.first().unwrap().pixel_range.start
                ..viewport.sections.last().unwrap().pixel_range.end
        ),
    );
    assert_eq!(
        first_visible_pixel_range.start,
        viewport.sections[0]
            .rows
            .first()
            .expect("first section should have rows")
            .surface_top_px,
    );
    assert!(
        session
            .row(viewport.sections[0].rows[0].row_index)
            .is_some()
    );
    assert_eq!(
        session.viewport_row_indices(0, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 2, 1, 1),
        viewport
            .sections
            .iter()
            .flat_map(|section| section.rows.iter().map(|row| row.row_index))
            .collect::<Vec<_>>(),
    );
    let code_row = viewport
        .sections
        .iter()
        .flat_map(|section| section.rows.iter())
        .find(|row| {
            session
                .row(row.row_index)
                .is_some_and(|session_row| session_row.kind == DiffRowKind::Code)
        })
        .expect("viewport should include at least one code row");
    let session_row = session
        .row(code_row.row_index)
        .expect("session row should exist for viewport code row");
    let session_row_meta = session
        .row_metadata(code_row.row_index)
        .expect("session row metadata should exist for viewport code row");
    assert_eq!(
        code_row.left_segments[0].plain_text.as_ref(),
        session_row.left.text
    );
    assert_eq!(
        code_row.right_segments[0].plain_text.as_ref(),
        session_row.right.text
    );
    assert_eq!(code_row.stable_id, session_row_meta.stable_id);
    assert_eq!(code_row.row_kind, session_row.kind);
    assert_eq!(code_row.stream_kind, session_row_meta.kind);
    assert_eq!(
        code_row.file_path.as_deref(),
        session_row_meta.file_path.as_deref()
    );
    assert_eq!(code_row.file_status, session_row_meta.file_status);
    assert_eq!(code_row.text, session_row.text);
    assert_eq!(code_row.left_cell_kind, session_row.left.kind);
    assert_eq!(code_row.left_line, session_row.left.line);
    assert_eq!(code_row.right_cell_kind, session_row.right.kind);
    assert_eq!(code_row.right_line, session_row.right.line);
    assert_eq!(
        code_row.surface_top_px,
        session
            .row_top_offset_px(code_row.row_index)
            .expect("row should have a top offset")
    );
    assert_eq!(
        viewport
            .row_by_raw_index(code_row.row_index)
            .map(|row| row.surface_top_px),
        Some(code_row.surface_top_px)
    );
    assert!(!code_row.left_segments.is_empty());
    assert!(!code_row.right_segments.is_empty());
    assert_eq!(code_row.height_px, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX);
    assert!(
        viewport.sections[0]
            .rows
            .windows(2)
            .all(|pair| pair[1].surface_top_px >= pair[0].surface_top_px + pair[0].height_px)
    );
}

#[test]
fn review_workspace_session_builds_visible_state_from_viewport() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
@@ -8,0 +10,2 @@
+tail
+more
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };
    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);

    let visible_state = session.build_visible_state(0, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 2);

    assert_eq!(
        visible_state.visible_row_range,
        session.visible_row_range_for_viewport(0, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 2),
    );
    assert_eq!(
        visible_state.top_row,
        visible_state
            .visible_row_range
            .as_ref()
            .map(|range| range.start),
    );
    assert_eq!(
        visible_state.top_display_row,
        visible_state
            .visible_display_row_range
            .as_ref()
            .map(|range| range.start),
    );
    assert_eq!(
        visible_state.visible_file_path.as_deref(),
        Some("src/main.rs")
    );
    assert_eq!(
        visible_state.visible_file_status,
        Some(FileStatus::Modified)
    );
    assert_eq!(visible_state.visible_file_header_row, Some(0));
    assert_eq!(visible_state.visible_hunk_header_row, Some(1));
    assert_eq!(
        visible_state.visible_display_row_range,
        visible_state.visible_row_range.clone(),
    );
}

#[test]
fn review_workspace_session_maps_viewport_pixels_back_to_rows() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };
    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);
    let viewport = session.build_viewport_snapshot(
        0,
        REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 3,
        1,
        1,
        &ReviewWorkspaceSurfaceOptions::default(),
    );
    let code_row = viewport
        .sections
        .iter()
        .flat_map(|section| section.rows.iter())
        .find(|row| row.row_kind == DiffRowKind::Code)
        .expect("viewport should include a code row");

    assert_eq!(
        viewport
            .row_at_viewport_position(0, code_row.surface_top_px)
            .map(|row| row.row_index),
        Some(code_row.row_index)
    );
    assert_eq!(
        viewport
            .row_at_viewport_position(0, code_row.surface_top_px + code_row.height_px / 2)
            .map(|row| row.row_index),
        Some(code_row.row_index)
    );
    assert_eq!(
        viewport
            .row_at_viewport_position(0, viewport.total_surface_height_px + 32)
            .map(|row| row.row_index),
        None
    );
}

#[test]
fn review_workspace_session_exposes_header_and_line_number_helpers() {
    let patch = "\
@@ -9,2 +10,3 @@
 before
-old
+new
 keep
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::from([(
            "src/main.rs".to_string(),
            LineStats {
                added: 2,
                removed: 1,
            },
        )]),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };
    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);

    let header = session
        .visible_file_header_at_surface_row(2)
        .expect("file header should resolve");
    assert_eq!(header.row_index, 0);
    assert_eq!(header.path, "src/main.rs");
    assert_eq!(header.status, FileStatus::Modified);
    assert_eq!(header.line_stats.added, 2);
    assert_eq!(header.line_stats.removed, 1);
    assert_eq!(session.line_number_digit_widths(), (3, 3));
}

#[test]
fn review_workspace_session_can_attach_render_rows() {
    let patch = "\
@@ -1,2 +1,2 @@
-before
+after
 stay
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let mut stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    stream.row_segments[1] = Some(app::DiffRowSegmentCache {
        quality: app::DiffSegmentQuality::Detailed,
        ..Default::default()
    });
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);

    assert_eq!(session.row_count(), session.layout().total_rows());
    assert_eq!(session.row_count(), rows.len() + 1);
    assert_eq!(session.row(0).map(|row| row.kind), Some(DiffRowKind::Meta));
    assert_eq!(
        session.row(rows.len()).map(|row| row.kind),
        rows.last().map(|row| row.kind)
    );
    assert_eq!(session.row_file_path(0), Some("src/main.rs"));
    assert!(session.row_supports_comments(2));
}

#[test]
fn review_workspace_session_prefetches_visible_code_rows_from_viewport_state() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::from([(
            "src/main.rs".to_string(),
            LineStats {
                added: 1,
                removed: 1,
            },
        )]),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let mut stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    stream.row_segments[2] = Some(app::DiffRowSegmentCache {
        quality: app::DiffSegmentQuality::Detailed,
        ..Default::default()
    });
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);

    let pending = session.build_segment_prefetch_rows(ReviewWorkspaceSegmentPrefetchRequest {
        scroll_top_px: 0,
        viewport_height_px: REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 3,
        anchor_row: 2,
        overscan_rows: 2,
        force_upgrade: false,
        recently_scrolling: false,
        batch_limit: 8,
    });

    assert!(!pending.is_empty());
    assert!(
        pending
            .iter()
            .all(|row| row.file_path.as_deref() == Some("src/main.rs"))
    );
    assert!(
        pending
            .iter()
            .all(|row| row.quality == app::DiffSegmentQuality::Detailed)
    );
    assert!(
        pending
            .iter()
            .all(|row| rows[row.row_index.saturating_sub(1)].kind == DiffRowKind::Code)
    );
    assert!(pending.iter().all(|row| row.row_index != 2));
}

#[test]
fn review_workspace_session_surface_snapshot_reuses_viewport_and_visible_state() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);
    let surface = session.build_surface_snapshot(
        0,
        REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 4,
        1,
        8,
        &ReviewWorkspaceSurfaceOptions::default(),
    );

    assert_eq!(surface.scroll_top_px, 0);
    assert_eq!(
        surface.viewport_height_px,
        REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 4
    );
    assert!(!surface.viewport.sections.is_empty());
    assert_eq!(surface.visible_state.top_row, Some(0));
    assert_eq!(
        surface.visible_state.visible_file_path.as_deref(),
        Some("src/main.rs")
    );
    assert!(surface.sticky_file_header.is_none());
    assert!(
        surface
            .visible_state
            .visible_row_range
            .as_ref()
            .is_some_and(|range| !range.is_empty())
    );
}

#[test]
fn review_workspace_session_surface_snapshot_includes_sticky_file_header_after_header_scrolls_off()
{
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);
    let surface = session.build_surface_snapshot(
        REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX,
        REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 4,
        1,
        8,
        &ReviewWorkspaceSurfaceOptions::default(),
    );

    let sticky = surface
        .sticky_file_header
        .as_ref()
        .expect("sticky file header should be present once the header row scrolls off");
    assert_eq!(sticky.row_index, 0);
    assert_eq!(sticky.path, "src/main.rs");
    assert_eq!(sticky.status, FileStatus::Modified);
}

#[test]
fn review_workspace_session_surface_snapshot_builds_sparse_overlays_from_surface_options() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old
+new
 keep
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::from([(
            "src/main.rs".to_string(),
            LineStats {
                added: 1,
                removed: 1,
            },
        )]),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };

    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);
    let comment_row = 2;
    let surface = session.build_surface_snapshot(
        0,
        REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 4,
        1,
        8,
        &ReviewWorkspaceSurfaceOptions {
            comment_affordance_rows: BTreeSet::from([comment_row]),
            comment_open_counts_by_row: BTreeMap::from([(comment_row, 1)]),
            active_comment_editor_row: Some(comment_row),
            collapsed_paths: BTreeSet::from(["src/main.rs".to_string()]),
            view_file_enabled_paths: BTreeSet::from(["src/main.rs".to_string()]),
            search_highlight_columns_by_row: BTreeMap::new(),
        },
    );

    assert_eq!(
        surface
            .active_comment_editor_overlay
            .as_ref()
            .map(|overlay| overlay.row_index),
        Some(comment_row)
    );
    assert!(surface.viewport.row_by_raw_index(0).is_some_and(|row| {
        row.stream_kind == app::DiffStreamRowKind::FileHeader
            && row.file_is_collapsed
            && row.can_view_file
    }));
    assert!(
        surface
            .viewport
            .row_by_raw_index(comment_row)
            .is_some_and(|row| row.show_comment_affordance && row.open_comment_count == 1)
    );
}

#[test]
fn review_workspace_session_can_build_editor_session_for_selected_path() {
    let first_patch = "\
@@ -1,2 +1,2 @@
-before
+after
 stay
";
    let second_patch = "\
@@ -4,0 +5,2 @@
+new
+tail
";
    let snapshot = CompareSnapshot {
        files: vec![
            changed_file("src/main.rs", FileStatus::Modified),
            changed_file("src/lib.rs", FileStatus::Added),
        ],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([
            ("src/main.rs".to_string(), first_patch.to_string()),
            ("src/lib.rs".to_string(), second_patch.to_string()),
        ]),
    };

    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build");
    let editor_session = session.build_editor_session(Some("src/lib.rs"));

    assert_eq!(
        editor_session
            .active_path()
            .map(|path| path.to_string_lossy().to_string()),
        Some("src/lib.rs".to_string())
    );
}

#[test]
fn review_workspace_session_exports_editor_documents_for_each_side() {
    let patch = "\
@@ -1,2 +1,2 @@
-before
+after
 stay
";
    let rows = parse_patch_side_by_side(patch);
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };

    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&review_stream_for_rows(
            &rows,
            "src/main.rs",
            FileStatus::Modified,
        ));

    assert_eq!(
        session.editor_documents(ReviewWorkspaceEditorSide::Left),
        vec![(
            std::path::PathBuf::from("src/main.rs"),
            "before\nstay\n".to_string(),
        )]
    );
    assert_eq!(
        session.editor_documents(ReviewWorkspaceEditorSide::Right),
        vec![(
            std::path::PathBuf::from("src/main.rs"),
            "after\nstay\n".to_string(),
        )]
    );
}

#[test]
fn review_workspace_session_prefers_higher_quality_segment_upgrades() {
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), String::new())]),
    };
    let mut stream = review_stream_for_rows(&[], "src/main.rs", FileStatus::Modified);
    stream.row_segments[0] = Some(app::DiffRowSegmentCache {
        quality: app::DiffSegmentQuality::Plain,
        ..Default::default()
    });
    let mut session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);

    assert!(session.set_row_segment_cache_if_better(
        0,
        app::DiffRowSegmentCache {
            quality: app::DiffSegmentQuality::Detailed,
            ..Default::default()
        },
    ));
    assert_eq!(
        session.row_segment_cache(0).map(|cache| cache.quality),
        Some(app::DiffSegmentQuality::Detailed)
    );
    assert!(!session.set_row_segment_cache_if_better(
        0,
        app::DiffRowSegmentCache {
            quality: app::DiffSegmentQuality::Plain,
            ..Default::default()
        },
    ));
}

#[test]
fn collapsed_review_workspace_files_keep_compact_surface_ranges() {
    let patch = "\
@@ -1,2 +1,2 @@
-before
+after
 stay
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };
    let collapsed = BTreeSet::from(["src/main.rs".to_string()]);

    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &collapsed)
        .expect("workspace session should build");
    let file_range = &session.file_ranges()[0];

    assert_eq!(file_range.end_row - file_range.start_row, 2);
    assert_eq!(
        session.path_at_surface_row(file_range.start_row),
        Some("src/main.rs")
    );
    assert_eq!(
        session.path_at_surface_row(file_range.end_row.saturating_sub(1)),
        Some("src/main.rs")
    );
    assert_eq!(session.layout().excerpts().len(), 1);
}

#[test]
fn review_workspace_session_builds_comment_anchors_from_render_rows() {
    let patch = "\
@@ -1,2 +1,3 @@
 before
-old line
+new line
 keep
+tail
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };
    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);
    let (anchors, rows_by_path) = session.build_comment_anchor_index(2);

    assert_eq!(rows_by_path.get("src/main.rs").map(Vec::len), Some(4));
    let added_row_ix = rows
        .iter()
        .position(|row| row.right.text == "new line")
        .map(|ix| ix + 1)
        .expect("added row should exist");
    let anchor = anchors
        .get(&added_row_ix)
        .expect("anchor should exist for added row");
    assert_eq!(anchor.file_path, "src/main.rs");
    assert!(anchor.line_text.contains("+new line"));
    assert_eq!(anchor.hunk_header.as_deref(), Some("@@ -1,2 +1,3 @@"));
}

#[test]
fn review_workspace_display_rows_require_complete_left_and_right_coverage() {
    let full = ReviewWorkspaceDisplayRows {
        rows: vec![
            ReviewWorkspaceDisplayRowEntry {
                display_row_index: 3,
                row_index: 3,
                raw_row_range: 3..4,
                left: display_row(3, "left-a"),
                right: display_row(3, "right-a"),
            },
            ReviewWorkspaceDisplayRowEntry {
                display_row_index: 4,
                row_index: 4,
                raw_row_range: 4..5,
                left: display_row(4, "left-b"),
                right: display_row(4, "right-b"),
            },
        ],
        left_by_display_row: BTreeMap::from([
            (3, display_row(3, "left-a")),
            (4, display_row(4, "left-b")),
        ]),
        right_by_display_row: BTreeMap::from([
            (3, display_row(3, "right-a")),
            (4, display_row(4, "right-b")),
        ]),
        left_syntax_by_display_row: BTreeMap::new(),
        right_syntax_by_display_row: BTreeMap::new(),
    };
    assert!(full.covers_row_range(3..5));

    let missing_right = ReviewWorkspaceDisplayRows {
        rows: Vec::new(),
        left_by_display_row: full.left_by_display_row.clone(),
        right_by_display_row: BTreeMap::from([(3, display_row(3, "right-a"))]),
        left_syntax_by_display_row: BTreeMap::new(),
        right_syntax_by_display_row: BTreeMap::new(),
    };
    assert!(!missing_right.covers_row_range(3..5));
}

#[test]
fn review_workspace_surface_snapshot_preserves_display_row_identity() {
    let patch = "\
@@ -1 +1 @@
-old
+new
";
    let snapshot = CompareSnapshot {
        files: vec![changed_file("src/main.rs", FileStatus::Modified)],
        file_line_stats: BTreeMap::new(),
        overall_line_stats: LineStats::default(),
        patches_by_path: BTreeMap::from([("src/main.rs".to_string(), patch.to_string())]),
    };
    let rows = parse_patch_side_by_side(patch);
    let stream = review_stream_for_rows(&rows, "src/main.rs", FileStatus::Modified);
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("review workspace session should build")
        .with_render_stream(&stream);
    let row_range = 0..session.row_count();
    let left_by_row = session
        .build_display_snapshot_for_side(row_range.clone(), ReviewWorkspaceEditorSide::Left)
        .into_iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let right_by_row = session
        .build_display_snapshot_for_side(row_range, ReviewWorkspaceEditorSide::Right)
        .into_iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let first_row_index = *left_by_row.keys().next().unwrap();
    let mut rows = left_by_row
        .iter()
        .filter_map(|(row_index, left)| {
            Some(ReviewWorkspaceDisplayRowEntry {
                display_row_index: *row_index,
                row_index: *row_index,
                raw_row_range: *row_index..row_index.saturating_add(1),
                left: left.clone(),
                right: right_by_row.get(row_index)?.clone(),
            })
        })
        .collect::<Vec<_>>();
    for (ix, row) in rows.iter_mut().enumerate() {
        row.display_row_index = 42 + ix;
    }
    let display_rows = ReviewWorkspaceDisplayRows {
        rows,
        left_by_display_row: left_by_row,
        right_by_display_row: right_by_row,
        left_syntax_by_display_row: BTreeMap::new(),
        right_syntax_by_display_row: BTreeMap::new(),
    };

    let surface = session.build_surface_snapshot_with_display_rows(
        0,
        256,
        1,
        8,
        &ReviewWorkspaceSurfaceOptions::default(),
        &display_rows,
    );

    let viewport_row = surface
        .viewport
        .sections
        .iter()
        .find_map(|section| section.rows.first())
        .expect("surface snapshot should contain the injected display row");
    assert_eq!(viewport_row.display_row_index, 42);
    assert_eq!(viewport_row.row_index, first_row_index);
    assert_eq!(surface.visible_state.top_display_row, Some(42));
    assert_eq!(
        surface
            .visible_state
            .visible_display_row_range
            .as_ref()
            .map(|range| range.start),
        Some(42)
    );
}

fn display_row(row_index: usize, text: &str) -> WorkspaceDisplayRow {
    WorkspaceDisplayRow {
        row_index,
        location: None,
        raw_start_column: 0,
        raw_end_column: text.len(),
        raw_column_offsets: (0..=text.len()).collect(),
        text: text.to_string(),
        whitespace_markers: Vec::new(),
        search_highlights: Vec::new(),
    }
}
