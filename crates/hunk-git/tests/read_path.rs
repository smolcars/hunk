use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use git2::{
    BranchType, IndexAddOption, Repository, Signature,
    build::{CheckoutBuilder, RepoBuilder},
};
use hunk_git::git::{
    FileStatus, LineStats, RepoTreeEntryKind, count_non_ignored_repo_tree_entries, load_patch,
    load_patches_for_files_from_session, load_repo_file_line_stats_without_refresh,
    load_repo_line_stats, load_repo_line_stats_without_refresh, load_repo_tree, load_snapshot,
    load_snapshot_without_refresh, load_visible_repo_file_paths, load_workflow_snapshot,
    load_workflow_snapshot_if_changed_without_refresh, load_workflow_snapshot_with_fingerprint,
    load_workflow_snapshot_without_refresh, open_patch_session,
};
use tempfile::TempDir;

#[test]
fn workflow_snapshot_reports_branch_files_line_stats_and_upstream_state() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/lib.rs", "one\ntwo\n")?;
    fixture.commit_all("initial")?;
    fixture.checkout_branch("feature")?;
    fixture.create_bare_remote("origin")?;
    fixture.push_current_branch("origin", "feature")?;
    fixture.set_upstream("feature", "origin/feature")?;
    fixture.write_file("committed.txt", "base\n")?;
    fixture.commit_all("ahead commit")?;

    fixture.write_file("src/lib.rs", "one\nthree\nfour\n")?;
    fixture.write_file("notes.txt", "alpha\n")?;

    let (fingerprint, workflow) = load_workflow_snapshot_with_fingerprint(fixture.root())?;
    let file_stats = load_repo_file_line_stats_without_refresh(fixture.root())?;
    let overall_stats = load_repo_line_stats(fixture.root())?;

    assert_eq!(workflow.branch_name, "feature");
    assert!(workflow.branch_has_upstream);
    assert_eq!(workflow.branch_ahead_count, 1);
    assert_eq!(workflow.branch_behind_count, 0);
    assert_eq!(
        workflow.last_commit_subject.as_deref(),
        Some("ahead commit")
    );
    assert_eq!(
        workflow.branches.first().map(|branch| branch.name.as_str()),
        Some("feature")
    );
    assert_eq!(
        workflow.branches.first().map(|branch| branch.is_current),
        Some(true)
    );
    assert!(
        fingerprint
            .head_ref_name()
            .is_some_and(|name| name.ends_with("/feature"))
    );
    assert!(fingerprint.head_commit_id().is_some());

    assert_eq!(workflow.files.len(), 2);
    assert!(workflow.files.iter().any(|file| {
        file.path == "notes.txt" && file.status == FileStatus::Untracked && !file.staged
    }));
    assert!(workflow.files.iter().any(|file| {
        file.path == "src/lib.rs" && file.status == FileStatus::Modified && !file.untracked
    }));

    assert_eq!(
        file_stats.get("src/lib.rs"),
        Some(&LineStats {
            added: 2,
            removed: 1,
        })
    );
    assert_eq!(
        file_stats.get("notes.txt"),
        Some(&LineStats {
            added: 1,
            removed: 0,
        })
    );
    assert_eq!(
        overall_stats,
        LineStats {
            added: 3,
            removed: 1,
        }
    );

    Ok(())
}

#[test]
fn workflow_snapshot_without_refresh_matches_full_snapshot_for_worktree_changes() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/lib.rs", "one\ntwo\n")?;
    fixture.write_file("src/old_name.rs", "rename me\n")?;
    fixture.commit_all("initial")?;

    fixture.write_file("src/lib.rs", "one\nthree\n")?;
    fixture.rename_path("src/old_name.rs", "src/new_name.rs")?;
    fixture.write_file("notes.txt", "alpha\n")?;

    let full = load_workflow_snapshot(fixture.root())?;
    let light = load_workflow_snapshot_without_refresh(fixture.root())?;

    assert_eq!(light.root, full.root);
    assert_eq!(light.branch_name, full.branch_name);
    assert_eq!(light.branch_has_upstream, full.branch_has_upstream);
    assert_eq!(light.branch_ahead_count, full.branch_ahead_count);
    assert_eq!(light.branch_behind_count, full.branch_behind_count);
    assert_eq!(light.branches, full.branches);
    assert_eq!(light.files, full.files);
    assert_eq!(light.last_commit_subject, full.last_commit_subject);
    assert_eq!(light.working_copy_commit_id, full.working_copy_commit_id);
    Ok(())
}

