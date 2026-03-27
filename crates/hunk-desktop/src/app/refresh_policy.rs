use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hunk_git::git::{ChangedFile, LineStats};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SnapshotRefreshPriority {
    Background,
    UserInitiated,
}

impl SnapshotRefreshPriority {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::UserInitiated => "user",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SnapshotRefreshBehavior {
    ReadOnly,
    RefreshWorkingCopy,
}

impl SnapshotRefreshBehavior {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::RefreshWorkingCopy => "refresh-working-copy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SnapshotRefreshRequest {
    pub(super) force: bool,
    pub(super) priority: SnapshotRefreshPriority,
    pub(super) behavior: SnapshotRefreshBehavior,
}

impl SnapshotRefreshRequest {
    pub(super) const fn user(force: bool) -> Self {
        Self {
            force,
            priority: SnapshotRefreshPriority::UserInitiated,
            behavior: SnapshotRefreshBehavior::RefreshWorkingCopy,
        }
    }

    pub(super) const fn background() -> Self {
        Self {
            force: false,
            priority: SnapshotRefreshPriority::Background,
            behavior: SnapshotRefreshBehavior::ReadOnly,
        }
    }

    pub(super) const fn background_refresh_working_copy() -> Self {
        Self {
            force: false,
            priority: SnapshotRefreshPriority::Background,
            behavior: SnapshotRefreshBehavior::RefreshWorkingCopy,
        }
    }

    pub(super) fn merge(self, other: Self) -> Self {
        Self {
            force: self.force || other.force,
            priority: if self.priority >= other.priority {
                self.priority
            } else {
                other.priority
            },
            behavior: if self.behavior >= other.behavior {
                self.behavior
            } else {
                other.behavior
            },
        }
    }

    pub(super) fn is_more_urgent_than(self, other: Self) -> bool {
        self.priority > other.priority
            || (self.priority == other.priority && self.behavior > other.behavior)
            || (self.priority == other.priority
                && self.behavior == other.behavior
                && self.force
                && !other.force)
    }
}

pub(super) const fn repo_watch_refresh_request(
    metadata_changed: bool,
    has_dirty_paths: bool,
) -> Option<SnapshotRefreshRequest> {
    if has_dirty_paths {
        return Some(SnapshotRefreshRequest::background_refresh_working_copy());
    }
    if metadata_changed {
        return Some(SnapshotRefreshRequest::background());
    }
    None
}

pub(super) const fn should_refresh_line_stats_after_snapshot(
    request: SnapshotRefreshRequest,
    diff_state_changed: bool,
) -> bool {
    diff_state_changed
        && !matches!(
            (request.priority, request.behavior),
            (
                SnapshotRefreshPriority::Background,
                SnapshotRefreshBehavior::ReadOnly
            )
        )
}

pub(super) const fn diff_state_changed(
    root_changed: bool,
    working_copy_commit_changed: bool,
    file_list_changed: bool,
) -> bool {
    root_changed || working_copy_commit_changed || file_list_changed
}

pub(super) const fn should_reload_diff_after_snapshot(
    supports_diff_stream: bool,
    diff_state_changed: bool,
    diff_rows_empty: bool,
) -> bool {
    supports_diff_stream && (diff_state_changed || diff_rows_empty)
}

pub(super) const fn should_scroll_selected_after_reload(
    selected_changed: bool,
    diff_rows_empty: bool,
) -> bool {
    selected_changed || diff_rows_empty
}

pub(super) const fn should_reload_empty_files_workspace_tree(
    files_view_active: bool,
    repo_tree_empty: bool,
    repo_tree_loading: bool,
) -> bool {
    files_view_active && repo_tree_empty && !repo_tree_loading
}

pub(super) const fn should_bootstrap_empty_files_workspace_editor(
    files_view_active: bool,
    editor_missing: bool,
    editor_loading: bool,
) -> bool {
    files_view_active && editor_missing && !editor_loading
}

pub(super) const fn should_reload_repo_tree_after_snapshot(
    root_changed: bool,
    supports_sidebar_tree: bool,
    file_list_changed: bool,
) -> bool {
    root_changed || (supports_sidebar_tree && file_list_changed)
}

pub(super) const fn should_run_cold_start_reconcile(
    cold_start: bool,
    loaded_without_refresh: bool,
    behavior: SnapshotRefreshBehavior,
) -> bool {
    cold_start
        && loaded_without_refresh
        && matches!(behavior, SnapshotRefreshBehavior::RefreshWorkingCopy)
}

pub(super) const fn should_request_startup_git_workspace_refresh(
    selected_target_is_primary: bool,
) -> bool {
    !selected_target_is_primary
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GitActionRefreshPlan {
    pub(super) refresh_primary_snapshot: bool,
    pub(super) refresh_git_workspace: bool,
    pub(super) refresh_recent_commits: bool,
}

pub(super) const fn git_action_refresh_plan(
    selected_root_is_primary: bool,
    refresh_recent_commits: bool,
) -> GitActionRefreshPlan {
    GitActionRefreshPlan {
        refresh_primary_snapshot: selected_root_is_primary,
        refresh_git_workspace: !selected_root_is_primary,
        refresh_recent_commits,
    }
}

pub(super) fn post_git_action_refresh_plan(
    action_name: &str,
    selected_root_is_primary: bool,
) -> GitActionRefreshPlan {
    git_action_refresh_plan(
        selected_root_is_primary,
        matches!(action_name, "Activate branch" | "Sync branch"),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GitWorkspaceRefreshRequest {
    pub(super) root: PathBuf,
    pub(super) refresh_recent_commits: bool,
}

impl GitWorkspaceRefreshRequest {
    pub(super) fn new(root: PathBuf, refresh_recent_commits: bool) -> Self {
        Self {
            root,
            refresh_recent_commits,
        }
    }

    pub(super) fn merge(self, newer: Self) -> Self {
        if self.root == newer.root {
            Self {
                root: newer.root,
                refresh_recent_commits: self.refresh_recent_commits || newer.refresh_recent_commits,
            }
        } else {
            newer
        }
    }
}

pub(super) fn missing_line_stat_paths(
    files: &[ChangedFile],
    file_line_stats: &BTreeMap<String, LineStats>,
) -> BTreeSet<String> {
    files
        .iter()
        .filter(|file| !file_line_stats.contains_key(file.path.as_str()))
        .map(|file| file.path.clone())
        .collect()
}

pub(super) fn line_stats_paths_from_dirty_paths(
    files: &[ChangedFile],
    pending_dirty_paths: &BTreeSet<String>,
) -> BTreeSet<String> {
    if pending_dirty_paths.is_empty() {
        return BTreeSet::new();
    }

    files
        .iter()
        .filter(|file| {
            pending_dirty_paths.iter().any(|dirty_path| {
                file.path == *dirty_path
                    || file
                        .path
                        .strip_prefix(dirty_path.as_str())
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
        })
        .map(|file| file.path.clone())
        .collect()
}
