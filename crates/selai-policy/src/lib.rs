//! Policy for agent tool calls (ADR-0005).
//!
//! Pure logic: no I/O. It decides whether a call is allowed, needs approval, or
//! is denied, and it turns model arguments into validated requests. The model
//! never supplies a host; every request carries the task's bound host.

mod diagnostics;
mod request;

use selai_domain::{ErrorCode, HostId, TaskId};

pub use diagnostics::{diagnostic_command, DiagnosticRequest, LogLines, ServiceName};
pub use request::{parse_model_call, CommandRequest, ModelCallError, ToolRequest};

/// Stored with every decision so a later change to the rules is visible in history.
pub const POLICY_VERSION: &str = "policy-v1";
/// Approval tickets expire after this many milliseconds (spec section 9).
pub const APPROVAL_TTL_MS: i64 = 5 * 60 * 1000;
pub const MAX_COMMAND_BYTES: usize = 4000;
pub const MAX_COMMAND_TIMEOUT_SECS: u32 = 60;

/// The scope a task was started with. Immutable for the life of the task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskScope {
    pub task_id: TaskId,
    pub host: HostId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    NeedsApproval,
    Deny(ErrorCode),
}

impl Decision {
    /// Text stored in `command_executions.policy_decision`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::NeedsApproval => "needs_approval",
            Self::Deny(_) => "deny",
        }
    }
}

/// Decides one proposed tool call against its task's scope.
///
/// Rules, in order:
/// 1. A request for any host other than the task's bound host is denied.
/// 2. Fixed diagnostics run without approval.
/// 3. Free-form commands always need approval (no trust grants in v1), and so
///    do root commands, which are free-form commands run through `sudo`.
pub fn evaluate(scope: &TaskScope, request: &ToolRequest) -> Decision {
    if request.host() != scope.host {
        return Decision::Deny(ErrorCode::TaskScopeInvalid);
    }
    match request {
        ToolRequest::Diagnostic { .. } => Decision::Allow,
        ToolRequest::Command { .. } => Decision::NeedsApproval,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> TaskScope {
        TaskScope {
            task_id: TaskId::new(),
            host: HostId::new(),
        }
    }

    #[test]
    fn diagnostics_on_the_bound_host_run_without_approval() {
        let s = scope();
        let req = ToolRequest::Diagnostic {
            host: s.host,
            request: DiagnosticRequest::DiskUsage,
        };
        assert_eq!(evaluate(&s, &req), Decision::Allow);
    }

    #[test]
    fn commands_always_need_approval() {
        let s = scope();
        let req = ToolRequest::Command {
            host: s.host,
            request: CommandRequest::parse("uname -a", 10).expect("valid"),
        };
        assert_eq!(evaluate(&s, &req), Decision::NeedsApproval);
    }

    #[test]
    fn a_request_for_another_host_is_denied_whatever_the_tool() {
        let s = scope();
        let other = HostId::new();
        let diag = ToolRequest::Diagnostic {
            host: other,
            request: DiagnosticRequest::Uptime,
        };
        let cmd = ToolRequest::Command {
            host: other,
            request: CommandRequest::parse("id", 5).expect("valid"),
        };
        assert_eq!(
            evaluate(&s, &diag),
            Decision::Deny(ErrorCode::TaskScopeInvalid)
        );
        assert_eq!(
            evaluate(&s, &cmd),
            Decision::Deny(ErrorCode::TaskScopeInvalid)
        );
    }
}
