#[path = "../src/app/refresh_policy.rs"]
mod refresh_policy;

use std::collections::BTreeSet;
use std::path::PathBuf;

use hunk_git::git::{ChangedFile, FileStatus, LineStats};
use refresh_policy::{
    GitWorkspaceRefreshRequest, SnapshotRefreshBehavior, SnapshotRefreshPriority,
    SnapshotRefreshRequest, diff_state_changed, line_stats_paths_from_dirty_paths,
    missing_line_stat_paths, post_git_action_refresh_plan, repo_watch_refresh_request,
    should_bootstrap_empty_files_workspace_editor, should_refresh_line_stats_after_snapshot,
    should_reload_diff_after_snapshot, should_reload_empty_files_workspace_tree,
    should_reload_repo_tree_after_snapshot, should_request_startup_git_workspace_refresh,
    should_run_cold_start_reconcile, should_scroll_selected_after_reload,
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
fn empty_files_workspace_tree_reload_only_happens_in_files_view() {
    assert!(should_reload_empty_files_workspace_tree(true, true, false));
    assert!(!should_reload_empty_files_workspace_tree(
        true, false, false
    ));
    assert!(!should_reload_empty_files_workspace_tree(true, true, true));
    assert!(!should_reload_empty_files_workspace_tree(
        false, true, false
    ));
}

#[test]
fn empty_files_workspace_editor_bootstrap_runs_only_once() {
    assert!(should_bootstrap_empty_files_workspace_editor(
        true, true, false
    ));
    assert!(!should_bootstrap_empty_files_workspace_editor(
        true, false, false
    ));
    assert!(!should_bootstrap_empty_files_workspace_editor(
        true, true, true
    ));
    assert!(!should_bootstrap_empty_files_workspace_editor(
        false, true, false
    ));
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
            unstaged: true,
            untracked: false,
        },
        ChangedFile {
            path: "src/nested/util.rs".to_string(),
            status: FileStatus::Modified,
            staged: false,
            unstaged: true,
            untracked: false,
        },
        ChangedFile {
            path: "README.md".to_string(),
            status: FileStatus::Modified,
            staged: false,
            unstaged: true,
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
            unstaged: true,
            untracked: false,
        },
        ChangedFile {
            path: "README.md".to_string(),
            status: FileStatus::Added,
            staged: false,
            unstaged: true,
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

#[test]
fn create_commit_actions_reuse_optimistic_recent_commit_state() {
    let primary_plan = post_git_action_refresh_plan("Create commit", true);
    assert!(primary_plan.refresh_primary_snapshot);
    assert!(!primary_plan.refresh_git_workspace);
    assert!(!primary_plan.refresh_recent_commits);

    let worktree_plan = post_git_action_refresh_plan("Create commit", false);
    assert!(!worktree_plan.refresh_primary_snapshot);
    assert!(worktree_plan.refresh_git_workspace);
    assert!(!worktree_plan.refresh_recent_commits);
}

#[test]
fn primary_git_actions_refresh_only_the_primary_snapshot() {
    let plan = post_git_action_refresh_plan("Undo file changes", true);

    assert!(plan.refresh_primary_snapshot);
    assert!(!plan.refresh_git_workspace);
    assert!(!plan.refresh_recent_commits);
}

#[test]
fn worktree_branch_switches_refresh_workspace_and_recent_commits() {
    let plan = post_git_action_refresh_plan("Activate branch", false);

    assert!(!plan.refresh_primary_snapshot);
    assert!(plan.refresh_git_workspace);
    assert!(plan.refresh_recent_commits);
}

#[test]
fn git_workspace_refresh_requests_merge_same_root_metadata() {
    let repo_root = PathBuf::from("/tmp/repo");
    let merged = GitWorkspaceRefreshRequest::new(repo_root.clone(), false)
        .merge(GitWorkspaceRefreshRequest::new(repo_root, true));

    assert!(merged.refresh_recent_commits);
}

#[test]
fn git_workspace_refresh_requests_replace_old_root_when_target_changes() {
    let merged = GitWorkspaceRefreshRequest::new(PathBuf::from("/tmp/repo"), true).merge(
        GitWorkspaceRefreshRequest::new(PathBuf::from("/tmp/repo-worktree"), false),
    );

    assert_eq!(merged.root, PathBuf::from("/tmp/repo-worktree"));
    assert!(!merged.refresh_recent_commits);
}

#[test]
fn startup_git_workspace_refresh_only_runs_for_non_primary_targets() {
    assert!(!should_request_startup_git_workspace_refresh(true));
    assert!(should_request_startup_git_workspace_refresh(false));
}
