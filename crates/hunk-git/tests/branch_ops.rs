use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use git2::{
    BranchType, IndexAddOption, Repository, Signature, WorktreeAddOptions, build::CheckoutBuilder,
};
use hunk_domain::config::{ReviewProviderKind, ReviewProviderMapping};
use hunk_git::branch::{
    RenameBranchIfSafeOutcome, RenameBranchSkipReason, rename_branch,
    rename_branch_if_current_unpublished, review_remote_for_branch_with_provider_map,
    review_remote_for_named_remote_with_provider_map, review_url_for_branch,
    review_url_for_branch_with_provider_map, sanitize_branch_name,
};
use hunk_git::git::load_workflow_snapshot;
use tempfile::TempDir;

#[test]
fn sanitize_branch_name_normalizes_invalid_input() {
    assert_eq!(
        sanitize_branch_name(" Feature / My branch "),
        "feature/my-branch"
    );
    assert_eq!(sanitize_branch_name("HEAD"), "head-branch");
    assert_eq!(sanitize_branch_name("   "), "branch");
    assert_eq!(sanitize_branch_name("detached"), "detached-branch");
}

#[test]
fn rename_branch_updates_head_and_clears_upstream_tracking() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature-old")?;
    fixture.create_bare_remote("origin")?;
    fixture.push_current_branch("origin", "feature-old")?;
    fixture.set_upstream("feature-old", "origin/feature-old")?;

    rename_branch(fixture.root(), "feature-old", "feature-new")?;

    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_name, "feature-new");
    assert!(!snapshot.branch_has_upstream);
    assert!(
        snapshot
            .branches
            .iter()
            .any(|branch| { branch.name == "feature-new" && branch.is_current })
    );
    assert!(
        snapshot
            .branches
            .iter()
            .all(|branch| branch.name != "feature-old")
    );
    Ok(())
}

#[test]
fn rename_branch_rejects_existing_target() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature-old")?;
    fixture.checkout_branch("feature-existing")?;

    let err = rename_branch(fixture.root(), "feature-old", "feature-existing")
        .expect_err("existing destination should fail");
    assert!(err.to_string().contains("already exists"));
    Ok(())
}

#[test]
fn rename_branch_if_current_unpublished_renames_linked_worktree_branch() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("main")?;
    let worktree_root = fixture.add_worktree("worktree-task", "feature-old")?;

    let outcome = rename_branch_if_current_unpublished(
        worktree_root.as_path(),
        "feature-old",
        "feature-new",
    )?;

    assert_eq!(outcome, RenameBranchIfSafeOutcome::Renamed);
    let snapshot = load_workflow_snapshot(worktree_root.as_path())?;
    assert_eq!(snapshot.branch_name, "feature-new");
    assert!(
        snapshot
            .branches
            .iter()
            .any(|branch| { branch.name == "feature-new" && branch.is_current })
    );
    assert!(
        snapshot
            .branches
            .iter()
            .all(|branch| branch.name != "feature-old")
    );
    Ok(())
}

#[test]
fn rename_branch_if_current_unpublished_skips_when_current_branch_changed() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("main")?;
    let worktree_root = fixture.add_worktree("worktree-task", "feature-old")?;
    let worktree_repo = Repository::open(worktree_root.as_path())?;
    let worktree_head = worktree_repo.head()?.peel_to_commit()?;
    worktree_repo.branch("feature-other", &worktree_head, false)?;
    worktree_repo.set_head("refs/heads/feature-other")?;
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    worktree_repo.checkout_head(Some(&mut checkout))?;

    let outcome = rename_branch_if_current_unpublished(
        worktree_root.as_path(),
        "feature-old",
        "feature-new",
    )?;

    assert_eq!(
        outcome,
        RenameBranchIfSafeOutcome::Skipped(RenameBranchSkipReason::CurrentBranchChanged)
    );
    let snapshot = load_workflow_snapshot(worktree_root.as_path())?;
    assert_eq!(snapshot.branch_name, "feature-other");
    Ok(())
}

#[test]
fn rename_branch_if_current_unpublished_skips_published_branch() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature-old")?;
    fixture.create_bare_remote("origin")?;
    fixture.push_current_branch("origin", "feature-old")?;
    fixture.set_upstream("feature-old", "origin/feature-old")?;

    let outcome =
        rename_branch_if_current_unpublished(fixture.root(), "feature-old", "feature-new")?;

    assert_eq!(
        outcome,
        RenameBranchIfSafeOutcome::Skipped(RenameBranchSkipReason::CurrentBranchPublished)
    );
    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_name, "feature-old");
    assert!(snapshot.branch_has_upstream);
    Ok(())
}

