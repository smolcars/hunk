use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context as _, Result};
use git2::{
    BranchType, IndexAddOption, Repository, RepositoryInitOptions, Signature,
    build::CheckoutBuilder,
};
use hunk_domain::paths::{HUNK_HOME_DIR_ENV_VAR, hunk_home_dir};
use hunk_git::compare::{CompareSource, load_compare_file_document, load_compare_snapshot};
use hunk_git::git::load_workflow_snapshot;
use hunk_git::worktree::{
    CreateWorktreeRequest, PRIMARY_WORKSPACE_TARGET_ID, WorkspaceTargetKind,
    create_managed_worktree, list_workspace_targets, managed_worktree_path, managed_worktrees_root,
    path_is_within_managed_worktrees, remove_managed_worktree,
    repo_relative_path_is_within_managed_worktrees, validate_managed_worktree_removal,
};
use tempfile::TempDir;

#[test]
fn managed_worktree_helpers_keep_paths_under_global_hunkdiff_root() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    let managed_root = managed_worktrees_root(fixture.root())?;
    let managed_path = managed_worktree_path(fixture.root(), "worktree-1")?;
    fs::create_dir_all(managed_path.join("src"))?;
    fs::write(managed_path.join("src/lib.rs"), "fn main() {}\n")?;

    let _ = test_hunk_home_dir();
    assert_eq!(
        managed_root.parent(),
        Some(hunk_home_dir()?.join("worktrees").as_path())
    );
    assert_eq!(managed_path, managed_root.join("worktree-1"));
    assert!(path_is_within_managed_worktrees(
        fixture.root(),
        managed_path.join("src/lib.rs").as_path(),
    )?);
    assert!(!path_is_within_managed_worktrees(
        fixture.root(),
        fixture.root().join("src/lib.rs").as_path(),
    )?);
    assert!(!repo_relative_path_is_within_managed_worktrees(
        ".hunkdiff/worktrees/worktree-1/src/lib.rs",
    ));
    assert!(!repo_relative_path_is_within_managed_worktrees(
        "src/lib.rs"
    ));
    Ok(())
}

#[test]
fn listing_workspace_targets_includes_primary_checkout_and_created_worktree() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;

    let created = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/worktree-one".to_string(),
            base_branch_name: None,
        },
    )?;

    let targets = list_workspace_targets(fixture.root())?;
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].kind, WorkspaceTargetKind::PrimaryCheckout);
    assert_eq!(targets[0].id, PRIMARY_WORKSPACE_TARGET_ID);
    assert_eq!(targets[0].root, fixture.root());
    assert_eq!(targets[0].branch_name, "main");
    assert!(targets[0].is_active);

    let created_target = targets
        .iter()
        .find(|target| target.id == created.id)
        .context("created worktree target should be listed")?;
    assert_eq!(created_target.kind, WorkspaceTargetKind::LinkedWorktree);
    assert_eq!(created_target.root, created.root);
    assert_eq!(created_target.name, "worktree-1");
    assert_eq!(created_target.display_name, "feature/worktree-one");
    assert_eq!(created_target.branch_name, "feature/worktree-one");
    assert!(created_target.managed);
    assert!(!created_target.is_active);
    Ok(())
}

