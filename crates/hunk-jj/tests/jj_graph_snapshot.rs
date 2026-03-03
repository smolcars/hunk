use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_jj::jj::{
    GraphBookmarkScope, GraphSnapshotOptions, checkout_or_create_bookmark, commit_staged,
    load_graph_snapshot, push_current_bookmark,
};

#[test]
fn graph_snapshot_exposes_active_bookmark_and_working_copy_context() {
    let fixture = TempRepo::new("graph-active-context");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main").expect("main bookmark should be created");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    commit_staged(fixture.path(), "main second commit").expect("second commit should succeed");

    let snapshot = load_graph_snapshot(
        fixture.path(),
        GraphSnapshotOptions {
            max_nodes: 64,
            offset: 0,
            include_remote_bookmarks: true,
        },
    )
    .expect("graph snapshot should load");

    assert_eq!(
        snapshot.active_bookmark.as_deref(),
        Some("main"),
        "active bookmark should resolve from current bookmark context"
    );
    assert!(
        !snapshot.working_copy_commit_id.is_empty(),
        "working-copy commit id should be populated"
    );

    let wc_parent = snapshot
        .working_copy_parent_commit_id
        .as_deref()
        .expect("working-copy parent should be present");

    let working_parent_nodes = snapshot
        .nodes
        .iter()
        .filter(|node| node.is_working_copy_parent)
        .collect::<Vec<_>>();
    assert_eq!(
        working_parent_nodes.len(),
        1,
        "graph should mark exactly one working-copy parent node in window"
    );
    assert_eq!(working_parent_nodes[0].id, wc_parent);
    assert!(
        working_parent_nodes[0].is_active_bookmark_target,
        "active bookmark tip should match working-copy parent on checked-out bookmark"
    );
}

#[test]
fn graph_snapshot_attaches_local_and_remote_bookmarks_to_nodes() {
    let fixture = TempRepo::new("graph-local-remote-bookmarks");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("feature bookmark should be created");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    commit_staged(fixture.path(), "feature update").expect("feature commit should succeed");

    let remote_path = fixture.path().join("remote");
    fs::create_dir_all(&remote_path).expect("remote directory should be created");
    run_jj(&remote_path, ["git", "init", "--colocate"]);
    let remote_path_str = remote_path.to_string_lossy().to_string();
    run_jj(
        fixture.path(),
        ["git", "remote", "add", "origin", remote_path_str.as_str()],
    );
    push_current_bookmark(fixture.path(), "feature", false).expect("push should succeed");

    let snapshot = load_graph_snapshot(
        fixture.path(),
        GraphSnapshotOptions {
            max_nodes: 96,
            offset: 0,
            include_remote_bookmarks: true,
        },
    )
    .expect("graph snapshot should load");

    assert!(
        snapshot.nodes.iter().any(|node| {
            node.bookmarks.iter().any(|bookmark| {
                bookmark.name == "feature" && bookmark.scope == GraphBookmarkScope::Local
            })
        }),
        "graph should include local bookmark attachment"
    );

    assert!(
        snapshot.nodes.iter().any(|node| {
            node.bookmarks.iter().any(|bookmark| {
                bookmark.name == "feature"
                    && bookmark.scope == GraphBookmarkScope::Remote
                    && bookmark.remote.as_deref() == Some("origin")
                    && bookmark.tracked
            })
        }),
        "graph should include tracked remote bookmark attachment"
    );

    assert!(
        snapshot.nodes.iter().any(|node| {
            let has_local = node.bookmarks.iter().any(|bookmark| {
                bookmark.name == "feature" && bookmark.scope == GraphBookmarkScope::Local
            });
            let has_remote = node.bookmarks.iter().any(|bookmark| {
                bookmark.name == "feature"
                    && bookmark.scope == GraphBookmarkScope::Remote
                    && bookmark.remote.as_deref() == Some("origin")
            });
            has_local && has_remote
        }),
        "after push, local and remote feature bookmarks should resolve to the same node"
    );
}

#[test]
fn graph_snapshot_supports_offset_windowing_for_large_histories() {
    let fixture = TempRepo::new("graph-windowing");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "stack").expect("stack bookmark should be created");

    for index in 0..6 {
        write_file(
            fixture.path().join("tracked.txt"),
            format!("line one\nline {}\n", index + 2).as_str(),
        );
        commit_staged(fixture.path(), format!("stack commit {index}").as_str())
            .expect("stack commit should succeed");
    }

    let page1 = load_graph_snapshot(
        fixture.path(),
        GraphSnapshotOptions {
            max_nodes: 2,
            offset: 0,
            include_remote_bookmarks: false,
        },
    )
    .expect("first graph page should load");
    assert_eq!(page1.nodes.len(), 2);
    assert!(
        page1.has_more,
        "first page should report additional history"
    );
    let page1_next = page1
        .next_offset
        .expect("first page should expose next offset");

    let page2 = load_graph_snapshot(
        fixture.path(),
        GraphSnapshotOptions {
            max_nodes: 2,
            offset: page1_next,
            include_remote_bookmarks: false,
        },
    )
    .expect("second graph page should load");
    assert_eq!(page2.nodes.len(), 2);

    let page1_ids = page1
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let page2_ids = page2
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        page1_ids.is_disjoint(&page2_ids),
        "windowed pages should not repeat commit nodes"
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