#[test]
fn rename_branch_if_current_unpublished_skips_known_remote_branch_without_upstream() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature-old")?;
    fixture.create_bare_remote("origin")?;
    fixture.push_current_branch("origin", "feature-old")?;
    fixture.set_remote_tracking_ref("origin", "feature-old")?;

    let outcome =
        rename_branch_if_current_unpublished(fixture.root(), "feature-old", "feature-new")?;

    assert_eq!(
        outcome,
        RenameBranchIfSafeOutcome::Skipped(RenameBranchSkipReason::CurrentBranchPublished)
    );
    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_name, "feature-old");
    assert!(!snapshot.branch_has_upstream);
    Ok(())
}

#[test]
fn rename_branch_if_current_unpublished_skips_known_remote_branch_after_local_commit() -> Result<()>
{
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature-old")?;
    fixture.create_bare_remote("origin")?;
    fixture.push_current_branch("origin", "feature-old")?;
    fixture.set_remote_tracking_ref("origin", "feature-old")?;
    fixture.write_file("tracked.txt", "line two\n")?;
    fixture.commit_all("second")?;

    let outcome =
        rename_branch_if_current_unpublished(fixture.root(), "feature-old", "feature-new")?;

    assert_eq!(
        outcome,
        RenameBranchIfSafeOutcome::Skipped(RenameBranchSkipReason::CurrentBranchPublished)
    );
    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_name, "feature-old");
    assert!(!snapshot.branch_has_upstream);
    Ok(())
}

#[test]
fn rename_branch_if_current_unpublished_skips_existing_target() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature-old")?;
    fixture.checkout_branch("feature-existing")?;
    fixture.checkout_branch("feature-old")?;

    let outcome =
        rename_branch_if_current_unpublished(fixture.root(), "feature-old", "feature-existing")?;

    assert_eq!(
        outcome,
        RenameBranchIfSafeOutcome::Skipped(RenameBranchSkipReason::TargetAlreadyExists)
    );
    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_name, "feature-old");
    Ok(())
}

#[test]
fn review_url_for_github_remote_uses_compare_link() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/review-url")?;
    fixture.add_remote("origin", "https://github.com/example-org/hunk.git")?;

    let review_url = review_url_for_branch(fixture.root(), "feature/review-url")?
        .expect("github remote should produce a review URL");

    assert_eq!(
        review_url,
        "https://github.com/example-org/hunk/compare/feature%2Freview-url?expand=1"
    );
    Ok(())
}

#[test]
fn review_url_for_provider_mapping_uses_self_hosted_gitlab() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/self-hosted")?;
    fixture.add_remote(
        "origin",
        "https://git.company.internal/example-org/hunk.git",
    )?;

    let review_url = review_url_for_branch_with_provider_map(
        fixture.root(),
        "feature/self-hosted",
        &[ReviewProviderMapping {
            host: "git.company.internal".to_string(),
            provider: ReviewProviderKind::GitLab,
        }],
    )?
    .expect("provider mapping should produce a review URL");

    assert_eq!(
        review_url,
        "https://git.company.internal/example-org/hunk/-/merge_requests/new?merge_request[source_branch]=feature%2Fself-hosted"
    );
    Ok(())
}

#[test]
fn review_remote_for_github_branch_returns_structured_metadata() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/remote-metadata")?;
    fixture.add_remote("origin", "https://github.com/example-org/hunk.git")?;

    let remote =
        review_remote_for_branch_with_provider_map(fixture.root(), "feature/remote-metadata", &[])?
            .expect("github remote should resolve structured review metadata");

    assert_eq!(remote.provider, ReviewProviderKind::GitHub);
    assert_eq!(remote.host, "github.com");
    assert_eq!(remote.authority, "github.com");
    assert_eq!(remote.repository_path, "example-org/hunk");
    assert_eq!(remote.base_url, "https://github.com/example-org/hunk");
    Ok(())
}

#[test]
fn review_remote_for_self_hosted_gitlab_preserves_namespace_and_port() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/grouped-path")?;
    fixture.add_remote(
        "origin",
        "ssh://git@git.company.internal:2222/platform/tools/hunk.git",
    )?;

    let remote = review_remote_for_branch_with_provider_map(
        fixture.root(),
        "feature/grouped-path",
        &[ReviewProviderMapping {
            host: "git.company.internal".to_string(),
            provider: ReviewProviderKind::GitLab,
        }],
    )?
    .expect("gitlab remote should resolve structured review metadata");

    assert_eq!(remote.provider, ReviewProviderKind::GitLab);
    assert_eq!(remote.host, "git.company.internal");
    assert_eq!(remote.authority, "git.company.internal:2222");
    assert_eq!(remote.repository_path, "platform/tools/hunk");
    assert_eq!(
        remote.base_url,
        "https://git.company.internal:2222/platform/tools/hunk"
    );
    Ok(())
}