#[test]
fn workflow_snapshot_marks_branches_with_their_checked_out_workspace_target() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;

    let created = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/worktree-one".to_string(),
            base_branch_name: None,
        },
    )?;

    let primary_snapshot = load_workflow_snapshot(fixture.root())?;
    let main_branch = primary_snapshot
        .branches
        .iter()
        .find(|branch| branch.name == "main")
        .context("main branch should exist in primary snapshot")?;
    assert_eq!(
        main_branch.attached_workspace_target_id.as_deref(),
        Some(PRIMARY_WORKSPACE_TARGET_ID)
    );
    assert_eq!(
        main_branch.attached_workspace_target_root.as_deref(),
        Some(fixture.root())
    );
    assert_eq!(
        main_branch.attached_workspace_target_label.as_deref(),
        Some("Primary Checkout")
    );

    let feature_branch = primary_snapshot
        .branches
        .iter()
        .find(|branch| branch.name == "feature/worktree-one")
        .context("worktree branch should exist in primary snapshot")?;
    assert_eq!(
        feature_branch.attached_workspace_target_id.as_deref(),
        Some(created.id.as_str())
    );
    assert_eq!(
        feature_branch.attached_workspace_target_root.as_deref(),
        Some(created.root.as_path())
    );
    assert_eq!(
        feature_branch.attached_workspace_target_label.as_deref(),
        Some(created.name.as_str())
    );

    let worktree_snapshot = load_workflow_snapshot(created.root.as_path())?;
    let current_feature_branch = worktree_snapshot
        .branches
        .iter()
        .find(|branch| branch.name == "feature/worktree-one")
        .context("worktree branch should exist in worktree snapshot")?;
    assert!(current_feature_branch.is_current);
    assert_eq!(
        current_feature_branch
            .attached_workspace_target_id
            .as_deref(),
        Some(created.id.as_str())
    );
    Ok(())
}

#[test]
fn creating_managed_worktree_rejects_existing_branch_name() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    fixture.create_branch("feature/existing")?;

    let err = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/existing".to_string(),
            base_branch_name: None,
        },
    )
    .expect_err("existing branch should block worktree creation");

    assert!(err.to_string().contains("already exists"));
    Ok(())
}

#[test]
fn creating_managed_worktree_auto_increments_generated_names() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;

    let first = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/one".to_string(),
            base_branch_name: None,
        },
    )?;
    let second = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/two".to_string(),
            base_branch_name: None,
        },
    )?;

    assert_eq!(first.name, "worktree-1");
    assert_eq!(second.name, "worktree-2");
    Ok(())
}

#[test]
fn creating_managed_worktree_can_start_from_explicit_base_branch() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    fixture.create_branch("feature/base")?;
    fixture.create_branch("feature/active")?;
    fixture.checkout_branch("feature/active")?;
    fixture.write_file("tracked.txt", "base\nactive\n")?;
    fixture.commit_all("active change")?;

    let created = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/worktree-from-base".to_string(),
            base_branch_name: Some("feature/base".to_string()),
        },
    )?;

    let repo = fixture.repository()?;
    let base_commit = repo
        .find_branch("feature/base", BranchType::Local)?
        .into_reference()
        .peel_to_commit()?
        .id();
    let created_commit = repo
        .find_branch("feature/worktree-from-base", BranchType::Local)?
        .into_reference()
        .peel_to_commit()?
        .id();
    assert_eq!(created_commit, base_commit);
    assert_eq!(
        fs::read_to_string(created.root.join("tracked.txt"))?,
        "base\n"
    );
    Ok(())
}

#[test]
fn removing_managed_worktree_deletes_checkout_and_unregisters_target() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/remove".to_string(),
            base_branch_name: None,
        },
    )?;

    remove_managed_worktree(worktree.root.as_path())?;

    assert!(!worktree.root.exists());
    let targets = list_workspace_targets(fixture.root())?;
    assert!(
        targets.iter().all(|target| target.root != worktree.root),
        "removed worktree should no longer be listed"
    );
    Ok(())
}

#[test]
fn removing_managed_worktree_rejects_dirty_checkouts() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/dirty-remove".to_string(),
            base_branch_name: None,
        },
    )?;
    fs::write(worktree.root.join("tracked.txt"), "base\ndirty\n")?;

    let err = remove_managed_worktree(worktree.root.as_path())
        .expect_err("dirty worktree removal should fail");

    assert!(err.to_string().contains("uncommitted changes"));
    assert!(worktree.root.exists());
    Ok(())
}

