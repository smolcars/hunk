CREATE TABLE IF NOT EXISTS comments (
  id TEXT PRIMARY KEY,
  repo_root TEXT NOT NULL,
  bookmark_name TEXT NOT NULL,
  created_head_commit TEXT,

  status TEXT NOT NULL CHECK (status IN ('open', 'stale', 'resolved')),

  file_path TEXT NOT NULL,
  line_side TEXT NOT NULL CHECK (line_side IN ('left', 'right', 'meta')),
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

CREATE INDEX IF NOT EXISTS comments_repo_bookmark_status_idx
  ON comments (repo_root, bookmark_name, status);

CREATE INDEX IF NOT EXISTS comments_repo_file_idx
  ON comments (repo_root, file_path);

CREATE INDEX IF NOT EXISTS comments_status_updated_idx
  ON comments (status, updated_at_unix_ms);
