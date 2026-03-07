use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context as _, Result, anyhow};

use crate::branch::is_valid_branch_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorktreeChange {
    AddOrUpdate,
    Remove,
}

// `gix` is still the primary backend for the hot read path. We isolate local worktree/index
// mutation here until it exposes a stable public checkout/index-editing surface we can rely on.
pub fn activate_or_create_branch(
    repo_root: &Path,
    branch_name: &str,
    move_changes_to_branch: bool,
) -> Result<()> {
    if move_changes_to_branch {
        return Err(anyhow!(
            "moving changes during branch switch is not supported"
        ));
    }

    let branch_name = branch_name.trim();
    if branch_name.is_empty() {
        return Err(anyhow!("branch name cannot be empty"));
    }
    if !is_valid_branch_name(branch_name) {
        return Err(anyhow!("invalid branch name: {branch_name}"));
    }

    let repo = open_repo(repo_root)?;
    ensure_no_hidden_index_changes(
        &repo,
        "switching branches with staged index changes is not supported",
    )?;
    if has_any_worktree_changes(&repo)? {
        return Err(anyhow!(
            "commit or discard working tree changes before switching branches"
        ));
    }

    if repo
        .find_branch(branch_name, git2::BranchType::Local)
        .is_err()
        && let Some(head_commit) = current_head_commit(&repo)?
    {
        repo.branch(branch_name, &head_commit, false)
            .with_context(|| format!("failed to create branch '{branch_name}'"))?;
    }

    let target_ref = format!("refs/heads/{branch_name}");
    repo.set_head(target_ref.as_str())
        .with_context(|| format!("failed to activate branch '{branch_name}'"))?;

    if repo
        .find_branch(branch_name, git2::BranchType::Local)
        .is_ok()
    {
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force();
        repo.checkout_head(Some(&mut checkout))
            .with_context(|| format!("failed to check out branch '{branch_name}'"))?;
    }

    Ok(())
}

pub fn commit_all(repo_root: &Path, message: &str) -> Result<()> {
    commit_paths_internal(repo_root, message, None).map(|_| ())
}

pub fn commit_selected_paths(
    repo_root: &Path,
    message: &str,
    selected_paths: &[String],
) -> Result<usize> {
    let selected_paths = normalize_selected_paths(selected_paths);
    if selected_paths.is_empty() {
        return Err(anyhow!("no files selected for commit"));
    }

    commit_paths_internal(repo_root, message, Some(&selected_paths))
}

pub fn restore_working_copy_paths(repo_root: &Path, paths: &[String]) -> Result<usize> {
    let selected_paths = normalize_selected_paths(paths);
    if selected_paths.is_empty() {
        return Err(anyhow!("no files selected to restore"));
    }

    let repo = open_repo(repo_root)?;
    let head_tree = current_head_tree(&repo)?;
    let mut tracked_paths = Vec::new();
    let mut restored_count = 0usize;

    for path in selected_paths {
        let full_path = repo_root.join(path.as_str());
        let tracked_in_head = head_tree
            .as_ref()
            .and_then(|tree| tree.get_path(Path::new(path.as_str())).ok())
            .is_some();
        if tracked_in_head {
            tracked_paths.push(path);
            continue;
        }

        remove_path_from_index_if_present(&repo, path.as_str())?;
        if !full_path.exists() {
            continue;
        }
        remove_worktree_path(full_path.as_path())?;
        restored_count += 1;
    }

    if !tracked_paths.is_empty() {
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force();
        for path in &tracked_paths {
            checkout.path(path.as_str());
        }
        repo.checkout_head(Some(&mut checkout))
            .context("failed to restore tracked paths from HEAD")?;
        restored_count += tracked_paths.len();
    }

    Ok(restored_count)
}

fn commit_paths_internal(
    repo_root: &Path,
    message: &str,
    selected_paths: Option<&BTreeSet<String>>,
) -> Result<usize> {
    let message = message.trim();
    if message.is_empty() {
        return Err(anyhow!("commit message cannot be empty"));
    }

    let repo = open_repo(repo_root)?;
    ensure_no_hidden_index_changes(
        &repo,
        "committing with staged index changes is not supported",
    )?;
    let changes = collect_worktree_changes(&repo, selected_paths)?;
    if changes.is_empty() {
        return Err(anyhow!("no changes to commit"));
    }

    stage_changes(&repo, &changes)?;
    create_commit_from_index(&repo, message)?;
    Ok(changes.len())
}

fn open_repo(repo_root: &Path) -> Result<git2::Repository> {
    git2::Repository::open(repo_root)
        .with_context(|| format!("failed to open Git repository at {}", repo_root.display()))
}

fn has_any_worktree_changes(repo: &git2::Repository) -> Result<bool> {
    Ok(!collect_worktree_changes(repo, None)?.is_empty())
}

