use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_jj::jj::{
    can_redo_last_operation, can_undo_last_operation, checkout_or_create_bookmark, commit_staged,
    load_snapshot, redo_last_operation, undo_last_operation,
};

#[test]
fn undo_last_operation_matches_cli_undo() {
    let jjlib_fixture = TempRepo::new("undo-parity-jjlib");
    let cli_fixture = TempRepo::new("undo-parity-cli");

    seed_linear_bookmark_history(jjlib_fixture.path(), "main");
    seed_linear_bookmark_history(cli_fixture.path(), "main");

    undo_last_operation(jjlib_fixture.path()).expect("jj-lib undo should succeed");
    run_jj(cli_fixture.path(), ["undo"]);

    let jjlib_snapshot =
        load_snapshot(jjlib_fixture.path()).expect("jj-lib snapshot should load after undo");
    let cli_snapshot =
        load_snapshot(cli_fixture.path()).expect("cli snapshot should load after undo");
    assert_snapshot_equivalent(&jjlib_snapshot, &cli_snapshot);
}

#[test]
fn redo_last_operation_matches_cli_redo() {
    let jjlib_fixture = TempRepo::new("redo-parity-jjlib");
    let cli_fixture = TempRepo::new("redo-parity-cli");

    seed_linear_bookmark_history(jjlib_fixture.path(), "main");
    seed_linear_bookmark_history(cli_fixture.path(), "main");

    run_jj(jjlib_fixture.path(), ["undo"]);
    run_jj(cli_fixture.path(), ["undo"]);

    redo_last_operation(jjlib_fixture.path()).expect("jj-lib redo should succeed");
    run_jj(cli_fixture.path(), ["redo"]);

    let jjlib_snapshot =
        load_snapshot(jjlib_fixture.path()).expect("jj-lib snapshot should load after redo");
    let cli_snapshot =
        load_snapshot(cli_fixture.path()).expect("cli snapshot should load after redo");
    assert_snapshot_equivalent(&jjlib_snapshot, &cli_snapshot);
}

#[test]
fn can_undo_operation_available_after_history_change() {
    let fixture = TempRepo::new("undo-availability");
    seed_linear_bookmark_history(fixture.path(), "main");

    assert!(
        can_undo_last_operation(fixture.path()).expect("undo availability should load"),
        "undo should be available after creating history operations"
    );
}

#[test]
fn can_redo_operation_toggles_after_undo_and_redo() {
    let fixture = TempRepo::new("redo-availability");
    seed_linear_bookmark_history(fixture.path(), "main");

    assert!(
        !can_redo_last_operation(fixture.path()).expect("redo availability should load"),
        "redo should be unavailable before any undo"
    );

    run_jj(fixture.path(), ["undo"]);
    assert!(
        can_redo_last_operation(fixture.path()).expect("redo availability should load"),
        "redo should be available after undo"
    );

    redo_last_operation(fixture.path()).expect("redo should succeed after undo");
    assert!(
        !can_redo_last_operation(fixture.path()).expect("redo availability should load"),
        "redo should be unavailable after replaying the undone operation"
    );
}

#[test]
fn redo_without_undo_reports_nothing_to_redo() {
    let fixture = TempRepo::new("redo-nothing");
    seed_linear_bookmark_history(fixture.path(), "main");

    let err = redo_last_operation(fixture.path()).expect_err("redo should fail without undo");
    assert!(
        err.to_string().contains("Nothing to redo"),
        "error should explain there is nothing to redo"
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

fn seed_linear_bookmark_history(repo_root: &Path, bookmark_name: &str) {
    write_file(repo_root.join("tracked.txt"), "line one\n");
    commit_staged(repo_root, "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(repo_root, bookmark_name)
        .expect("creating bookmark should succeed");
    write_file(repo_root.join("tracked.txt"), "line one\nline two\n");
    commit_staged(repo_root, "second commit").expect("second commit should succeed");
}

fn assert_snapshot_equivalent(left: &hunk_jj::jj::RepoSnapshot, right: &hunk_jj::jj::RepoSnapshot) {
    assert_eq!(left.branch_name, right.branch_name);
    assert_eq!(left.branch_has_upstream, right.branch_has_upstream);
    assert_eq!(left.branch_ahead_count, right.branch_ahead_count);
    assert_eq!(left.last_commit_subject, right.last_commit_subject);

    let left_subjects: Vec<_> = left
        .bookmark_revisions
        .iter()
        .map(|revision| revision.subject.as_str())
        .collect();
    let right_subjects: Vec<_> = right
        .bookmark_revisions
        .iter()
        .map(|revision| revision.subject.as_str())
        .collect();
    assert_eq!(left_subjects, right_subjects);

    let left_files: Vec<_> = left
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.status, file.untracked))
        .collect();
    let right_files: Vec<_> = right
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.status, file.untracked))
        .collect();
    assert_eq!(left_files, right_files);
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
