use std::path::Path;

use anyhow::{Context as _, Result, anyhow};
use git2::{
    AutotagOption, BranchType, Cred, CredentialType, FetchOptions, PushOptions, RemoteCallbacks,
    Repository,
};

use crate::branch::is_valid_branch_name;

#[derive(Debug, Clone)]
struct UpstreamTarget {
    remote_name: String,
    remote_branch_name: String,
    tracking_ref_name: String,
}

pub fn push_current_branch(
    repo_root: &Path,
    branch_name: &str,
    require_existing_upstream: bool,
) -> Result<()> {
    let branch_name = normalized_branch_name(branch_name)?;
    let repo = open_repo(repo_root)?;
    repo.find_branch(branch_name, BranchType::Local)
        .with_context(|| format!("branch '{branch_name}' does not exist"))?;

    let maybe_upstream = resolve_upstream_target(&repo, branch_name)?;
    if require_existing_upstream && maybe_upstream.is_none() {
        return Err(anyhow!("publish this branch before pushing"));
    }
    if !require_existing_upstream && maybe_upstream.is_some() {
        return Err(anyhow!("branch '{branch_name}' is already published"));
    }

    let upstream = match maybe_upstream {
        Some(upstream) => upstream,
        None => {
            let remote_name = resolve_publish_remote_name(&repo, branch_name)?;
            UpstreamTarget {
                tracking_ref_name: format!("refs/remotes/{remote_name}/{branch_name}"),
                remote_name,
                remote_branch_name: branch_name.to_string(),
            }
        }
    };

    let local_ref_name = local_branch_ref_name(branch_name);
    let refspec = format!(
        "{local_ref_name}:refs/heads/{}",
        upstream.remote_branch_name
    );
    let mut push_options = push_options(&repo)?;
    let mut remote = repo
        .find_remote(upstream.remote_name.as_str())
        .with_context(|| format!("remote '{}' is not configured", upstream.remote_name))?;
    remote
        .push(&[refspec.as_str()], Some(&mut push_options))
        .with_context(|| {
            format!(
                "failed to push branch '{}' to remote '{}'",
                branch_name, upstream.remote_name
            )
        })?;

    if !require_existing_upstream {
        let mut branch = repo.find_branch(branch_name, BranchType::Local)?;
        branch
            .set_upstream(Some(
                format!("{}/{}", upstream.remote_name, upstream.remote_branch_name).as_str(),
            ))
            .with_context(|| {
                format!(
                    "failed to set upstream for branch '{branch_name}' to '{}/{}'",
                    upstream.remote_name, upstream.remote_branch_name
                )
            })?;
    }

    update_tracking_ref_to_local_head(&repo, branch_name)?;
    Ok(())
}

pub fn sync_current_branch(repo_root: &Path, branch_name: &str) -> Result<()> {
    let branch_name = normalized_branch_name(branch_name)?;
    let repo = open_repo(repo_root)?;
    ensure_sync_worktree_is_clean(&repo)?;
    let upstream = resolve_upstream_target(&repo, branch_name)?
        .ok_or_else(|| anyhow!("no upstream branch to sync from"))?;

    fetch_upstream(&repo, &upstream)?;

    let local_branch = repo
        .find_branch(branch_name, BranchType::Local)
        .with_context(|| format!("branch '{branch_name}' does not exist"))?;
    let tracking_reference = repo
        .find_reference(upstream.tracking_ref_name.as_str())
        .with_context(|| {
            format!(
                "tracking branch '{}' does not exist after fetch",
                upstream.tracking_ref_name
            )
        })?;
    let remote_commit = repo
        .reference_to_annotated_commit(&tracking_reference)
        .context("failed to resolve fetched upstream commit")?;
    let (analysis, _) = repo
        .merge_analysis_for_ref(local_branch.get(), &[&remote_commit])
        .context("failed to analyze sync merge state")?;

    if analysis.is_up_to_date() {
        return Ok(());
    }
    if analysis.is_fast_forward() {
        fast_forward_branch(
            &repo,
            branch_name,
            upstream.tracking_ref_name.as_str(),
            remote_commit.id(),
            local_branch.is_head(),
        )?;
        return Ok(());
    }
    if analysis.is_normal() {
        return Err(anyhow!(
            "branch has diverged from upstream; sync only supports fast-forward updates"
        ));
    }
    if analysis.is_unborn() {
        return Err(anyhow!(
            "cannot sync branch '{branch_name}' because the local branch has no commits"
        ));
    }

    Err(anyhow!(
        "sync is not supported for the current branch state"
    ))
}

