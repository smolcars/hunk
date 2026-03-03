use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_jj::jj::{
    checkout_or_create_bookmark, commit_staged, load_snapshot, push_current_bookmark,
    sync_current_bookmark,
};

#[test]
fn sync_current_branch_fetches_and_updates_from_remote() {
    let fixture = SyncFixture::new("sync-current-branch");

    checkout_or_create_bookmark(fixture.local_path(), "master")
        .expect("master bookmark should be created in local repo");
    let tracked_local = fixture.local_path().join("tracked.txt");
    write_file(tracked_local, "line one\n");
    commit_staged(fixture.local_path(), "initial commit").expect("initial commit should succeed");
    push_current_bookmark(fixture.local_path(), "master", false)
        .expect("initial publish should succeed");

    run_jj(fixture.peer_path(), &["git", "fetch", "--remote", "origin"]);
    run_jj(fixture.peer_path(), &["bookmark", "track", "master@origin"]);
    checkout_or_create_bookmark(fixture.peer_path(), "master")
        .expect("peer should checkout fetched master bookmark");
    let tracked_peer = fixture.peer_path().join("tracked.txt");
    write_file(tracked_peer, "line one\nline two\n");
    commit_staged(fixture.peer_path(), "peer update").expect("peer commit should succeed");
    push_current_bookmark(fixture.peer_path(), "master", true).expect("peer push should succeed");

    sync_current_bookmark(fixture.local_path(), "master").expect("sync should succeed");

    let snapshot =
        load_snapshot(fixture.local_path()).expect("snapshot should load after successful sync");
    assert_eq!(snapshot.branch_name, "master");
    assert!(
        snapshot.branch_has_upstream,
        "master should still have upstream after sync"
    );
    assert_eq!(
        snapshot.branch_ahead_count, 0,
        "master should not be ahead after syncing to remote state"
    );
    assert!(
        snapshot.files.is_empty(),
        "working copy should remain clean after sync"
    );
    assert_eq!(
        snapshot.last_commit_subject.as_deref(),
        Some("peer update"),
        "latest commit after sync should match remote update"
    );
}

#[test]
fn sync_prefers_present_untracked_remote_over_origin_fallback() {
    let fixture = DualRemoteSyncFixture::new("sync-prefers-untracked-remote");

    checkout_or_create_bookmark(fixture.peer_path(), "master")
        .expect("peer should create master bookmark");
    write_file(fixture.peer_path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.peer_path(), "initial peer commit")
        .expect("initial peer commit should succeed");
    push_current_bookmark(fixture.peer_path(), "master", false)
        .expect("peer should publish master to upstream");

    run_jj(
        fixture.local_path(),
        &["git", "fetch", "--remote", "upstream"],
    );
    run_jj(
        fixture.local_path(),
        &["bookmark", "track", "master@upstream"],
    );
    checkout_or_create_bookmark(fixture.local_path(), "master")
        .expect("local should checkout fetched upstream master bookmark");
    run_jj(
        fixture.local_path(),
        &["bookmark", "untrack", "master@upstream"],
    );

    write_file(
        fixture.peer_path().join("tracked.txt"),
        "line one\nline two\n",
    );
    commit_staged(fixture.peer_path(), "peer upstream update")
        .expect("peer update commit should succeed");
    push_current_bookmark(fixture.peer_path(), "master", true)
        .expect("peer should push update to upstream");

    sync_current_bookmark(fixture.local_path(), "master").expect("sync should succeed");

    let snapshot =
        load_snapshot(fixture.local_path()).expect("snapshot should load after successful sync");
    assert_eq!(snapshot.branch_name, "master");
    assert!(
        snapshot.branch_has_upstream,
        "master should have upstream after sync"
    );
    assert_eq!(
        snapshot.last_commit_subject.as_deref(),
        Some("peer upstream update"),
        "sync should update master from upstream remote, not origin fallback"
    );
}

