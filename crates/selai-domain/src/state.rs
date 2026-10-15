//! State machines for tasks, command executions and approvals
//! (engineering spec sections 6.1, 6.4 and 8).
//!
//! Terminal states are final: nothing is ever resumed or replayed after
//! `interrupted`, `cancelled`, etc. The same rules are mirrored by SQL
//! triggers in storage migration 0002 as defence in depth.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("illegal transition {from} -> {to}")]
pub struct TransitionError {
    pub from: &'static str,
    pub to: &'static str,
}

impl crate::SelaiError for TransitionError {
    fn code(&self) -> crate::ErrorCode {
        crate::ErrorCode::Internal
    }
}

macro_rules! state_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$(Self::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($text => Some(Self::$variant),)+ _ => None }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

state_enum!(
    /// Canonical task states. `denied` is an execution result, not a task state.
    TaskState {
        Created => "created",
        ValidatingScope => "validating_scope",
        Ready => "ready",
        Running => "running",
        AwaitingApproval => "awaiting_approval",
        Completed => "completed",
        Failed => "failed",
        Cancelled => "cancelled",
        Interrupted => "interrupted",
    }
);

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use TaskState::*;
        if self.is_terminal() {
            return false;
        }
        match (self, next) {
            (Created, ValidatingScope) => true,
            (ValidatingScope, Ready) => true,
            (Ready, Running) => true,
            (Running, AwaitingApproval) | (AwaitingApproval, Running) => true,
            (Running, Completed) => true,
            // Any live state may fail, be cancelled, or be interrupted by a
            // crash. Interruption is how startup recovery closes stale tasks.
            (_, Failed | Cancelled | Interrupted) => true,
            _ => false,
        }
    }

    pub fn transition(self, next: Self) -> Result<Self, TransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(TransitionError {
                from: self.as_str(),
                to: next.as_str(),
            })
        }
    }
}

state_enum!(
    /// Status of one AI command attempt (`command_executions.state`).
    ExecStatus {
        Proposed => "proposed",
        AwaitingApproval => "awaiting_approval",
        Denied => "denied",
        Running => "running",
        Completed => "completed",
        Failed => "failed",
        TimedOut => "timed_out",
        Cancelled => "cancelled",
        Interrupted => "interrupted",
    }
);

impl ExecStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(
            self,
            Self::Proposed | Self::AwaitingApproval | Self::Running
        )
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use ExecStatus::*;
        if self.is_terminal() {
            return false;
        }
        match (self, next) {
            // Bounded diagnostics run without approval: proposed -> running.
            (Proposed, AwaitingApproval | Running | Denied) => true,
            (AwaitingApproval, Running | Denied) => true,
            (Running, Completed | Failed | TimedOut) => true,
            (_, Cancelled | Interrupted) => true,
            // A proposal can fail before it ever runs (e.g. audit or scope error).
            (Proposed | AwaitingApproval, Failed) => true,
            _ => false,
        }
    }

    pub fn transition(self, next: Self) -> Result<Self, TransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(TransitionError {
                from: self.as_str(),
                to: next.as_str(),
            })
        }
    }
}

state_enum!(
    /// Status of a one-time approval ticket.
    ApprovalState {
        Pending => "pending",
        Approved => "approved",
        Denied => "denied",
        Expired => "expired",
        Cancelled => "cancelled",
    }
);

impl ApprovalState {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use ApprovalState::*;
        matches!(
            (self, next),
            (Pending, Approved | Denied | Expired | Cancelled)
                // An approved ticket that was never consumed can still be
                // withdrawn (task cancelled, app restarted).
                | (Approved, Cancelled | Expired)
        )
    }

    pub fn transition(self, next: Self) -> Result<Self, TransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(TransitionError {
                from: self.as_str(),
                to: next.as_str(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_forms_roundtrip_and_match_serde() {
        for s in TaskState::ALL {
            assert_eq!(TaskState::parse(s.as_str()), Some(*s));
            assert_eq!(serde_json::to_string(s).expect("ser"), format!("\"{s}\""));
        }
        for s in ExecStatus::ALL {
            assert_eq!(ExecStatus::parse(s.as_str()), Some(*s));
            assert_eq!(serde_json::to_string(s).expect("ser"), format!("\"{s}\""));
        }
        for s in ApprovalState::ALL {
            assert_eq!(ApprovalState::parse(s.as_str()), Some(*s));
        }
        assert_eq!(TaskState::parse("denied"), None);
    }

    #[test]
    fn task_happy_path_and_approval_loop() {
        let mut s = TaskState::Created;
        for next in [
            TaskState::ValidatingScope,
            TaskState::Ready,
            TaskState::Running,
            TaskState::AwaitingApproval,
            TaskState::Running,
            TaskState::Completed,
        ] {
            s = s.transition(next).expect("legal");
        }
        assert!(s.is_terminal());
    }

    #[test]
    fn task_cannot_skip_or_leave_terminal_states() {
        assert!(TaskState::Created.transition(TaskState::Running).is_err());
        assert!(TaskState::Ready.transition(TaskState::Completed).is_err());
        for t in TaskState::ALL.iter().filter(|t| t.is_terminal()) {
            for n in TaskState::ALL {
                assert!(!t.can_transition_to(*n), "{t} -> {n} must be illegal");
            }
        }
    }

    #[test]
    fn every_live_task_state_can_be_interrupted() {
        for s in TaskState::ALL.iter().filter(|s| !s.is_terminal()) {
            assert!(s.can_transition_to(TaskState::Interrupted));
        }
    }

    #[test]
    fn exec_terminal_states_are_final() {
        for t in ExecStatus::ALL.iter().filter(|t| t.is_terminal()) {
            for n in ExecStatus::ALL {
                assert!(!t.can_transition_to(*n), "{t} -> {n} must be illegal");
            }
        }
    }

    #[test]
    fn exec_cannot_rewind() {
        assert!(!ExecStatus::Running.can_transition_to(ExecStatus::Proposed));
        assert!(!ExecStatus::Running.can_transition_to(ExecStatus::AwaitingApproval));
        assert!(!ExecStatus::AwaitingApproval.can_transition_to(ExecStatus::Proposed));
        assert!(!ExecStatus::Proposed.can_transition_to(ExecStatus::Completed));
    }

    #[test]
    fn exec_paths() {
        assert!(ExecStatus::Proposed.can_transition_to(ExecStatus::Running));
        assert!(ExecStatus::Proposed.can_transition_to(ExecStatus::AwaitingApproval));
        assert!(ExecStatus::AwaitingApproval.can_transition_to(ExecStatus::Denied));
        assert!(ExecStatus::Running.can_transition_to(ExecStatus::TimedOut));
        for s in ExecStatus::ALL.iter().filter(|s| !s.is_terminal()) {
            assert!(s.can_transition_to(ExecStatus::Interrupted));
            assert!(s.can_transition_to(ExecStatus::Cancelled));
        }
    }

    #[test]
    fn approval_is_single_use() {
        assert!(ApprovalState::Pending.can_transition_to(ApprovalState::Approved));
        assert!(!ApprovalState::Denied.can_transition_to(ApprovalState::Approved));
        assert!(!ApprovalState::Expired.can_transition_to(ApprovalState::Approved));
        assert!(!ApprovalState::Cancelled.can_transition_to(ApprovalState::Approved));
        assert!(!ApprovalState::Approved.can_transition_to(ApprovalState::Denied));
        assert!(ApprovalState::Approved.can_transition_to(ApprovalState::Cancelled));
    }
}