#[test]
fn repo_snapshot_without_refresh_preserves_full_snapshot_line_stats() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/lib.rs", "one\ntwo\n")?;
    fixture.commit_all("initial")?;
    fixture.write_file("src/lib.rs", "one\nthree\nfour\n")?;
    fixture.write_file("notes.txt", "alpha\n")?;

    let full = load_snapshot(fixture.root())?;
    let without_refresh = load_snapshot_without_refresh(fixture.root())?;
    let line_stats = load_repo_line_stats_without_refresh(fixture.root())?;

    assert_eq!(without_refresh.root, full.root);
    assert_eq!(
        without_refresh.working_copy_commit_id,
        full.working_copy_commit_id
    );
    assert_eq!(without_refresh.branch_name, full.branch_name);
    assert_eq!(
        without_refresh.branch_has_upstream,
        full.branch_has_upstream
    );
    assert_eq!(without_refresh.branch_ahead_count, full.branch_ahead_count);
    assert_eq!(
        without_refresh.branch_behind_count,
        full.branch_behind_count
    );
    assert_eq!(without_refresh.branches, full.branches);
    assert_eq!(without_refresh.files, full.files);
    assert_eq!(without_refresh.line_stats, full.line_stats);
    assert_eq!(
        without_refresh.last_commit_subject,
        full.last_commit_subject
    );
    assert_eq!(line_stats, full.line_stats);
    Ok(())
}

#[test]
fn workflow_snapshot_fingerprint_changes_for_worktree_edits() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;

    let (previous_fingerprint, previous_workflow) =
        load_workflow_snapshot_with_fingerprint(fixture.root())?;
    assert!(previous_workflow.files.is_empty());

    fixture.write_file("tracked.txt", "base\nnext\n")?;

    let (next_fingerprint, next_workflow) = load_workflow_snapshot_if_changed_without_refresh(
        fixture.root(),
        Some(&previous_fingerprint),
    )?;

    assert_ne!(previous_fingerprint, next_fingerprint);
    let next_workflow = next_workflow.expect("worktree edit should invalidate fingerprint");
    assert_eq!(next_workflow.files.len(), 1);
    assert_eq!(next_workflow.files[0].path, "tracked.txt");
    assert_eq!(next_workflow.files[0].status, FileStatus::Modified);
    Ok(())
}

#[test]
fn workflow_snapshot_fingerprint_changes_for_same_size_modified_edits_without_refresh() -> Result<()>
{
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    fixture.write_file("tracked.txt", "base\n1111\n")?;

    let (previous_fingerprint, previous_workflow) =
        load_workflow_snapshot_with_fingerprint(fixture.root())?;
    assert_eq!(previous_workflow.files.len(), 1);
    assert_eq!(previous_workflow.files[0].status, FileStatus::Modified);

    thread::sleep(Duration::from_millis(1100));
    fixture.write_file("tracked.txt", "base\n2222\n")?;

    let (next_fingerprint, next_workflow) = load_workflow_snapshot_if_changed_without_refresh(
        fixture.root(),
        Some(&previous_fingerprint),
    )?;

    assert_ne!(previous_fingerprint, next_fingerprint);
    let next_workflow =
        next_workflow.expect("same-size content change should invalidate fingerprint");
    assert_eq!(next_workflow.files.len(), 1);
    assert_eq!(next_workflow.files[0].path, "tracked.txt");
    assert_eq!(next_workflow.files[0].status, FileStatus::Modified);
    assert_ne!(
        previous_workflow.working_copy_commit_id,
        next_workflow.working_copy_commit_id
    );
    Ok(())
}

