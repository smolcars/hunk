use std::path::Path;

use anyhow::{Context as _, Result, anyhow};

use crate::config::{ReviewProviderKind, ReviewProviderMapping};
use crate::git::open_repo_at_root;
use crate::git2_helpers::open_git2_repo;

const RESERVED_BRANCH_NAMES: &[&str] = &["detached", "unknown"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRemote {
    pub provider: ReviewProviderKind,
    pub host: String,
    pub authority: String,
    pub repository_path: String,
    pub base_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedReviewRemote {
    host: String,
    authority: String,
    repository_path: String,
    base_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameBranchIfSafeOutcome {
    Renamed,
    Skipped(RenameBranchSkipReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameBranchSkipReason {
    UnchangedBranchName,
    InvalidTargetName,
    CurrentBranchChanged,
    CurrentBranchPublished,
    TargetAlreadyExists,
}

pub fn sanitize_branch_name(input: &str) -> String {
    let lowered = input.trim().to_lowercase();

    let mut normalized = String::with_capacity(lowered.len());
    let mut last_dash = false;
    for ch in lowered.chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' | '/' | '.' | '_' | '-' => ch,
            c if c.is_whitespace() => '-',
            _ => '-',
        };

        if mapped == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }

        normalized.push(mapped);
    }

    let mut segments = Vec::new();
    for segment in normalized.split('/') {
        let mut clean = segment
            .trim_matches(|c: char| c == '-' || c == '.')
            .replace("@{", "-")
            .replace(['~', '^', ':', '?', '*', '[', '\\'], "-");

        while clean.contains("--") {
            clean = clean.replace("--", "-");
        }

        while clean.contains("..") {
            clean = clean.replace("..", ".");
        }

        if clean.ends_with(".lock") {
            clean = clean
                .trim_end_matches(".lock")
                .trim_end_matches('.')
                .to_string();
        }

        if !clean.is_empty() {
            segments.push(clean);
        }
    }

    let mut candidate = if segments.is_empty() {
        "branch".to_string()
    } else {
        segments.join("/")
    };

    if candidate.eq_ignore_ascii_case("head") {
        candidate = "head-branch".to_string();
    }
    if is_reserved_branch_name(candidate.as_str()) {
        candidate = format!("{candidate}-branch");
    }

    candidate = candidate.trim_matches('/').to_string();

    if !is_valid_branch_name(&candidate) {
        candidate = candidate
            .chars()
            .map(|ch| match ch {
                'a'..='z' | '0'..='9' | '-' | '_' | '.' | '/' => ch,
                _ => '-',
            })
            .collect::<String>();

        candidate = candidate
            .split('/')
            .filter_map(|segment| {
                let segment = segment.trim_matches(|c: char| c == '-' || c == '.');
                if segment.is_empty() {
                    None
                } else {
                    Some(segment.to_string())
                }
            })
            .collect::<Vec<_>>()
            .join("/");
    }

    if candidate.is_empty() {
        candidate = "branch".to_string();
    }

    if !is_valid_branch_name(&candidate) {
        "branch-new".to_string()
    } else {
        candidate
    }
}

pub fn is_valid_branch_name(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    if is_reserved_branch_name(name) || name.eq_ignore_ascii_case("head") {
        return false;
    }
    if name.starts_with('/') || name.ends_with('/') {
        return false;
    }
    if name.starts_with('.') || name.ends_with('.') {
        return false;
    }
    if name.contains("//") || name.contains("..") || name.contains("@{") || name.ends_with(".lock")
    {
        return false;
    }
    if name.chars().any(|ch| {
        ch.is_ascii_control()
            || ch.is_whitespace()
            || matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    }) {
        return false;
    }
    if !name.split('/').all(|segment| {
        !segment.is_empty()
            && !segment.starts_with('.')
            && !segment.ends_with('.')
            && segment != "@"
    }) {
        return false;
    }

    format!("refs/heads/{name}")
        .try_into()
        .map(|_: gix::refs::FullName| ())
        .is_ok()
}

pub fn rename_branch(repo_root: &Path, old_branch_name: &str, new_branch_name: &str) -> Result<()> {
    let old_branch_name = old_branch_name.trim();
    if old_branch_name.is_empty() {
        return Err(anyhow!("current branch name cannot be empty"));
    }

    let new_branch_name = new_branch_name.trim();
    if new_branch_name.is_empty() {
        return Err(anyhow!("new branch name cannot be empty"));
    }
    if old_branch_name == new_branch_name {
        return Err(anyhow!("new branch name must differ from current branch"));
    }
    if !is_valid_branch_name(new_branch_name) {
        return Err(anyhow!("invalid branch name: {new_branch_name}"));
    }

    let repo = open_git2_repo(repo_root)?;
    if repo
        .find_branch(new_branch_name, git2::BranchType::Local)
        .is_ok()
    {
        return Err(anyhow!("branch '{new_branch_name}' already exists"));
    }

    let mut branch = repo
        .find_branch(old_branch_name, git2::BranchType::Local)
        .with_context(|| format!("branch '{old_branch_name}' does not exist"))?;
    let had_upstream = branch_has_upstream(&branch)
        .with_context(|| format!("failed to inspect upstream for branch '{old_branch_name}'"))?;
    let mut renamed = branch
        .rename(new_branch_name, false)
        .with_context(|| format!("failed to rename branch '{old_branch_name}'"))?;
    if had_upstream
        && let Err(err) = renamed.set_upstream(None)
        && err.code() != git2::ErrorCode::NotFound
        && err.class() != git2::ErrorClass::Config
    {
        return Err(err)
            .with_context(|| format!("failed to clear upstream for branch '{new_branch_name}'"));
    }
    Ok(())
}

pub fn rename_branch_if_current_unpublished(
    repo_root: &Path,
    expected_current_branch_name: &str,
    new_branch_name: &str,
) -> Result<RenameBranchIfSafeOutcome> {
    let expected_current_branch_name = expected_current_branch_name.trim();
    if expected_current_branch_name.is_empty() {
        return Err(anyhow!("expected current branch name cannot be empty"));
    }

    let new_branch_name = new_branch_name.trim();
    if new_branch_name.is_empty() {
        return Ok(RenameBranchIfSafeOutcome::Skipped(
            RenameBranchSkipReason::InvalidTargetName,
        ));
    }
    if expected_current_branch_name == new_branch_name {
        return Ok(RenameBranchIfSafeOutcome::Skipped(
            RenameBranchSkipReason::UnchangedBranchName,
        ));
    }
    if !is_valid_branch_name(new_branch_name) {
        return Ok(RenameBranchIfSafeOutcome::Skipped(
            RenameBranchSkipReason::InvalidTargetName,
        ));
    }

    let repo = git2::Repository::open(repo_root)
        .with_context(|| format!("failed to open Git repository at {}", repo_root.display()))?;

    let Some(current_branch_name) = checked_out_branch_name(&repo)? else {
        return Ok(RenameBranchIfSafeOutcome::Skipped(
            RenameBranchSkipReason::CurrentBranchChanged,
        ));
    };
    if current_branch_name != expected_current_branch_name {
        return Ok(RenameBranchIfSafeOutcome::Skipped(
            RenameBranchSkipReason::CurrentBranchChanged,
        ));
    }

    match repo.find_branch(new_branch_name, git2::BranchType::Local) {
        Ok(_) => {
            return Ok(RenameBranchIfSafeOutcome::Skipped(
                RenameBranchSkipReason::TargetAlreadyExists,
            ));
        }
        Err(err) if err.code() == git2::ErrorCode::NotFound => {}
        Err(err) => {
            return Err(err).with_context(|| {
                format!("failed to check whether branch '{new_branch_name}' already exists")
            });
        }
    }

    let mut branch = repo
        .find_branch(expected_current_branch_name, git2::BranchType::Local)
        .with_context(|| format!("branch '{expected_current_branch_name}' does not exist"))?;
    let branch_is_published = branch_has_upstream(&branch).with_context(|| {
        format!("failed to inspect upstream for branch '{expected_current_branch_name}'")
    })? || branch_has_known_remote_counterpart(
        &repo,
        expected_current_branch_name,
    )
    .with_context(|| {
        format!(
            "failed to inspect remote tracking state for branch '{expected_current_branch_name}'"
        )
    })?;
    if branch_is_published {
        return Ok(RenameBranchIfSafeOutcome::Skipped(
            RenameBranchSkipReason::CurrentBranchPublished,
        ));
    }

    let mut renamed = branch
        .rename(new_branch_name, false)
        .with_context(|| format!("failed to rename branch '{expected_current_branch_name}'"))?;
    if let Err(err) = renamed.set_upstream(None)
        && err.code() != git2::ErrorCode::NotFound
        && err.class() != git2::ErrorClass::Config
    {
        return Err(err)
            .with_context(|| format!("failed to clear upstream for branch '{new_branch_name}'"));
    }
    Ok(RenameBranchIfSafeOutcome::Renamed)
}

pub fn review_url_for_branch(repo_root: &Path, branch_name: &str) -> Result<Option<String>> {
    review_url_for_branch_with_provider_map(repo_root, branch_name, &[])
}

pub fn review_url_for_branch_with_provider_map(
    repo_root: &Path,
    branch_name: &str,
    provider_mappings: &[ReviewProviderMapping],
) -> Result<Option<String>> {
    Ok(
        review_remote_for_branch_with_provider_map(repo_root, branch_name, provider_mappings)?
            .map(|remote| review_url_for_review_remote(&remote, branch_name)),
    )
}

pub fn review_remote_for_branch(
    repo_root: &Path,
    branch_name: &str,
) -> Result<Option<ReviewRemote>> {
    review_remote_for_branch_with_provider_map(repo_root, branch_name, &[])
}

pub fn review_remote_for_named_remote_with_provider_map(
    repo_root: &Path,
    remote_name: &str,
    provider_mappings: &[ReviewProviderMapping],
) -> Result<Option<ReviewRemote>> {
    let remote_name = remote_name.trim();
    if remote_name.is_empty() {
        return Err(anyhow!(
            "cannot resolve review remote without a remote name"
        ));
    }

    let repo = open_repo_at_root(repo_root)?;
    review_remote_for_named_remote(repo.repository(), remote_name, provider_mappings)
}

pub fn review_remote_for_branch_with_provider_map(
    repo_root: &Path,
    branch_name: &str,
    provider_mappings: &[ReviewProviderMapping],
) -> Result<Option<ReviewRemote>> {
    let branch_name = branch_name.trim();
    if branch_name.is_empty() || branch_name == "detached" {
        return Err(anyhow!(
            "cannot resolve review remote without a branch name"
        ));
    }

    let repo = open_repo_at_root(repo_root)?;
    let remote = resolve_review_remote(repo.repository(), branch_name)?;
    let Some(remote_url) = remote
        .url(gix::remote::Direction::Push)
        .or_else(|| remote.url(gix::remote::Direction::Fetch))
    else {
        return Ok(None);
    };
    let remote_url = remote_url.to_string();

    Ok(review_remote_for_remote(
        remote_url.as_str(),
        provider_mappings,
    ))
}

fn is_reserved_branch_name(name: &str) -> bool {
    RESERVED_BRANCH_NAMES
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

fn checked_out_branch_name(repo: &git2::Repository) -> Result<Option<String>> {
    match repo.head() {
        Ok(head) => {
            if !head.is_branch() {
                return Ok(None);
            }
            Ok(head
                .shorthand()
                .map(str::to_string)
                .filter(|name| !name.is_empty()))
        }
        Err(err) if err.code() == git2::ErrorCode::UnbornBranch => Ok(None),
        Err(err) => Err(err).context("failed to resolve current branch"),
    }
}

fn branch_has_upstream(branch: &git2::Branch<'_>) -> Result<bool> {
    match branch.upstream() {
        Ok(_) => Ok(true),
        Err(err)
            if err.code() == git2::ErrorCode::NotFound
                || err.class() == git2::ErrorClass::Config =>
        {
            Ok(false)
        }
        Err(err) => Err(err.into()),
    }
}

fn branch_has_known_remote_counterpart(repo: &git2::Repository, branch_name: &str) -> Result<bool> {
    let remote_branches = repo
        .branches(Some(git2::BranchType::Remote))
        .context("failed to enumerate remote branches")?;
    for remote_branch in remote_branches {
        let (remote_branch, _) =
            remote_branch.context("failed to inspect remote branch reference")?;
        let Some(remote_branch_name) = remote_branch
            .name()
            .context("failed to inspect remote branch name")?
        else {
            continue;
        };
        let Some((_, remote_branch_suffix)) = remote_branch_name.split_once('/') else {
            continue;
        };
        if remote_branch_suffix != branch_name {
            continue;
        }
        return Ok(true);
    }

    Ok(false)
}

fn resolve_review_remote<'repo>(
    repo: &'repo gix::Repository,
    branch_name: &str,
) -> Result<gix::Remote<'repo>> {
    if let Some(remote) = repo.branch_remote(branch_name, gix::remote::Direction::Push) {
        return remote.context("failed to resolve Git push remote for review URL");
    }
    if let Some(remote) = repo.branch_remote(branch_name, gix::remote::Direction::Fetch) {
        return remote.context("failed to resolve Git fetch remote for review URL");
    }
    if let Some(remote) = repo.find_default_remote(gix::remote::Direction::Push) {
        return remote.context("failed to resolve default Git remote for review URL");
    }

    if let Some(name) = repo.remote_names().into_iter().next() {
        return repo
            .find_remote(name.as_ref())
            .context("failed to resolve configured Git remote for review URL");
    }

    Err(anyhow!("no Git remote configured for push"))
}

fn review_remote_for_named_remote(
    repo: &gix::Repository,
    remote_name: &str,
    provider_mappings: &[ReviewProviderMapping],
) -> Result<Option<ReviewRemote>> {
    if !repo
        .remote_names()
        .into_iter()
        .any(|name| name.as_ref() == remote_name)
    {
        return Ok(None);
    }

    let remote = repo
        .find_remote(remote_name)
        .with_context(|| format!("failed to resolve Git remote '{remote_name}' for review URL"))?;
    let Some(remote_url) = remote
        .url(gix::remote::Direction::Push)
        .or_else(|| remote.url(gix::remote::Direction::Fetch))
    else {
        return Ok(None);
    };

    Ok(review_remote_for_remote(
        remote_url.to_string().as_str(),
        provider_mappings,
    ))
}

fn review_remote_for_remote(
    remote_url: &str,
    provider_mappings: &[ReviewProviderMapping],
) -> Option<ReviewRemote> {
    let normalized = normalized_remote_base(remote_url)?;
    let provider = review_provider_from_host(normalized.host.as_str(), provider_mappings)?;
    Some(ReviewRemote {
        provider,
        host: normalized.host,
        authority: normalized.authority,
        repository_path: normalized.repository_path,
        base_url: normalized.base_url,
    })
}

fn review_url_for_review_remote(remote: &ReviewRemote, branch_name: &str) -> String {
    let encoded_branch = percent_encode(branch_name);
    match remote.provider {
        ReviewProviderKind::GitLab => format!(
            "{}/-/merge_requests/new?merge_request[source_branch]={encoded_branch}",
            remote.base_url
        ),
        ReviewProviderKind::GitHub => {
            format!("{}/compare/{encoded_branch}?expand=1", remote.base_url)
        }
    }
}

fn normalized_remote_base(remote_url: &str) -> Option<NormalizedReviewRemote> {
    if let Some((scheme, rest)) = remote_url
        .strip_prefix("https://")
        .map(|rest| ("https", rest))
        .or_else(|| {
            remote_url
                .strip_prefix("http://")
                .map(|rest| ("http", rest))
        })
        && let Some((authority, path)) = split_authority_and_path(rest)
    {
        let (host, authority) = sanitize_authority(authority)?;
        return normalized_remote_base_with_parts(scheme, host, authority, path);
    }

    if let Some(stripped) = remote_url.strip_prefix("ssh://") {
        let after_user = stripped
            .split_once('@')
            .map_or(stripped, |(_, remainder)| remainder);
        let (authority, path) = split_authority_and_path(after_user)?;
        let (host, authority) = sanitize_authority(authority)?;
        return normalized_remote_base_with_parts("https", host, authority, path);
    }

    if let Some((authority, path)) = split_scp_like(remote_url) {
        let (host, authority) = sanitize_authority(authority)?;
        return normalized_remote_base_with_parts("https", host, authority, path);
    }

    None
}

fn normalized_remote_base_with_parts(
    scheme: &str,
    host: String,
    authority: String,
    path: &str,
) -> Option<NormalizedReviewRemote> {
    let repository_path = trim_remote_path(path);
    if repository_path.is_empty() {
        return None;
    }
    Some(NormalizedReviewRemote {
        host,
        authority: authority.clone(),
        base_url: format!("{scheme}://{authority}/{repository_path}"),
        repository_path,
    })
}

fn split_authority_and_path(value: &str) -> Option<(&str, &str)> {
    value.split_once('/')
}

fn split_scp_like(remote_url: &str) -> Option<(&str, &str)> {
    if remote_url.contains("://") {
        return None;
    }

    let (authority, path) = remote_url.split_once(':')?;
    if authority.is_empty() || path.is_empty() {
        return None;
    }
    if authority.contains('/') {
        return None;
    }
    if authority.len() == 1 && authority.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return None;
    }

    Some((authority, path))
}

