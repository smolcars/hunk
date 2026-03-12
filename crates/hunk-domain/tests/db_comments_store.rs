use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_domain::db::{CommentLineSide, CommentStatus, DatabaseStore, NewComment};
use rusqlite::Connection;

const MIGRATION_0001_INIT: &str = include_str!("../src/db/migrations/0001_init.sql");

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

fn new_comment(repo_root: &str, branch_name: &str, file_path: &str, text: &str) -> NewComment {
    NewComment {
        repo_root: repo_root.to_string(),
        branch_name: branch_name.to_string(),
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
    assert_eq!(user_version, 3);
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
    assert_eq!(created.branch_name, "main");
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
fn scope_filtering_is_repo_and_branch_specific() {
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
fn batch_comment_updates_apply_to_each_requested_id() {
    let fixture = TempDb::new("comments-batch-updates");
    let first = fixture
        .store
        .create_comment(&new_comment("/repo", "main", "src/one.rs", "first"))
        .expect("create first comment");
    let second = fixture
        .store
        .create_comment(&new_comment("/repo", "main", "src/two.rs", "second"))
        .expect("create second comment");
    let third = fixture
        .store
        .create_comment(&new_comment("/repo", "main", "src/three.rs", "third"))
        .expect("create third comment");

    let touched = fixture
        .store
        .touch_many_comment_seen(&[first.id.clone(), second.id.clone()], 2222)
        .expect("touch many comments");
    assert_eq!(touched, 2);

    let marked = fixture
        .store
        .mark_many_comment_status(
            &[first.id.clone(), second.id.clone()],
            CommentStatus::Stale,
            Some("anchor_not_found"),
            3333,
        )
        .expect("mark many comments stale");
    assert_eq!(marked, 2);

    let deleted = fixture
        .store
        .delete_many_comments(std::slice::from_ref(&third.id))
        .expect("delete many comments");
    assert_eq!(deleted, 1);

    let comments = fixture
        .store
        .list_comments("/repo", "main", true)
        .expect("list comments after batch updates");
    assert_eq!(comments.len(), 2);
    assert!(
        comments
            .iter()
            .all(|comment| comment.status == CommentStatus::Stale)
    );
    assert!(
        comments
            .iter()
            .all(|comment| comment.last_seen_at_unix_ms == Some(2222))
    );
    assert!(comments.iter().all(|comment| {
        comment.stale_reason.as_deref() == Some("anchor_not_found")
            && comment.updated_at_unix_ms == 3333
    }));
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

#[test]
fn create_comment_drops_row_stable_id_larger_than_i64_max() {
    let fixture = TempDb::new("comments-row-stable-id-overflow");
    let mut comment = new_comment("/repo", "main", "src/lib.rs", "overflow");
    comment.row_stable_id = Some(i64::MAX as u64 + 1);

    let created = fixture
        .store
        .create_comment(&comment)
        .expect("oversized row_stable_id should not block comment creation");

    assert_eq!(created.row_stable_id, None);
}

#[test]
fn sqlite_schema_rejects_negative_row_stable_id() {
    let fixture = TempDb::new("comments-row-stable-id-negative");
    fixture
        .store
        .list_comments("/repo", "main", true)
        .expect("listing comments should initialize db");

    let conn = Connection::open(&fixture.path).expect("open sqlite db");
    let err = conn
        .execute(
            "INSERT INTO comments (
                id,
                repo_root,
                branch_name,
                created_head_commit,
                status,
                file_path,
                line_side,
                old_line,
                new_line,
                row_stable_id,
                hunk_header,
                line_text,
                context_before,
                context_after,
                anchor_hash,
                comment_text,
                stale_reason,
                created_at_unix_ms,
                updated_at_unix_ms,
                last_seen_at_unix_ms,
                resolved_at_unix_ms
            ) VALUES (
                'comment-negative-row-stable-id',
                '/repo',
                'main',
                'abc123',
                'open',
                'src/lib.rs',
                'right',
                10,
                11,
                -1,
                '@@ -10,3 +11,4 @@',
                'let value = 1;',
                ' let other = 0;',
                '+let value = 1;',
                'anchor-hash-negative',
                'negative row stable id',
                NULL,
                1,
                1,
                1,
                NULL
            )",
            [],
        )
        .expect_err("negative row_stable_id should violate the schema constraint");

    assert!(err.to_string().contains("row_stable_id"));
}

#[test]
fn migration_sanitizes_legacy_negative_row_stable_ids() {
    let fixture = TempDb::new("comments-row-stable-id-legacy-negative");
    let conn = Connection::open(&fixture.path).expect("open sqlite db");
    conn.execute_batch(
        "CREATE TABLE comments (
            id TEXT PRIMARY KEY,
            repo_root TEXT NOT NULL,
            branch_name TEXT NOT NULL,
            created_head_commit TEXT,
            status TEXT NOT NULL,
            file_path TEXT NOT NULL,
            line_side TEXT NOT NULL,
            old_line INTEGER,
            new_line INTEGER,
            row_stable_id INTEGER,
            hunk_header TEXT,
            line_text TEXT NOT NULL,
            context_before TEXT NOT NULL,
            context_after TEXT NOT NULL,
            anchor_hash TEXT NOT NULL,
            comment_text TEXT NOT NULL,
            stale_reason TEXT,
            created_at_unix_ms INTEGER NOT NULL,
            updated_at_unix_ms INTEGER NOT NULL,
            last_seen_at_unix_ms INTEGER,
            resolved_at_unix_ms INTEGER
        );
        CREATE INDEX comments_repo_branch_status_idx
          ON comments (repo_root, branch_name, status);",
    )
    .expect("create legacy comments table without row_stable_id check");
    conn.pragma_update(None, "user_version", 2_i64)
        .expect("set sqlite user_version to 2");
    conn.execute(
        "INSERT INTO comments (
            id,
            repo_root,
            branch_name,
            created_head_commit,
            status,
            file_path,
            line_side,
            old_line,
            new_line,
            row_stable_id,
            hunk_header,
            line_text,
            context_before,
            context_after,
            anchor_hash,
            comment_text,
            stale_reason,
            created_at_unix_ms,
            updated_at_unix_ms,
            last_seen_at_unix_ms,
            resolved_at_unix_ms
        ) VALUES (
            'comment-legacy-negative-row-stable-id',
            '/repo',
            'main',
            'abc123',
            'open',
            'src/lib.rs',
            'right',
            10,
            11,
            -1040741354035379776,
            '@@ -10,3 +11,4 @@',
            'let value = 1;',
            ' let other = 0;',
            '+let value = 1;',
            'anchor-hash-legacy-negative',
            'legacy negative row stable id',
            NULL,
            1,
            1,
            1,
            NULL
        )",
        [],
    )
    .expect("insert legacy negative row_stable_id comment");
    drop(conn);

    let comments = fixture
        .store
        .list_comments("/repo", "main", true)
        .expect("load and sanitize legacy negative row_stable_id");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].row_stable_id, None);

    let conn = Connection::open(&fixture.path).expect("reopen migrated sqlite db");
    let stored_row_stable_id: Option<i64> = conn
        .query_row(
            "SELECT row_stable_id FROM comments WHERE id = 'comment-legacy-negative-row-stable-id'",
            [],
            |row| row.get(0),
        )
        .expect("read sanitized row_stable_id");
    assert_eq!(stored_row_stable_id, None);
    let user_version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read sanitized sqlite user_version");
    assert_eq!(user_version, 3);
}

