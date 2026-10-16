//! Provider profile repository (S-020). Holds no API key: that lives in the
//! OS credential store and is referenced only by `key_configured`.

use rusqlite::{params, OptionalExtension, Row};
use selai_domain::{Page, PageRequest, ProviderId, ProviderProtocol, ProviderSettings};

use crate::{Database, StorageError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRecord {
    pub id: ProviderId,
    pub name: String,
    pub protocol: ProviderProtocol,
    pub base_url: String,
    pub model_id: String,
    pub timeout_seconds: u32,
    pub max_output_tokens: u32,
    /// Operator opt-in for plain http to a non-loopback host.
    pub allow_insecure_http: bool,
    pub key_configured: bool,
    pub enabled: bool,
    /// True only when the probe passed for the current `probe_identity`.
    pub tool_probe_passed: bool,
    pub last_tool_probe_at_ms: Option<i64>,
    pub tool_probe_identity: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

const COLUMNS: &str =
    "id, name, protocol, base_url, model_id, options_json, key_configured, enabled,
    tool_probe_passed, last_tool_probe_at_ms, tool_probe_fingerprint, created_at_ms, updated_at_ms";

fn to_sql_err(e: &dyn std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )),
    )
}

fn row_to_provider(row: &Row<'_>) -> rusqlite::Result<ProviderRecord> {
    let id: String = row.get(0)?;
    let protocol: String = row.get(2)?;
    let options: String = row.get(5)?;
    let options: serde_json::Value = serde_json::from_str(&options).map_err(|e| to_sql_err(&e))?;
    let key_configured: i64 = row.get(6)?;
    let enabled: i64 = row.get(7)?;
    let probe_passed: i64 = row.get(8)?;
    Ok(ProviderRecord {
        id: id.parse().map_err(|e| to_sql_err(&e))?,
        name: row.get(1)?,
        protocol: ProviderProtocol::parse(&protocol)
            .ok_or_else(|| to_sql_err(&"unknown protocol"))?,
        base_url: row.get(3)?,
        model_id: row.get(4)?,
        timeout_seconds: options["timeoutSeconds"].as_u64().unwrap_or(120) as u32,
        max_output_tokens: options["maxOutputTokens"].as_u64().unwrap_or(4096) as u32,
        allow_insecure_http: options["allowInsecureHttp"].as_bool().unwrap_or(false),
        key_configured: key_configured == 1,
        enabled: enabled == 1,
        tool_probe_passed: probe_passed == 1,
        last_tool_probe_at_ms: row.get(9)?,
        tool_probe_identity: row.get(10)?,
        created_at_ms: row.get(11)?,
        updated_at_ms: row.get(12)?,
    })
}

