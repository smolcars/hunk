use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use std::process::Command;

use anyhow::{Context as _, Result, anyhow};

use crate::branch::is_valid_branch_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorktreeChange {
    AddOrUpdate,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedCommit {
    pub commit_id: String,
    pub subject: String,
    pub committed_unix_time: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiWorkingCopyContext {
    pub changed_files_summary: String,
    pub diff_patch: String,
}

// `gix` is still the primary backend for the hot read path. We isolate local worktree/index
// mutation here until it exposes a stable public checkout/index-editing surface we can rely on.
pub fn activate_or_create_branch(
    repo_root: &Path,
    branch_name: &str,
    move_changes_to_branch: bool,
) -> Result<()> {
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
        if move_changes_to_branch {
            "moving changes to a review branch with staged index changes is not supported"
        } else {
            "switching branches with staged index changes is not supported"
        },
    )?;
    if move_changes_to_branch {
        if repo
            .find_branch(branch_name, git2::BranchType::Local)
            .is_ok()
        {
            return Err(anyhow!("branch '{branch_name}' already exists"));
        }

        let head_commit = current_head_commit(&repo)?.ok_or_else(|| {
            anyhow!("cannot create branch '{branch_name}' without an existing HEAD commit")
        })?;
        repo.branch(branch_name, &head_commit, false)
            .with_context(|| format!("failed to create branch '{branch_name}'"))?;
        repo.set_head(format!("refs/heads/{branch_name}").as_str())
            .with_context(|| format!("failed to activate branch '{branch_name}'"))?;
        return Ok(());
    }

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

pub fn commit_all_with_details(repo_root: &Path, message: &str) -> Result<CreatedCommit> {
    let (_, commit) = commit_paths_internal(repo_root, message, None)?;
    Ok(commit)
}

pub fn commit_selected_paths(
    repo_root: &Path,
    message: &str,
    selected_paths: &[String],
) -> Result<usize> {
    let selected_paths = normalize_selected_paths(selected_paths)?;
    if selected_paths.is_empty() {
        return Err(anyhow!("no files selected for commit"));
    }

    commit_paths_internal(repo_root, message, Some(&selected_paths)).map(|(count, _)| count)
}

pub fn commit_selected_paths_with_details(
    repo_root: &Path,
    message: &str,
    selected_paths: &[String],
) -> Result<(usize, CreatedCommit)> {
    let selected_paths = normalize_selected_paths(selected_paths)?;
    if selected_paths.is_empty() {
        return Err(anyhow!("no files selected for commit"));
    }

    commit_paths_internal(repo_root, message, Some(&selected_paths))
}

pub fn working_copy_context_for_ai(
    repo_root: &Path,
    max_files: usize,
    max_patch_bytes: usize,
) -> Result<Option<AiWorkingCopyContext>> {
    let repo = open_repo(repo_root)?;
    ensure_no_hidden_index_changes(
        &repo,
        "summarizing changes with staged index changes is not supported",
    )?;
    let changes = collect_worktree_changes(&repo, None)?;
    if changes.is_empty() {
        return Ok(None);
    }

    let limited_files = max_files.max(1);
    let mut summary_lines = changes
        .iter()
        .take(limited_files)
        .map(|(path, change)| format!("{} {}", worktree_change_status_code(*change), path))
        .collect::<Vec<_>>();
    if changes.len() > limited_files {
        summary_lines.push(format!(
            "... {} more file(s)",
            changes.len() - limited_files
        ));
    }
    let changed_files_summary = summary_lines.join("\n");

    let mut diff_options = git2::DiffOptions::new();
    diff_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_unmodified(false)
        .ignore_submodules(true);
    let head_tree = current_head_tree(&repo)?;
    let diff = repo.diff_tree_to_workdir(head_tree.as_ref(), Some(&mut diff_options))?;

    let capped_bytes = max_patch_bytes.max(1);
    let mut patch_bytes = Vec::new();
    let mut truncated = false;
    let print_result = diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        if patch_bytes.len() >= capped_bytes {
            truncated = true;
            return false;
        }

        let content = line.content();
        let remaining = capped_bytes.saturating_sub(patch_bytes.len());
        if content.len() > remaining {
            patch_bytes.extend_from_slice(&content[..remaining]);
            truncated = true;
            return false;
        }

        patch_bytes.extend_from_slice(content);
        true
    });
    if let Err(err) = print_result
        && !(truncated && err.code() == git2::ErrorCode::User)
    {
        return Err(err.into());
    }

    let mut diff_patch = String::from_utf8_lossy(patch_bytes.as_slice()).to_string();
    if truncated {
        if !diff_patch.ends_with('\n') {
            diff_patch.push('\n');
        }
        diff_patch.push_str("[truncated]\n");
    }

    Ok(Some(AiWorkingCopyContext {
        changed_files_summary,
        diff_patch,
    }))
}

