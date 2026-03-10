use std::path::Path;

use anyhow::{Context as _, Result};
use git2::{Repository, StatusOptions, Statuses};

pub(crate) fn open_git2_repo(repo_root: &Path) -> Result<Repository> {
    Repository::open(repo_root)
        .with_context(|| format!("failed to open Git repository at {}", repo_root.display()))
}

pub(crate) fn standard_status_options() -> StatusOptions {
    let mut status_options = git2::StatusOptions::new();
    status_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .renames_head_to_index(false)
        .renames_index_to_workdir(false);
    status_options
}

pub(crate) fn load_statuses<'repo>(
    repo: &'repo Repository,
    context: impl FnOnce() -> String,
) -> Result<Statuses<'repo>> {
    let mut status_options = standard_status_options();
    repo.statuses(Some(&mut status_options))
        .with_context(context)
}
