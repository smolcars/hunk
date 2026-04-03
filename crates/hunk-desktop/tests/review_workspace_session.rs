mod app {
    #[derive(Debug, Clone, Default)]
    pub struct DiffRowSegmentCache;

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
    let session = ReviewWorkspaceSession::from_compare_snapshot(&snapshot, &BTreeSet::new())
        .expect("workspace session should build")
        .with_test_render_rows(rows.clone());

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