#[test]
fn workflow_snapshot_fingerprint_changes_for_modified_content_with_same_status() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    fixture.write_file("tracked.txt", "base\nfirst\n")?;

    let (previous_fingerprint, previous_workflow) =
        load_workflow_snapshot_with_fingerprint(fixture.root())?;
    assert_eq!(previous_workflow.files.len(), 1);
    assert_eq!(previous_workflow.files[0].status, FileStatus::Modified);

    fixture.write_file("tracked.txt", "base\nsecond\n")?;

    let (next_fingerprint, next_workflow) = load_workflow_snapshot_if_changed_without_refresh(
        fixture.root(),
        Some(&previous_fingerprint),
    )?;

    assert_ne!(previous_fingerprint, next_fingerprint);
    let next_workflow = next_workflow.expect("content change should invalidate fingerprint");
    assert_eq!(next_workflow.files.len(), 1);
    assert_eq!(next_workflow.files[0].path, "tracked.txt");
    assert_eq!(next_workflow.files[0].status, FileStatus::Modified);
    assert_ne!(
        previous_workflow.working_copy_commit_id,
        next_workflow.working_copy_commit_id
    );
    Ok(())
}

#[test]
fn workflow_snapshot_fingerprint_changes_for_tracking_only_updates() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;
    let branch_name = fixture.current_branch_name()?;
    let remote_root = fixture.create_bare_remote("origin")?;
    fixture.push_current_branch("origin", branch_name.as_str())?;
    fixture.set_upstream(branch_name.as_str(), &format!("origin/{branch_name}"))?;

    let (previous_fingerprint, previous_workflow) =
        load_workflow_snapshot_with_fingerprint(fixture.root())?;
    assert!(previous_workflow.branch_has_upstream);
    assert_eq!(previous_workflow.branch_ahead_count, 0);
    assert_eq!(previous_workflow.branch_behind_count, 0);
    assert!(previous_workflow.files.is_empty());

    let peer = TempGitRepo::clone_branch(remote_root.as_path(), branch_name.as_str())?;
    peer.write_file("peer.txt", "peer\n")?;
    peer.commit_all("peer update")?;
    peer.push_current_branch("origin", branch_name.as_str())?;

    let refspec = format!("refs/heads/{branch_name}:refs/remotes/origin/{branch_name}");
    fixture.fetch_remote("origin", &[refspec.as_str()])?;

    let (next_fingerprint, next_workflow) = load_workflow_snapshot_if_changed_without_refresh(
        fixture.root(),
        Some(&previous_fingerprint),
    )?;

    assert_ne!(previous_fingerprint, next_fingerprint);
    let next_workflow = next_workflow.expect("tracking update should invalidate fingerprint");
    assert_eq!(
        next_workflow.working_copy_commit_id,
        previous_workflow.working_copy_commit_id
    );
    assert!(next_workflow.branch_has_upstream);
    assert_eq!(next_workflow.branch_ahead_count, 0);
    assert_eq!(next_workflow.branch_behind_count, 1);
    assert!(next_workflow.files.is_empty());
    Ok(())
}

#[test]
fn workflow_snapshot_reports_staged_index_only_changes_when_worktree_matches_head() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("tracked.txt", "base\n")?;
    fixture.commit_all("initial")?;

    fixture.write_file("tracked.txt", "staged\n")?;
    fixture.stage_path("tracked.txt")?;
    fixture.write_file("tracked.txt", "base\n")?;

    let workflow = load_workflow_snapshot(fixture.root())?;
    let overall_stats = load_repo_line_stats(fixture.root())?;

    assert_eq!(workflow.files.len(), 1);
    assert_eq!(workflow.files[0].path, "tracked.txt");
    assert_eq!(workflow.files[0].status, FileStatus::Modified);
    assert!(workflow.files[0].staged);
    assert!(workflow.files[0].unstaged);
    let patch = load_patch(
        fixture.root(),
        workflow.files[0].path.as_str(),
        workflow.files[0].status,
    )?;
    assert!(patch.contains("diff --git a/tracked.txt b/tracked.txt"));
    assert!(patch.contains("-base"));
    assert!(patch.contains("+staged"));
    assert_eq!(
        overall_stats,
        LineStats {
            added: 1,
            removed: 1
        }
    );
    Ok(())
}