#[test]
fn validating_managed_worktree_removal_rejects_dirty_checkouts() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/dirty-validate".to_string(),
            base_branch_name: None,
        },
    )?;
    fs::write(worktree.root.join("tracked.txt"), "base\ndirty\n")?;

    let err = validate_managed_worktree_removal(worktree.root.as_path())
        .expect_err("dirty worktree validation should fail");

    assert!(err.to_string().contains("uncommitted changes"));
    assert!(worktree.root.exists());
    Ok(())
}

#[test]
fn removing_managed_worktree_rejects_primary_checkout() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;

    let err = remove_managed_worktree(fixture.root())
        .expect_err("primary checkout should not be removable as a worktree");

    assert!(err.to_string().contains("primary checkout"));
    Ok(())
}

#[test]
fn removing_managed_worktree_rejects_external_linked_worktrees() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let external_parent = tempfile::tempdir()?;
    let external_root = external_parent.path().join("external-worktree");
    fixture
        .repository()?
        .worktree("external", external_root.as_path(), None)?;

    let err = remove_managed_worktree(external_root.as_path())
        .expect_err("external linked worktree should not be removable as managed");

    assert!(err.to_string().contains("Hunk-managed"));
    Ok(())
}

#[test]
fn compare_snapshot_supports_branch_to_worktree_diffs() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/compare".to_string(),
            base_branch_name: None,
        },
    )?;
    fs::write(worktree.root.join("tracked.txt"), "base\nworktree change\n")?;

    let snapshot = load_compare_snapshot(
        fixture.root(),
        &CompareSource::Branch {
            name: "main".to_string(),
        },
        &CompareSource::WorkspaceTarget {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
    )?;

    println!("{snapshot:#?}");
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, "tracked.txt");
    assert!(snapshot.overall_line_stats.added >= 1, "{snapshot:#?}");
    assert!(
        snapshot
            .patches_by_path
            .get("tracked.txt")
            .is_some_and(|patch| patch.contains("worktree change"))
    );
    Ok(())
}

#[test]
fn compare_snapshot_supports_worktree_to_branch_diffs() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/compare-reverse".to_string(),
            base_branch_name: None,
        },
    )?;
    fs::write(worktree.root.join("tracked.txt"), "base\nworktree change\n")?;

    let snapshot = load_compare_snapshot(
        fixture.root(),
        &CompareSource::WorkspaceTarget {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
        &CompareSource::Branch {
            name: "main".to_string(),
        },
    )?;

    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, "tracked.txt");
    assert!(snapshot.overall_line_stats.removed >= 1);
    assert!(
        snapshot
            .patches_by_path
            .get("tracked.txt")
            .is_some_and(|patch| patch.contains("@@") && patch.contains("worktree change"))
    );
    Ok(())
}

#[test]
fn compare_snapshot_supports_workspace_head_to_worktree_diffs() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/workspace-head".to_string(),
            base_branch_name: None,
        },
    )?;

    write_file(
        worktree.root.as_path(),
        "tracked.txt",
        "base\ncommitted branch change\n",
    )?;
    commit_all_in_repo(worktree.root.as_path(), "branch commit")?;
    write_file(
        worktree.root.as_path(),
        "tracked.txt",
        "base\ncommitted branch change\nunstaged worktree change\n",
    )?;

    let head_snapshot = load_compare_snapshot(
        fixture.root(),
        &CompareSource::WorkspaceTargetHead {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
        &CompareSource::WorkspaceTarget {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
    )?;
    let branch_snapshot = load_compare_snapshot(
        fixture.root(),
        &CompareSource::Branch {
            name: "main".to_string(),
        },
        &CompareSource::WorkspaceTarget {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
    )?;

    let head_patch = head_snapshot
        .patches_by_path
        .get("tracked.txt")
        .context("head compare patch should exist")?;
    let branch_patch = branch_snapshot
        .patches_by_path
        .get("tracked.txt")
        .context("branch compare patch should exist")?;

    assert!(head_patch.contains("unstaged worktree change"));
    assert!(!head_patch.contains("+committed branch change"));
    assert_eq!(head_snapshot.overall_line_stats.added, 1);
    assert!(branch_patch.contains("committed branch change"));
    assert!(branch_patch.contains("unstaged worktree change"));
    assert!(branch_snapshot.overall_line_stats.added > head_snapshot.overall_line_stats.added);
    Ok(())
}

#[test]
fn compare_snapshot_supports_branch_to_worktree_new_files() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/compare-new-file".to_string(),
            base_branch_name: None,
        },
    )?;
    fs::write(worktree.root.join("test.md"), "hello\nnew file\n")?;

    let snapshot = load_compare_snapshot(
        fixture.root(),
        &CompareSource::Branch {
            name: "main".to_string(),
        },
        &CompareSource::WorkspaceTarget {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
    )?;

    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, "test.md");
    assert!(snapshot.overall_line_stats.added >= 1);
    assert!(
        snapshot
            .patches_by_path
            .get("test.md")
            .is_some_and(|patch| patch.contains("@@") && patch.contains("new file"))
    );
    Ok(())
}

