use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use git2::{BranchType, Repository, WorktreeAddOptions};
use hunk_domain::paths::hunk_home_dir;

use crate::branch::is_valid_branch_name;
use crate::git::discover_repo_root;

pub const PRIMARY_WORKSPACE_TARGET_ID: &str = "primary";
pub const MANAGED_WORKTREES_DIR_NAME: &str = "worktrees";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTargetKind {
    PrimaryCheckout,
    LinkedWorktree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTargetSummary {
    pub id: String,
    pub kind: WorkspaceTargetKind,
    pub root: PathBuf,
    pub name: String,
    pub display_name: String,
    pub branch_name: String,
    pub managed: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateWorktreeRequest {
    pub branch_name: String,
    pub base_branch_name: Option<String>,
}

pub fn workspace_target_id_for_worktree(name: &str) -> String {
    format!("worktree:{name}")
}

pub fn managed_worktrees_root(primary_repo_root: &Path) -> Result<PathBuf> {
    Ok(hunk_home_dir()?
        .join(MANAGED_WORKTREES_DIR_NAME)
        .join(repository_storage_key(primary_repo_root)))
}

pub fn managed_worktree_path(primary_repo_root: &Path, worktree_name: &str) -> Result<PathBuf> {
    Ok(managed_worktrees_root(primary_repo_root)?.join(worktree_name))
}

pub fn repo_relative_path_is_within_managed_worktrees(_relative_path: &str) -> bool {
    false
}

pub fn path_is_within_managed_worktrees(
    primary_repo_root: &Path,
    absolute_path: &Path,
) -> Result<bool> {
    let primary_repo_root = canonicalize_existing_path(primary_repo_root)?;
    let absolute_path = normalize_absolute_path(primary_repo_root.as_path(), absolute_path)?;
    Ok(path_is_within_root(
        absolute_path.as_path(),
        managed_worktrees_root(primary_repo_root.as_path())?.as_path(),
    ))
}

pub fn primary_repo_root(path: &Path) -> Result<PathBuf> {
    let repo = gix::discover(path)
        .with_context(|| format!("failed to discover Git repository from {}", path.display()))?;
    let common_dir = canonicalize_existing_path(repo.common_dir())?;
    let root = common_dir.parent().ok_or_else(|| {
        anyhow!(
            "failed to resolve primary repository root from common dir {}",
            common_dir.display()
        )
    })?;
    canonicalize_existing_path(root)
}

pub fn list_workspace_targets(path: &Path) -> Result<Vec<WorkspaceTargetSummary>> {
    let active_root = canonicalize_existing_path(discover_repo_root(path)?.as_path())?;
    let primary_root = primary_repo_root(path)?;
    let repo = open_repository(primary_root.as_path())?;
    let mut targets = Vec::new();

    targets.push(primary_workspace_target_summary(
        primary_root.as_path(),
        active_root.as_path(),
    )?);

    for worktree_name in repo.worktrees()?.iter().flatten() {
        let worktree = repo
            .find_worktree(worktree_name)
            .with_context(|| format!("failed to open worktree '{worktree_name}'"))?;
        if worktree.validate().is_err() {
            continue;
        }
        let root = canonicalize_existing_path(worktree.path())?;
        targets.push(worktree_target_summary(
            worktree_name,
            root,
            active_root.as_path(),
            primary_root.as_path(),
        )?);
    }

    targets.sort_by(|left, right| match (left.kind, right.kind) {
        (WorkspaceTargetKind::PrimaryCheckout, WorkspaceTargetKind::LinkedWorktree) => {
            std::cmp::Ordering::Less
        }
        (WorkspaceTargetKind::LinkedWorktree, WorkspaceTargetKind::PrimaryCheckout) => {
            std::cmp::Ordering::Greater
        }
        _ => left
            .display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase()),
    });

    Ok(targets)
}

pub fn create_managed_worktree(
    path: &Path,
    request: &CreateWorktreeRequest,
) -> Result<WorkspaceTargetSummary> {
    let branch_name = request.branch_name.trim();
    if !is_valid_branch_name(branch_name) {
        return Err(anyhow!("invalid branch name: {branch_name}"));
    }
    let base_branch_name = request
        .base_branch_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    if let Some(base_branch_name) = base_branch_name
        && !is_valid_branch_name(base_branch_name)
    {
        return Err(anyhow!("invalid base branch name: {base_branch_name}"));
    }

    let active_repo = open_repository(path)?;
    let primary_root = primary_repo_root(path)?;
    let primary_repo = open_repository(primary_root.as_path())?;

    if primary_repo
        .find_branch(branch_name, BranchType::Local)
        .is_ok()
    {
        return Err(anyhow!("branch '{branch_name}' already exists"));
    }

    let worktree_name = allocate_managed_worktree_name(primary_root.as_path(), &primary_repo)?;
    let registration_name = managed_worktree_registration_name(worktree_name.as_str());

    let managed_root = managed_worktrees_root(primary_root.as_path())?;
    let target_path = managed_worktree_path(primary_root.as_path(), worktree_name.as_str())?;
    if target_path.exists() {
        return Err(anyhow!(
            "worktree path already exists: {}",
            target_path.display()
        ));
    }
    fs::create_dir_all(managed_root.as_path()).with_context(|| {
        format!(
            "failed to create managed worktree directory {}",
            managed_root.display()
        )
    })?;
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create managed worktree parent directory {}",
                parent.display()
            )
        })?;
    }

    let base_commit = if let Some(base_branch_name) = base_branch_name {
        let base_branch = primary_repo
            .find_branch(base_branch_name, BranchType::Local)
            .with_context(|| format!("base branch '{base_branch_name}' does not exist"))?;
        base_branch
            .into_reference()
            .peel_to_commit()
            .with_context(|| format!("failed to resolve base branch '{base_branch_name}' commit"))?
    } else {
        let head_commit = active_repo
            .head()
            .context("failed to resolve HEAD for worktree creation")?
            .peel_to_commit()
            .context("failed to resolve HEAD commit for worktree creation")?;
        primary_repo
            .find_commit(head_commit.id())
            .context("failed to resolve shared HEAD commit in primary repository")?
    };
    let branch = primary_repo
        .branch(branch_name, &base_commit, false)
        .with_context(|| format!("failed to create branch '{branch_name}'"))?;
    let mut options = WorktreeAddOptions::new();
    options.reference(Some(branch.get()));
    primary_repo
        .worktree(
            registration_name.as_str(),
            target_path.as_path(),
            Some(&options),
        )
        .with_context(|| {
            format!(
                "failed to create worktree '{}' at {}",
                worktree_name.as_str(),
                target_path.display()
            )
        })?;

    let active_root = canonicalize_existing_path(discover_repo_root(path)?.as_path())?;
    let created_root = canonicalize_existing_path(target_path.as_path())?;
    worktree_target_summary(
        worktree_name.as_str(),
        created_root,
        active_root.as_path(),
        primary_root.as_path(),
    )
}