fn collect_worktree_changes(
    repo: &git2::Repository,
    selected_paths: Option<&BTreeSet<String>>,
) -> Result<BTreeMap<String, WorktreeChange>> {
    let mut status_options = git2::StatusOptions::new();
    status_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .renames_head_to_index(false)
        .renames_index_to_workdir(false);

    let statuses = repo.statuses(Some(&mut status_options))?;
    for entry in statuses.iter() {
        if entry.status().is_conflicted() {
            return Err(anyhow!("cannot operate on conflicted files"));
        }
    }

    let mut diff_options = git2::DiffOptions::new();
    diff_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_unmodified(false)
        .ignore_submodules(true);
    if let Some(selected) = selected_paths {
        for path in selected {
            diff_options.pathspec(path);
        }
    }

    let head_tree = current_head_tree(repo)?;
    let diff = repo.diff_tree_to_workdir(head_tree.as_ref(), Some(&mut diff_options))?;
    let mut changes = BTreeMap::new();

    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(path_to_repo_string)
            .unwrap_or_default();
        if path.is_empty() {
            continue;
        }

        let change = match delta.status() {
            git2::Delta::Added
            | git2::Delta::Modified
            | git2::Delta::Renamed
            | git2::Delta::Copied
            | git2::Delta::Typechange
            | git2::Delta::Untracked => WorktreeChange::AddOrUpdate,
            git2::Delta::Deleted => WorktreeChange::Remove,
            git2::Delta::Ignored | git2::Delta::Unreadable | git2::Delta::Unmodified => {
                continue;
            }
            git2::Delta::Conflicted => return Err(anyhow!("cannot operate on conflicted files")),
        };

        changes.insert(path, change);
    }

    Ok(changes)
}

fn stage_changes(
    repo: &git2::Repository,
    changes: &BTreeMap<String, WorktreeChange>,
) -> Result<()> {
    let mut index = repo.index()?;
    for (path, change) in changes {
        let path = Path::new(path);
        match change {
            WorktreeChange::AddOrUpdate => {
                index
                    .add_path(path)
                    .with_context(|| format!("failed to stage {}", path.display()))?;
            }
            WorktreeChange::Remove => {
                index
                    .remove_path(path)
                    .with_context(|| format!("failed to stage deletion for {}", path.display()))?;
            }
        }
    }
    index.write()?;
    Ok(())
}

fn ensure_no_hidden_index_changes(repo: &git2::Repository, action_message: &str) -> Result<()> {
    let mut status_options = git2::StatusOptions::new();
    status_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .renames_head_to_index(false)
        .renames_index_to_workdir(false);

    let statuses = repo.statuses(Some(&mut status_options))?;
    for entry in statuses.iter() {
        let status = entry.status();
        if status.is_conflicted() {
            return Err(anyhow!("cannot operate on conflicted files"));
        }
        if has_index_changes(status) {
            return Err(anyhow!(
                "{action_message}; unstage or commit those changes outside Hunk first"
            ));
        }
    }

    Ok(())
}

fn has_index_changes(status: git2::Status) -> bool {
    status.is_index_new()
        || status.is_index_modified()
        || status.is_index_deleted()
        || status.is_index_renamed()
        || status.is_index_typechange()
}

fn create_commit_from_index(repo: &git2::Repository, message: &str) -> Result<git2::Oid> {
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let signature = repo
        .signature()
        .context("Git user.name and user.email must be configured before committing")?;
    let parents = current_head_commit(repo)?.into_iter().collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();

    Ok(repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        parent_refs.as_slice(),
    )?)
}

fn current_head_tree(repo: &git2::Repository) -> Result<Option<git2::Tree<'_>>> {
    Ok(current_head_commit(repo)?
        .map(|commit| commit.tree())
        .transpose()?)
}

fn current_head_commit(repo: &git2::Repository) -> Result<Option<git2::Commit<'_>>> {
    let head = match repo.head() {
        Ok(head) => head,
        Err(err) if err.code() == git2::ErrorCode::UnbornBranch => return Ok(None),
        Err(err) if err.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    match head.peel_to_commit() {
        Ok(commit) => Ok(Some(commit)),
        Err(err) if err.code() == git2::ErrorCode::UnbornBranch => Ok(None),
        Err(err) if err.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn normalize_selected_paths(paths: &[String]) -> BTreeSet<String> {
    paths
        .iter()
        .map(String::as_str)
        .map(normalize_repo_path)
        .filter(|path| !path.is_empty())
        .collect()
}

fn normalize_repo_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

fn path_to_repo_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn remove_worktree_path(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.is_dir() {
        std::fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove {}", path.display()))?;
    } else {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn remove_path_from_index_if_present(repo: &git2::Repository, path: &str) -> Result<()> {
    let mut index = repo.index()?;
    match index.remove_path(Path::new(path)) {
        Ok(()) => {
            index.write()?;
            Ok(())
        }
        Err(err) if err.code() == git2::ErrorCode::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}