#[test]
fn load_repo_tree_marks_ignored_entries_and_counts_visible_nodes() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/main.rs", "fn main() {}\n")?;
    fixture.commit_all("initial")?;
    fixture.write_file(".gitignore", "target/\n*.log\n")?;
    fixture.write_file("draft.txt", "draft\n")?;
    fixture.write_file("target/cache.bin", "cache\n")?;
    fixture.write_file("logs/app.log", "hello\n")?;

    let entries = load_repo_tree(fixture.root())?;
    let (file_count, folder_count) = count_non_ignored_repo_tree_entries(&entries);

    assert!(entries.iter().any(|entry| {
        entry.path == "src" && entry.kind == RepoTreeEntryKind::Directory && !entry.ignored
    }));
    assert!(entries.iter().any(|entry| {
        entry.path == "src/main.rs" && entry.kind == RepoTreeEntryKind::File && !entry.ignored
    }));
    assert!(entries.iter().any(|entry| {
        entry.path == ".gitignore" && entry.kind == RepoTreeEntryKind::File && !entry.ignored
    }));
    assert!(entries.iter().any(|entry| {
        entry.path == "draft.txt" && entry.kind == RepoTreeEntryKind::File && !entry.ignored
    }));
    assert!(entries.iter().any(|entry| {
        entry.path == "target" && entry.kind == RepoTreeEntryKind::Directory && entry.ignored
    }));
    assert!(entries.iter().all(|entry| entry.path != "target/cache.bin"));
    assert!(entries.iter().all(|entry| entry.path != "logs/app.log"));
    assert!(
        entries
            .iter()
            .all(|entry| entry.path != ".git" && !entry.path.starts_with(".git/"))
    );
    assert_eq!(file_count, 3);
    assert_eq!(folder_count, 1);

    Ok(())
}

#[test]
fn load_visible_repo_file_paths_honors_gitignore() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/main.rs", "fn main() {}\n")?;
    fixture.commit_all("initial")?;
    fixture.write_file(".gitignore", "target/\n*.log\n")?;
    fixture.write_file("draft.txt", "draft\n")?;
    fixture.write_file("target/cache.bin", "cache\n")?;
    fixture.write_file("logs/app.log", "hello\n")?;

    let paths = load_visible_repo_file_paths(fixture.root())?;

    assert!(paths.contains(&"src/main.rs".to_string()));
    assert!(paths.contains(&".gitignore".to_string()));
    assert!(paths.contains(&"draft.txt".to_string()));
    assert!(!paths.contains(&"target".to_string()));
    assert!(!paths.contains(&"target/cache.bin".to_string()));
    assert!(!paths.contains(&"logs/app.log".to_string()));

    Ok(())
}

#[test]
fn workflow_snapshot_excludes_non_ignored_nested_repo_contents() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/main.rs", "fn main() {}\n")?;
    fixture.commit_all("initial")?;

    let nested_root = fixture.root().join("vendor/nested");
    fs::create_dir_all(nested_root.join("src"))?;
    let nested_repo = Repository::init(nested_root.as_path())?;
    drop(nested_repo);
    fs::write(nested_root.join("src/lib.rs"), "nested\n")?;

    let full = load_workflow_snapshot(fixture.root())?;
    let light = load_workflow_snapshot_without_refresh(fixture.root())?;
    let entries = load_repo_tree(fixture.root())?;

    assert!(full.files.is_empty());
    assert_eq!(light.files, full.files);
    assert!(
        entries
            .iter()
            .all(|entry| entry.path != "vendor/nested" && !entry.path.starts_with("vendor/nested/"))
    );

    Ok(())
}

#[test]
fn load_visible_repo_file_paths_skips_nested_repo_contents() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/main.rs", "fn main() {}\n")?;
    fixture.commit_all("initial")?;

    let nested_root = fixture.root().join("vendor/nested");
    fs::create_dir_all(nested_root.join("src"))?;
    let nested_repo = Repository::init(nested_root.as_path())?;
    drop(nested_repo);
    fs::write(nested_root.join("src/lib.rs"), "nested\n")?;

    let paths = load_visible_repo_file_paths(fixture.root())?;

    assert!(paths.contains(&"src/main.rs".to_string()));
    assert!(!paths.iter().any(|path| path.starts_with("vendor/nested")));

    Ok(())
}