#[test]
fn review_remote_for_named_remote_resolves_upstream_github_repo() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/fork-pr")?;
    fixture.add_remote("origin", "https://github.com/example-user/hunk.git")?;
    fixture.add_remote("upstream", "https://github.com/example-org/hunk.git")?;

    let remote = review_remote_for_named_remote_with_provider_map(fixture.root(), "upstream", &[])?
        .expect("named upstream remote should resolve review metadata");

    assert_eq!(remote.provider, ReviewProviderKind::GitHub);
    assert_eq!(remote.repository_path, "example-org/hunk");
    assert_eq!(remote.base_url, "https://github.com/example-org/hunk");
    Ok(())
}

#[test]
fn review_url_preserves_non_default_port_for_self_hosted_remote() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/port")?;
    fixture.add_remote(
        "origin",
        "https://git.company.internal:8443/example-org/hunk.git",
    )?;

    let review_url = review_url_for_branch_with_provider_map(
        fixture.root(),
        "feature/port",
        &[ReviewProviderMapping {
            host: "git.company.internal".to_string(),
            provider: ReviewProviderKind::GitLab,
        }],
    )?
    .expect("provider mapping should produce a review URL");

    assert_eq!(
        review_url,
        "https://git.company.internal:8443/example-org/hunk/-/merge_requests/new?merge_request[source_branch]=feature%2Fport"
    );
    Ok(())
}

#[test]
fn review_url_prefers_push_url_over_fetch_url() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/pushurl")?;
    fixture.add_remote("origin", "https://github.com/example-org/hunk.git")?;
    fixture.set_push_url(
        "origin",
        "https://git.company.internal/example-org/hunk.git",
    )?;

    let review_url = review_url_for_branch_with_provider_map(
        fixture.root(),
        "feature/pushurl",
        &[ReviewProviderMapping {
            host: "git.company.internal".to_string(),
            provider: ReviewProviderKind::GitLab,
        }],
    )?
    .expect("pushurl should be used when generating a review URL");

    assert_eq!(
        review_url,
        "https://git.company.internal/example-org/hunk/-/merge_requests/new?merge_request[source_branch]=feature%2Fpushurl"
    );
    Ok(())
}

#[test]
fn review_url_preserves_plain_http_remotes() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/http")?;
    fixture.add_remote("origin", "http://git.company.internal/example-org/hunk.git")?;

    let review_url = review_url_for_branch_with_provider_map(
        fixture.root(),
        "feature/http",
        &[ReviewProviderMapping {
            host: "git.company.internal".to_string(),
            provider: ReviewProviderKind::GitLab,
        }],
    )?
    .expect("plain-http remote should produce a review URL");

    assert_eq!(
        review_url,
        "http://git.company.internal/example-org/hunk/-/merge_requests/new?merge_request[source_branch]=feature%2Fhttp"
    );
    Ok(())
}

#[test]
fn review_url_normalizes_ssh_and_strips_credentials() -> Result<()> {
    let ssh_fixture = TempGitRepo::new()?;
    ssh_fixture.write_file("tracked.txt", "line one\n")?;
    ssh_fixture.commit_all("initial")?;
    ssh_fixture.checkout_branch("feature/ssh")?;
    ssh_fixture.add_remote("origin", "ssh://git@github.com/example-org/hunk.git")?;

    let ssh_url = review_url_for_branch(ssh_fixture.root(), "feature/ssh")?
        .expect("ssh remote should produce a review URL");
    assert_eq!(
        ssh_url,
        "https://github.com/example-org/hunk/compare/feature%2Fssh?expand=1"
    );

    let creds_fixture = TempGitRepo::new()?;
    creds_fixture.write_file("tracked.txt", "line one\n")?;
    creds_fixture.commit_all("initial")?;
    creds_fixture.checkout_branch("feature/creds")?;
    creds_fixture.add_remote(
        "origin",
        "https://user:secret-token@github.com/example-org/hunk.git",
    )?;

    let creds_url = review_url_for_branch(creds_fixture.root(), "feature/creds")?
        .expect("credentialed remote should produce a review URL");
    assert_eq!(
        creds_url,
        "https://github.com/example-org/hunk/compare/feature%2Fcreds?expand=1"
    );
    Ok(())
}

