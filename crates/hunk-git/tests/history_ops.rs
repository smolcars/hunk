use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use git2::{BranchType, IndexAddOption, Repository, Signature, build::CheckoutBuilder};
use hunk_git::history::{
    DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT, load_recent_authored_commits_if_changed,
    load_recent_authored_commits_with_fingerprint,
};
use tempfile::TempDir;

#[test]
fn recent_authored_commits_only_include_the_checked_out_branch() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature("Hunk", "hunk@example.com")?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_at("initial", 1_700_000_000, "Hunk", "hunk@example.com")?;
    let default_branch = fixture.current_branch_name()?;
    fixture.write_file("tracked.txt", "main one\n")?;
    fixture.commit_all_at("main one", 1_700_000_010, "Hunk", "hunk@example.com")?;
    fixture.checkout_branch("feature")?;
    fixture.write_file("tracked.txt", "feature one\n")?;
    fixture.commit_all_at("feature one", 1_700_000_020, "Hunk", "hunk@example.com")?;
    fixture.write_file("tracked.txt", "feature two\n")?;
    fixture.commit_all_at("feature two", 1_700_000_030, "Hunk", "hunk@example.com")?;
    fixture.checkout_branch(default_branch.as_str())?;
    fixture.write_file("tracked.txt", "other author\n")?;
    fixture.commit_all_at("other author", 1_700_000_040, "Other", "other@example.com")?;
    fixture.checkout_branch("feature")?;

    let (_, snapshot) = load_recent_authored_commits_with_fingerprint(
        fixture.root(),
        DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT,
    )?;

    let subjects = snapshot
        .commits
        .iter()
        .map(|commit| commit.subject.as_str())
        .collect::<Vec<_>>();
    assert_eq!(subjects, vec!["feature two", "feature one"]);
    assert_eq!(snapshot.commits.len(), 2);
    Ok(())
}

#[test]
fn recent_authored_commits_if_changed_skips_when_branch_tips_match() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature("Hunk", "hunk@example.com")?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_at("initial", 1_700_000_000, "Hunk", "hunk@example.com")?;

    let (fingerprint, snapshot) = load_recent_authored_commits_with_fingerprint(
        fixture.root(),
        DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT,
    )?;
    assert_eq!(snapshot.commits.len(), 1);

    let (_, skipped) = load_recent_authored_commits_if_changed(
        fixture.root(),
        DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT,
        Some(&fingerprint),
    )?;
    assert!(skipped.is_none());
    Ok(())
}

#[test]
fn recent_authored_commits_include_all_authors_on_the_checked_out_branch() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature("Configured", "configured@example.com")?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_at("initial", 1_700_000_000, "Hunk", "hunk@example.com")?;
    fixture.write_file("tracked.txt", "second\n")?;
    fixture.commit_all_at("second", 1_700_000_010, "Other", "other@example.com")?;

    let (_, snapshot) = load_recent_authored_commits_with_fingerprint(
        fixture.root(),
        DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT,
    )?;

    let subjects = snapshot
        .commits
        .iter()
        .map(|commit| commit.subject.as_str())
        .collect::<Vec<_>>();
    assert_eq!(subjects, vec!["second", "initial"]);
    Ok(())
}

#[test]
fn recent_authored_commits_if_changed_refreshes_when_head_ref_changes() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature("Hunk", "hunk@example.com")?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_at("initial", 1_700_000_000, "Hunk", "hunk@example.com")?;

    let (main_fingerprint, main_snapshot) = load_recent_authored_commits_with_fingerprint(
        fixture.root(),
        DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT,
    )?;
    assert_eq!(main_snapshot.commits.len(), 1);

    fixture.checkout_branch("feature")?;

    let (_, refreshed_snapshot) = load_recent_authored_commits_if_changed(
        fixture.root(),
        DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT,
        Some(&main_fingerprint),
    )?;

    assert!(refreshed_snapshot.is_some());
    Ok(())
}

struct TempGitRepo {
    _tempdir: TempDir,
    root: PathBuf,
}

impl TempGitRepo {
    fn new() -> Result<Self> {
        let tempdir = tempfile::tempdir()?;
        let root = tempdir.path().join("repo");
        let repo = Repository::init(root.as_path())?;
        let mut config = repo.config()?;
        config.set_str("init.defaultBranch", "main")?;
        drop(config);
        drop(repo);
        Ok(Self {
            _tempdir: tempdir,
            root: fs::canonicalize(root)?,
        })
    }

    fn root(&self) -> &Path {
        self.root.as_path()
    }

    fn repository(&self) -> Result<Repository> {
        Ok(Repository::open(self.root.as_path())?)
    }

    fn configure_signature(&self, name: &str, email: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut config = repo.config()?;
        config.set_str("user.name", name)?;
        config.set_str("user.email", email)?;
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

    fn commit_all_at(
        &self,
        message: &str,
        seconds: i64,
        name: &str,
        email: &str,
    ) -> Result<git2::Oid> {
        let repo = self.repository()?;
        let mut index = repo.index()?;
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let signature = Signature::new(name, email, &git2::Time::new(seconds, 0))?;
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

    fn current_branch_name(&self) -> Result<String> {
        let repo = self.repository()?;
        let head = repo.head()?;
        Ok(head.shorthand().unwrap_or("HEAD").to_string())
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
