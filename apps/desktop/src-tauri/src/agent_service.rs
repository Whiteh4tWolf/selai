//! Agent tasks in the app: starting a task on one host, approvals, cancellation,
//! per-host history and audit verification (S-030, S-033, S-040).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use selai_agent::{
    run_task, ApprovalWaiter, DbStore, EventSink, Limits, RemoteExecutor, TaskEvent, TaskSpec,
    WaitOutcome,
};
use selai_domain::{
    ApprovalId, ErrorCode, HostId, Page, PageRequest, ProviderId, SelaiError, TaskId,
};
use selai_secrets::SecretKey;
use selai_ssh::{connect, ConnectOptions, Endpoint, ExecOutput, Session, SshError};
use selai_storage::{ApprovalDecision, Database};
use serde::Serialize;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::{
    dto::{CommandError, KeyErrorDetails},
    provider_service::{settings_from_record, tool_calling_view},
    service::Services,
};

const PROMPT_MAX_BYTES: usize = 4000;

/// Waiters and early decisions share one lock, so a decision either reaches a
/// registered waiter or is kept for the next `wait`, never lost in between.
#[derive(Default)]
struct Approvals {
    waiters: HashMap<ApprovalId, oneshot::Sender<ApprovalDecision>>,
    /// Decisions that arrived before their task started waiting. The runtime
    /// emits the approval event before it calls `wait`, so a fast click can land
    /// first.
    early: HashMap<ApprovalId, ApprovalDecision>,
}

/// Live tasks and approvals waiting for a decision.
#[derive(Default)]
pub struct AgentRegistry {
    tokens: Mutex<HashMap<TaskId, CancellationToken>>,
    approvals: Mutex<Approvals>,
}

impl AgentRegistry {
    fn register_task(&self, task: TaskId) -> CancellationToken {
        let token = CancellationToken::new();
        if let Ok(mut map) = self.tokens.lock() {
            map.insert(task, token.clone());
        }
        token
    }

    fn finish_task(&self, task: TaskId) {
        if let Ok(mut map) = self.tokens.lock() {
            map.remove(&task);
        }
    }

    fn cancel_task(&self, task: TaskId) -> bool {
        self.tokens
            .lock()
            .ok()
            .and_then(|map| map.get(&task).cloned())
            .map(|t| t.cancel())
            .is_some()
    }

    fn deliver(&self, approval: ApprovalId, decision: ApprovalDecision) {
        let Ok(mut state) = self.approvals.lock() else {
            return;
        };
        match state.waiters.remove(&approval) {
            Some(tx) => {
                let _ = tx.send(decision);
            }
            None => {
                state.early.insert(approval, decision);
            }
        }
    }
}

/// Waits for an operator decision delivered through the registry. The database
/// decides validity; this only wakes the task.
struct RegistryWaiter {
    registry: Arc<AgentRegistry>,
}

