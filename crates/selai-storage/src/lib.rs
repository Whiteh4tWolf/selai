//! SQLite persistence for Selai: connection setup, migrations, startup
//! reconciliation and error mapping. Secret material never reaches this crate.

mod db;
mod error;
mod migrate;
mod recovery;

#[cfg(test)]
mod tests;

pub use db::Database;
pub use error::StorageError;
pub use migrate::{Migration, MIGRATIONS};
pub use recovery::ReconcileReport;
