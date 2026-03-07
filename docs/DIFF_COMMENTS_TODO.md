# Diff Comments TODO (SQLite)

## Goal

Add line-level comments in the diff view so reviewers can:

- add comments from hovered diff rows,
- see accumulated comments in a top counter/pile-up entry,
- preview comments in a popover-like panel,
- copy comment bundles that include nearby diff context for pasting into coding agents.

## Finalized Decisions

- Storage engine: shared SQLite app database (`hunk.db`), not TOML.
- Storage location: global app config dir (`~/.hunkdiff/hunk.db`), not repo-local metadata.
- App config: keep `~/.hunkdiff/config.toml` as TOML (no migration to SQLite now).
- Database strategy: one DB for future features; comments are one table namespace inside it.
- SQL organization: keep schema migrations as `.sql` files; keep runtime query SQL in Rust constants.
- Scope model:
  - comments are always tied to `repo_root`,
  - comments are additionally scoped to `branch_name` by default in UI,
  - optional cross-branch views are follow-up work.
- Lifecycle:
  - `open`: active and shown in the main counter.
  - `stale`: anchor no longer maps cleanly after diff changes.
  - `resolved`: anchor disappears because code changed/cleaned up.
- Retention:
  - do not hard-delete on commit,
  - prune `resolved` and `stale` comments older than retention window (default 14 days),
  - keep `open` comments indefinitely until manual resolve/delete.

## Data Model

Use a shared app DB with a `comments` table in Phase 1.

```sql
CREATE TABLE IF NOT EXISTS comments (
  id TEXT PRIMARY KEY,
  repo_root TEXT NOT NULL,
  branch_name TEXT NOT NULL,
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

CREATE INDEX IF NOT EXISTS comments_repo_branch_status_idx
  ON comments(repo_root, branch_name, status);
CREATE INDEX IF NOT EXISTS comments_repo_file_idx
  ON comments(repo_root, file_path);
CREATE INDEX IF NOT EXISTS comments_status_updated_idx
  ON comments(status, updated_at_unix_ms);
```

Notes:

- `anchor_hash` = normalized hash of `{file_path, hunk_header, line_text, context_before, context_after}`.
- `row_stable_id` is a helper anchor only; it is not enough by itself across diff rebuilds.
- `created_head_commit` is for future diagnostics and filtering; Phase 1 does not gate behavior on commit graph analysis.
- Future tables should be feature-scoped (`comments`, `annotations`, `exports`, etc.) and share the same DB migration versioning.

## Runtime Behavior

### Add Comment

1. User hovers a diff row (code/meta row only).
2. Note icon appears near that row.
3. Clicking note icon opens compact inline editor.
4. On save:
   - build anchor payload from current diff row + nearby context (`N=2` lines default),
   - insert `open` row into SQLite,
   - refresh in-memory comment list and toolbar counter.

### Counter + Preview

- Toolbar shows comment counter badge for `open` comments in current scope (`repo_root + branch_name`).
- Clicking badge opens preview panel listing comments (newest first).
- Each preview item includes:
  - file path + line hint,
  - status chip,
  - comment text,
  - copy action.
- Panel includes:
  - `Copy All Open` action,
  - optional `Show stale/resolved` toggle (if not in Phase 1, hide these by default).

### Copy Payload Format

Copy action emits structured plain text:

```text
[Hunk Comment]
File: <file_path>
Lines: old <old_line or -> | new <new_line or ->
Comment:
<comment_text>

Snippet:
<last relevant line before>
<anchored line>
<first relevant line after>
```

`Copy All Open` concatenates multiple blocks separated by `\n\n---\n\n`.

### Refresh + Remap

When diff stream is rebuilt:

1. Load scoped comments (`repo_root + branch_name`).
2. For each `open` comment:
   - exact match attempt: file + old/new line and side,
   - fallback: `anchor_hash` against regenerated row snippets in same file.
3. If matched: keep `open`, update `last_seen_at_unix_ms`.
4. If not matched:
   - mark `resolved` if file is no longer changed in current working copy,
   - otherwise mark `stale` with reason (`anchor_not_found`).

