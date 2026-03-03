use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_domain::db::{CommentLineSide, CommentStatus, DatabaseStore, NewComment};
use rusqlite::Connection;

struct TempDb {
    path: PathBuf,
    store: DatabaseStore,
}

impl TempDb {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("hunk-{prefix}-{}-{unique}.db", std::process::id()));
        Self {
            store: DatabaseStore::from_path(path.clone()),
            path,
        }
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(self.path.with_extension("db-shm"));
        let _ = fs::remove_file(self.path.with_extension("db-wal"));
    }
}

fn new_comment(repo_root: &str, bookmark_name: &str, file_path: &str, text: &str) -> NewComment {
    NewComment {
        repo_root: repo_root.to_string(),
        bookmark_name: bookmark_name.to_string(),
        created_head_commit: Some("abc123".to_string()),
        file_path: file_path.to_string(),
        line_side: CommentLineSide::Right,
        old_line: Some(10),
        new_line: Some(11),
        row_stable_id: Some(42),
        hunk_header: Some("@@ -10,3 +11,4 @@".to_string()),
        line_text: "let value = 1;".to_string(),
        context_before: " let other = 0;".to_string(),
        context_after: "+let value = 1;".to_string(),
        anchor_hash: "anchor-hash-1".to_string(),
        comment_text: text.to_string(),
    }
}

#[test]
fn sqlite_store_bootstraps_schema() {
    let fixture = TempDb::new("comments-schema");

    let comments = fixture
        .store
        .list_comments("/repo", "main", true)
        .expect("listing comments should initialize db");
    assert!(comments.is_empty());
    assert!(fixture.path.exists());

    let conn = Connection::open(&fixture.path).expect("open sqlite db");
    let user_version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read sqlite user_version");
    assert_eq!(user_version, 1);
}

#[test]
fn create_and_list_comment_round_trip() {
    let fixture = TempDb::new("comments-create-list");

    let created = fixture
        .store
        .create_comment(&new_comment("/repo", "main", "src/lib.rs", "fix this"))
        .expect("create comment");

    assert_eq!(created.status, CommentStatus::Open);
    assert_eq!(created.repo_root, "/repo");
    assert_eq!(created.bookmark_name, "main");
    assert_eq!(created.file_path, "src/lib.rs");
    assert_eq!(created.old_line, Some(10));
    assert_eq!(created.new_line, Some(11));
    assert_eq!(created.row_stable_id, Some(42));

    let listed = fixture
        .store
        .list_comments("/repo", "main", false)
        .expect("list open comments");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
}

#[test]
fn scope_filtering_is_repo_and_bookmark_specific() {
    let fixture = TempDb::new("comments-scope");

    fixture
        .store
        .create_comment(&new_comment("/repo", "main", "src/a.rs", "comment a"))
        .expect("create main comment");
    fixture
        .store
        .create_comment(&new_comment("/repo", "feature", "src/b.rs", "comment b"))
        .expect("create feature comment");
    fixture
        .store
        .create_comment(&new_comment("/other-repo", "main", "src/c.rs", "comment c"))
        .expect("create other repo comment");

    let main_scope = fixture
        .store
        .list_comments("/repo", "main", true)
        .expect("list main scope");
    assert_eq!(main_scope.len(), 1);
    assert_eq!(main_scope[0].comment_text, "comment a");

    let feature_scope = fixture
        .store
        .list_comments("/repo", "feature", true)
        .expect("list feature scope");
    assert_eq!(feature_scope.len(), 1);
    assert_eq!(feature_scope[0].comment_text, "comment b");
}

#[test]
fn status_updates_and_pruning_work() {
    let fixture = TempDb::new("comments-status-prune");

    let created = fixture
        .store
        .create_comment(&new_comment("/repo", "main", "src/lib.rs", "stale me"))
        .expect("create comment");

    let updated = fixture
        .store
        .mark_comment_status(
            created.id.as_str(),
            CommentStatus::Stale,
            Some("anchor_not_found"),
            100,
        )
        .expect("mark stale");
    assert!(updated);

    let open_only = fixture
        .store
        .list_comments("/repo", "main", false)
        .expect("list open only");
    assert!(open_only.is_empty());

    let with_non_open = fixture
        .store
        .list_comments("/repo", "main", true)
        .expect("list all statuses");
    assert_eq!(with_non_open.len(), 1);
    assert_eq!(with_non_open[0].status, CommentStatus::Stale);
    assert_eq!(
        with_non_open[0].stale_reason.as_deref(),
        Some("anchor_not_found")
    );

    let removed = fixture
        .store
        .prune_non_open_comments(200)
        .expect("prune stale comments");
    assert_eq!(removed, 1);

    let after_prune = fixture
        .store
        .list_comments("/repo", "main", true)
        .expect("list comments after prune");
    assert!(after_prune.is_empty());
}

#[test]
fn touch_and_delete_comment_work() {
    let fixture = TempDb::new("comments-touch-delete");

    let created = fixture
        .store
        .create_comment(&new_comment("/repo", "main", "src/lib.rs", "delete me"))
        .expect("create comment");

    let touched = fixture
        .store
        .touch_comment_seen(created.id.as_str(), 1234)
        .expect("touch last seen");
    assert!(touched);

    let loaded = fixture
        .store
        .get_comment(created.id.as_str())
        .expect("load comment by id")
        .expect("comment should exist");
    assert_eq!(loaded.last_seen_at_unix_ms, Some(1234));

    let deleted = fixture
        .store
        .delete_comment(created.id.as_str())
        .expect("delete comment");
    assert!(deleted);

    let missing = fixture
        .store
        .get_comment(created.id.as_str())
        .expect("load missing comment");
    assert!(missing.is_none());
}

#[test]
fn create_comment_ids_are_unique_within_process() {
    let fixture = TempDb::new("comments-id-unique");
    let mut ids = HashSet::new();

    for ix in 0..256 {
        let created = fixture
            .store
            .create_comment(&new_comment(
                "/repo",
                "main",
                "src/lib.rs",
                format!("comment-{ix}").as_str(),
            ))
            .expect("create comment");
        assert!(
            ids.insert(created.id),
            "duplicate comment id should never be generated"
        );
    }
}
