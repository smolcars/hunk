mod app {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
    pub enum DiffSegmentQuality {
        #[default]
        Plain,
        Detailed,
    }

    #[derive(Debug, Clone, Default)]
    pub struct DiffRowSegmentCache {
        pub quality: DiffSegmentQuality,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    pub struct DiffStreamRowMeta {
        pub stable_id: u64,
    }

    pub mod data {
        use super::{DiffRowSegmentCache, DiffStreamRowMeta};
        use hunk_domain::diff::SideBySideRow;

        #[derive(Debug, Clone, Default)]
        pub struct DiffStream {
            pub rows: Vec<SideBySideRow>,
            pub row_metadata: Vec<DiffStreamRowMeta>,
            pub row_segments: Vec<Option<DiffRowSegmentCache>>,
        }
    }
}

#[allow(dead_code)]
#[path = "../src/app/review_workspace_session.rs"]
mod review_workspace_session;

use std::collections::{BTreeMap, BTreeSet};

use hunk_domain::diff::parse_patch_side_by_side;
use hunk_git::compare::CompareSnapshot;
use hunk_git::git::{ChangedFile, FileStatus, LineStats};
use review_workspace_session::ReviewWorkspaceSession;

fn changed_file(path: &str, status: FileStatus) -> ChangedFile {
    ChangedFile {
        path: path.to_string(),
        status,
        staged: false,
        unstaged: false,
        untracked: false,
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
    let stream = app::data::DiffStream {
        rows: rows.clone(),
        row_metadata: vec![
            app::DiffStreamRowMeta { stable_id: 1 },
            app::DiffStreamRowMeta { stable_id: 2 },
            app::DiffStreamRowMeta { stable_id: 3 },
        ],
        row_segments: vec![
            None,
            Some(app::DiffRowSegmentCache {
                quality: app::DiffSegmentQuality::Detailed,
            }),
            None,
        ],
    };
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_render_stream(&stream);

    assert_eq!(session.row_count(), rows.len());
    assert_eq!(
        session.row(0).map(|row| row.kind),
        rows.first().map(|row| row.kind)
    );
    assert_eq!(
        session
            .row(rows.len().saturating_sub(1))
            .map(|row| row.kind),
        rows.last().map(|row| row.kind)
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
    let stream = app::data::DiffStream {
        rows: Vec::new(),
        row_metadata: Vec::new(),
        row_segments: vec![Some(app::DiffRowSegmentCache {
            quality: app::DiffSegmentQuality::Plain,
        })],
    };
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