fn ensure_sync_worktree_is_clean(repo: &Repository) -> Result<()> {
    let mut status_options = git2::StatusOptions::new();
    status_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .renames_head_to_index(false)
        .renames_index_to_workdir(false);

    let statuses = repo.statuses(Some(&mut status_options))?;
    if statuses.is_empty() {
        return Ok(());
    }

    Err(anyhow!(
        "commit, unstage, or discard local changes before syncing"
    ))
}

fn normalized_branch_name(branch_name: &str) -> Result<&str> {
    let branch_name = branch_name.trim();
    if branch_name.is_empty() || branch_name == "detached" {
        return Err(anyhow!("cannot operate without a branch name"));
    }
    if !is_valid_branch_name(branch_name) {
        return Err(anyhow!("invalid branch name: {branch_name}"));
    }
    Ok(branch_name)
}

fn open_repo(repo_root: &Path) -> Result<Repository> {
    Repository::open(repo_root)
        .with_context(|| format!("failed to open Git repository at {}", repo_root.display()))
}

fn local_branch_ref_name(branch_name: &str) -> String {
    format!("refs/heads/{branch_name}")
}

fn resolve_upstream_target(repo: &Repository, branch_name: &str) -> Result<Option<UpstreamTarget>> {
    let local_ref_name = local_branch_ref_name(branch_name);
    let remote_name = match repo.branch_upstream_remote(local_ref_name.as_str()) {
        Ok(name) => name,
        Err(err)
            if matches!(
                err.code(),
                git2::ErrorCode::NotFound | git2::ErrorCode::InvalidSpec
            ) =>
        {
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };
    let remote_name = remote_name
        .as_str()
        .ok_or_else(|| anyhow!("upstream remote name for '{branch_name}' is not valid UTF-8"))?
        .to_string();

    let merge_ref = repo
        .branch_upstream_merge(local_ref_name.as_str())
        .with_context(|| format!("failed to resolve upstream branch for '{branch_name}'"))?;
    let merge_ref = merge_ref
        .as_str()
        .ok_or_else(|| anyhow!("upstream branch name for '{branch_name}' is not valid UTF-8"))?;
    let remote_branch_name = merge_ref
        .strip_prefix("refs/heads/")
        .ok_or_else(|| anyhow!("unsupported upstream branch target '{merge_ref}'"))?
        .to_string();

    let tracking_ref_name = match repo.branch_upstream_name(local_ref_name.as_str()) {
        Ok(name) => name
            .as_str()
            .ok_or_else(|| anyhow!("tracking branch name for '{branch_name}' is not valid UTF-8"))?
            .to_string(),
        Err(err)
            if matches!(
                err.code(),
                git2::ErrorCode::NotFound | git2::ErrorCode::InvalidSpec
            ) =>
        {
            git_tracking_ref_name(remote_name.as_str(), remote_branch_name.as_str())
        }
        Err(err) => return Err(err.into()),
    };

    Ok(Some(UpstreamTarget {
        remote_name,
        remote_branch_name,
        tracking_ref_name,
    }))
}

fn git_tracking_ref_name(remote_name: &str, remote_branch_name: &str) -> String {
    format!("refs/remotes/{remote_name}/{remote_branch_name}")
}

fn resolve_publish_remote_name(repo: &Repository, branch_name: &str) -> Result<String> {
    let config = repo.config()?;
    for key in [
        format!("branch.{branch_name}.pushRemote"),
        "remote.pushDefault".to_string(),
        format!("branch.{branch_name}.remote"),
    ] {
        if let Ok(candidate) = config.get_string(key.as_str())
            && repo.find_remote(candidate.as_str()).is_ok()
        {
            return Ok(candidate);
        }
    }

    let remotes = repo
        .remotes()
        .context("failed to list configured Git remotes")?;
    if remotes.iter().flatten().any(|name| name == "origin") {
        return Ok("origin".to_string());
    }
    if remotes.len() == 1 {
        let remote_name = remotes
            .get(0)
            .ok_or_else(|| anyhow!("remote name is missing"))?;
        return Ok(remote_name.to_string());
    }
    if remotes.len() > 1 {
        return Err(anyhow!(
            "multiple Git remotes are configured; set branch.{branch_name}.pushRemote or remote.pushDefault before publishing"
        ));
    }

    Err(anyhow!("no Git remote configured for publish/push"))
}

fn fetch_upstream(repo: &Repository, upstream: &UpstreamTarget) -> Result<()> {
    let mut fetch_options = fetch_options(repo)?;
    let mut remote = repo
        .find_remote(upstream.remote_name.as_str())
        .with_context(|| format!("remote '{}' is not configured", upstream.remote_name))?;
    let fetch_refspec = format!(
        "+refs/heads/{}:{}",
        upstream.remote_branch_name, upstream.tracking_ref_name
    );
    remote
        .fetch(&[fetch_refspec.as_str()], Some(&mut fetch_options), None)
        .with_context(|| {
            format!(
                "failed to fetch branch '{}' from remote '{}'",
                upstream.remote_branch_name, upstream.remote_name
            )
        })?;
    Ok(())
}

fn fast_forward_branch(
    repo: &Repository,
    branch_name: &str,
    tracking_ref_name: &str,
    target_id: git2::Oid,
    branch_is_head: bool,
) -> Result<()> {
    if branch_is_head {
        let target = repo.find_object(target_id, None)?;
        repo.reset(&target, git2::ResetType::Hard, None)
            .with_context(|| {
                format!(
                    "failed to fast-forward checked out branch '{branch_name}' to '{tracking_ref_name}'"
                )
            })?;
        return Ok(());
    }

    let local_ref_name = local_branch_ref_name(branch_name);
    let mut reference = repo
        .find_reference(local_ref_name.as_str())
        .with_context(|| format!("branch reference '{local_ref_name}' is missing"))?;
    reference
        .set_target(
            target_id,
            format!("hunk sync fast-forward from {tracking_ref_name}").as_str(),
        )
        .with_context(|| {
            format!("failed to fast-forward branch '{branch_name}' to '{tracking_ref_name}'")
        })?;
    Ok(())
}

fn update_tracking_ref_to_local_head(repo: &Repository, branch_name: &str) -> Result<()> {
    let Some(upstream) = resolve_upstream_target(repo, branch_name)? else {
        return Ok(());
    };
    let local_branch = repo.find_branch(branch_name, BranchType::Local)?;
    let local_target = local_branch
        .get()
        .target()
        .ok_or_else(|| anyhow!("branch '{branch_name}' does not point to a commit"))?;

    repo.reference(
        upstream.tracking_ref_name.as_str(),
        local_target,
        true,
        format!(
            "hunk update tracking ref for {}",
            local_branch_ref_name(branch_name)
        )
        .as_str(),
    )
    .with_context(|| {
        format!(
            "failed to update tracking branch '{}'",
            upstream.tracking_ref_name
        )
    })?;
    Ok(())
}

fn fetch_options(repo: &Repository) -> Result<FetchOptions<'static>> {
    let mut options = FetchOptions::new();
    options.download_tags(AutotagOption::Unspecified);
    options.remote_callbacks(remote_callbacks(repo)?);
    Ok(options)
}