impl ApprovalWaiter for RegistryWaiter {
    async fn wait(
        &self,
        approval: ApprovalId,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> WaitOutcome {
        let rx = {
            let Ok(mut state) = self.registry.approvals.lock() else {
                return WaitOutcome::TimedOut;
            };
            if let Some(decision) = state.early.remove(&approval) {
                return WaitOutcome::Decided(decision);
            }
            let (tx, rx) = oneshot::channel();
            state.waiters.insert(approval, tx);
            rx
        };
        let outcome = tokio::select! {
            () = cancel.cancelled() => WaitOutcome::Cancelled,
            r = tokio::time::timeout(timeout, rx) => match r {
                Ok(Ok(decision)) => WaitOutcome::Decided(decision),
                _ => WaitOutcome::TimedOut,
            },
        };
        if let Ok(mut state) = self.registry.approvals.lock() {
            state.waiters.remove(&approval);
        }
        outcome
    }
}

/// Runs commands on the task's own SSH session. Holds no credential.
struct SessionExecutor {
    session: Arc<Session>,
}

impl RemoteExecutor for SessionExecutor {
    fn run(
        &self,
        command: &str,
        timeout: Duration,
        max_bytes_per_stream: usize,
        cancel: &CancellationToken,
    ) -> impl std::future::Future<Output = Result<ExecOutput, SshError>> + Send {
        let session = Arc::clone(&self.session);
        let command = command.to_owned();
        let cancel = cancel.clone();
        async move {
            session
                .exec(&command, timeout, max_bytes_per_stream, &cancel)
                .await
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskEventView {
    State {
        state: String,
        error_code: Option<String>,
    },
    TextDelta {
        text: String,
    },
    ToolProposed {
        execution_id: String,
        tool: String,
        command: String,
        host_label: String,
    },
    ApprovalRequired {
        approval_id: String,
        execution_id: String,
        command: String,
        expires_at_ms: i64,
    },
    ToolFinished {
        execution_id: String,
        state: String,
        exit_code: Option<i64>,
        error_code: Option<String>,
    },
    Completed {
        text: String,
    },
    Failed {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventView {
    pub task_id: String,
    #[serde(flatten)]
    pub event: TaskEventView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionView {
    pub id: String,
    pub task_id: String,
    pub host_id: String,
    pub model_id: String,
    pub tool: String,
    pub command_redacted: String,
    pub command_sha256: String,
    pub state: &'static str,
    pub policy_decision: Option<String>,
    pub exit_code: Option<i64>,
    pub error_code: Option<String>,
    pub stdout_preview: Option<String>,
    pub stderr_preview: Option<String>,
    pub output_truncated: bool,
    pub created_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditVerifyView {
    pub ok: bool,
    pub events_checked: usize,
    pub broken_at_seq: Option<i64>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

fn event_view(event: TaskEvent) -> TaskEventView {
    match event {
        TaskEvent::State { state, error_code } => TaskEventView::State { state, error_code },
        TaskEvent::TextDelta { text } => TaskEventView::TextDelta { text },
        TaskEvent::ToolProposed {
            execution,
            tool,
            command,
            host_label,
        } => TaskEventView::ToolProposed {
            execution_id: execution.to_string(),
            tool,
            command,
            host_label,
        },
        TaskEvent::ApprovalRequired {
            approval,
            execution,
            command,
            expires_at_ms,
        } => TaskEventView::ApprovalRequired {
            approval_id: approval.to_string(),
            execution_id: execution.to_string(),
            command,
            expires_at_ms,
        },
        TaskEvent::ToolFinished {
            execution,
            state,
            exit_code,
            error_code,
        } => TaskEventView::ToolFinished {
            execution_id: execution.to_string(),
            state,
            exit_code,
            error_code,
        },
        TaskEvent::Completed { text } => TaskEventView::Completed { text },
        TaskEvent::Failed { code, message } => TaskEventView::Failed { code, message },
    }
}

pub(crate) fn execution_view(e: selai_storage::ExecutionRecord) -> ExecutionView {
    ExecutionView {
        id: e.id.to_string(),
        task_id: e.task.to_string(),
        host_id: e.host.to_string(),
        model_id: e.model_id,
        tool: e.tool_name,
        command_redacted: e.command_redacted,
        command_sha256: e.command_sha256,
        state: e.state.as_str(),
        policy_decision: e.policy_decision,
        exit_code: e.exit_code,
        error_code: e.error_code,
        stdout_preview: e.stdout_preview,
        stderr_preview: e.stderr_preview,
        output_truncated: e.output_truncated,
        created_at_ms: e.created_at_ms,
        started_at_ms: e.started_at_ms,
        finished_at_ms: e.finished_at_ms,
    }
}

fn storage(e: &selai_storage::StorageError) -> CommandError {
    CommandError::new(e.code(), e.to_string())
}

impl Services {
    /// Starts one task on one host. The host is fixed here, from the mention chip;
    /// the model cannot choose it. Refuses if the provider's tool calling has not
    /// been verified for its current endpoint and model.
    pub async fn agent_start(
        &self,
        host_id: &str,
        provider_id: &str,
        prompt: &str,
        on_event: Arc<dyn Fn(AgentEventView) + Send + Sync>,
    ) -> Result<String, CommandError> {
        let host: HostId = host_id
            .parse()
            .map_err(|_| CommandError::new(ErrorCode::InvalidInput, "invalid host id"))?;
        let provider: ProviderId = provider_id
            .parse()
            .map_err(|_| CommandError::new(ErrorCode::InvalidInput, "invalid provider id"))?;
        let prompt = prompt.trim();
        if prompt.is_empty() || prompt.len() > PROMPT_MAX_BYTES {
            return Err(CommandError::new(
                ErrorCode::InvalidInput,
                "the request must be 1 to 4000 bytes",
            ));
        }

        let (host_record, pins) = self.active_host_and_pins(host)?;
        let rec = self
            .lock_db_pub()?
            .get_provider(provider)
            .map_err(|e| storage(&e))?
            .ok_or_else(|| CommandError::new(ErrorCode::NotFound, "provider not found"))?;
        if tool_calling_view(&rec).status != "verified" {
            return Err(CommandError::new(
                ErrorCode::ProviderToolUnsupported,
                "run the tool calling test for this provider before starting a task",
            ));
        }
        let settings = settings_from_record(&rec)?;
        let api_key = self
            .secrets
            .get(&SecretKey::provider_api_key(provider))
            .map_err(|e| CommandError::new(e.code(), e.to_string()))?;

        let auth = self.load_auth(host, &host_record)?;
        let session = connect(ConnectOptions {
            endpoint: Endpoint::new(host_record.hostname.clone(), host_record.port),
            username: host_record.username.clone(),
            auth,
            pinned: pins,
            connect_timeout: Duration::from_secs(u64::from(host_record.connect_timeout_seconds)),
            keepalive: match host_record.keepalive_seconds {
                0 => None,
                s => Some(Duration::from_secs(u64::from(s))),
            },
        })
        .await
        .map_err(|e| {
            let mut out = CommandError::new(e.code(), e.to_string());
            if let SshError::HostKeyUnknown(k) = &e {
                out.details = Some(KeyErrorDetails {
                    algorithm: k.algorithm.clone(),
                    fingerprint_sha256: k.fingerprint_sha256.clone(),
                    pinned_fingerprint_sha256: None,
                });
            }
            out
        })?;

        let task = TaskId::new();
        let token = self.agent.register_task(task);
        let store = DbStore::new(Arc::clone(&self.db));
        let llm = Arc::clone(&self.llm);
        let registry = Arc::clone(&self.agent);
        let session = Arc::new(session);
        let sink: EventSink = Arc::new(move |event| on_event(wire_event(task, event)));
        let spec = TaskSpec {
            task,
            host,
            host_label: host_record.label.clone(),
            provider: Some(provider),
            settings,
            api_key,
            prompt: prompt.to_owned(),
            limits: Limits::default(),
        };
        tokio::spawn(async move {
            let executor = SessionExecutor {
                session: Arc::clone(&session),
            };
            let waiter = RegistryWaiter {
                registry: Arc::clone(&registry),
            };
            run_task(spec, &store, &llm, &executor, &waiter, sink, token).await;
            // The executor holds a clone of the session; release it first so the
            // session is the last owner and can be closed cleanly.
            drop(executor);
            registry.finish_task(task);
            if let Ok(s) = Arc::try_unwrap(session) {
                let _ = s.disconnect().await;
            }
        });
        Ok(task.to_string())
    }

    pub fn agent_cancel(&self, task_id: &str) -> Result<bool, CommandError> {
        let task: TaskId = task_id
            .parse()
            .map_err(|_| CommandError::new(ErrorCode::InvalidInput, "invalid task id"))?;
        Ok(self.agent.cancel_task(task))
    }

    /// Records the operator's decision in the database first, then wakes the task.
    /// An expired or already-decided ticket is refused by the database.
    pub fn approval_decide(&self, approval_id: &str, approve: bool) -> Result<(), CommandError> {
        let id: ApprovalId = approval_id
            .parse()
            .map_err(|_| CommandError::new(ErrorCode::InvalidInput, "invalid approval id"))?;
        let decision = if approve {
            ApprovalDecision::Approve
        } else {
            ApprovalDecision::Deny
        };
        self.lock_db_pub()?
            .decide_approval(id, decision, now_ms())
            .map_err(|e| storage(&e))?;
        self.agent.deliver(id, decision);
        Ok(())
    }

    pub fn history_list(
        &self,
        host_id: &str,
        cursor: Option<String>,
    ) -> Result<Page<ExecutionView>, CommandError> {
        let host: HostId = host_id
            .parse()
            .map_err(|_| CommandError::new(ErrorCode::InvalidInput, "invalid host id"))?;
        let db = self.lock_db_pub()?;
        let page = db
            .host_command_history(host, &PageRequest { cursor, limit: 50 })
            .map_err(|e| storage(&e))?;
        Ok(Page {
            items: page.items.into_iter().map(execution_view).collect(),
            next_cursor: page.next_cursor,
        })
    }

    pub fn audit_verify(&self) -> Result<AuditVerifyView, CommandError> {
        let db: std::sync::MutexGuard<'_, Database> = self.lock_db_pub()?;
        match db.verify_audit_chain().map_err(|e| storage(&e))? {
            Ok(count) => Ok(AuditVerifyView {
                ok: true,
                events_checked: count,
                broken_at_seq: None,
            }),
            Err(err) => {
                let seq = match err {
                    selai_audit::ChainError::Broken { seq }
                    | selai_audit::ChainError::Sequence { seq } => seq,
                };
                Ok(AuditVerifyView {
                    ok: false,
                    events_checked: 0,
                    broken_at_seq: Some(seq),
                })
            }
        }
    }
}

/// Converts a task event into the wire shape, tagged with its task.
fn wire_event(task: TaskId, event: TaskEvent) -> AgentEventView {
    AgentEventView {
        task_id: task.to_string(),
        event: event_view(event),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Arc<AgentRegistry> {
        Arc::new(AgentRegistry::default())
    }

    #[tokio::test]
    async fn a_decision_delivered_before_the_wait_is_not_lost() {
        let registry = registry();
        let approval = ApprovalId::new();
        registry.deliver(approval, ApprovalDecision::Approve);

        let waiter = RegistryWaiter {
            registry: Arc::clone(&registry),
        };
        let outcome = waiter
            .wait(approval, Duration::from_secs(5), &CancellationToken::new())
            .await;
        assert_eq!(outcome, WaitOutcome::Decided(ApprovalDecision::Approve));
    }

    #[tokio::test]
    async fn a_decision_delivered_during_the_wait_wakes_it() {
        let registry = registry();
        let approval = ApprovalId::new();
        let waiter = RegistryWaiter {
            registry: Arc::clone(&registry),
        };
        let handle = tokio::spawn({
            let registry = Arc::clone(&registry);
            async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                registry.deliver(approval, ApprovalDecision::Deny);
            }
        });
        let outcome = waiter
            .wait(approval, Duration::from_secs(5), &CancellationToken::new())
            .await;
        handle.await.expect("deliver task");
        assert_eq!(outcome, WaitOutcome::Decided(ApprovalDecision::Deny));
    }

    #[tokio::test]
    async fn no_decision_times_out_and_leaves_no_waiter_behind() {
        let registry = registry();
        let approval = ApprovalId::new();
        let waiter = RegistryWaiter {
            registry: Arc::clone(&registry),
        };
        let outcome = waiter
            .wait(
                approval,
                Duration::from_millis(10),
                &CancellationToken::new(),
            )
            .await;
        assert_eq!(outcome, WaitOutcome::TimedOut);
        assert!(registry.approvals.lock().expect("lock").waiters.is_empty());
    }
}
