//! Network profile metadata. Secrets remain in the encrypted credential store.
use crate::{Database, StorageError};
use rusqlite::{params, OptionalExtension};

impl Database {
    pub fn network_profiles(&self, kind: &str) -> Result<Vec<String>, StorageError> {
        let mut stmt = self.conn().prepare("SELECT config_json FROM network_profiles WHERE kind=?1 ORDER BY updated_at_ms DESC, id")?;
        let rows = stmt.query_map([kind], |r| r.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    pub fn network_profile(&self, id: &str, kind: &str) -> Result<Option<String>, StorageError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT config_json FROM network_profiles WHERE id=?1 AND kind=?2",
                params![id, kind],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn save_network_profile(
        &self,
        id: &str,
        kind: &str,
        json: &str,
        now: i64,
    ) -> Result<(), StorageError> {
        self.conn().execute("INSERT INTO network_profiles(id,kind,config_json,updated_at_ms) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET config_json=excluded.config_json,updated_at_ms=excluded.updated_at_ms WHERE kind=excluded.kind", params![id,kind,json,now])?;
        Ok(())
    }
    pub fn delete_network_profile(&self, id: &str, kind: &str) -> Result<(), StorageError> {
        self.conn().execute(
            "DELETE FROM network_profiles WHERE id=?1 AND kind=?2",
            params![id, kind],
        )?;
        Ok(())
    }
    pub fn host_proxy(&self, host: &str) -> Result<Option<String>, StorageError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT proxy_id FROM host_proxies WHERE host_id=?1",
                [host],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn set_host_proxy(&self, host: &str, proxy: Option<&str>) -> Result<(), StorageError> {
        if let Some(proxy) = proxy {
            self.conn().execute("INSERT INTO host_proxies(host_id,proxy_id) VALUES (?1,?2) ON CONFLICT(host_id) DO UPDATE SET proxy_id=excluded.proxy_id", params![host,proxy])?;
        } else {
            self.conn()
                .execute("DELETE FROM host_proxies WHERE host_id=?1", [host])?;
        }
        Ok(())
    }
    pub fn proxy_usage_count(&self, proxy: &str) -> Result<i64, StorageError> {
        Ok(self.conn().query_row(
            "SELECT count(*) FROM host_proxies WHERE proxy_id=?1",
            [proxy],
            |r| r.get(0),
        )?)
    }
}
