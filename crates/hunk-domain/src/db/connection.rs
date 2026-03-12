use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use rusqlite::Connection;

use super::sql;

const DB_FILE_NAME: &str = "hunk.db";
const DB_SCHEMA_VERSION: i64 = 3;
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "0001_init.sql",
        sql: include_str!("migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "0002_branch_scope_reset.sql",
        sql: include_str!("migrations/0002_branch_scope_reset.sql"),
    },
    Migration {
        version: 3,
        name: "0003_row_stable_id_cleanup.sql",
        sql: include_str!("migrations/0003_row_stable_id_cleanup.sql"),
    },
];

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

#[derive(Debug, Clone)]
pub struct DatabaseStore {
    path: PathBuf,
}

impl DatabaseStore {
    pub fn new() -> Result<Self> {
        Ok(Self {
            path: crate::paths::hunk_home_dir()?.join(DB_FILE_NAME),
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

    for migration in MIGRATIONS {
        if user_version >= migration.version {
            continue;
        }

        conn.execute_batch(migration.sql)
            .with_context(|| format!("failed to run migration {}", migration.name))?;
        conn.pragma_update(None, "user_version", migration.version)
            .with_context(|| {
                format!(
                    "failed to update sqlite user_version to schema version {}",
                    migration.version
                )
            })?;
    }

    Ok(())
}
