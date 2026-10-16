use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::Connection;

use crate::{migrate, StorageError};

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// One SQLite connection with Selai's required settings and schema applied.
///
/// Connection-level pragmas are applied here, not in migrations: they do not
/// take effect inside a transaction, so a migration cannot set them.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens (creating if needed) a file-backed database in WAL mode and
    /// migrates it to the latest schema.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let mut conn = Connection::open(path)?;
        configure(&conn, true)?;
        migrate::apply(&mut conn, migrate::MIGRATIONS, now_ms())?;
        Ok(Self { conn })
    }

    /// In-memory database for tests. WAL does not apply to `:memory:`.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let mut conn = Connection::open_in_memory()?;
        configure(&conn, false)?;
        migrate::apply(&mut conn, migrate::MIGRATIONS, now_ms())?;
        Ok(Self { conn })
    }

    /// Raw connection for repositories in this crate. Not exposed outside it.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Number of retired host keys for a host (history of rotations).
    pub fn retired_key_count(&self, host: selai_domain::HostId) -> Result<i64, StorageError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM known_host_keys WHERE host_id = ?1 AND replaced_at_ms IS NOT NULL",
            rusqlite::params![host.to_string()],
            |row| row.get(0),
        )?)
    }

    pub fn journal_mode(&self) -> Result<String, StorageError> {
        Ok(self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?)
    }

    pub fn schema_version(&self) -> Result<i64, StorageError> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?)
    }
}

fn configure(conn: &Connection, file_backed: bool) -> Result<(), StorageError> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    if file_backed {
        let mode: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StorageError::Pragma("journal_mode"));
        }
        // FULL: a committed audit record must survive power loss before SSH I/O.
        conn.pragma_update(None, "synchronous", "FULL")?;
    }
    let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    if foreign_keys != 1 {
        return Err(StorageError::Pragma("foreign_keys"));
    }
    Ok(())
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}
