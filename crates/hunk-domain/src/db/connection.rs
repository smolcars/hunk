use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use rusqlite::Connection;

use super::sql;

const DB_DIR_NAME: &str = ".hunkdiff";
const DB_FILE_NAME: &str = "hunk.db";
const DB_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone)]
pub struct DatabaseStore {
    path: PathBuf,
}

impl DatabaseStore {
    pub fn new() -> Result<Self> {
        let home_dir =
            dirs::home_dir().ok_or_else(|| anyhow!("failed to resolve home directory"))?;
        Ok(Self {
            path: home_dir.join(DB_DIR_NAME).join(DB_FILE_NAME),
        })
    }

    pub fn from_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn open_connection(&self) -> Result<Connection> {
        ensure_db_parent_dir(&self.path)?;
        let conn = Connection::open(&self.path).with_context(|| {
            format!("failed to open sqlite database at {}", self.path.display())
        })?;

        conn.execute_batch(sql::connection::SETUP)
            .context("failed to apply sqlite pragmas")?;
        run_migrations(&conn)?;
        Ok(conn)
    }
}

fn ensure_db_parent_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("database path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create database directory {}", parent.display()))?;
    Ok(())
}

fn run_migrations(conn: &Connection) -> Result<()> {
    let user_version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .context("failed to read sqlite user_version")?;

    if user_version > DB_SCHEMA_VERSION {
        return Err(anyhow!(
            "database schema version {} is newer than supported {}",
            user_version,
            DB_SCHEMA_VERSION
        ));
    }

    if user_version < 1 {
        conn.execute_batch(include_str!("migrations/0001_init.sql"))
            .context("failed to run migration 0001_init.sql")?;
        conn.pragma_update(None, "user_version", DB_SCHEMA_VERSION)
            .context("failed to update sqlite user_version to schema version 1")?;
    }

    Ok(())
}
