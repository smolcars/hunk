use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, Default)]
struct SshConfigMatch {
    user: Option<String>,
    identity_files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct SshTarget {
    host_alias: String,
    username: String,
    port: Option<u16>,
}

#[derive(Debug, Clone)]
struct ParsedSshRemote {
    host_alias: String,
    username: Option<String>,
    port: Option<u16>,
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
    sync_branch_from_remote(repo_root, branch_name)
}

pub fn sync_branch_from_remote(repo_root: &Path, branch_name: &str) -> Result<()> {
    let branch_name = normalized_branch_name(branch_name)?;
    let repo = open_repo(repo_root)?;
    let upstream = resolve_upstream_target(&repo, branch_name)?
        .ok_or_else(|| anyhow!("no upstream branch to sync from"))?;

    fetch_upstream(&repo, &upstream)?;

    let local_branch = repo
        .find_branch(branch_name, BranchType::Local)
        .with_context(|| format!("branch '{branch_name}' does not exist"))?;
    let branch_is_head = local_branch.is_head();
    if branch_is_head {
        ensure_sync_worktree_is_clean(&repo)?;
    }
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
            branch_is_head,
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
    let ssh_target = (allowed.contains(CredentialType::USERNAME)
        || allowed.contains(CredentialType::SSH_KEY))
    .then(|| ssh_target(url, username_from_url, config))
    .flatten();

    if allowed.contains(CredentialType::USERNAME)
        && let Some(username) =
            username_from_url.or_else(|| ssh_target.as_ref().map(|target| target.username.as_str()))
    {
        return Cred::username(username);
    }

    if allowed.contains(CredentialType::SSH_KEY) {
        let username = ssh_target
            .as_ref()
            .map(|target| target.username.as_str())
            .unwrap_or_else(|| username_from_url.unwrap_or("git"));
        if let Ok(cred) = Cred::ssh_key_from_agent(username) {
            return Ok(cred);
        }
        if let Some(target) = ssh_target.as_ref() {
            for identity in ssh_identity_candidates(target) {
                let public_key = public_key_path(identity.as_path());
                if let Ok(cred) = Cred::ssh_key(
                    target.username.as_str(),
                    public_key.as_deref(),
                    identity.as_path(),
                    None,
                ) {
                    return Ok(cred);
                }
            }
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
        return Cred::username(
            ssh_target
                .as_ref()
                .map(|target| target.username.as_str())
                .unwrap_or_else(|| username_from_url.unwrap_or("git")),
        );
    }

    Err(git2::Error::from_str(
        "failed to acquire credentials for Git remote operation",
    ))
}

fn ssh_target(
    url: &str,
    username_from_url: Option<&str>,
    config: &git2::Config,
) -> Option<SshTarget> {
    let parsed = parse_ssh_remote(url)?;
    let lookup_target = SshTarget {
        host_alias: parsed.host_alias.clone(),
        username: parsed.username.clone().unwrap_or_else(|| "git".to_string()),
        port: parsed.port,
    };
    let ssh_config = load_matching_ssh_config(&lookup_target).ok()?;
    let username = username_from_url
        .map(str::to_owned)
        .or(parsed.username)
        .or(ssh_config.user)
        .or_else(|| git_config_ssh_user(config, parsed.host_alias.as_str()))
        .unwrap_or_else(|| "git".to_string());

    Some(SshTarget {
        host_alias: parsed.host_alias,
        username,
        port: parsed.port,
    })
}

fn ssh_identity_candidates(target: &SshTarget) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(config_match) = load_matching_ssh_config(target) {
        candidates.extend(config_match.identity_files);
    }

    if let Some(ssh_dir) = default_ssh_dir() {
        for name in ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"] {
            candidates.push(ssh_dir.join(name));
        }
    }

    dedupe_existing_files(candidates)
}

fn git_config_ssh_user(config: &git2::Config, host_alias: &str) -> Option<String> {
    for key in [
        format!("hunk.ssh.{host_alias}.user"),
        format!("remote.{host_alias}.user"),
    ] {
        if let Ok(value) = config.get_string(key.as_str()) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn load_matching_ssh_config(target: &SshTarget) -> Result<SshConfigMatch> {
    let Some(config_path) = default_ssh_dir().map(|dir| dir.join("config")) else {
        return Ok(SshConfigMatch::default());
    };
    let contents = match std::fs::read_to_string(config_path.as_path()) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SshConfigMatch::default());
        }
        Err(err) => {
            return Err(err).with_context(|| {
                format!("failed to read SSH config at {}", config_path.display())
            });
        }
    };

    let mut applies = true;
    let mut matched = SshConfigMatch::default();
    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = split_ssh_config_line(line) else {
            continue;
        };
        if key.eq_ignore_ascii_case("Host") {
            applies = value
                .split_whitespace()
                .any(|pattern| ssh_host_pattern_matches(target.host_alias.as_str(), pattern));
            continue;
        }
        if !applies {
            continue;
        }

        if key.eq_ignore_ascii_case("User") {
            if matched.user.is_none() {
                let user = value.trim_matches('"').trim();
                if !user.is_empty() {
                    matched.user = Some(user.to_string());
                }
            }
            continue;
        }

        if key.eq_ignore_ascii_case("IdentityFile") {
            let value = value.trim_matches('"').trim();
            if value.is_empty() {
                continue;
            }
            if let Some(path) = expand_ssh_identity_path(value, target) {
                matched.identity_files.push(path);
            }
        }
    }

    matched.identity_files = dedupe_existing_files(matched.identity_files);
    Ok(matched)
}