#[test]
fn upgrading_a_version_1_database_runs_ordered_migrations() {
    let fixture = TempDb::new("comments-version-1-upgrade");

    let conn = Connection::open(&fixture.path).expect("open sqlite db");
    conn.execute_batch(MIGRATION_0001_INIT)
        .expect("apply version 1 schema");
    conn.pragma_update(None, "user_version", 1_i64)
        .expect("set sqlite user_version to 1");
    conn.execute(
        "INSERT INTO comments (
            id,
            repo_root,
            branch_name,
            created_head_commit,
            status,
            file_path,
            line_side,
            old_line,
            new_line,
            row_stable_id,
            hunk_header,
            line_text,
            context_before,
            context_after,
            anchor_hash,
            comment_text,
            stale_reason,
            created_at_unix_ms,
            updated_at_unix_ms,
            last_seen_at_unix_ms,
            resolved_at_unix_ms
        ) VALUES (
            'comment-version-1',
            '/repo',
            'main',
            'abc123',
            'open',
            'src/lib.rs',
            'right',
            10,
            11,
            42,
            '@@ -10,3 +11,4 @@',
            'let value = 1;',
            ' let other = 0;',
            '+let value = 1;',
            'anchor-hash-version-1',
            'legacy comment',
            NULL,
            1,
            1,
            1,
            NULL
        )",
        [],
    )
    .expect("insert legacy version 1 comment");
    drop(conn);

    let comments = fixture
        .store
        .list_comments("/repo", "main", true)
        .expect("upgrade version 1 database");
    assert!(comments.is_empty());

    let conn = Connection::open(&fixture.path).expect("reopen upgraded sqlite db");
    let user_version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read upgraded sqlite user_version");
    assert_eq!(user_version, 3);
}