impl Database {
    pub fn create_provider(
        &mut self,
        id: ProviderId,
        settings: &ProviderSettings,
        now_ms: i64,
    ) -> Result<ProviderRecord, StorageError> {
        self.conn().execute(
            "INSERT INTO provider_profiles (id, name, protocol, base_url, model_id, options_json,
                key_configured, enabled, tool_probe_passed, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 1, 0, ?7, ?7)",
            params![
                id.to_string(),
                settings.name.as_str(),
                settings.protocol.as_str(),
                settings.base_url.as_str(),
                settings.model_id.as_str(),
                options_json(settings),
                now_ms,
            ],
        )?;
        self.get_provider(id)?
            .ok_or_else(|| StorageError::NotFound(format!("provider {id} after insert")))
    }

    /// Updates a provider if it still has `expected_updated_at_ms`. Changing the
    /// protocol, base URL or model clears the tool-calling result, because that
    /// result described the old endpoint.
    pub fn update_provider(
        &mut self,
        id: ProviderId,
        settings: &ProviderSettings,
        expected_updated_at_ms: i64,
        now_ms: i64,
    ) -> Result<ProviderRecord, StorageError> {
        let current = self
            .get_provider(id)?
            .ok_or_else(|| StorageError::NotFound(format!("provider {id}")))?;
        if current.updated_at_ms != expected_updated_at_ms {
            return Err(StorageError::Conflict(format!(
                "provider {id} was changed elsewhere"
            )));
        }
        let identity_changed = current.protocol != settings.protocol
            || current.base_url != settings.base_url.as_str()
            || current.model_id != settings.model_id.as_str();
        let (probe_passed, probe_at, probe_identity) = if identity_changed {
            (0, None, None)
        } else {
            (
                i64::from(current.tool_probe_passed),
                current.last_tool_probe_at_ms,
                current.tool_probe_identity.clone(),
            )
        };
        self.conn().execute(
            "UPDATE provider_profiles SET name = ?2, protocol = ?3, base_url = ?4, model_id = ?5,
                options_json = ?6, tool_probe_passed = ?7, last_tool_probe_at_ms = ?8,
                tool_probe_fingerprint = ?9, updated_at_ms = ?10
              WHERE id = ?1",
            params![
                id.to_string(),
                settings.name.as_str(),
                settings.protocol.as_str(),
                settings.base_url.as_str(),
                settings.model_id.as_str(),
                options_json(settings),
                probe_passed,
                probe_at,
                probe_identity,
                now_ms,
            ],
        )?;
        self.get_provider(id)?
            .ok_or_else(|| StorageError::NotFound(format!("provider {id} after update")))
    }

    pub fn get_provider(&self, id: ProviderId) -> Result<Option<ProviderRecord>, StorageError> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM provider_profiles WHERE id = ?1"),
                params![id.to_string()],
                row_to_provider,
            )
            .optional()?)
    }

    pub fn list_providers(&self, page: &PageRequest) -> Result<Page<ProviderRecord>, StorageError> {
        let after = match page.cursor.as_deref() {
            None => String::new(),
            Some(c) => c.to_owned(),
        };
        let limit = i64::from(page.effective_limit());
        let sql = format!(
            "SELECT {COLUMNS} FROM provider_profiles
              WHERE (lower(name) || '|' || id) > ?1
              ORDER BY lower(name), id LIMIT ?2"
        );
        let mut stmt = self.conn().prepare(&sql)?;
        let items = stmt
            .query_map(params![after, limit], row_to_provider)?
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if items.len() as i64 == limit {
            items
                .last()
                .map(|p| format!("{}|{}", p.name.to_ascii_lowercase(), p.id))
        } else {
            None
        };
        Ok(Page { items, next_cursor })
    }

    pub fn set_provider_key_configured(
        &self,
        id: ProviderId,
        configured: bool,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        let changed = self.conn().execute(
            "UPDATE provider_profiles SET key_configured = ?2, updated_at_ms = ?3 WHERE id = ?1",
            params![id.to_string(), i64::from(configured), now_ms],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound(format!("provider {id}")));
        }
        Ok(())
    }

    /// Records a tool-calling probe. `identity` must be the current
    /// `ProviderSettings::probe_identity`, so a later change cannot inherit it.
    pub fn record_tool_probe(
        &self,
        id: ProviderId,
        passed: bool,
        identity: &str,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        let changed = self.conn().execute(
            "UPDATE provider_profiles SET tool_probe_passed = ?2, last_tool_probe_at_ms = ?3,
                tool_probe_fingerprint = ?4 WHERE id = ?1",
            params![id.to_string(), i64::from(passed), now_ms, identity],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound(format!("provider {id}")));
        }
        Ok(())
    }

    pub fn delete_provider(&self, id: ProviderId) -> Result<(), StorageError> {
        let changed = self.conn().execute(
            "DELETE FROM provider_profiles WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound(format!("provider {id}")));
        }
        Ok(())
    }
}

fn options_json(settings: &ProviderSettings) -> String {
    serde_json::json!({
        "timeoutSeconds": settings.timeout.seconds(),
        "maxOutputTokens": settings.max_output_tokens.tokens(),
        "allowInsecureHttp": settings.allow_insecure_http,
    })
    .to_string()
}
