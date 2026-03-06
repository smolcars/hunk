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
