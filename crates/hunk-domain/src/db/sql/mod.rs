pub(crate) mod comments {
    pub(crate) const INSERT: &str = r#"
INSERT INTO comments (
  id,
  repo_root,
  bookmark_name,
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
  ?1,
  ?2,
  ?3,
  ?4,
  ?5,
  ?6,
  ?7,
  ?8,
  ?9,
  ?10,
  ?11,
  ?12,
  ?13,
  ?14,
  ?15,
  ?16,
  NULL,
  ?17,
  ?18,
  ?19,
  NULL
);
"#;

    pub(crate) const SELECT_BY_ID: &str = r#"
SELECT
  id,
  repo_root,
  bookmark_name,
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
FROM comments
WHERE id = ?1;
"#;

    pub(crate) const SELECT_BY_SCOPE: &str = r#"
SELECT
  id,
  repo_root,
  bookmark_name,
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
FROM comments
WHERE
  repo_root = ?1
  AND bookmark_name = ?2
  AND (?3 = 1 OR status = 'open')
ORDER BY updated_at_unix_ms DESC, created_at_unix_ms DESC, id DESC;
"#;

    pub(crate) const UPDATE_STATUS: &str = r#"
UPDATE comments
SET
  status = ?2,
  stale_reason = CASE
    WHEN ?2 = 'stale' THEN ?3
    ELSE NULL
  END,
  updated_at_unix_ms = ?4,
  resolved_at_unix_ms = CASE
    WHEN ?2 = 'resolved' THEN ?4
    WHEN ?2 = 'open' THEN NULL
    ELSE resolved_at_unix_ms
  END
WHERE id = ?1;
"#;

    pub(crate) const TOUCH_SEEN: &str = r#"
UPDATE comments
SET last_seen_at_unix_ms = ?2
WHERE id = ?1;
"#;

    pub(crate) const DELETE_BY_ID: &str = r#"
DELETE FROM comments
WHERE id = ?1;
"#;

    pub(crate) const PRUNE_NON_OPEN: &str = r#"
DELETE FROM comments
WHERE
  status IN ('stale', 'resolved')
  AND COALESCE(resolved_at_unix_ms, updated_at_unix_ms) < ?1;
"#;
}

pub(crate) mod connection {
    pub(crate) const SETUP: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
PRAGMA busy_timeout = 5000;
"#;
}
