//! Stable machine-readable error codes (engineering spec section 13).

use serde::{Deserialize, Serialize};

/// Stable error codes. The serialized form is `SCREAMING_SNAKE_CASE` and must
/// not change between releases.
///
/// The first block is the taxonomy fixed by the v1 spec. The `Storage*` and
/// `Invalid*`/`NotFound` variants extend it; see ADR-0001.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    SshDns,
    SshConnectTimeout,
    SshHostkeyUnknown,
    SshHostkeyChanged,
    SshAuthFailed,
    SshChannelClosed,
    SshExitStatusMissing,
    SecretStoreLocked,
    SecretStoreUnavailable,
    ProviderUnauthorized,
    ProviderRateLimit,
    ProviderTimeout,
    ProviderToolUnsupported,
    ProviderProtocolError,
    TaskScopeInvalid,
    TaskCancelled,
    TaskBudgetExceeded,
    PolicyDenied,
    ApprovalExpired,
    SudoPasswordRequired,
    SudoAuthFailed,
    SudoNotAllowed,
    AuditWriteFailed,
    StorageMigrationFailed,
    // Extensions to the spec taxonomy.
    TaskInterrupted,
    SshConnectFailed,
    StorageBusy,
    StorageConflict,
    StorageConstraint,
    StorageIo,
    InvalidInput,
    NotFound,
    Internal,
}

impl ErrorCode {
    /// The wire string, identical to the serde representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SshDns => "SSH_DNS",
            Self::SshConnectTimeout => "SSH_CONNECT_TIMEOUT",
            Self::SshHostkeyUnknown => "SSH_HOSTKEY_UNKNOWN",
            Self::SshHostkeyChanged => "SSH_HOSTKEY_CHANGED",
            Self::SshAuthFailed => "SSH_AUTH_FAILED",
            Self::SshChannelClosed => "SSH_CHANNEL_CLOSED",
            Self::SshExitStatusMissing => "SSH_EXIT_STATUS_MISSING",
            Self::SecretStoreLocked => "SECRET_STORE_LOCKED",
            Self::SecretStoreUnavailable => "SECRET_STORE_UNAVAILABLE",
            Self::ProviderUnauthorized => "PROVIDER_UNAUTHORIZED",
            Self::ProviderRateLimit => "PROVIDER_RATE_LIMIT",
            Self::ProviderTimeout => "PROVIDER_TIMEOUT",
            Self::ProviderToolUnsupported => "PROVIDER_TOOL_UNSUPPORTED",
            Self::ProviderProtocolError => "PROVIDER_PROTOCOL_ERROR",
            Self::TaskScopeInvalid => "TASK_SCOPE_INVALID",
            Self::TaskCancelled => "TASK_CANCELLED",
            Self::TaskBudgetExceeded => "TASK_BUDGET_EXCEEDED",
            Self::PolicyDenied => "POLICY_DENIED",
            Self::ApprovalExpired => "APPROVAL_EXPIRED",
            Self::SudoPasswordRequired => "SUDO_PASSWORD_REQUIRED",
            Self::SudoAuthFailed => "SUDO_AUTH_FAILED",
            Self::SudoNotAllowed => "SUDO_NOT_ALLOWED",
            Self::AuditWriteFailed => "AUDIT_WRITE_FAILED",
            Self::StorageMigrationFailed => "STORAGE_MIGRATION_FAILED",
            Self::TaskInterrupted => "TASK_INTERRUPTED",
            Self::SshConnectFailed => "SSH_CONNECT_FAILED",
            Self::StorageBusy => "STORAGE_BUSY",
            Self::StorageConflict => "STORAGE_CONFLICT",
            Self::StorageConstraint => "STORAGE_CONSTRAINT",
            Self::StorageIo => "STORAGE_IO",
            Self::InvalidInput => "INVALID_INPUT",
            Self::NotFound => "NOT_FOUND",
            Self::Internal => "INTERNAL",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Implemented by every error type that crosses a crate or IPC boundary.
pub trait SelaiError: std::error::Error {
    fn code(&self) -> ErrorCode;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_for_every_variant() {
        let all = [
            ErrorCode::SshDns,
            ErrorCode::SshConnectTimeout,
            ErrorCode::SshHostkeyUnknown,
            ErrorCode::SshHostkeyChanged,
            ErrorCode::SshAuthFailed,
            ErrorCode::SshChannelClosed,
            ErrorCode::SshExitStatusMissing,
            ErrorCode::SecretStoreLocked,
            ErrorCode::SecretStoreUnavailable,
            ErrorCode::ProviderUnauthorized,
            ErrorCode::ProviderRateLimit,
            ErrorCode::ProviderTimeout,
            ErrorCode::ProviderToolUnsupported,
            ErrorCode::ProviderProtocolError,
            ErrorCode::TaskScopeInvalid,
            ErrorCode::TaskCancelled,
            ErrorCode::TaskBudgetExceeded,
            ErrorCode::PolicyDenied,
            ErrorCode::ApprovalExpired,
            ErrorCode::SudoPasswordRequired,
            ErrorCode::SudoAuthFailed,
            ErrorCode::SudoNotAllowed,
            ErrorCode::AuditWriteFailed,
            ErrorCode::StorageMigrationFailed,
            ErrorCode::TaskInterrupted,
            ErrorCode::SshConnectFailed,
            ErrorCode::StorageBusy,
            ErrorCode::StorageConflict,
            ErrorCode::StorageConstraint,
            ErrorCode::StorageIo,
            ErrorCode::InvalidInput,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ];
        for code in all {
            let json = serde_json::to_string(&code).expect("serialize");
            assert_eq!(json, format!("\"{}\"", code.as_str()), "{code:?}");
        }
    }
}