pub fn restore_working_copy_paths(repo_root: &Path, paths: &[String]) -> Result<usize> {
    let selected_paths = normalize_selected_paths(paths)?;
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
) -> Result<(usize, CreatedCommit)> {
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
    let commit_id = create_commit_from_index(&repo, message)?;
    let refreshed_repo = open_repo(repo_root)?;
    Ok((
        changes.len(),
        created_commit(&refreshed_repo, commit_id, message)?,
    ))
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

fn worktree_change_status_code(change: WorktreeChange) -> &'static str {
    match change {
        WorktreeChange::AddOrUpdate => "M",
        WorktreeChange::Remove => "D",
    }
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
    run_git_commit(repo, message)?;
    let refreshed_repo = reopen_existing_repo(repo)?;
    current_head_commit(&refreshed_repo)?
        .map(|commit| commit.id())
        .ok_or_else(|| anyhow!("git commit completed without creating a HEAD commit"))
}

fn run_git_commit(repo: &git2::Repository, message: &str) -> Result<()> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow!("committing without a worktree is not supported"))?;
    let output = Command::new("git")
        .current_dir(workdir)
        .args(["commit", "--quiet", "--cleanup=verbatim", "-m"])
        .arg(message)
        .output()
        .context("failed to launch git commit")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(output.stderr.as_slice())
        .trim()
        .to_string();
    let stdout = String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string();
    let details = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("git commit exited with status {}", output.status)
    };
    Err(anyhow!("git commit failed: {details}"))
}

fn reopen_existing_repo(repo: &git2::Repository) -> Result<git2::Repository> {
    if let Some(workdir) = repo.workdir() {
        return git2::Repository::open(workdir)
            .with_context(|| format!("failed to reopen Git repository at {}", workdir.display()));
    }

    let git_dir = repo.path();
    git2::Repository::open(git_dir)
        .with_context(|| format!("failed to reopen Git repository at {}", git_dir.display()))
}

fn created_commit(
    repo: &git2::Repository,
    commit_id: git2::Oid,
    subject: &str,
) -> Result<CreatedCommit> {
    let commit = repo
        .find_commit(commit_id)
        .with_context(|| format!("failed to load created commit {commit_id}"))?;
    Ok(CreatedCommit {
        commit_id: commit_id.to_string(),
        subject: commit_subject(subject),
        committed_unix_time: Some(commit.time().seconds()),
    })
}

fn commit_subject(message: &str) -> String {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_default()
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

fn normalize_selected_paths(paths: &[String]) -> Result<BTreeSet<String>> {
    let mut normalized = BTreeSet::new();
    for path in paths {
        let path = normalize_repo_path(path.as_str())?;
        if !path.is_empty() {
            normalized.insert(path);
        }
    }
    Ok(normalized)
}

fn normalize_repo_path(path: &str) -> Result<String> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(String::new());
    }

    let normalized = path.replace('\\', "/");
    let mut components = Vec::new();
    for component in Path::new(normalized.as_str()).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| anyhow!("path '{}' is not valid UTF-8", path))?;
                components.push(part.to_string());
            }
            Component::ParentDir => {
                return Err(anyhow!("path '{}' escapes the repository root", path));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!(
                    "path '{}' must be relative to the repository root",
                    path
                ));
            }
        }
    }

    Ok(components.join("/"))
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