#[test]
fn workflow_snapshot_skips_nested_repo_contents_inside_ignored_directories() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file(".gitignore", "target-shared/\n")?;
    fixture.commit_all("initial")?;

    let nested_root = fixture.root().join("target-shared/cache/repo");
    fs::create_dir_all(nested_root.join("src"))?;
    let nested_repo = Repository::init(nested_root.as_path())?;
    drop(nested_repo);
    fs::write(nested_root.join("src/lib.rs"), "nested\n")?;

    let workflow = load_workflow_snapshot_without_refresh(fixture.root())?;
    let entries = load_repo_tree(fixture.root())?;

    assert!(workflow.files.is_empty());
    assert!(entries.iter().any(|entry| {
        entry.path == "target-shared" && entry.kind == RepoTreeEntryKind::Directory && entry.ignored
    }));
    assert!(
        entries
            .iter()
            .all(|entry| !entry.path.starts_with("target-shared/cache"))
    );

    Ok(())
}

#[test]
fn file_line_stats_for_paths_only_returns_requested_changed_files() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/lib.rs", "one\ntwo\n")?;
    fixture.write_file("README.md", "hello\n")?;
    fixture.commit_all("initial")?;
    fixture.write_file("src/lib.rs", "one\nthree\n")?;
    fixture.write_file("README.md", "hello\nworld\n")?;

    let requested = BTreeSet::from([String::from("README.md")]);
    let stats = hunk_git::git::load_repo_file_line_stats_for_paths_without_refresh(
        fixture.root(),
        &requested,
    )?;

    assert_eq!(stats.len(), 1);
    assert_eq!(
        stats.get("README.md"),
        Some(&LineStats {
            added: 1,
            removed: 0,
        })
    );
    Ok(())
}

#[test]
fn patch_session_renders_unified_diff_for_requested_files() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/lib.rs", "one\ntwo\n")?;
    fixture.write_file("README.md", "hello\n")?;
    fixture.commit_all("initial")?;
    fixture.write_file("src/lib.rs", "one\nthree\nfour\n")?;
    fixture.write_file("README.md", "hello\nworld\n")?;

    let workflow = load_workflow_snapshot(fixture.root())?;
    let session = open_patch_session(fixture.root())?;
    let patches = load_patches_for_files_from_session(&session, &workflow.files)?;

    let lib_patch = patches
        .get("src/lib.rs")
        .expect("session should render src/lib.rs");
    assert!(lib_patch.contains("diff --git a/src/lib.rs b/src/lib.rs"));
    assert!(lib_patch.contains("--- a/src/lib.rs"));
    assert!(lib_patch.contains("+++ b/src/lib.rs"));
    assert!(lib_patch.contains("@@ -1,2 +1,3 @@"));
    assert!(lib_patch.contains("-two"));
    assert!(lib_patch.contains("+three"));
    assert!(lib_patch.contains("+four"));

    let readme_patch = patches
        .get("README.md")
        .expect("session should render README.md");
    assert!(readme_patch.contains("diff --git a/README.md b/README.md"));
    assert!(readme_patch.contains("+world"));
    Ok(())
}

#[test]
fn workflow_snapshot_reports_unstaged_rename_as_single_entry() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/old_name.rs", "one\ntwo\n")?;
    fixture.commit_all("initial")?;
    fixture.rename_path("src/old_name.rs", "src/new_name.rs")?;

    let workflow = load_workflow_snapshot(fixture.root())?;
    assert_eq!(workflow.files.len(), 1);
    assert_eq!(workflow.files[0].path, "src/new_name.rs");
    assert_eq!(workflow.files[0].status, FileStatus::Renamed);
    assert!(!workflow.files[0].staged);
    assert!(workflow.files[0].unstaged);

    let patch = load_patch(
        fixture.root(),
        workflow.files[0].path.as_str(),
        workflow.files[0].status,
    )?;
    assert!(patch.contains("diff --git a/src/old_name.rs b/src/new_name.rs"));
    assert!(patch.contains("rename from src/old_name.rs"));
    assert!(patch.contains("rename to src/new_name.rs"));
    Ok(())
}

