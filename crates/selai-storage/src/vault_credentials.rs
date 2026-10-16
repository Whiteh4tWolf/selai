use crate::{Database, StorageError};
use rusqlite::{params, OptionalExtension};

impl Database {
    pub fn legacy_credential_accounts(&self) -> Result<Vec<(String, bool)>, StorageError> {
        let mut result = Vec::new();
        let mut stmt = self
            .conn()
            .prepare("SELECT id,auth_method,credential_configured FROM hosts WHERE kind='ssh'")?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })? {
            let (id, auth, configured) = row?;
            for kind in ["password", "key_passphrase", "sudo_password"] {
                result.push((
                    format!("ssh/{id}/{kind}"),
                    configured
                        && ((kind == "password" && auth == "password")
                            || (kind == "key_passphrase" && auth == "private_key")),
                ));
            }
        }
        let mut stmt = self
            .conn()
            .prepare("SELECT id,key_configured FROM provider_profiles")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))? {
            let (id, configured) = row?;
            result.push((format!("provider/{id}/api_key"), configured));
        }
        Ok(result)
    }
    pub fn external_key_files(&self) -> Result<Vec<(String, String)>, StorageError> {
        let mut stmt = self.conn().prepare("SELECT id,private_key_path FROM hosts WHERE auth_method='private_key' AND private_key_path NOT LIKE 'vault://%'")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
    pub fn mark_key_managed(&self, id: &str) -> Result<(), StorageError> {
        self.conn().execute(
            "UPDATE hosts SET private_key_path=?2 WHERE id=?1",
            params![id, format!("vault://{id}")],
        )?;
        Ok(())
    }
    pub fn store_vault_file(&self, name: &str, data: &[u8]) -> Result<(), StorageError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO vault_files(name,data) VALUES(?1,?2)",
            params![name, data],
        )?;
        Ok(())
    }
    pub fn archived_vault_files(&self) -> Result<Vec<String>, StorageError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT name FROM vault_files ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
    pub fn archived_vault_file(&self, name: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT data FROM vault_files WHERE name=?1",
                [name],
                |row| row.get(0),
            )
            .optional()?)
    }
    pub fn vault_secret(&self, account: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT secret FROM vault_credentials WHERE account=?1 AND length(secret)>0",
                [account],
                |row| row.get(0),
            )
            .optional()?)
    }
    pub fn put_vault_secret(
        &self,
        account: &str,
        secret: &[u8],
        legacy: bool,
    ) -> Result<(), StorageError> {
        self.conn().execute("INSERT INTO vault_credentials(account,secret,legacy_cleanup) VALUES(?1,?2,?3)
            ON CONFLICT(account) DO UPDATE SET secret=excluded.secret,legacy_cleanup=MAX(legacy_cleanup,excluded.legacy_cleanup)", params![account,secret,legacy])?;
        Ok(())
    }
    pub fn delete_vault_secret(&self, account: &str) -> Result<(), StorageError> {
        self.conn().execute(
            "DELETE FROM vault_credentials WHERE account=?1 AND legacy_cleanup=0",
            [account],
        )?;
        self.conn().execute(
            "UPDATE vault_credentials SET secret=X'' WHERE account=?1 AND legacy_cleanup=1",
            [account],
        )?;
        Ok(())
    }
    pub fn pending_legacy_credentials(&self) -> Result<Vec<String>, StorageError> {
        let mut statement = self
            .conn()
            .prepare("SELECT account FROM vault_credentials WHERE legacy_cleanup=1")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
    pub fn mark_legacy_credential_removed(&self, account: &str) -> Result<(), StorageError> {
        self.conn().execute(
            "UPDATE vault_credentials SET legacy_cleanup=0 WHERE account=?1",
            [account],
        )?;
        self.conn().execute(
            "DELETE FROM vault_credentials WHERE account=?1 AND length(secret)=0",
            [account],
        )?;
        Ok(())
    }
}