#[test]
fn review_url_for_path_remote_returns_none() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "line one\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/path")?;
    fixture.add_remote("origin", "../local-bare-remote")?;

    let review_url = review_url_for_branch(fixture.root(), "feature/path")?;
    assert!(review_url.is_none());
    Ok(())
}

struct TempGitRepo {
    tempdir: TempDir,
    root: PathBuf,
}

impl TempGitRepo {
    fn new() -> Result<Self> {
        let tempdir = tempfile::tempdir()?;
        let root = tempdir.path().join("repo");
        let repo = Repository::init(root.as_path())?;
        drop(repo);
        Ok(Self {
            root: fs::canonicalize(root)?,
            tempdir,
        })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn repository(&self) -> Result<Repository> {
        Ok(Repository::open(self.root.as_path())?)
    }

    fn write_file(&self, relative: &str, contents: &str) -> Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
        Ok(())
    }

    fn commit_all(&self, message: &str) -> Result<git2::Oid> {
        let repo = self.repository()?;
        let mut index = repo.index()?;
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let signature = test_signature()?;
        let parents = self.head_commits(&repo)?;
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

    fn checkout_branch(&self, name: &str) -> Result<()> {
        let repo = self.repository()?;
        let head_commit = repo.head()?.peel_to_commit()?;
        if repo.find_branch(name, BranchType::Local).is_err() {
            repo.branch(name, &head_commit, false)?;
        }
        repo.set_head(&format!("refs/heads/{name}"))?;
        let mut checkout = CheckoutBuilder::new();
        checkout.force();
        repo.checkout_head(Some(&mut checkout))?;
        Ok(())
    }

    fn add_remote(&self, name: &str, url: &str) -> Result<()> {
        let repo = self.repository()?;
        if repo.find_remote(name).is_err() {
            repo.remote(name, url)?;
        }
        Ok(())
    }

    fn set_push_url(&self, name: &str, url: &str) -> Result<()> {
        let repo = self.repository()?;
        repo.remote_set_pushurl(name, Some(url))?;
        Ok(())
    }

    fn create_bare_remote(&self, name: &str) -> Result<PathBuf> {
        let remote_root = self.tempdir.path().join(format!("{name}.git"));
        Repository::init_bare(remote_root.as_path())?;
        self.add_remote(name, remote_root.to_string_lossy().as_ref())?;
        Ok(remote_root)
    }

    fn push_current_branch(&self, remote_name: &str, branch_name: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut remote = repo.find_remote(remote_name)?;
        remote.push(
            &[format!("refs/heads/{branch_name}:refs/heads/{branch_name}")],
            None,
        )?;
        Ok(())
    }

    fn add_worktree(&self, registration_name: &str, branch_name: &str) -> Result<PathBuf> {
        let repo = self.repository()?;
        let head_commit = repo.head()?.peel_to_commit()?;
        if repo.find_branch(branch_name, BranchType::Local).is_err() {
            repo.branch(branch_name, &head_commit, false)?;
        }
        let branch = repo.find_branch(branch_name, BranchType::Local)?;
        let path = self.tempdir.path().join(registration_name);
        let mut options = WorktreeAddOptions::new();
        options.reference(Some(branch.get()));
        repo.worktree(registration_name, path.as_path(), Some(&options))?;
        Ok(fs::canonicalize(path)?)
    }

    fn set_upstream(&self, branch_name: &str, upstream: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut branch = repo.find_branch(branch_name, BranchType::Local)?;
        branch.set_upstream(Some(upstream))?;
        Ok(())
    }

    fn set_remote_tracking_ref(&self, remote_name: &str, branch_name: &str) -> Result<()> {
        let repo = self.repository()?;
        let branch = repo.find_branch(branch_name, BranchType::Local)?;
        let target = branch
            .get()
            .target()
            .ok_or_else(|| anyhow::anyhow!("branch '{branch_name}' has no target"))?;
        repo.reference(
            format!("refs/remotes/{remote_name}/{branch_name}").as_str(),
            target,
            true,
            "test remote tracking ref",
        )?;
        Ok(())
    }

    fn head_commits<'repo>(&self, repo: &'repo Repository) -> Result<Vec<git2::Commit<'repo>>> {
        let head = match repo.head() {
            Ok(head) => head,
            Err(_) => return Ok(Vec::new()),
        };
        let Some(target) = head.target() else {
            return Ok(Vec::new());
        };
        Ok(vec![repo.find_commit(target)?])
    }
}

fn test_signature() -> Result<Signature<'static>> {
    Ok(Signature::now("Hunk", "hunk@example.com")?)
}