#[test]
fn patch_session_uses_source_path_for_staged_rename() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_file("src/old_name.rs", "one\ntwo\n")?;
    fixture.commit_all("initial")?;
    fixture.rename_path("src/old_name.rs", "src/new_name.rs")?;
    fixture.stage_rename("src/old_name.rs", "src/new_name.rs")?;

    let workflow = load_workflow_snapshot(fixture.root())?;
    assert_eq!(workflow.files.len(), 1);
    assert_eq!(workflow.files[0].path, "src/new_name.rs");
    assert_eq!(workflow.files[0].status, FileStatus::Renamed);
    assert!(workflow.files[0].staged);
    assert!(!workflow.files[0].unstaged);

    let session = open_patch_session(fixture.root())?;
    let patches = load_patches_for_files_from_session(&session, &workflow.files)?;
    let patch = patches
        .get("src/new_name.rs")
        .expect("rename patch should be present");
    assert!(patch.contains("diff --git a/src/old_name.rs b/src/new_name.rs"));
    assert!(patch.contains("rename from src/old_name.rs"));
    assert!(patch.contains("rename to src/new_name.rs"));
    Ok(())
}

#[test]
fn load_patch_marks_binary_diffs_without_preview_hunks() -> Result<()> {
    let fixture = TempGitRepo::new()?;
    fixture.write_bytes("image.bin", b"\0before")?;
    fixture.commit_all("initial")?;
    fixture.write_bytes("image.bin", b"\0after")?;

    let workflow = load_workflow_snapshot(fixture.root())?;
    let binary_file = workflow
        .files
        .iter()
        .find(|file| file.path == "image.bin")
        .expect("binary file should be reported as changed");
    let patch = load_patch(
        fixture.root(),
        binary_file.path.as_str(),
        binary_file.status,
    )?;

    assert!(patch.contains("diff --git a/image.bin b/image.bin"));
    assert!(patch.contains("Binary files a/image.bin and b/image.bin differ"));
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

    fn clone_branch(remote: &Path, branch_name: &str) -> Result<Self> {
        let tempdir = tempfile::tempdir()?;
        let root = tempdir.path().join("repo");
        let repo = RepoBuilder::new()
            .branch(branch_name)
            .clone(remote.to_string_lossy().as_ref(), root.as_path())?;
        drop(repo);
        Ok(Self {
            root: fs::canonicalize(root)?,
            tempdir,
        })
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

    fn write_bytes(&self, relative: &str, contents: &[u8]) -> Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
        Ok(())
    }

    fn rename_path(&self, from: &str, to: &str) -> Result<()> {
        let destination = self.root.join(to);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(self.root.join(from), destination)?;
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

    fn stage_path(&self, relative: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut index = repo.index()?;
        index.add_path(Path::new(relative))?;
        index.write()?;
        Ok(())
    }

    fn stage_rename(&self, from: &str, to: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut index = repo.index()?;
        index.remove_path(Path::new(from))?;
        index.add_path(Path::new(to))?;
        index.write()?;
        Ok(())
    }

    fn current_branch_name(&self) -> Result<String> {
        let repo = self.repository()?;
        let head = repo.head()?;
        Ok(head.shorthand().unwrap_or("HEAD").to_string())
    }

    fn checkout_branch(&self, name: &str) -> Result<()> {
        let repo = self.repository()?;
        let head_commit = repo.head()?.peel_to_commit()?;
        repo.branch(name, &head_commit, false)?;
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

    fn push_current_branch(&self, remote_name: &str, branch_name: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut remote = repo.find_remote(remote_name)?;
        remote.push(
            &[format!("refs/heads/{branch_name}:refs/heads/{branch_name}")],
            None,
        )?;
        Ok(())
    }

    fn fetch_remote(&self, remote_name: &str, refspecs: &[&str]) -> Result<()> {
        let repo = self.repository()?;
        let mut remote = repo.find_remote(remote_name)?;
        remote.fetch(refspecs, None, None)?;
        Ok(())
    }

    fn set_upstream(&self, branch_name: &str, upstream: &str) -> Result<()> {
        let repo = self.repository()?;
        let mut branch = repo.find_branch(branch_name, BranchType::Local)?;
        branch.set_upstream(Some(upstream))?;
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
