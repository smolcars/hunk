use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use rusqlite::{OptionalExtension as _, params};

use super::connection::DatabaseStore;
use super::sql;

static COMMENT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStatus {
    Open,
    Stale,
    Resolved,
}

impl CommentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Stale => "stale",
            Self::Resolved => "resolved",
        }
    }

    fn from_db(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "stale" => Some(Self::Stale),
            "resolved" => Some(Self::Resolved),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentLineSide {
    Left,
    Right,
    Meta,
}

impl CommentLineSide {
    fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Meta => "meta",
        }
    }

    fn from_db(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "meta" => Some(Self::Meta),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewComment {
    pub repo_root: String,
    pub bookmark_name: String,
    pub created_head_commit: Option<String>,
    pub file_path: String,
    pub line_side: CommentLineSide,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub row_stable_id: Option<u64>,
    pub hunk_header: Option<String>,
    pub line_text: String,
    pub context_before: String,
    pub context_after: String,
    pub anchor_hash: String,
    pub comment_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentRecord {
    pub id: String,
    pub repo_root: String,
    pub bookmark_name: String,
    pub created_head_commit: Option<String>,
    pub status: CommentStatus,
    pub file_path: String,
    pub line_side: CommentLineSide,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub row_stable_id: Option<u64>,
    pub hunk_header: Option<String>,
    pub line_text: String,
    pub context_before: String,
    pub context_after: String,
    pub anchor_hash: String,
    pub comment_text: String,
    pub stale_reason: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub last_seen_at_unix_ms: Option<i64>,
    pub resolved_at_unix_ms: Option<i64>,
}

pub fn now_unix_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    duration
        .as_millis()
        .min(i64::MAX as u128)
        .try_into()
        .unwrap_or(i64::MAX)
}

pub fn comment_status_label(status: CommentStatus) -> &'static str {
    match status {
        CommentStatus::Open => "open",
        CommentStatus::Stale => "stale",
        CommentStatus::Resolved => "resolved",
    }
}

pub fn next_status_for_unmatched_anchor(
    file_is_changed: bool,
) -> (CommentStatus, Option<&'static str>) {
    if file_is_changed {
        (CommentStatus::Stale, Some("anchor_not_found"))
    } else {
        (CommentStatus::Resolved, None)
    }
}

pub fn compute_comment_anchor_hash(
    file_path: &str,
    hunk_header: Option<&str>,
    line_text: &str,
    context_before: &str,
    context_after: &str,
) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    hash = fnv1a64_update(hash, b"file:");
    hash = fnv1a64_update(hash, file_path.as_bytes());
    hash = fnv1a64_update(hash, b"\nheader:");
    hash = fnv1a64_update(hash, hunk_header.unwrap_or("").as_bytes());
    hash = fnv1a64_update(hash, b"\nline:");
    hash = fnv1a64_update(hash, line_text.as_bytes());
    hash = fnv1a64_update(hash, b"\nbefore:");
    hash = fnv1a64_update(hash, context_before.as_bytes());
    hash = fnv1a64_update(hash, b"\nafter:");
    hash = fnv1a64_update(hash, context_after.as_bytes());
    format!("{hash:016x}")
}

pub fn format_comment_clipboard_blob(comment: &CommentRecord) -> String {
    let old_line = comment
        .old_line
        .map(|line| line.to_string())
        .unwrap_or_else(|| "-".to_string());
    let new_line = comment
        .new_line
        .map(|line| line.to_string())
        .unwrap_or_else(|| "-".to_string());

    let mut snippet_lines = Vec::new();
    if let Some(before) = comment
        .context_before
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
    {
        snippet_lines.push(before.to_string());
    }

    snippet_lines.extend(
        comment
            .line_text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToString::to_string),
    );

    if let Some(after) = comment
        .context_after
        .lines()
        .find(|line| !line.trim().is_empty())
    {
        snippet_lines.push(after.to_string());
    }

    let snippet = if snippet_lines.is_empty() {
        "(no diff context captured)".to_string()
    } else {
        snippet_lines.join("\n")
    };

    format!(
        "[Hunk Comment]\nFile: {}\nLines: old {} | new {}\nComment:\n{}\nSnippet:\n{}",
        comment.file_path, old_line, new_line, comment.comment_text, snippet,
    )
}

