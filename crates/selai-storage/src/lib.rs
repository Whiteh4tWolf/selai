//! SQLite persistence for Selai: connection setup, migrations, startup
//! reconciliation and error mapping. Secret material never reaches this crate.

mod agent;
mod db;
mod error;
mod history;
mod hosts;
mod migrate;
mod providers;
mod recovery;

#[cfg(test)]
mod tests;

pub use agent::{
    ApprovalDecision, ApprovalRecord, ExecutionOutcome, ExecutionRecord, NewApproval, NewExecution,
    NewTask, TaskRecord,
};
pub use db::Database;
pub use error::StorageError;
pub use history::{HistoryFilter, PurgeScope};
pub use hosts::{GroupRecord, HostRecord, PinOutcome, PinnedKey};
pub use migrate::{Migration, MIGRATIONS};
pub use providers::ProviderRecord;
pub use recovery::ReconcileReport;