fn write_file(root: &Path, relative_path: &str, contents: &str) -> Result<()> {
    let full_path = root.join(relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(full_path, contents)?;
    Ok(())
}

fn commit_all_in_repo(root: &Path, message: &str) -> Result<()> {
    let repo = Repository::open(root)?;
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let signature = Signature::now("Hunk Test", "hunk@example.com")?;

    let parent = match repo.head() {
        Ok(head) => Some(head.peel_to_commit()?),
        Err(err)
            if err.code() == git2::ErrorCode::UnbornBranch
                || err.code() == git2::ErrorCode::NotFound =>
        {
            None
        }
        Err(err) => return Err(err).context("failed to resolve HEAD while committing"),
    };

    match parent.as_ref() {
        Some(parent) => {
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[parent],
            )?;
        }
        None => {
            repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])?;
        }
    }

    Ok(())
}

#[test]
fn compare_snapshot_marks_binary_branch_to_worktree_diffs() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_binary_file("asset.bin", b"\0base")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/binary".to_string(),
            base_branch_name: None,
        },
    )?;
    fs::write(worktree.root.join("asset.bin"), b"\0base\0changed")?;

    let snapshot = load_compare_snapshot(
        fixture.root(),
        &CompareSource::Branch {
            name: "main".to_string(),
        },
        &CompareSource::WorkspaceTarget {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
    )?;

    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, "asset.bin");
    assert_eq!(snapshot.overall_line_stats.added, 0);
    assert_eq!(snapshot.overall_line_stats.removed, 0);
    assert!(
        snapshot
            .patches_by_path
            .get("asset.bin")
            .is_some_and(|patch| patch.contains("Binary files"))
    );
    Ok(())
}

