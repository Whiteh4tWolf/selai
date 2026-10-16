//! Per-host history, task timelines, output retention and small app settings
//! (S-040, S-042). Every retention action writes an audit event in the same
//! transaction, so deleted output always leaves a trace.

use rusqlite::{params, params_from_iter, types::Value as SqlValue, OptionalExtension, Row};
use selai_domain::{ExecStatus, HostId, Page, PageRequest, TaskId, TaskState};
use serde_json::{json, Value};

use crate::{
    agent::{
        append_audit, now_tx, row_to_execution, ExecutionRecord, TaskRecord, EXECUTION_COLUMNS,
    },
    Database, StorageError,
};

/// Filters for the host history. Empty or `None` fields do not restrict.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryFilter {
    pub states: Vec<ExecStatus>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
}

/// What a retention action removes. Metadata is never removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurgeScope {
    /// Output of executions that finished strictly before this time.
    FinishedBefore(i64),
    /// Output of every execution on one host, including archived hosts.
    Host(HostId),
}

fn cursor_parts(cursor: Option<&str>) -> Result<(i64, String), StorageError> {
    match cursor {
        None => Ok((i64::MAX, String::from("~"))),
        Some(c) => {
            let (ms, id) = c
                .split_once('|')
                .ok_or_else(|| StorageError::Constraint("malformed cursor".into()))?;
            let ms: i64 = ms
                .parse()
                .map_err(|_| StorageError::Constraint("malformed cursor".into()))?;
            Ok((ms, id.to_owned()))
        }
    }
}

impl Database {
    /// Host history with filters, newest first. Keyset-paged on
    /// `(created_at_ms, id)`.
    pub fn host_history_filtered(
        &self,
        host: HostId,
        filter: &HistoryFilter,
        page: &PageRequest,
    ) -> Result<Page<ExecutionRecord>, StorageError> {
        let (after_ms, after_id) = cursor_parts(page.cursor.as_deref())?;
        let limit = i64::from(page.effective_limit());

        let mut sql = format!(
            "SELECT {EXECUTION_COLUMNS} FROM command_executions
              WHERE host_id = ? AND (created_at_ms < ? OR (created_at_ms = ? AND id < ?))"
        );
        let mut args: Vec<SqlValue> = vec![
            SqlValue::Text(host.to_string()),
            SqlValue::Integer(after_ms),
            SqlValue::Integer(after_ms),
            SqlValue::Text(after_id),
        ];
        if !filter.states.is_empty() {
            let marks = vec!["?"; filter.states.len()].join(", ");
            sql.push_str(&format!(" AND state IN ({marks})"));
            args.extend(
                filter
                    .states
                    .iter()
                    .map(|s| SqlValue::Text(s.as_str().to_owned())),
            );
        }
        if let Some(since) = filter.since_ms {
            sql.push_str(" AND created_at_ms >= ?");
            args.push(SqlValue::Integer(since));
        }
        if let Some(until) = filter.until_ms {
            sql.push_str(" AND created_at_ms <= ?");
            args.push(SqlValue::Integer(until));
        }
        sql.push_str(" ORDER BY created_at_ms DESC, id DESC LIMIT ?");
        args.push(SqlValue::Integer(limit));

        let mut stmt = self.conn().prepare(&sql)?;
        let items = stmt
            .query_map(params_from_iter(args), row_to_execution)?
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if items.len() as i64 == limit {
            items
                .last()
                .map(|e| format!("{}|{}", e.created_at_ms, e.id))
        } else {
            None
        };
        Ok(Page { items, next_cursor })
    }

