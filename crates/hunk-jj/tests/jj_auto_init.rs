use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_jj::jj::load_snapshot;

#[test]
fn load_snapshot_auto_initializes_jj_for_git_repo() {
    let fixture = TempDir::new("jj-auto-init-git");
    run_jj(fixture.path(), &["git", "init", "--colocate"]);
    fs::remove_dir_all(fixture.path().join(".jj"))
        .expect("should remove colocated jj metadata to simulate git-only checkout");
    write_file(fixture.path().join("hello.txt"), "hello\n");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load for git repo");

    assert!(
        fixture.path().join(".jj").exists(),
        "JJ metadata should be auto-initialized in git repo"
    );
    let jj_gitignore = fixture.path().join(".jj").join(".gitignore");
    assert!(
        jj_gitignore.exists(),
        ".jj/.gitignore should be created so git clients ignore JJ metadata"
    );
    let contents = fs::read_to_string(jj_gitignore).expect("should read .jj/.gitignore");
    assert!(
        contents.contains("/*"),
        ".jj/.gitignore should ignore JJ internals"
    );
    assert!(
        snapshot.files.iter().any(|file| file.path == "hello.txt"),
        "snapshot should include working copy change after JJ auto-init"
    );
}

#[test]
fn load_snapshot_auto_init_uses_current_git_branch_as_active_branch() {
    let fixture = TempDir::new("jj-auto-init-current-branch");
    run_jj(fixture.path(), &["git", "init", "--colocate"]);
    run_jj(
        fixture.path(),
        &["config", "set", "--repo", "user.name", "Hunk Test User"],
    );
    run_jj(
        fixture.path(),
        &[
            "config",
            "set",
            "--repo",
            "user.email",
            "hunk-tests@example.com",
        ],
    );

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    run_jj(fixture.path(), &["commit", "-m", "initial commit"]);
    run_jj(
        fixture.path(),
        &["bookmark", "create", "master", "-r", "@-"],
    );
    run_jj(fixture.path(), &["git", "export"]);
    fs::write(
        fixture.path().join(".git").join("HEAD"),
        "ref: refs/heads/master\n",
    )
    .expect("should set git HEAD to master");
    fs::remove_dir_all(fixture.path().join(".jj"))
        .expect("should remove colocated jj metadata to simulate git-only checkout");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load for git checkout");
    assert_eq!(
        snapshot.branch_name, "master",
        "snapshot should use current git branch as active branch"
    );
    assert!(
        snapshot.files.is_empty(),
        "clean git checkout should not load as untracked after auto-init"
    );
}

#[test]
fn load_snapshot_errors_when_no_jj_or_git_repo_exists() {
    let fixture = TempDir::new("jj-auto-init-none");

    let err = load_snapshot(fixture.path()).expect_err("snapshot should fail outside repositories");
    let message = err.to_string().to_lowercase();
    assert!(
        message.contains("failed to discover jj repository"),
        "error should explain JJ repository discovery failure"
    );
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("hunk-{prefix}-{unique}"));
        fs::create_dir_all(&path).expect("temp directory should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
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

fn run_jj(cwd: &Path, args: &[&str]) {
    let status = Command::new("jj")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("jj command should run");
    assert!(status.success(), "jj command failed");
}