#[test]
fn compare_file_document_loads_workspace_head_and_worktree_text() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/lib.rs", "fn value() -> i32 {\n    1\n}\n")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/review-editor".to_string(),
            base_branch_name: None,
        },
    )?;
    fs::write(
        worktree.root.join("src/lib.rs"),
        "fn value() -> i32 {\n    2\n}\n",
    )?;

    let document = load_compare_file_document(
        fixture.root(),
        &CompareSource::WorkspaceTargetHead {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
        &CompareSource::WorkspaceTarget {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
        "src/lib.rs",
    )?;

    assert!(document.left_present);
    assert!(document.right_present);
    assert!(document.left_text.contains("1"));
    assert!(document.right_text.contains("2"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn compare_snapshot_keeps_mode_only_worktree_diffs() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("script.sh", "#!/bin/sh\necho hi\n")?;
    fixture.commit_all("initial")?;
    let worktree = create_managed_worktree(
        fixture.root(),
        &CreateWorktreeRequest {
            branch_name: "feature/mode".to_string(),
            base_branch_name: None,
        },
    )?;
    fixture.make_executable(worktree.root.join("script.sh").as_path())?;

    let snapshot = load_compare_snapshot(
        fixture.root(),
        &CompareSource::Branch {
            name: "main".to_string(),
        },
        &CompareSource::WorkspaceTarget {
            target_id: worktree.id.clone(),
            root: worktree.root.clone(),
        },
    )?;

    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, "script.sh");
    assert_eq!(snapshot.overall_line_stats.added, 0);
    assert_eq!(snapshot.overall_line_stats.removed, 0);
    assert!(
        snapshot
            .patches_by_path
            .get("script.sh")
            .is_some_and(|patch| patch.contains("old mode 100644\nnew mode 100755"))
    );
    Ok(())
}

struct TempGitRepo {
    _tempdir: TempDir,
    root: PathBuf,
}

impl TempGitRepo {
    fn new() -> Result<Self> {
        let _ = test_hunk_home_dir();
        let tempdir = tempfile::tempdir()?;
        let root = tempdir.path().join("repo");
        let mut options = RepositoryInitOptions::new();
        options.initial_head("main");
        Repository::init_opts(root.as_path(), &options)?;
        let root = fs::canonicalize(root.as_path())?;

        Ok(Self {
            _tempdir: tempdir,
            root,
        })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn repository(&self) -> Result<Repository> {
        Ok(Repository::open(self.root())?)
    }

    fn write_file(&self, relative_path: &str, contents: &str) -> Result<()> {
        let full_path = self.root.join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(full_path, contents)?;
        Ok(())
    }

    fn write_binary_file(&self, relative_path: &str, contents: &[u8]) -> Result<()> {
        let full_path = self.root.join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(full_path, contents)?;
        Ok(())
    }

    fn commit_all(&self, message: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut index = repo.index()?;
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let signature = Signature::now("Hunk Test", "hunk@example.com")?;

        let parent = match repo.head() {
            Ok(head) => Some(head.peel_to_commit()?),
            Err(err)
                if err.code() == git2::ErrorCode::UnbornBranch
                    || err.code() == git2::ErrorCode::NotFound =>
            {
                None
            }
            Err(err) => return Err(err).context("failed to resolve HEAD while committing"),
        };

        match parent.as_ref() {
            Some(parent) => {
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[parent],
                )?;
            }
            None => {
                repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])?;
            }
        }

        Ok(())
    }

    fn create_branch(&self, branch_name: &str) -> Result<()> {
        let repo = self.repository()?;
        let head_commit = repo.head()?.peel_to_commit()?;
        repo.branch(branch_name, &head_commit, false)?;
        Ok(())
    }

    fn checkout_branch(&self, branch_name: &str) -> Result<()> {
        let repo = self.repository()?;
        repo.set_head(format!("refs/heads/{branch_name}").as_str())?;
        let mut checkout = CheckoutBuilder::new();
        checkout.force();
        repo.checkout_head(Some(&mut checkout))?;
        Ok(())
    }

    #[cfg(unix)]
    fn make_executable(&self, path: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
        Ok(())
    }
}

fn test_hunk_home_dir() -> &'static PathBuf {
    static TEST_HUNK_HOME_DIR: OnceLock<PathBuf> = OnceLock::new();

    TEST_HUNK_HOME_DIR.get_or_init(|| {
        let path = std::env::temp_dir()
            .join(format!("hunk-git-tests-{}", std::process::id()))
            .join(".hunkdiff");
        fs::create_dir_all(path.as_path()).expect("test hunk home dir should be created");
        set_test_hunk_home_dir(path.as_path());
        path
    })
}

#[allow(unused_unsafe)]
fn set_test_hunk_home_dir(path: &Path) {
    unsafe {
        std::env::set_var(HUNK_HOME_DIR_ENV_VAR, path.as_os_str());
    }
}