impl DatabaseStore {
    pub fn create_comment(&self, input: &NewComment) -> Result<CommentRecord> {
        let id = next_comment_id();
        let now = now_unix_ms();

        let conn = self.open_connection()?;
        conn.execute(
            sql::comments::INSERT,
            params![
                id,
                input.repo_root,
                input.bookmark_name,
                input.created_head_commit,
                CommentStatus::Open.as_str(),
                input.file_path,
                input.line_side.as_str(),
                input.old_line.map(i64::from),
                input.new_line.map(i64::from),
                input.row_stable_id.map(|value| value as i64),
                input.hunk_header,
                input.line_text,
                input.context_before,
                input.context_after,
                input.anchor_hash,
                input.comment_text,
                now,
                now,
                now,
            ],
        )
        .context("failed to insert comment")?;

        self.get_comment(&id)?
            .ok_or_else(|| anyhow!("inserted comment id {id} was not found"))
    }

    pub fn get_comment(&self, id: &str) -> Result<Option<CommentRecord>> {
        let conn = self.open_connection()?;
        let mut stmt = conn
            .prepare(sql::comments::SELECT_BY_ID)
            .context("failed to prepare select comment by id query")?;

        stmt.query_row(params![id], map_comment_row)
            .optional()
            .context("failed to query comment by id")
    }

    pub fn list_comments(
        &self,
        repo_root: &str,
        bookmark_name: &str,
        include_non_open: bool,
    ) -> Result<Vec<CommentRecord>> {
        let conn = self.open_connection()?;
        let mut stmt = conn
            .prepare(sql::comments::SELECT_BY_SCOPE)
            .context("failed to prepare select comments by scope query")?;

        let include_non_open_flag = if include_non_open { 1_i64 } else { 0_i64 };
        let rows = stmt
            .query_map(
                params![repo_root, bookmark_name, include_non_open_flag],
                map_comment_row,
            )
            .context("failed to query comments by scope")?;

        let mut comments = Vec::new();
        for row in rows {
            comments.push(row?);
        }
        Ok(comments)
    }

    pub fn mark_comment_status(
        &self,
        id: &str,
        status: CommentStatus,
        stale_reason: Option<&str>,
        updated_at_unix_ms: i64,
    ) -> Result<bool> {
        let stale_reason_value = match status {
            CommentStatus::Stale => stale_reason,
            _ => None,
        };

        let conn = self.open_connection()?;
        let rows_updated = conn
            .execute(
                sql::comments::UPDATE_STATUS,
                params![id, status.as_str(), stale_reason_value, updated_at_unix_ms,],
            )
            .with_context(|| format!("failed to update status for comment {id}"))?;
        Ok(rows_updated > 0)
    }

    pub fn mark_many_comment_status(
        &self,
        ids: &[String],
        status: CommentStatus,
        stale_reason: Option<&str>,
        updated_at_unix_ms: i64,
    ) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let stale_reason_value = match status {
            CommentStatus::Stale => stale_reason,
            _ => None,
        };

