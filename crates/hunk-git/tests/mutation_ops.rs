use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use git2::{BranchType, IndexAddOption, Repository, Signature, build::CheckoutBuilder};
use hunk_git::git::load_workflow_snapshot;
use hunk_git::mutation::{
    activate_or_create_branch, commit_all, commit_all_with_details, commit_selected_paths,
    commit_selected_paths_with_details, restore_working_copy_paths, working_copy_context_for_ai,
};
use tempfile::TempDir;

#[test]
fn activating_existing_branch_updates_worktree_contents() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    let default_branch = fixture.current_branch_name()?;

    fixture.checkout_branch("feature")?;
    fixture.write_file("tracked.txt", "feature\n")?;
    fixture.commit_all_git2("feature work")?;
    fixture.checkout_branch(default_branch.as_str())?;

    activate_or_create_branch(fixture.root(), "feature", false)?;

    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_name, "feature");
    assert_eq!(
        fs::read_to_string(fixture.root().join("tracked.txt"))?,
        "feature\n"
    );
    Ok(())
}

#[test]
fn creating_branch_from_head_updates_active_branch() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;

    activate_or_create_branch(fixture.root(), "feature/new-branch", false)?;

    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_name, "feature/new-branch");
    assert!(
        snapshot
            .branches
            .iter()
            .any(|branch| { branch.name == "feature/new-branch" && branch.is_current })
    );
    Ok(())
}

#[test]
fn activating_branch_rejects_dirty_worktree() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "dirty\n")?;

    let err = activate_or_create_branch(fixture.root(), "feature", false)
        .expect_err("dirty worktree should block branch switch");
    assert!(
        err.to_string()
            .contains("commit or discard working tree changes before switching branches")
    );
    Ok(())
}

#[test]
fn creating_new_branch_can_keep_dirty_changes_on_review_branch() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "base\npending review\n")?;

    activate_or_create_branch(fixture.root(), "ai/local/review-branch", true)?;

    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(snapshot.branch_name, "ai/local/review-branch");
    assert_eq!(
        fs::read_to_string(fixture.root().join("tracked.txt"))?,
        "base\npending review\n"
    );
    assert!(
        snapshot
            .branches
            .iter()
            .any(|branch| branch.name == "ai/local/review-branch" && branch.is_current)
    );
    Ok(())
}

#[test]
fn activating_branch_rejects_hidden_index_changes() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "staged\n")?;
    fixture.stage_path("tracked.txt")?;
    fixture.write_file("tracked.txt", "base\n")?;

    let err = activate_or_create_branch(fixture.root(), "feature", false)
        .expect_err("hidden index changes should block branch switch");
    assert!(err.to_string().contains("staged index changes"));
    Ok(())
}

#[test]
fn commit_all_records_all_worktree_changes() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "base\nupdated\n")?;
    fixture.write_file("notes.txt", "hello\n")?;

    commit_all(fixture.root(), "record all")?;

    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert!(snapshot.files.is_empty());
    assert_eq!(snapshot.last_commit_subject.as_deref(), Some("record all"));
    Ok(())
}

#[test]
fn commit_all_respects_repo_override_when_commit_signing_is_disabled() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.set_config_str("gpg.program", "does-not-exist-hunk-signer")?;
    fixture.set_config_bool("commit.gpgSign", false)?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "base\nupdated\n")?;

    commit_all(fixture.root(), "record all")?;

    assert_eq!(fixture.head_subject()?.as_deref(), Some("record all"));
    Ok(())
}

#[test]
fn commit_all_respects_commit_gpg_sign_config() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.set_config_str("gpg.program", "does-not-exist-hunk-signer")?;
    fixture.set_config_bool("commit.gpgSign", true)?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "base\nupdated\n")?;

    let err = commit_all(fixture.root(), "record all")
        .expect_err("commit signing should be delegated to git");

    assert!(err.to_string().contains("git commit failed"));
    assert_eq!(fixture.head_subject()?.as_deref(), Some("initial"));
    Ok(())
}

#[test]
fn commit_selected_paths_leaves_excluded_changes_dirty() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("src/lib.rs", "one\n")?;
    fixture.write_file("README.md", "hello\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("src/lib.rs", "one\ntwo\n")?;
    fixture.write_file("README.md", "hello\nworld\n")?;

    let committed =
        commit_selected_paths(fixture.root(), "partial", &[String::from("src/lib.rs")])?;

    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(committed, 1);
    assert_eq!(snapshot.last_commit_subject.as_deref(), Some("partial"));
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, "README.md");
    Ok(())
}

#[test]
fn commit_all_with_details_returns_created_commit_metadata() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "changed\n")?;

    let created = commit_all_with_details(fixture.root(), "record all")?;

    assert_eq!(created.subject, "record all");
    assert!(created.committed_unix_time.is_some());
    assert_eq!(created.commit_id.len(), 40);
    Ok(())
}

#[test]
fn commit_selected_paths_with_details_returns_count_and_commit_metadata() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("src/lib.rs", "one\n")?;
    fixture.write_file("README.md", "hello\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("src/lib.rs", "one\ntwo\n")?;
    fixture.write_file("README.md", "hello\nworld\n")?;

    let (count, created) = commit_selected_paths_with_details(
        fixture.root(),
        "partial",
        &[String::from("src/lib.rs")],
    )?;

    assert_eq!(count, 1);
    assert_eq!(created.subject, "partial");
    assert_eq!(created.commit_id.len(), 40);
    Ok(())
}