fn sanitize_authority(authority: &str) -> Option<(String, String)> {
    let without_user = authority.rsplit('@').next().unwrap_or(authority);
    if without_user.is_empty() {
        return None;
    }

    let (host, authority) = if without_user.starts_with('[') {
        let host = without_user
            .split_once(']')
            .map(|(host, _)| format!("{host}]"))
            .filter(|host| !host.is_empty())?
            .to_ascii_lowercase();
        (host, without_user.to_ascii_lowercase())
    } else {
        let host = without_user.split(':').next()?.to_ascii_lowercase();
        (host, without_user.to_ascii_lowercase())
    };

    (!host.is_empty() && !authority.is_empty()).then_some((host, authority))
}

fn trim_remote_path(path: &str) -> String {
    path.trim_start_matches('/')
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string()
}

fn review_provider_from_host(
    host: &str,
    provider_mappings: &[ReviewProviderMapping],
) -> Option<ReviewProviderKind> {
    if let Some(provider) = review_provider_from_mappings(host, provider_mappings) {
        return Some(provider);
    }

    if host.contains("gitlab") {
        Some(ReviewProviderKind::GitLab)
    } else if host.contains("github") {
        Some(ReviewProviderKind::GitHub)
    } else {
        None
    }
}

fn review_provider_from_mappings(
    host: &str,
    provider_mappings: &[ReviewProviderMapping],
) -> Option<ReviewProviderKind> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }

    provider_mappings
        .iter()
        .find(|mapping| host_matches_provider_pattern(host.as_str(), mapping.host.as_str()))
        .map(|mapping| mapping.provider)
}

fn host_matches_provider_pattern(host: &str, raw_pattern: &str) -> bool {
    let pattern = raw_pattern
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        if host == suffix {
            return true;
        }
        if host.len() <= suffix.len() || !host.ends_with(suffix) {
            return false;
        }
        let separator_ix = host.len().saturating_sub(suffix.len() + 1);
        return host.as_bytes().get(separator_ix) == Some(&b'.');
    }

    host == pattern
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let is_unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(format!("%{byte:02X}").as_str());
        }
    }
    encoded
}