        let mut conn = self.open_connection()?;
        let tx = conn
            .transaction()
            .context("failed to start sqlite transaction for status batch update")?;
        let mut stmt = tx
            .prepare(sql::comments::UPDATE_STATUS)
            .context("failed to prepare status batch update statement")?;
        let mut updated = 0usize;
        for id in ids {
            updated += stmt
                .execute(params![
                    id,
                    status.as_str(),
                    stale_reason_value,
                    updated_at_unix_ms,
                ])
                .with_context(|| format!("failed to batch update status for comment {id}"))?;
        }
        drop(stmt);
        tx.commit()
            .context("failed to commit status batch update transaction")?;
        Ok(updated)
    }

    pub fn touch_comment_seen(&self, id: &str, seen_at_unix_ms: i64) -> Result<bool> {
        let conn = self.open_connection()?;
        let rows_updated = conn
            .execute(sql::comments::TOUCH_SEEN, params![id, seen_at_unix_ms])
            .with_context(|| format!("failed to update last_seen for comment {id}"))?;
        Ok(rows_updated > 0)
    }

    pub fn touch_many_comment_seen(&self, ids: &[String], seen_at_unix_ms: i64) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut conn = self.open_connection()?;
        let tx = conn
            .transaction()
            .context("failed to start sqlite transaction for last_seen batch update")?;
        let mut stmt = tx
            .prepare(sql::comments::TOUCH_SEEN)
            .context("failed to prepare last_seen batch update statement")?;
        let mut updated = 0usize;
        for id in ids {
            updated += stmt
                .execute(params![id, seen_at_unix_ms])
                .with_context(|| format!("failed to batch update last_seen for comment {id}"))?;
        }
        drop(stmt);
        tx.commit()
            .context("failed to commit last_seen batch update transaction")?;
        Ok(updated)
    }

    pub fn delete_comment(&self, id: &str) -> Result<bool> {
        let conn = self.open_connection()?;
        let rows_deleted = conn
            .execute(sql::comments::DELETE_BY_ID, params![id])
            .with_context(|| format!("failed to delete comment {id}"))?;
        Ok(rows_deleted > 0)
    }

    pub fn delete_many_comments(&self, ids: &[String]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut conn = self.open_connection()?;
        let tx = conn
            .transaction()
            .context("failed to start sqlite transaction for comment batch delete")?;
        let mut stmt = tx
            .prepare(sql::comments::DELETE_BY_ID)
            .context("failed to prepare comment batch delete statement")?;
        let mut deleted = 0usize;
        for id in ids {
            deleted += stmt
                .execute(params![id])
                .with_context(|| format!("failed to batch delete comment {id}"))?;
        }
        drop(stmt);
        tx.commit()
            .context("failed to commit comment batch delete transaction")?;
        Ok(deleted)
    }

    pub fn prune_non_open_comments(&self, cutoff_unix_ms: i64) -> Result<usize> {
        let conn = self.open_connection()?;
        conn.execute(sql::comments::PRUNE_NON_OPEN, params![cutoff_unix_ms])
            .context("failed to prune stale/resolved comments")
    }
}

fn fnv1a64_update(mut hash: u64, bytes: &[u8]) -> u64 {
    const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn next_comment_id() -> String {
    let counter = COMMENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    format!("comment-{now_nanos:032x}-{pid:08x}-{counter:016x}")
}

fn map_comment_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommentRecord> {
    let status_raw: String = row.get("status")?;
    let line_side_raw: String = row.get("line_side")?;
    let old_line_db: Option<i64> = row.get("old_line")?;
    let new_line_db: Option<i64> = row.get("new_line")?;
    let row_stable_id_db: Option<i64> = row.get("row_stable_id")?;

    let status = CommentStatus::from_db(status_raw.as_str())
        .ok_or_else(|| invalid_text_value("status", status_raw.as_str()))?;
    let line_side = CommentLineSide::from_db(line_side_raw.as_str())
        .ok_or_else(|| invalid_text_value("line_side", line_side_raw.as_str()))?;

    let old_line = old_line_db.map(sql_i64_to_u32).transpose()?;
    let new_line = new_line_db.map(sql_i64_to_u32).transpose()?;
    let row_stable_id = row_stable_id_db.map(|value| value as u64);

    Ok(CommentRecord {
        id: row.get("id")?,
        repo_root: row.get("repo_root")?,
        bookmark_name: row.get("bookmark_name")?,
        created_head_commit: row.get("created_head_commit")?,
        status,
        file_path: row.get("file_path")?,
        line_side,
        old_line,
        new_line,
        row_stable_id,
        hunk_header: row.get("hunk_header")?,
        line_text: row.get("line_text")?,
        context_before: row.get("context_before")?,
        context_after: row.get("context_after")?,
        anchor_hash: row.get("anchor_hash")?,
        comment_text: row.get("comment_text")?,
        stale_reason: row.get("stale_reason")?,
        created_at_unix_ms: row.get("created_at_unix_ms")?,
        updated_at_unix_ms: row.get("updated_at_unix_ms")?,
        last_seen_at_unix_ms: row.get("last_seen_at_unix_ms")?,
        resolved_at_unix_ms: row.get("resolved_at_unix_ms")?,
    })
}

fn sql_i64_to_u32(value: i64) -> rusqlite::Result<u32> {
    u32::try_from(value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("cannot convert sqlite integer {value} to u32"),
            )),
        )
    })
}

fn invalid_text_value(column: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid {column} value: {value}"),
        )),
    )
}
