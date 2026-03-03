use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_jj::jj::{RepoTreeEntryKind, count_non_ignored_repo_tree_entries, load_repo_tree};

#[test]
fn load_repo_tree_marks_jj_ignored_entries() {
    let fixture = TempRepo::new("repo-tree-ignored");
    write_file(fixture.path().join(".gitignore"), "target/\n*.log\n");
    write_file(fixture.path().join("src/main.rs"), "fn main() {}\n");
    write_file(fixture.path().join("target/cache.bin"), "cache\n");
    write_file(fixture.path().join("logs/app.log"), "hello\n");
    run_jj(fixture.path(), ["status"]);

    let entries = load_repo_tree(fixture.path()).expect("repo tree should load");

    assert!(entries.iter().any(|entry| {
        entry.path == "src" && entry.kind == RepoTreeEntryKind::Directory && !entry.ignored
    }));
    assert!(entries.iter().any(|entry| {
        entry.path == "src/main.rs" && entry.kind == RepoTreeEntryKind::File && !entry.ignored
    }));
    assert!(entries.iter().any(|entry| {
        entry.path == "target" && entry.kind == RepoTreeEntryKind::Directory && entry.ignored
    }));
    assert!(entries.iter().all(|entry| entry.path != "target/cache.bin"));
    assert!(entries.iter().all(|entry| entry.path != "logs/app.log"));
}

#[test]
fn count_non_ignored_repo_tree_entries_excludes_gitignored_paths() {
    let fixture = TempRepo::new("repo-tree-counts-ignore-gitignored");
    write_file(fixture.path().join(".gitignore"), "target/\n*.log\n");
    write_file(fixture.path().join("src/main.rs"), "fn main() {}\n");
    write_file(fixture.path().join("target/cache.bin"), "cache\n");
    write_file(fixture.path().join("logs/app.log"), "hello\n");
    run_jj(fixture.path(), ["status"]);

    let entries = load_repo_tree(fixture.path()).expect("repo tree should load");
    let (file_count, folder_count) = count_non_ignored_repo_tree_entries(&entries);

    assert_eq!(
        file_count, 2,
        "only .gitignore and src/main.rs should be counted"
    );
    assert_eq!(folder_count, 1, "only src should be counted");
}

#[test]
fn load_repo_tree_excludes_internal_vcs_directories() {
    let fixture = TempRepo::new("repo-tree-no-vcs-internals");
    write_file(fixture.path().join("README.md"), "# hunk\n");
    run_jj(fixture.path(), ["status"]);

    let entries = load_repo_tree(fixture.path()).expect("repo tree should load");
    assert!(entries.iter().all(|entry| !entry.path.starts_with(".git")));
    assert!(entries.iter().all(|entry| !entry.path.starts_with(".jj")));
}

#[test]
fn load_repo_tree_includes_short_markdown_filenames() {
    let fixture = TempRepo::new("repo-tree-short-markdown-names");
    write_file(fixture.path().join("x.md"), "x\n");
    write_file(fixture.path().join("xxyy.md"), "xxyy\n");
    write_file(fixture.path().join("readme2.md"), "readme2\n");
    run_jj(fixture.path(), ["status"]);

    let entries = load_repo_tree(fixture.path()).expect("repo tree should load");
    for expected in ["x.md", "xxyy.md", "readme2.md"] {
        assert!(
            entries
                .iter()
                .any(|entry| entry.path == expected && entry.kind == RepoTreeEntryKind::File),
            "missing expected file entry for {expected}"
        );
    }
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
