//! Agent runtime for one scoped task (ADR-0005).
//!
//! A task is bound to one host at creation. The model can call two tools. Each
//! call is validated, recorded in the audit chain before any network I/O, checked
//! by policy, and, for free-form commands, gated on an approval that is re-read
//! from the database before the command runs. The runtime holds no credentials:
//! SSH access arrives as a [`RemoteExecutor`] for the task's host.

mod ports;
mod runner;
mod store;
mod tools;

pub use ports::{ApprovalWaiter, EventSink, RemoteExecutor, TaskEvent, WaitOutcome};
pub use runner::{run_task, Limits, TaskOutcome, TaskSpec};
pub use store::{AgentStore, DbStore};
pub use tools::{tool_definitions, COMMAND_TOOL, DIAGNOSTIC_TOOL};
