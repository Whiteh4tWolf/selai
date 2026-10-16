//! Ports the runtime depends on: remote execution, approval waiting, and the
//! event stream shown to the operator. Implementations live in the app crate
//! (and in tests).

use std::{future::Future, sync::Arc, time::Duration};

use selai_domain::{ApprovalId, ExecutionId};
use selai_ssh::{ExecOutput, SshError};
use selai_storage::ApprovalDecision;
use tokio_util::sync::CancellationToken;

/// Runs one non-interactive command on the task's host.
pub trait RemoteExecutor: Send + Sync {
    fn run(
        &self,
        command: &str,
        timeout: Duration,
        max_bytes_per_stream: usize,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<ExecOutput, SshError>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    /// The operator decided. The decision is already in the database; the runtime
    /// still re-reads it before acting.
    Decided(ApprovalDecision),
    TimedOut,
    Cancelled,
}

/// Waits for the operator to decide an approval ticket.
pub trait ApprovalWaiter: Send + Sync {
    fn wait(
        &self,
        approval: ApprovalId,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> impl Future<Output = WaitOutcome> + Send;
}

/// What the operator sees while a task runs.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEvent {
    State {
        state: String,
        error_code: Option<String>,
    },
    TextDelta {
        text: String,
    },
    ToolProposed {
        execution: ExecutionId,
        tool: String,
        command: String,
        host_label: String,
    },
    ApprovalRequired {
        approval: ApprovalId,
        execution: ExecutionId,
        command: String,
        expires_at_ms: i64,
    },
    ToolFinished {
        execution: ExecutionId,
        state: String,
        exit_code: Option<i64>,
        error_code: Option<String>,
    },
    /// Running token totals for the task after a model turn. `None` when the
    /// provider has not reported that count.
    Usage {
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    Completed {
        text: String,
    },
    Failed {
        code: String,
        message: String,
    },
}

pub type EventSink = Arc<dyn Fn(TaskEvent) + Send + Sync>;
