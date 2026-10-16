//! Persistence the runtime needs. The trait lets tests inject failures (for
//! example, a failed audit write must stop a command). `DbStore` is the real one.

use std::sync::{Arc, Mutex};

use selai_domain::{ApprovalId, ExecStatus, ExecutionId, TaskId, TaskState};
use selai_storage::{
    ApprovalRecord, Database, ExecutionOutcome, ExecutionRecord, NewApproval, NewExecution,
    NewTask, StorageError,
};
use serde_json::Value;

pub trait AgentStore: Send + Sync {
    fn create_task(&self, task: &NewTask, now_ms: i64) -> Result<(), StorageError>;
    fn transition_task(
        &self,
        id: TaskId,
        to: TaskState,
        error_code: Option<&str>,
        now_ms: i64,
    ) -> Result<(), StorageError>;
    /// Must succeed before the command is sent anywhere.
    fn propose(&self, execution: &NewExecution) -> Result<(), StorageError>;
    fn transition_execution(
        &self,
        id: ExecutionId,
        to: ExecStatus,
        outcome: &ExecutionOutcome,
        now_ms: i64,
    ) -> Result<(), StorageError>;
    fn create_approval(&self, approval: &NewApproval) -> Result<(), StorageError>;
    /// Marks a ticket expired once its deadline has passed. Before the deadline it does nothing.
    fn expire_approval(&self, id: ApprovalId, now_ms: i64) -> Result<(), StorageError>;
    fn get_approval(&self, id: ApprovalId) -> Result<Option<ApprovalRecord>, StorageError>;
    fn get_execution(&self, id: ExecutionId) -> Result<Option<ExecutionRecord>, StorageError>;
    fn audit(
        &self,
        event_type: &str,
        task: Option<TaskId>,
        execution: Option<ExecutionId>,
        payload: &Value,
        now_ms: i64,
    ) -> Result<(), StorageError>;
    /// Writes one checkpoint. A failure stops the task: progress that cannot be
    /// recorded is not made.
    fn checkpoint(
        &self,
        task: TaskId,
        sequence_no: u32,
        kind: &str,
        state: &str,
        metadata: &Value,
        now_ms: i64,
    ) -> Result<(), StorageError>;
    /// Adds one model turn's token counts to the task.
    fn record_usage(
        &self,
        task: TaskId,
        input: Option<u64>,
        output: Option<u64>,
    ) -> Result<(), StorageError>;
}

/// `AgentStore` over the local SQLite database. Locks are held only for one call.
#[derive(Clone)]
pub struct DbStore {
    db: Arc<Mutex<Database>>,
}

impl DbStore {
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Database>, StorageError> {
        self.db
            .lock()
            .map_err(|_| StorageError::Io("database lock poisoned".into()))
    }
}

impl AgentStore for DbStore {
    fn create_task(&self, task: &NewTask, now_ms: i64) -> Result<(), StorageError> {
        self.lock()?.create_agent_task(task, now_ms)
    }

    fn transition_task(
        &self,
        id: TaskId,
        to: TaskState,
        error_code: Option<&str>,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.lock()?.transition_task(id, to, error_code, now_ms)
    }

    fn propose(&self, execution: &NewExecution) -> Result<(), StorageError> {
        self.lock()?.propose_execution(execution)
    }

    fn transition_execution(
        &self,
        id: ExecutionId,
        to: ExecStatus,
        outcome: &ExecutionOutcome,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.lock()?.transition_execution(id, to, outcome, now_ms)
    }

    fn create_approval(&self, approval: &NewApproval) -> Result<(), StorageError> {
        self.lock()?.create_approval(approval)
    }

    fn expire_approval(&self, id: ApprovalId, now_ms: i64) -> Result<(), StorageError> {
        self.lock()?.expire_approval(id, now_ms).map(|_| ())
    }

    fn get_approval(&self, id: ApprovalId) -> Result<Option<ApprovalRecord>, StorageError> {
        self.lock()?.get_approval(id)
    }

    fn get_execution(&self, id: ExecutionId) -> Result<Option<ExecutionRecord>, StorageError> {
        self.lock()?.get_execution(id)
    }

    fn audit(
        &self,
        event_type: &str,
        task: Option<TaskId>,
        execution: Option<ExecutionId>,
        payload: &Value,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.lock()?
            .append_audit_event(event_type, task, execution, payload, now_ms)
    }

    fn checkpoint(
        &self,
        task: TaskId,
        sequence_no: u32,
        kind: &str,
        state: &str,
        metadata: &Value,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.lock()?
            .record_step(task, sequence_no, kind, state, metadata, now_ms)
    }

    fn record_usage(
        &self,
        task: TaskId,
        input: Option<u64>,
        output: Option<u64>,
    ) -> Result<(), StorageError> {
        self.lock()?.record_task_usage(task, input, output)
    }
}
