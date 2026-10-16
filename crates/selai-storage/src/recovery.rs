use rusqlite::params;

use crate::{Database, StorageError};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub tasks_interrupted: usize,
    pub executions_interrupted: usize,
    pub approvals_closed: usize,
}

impl Database {
    /// Closes everything that was live when the previous process stopped.
    ///
    /// Work is marked `interrupted` and is never re-run: replaying a remote
    /// command after a crash is forbidden. Runs once at startup, before any
    /// new task can start.
    pub fn reconcile_after_restart(
        &mut self,
        now_ms: i64,
    ) -> Result<ReconcileReport, StorageError> {
        let tx = self
            .conn_mut()
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let executions_interrupted = tx.execute(
            "UPDATE command_executions
                SET state = 'interrupted', error_code = 'TASK_INTERRUPTED', finished_at_ms = ?1
              WHERE state IN ('proposed', 'awaiting_approval', 'running')",
            params![now_ms],
        )?;
        let tasks_interrupted = tx.execute(
            "UPDATE agent_tasks
                SET state = 'interrupted', error_code = 'TASK_INTERRUPTED', finished_at_ms = ?1
              WHERE state IN ('created', 'validating_scope', 'ready', 'running', 'awaiting_approval')",
            params![now_ms],
        )?;
        // A ticket past its deadline is expired; any other open ticket is cancelled.
        // Nothing is replayed after a restart.
        let approvals_expired = tx.execute(
            "UPDATE approval_requests SET state = 'expired', decided_at_ms = ?1
              WHERE state = 'pending' AND expires_at_ms <= ?1",
            params![now_ms],
        )?;
        let approvals_closed = tx.execute(
            "UPDATE approval_requests SET state = 'cancelled', decided_at_ms = ?1
              WHERE state IN ('pending', 'approved')",
            params![now_ms],
        )?;
        let approvals_closed = approvals_closed + approvals_expired;
        if executions_interrupted + tasks_interrupted + approvals_closed > 0 {
            crate::agent::append_audit(
                &tx,
                "startup_reconciled",
                None,
                None,
                &serde_json::json!({
                    "tasks_interrupted": tasks_interrupted,
                    "executions_interrupted": executions_interrupted,
                    "approvals_closed": approvals_closed,
                }),
                now_ms,
            )?;
        }
        tx.commit()?;
        Ok(ReconcileReport {
            tasks_interrupted,
            executions_interrupted,
            approvals_closed,
        })
    }
}