pub fn remove_managed_worktree(path: &Path) -> Result<()> {
    let target_root = canonicalize_existing_path(discover_repo_root(path)?.as_path())?;
    let primary_root = primary_repo_root(path)?;
    if target_root == primary_root {
        return Err(anyhow!("primary checkout cannot be removed as a worktree"));
    }
    if !path_is_within_managed_worktrees(primary_root.as_path(), target_root.as_path())? {
        return Err(anyhow!("only Hunk-managed linked worktrees can be removed"));
    }

    ensure_worktree_is_clean(target_root.as_path())?;

    let repo = open_repository(target_root.as_path())?;
    let worktree = git2::Worktree::open_from_repository(&repo)
        .context("failed to resolve linked worktree metadata for removal")?;
    if let git2::WorktreeLockStatus::Locked(reason) = worktree.is_locked()? {
        let detail = reason.unwrap_or_else(|| "worktree is locked".to_string());
        return Err(anyhow!("cannot remove locked worktree: {detail}"));
    }

    let mut prune_options = git2::WorktreePruneOptions::new();
    prune_options.valid(true).working_tree(true);
    worktree.prune(Some(&mut prune_options)).with_context(|| {
        format!(
            "failed to remove managed worktree at {}",
            target_root.display()
        )
    })?;
    Ok(())
}

fn primary_workspace_target_summary(
    primary_root: &Path,
    active_root: &Path,
) -> Result<WorkspaceTargetSummary> {
    Ok(WorkspaceTargetSummary {
        id: PRIMARY_WORKSPACE_TARGET_ID.to_string(),
        kind: WorkspaceTargetKind::PrimaryCheckout,
        root: primary_root.to_path_buf(),
        name: primary_checkout_name(primary_root),
        display_name: "Primary Checkout".to_string(),
        branch_name: checked_out_branch_name(primary_root)?,
        managed: false,
        is_active: primary_root == active_root,
    })
}

