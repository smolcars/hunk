use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use git2::{BranchType, IndexAddOption, Repository, Signature, build::CheckoutBuilder};
use hunk_git::git::load_workflow_snapshot;
use hunk_git::network::{push_current_branch, sync_current_branch};
use tempfile::TempDir;

#[test]
fn publish_branch_sets_upstream_and_clears_ahead_count() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    fixture.create_bare_remote("origin")?;
    fixture.checkout_branch("feature/publish")?;

    push_current_branch(fixture.root(), "feature/publish", false)?;

    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_name, "feature/publish");
    assert!(snapshot.branch_has_upstream);
    assert_eq!(snapshot.branch_ahead_count, 0);
    assert_eq!(snapshot.branch_behind_count, 0);
    Ok(())
}

#[test]
fn push_branch_updates_tracking_ref_after_new_commit() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    fixture.create_bare_remote("origin")?;
    fixture.checkout_branch("feature/push")?;
    push_current_branch(fixture.root(), "feature/push", false)?;

    fixture.write_file("tracked.txt", "base\nnext\n")?;
    fixture.commit_all("next")?;

    let before_push = load_workflow_snapshot(fixture.root())?;
    assert_eq!(before_push.branch_ahead_count, 1);

    push_current_branch(fixture.root(), "feature/push", true)?;

    let after_push = load_workflow_snapshot(fixture.root())?;
    assert!(after_push.branch_has_upstream);
    assert_eq!(after_push.branch_ahead_count, 0);
    assert_eq!(after_push.branch_behind_count, 0);
    Ok(())
}

#[test]
fn sync_branch_fast_forwards_checked_out_branch() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let remote_root = fixture.create_bare_remote("origin")?;
    fixture.checkout_branch("feature/sync")?;
    push_current_branch(fixture.root(), "feature/sync", false)?;

    let peer = TempGitRepo::clone_from(fixture.root(), "peer")?;
    peer.configure_signature()?;
    peer.set_remote_url("origin", remote_root.to_string_lossy().as_ref())?;
    peer.checkout_branch("feature/sync")?;
    peer.write_file("tracked.txt", "base\nremote\n")?;
    peer.commit_all("remote update")?;
    peer.push_to_remote("origin", "feature/sync", "feature/sync")?;

    sync_current_branch(fixture.root(), "feature/sync")?;

    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_ahead_count, 0);
    assert_eq!(snapshot.branch_behind_count, 0);
    assert_eq!(
        snapshot.last_commit_subject.as_deref(),
        Some("remote update")
    );
    assert_eq!(
        fs::read_to_string(fixture.root().join("tracked.txt"))?,
        "base\nremote\n"
    );
    Ok(())
}

#[test]
fn sync_branch_rejects_diverged_history() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let remote_root = fixture.create_bare_remote("origin")?;
    fixture.checkout_branch("feature/diverged")?;
    push_current_branch(fixture.root(), "feature/diverged", false)?;

    let peer = TempGitRepo::clone_from(fixture.root(), "peer")?;
    peer.configure_signature()?;
    peer.set_remote_url("origin", remote_root.to_string_lossy().as_ref())?;
    peer.checkout_branch("feature/diverged")?;
    peer.write_file("tracked.txt", "base\nremote\n")?;
    peer.commit_all("remote update")?;
    peer.push_to_remote("origin", "feature/diverged", "feature/diverged")?;

    fixture.write_file("tracked.txt", "base\nlocal\n")?;
    fixture.commit_all("local update")?;

    let err = sync_current_branch(fixture.root(), "feature/diverged")
        .expect_err("diverged history should require manual resolution");
    assert!(err.to_string().contains("fast-forward"));
    Ok(())
}

#[test]
fn publish_branch_rejects_ambiguous_remote_selection() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature/ambiguous")?;
    fixture.create_bare_remote("fork")?;
    fixture.create_bare_remote("upstream")?;

    let err = push_current_branch(fixture.root(), "feature/ambiguous", false)
        .expect_err("publish should require an explicit remote when multiple remotes exist");
    assert!(err.to_string().contains("multiple Git remotes"));
    Ok(())
}

#[test]
fn sync_branch_rejects_hidden_index_changes() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    fixture.create_bare_remote("origin")?;
    fixture.checkout_branch("feature/index")?;
    push_current_branch(fixture.root(), "feature/index", false)?;
    fixture.write_file("tracked.txt", "staged\n")?;
    fixture.stage_path("tracked.txt")?;
    fixture.write_file("tracked.txt", "base\n")?;

    let err = sync_current_branch(fixture.root(), "feature/index")
        .expect_err("hidden index changes should block sync");
    assert!(err.to_string().contains("unstage"));
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
            tempdir,
            root: fs::canonicalize(root)?,
        })
    }

    fn clone_from(source: &Path, name: &str) -> Result<Self> {
        let tempdir = tempfile::tempdir()?;
        let root = tempdir.path().join(name);
        let repo = Repository::clone(source.to_string_lossy().as_ref(), root.as_path())?;
        drop(repo);
        Ok(Self {
            tempdir,
            root: fs::canonicalize(root)?,
        })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn repository(&self) -> Result<Repository> {
        Ok(Repository::open(self.root.as_path())?)
    }

    fn configure_signature(&self) -> Result<()> {
        let repo = self.repository()?;
        let mut config = repo.config()?;
        config.set_str("user.name", "Hunk")?;
        config.set_str("user.email", "hunk@example.com")?;
        Ok(())
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

    fn create_bare_remote(&self, name: &str) -> Result<PathBuf> {
        let remote_root = self.tempdir.path().join(format!("{name}.git"));
        Repository::init_bare(remote_root.as_path())?;
        let repo = self.repository()?;
        if repo.find_remote(name).is_err() {
            repo.remote(name, remote_root.to_string_lossy().as_ref())?;
        }
        Ok(remote_root)
    }

    fn stage_path(&self, relative: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut index = repo.index()?;
        index.add_path(Path::new(relative))?;
        index.write()?;
        Ok(())
    }

    fn set_remote_url(&self, name: &str, url: &str) -> Result<()> {
        let repo = self.repository()?;
        repo.remote_set_url(name, url)?;
        Ok(())
    }

    fn push_to_remote(
        &self,
        remote_name: &str,
        local_branch_name: &str,
        remote_branch_name: &str,
    ) -> Result<()> {
        let repo = self.repository()?;
        let mut remote = repo.find_remote(remote_name)?;
        remote.push(
            &[format!(
                "refs/heads/{local_branch_name}:refs/heads/{remote_branch_name}"
            )],
            None,
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
