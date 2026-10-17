use rusqlite::{params, OptionalExtension};

use crate::{Database, StorageError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKeyRecord {
    pub id: String,
    pub name: String,
    pub algorithm: String,
    pub public_key: String,
    pub fingerprint_sha256: String,
    pub created_at_ms: i64,
}

impl Database {
    pub fn list_ssh_keys(&self) -> Result<Vec<SshKeyRecord>, StorageError> {
        let mut stmt = self.conn().prepare("SELECT id,name,algorithm,public_key,fingerprint_sha256,created_at_ms FROM ssh_keys ORDER BY lower(name),id")?;
        let keys = stmt
            .query_map([], |r| {
                Ok(SshKeyRecord {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    algorithm: r.get(2)?,
                    public_key: r.get(3)?,
                    fingerprint_sha256: r.get(4)?,
                    created_at_ms: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(keys)
    }
    pub fn create_ssh_key(&self, key: &SshKeyRecord) -> Result<(), StorageError> {
        self.conn().execute("INSERT INTO ssh_keys(id,name,algorithm,public_key,fingerprint_sha256,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6)", params![key.id,key.name,key.algorithm,key.public_key,key.fingerprint_sha256,key.created_at_ms])?;
        Ok(())
    }
    pub fn delete_ssh_key(&self, id: &str) -> Result<(), StorageError> {
        let refs: i64 = self.conn().query_row(
            "SELECT count(*) FROM hosts WHERE private_key_path=?1 AND archived_at_ms IS NULL",
            params![format!("vault://key:{id}")],
            |r| r.get(0),
        )?;
        if refs > 0 {
            return Err(StorageError::Conflict(
                "SSH key is assigned to one or more host profiles".into(),
            ));
        }
        let changed = self
            .conn()
            .execute("DELETE FROM ssh_keys WHERE id=?1", params![id])?;
        if changed == 0 {
            return Err(StorageError::NotFound(format!("SSH key {id}")));
        }
        Ok(())
    }
    pub fn ssh_key(&self, id: &str) -> Result<Option<SshKeyRecord>, StorageError> {
        Ok(self.conn().query_row("SELECT id,name,algorithm,public_key,fingerprint_sha256,created_at_ms FROM ssh_keys WHERE id=?1", params![id], |r| Ok(SshKeyRecord { id:r.get(0)?, name:r.get(1)?, algorithm:r.get(2)?, public_key:r.get(3)?, fingerprint_sha256:r.get(4)?, created_at_ms:r.get(5)? })).optional()?)
    }
}
