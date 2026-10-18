//! SQLite persistence for Selai: connection setup, migrations, startup
//! reconciliation and error mapping. Host and provider credential values stay
//! in the OS credential store; chat transcripts contain user-provided text.

mod agent;
mod chat_sessions;
mod db;
mod error;
mod history;
mod hosts;
mod migrate;
mod network;
mod providers;
mod recovery;
mod ssh_keys;
mod vault_credentials;

#[cfg(test)]
mod tests;

pub use agent::{
    ApprovalDecision, ApprovalRecord, ExecutionOutcome, ExecutionRecord, NewApproval, NewExecution,
    NewTask, TaskRecord, PROMPT_PREVIEW_MAX,
};
pub use chat_sessions::ChatSessionRecord;
pub use db::Database;
pub use error::StorageError;
pub use history::{HistoryFilter, HostUsageSummary, PurgeScope};
pub use hosts::{GroupRecord, HostRecord, PinOutcome, PinnedKey, LOCAL_HOST_ID};
pub use migrate::{Migration, MIGRATIONS};
pub use providers::ProviderRecord;
pub use recovery::ReconcileReport;
pub use ssh_keys::SshKeyRecord;
