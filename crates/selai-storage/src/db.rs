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
    /// Uses a raw random key, already unwrapped by the passphrase vault. Refuses
    /// to run if a build accidentally links an SQLite without SQLCipher.
    pub fn open_encrypted(path: &Path, key: &[u8; 32]) -> Result<Self, StorageError> {
        let mut conn = Connection::open(path)?;
        Self::set_key(&conn, key)?;
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
            row.get::<_, i64>(0)
        })?;
        configure(&conn, true)?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "secure_delete", true)?;
        conn.pragma_update(None, "cipher_memory_security", "ON")?;
        migrate::apply(&mut conn, migrate::MIGRATIONS, now_ms())?;
        Ok(Self { conn })
    }

    fn set_key(conn: &Connection, key: &[u8; 32]) -> Result<(), StorageError> {
        let version: String = conn.query_row("PRAGMA cipher_version", [], |row| row.get(0))?;
        if version.is_empty() {
            return Err(StorageError::Io("SQLCipher is unavailable".into()));
        }
        let encoded = zeroize::Zeroizing::new(
            key.iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        );
        let literal = zeroize::Zeroizing::new(format!("x'{}'", encoded.as_str()));
        conn.pragma_update(None, "key", literal.as_str())?;
        Ok(())
    }

    /// SQLCipher's supported export path also handles legacy plaintext databases.
    /// The destination is encrypted before any pages are written.
    pub fn export_encrypted(&self, path: &Path, key: &[u8; 32]) -> Result<(), StorageError> {
        let encoded = zeroize::Zeroizing::new(
            key.iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        );
        let literal = zeroize::Zeroizing::new(format!("x'{}'", encoded.as_str()));
        self.conn.execute(
            "ATTACH DATABASE ?1 AS sealed KEY ?2",
            rusqlite::params![path.to_string_lossy(), literal.as_str()],
        )?;
        let result = self
            .conn
            .query_row("SELECT sqlcipher_export('sealed')", [], |_| Ok(()));
        let detach = self.conn.execute_batch("DETACH DATABASE sealed");
        result?;
        detach?;
        Ok(())
    }

    pub fn checkpoint(&self) -> Result<(), StorageError> {
        let busy: i64 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 {
            return Err(StorageError::Busy);
        }
        Ok(())
    }

    /// Close the encrypted connection even when a cancelled task still owns an
    /// Arc to this Database. Every subsequent query sees an empty connection.
    pub fn seal(&mut self) -> Result<(), StorageError> {
        // Checkpointing is an optimization for reopening. On an I/O/busy error
        // the committed WAL remains encrypted and recoverable; do not retain
        // the decryption key merely because it cannot be truncated now.
        let _ = self.checkpoint();
        self.conn = Connection::open_in_memory()?;
        Ok(())
    }

    pub fn integrity_check(&self) -> Result<(), StorageError> {
        let result: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if result != "ok" {
            return Err(StorageError::Io("vault integrity check failed".into()));
        }
        Ok(())
    }

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

    /// Raw connection for tests that check schema constraints directly.
    #[doc(hidden)]
    pub fn connection_for_provider_test(&self) -> &Connection {
        &self.conn
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
    conn.pragma_update(None, "temp_store", "MEMORY")?;
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