#[test]
fn snapshot_counts_multiple_revisions_ahead_of_upstream() {
    let fixture = SyncFixture::new("ahead-count");

    checkout_or_create_bookmark(fixture.local_path(), "master")
        .expect("master bookmark should be created in local repo");
    let tracked_local = fixture.local_path().join("tracked.txt");
    write_file(tracked_local.clone(), "line one\n");
    commit_staged(fixture.local_path(), "initial commit").expect("initial commit should succeed");
    push_current_bookmark(fixture.local_path(), "master", false)
        .expect("initial publish should succeed");

    write_file(tracked_local.clone(), "line one\nline two\n");
    commit_staged(fixture.local_path(), "second local commit")
        .expect("second commit should succeed");
    write_file(tracked_local, "line one\nline two\nline three\n");
    commit_staged(fixture.local_path(), "third local commit").expect("third commit should succeed");

    let snapshot =
        load_snapshot(fixture.local_path()).expect("snapshot should load for ahead count");
    assert!(
        snapshot.branch_has_upstream,
        "master should still have upstream after local commits"
    );
    assert_eq!(
        snapshot.branch_ahead_count, 2,
        "ahead count should include all unpushed revisions on bookmark"
    );
}

struct SyncFixture {
    root: PathBuf,
    local: PathBuf,
    peer: PathBuf,
}

struct DualRemoteSyncFixture {
    root: PathBuf,
    local: PathBuf,
    peer: PathBuf,
}

impl DualRemoteSyncFixture {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-{prefix}-{unique}"));
        let local = root.join("local");
        let peer = root.join("peer");
        let origin_remote = root.join("origin-remote");
        let upstream_remote = root.join("upstream-remote");
        fs::create_dir_all(&local).expect("local repo directory should be created");
        fs::create_dir_all(&peer).expect("peer repo directory should be created");
        fs::create_dir_all(&origin_remote).expect("origin remote directory should be created");
        fs::create_dir_all(&upstream_remote).expect("upstream remote directory should be created");

        run_jj(&origin_remote, &["git", "init", "--colocate"]);
        run_jj(&upstream_remote, &["git", "init", "--colocate"]);
        run_jj(&local, &["git", "init", "--colocate"]);
        run_jj(&peer, &["git", "init", "--colocate"]);

        run_jj(
            &local,
            &["config", "set", "--repo", "user.name", "Hunk Test User"],
        );
        run_jj(
            &local,
            &[
                "config",
                "set",
                "--repo",
                "user.email",
                "hunk-tests@example.com",
            ],
        );
        run_jj(
            &peer,
            &["config", "set", "--repo", "user.name", "Hunk Test User"],
        );
        run_jj(
            &peer,
            &[
                "config",
                "set",
                "--repo",
                "user.email",
                "hunk-tests@example.com",
            ],
        );

        let origin_path = origin_remote.to_string_lossy().to_string();
        let upstream_path = upstream_remote.to_string_lossy().to_string();
        run_jj(
            &local,
            &["git", "remote", "add", "origin", origin_path.as_str()],
        );
        run_jj(
            &local,
            &["git", "remote", "add", "upstream", upstream_path.as_str()],
        );
        run_jj(
            &peer,
            &["git", "remote", "add", "upstream", upstream_path.as_str()],
        );

        Self { root, local, peer }
    }

    fn local_path(&self) -> &Path {
        &self.local
    }

    fn peer_path(&self) -> &Path {
        &self.peer
    }
}

impl SyncFixture {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-{prefix}-{unique}"));
        let local = root.join("local");
        let peer = root.join("peer");
        let remote = root.join("remote");
        fs::create_dir_all(&local).expect("local repo directory should be created");
        fs::create_dir_all(&peer).expect("peer repo directory should be created");
        fs::create_dir_all(&remote).expect("remote repo directory should be created");

        run_jj(&remote, &["git", "init", "--colocate"]);
        run_jj(&local, &["git", "init", "--colocate"]);
        run_jj(&peer, &["git", "init", "--colocate"]);

        run_jj(
            &local,
            &["config", "set", "--repo", "user.name", "Hunk Test User"],
        );
        run_jj(
            &local,
            &[
                "config",
                "set",
                "--repo",
                "user.email",
                "hunk-tests@example.com",
            ],
        );
        run_jj(
            &peer,
            &["config", "set", "--repo", "user.name", "Hunk Test User"],
        );
        run_jj(
            &peer,
            &[
                "config",
                "set",
                "--repo",
                "user.email",
                "hunk-tests@example.com",
            ],
        );

        let remote_path = remote.to_string_lossy().to_string();
        run_jj(
            &local,
            &["git", "remote", "add", "origin", remote_path.as_str()],
        );
        run_jj(
            &peer,
            &["git", "remote", "add", "origin", remote_path.as_str()],
        );

        Self { root, local, peer }
    }

    fn local_path(&self) -> &Path {
        &self.local
    }

    fn peer_path(&self) -> &Path {
        &self.peer
    }
}

impl Drop for SyncFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Drop for DualRemoteSyncFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
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