fn worktree_target_summary(
    worktree_registration_name: &str,
    root: PathBuf,
    active_root: &Path,
    primary_root: &Path,
) -> Result<WorkspaceTargetSummary> {
    let managed_name = managed_worktree_name_from_root(root.as_path(), primary_root);
    let managed = managed_name.is_some();
    let worktree_name = managed_name.unwrap_or_else(|| worktree_registration_name.to_string());
    let branch_name = checked_out_branch_name(root.as_path())?;
    Ok(WorkspaceTargetSummary {
        id: workspace_target_id_for_worktree(worktree_name.as_str()),
        kind: WorkspaceTargetKind::LinkedWorktree,
        root: root.clone(),
        name: worktree_name.clone(),
        display_name: linked_workspace_target_display_name(
            worktree_name.as_str(),
            branch_name.as_str(),
        ),
        branch_name,
        managed,
        is_active: root == active_root,
    })
}

fn checked_out_branch_name(path: &Path) -> Result<String> {
    let repo = open_repository(path)?;
    Ok(match repo.head() {
        Ok(head) => head.shorthand().unwrap_or("detached").to_string(),
        Err(err) if err.code() == git2::ErrorCode::UnbornBranch => "unborn".to_string(),
        Err(_) => "detached".to_string(),
    })
}

fn ensure_worktree_is_clean(path: &Path) -> Result<()> {
    let repo = open_repository(path)?;
    let mut status_options = git2::StatusOptions::new();
    status_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .renames_head_to_index(false)
        .renames_index_to_workdir(false);

    let statuses = repo.statuses(Some(&mut status_options)).with_context(|| {
        format!(
            "failed to inspect worktree changes before removing {}",
            path.display()
        )
    })?;
    if statuses.iter().any(|entry| entry.status().is_conflicted()) {
        return Err(anyhow!(
            "cannot remove worktree with conflicted changes; resolve or discard them first"
        ));
    }
    if !statuses.is_empty() {
        return Err(anyhow!(
            "cannot remove worktree with uncommitted changes; commit, stash, or discard them first"
        ));
    }

    Ok(())
}

fn open_repository(path: &Path) -> Result<Repository> {
    Repository::open(path)
        .with_context(|| format!("failed to open Git repository at {}", path.display()))
}

fn canonicalize_existing_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("failed to resolve path {}", path.display()))
}

fn normalize_absolute_path(primary_repo_root: &Path, path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return canonicalize_existing_path(path);
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        primary_repo_root.join(path)
    };
    Ok(normalize_lexical_path(absolute.as_path()))
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn primary_checkout_name(primary_root: &Path) -> String {
    primary_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("project")
        .to_string()
}

fn allocate_managed_worktree_name(primary_root: &Path, repo: &Repository) -> Result<String> {
    let mut index = 1usize;
    loop {
        let candidate = format!("worktree-{index}");
        let registration_name = managed_worktree_registration_name(candidate.as_str());
        if repo.find_worktree(registration_name.as_str()).is_ok() {
            index += 1;
            continue;
        }
        if managed_worktree_path(primary_root, candidate.as_str())?.exists() {
            index += 1;
            continue;
        }
        return Ok(candidate);
    }
}

fn managed_worktree_registration_name(worktree_name: &str) -> String {
    let mut encoded = String::with_capacity(worktree_name.len());
    for byte in worktree_name.bytes() {
        let ch = byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            encoded.push(ch);
        } else {
            encoded.push_str(format!("_x{byte:02X}_").as_str());
        }
    }
    encoded
}

fn managed_worktree_name_from_root(root: &Path, primary_root: &Path) -> Option<String> {
    managed_worktree_name_from_managed_root(
        root,
        managed_worktrees_root(primary_root).ok()?.as_path(),
    )
}

fn linked_workspace_target_display_name(worktree_name: &str, branch_name: &str) -> String {
    if is_detached_workspace_target_branch(branch_name) {
        worktree_name.to_string()
    } else {
        branch_name.to_string()
    }
}

fn is_detached_workspace_target_branch(branch_name: &str) -> bool {
    matches!(branch_name, "detached" | "unborn")
}

fn path_is_within_root(path: &Path, root: &Path) -> bool {
    let root = normalize_lexical_path(root);
    let path = normalize_lexical_path(path);
    path == root || path.starts_with(root.as_path())
}

fn managed_worktree_name_from_managed_root(root: &Path, managed_root: &Path) -> Option<String> {
    let relative = root.strip_prefix(managed_root).ok()?;
    let name = relative.to_string_lossy().replace('\\', "/");
    (!name.is_empty()).then_some(name)
}

fn repository_storage_key(primary_repo_root: &Path) -> String {
    let repo_name = primary_repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_repo_storage_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "project".to_string());
    let hash = fnv1a64_hex(primary_repo_root.to_string_lossy().as_bytes());
    format!("{repo_name}-{hash}")
}

fn sanitize_repo_storage_name(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
