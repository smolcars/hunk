#[path = "../src/app/refresh_policy.rs"]
mod refresh_policy;

use std::collections::BTreeSet;

use hunk_git::git::{ChangedFile, FileStatus, LineStats};
use refresh_policy::{
    SnapshotRefreshBehavior, SnapshotRefreshPriority, SnapshotRefreshRequest, diff_state_changed,
    line_stats_paths_from_dirty_paths, missing_line_stat_paths, repo_watch_refresh_request,
    should_refresh_line_stats_after_snapshot, should_reload_diff_after_snapshot,
    should_reload_repo_tree_after_snapshot, should_run_cold_start_reconcile,
    should_scroll_selected_after_reload,
};

#[test]
fn watcher_prefers_working_copy_refresh_for_dirty_paths() {
    let request =
        repo_watch_refresh_request(true, true).expect("dirty paths should schedule a refresh");

    assert_eq!(
        request,
        SnapshotRefreshRequest::background_refresh_working_copy()
    );
}

#[test]
fn metadata_only_watcher_refresh_stays_read_only() {
    let request = repo_watch_refresh_request(true, false)
        .expect("metadata changes should schedule a refresh");

    assert_eq!(request, SnapshotRefreshRequest::background());
}

#[test]
fn merged_refresh_requests_keep_the_more_comprehensive_behavior() {
    let merged = SnapshotRefreshRequest::background()
        .merge(SnapshotRefreshRequest::background_refresh_working_copy());

    assert_eq!(
        merged,
        SnapshotRefreshRequest::background_refresh_working_copy()
    );
}

#[test]
fn request_priority_strings_and_urgency_are_stable() {
    assert_eq!(SnapshotRefreshPriority::Background.as_str(), "background");
    assert_eq!(SnapshotRefreshPriority::UserInitiated.as_str(), "user");
    assert_eq!(SnapshotRefreshBehavior::ReadOnly.as_str(), "read-only");
    assert_eq!(
        SnapshotRefreshBehavior::RefreshWorkingCopy.as_str(),
        "refresh-working-copy"
    );
    assert!(
        SnapshotRefreshRequest::user(false)
            .is_more_urgent_than(SnapshotRefreshRequest::background())
    );
}

#[test]
fn diff_reload_requires_real_diff_state_change_or_empty_rows() {
    assert!(diff_state_changed(false, true, false));
    assert!(should_reload_diff_after_snapshot(true, true, false));
    assert!(!should_reload_diff_after_snapshot(true, false, false));
    assert!(should_reload_diff_after_snapshot(true, false, true));
    assert!(!should_reload_diff_after_snapshot(false, true, true));
}

#[test]
fn repo_tree_reload_only_tracks_file_list_changes_or_root_switches() {
    assert!(should_reload_repo_tree_after_snapshot(true, false, false));
    assert!(should_reload_repo_tree_after_snapshot(false, true, true));
    assert!(!should_reload_repo_tree_after_snapshot(false, true, false));
}

#[test]
fn selected_file_scroll_reset_only_happens_on_selection_change_or_initial_load() {
    assert!(should_scroll_selected_after_reload(true, false));
    assert!(should_scroll_selected_after_reload(false, true));
    assert!(!should_scroll_selected_after_reload(false, false));
}

#[test]
fn cold_start_reconcile_only_runs_for_mutating_refreshes() {
    assert!(should_run_cold_start_reconcile(
        true,
        true,
        SnapshotRefreshBehavior::RefreshWorkingCopy,
    ));
    assert!(!should_run_cold_start_reconcile(
        true,
        true,
        SnapshotRefreshBehavior::ReadOnly,
    ));
    assert!(!should_run_cold_start_reconcile(
        false,
        true,
        SnapshotRefreshBehavior::RefreshWorkingCopy,
    ));
}

#[test]
fn line_stats_refresh_requires_real_diff_state_changes() {
    assert!(!should_refresh_line_stats_after_snapshot(
        SnapshotRefreshRequest::user(false),
        false,
    ));
    assert!(should_refresh_line_stats_after_snapshot(
        SnapshotRefreshRequest::user(false),
        true,
    ));
    assert!(!should_refresh_line_stats_after_snapshot(
        SnapshotRefreshRequest::background(),
        true,
    ));
    assert!(should_refresh_line_stats_after_snapshot(
        SnapshotRefreshRequest::background_refresh_working_copy(),
        true,
    ));
}

#[test]
fn dirty_path_matching_supports_exact_and_directory_prefix_hits() {
    let files = vec![
        ChangedFile {
            path: "src/lib.rs".to_string(),
            status: FileStatus::Modified,
            staged: false,
            untracked: false,
        },
        ChangedFile {
            path: "src/nested/util.rs".to_string(),
            status: FileStatus::Modified,
            staged: false,
            untracked: false,
        },
        ChangedFile {
            path: "README.md".to_string(),
            status: FileStatus::Modified,
            staged: false,
            untracked: false,
        },
    ];
    let dirty_paths = BTreeSet::from([
        String::from("src"),
        String::from("README.md"),
        String::from("missing.txt"),
    ]);

    let matched = line_stats_paths_from_dirty_paths(&files, &dirty_paths);

    assert_eq!(
        matched,
        BTreeSet::from([
            String::from("README.md"),
            String::from("src/lib.rs"),
            String::from("src/nested/util.rs"),
        ])
    );
}

#[test]
fn missing_line_stats_only_returns_changed_files_without_cached_stats() {
    let files = vec![
        ChangedFile {
            path: "src/lib.rs".to_string(),
            status: FileStatus::Modified,
            staged: false,
            untracked: false,
        },
        ChangedFile {
            path: "README.md".to_string(),
            status: FileStatus::Added,
            staged: false,
            untracked: true,
        },
    ];
    let file_line_stats = std::collections::BTreeMap::from([(
        String::from("src/lib.rs"),
        LineStats {
            added: 1,
            removed: 1,
        },
    )]);

    let missing = missing_line_stat_paths(&files, &file_line_stats);

    assert_eq!(missing, BTreeSet::from([String::from("README.md")]));
}
