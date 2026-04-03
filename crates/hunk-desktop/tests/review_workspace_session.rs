#[allow(dead_code)]
#[path = "../src/app/native_files_editor_workspace.rs"]
mod workspace_editor_session;

mod app {
    use hunk_git::git::FileStatus;

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
        use super::{DiffRowSegmentCache, DiffStreamRowMeta};
        use hunk_domain::diff::SideBySideRow;

        pub use super::DiffSegmentQuality;

        #[derive(Debug, Clone, Default)]
        pub struct DiffStream {
            pub rows: Vec<SideBySideRow>,
            pub row_metadata: Vec<DiffStreamRowMeta>,
            pub row_segments: Vec<Option<DiffRowSegmentCache>>,
        }

        pub use super::DiffStreamRowKind;
    }

    pub mod native_files_editor {
        pub(crate) use crate::workspace_editor_session::WorkspaceEditorSession;
    }
}

#[allow(dead_code)]
#[path = "../src/app/review_workspace_session.rs"]
mod review_workspace_session;

use std::collections::{BTreeMap, BTreeSet};

use hunk_domain::diff::{
    DiffCell, DiffCellKind, DiffRowKind, SideBySideRow, parse_patch_side_by_side,
};
use hunk_git::compare::CompareSnapshot;
use hunk_git::git::{ChangedFile, FileStatus, LineStats};
use review_workspace_session::{
    REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX, REVIEW_SURFACE_HUNK_DIVIDER_HEIGHT_PX,
    ReviewWorkspaceSegmentPrefetchRequest, ReviewWorkspaceSession,
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
    let expected_first_visible = session
        .section_visible_row_range(0, 0, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 2, 1)
        .expect("first section visible rows");
    let expected_first_visible_rows = expected_first_visible.clone().collect::<Vec<_>>();

    let viewport =
        session.build_viewport_snapshot(0, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 2, 1, 1);

    assert_eq!(
        viewport.total_surface_height_px,
        session.total_surface_height_px()
    );
    assert_eq!(viewport.sections.len(), 2);
    assert_eq!(viewport.sections[0].section_index, 0);
    assert_eq!(viewport.sections[0].pixel_range.start, 0);
    assert_eq!(
        viewport.sections[0].visible_row_range,
        expected_first_visible,
    );
    assert_eq!(
        viewport.sections[0].top_spacer_height_px,
        session
            .row_boundary_offset_px(viewport.sections[0].visible_row_range.start)
            .unwrap_or(viewport.sections[0].pixel_range.start)
            .saturating_sub(viewport.sections[0].pixel_range.start),
    );
    assert!(viewport.sections[0].bottom_spacer_height_px > 0);
    assert_eq!(
        viewport.sections[0]
            .rows
            .iter()
            .map(|row| row.row_index)
            .collect::<Vec<_>>(),
        expected_first_visible_rows,
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
    assert_eq!(code_row.left_display_row.text, session_row.left.text);
    assert_eq!(code_row.right_display_row.text, session_row.right.text);
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
    let visible_start_px = session
        .row_boundary_offset_px(viewport.sections[0].visible_row_range.start)
        .expect("visible range should have a top offset");
    assert_eq!(
        code_row.local_top_px,
        session
            .row_top_offset_px(code_row.row_index)
            .expect("code row should have a top offset")
            .saturating_sub(visible_start_px)
    );
    assert_eq!(code_row.height_px, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX);
    assert!(
        viewport.sections[0]
            .rows
            .windows(2)
            .all(|pair| pair[1].local_top_px >= pair[0].local_top_px + pair[0].height_px)
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
        visible_state.visible_file_path.as_deref(),
        Some("src/main.rs")
    );
    assert_eq!(
        visible_state.visible_file_status,
        Some(FileStatus::Modified)
    );
    assert_eq!(visible_state.visible_file_header_row, Some(0));
    assert_eq!(visible_state.visible_hunk_header_row, Some(1));
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
    let surface = session.build_surface_snapshot(0, REVIEW_SURFACE_COMPACT_ROW_HEIGHT_PX * 4, 1, 8);

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
    assert!(
        surface
            .visible_state
            .visible_row_range
            .as_ref()
            .is_some_and(|range| !range.is_empty())
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
    });
    let mut session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);

    assert!(session.set_row_segment_cache_if_better(
        0,
        app::DiffRowSegmentCache {
            quality: app::DiffSegmentQuality::Detailed,
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
