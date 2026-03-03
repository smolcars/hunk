use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_jj::jj::{
    checkout_or_create_bookmark, commit_selected_paths, commit_staged, load_snapshot,
    restore_all_working_copy_changes, restore_working_copy_paths, stage_all, unstage_all,
};

#[test]
fn stage_actions_are_rejected_with_jj_backend() {
    let fixture = TempRepo::new("stage-actions-unsupported");
    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");

    let stage_all_err = stage_all(fixture.path()).expect_err("stage_all should fail under JJ");
    assert!(
        stage_all_err.to_string().contains("staging index"),
        "error should explain why stage_all is unsupported"
    );

    let unstage_all_err =
        unstage_all(fixture.path()).expect_err("unstage_all should fail under JJ");
    assert!(
        unstage_all_err.to_string().contains("staging index"),
        "error should explain why unstage_all is unsupported"
    );
}

#[test]
fn commit_staged_commits_working_copy_changes_with_jj() {
    let fixture = TempRepo::new("commit-staged-jj");
    let tracked = fixture.path().join("tracked.txt");
    write_file(tracked.clone(), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");

    write_file(tracked, "line one\nline two\n");
    commit_staged(fixture.path(), "update tracked").expect("second commit should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after commit");
    assert!(snapshot.files.is_empty(), "working copy should be clean");
    assert!(
        snapshot.last_commit_subject.as_deref() == Some("update tracked"),
        "last commit subject should match the latest commit"
    );
}

#[test]
fn commit_prefers_active_bookmark_preference_over_git_head() {
    let fixture = TempRepo::new("commit-prefers-active-bookmark");
    let tracked = fixture.path().join("tracked.txt");
    write_file(tracked.clone(), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("creating main bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("creating feature bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("switching back to main should succeed");

    fs::write(
        fixture.path().join(".jj").join("hunk-active-bookmark"),
        "feature\n",
    )
    .expect("active bookmark preference should be writable");

    write_file(tracked, "line one\nline two\n");
    commit_staged(fixture.path(), "prefer active bookmark").expect("commit should succeed");

    let feature_log = run_jj_capture(
        fixture.path(),
        ["log", "-r", "feature", "-n", "1", "--no-graph"],
    );
    assert!(
        feature_log.contains("prefer active bookmark"),
        "feature bookmark should advance when it is the active preference"
    );

    let main_log = run_jj_capture(
        fixture.path(),
        ["log", "-r", "main", "-n", "1", "--no-graph"],
    );
    assert!(
        !main_log.contains("prefer active bookmark"),
        "main bookmark should not advance when active preference points to feature"
    );
}

#[test]
fn commit_selected_paths_only_commits_requested_files() {
    let fixture = TempRepo::new("commit-selected-paths");
    let alpha = fixture.path().join("alpha.txt");
    let beta = fixture.path().join("beta.txt");

    write_file(alpha.clone(), "alpha one\n");
    write_file(beta.clone(), "beta one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");

    write_file(alpha, "alpha one\nalpha two\n");
    write_file(beta, "beta one\nbeta two\n");
    commit_selected_paths(
        fixture.path(),
        "commit alpha only",
        &["alpha.txt".to_string()],
    )
    .expect("partial commit should succeed");

    let snapshot =
        load_snapshot(fixture.path()).expect("snapshot should load after partial commit");
    assert!(
        snapshot.files.iter().any(|file| file.path == "beta.txt"),
        "unselected file should remain in working copy"
    );
    assert!(
        snapshot.files.iter().all(|file| file.path != "alpha.txt"),
        "selected file should be committed"
    );
    assert_eq!(
        snapshot.last_commit_subject.as_deref(),
        Some("commit alpha only"),
        "last commit subject should match partial commit message"
    );
}

#[test]
fn commit_selected_paths_deduplicates_file_list() {
    let fixture = TempRepo::new("commit-selected-dedup");
    let alpha = fixture.path().join("alpha.txt");
    let beta = fixture.path().join("beta.txt");

    write_file(alpha.clone(), "alpha one\n");
    write_file(beta.clone(), "beta one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");

    write_file(alpha, "alpha one\nalpha two\n");
    write_file(beta, "beta one\nbeta two\n");

    let committed = commit_selected_paths(
        fixture.path(),
        "commit alpha once",
        &[
            "alpha.txt".to_string(),
            "alpha.txt".to_string(),
            "alpha.txt/".to_string(),
        ],
    )
    .expect("partial commit should succeed");
    assert_eq!(committed, 1, "duplicate paths should be committed once");

    let snapshot =
        load_snapshot(fixture.path()).expect("snapshot should load after partial commit");
    assert!(
        snapshot.files.iter().any(|file| file.path == "beta.txt"),
        "unselected file should remain in working copy"
    );
    assert!(
        snapshot.files.iter().all(|file| file.path != "alpha.txt"),
        "selected file should be committed"
    );
}

#[test]
fn restore_working_copy_paths_reverts_modified_and_untracked_files() {
    let fixture = TempRepo::new("restore-working-copy-paths");
    let tracked = fixture.path().join("tracked.txt");
    let scratch = fixture.path().join("scratch.txt");

    write_file(tracked.clone(), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");

    write_file(tracked.clone(), "line one\nline two\n");
    write_file(scratch.clone(), "temporary\n");

    restore_working_copy_paths(
        fixture.path(),
        &["tracked.txt".to_string(), "scratch.txt".to_string()],
    )
    .expect("restoring selected files should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after restore");
    assert!(
        snapshot.files.is_empty(),
        "working copy should be clean after restoring all changed files"
    );
    assert_eq!(
        fs::read_to_string(tracked).expect("tracked file should remain on disk"),
        "line one\n",
        "tracked file content should be restored to the parent revision"
    );
    assert!(
        !scratch.exists(),
        "untracked file should be removed after restore"
    );
}

#[test]
fn restore_working_copy_paths_only_reverts_requested_file() {
    let fixture = TempRepo::new("restore-single-working-copy-path");
    let alpha = fixture.path().join("alpha.txt");
    let beta = fixture.path().join("beta.txt");

    write_file(alpha.clone(), "alpha one\n");
    write_file(beta.clone(), "beta one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");

    write_file(alpha.clone(), "alpha one\nalpha two\n");
    write_file(beta.clone(), "beta one\nbeta two\n");

    restore_working_copy_paths(fixture.path(), &["alpha.txt".to_string()])
        .expect("restoring a single file should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after restore");
    assert!(
        snapshot.files.iter().any(|file| file.path == "beta.txt"),
        "non-restored file should remain modified"
    );
    assert!(
        snapshot.files.iter().all(|file| file.path != "alpha.txt"),
        "restored file should no longer appear in changed files"
    );
    assert_eq!(
        fs::read_to_string(alpha).expect("alpha file should remain on disk"),
        "alpha one\n",
        "restored file should match the parent revision content"
    );
}

#[test]
fn restore_all_working_copy_changes_reverts_everything() {
    let fixture = TempRepo::new("restore-all-working-copy-changes");
    let tracked = fixture.path().join("tracked.txt");
    let scratch = fixture.path().join("scratch.txt");

    write_file(tracked.clone(), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");

    write_file(tracked.clone(), "line one\nline two\n");
    write_file(scratch.clone(), "temporary\n");

    restore_all_working_copy_changes(fixture.path())
        .expect("restoring all working-copy changes should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after restore");
    assert!(
        snapshot.files.is_empty(),
        "working copy should be clean after restoring all changes"
    );
    assert_eq!(
        fs::read_to_string(tracked).expect("tracked file should remain on disk"),
        "line one\n",
        "tracked file should return to parent content"
    );
    assert!(
        !scratch.exists(),
        "untracked files should be removed when restoring all changes"
    );
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("hunk-{prefix}-{unique}"));
        fs::create_dir_all(&path).expect("temp repo directory should be created");

        run_jj(&path, ["git", "init", "--colocate"]);
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: PathBuf, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directories should be created");
    }
    fs::write(path, contents).expect("file should be written");
}

fn run_jj<const N: usize>(cwd: &Path, args: [&str; N]) {
    let status = Command::new("jj")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("jj command should run");
    assert!(status.success(), "jj command failed");
}

fn run_jj_capture<const N: usize>(cwd: &Path, args: [&str; N]) -> String {
    let output = Command::new("jj")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("jj command should run");
    assert!(output.status.success(), "jj command failed");
    String::from_utf8(output.stdout).expect("jj output should be utf-8")
}