fn push_options(repo: &Repository) -> Result<PushOptions<'static>> {
    let mut options = PushOptions::new();
    options.remote_callbacks(remote_callbacks(repo)?);
    Ok(options)
}

fn remote_callbacks(repo: &Repository) -> Result<RemoteCallbacks<'static>> {
    let config = repo
        .config()
        .context("failed to load Git config for authentication")?;
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |url, username_from_url, allowed| {
        resolve_credentials(&config, url, username_from_url, allowed)
    });
    Ok(callbacks)
}

fn resolve_credentials(
    config: &git2::Config,
    url: &str,
    username_from_url: Option<&str>,
    allowed: CredentialType,
) -> std::result::Result<Cred, git2::Error> {
    if allowed.contains(CredentialType::USERNAME)
        && let Some(username) = username_from_url
    {
        return Cred::username(username);
    }

    if allowed.contains(CredentialType::SSH_KEY) {
        let username = username_from_url.unwrap_or("git");
        if let Ok(cred) = Cred::ssh_key_from_agent(username) {
            return Ok(cred);
        }
    }

    if allowed.contains(CredentialType::USER_PASS_PLAINTEXT)
        && let Ok(cred) = Cred::credential_helper(config, url, username_from_url)
    {
        return Ok(cred);
    }

    if allowed.contains(CredentialType::DEFAULT)
        && let Ok(cred) = Cred::default()
    {
        return Ok(cred);
    }

    if allowed.contains(CredentialType::USERNAME) {
        return Cred::username(username_from_url.unwrap_or("git"));
    }

    Err(git2::Error::from_str(
        "failed to acquire credentials for Git remote operation",
    ))
}