## Code Layout Plan

- Implemented DB module: `src/db/`
- SQL files: `src/db/sql/**`
- Migrations: `src/db/migrations/**`
- App integration:
  - `src/app/controller/comments.rs`
  - `src/app/render/comments.rs`

Integration points:

- `src/app.rs`
  - add `comments_store`, `comments_cache`, `hovered_row`, `active_comment_editor_row`,
    `comments_preview_open` fields.
- `src/app/controller/core.rs`
  - initialize/store lifecycle,
  - trigger remap after `apply_loaded_diff_stream`.
- `src/app/render/diff.rs`
  - hover note icon rendering + row inline editor.
- `src/app/render/toolbar.rs`
  - counter badge and preview trigger.
- `src/app/controller/selection.rs` (or new controller file)
  - clipboard actions for one/all comment bundles.
- `src/state.rs`
  - optional UI-only persistence fields (e.g., preview panel default visibility), not comment data.

## Phase Plan

## Phase 1: MVP (ship first usable version)

- [x] Add SQLite dependency and DB bootstrap (`~/.hunkdiff/hunk.db`).
- [x] Implement `comments` schema + migration versioning table (`PRAGMA user_version` is fine).
- [x] Add CRUD APIs:
  - [x] create comment,
  - [x] list comments by scope,
  - [x] update status,
  - [x] delete comment,
  - [x] prune stale/resolved by retention age.
- [x] Add in-memory cache in `DiffViewer`.
- [x] Add hover affordance in diff rows:
  - [x] icon visible on hover,
  - [x] inline add-comment editor,
  - [x] save/cancel.
- [x] Add toolbar pile-up counter and preview panel.
- [x] Add copy behavior:
  - [x] copy single comment bundle,
  - [x] copy all open comments bundle.
- [x] Add basic remap at diff refresh:
  - [x] exact line match first,
  - [x] hash fallback,
  - [x] stale/resolved transitions.
- [ ] Add tests under `tests/`:
  - [x] DB migration/create/list/update/delete,
  - [x] retention prune,
  - [x] anchor hash determinism,
  - [x] copy bundle formatting,
  - [x] remap status transition logic.
- [x] Run validation:
  - [x] `cargo test` (targeted `db_comments_store`),
  - [x] `cargo clippy --all-targets --all-features -- -D warnings`,
  - [x] `cargo build`.

### Phase 1 Acceptance Criteria

- User can add comments from hovered diff rows.
- Open comment counter updates immediately.
- User can preview and copy one or all comments with diff context.
- Comments persist across app restart.
- Switching branches shows only scoped comments for that branch.
- After diff refresh, comments become `open`, `stale`, or `resolved` deterministically.

## Phase 2: UX and Robustness

- [x] Add direct navigation from preview item to row.
- [x] Auto-show non-open comments when open count is zero.
- [x] Add per-comment `Reopen` action in preview.
- [x] Require repeated misses before auto-transitioning open comments (`threshold=2`).
- [x] Keep preview panel closed after saving a new inline comment.
- [x] Render compact inline comment editor near the row affordance instead of full-width.
- [x] Make preview panel content fully contained and scrollable for long comment lists.
- [x] Add stronger fuzzy remap heuristics for moved/edited hunks.
- [ ] Add filters: `open/stale/resolved`, file path, and search text.
- [x] Add manual bulk actions: resolve all stale, delete resolved, reopen stale.
- [ ] Add configurable retention in `config.toml`.

## Phase 3: Advanced Work

- [ ] Optional cross-branch dashboard for same repo.
- [ ] Optional export template presets for different coding agents.
- [ ] Optional attachment support (code block snippets, tags, severity).

## Open Questions

- [ ] Should branch scope use current active branch only, or include detached states separately?
- [ ] Should comment create/edit support markdown in Phase 1, or plain text only?
- [ ] Should `meta` rows be commentable in Phase 1, or only code rows?
- [ ] Should `Copy All Open` include stale comments behind a toggle?