fn split_ssh_config_line(line: &str) -> Option<(&str, &str)> {
    if let Some(index) = line.find(char::is_whitespace) {
        let (key, value) = line.split_at(index);
        return Some((key.trim(), value.trim()));
    }
    line.split_once('=')
        .map(|(key, value)| (key.trim(), value.trim()))
}

fn ssh_host_pattern_matches(host: &str, pattern: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    let pattern = pattern.trim().to_ascii_lowercase();
    if host.is_empty() || pattern.is_empty() {
        return false;
    }
    wildcard_match(pattern.as_bytes(), host.as_bytes())
}

fn wildcard_match(pattern: &[u8], value: &[u8]) -> bool {
    let (mut pattern_ix, mut value_ix) = (0usize, 0usize);
    let mut star_ix = None;
    let mut match_ix = 0usize;

    while value_ix < value.len() {
        if pattern_ix < pattern.len()
            && (pattern[pattern_ix] == b'?'
                || pattern[pattern_ix].eq_ignore_ascii_case(&value[value_ix]))
        {
            pattern_ix += 1;
            value_ix += 1;
            continue;
        }
        if pattern_ix < pattern.len() && pattern[pattern_ix] == b'*' {
            star_ix = Some(pattern_ix);
            pattern_ix += 1;
            match_ix = value_ix;
            continue;
        }
        if let Some(star_ix) = star_ix {
            pattern_ix = star_ix + 1;
            match_ix += 1;
            value_ix = match_ix;
            continue;
        }
        return false;
    }

    while pattern_ix < pattern.len() && pattern[pattern_ix] == b'*' {
        pattern_ix += 1;
    }

    pattern_ix == pattern.len()
}

fn expand_ssh_identity_path(raw_value: &str, target: &SshTarget) -> Option<PathBuf> {
    let home_dir = default_home_dir()?;
    let port = target.port.unwrap_or(22).to_string();
    let replaced = raw_value
        .replace("%d", home_dir.to_string_lossy().as_ref())
        .replace("%h", target.host_alias.as_str())
        .replace("%r", target.username.as_str())
        .replace("%p", port.as_str());
    let path = if replaced == "~" {
        home_dir
    } else if let Some(stripped) = replaced.strip_prefix("~/") {
        home_dir.join(stripped)
    } else if Path::new(replaced.as_str()).is_absolute() {
        PathBuf::from(replaced)
    } else {
        home_dir.join(replaced)
    };
    Some(path)
}

fn default_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn default_ssh_dir() -> Option<PathBuf> {
    default_home_dir().map(|home| home.join(".ssh"))
}

fn dedupe_existing_files(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::BTreeSet::new();
    let mut deduped = Vec::new();
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let key = path.to_string_lossy().into_owned();
        if seen.insert(key) {
            deduped.push(path);
        }
    }
    deduped
}

fn public_key_path(private_key: &Path) -> Option<PathBuf> {
    let public_key = PathBuf::from(format!("{}.pub", private_key.display()));
    public_key.is_file().then_some(public_key)
}

fn parse_ssh_remote(url: &str) -> Option<ParsedSshRemote> {
    if let Some(stripped) = url.strip_prefix("ssh://") {
        let (authority, _) = stripped.split_once('/')?;
        let (username, host_alias, port) = parse_ssh_authority(authority)?;
        return Some(ParsedSshRemote {
            host_alias,
            username,
            port,
        });
    }

    let (authority, _) = split_scp_like_remote(url)?;
    let (username, host_alias, port) = parse_ssh_authority(authority)?;
    Some(ParsedSshRemote {
        host_alias,
        username,
        port,
    })
}

fn split_scp_like_remote(url: &str) -> Option<(&str, &str)> {
    if url.contains("://") {
        return None;
    }

    let (authority, path) = url.split_once(':')?;
    if authority.is_empty() || path.is_empty() || authority.contains('/') {
        return None;
    }
    if authority.len() == 1 && authority.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return None;
    }
    Some((authority, path))
}

fn parse_ssh_authority(authority: &str) -> Option<(Option<String>, String, Option<u16>)> {
    let (username, host_port) = authority
        .split_once('@')
        .map_or((None, authority), |(user, rest)| {
            (Some(user.to_string()), rest)
        });
    if host_port.is_empty() {
        return None;
    }

    if let Some(stripped) = host_port.strip_prefix('[') {
        let (host, remainder) = stripped.split_once(']')?;
        let port = remainder
            .strip_prefix(':')
            .and_then(|value| value.parse().ok());
        return Some((username, host.to_ascii_lowercase(), port));
    }

    let (host, port) = host_port
        .split_once(':')
        .map_or((host_port, None), |(host, port)| (host, port.parse().ok()));
    if host.is_empty() {
        return None;
    }
    Some((username, host.to_ascii_lowercase(), port))
}
