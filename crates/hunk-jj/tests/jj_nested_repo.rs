use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_jj::jj::{commit_staged, load_snapshot};

#[test]
fn parent_snapshot_ignores_changes_under_nested_git_repo() {
    let fixture = TempRepo::new("nested-repo-ignore");

    write_file(fixture.path().join("app.txt"), "app base\n");
    write_file(
        fixture.path().join("nested").join("module.txt"),
        "module base\n",
    );
    commit_staged(fixture.path(), "initial parent commit").expect("initial commit should succeed");

    run_jj(
        fixture.path().join("nested").as_path(),
        ["git", "init", "--colocate"],
    );

    let snapshot_after_nested_init =
        load_snapshot(fixture.path()).expect("snapshot should load after nested repo init");
    assert!(
        snapshot_after_nested_init
            .files
            .iter()
            .all(|file| !file.path.starts_with("nested/")),
        "parent snapshot should ignore nested-repo subtree entries"
    );

    write_file(
        fixture.path().join("nested").join("module.txt"),
        "module changed\n",
    );
    write_file(fixture.path().join("app.txt"), "app changed\n");

    let snapshot_after_changes =
        load_snapshot(fixture.path()).expect("snapshot should load after nested and parent edits");
    assert!(
        snapshot_after_changes
            .files
            .iter()
            .any(|file| file.path == "app.txt"),
        "parent changes should still be visible"
    );
    assert!(
        snapshot_after_changes
            .files
            .iter()
            .all(|file| !file.path.starts_with("nested/")),
        "nested repo edits should remain hidden in parent snapshot"
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
