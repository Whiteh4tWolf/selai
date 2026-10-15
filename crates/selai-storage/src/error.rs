use selai_domain::{ErrorCode, SelaiError};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database is busy or locked by another connection")]
    Busy,
    #[error("constraint violated: {0}")]
    Constraint(String),
    #[error("storage I/O error: {0}")]
    Io(String),
    #[error("migration {version} failed: {message}")]
    Migration { version: i64, message: String },
    #[error("database schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("required SQLite setting `{0}` could not be applied")]
    Pragma(&'static str),
}

impl SelaiError for StorageError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Busy => ErrorCode::StorageBusy,
            Self::Constraint(_) => ErrorCode::StorageConstraint,
            Self::Io(_) | Self::Pragma(_) => ErrorCode::StorageIo,
            Self::Migration { .. } | Self::SchemaTooNew { .. } => ErrorCode::StorageMigrationFailed,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(err: rusqlite::Error) -> Self {
        match &err {
            rusqlite::Error::SqliteFailure(failure, message) => match failure.code {
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                    Self::Busy
                }
                rusqlite::ErrorCode::ConstraintViolation => {
                    Self::Constraint(message.clone().unwrap_or_else(|| failure.to_string()))
                }
                _ => Self::Io(err.to_string()),
            },
            _ => Self::Io(err.to_string()),
        }
    }
}