#[test]
fn commit_details_use_the_commit_subject_for_multiline_messages() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "changed\n")?;

    let created = commit_all_with_details(fixture.root(), "subject line\n\nbody line")?;

    assert_eq!(created.subject, "subject line");
    Ok(())
}

#[test]
fn working_copy_context_for_ai_returns_none_when_clean() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;

    let context = working_copy_context_for_ai(fixture.root(), 10, 10_000)?;

    assert!(context.is_none());
    Ok(())
}

#[test]
fn working_copy_context_for_ai_returns_summary_and_patch() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "base\nupdated\n")?;
    fixture.write_file("new.txt", "hello\n")?;

    let context = working_copy_context_for_ai(fixture.root(), 10, 10_000)?
        .expect("context should exist for dirty worktree");

    assert!(context.changed_files_summary.contains("tracked.txt"));
    assert!(context.changed_files_summary.contains("new.txt"));
    assert!(context.diff_patch.contains("diff --git"));
    assert!(context.diff_patch.contains("updated"));
    Ok(())
}

#[test]
fn working_copy_context_for_ai_rejects_hidden_index_changes() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "staged\n")?;
    fixture.stage_path("tracked.txt")?;
    fixture.write_file("tracked.txt", "base\n")?;

    let err = working_copy_context_for_ai(fixture.root(), 10, 10_000)
        .expect_err("hidden index changes should be rejected");

    assert!(err.to_string().contains("staged index changes"));
    Ok(())
}

#[test]
fn working_copy_context_for_ai_truncates_large_patch_output() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", &format!("base\n{}\n", "updated".repeat(100)))?;

    let context = working_copy_context_for_ai(fixture.root(), 10, 80)?
        .expect("context should exist for dirty worktree");

    assert!(context.diff_patch.contains("[truncated]"));
    Ok(())
}

#[test]
fn working_copy_context_for_ai_supports_unborn_head_repositories() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "pending\n")?;

    let context = working_copy_context_for_ai(fixture.root(), 10, 10_000)?
        .expect("context should exist for an unborn repo");

    assert!(context.changed_files_summary.contains("tracked.txt"));
    Ok(())
}

#[test]
fn commit_all_rejects_hidden_index_changes() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.configure_signature()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "staged\n")?;
    fixture.stage_path("tracked.txt")?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.write_file("visible.txt", "visible\n")?;

    let err = commit_all(fixture.root(), "record all")
        .expect_err("hidden index changes should block commit creation");
    assert!(err.to_string().contains("staged index changes"));
    Ok(())
}

#[test]
fn restore_working_copy_paths_restores_tracked_and_removes_untracked() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("tracked.txt", "changed\n")?;
    fixture.write_file("scratch.txt", "scratch\n")?;

    let restored = restore_working_copy_paths(
        fixture.root(),
        &[String::from("tracked.txt"), String::from("scratch.txt")],
    )?;

    let snapshot = load_workflow_snapshot(fixture.root())?;
    assert_eq!(restored, 2);
    assert_eq!(
        fs::read_to_string(fixture.root().join("tracked.txt"))?,
        "base\n"
    );
    assert!(!fixture.root().join("scratch.txt").exists());
    assert!(snapshot.files.is_empty());
    Ok(())
}

#[test]
fn restore_working_copy_paths_clears_staged_new_file_from_index() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all_git2("initial")?;
    fixture.write_file("scratch.txt", "scratch\n")?;
    fixture.stage_path("scratch.txt")?;

    let restored = restore_working_copy_paths(fixture.root(), &[String::from("scratch.txt")])?;

    let repo = fixture.repository()?;
    let statuses = repo.statuses(None)?;
    assert_eq!(restored, 1);
    assert!(!fixture.root().join("scratch.txt").exists());
    assert!(statuses.is_empty());
    Ok(())
}

#[test]
fn restore_working_copy_paths_rejects_paths_outside_repo_root() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    let outside_path = fixture._tempdir.path().join("outside.txt");
    fs::write(outside_path.as_path(), "outside\n")?;

    let err = restore_working_copy_paths(fixture.root(), &[String::from("../outside.txt")])
        .expect_err("restore should reject paths that escape the repository root");

    assert!(err.to_string().contains("escapes the repository root"));
    assert_eq!(fs::read_to_string(outside_path)?, "outside\n");
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
        drop(repo);
        Ok(Self {
            _tempdir: tempdir,
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

    fn set_config_bool(&self, key: &str, value: bool) -> Result<()> {
        let repo = self.repository()?;
        let mut config = repo.config()?;
        config.set_bool(key, value)?;
        Ok(())
    }

    fn set_config_str(&self, key: &str, value: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut config = repo.config()?;
        config.set_str(key, value)?;
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

    fn commit_all_git2(&self, message: &str) -> Result<git2::Oid> {
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

    fn current_branch_name(&self) -> Result<String> {
        let repo = self.repository()?;
        let head = repo.head()?;
        Ok(head.shorthand().unwrap_or("HEAD").to_string())
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

    fn stage_path(&self, relative: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut index = repo.index()?;
        index.add_path(Path::new(relative))?;
        index.write()?;
        Ok(())
    }

    fn head_subject(&self) -> Result<Option<String>> {
        let repo = self.repository()?;
        let head = match repo.head() {
            Ok(head) => head,
            Err(_) => return Ok(None),
        };
        let commit = head.peel_to_commit()?;
        Ok(commit.summary().map(ToOwned::to_owned))
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
