use std::path::Path;

use anyhow::{Context as _, Result, anyhow};
use git2::{BranchType, Signature, StashFlags};

use crate::branch::is_valid_branch_name;

pub fn create_branch_from_base_with_change_transfer(
    repo_root: &Path,
    branch_name: &str,
    base_branch_name: &str,
) -> Result<()> {
    let branch_name = branch_name.trim();
    if branch_name.is_empty() {
        return Err(anyhow!("branch name cannot be empty"));
    }
    if !is_valid_branch_name(branch_name) {
        return Err(anyhow!("invalid branch name: {branch_name}"));
    }

    let base_branch_name = base_branch_name.trim();
    if base_branch_name.is_empty() {
        return Err(anyhow!("base branch name cannot be empty"));
    }
    if !is_valid_branch_name(base_branch_name) {
        return Err(anyhow!("invalid base branch name: {base_branch_name}"));
    }

    let mut repo = super::open_repo(repo_root)?;
    super::ensure_no_hidden_index_changes(
        &repo,
        "moving changes to a review branch with staged index changes is not supported",
    )?;
    if repo.find_branch(branch_name, BranchType::Local).is_ok() {
        return Err(anyhow!("branch '{branch_name}' already exists"));
    }

    let original_branch_name = current_local_branch_name(&repo)?.ok_or_else(|| {
        anyhow!("cannot create branch '{branch_name}' without an active local branch")
    })?;
    let changed_paths = super::collect_worktree_changes(&repo, None)?
        .into_keys()
        .collect::<Vec<_>>();
    if changed_paths.is_empty() {
        return Err(anyhow!(
            "cannot create isolated branch '{branch_name}' from '{base_branch_name}' without uncommitted changes to transfer"
        ));
    }

    let base_commit_id = repo
        .find_branch(base_branch_name, BranchType::Local)
        .with_context(|| format!("base branch '{base_branch_name}' does not exist"))?
        .into_reference()
        .peel_to_commit()
        .with_context(|| format!("failed to resolve base branch '{base_branch_name}' commit"))?
        .id();

    let stash_signature = stash_signature(&repo)?;
    repo.stash_save(
        &stash_signature,
        format!("hunk: transfer changes to {branch_name}").as_str(),
        Some(StashFlags::INCLUDE_UNTRACKED),
    )
    .context("failed to stash working copy changes for branch transfer")?;

    let transfer_result = (|| -> Result<()> {
        {
            let base_commit = repo
                .find_commit(base_commit_id)
                .with_context(|| format!("failed to reload base commit '{base_commit_id}'"))?;
            repo.branch(branch_name, &base_commit, false)
                .with_context(|| {
                    format!("failed to create branch '{branch_name}' from '{base_branch_name}'")
                })?;
        }
        super::checkout_local_branch(&repo, branch_name)?;
        repo.stash_pop(0, None).with_context(|| {
            format!("failed to apply stashed changes onto branch '{branch_name}'")
        })?;
        Ok(())
    })();

    if transfer_result.is_ok() {
        return Ok(());
    }

    let transfer_err = transfer_result.expect_err("branch transfer should have failed");
    rollback_branch_transfer(
        repo_root,
        original_branch_name.as_str(),
        branch_name,
        changed_paths.as_slice(),
    )
    .map_err(|rollback_err| anyhow!("{transfer_err:#}; rollback failed: {rollback_err:#}"))?;
    Err(transfer_err)
}

fn current_local_branch_name(repo: &git2::Repository) -> Result<Option<String>> {
    let head = match repo.head() {
        Ok(head) => head,
        Err(err) if err.code() == git2::ErrorCode::UnbornBranch => return Ok(None),
        Err(err) if err.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    if !head.is_branch() {
        return Ok(None);
    }
    Ok(head.shorthand().map(ToOwned::to_owned))
}

fn stash_signature(repo: &git2::Repository) -> Result<Signature<'static>> {
    repo.signature()
        .or_else(|_| Signature::now("Hunk", "hunk@example.com"))
        .context("failed to build stash signature for branch transfer")
}

fn rollback_branch_transfer(
    repo_root: &Path,
    original_branch_name: &str,
    created_branch_name: &str,
    changed_paths: &[String],
) -> Result<()> {
    super::restore_working_copy_paths(repo_root, changed_paths).with_context(|| {
        format!(
            "failed to clean partial branch-transfer changes before restoring '{}'",
            original_branch_name
        )
    })?;

    let repo = super::open_repo(repo_root)?;
    super::checkout_local_branch(&repo, original_branch_name).with_context(|| {
        format!("failed to reactivate original branch '{original_branch_name}'")
    })?;

    let mut repo = super::open_repo(repo_root)?;
    repo.stash_pop(0, None).with_context(|| {
        format!("failed to restore stashed changes on branch '{original_branch_name}'")
    })?;

    if let Ok(mut created_branch) = repo.find_branch(created_branch_name, BranchType::Local) {
        created_branch.delete().with_context(|| {
            format!("failed to delete branch '{created_branch_name}' during rollback")
        })?;
    }

    Ok(())
}
