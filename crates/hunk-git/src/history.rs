use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use gix::traverse::commit::simple::CommitTimeOrder;

use crate::git::open_repo;

pub const DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT: usize = 15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentCommitSummary {
    pub commit_id: String,
    pub subject: String,
    pub committed_unix_time: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentCommitsSnapshot {
    pub root: PathBuf,
    pub commits: Vec<RecentCommitSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentCommitsFingerprint {
    root: PathBuf,
    head_ref_name: Option<String>,
    head_commit_id: Option<String>,
    base_tip_id: Option<String>,
    limit: usize,
}

impl RecentCommitsFingerprint {
    pub fn root(&self) -> &Path {
        self.root.as_path()
    }

    pub fn head_ref_name(&self) -> Option<&str> {
        self.head_ref_name.as_deref()
    }

    pub fn head_commit_id(&self) -> Option<&str> {
        self.head_commit_id.as_deref()
    }

    pub fn base_tip_id(&self) -> Option<&str> {
        self.base_tip_id.as_deref()
    }
}

pub fn load_recent_authored_commits(path: &Path, limit: usize) -> Result<RecentCommitsSnapshot> {
    let (_, snapshot) = load_recent_authored_commits_with_fingerprint(path, limit)?;
    Ok(snapshot)
}

pub fn load_recent_authored_commits_fingerprint(
    path: &Path,
    limit: usize,
) -> Result<RecentCommitsFingerprint> {
    let (_, _, _, fingerprint) = recent_commits_context(path, limit)?;
    Ok(fingerprint)
}

pub fn load_recent_authored_commits_with_fingerprint(
    path: &Path,
    limit: usize,
) -> Result<(RecentCommitsFingerprint, RecentCommitsSnapshot)> {
    let (repo, tip_id, base_tip_id, fingerprint) = recent_commits_context(path, limit)?;
    let commits =
        load_recent_authored_commits_from_context(repo.repository(), tip_id, base_tip_id, limit)?;

    Ok((
        fingerprint,
        RecentCommitsSnapshot {
            root: repo.root().to_path_buf(),
            commits,
        },
    ))
}

pub fn load_recent_authored_commits_if_changed(
    path: &Path,
    limit: usize,
    previous_fingerprint: Option<&RecentCommitsFingerprint>,
) -> Result<(RecentCommitsFingerprint, Option<RecentCommitsSnapshot>)> {
    let (repo, tip_id, base_tip_id, fingerprint) = recent_commits_context(path, limit)?;
    if previous_fingerprint.is_some_and(|previous| previous == &fingerprint) {
        return Ok((fingerprint, None));
    }
    let commits =
        load_recent_authored_commits_from_context(repo.repository(), tip_id, base_tip_id, limit)?;
    Ok((
        fingerprint,
        Some(RecentCommitsSnapshot {
            root: repo.root().to_path_buf(),
            commits,
        }),
    ))
}

fn recent_commits_context(
    path: &Path,
    limit: usize,
) -> Result<(
    crate::git::GitRepo,
    gix::ObjectId,
    Option<gix::ObjectId>,
    RecentCommitsFingerprint,
)> {
    let repo = open_repo(path)?;
    let head_ref_name = repo
        .repository()
        .head_name()
        .context("failed to resolve Git HEAD name for recent commits")?
        .map(|name| name.to_string());
    let Some(head_commit_id) = repo.repository().head_id().ok().map(|id| id.detach()) else {
        let fingerprint = RecentCommitsFingerprint {
            root: repo.root().to_path_buf(),
            head_ref_name,
            head_commit_id: None,
            base_tip_id: None,
            limit,
        };
        return Ok((
            repo,
            gix::ObjectId::null(gix::hash::Kind::Sha1),
            None,
            fingerprint,
        ));
    };
    let base_tip_id =
        branch_base_tip_id(repo.repository(), head_ref_name.as_deref(), head_commit_id)?;
    let fingerprint = RecentCommitsFingerprint {
        root: repo.root().to_path_buf(),
        head_ref_name,
        head_commit_id: Some(head_commit_id.to_string()),
        base_tip_id: base_tip_id.as_ref().map(ToString::to_string),
        limit,
    };
    Ok((repo, head_commit_id, base_tip_id, fingerprint))
}

fn load_recent_authored_commits_from_context(
    repo: &gix::Repository,
    tip_id: gix::ObjectId,
    base_tip_id: Option<gix::ObjectId>,
    limit: usize,
) -> Result<Vec<RecentCommitSummary>> {
    if tip_id.is_null() || limit == 0 {
        return Ok(Vec::new());
    }
    collect_recent_authored_commits(repo, tip_id, base_tip_id, limit)
}

fn collect_recent_authored_commits(
    repo: &gix::Repository,
    tip_id: gix::ObjectId,
    base_tip_id: Option<gix::ObjectId>,
    limit: usize,
) -> Result<Vec<RecentCommitSummary>> {
    let walk_builder = repo.rev_walk([tip_id]);
    let walk_builder = if let Some(base_tip_id) = base_tip_id {
        walk_builder.with_hidden([base_tip_id])
    } else {
        walk_builder
    };
    let walk = walk_builder
        .sorting(gix::revision::walk::Sorting::ByCommitTime(
            CommitTimeOrder::NewestFirst,
        ))
        .all()
        .context("failed to start Git recent-commit traversal")?;
    let mut commits = Vec::with_capacity(limit);

    for info in walk {
        let info = info.context("failed to walk recent Git history")?;
        let commit = info
            .object()
            .with_context(|| format!("failed to load commit {}", info.id))?;

        commits.push(RecentCommitSummary {
            commit_id: info.id.to_string(),
            subject: commit_subject(&commit),
            committed_unix_time: Some(info.commit_time()),
        });
        if commits.len() >= limit {
            break;
        }
    }

    Ok(commits)
}

fn branch_base_tip_id(
    repo: &gix::Repository,
    head_ref_name: Option<&str>,
    head_commit_id: gix::ObjectId,
) -> Result<Option<gix::ObjectId>> {
    let Some(head_ref_name) = head_ref_name else {
        return Ok(None);
    };
    let Some(current_branch_name) = short_branch_name(head_ref_name) else {
        return Ok(None);
    };

    if let Some(base_tip_id) =
        remote_default_branch_tip_id(repo, current_branch_name, head_ref_name, head_commit_id)?
    {
        return Ok(Some(base_tip_id));
    }
    if matches!(current_branch_name, "main" | "master") {
        return Ok(None);
    }

    for candidate in ["main", "master"] {
        if candidate == current_branch_name {
            continue;
        }
        if let Some(base_tip_id) =
            peel_reference_to_id(repo, format!("refs/heads/{candidate}").as_str())?
        {
            return Ok(Some(base_tip_id));
        }
    }

    Ok(None)
}

fn remote_default_branch_tip_id(
    repo: &gix::Repository,
    current_branch_name: &str,
    head_ref_name: &str,
    head_commit_id: gix::ObjectId,
) -> Result<Option<gix::ObjectId>> {
    let head_ref_name = <&gix::refs::FullNameRef>::try_from(head_ref_name).map_err(|err| {
        anyhow::anyhow!("failed to validate current Git branch reference name: {err}")
    })?;
    let Some(tracking_ref_name) = repo
        .branch_remote_tracking_ref_name(head_ref_name, gix::remote::Direction::Fetch)
        .transpose()
        .context("failed to resolve Git tracking branch for recent commits")?
    else {
        return Ok(None);
    };
    let tracking_ref_name = tracking_ref_name.to_string();
    let Some(remote_name) = tracking_ref_name
        .strip_prefix("refs/remotes/")
        .and_then(|name| name.split('/').next())
    else {
        return Ok(None);
    };
    let default_remote_head_ref = format!("refs/remotes/{remote_name}/HEAD");
    let Some(mut default_remote_head) = find_reference(repo, default_remote_head_ref.as_str())
    else {
        return Ok(None);
    };
    if default_remote_head
        .target()
        .try_name()
        .map(|name| name.to_string())
        .and_then(|name| {
            name.strip_prefix(format!("refs/remotes/{remote_name}/").as_str())
                .map(str::to_owned)
        })
        .as_deref()
        == Some(current_branch_name)
    {
        return Ok(None);
    }
    let Ok(base_tip_id) = default_remote_head.peel_to_id() else {
        return Ok(None);
    };
    let base_tip_id = base_tip_id.detach();
    if base_tip_id == head_commit_id {
        return Ok(None);
    }
    Ok(Some(base_tip_id))
}

fn find_reference<'repo>(
    repo: &'repo gix::Repository,
    ref_name: &str,
) -> Option<gix::Reference<'repo>> {
    repo.find_reference(ref_name).ok()
}

fn peel_reference_to_id(repo: &gix::Repository, ref_name: &str) -> Result<Option<gix::ObjectId>> {
    let Some(mut reference) = find_reference(repo, ref_name) else {
        return Ok(None);
    };
    let Ok(id) = reference.peel_to_id() else {
        return Ok(None);
    };
    Ok(Some(id.detach()))
}

fn short_branch_name(head_ref_name: &str) -> Option<&str> {
    head_ref_name.strip_prefix("refs/heads/")
}

fn commit_subject(commit: &gix::Commit<'_>) -> String {
    String::from_utf8_lossy(commit.message_raw_sloppy().as_ref())
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "(no subject)".to_string())
}