    /// The executions of one task in the order they were proposed.
    pub fn task_timeline(&self, task: TaskId) -> Result<Vec<ExecutionRecord>, StorageError> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {EXECUTION_COLUMNS} FROM command_executions
              WHERE task_id = ?1 ORDER BY created_at_ms ASC, id ASC"
        ))?;
        let rows = stmt
            .query_map(params![task.to_string()], row_to_execution)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Tasks run on one host, newest first, with the same keyset paging.
    pub fn host_tasks(
        &self,
        host: HostId,
        page: &PageRequest,
    ) -> Result<Page<TaskRecord>, StorageError> {
        let (after_ms, after_id) = cursor_parts(page.cursor.as_deref())?;
        let limit = i64::from(page.effective_limit());
        let mut stmt = self.conn().prepare(
            "SELECT id, target_host_id, state, model_id_snapshot, error_code, created_at_ms, finished_at_ms
               FROM agent_tasks
              WHERE target_host_id = ?1 AND (created_at_ms < ?2 OR (created_at_ms = ?2 AND id < ?3))
              ORDER BY created_at_ms DESC, id DESC LIMIT ?4",
        )?;
        let items = stmt
            .query_map(
                params![host.to_string(), after_ms, after_id, limit],
                row_to_task,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if items.len() as i64 == limit {
            items
                .last()
                .map(|t| format!("{}|{}", t.created_at_ms, t.id))
        } else {
            None
        };
        Ok(Page { items, next_cursor })
    }

    /// Removes stored output previews in `scope` and writes one audit event for
    /// the whole action. Returns how many executions lost output. Command text,
    /// digests, exit codes and times stay.
    pub fn purge_output(&mut self, scope: PurgeScope, now_ms: i64) -> Result<usize, StorageError> {
        let tx = now_tx(self.conn_mut())?;
        let (changed, detail) = match scope {
            PurgeScope::FinishedBefore(cutoff) => (
                tx.execute(
                    "UPDATE command_executions SET stdout_preview = NULL, stderr_preview = NULL
                      WHERE finished_at_ms IS NOT NULL AND finished_at_ms < ?1
                        AND (stdout_preview IS NOT NULL OR stderr_preview IS NOT NULL)",
                    params![cutoff],
                )?,
                json!({"scope": "finished_before", "finished_before_ms": cutoff}),
            ),
            PurgeScope::Host(host) => (
                tx.execute(
                    "UPDATE command_executions SET stdout_preview = NULL, stderr_preview = NULL
                      WHERE host_id = ?1
                        AND (stdout_preview IS NOT NULL OR stderr_preview IS NOT NULL)",
                    params![host.to_string()],
                )?,
                json!({"scope": "host", "host_id": host.to_string()}),
            ),
        };
        if changed > 0 {
            let mut payload = detail;
            payload["executions"] = json!(changed);
            append_audit(&tx, "output_purged", None, None, &payload, now_ms)?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Reads one app setting. Settings hold small, non-secret preferences only.
    pub fn get_setting(&self, key: &str) -> Result<Option<Value>, StorageError> {
        let raw: Option<String> = self
            .conn()
            .query_row(
                "SELECT value_json FROM app_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        raw.map(|text| serde_json::from_str(&text))
            .transpose()
            .map_err(|e| StorageError::Constraint(format!("stored setting is not JSON: {e}")))
    }

    /// Writes one app setting. Refuses keys that look like secrets.
    pub fn set_setting(
        &mut self,
        key: &str,
        value: &Value,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        let lowered = key.to_ascii_lowercase();
        if ["secret", "password", "token", "key"]
            .iter()
            .any(|w| lowered.contains(w))
        {
            return Err(StorageError::Constraint(
                "settings must not hold secret-like keys".into(),
            ));
        }
        let text = serde_json::to_string(value)
            .map_err(|e| StorageError::Constraint(format!("setting is not serialisable: {e}")))?;
        self.conn().execute(
            "INSERT INTO app_settings (key, value_json, updated_at_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                            updated_at_ms = excluded.updated_at_ms",
            params![key, text, now_ms],
        )?;
        Ok(())
    }
}

fn row_to_task(row: &Row<'_>) -> rusqlite::Result<TaskRecord> {
    let state: String = row.get(2)?;
    let state = TaskState::parse(&state).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            "unknown task state".into(),
        )
    })?;
    let id: String = row.get(0)?;
    let host: String = row.get(1)?;
    Ok(TaskRecord {
        id: id.parse().map_err(|_| conversion(0))?,
        host: host.parse().map_err(|_| conversion(1))?,
        state,
        model_id: row.get(3)?,
        error_code: row.get(4)?,
        created_at_ms: row.get(5)?,
        finished_at_ms: row.get(6)?,
    })
}

fn conversion(column: usize) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        "invalid id".into(),
    )
}

impl Database {
    /// Records one checkpoint of a task. Sequence numbers are unique per task, so
    /// a replayed step is refused. Metadata holds summaries only, never model
    /// reasoning or command output.
    pub fn record_step(
        &mut self,
        task: TaskId,
        sequence_no: u32,
        kind: &str,
        state: &str,
        metadata: &Value,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        let id = format!("{task}:{sequence_no}");
        self.conn().execute(
            "INSERT INTO agent_steps (id, task_id, sequence_no, kind, state, metadata_json, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, task.to_string(), sequence_no, kind, state, metadata.to_string(), now_ms],
        )?;
        Ok(())
    }

    /// Checkpoints of one task in order.
    pub fn task_steps(&self, task: TaskId) -> Result<Vec<(u32, String, String)>, StorageError> {
        let mut stmt = self.conn().prepare(
            "SELECT sequence_no, kind, state FROM agent_steps WHERE task_id = ?1 ORDER BY sequence_no",
        )?;
        let rows = stmt
            .query_map(params![task.to_string()], |r| {
                Ok((r.get::<_, u32>(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
