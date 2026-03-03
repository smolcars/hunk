use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_jj::jj::{
    GraphBookmarkScope, GraphSnapshotOptions, checkout_or_create_bookmark, commit_staged,
    create_bookmark_at_revision, load_graph_snapshot, load_snapshot, move_bookmark_to_revision,
    rename_bookmark,
};

#[test]
fn creating_bookmark_at_revision_targets_selected_revision_without_switching() {
    let fixture = TempRepo::new("graph-create-bookmark-at-revision");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("creating main bookmark should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    commit_staged(fixture.path(), "main second commit").expect("second commit should succeed");

    let main_target = bookmark_target_commit_id(fixture.path(), "main");
    create_bookmark_at_revision(fixture.path(), "review-preview", &main_target)
        .expect("creating bookmark at selected revision should succeed");

    let preview_target = bookmark_target_commit_id(fixture.path(), "review-preview");
    assert_eq!(
        preview_target, main_target,
        "created bookmark should target selected revision"
    );

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load");
    assert_eq!(
        snapshot.branch_name, "main",
        "creating a bookmark from graph action should not switch active bookmark"
    );
}

#[test]
fn forking_bookmark_flow_points_new_bookmark_to_source_tip_revision() {
    let fixture = TempRepo::new("graph-fork-bookmark-flow");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("creating feature bookmark should succeed");

    write_file(
        fixture.path().join("tracked.txt"),
        "line one\nfeature change\n",
    );
    commit_staged(fixture.path(), "feature tip").expect("feature commit should succeed");

    let feature_tip = bookmark_target_commit_id(fixture.path(), "feature");
    create_bookmark_at_revision(fixture.path(), "feature-fork", &feature_tip)
        .expect("fork bookmark creation should succeed");

    let fork_tip = bookmark_target_commit_id(fixture.path(), "feature-fork");
    assert_eq!(
        fork_tip, feature_tip,
        "forked bookmark should point at source bookmark tip revision"
    );
}

#[test]
fn renaming_bookmark_flow_updates_listing() {
    let fixture = TempRepo::new("graph-rename-bookmark-flow");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "rename-me")
        .expect("creating source bookmark should succeed");

    rename_bookmark(fixture.path(), "rename-me", "renamed")
        .expect("renaming bookmark should succeed");

    let listing = run_jj_capture(fixture.path(), ["bookmark", "list", "rename-me", "renamed"]);
    assert!(
        listing.contains("renamed:"),
        "renamed bookmark should be present in listing"
    );
    assert!(
        !listing.contains("rename-me:"),
        "old bookmark should be absent after rename"
    );
}

#[test]
fn moving_bookmark_to_revision_retargets_existing_bookmark() {
    let fixture = TempRepo::new("graph-move-bookmark-flow");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("creating main bookmark should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nmain two\n");
    commit_staged(fixture.path(), "main second commit").expect("main second commit should succeed");
    let main_tip = bookmark_target_commit_id(fixture.path(), "main");

    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("creating feature bookmark should succeed");
    write_file(
        fixture.path().join("tracked.txt"),
        "line one\nmain two\nfeature three\n",
    );
    commit_staged(fixture.path(), "feature third commit").expect("feature commit should succeed");

    move_bookmark_to_revision(fixture.path(), "feature", &main_tip)
        .expect("moving feature bookmark to selected revision should succeed");

    let feature_target = bookmark_target_commit_id(fixture.path(), "feature");
    assert_eq!(
        feature_target, main_tip,
        "move operation should retarget feature bookmark to selected revision"
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

fn bookmark_target_commit_id(cwd: &Path, bookmark_name: &str) -> String {
    let graph = load_graph_snapshot(cwd, GraphSnapshotOptions::default())
        .expect("graph snapshot should load");
    graph
        .nodes
        .iter()
        .find(|node| {
            node.bookmarks.iter().any(|bookmark| {
                bookmark.scope == GraphBookmarkScope::Local
                    && bookmark.name == bookmark_name
                    && bookmark.remote.is_none()
            })
        })
        .map(|node| node.id.clone())
        .expect("bookmark target commit id should exist in graph snapshot")
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
    assert!(
        output.status.success(),
        "jj command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}
