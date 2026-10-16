//! Secret values and storage contracts for Selai (ADR-0006).
//!
//! The production desktop uses an encrypted database implementation of
//! SecretStore. KeyringStore remains for migration of older installations.
//! There is no plaintext fallback when a store is locked or unavailable.

mod keyring_store;
mod memory;
mod value;

use selai_domain::{ErrorCode, HostId, ProviderId, SelaiError};

pub use keyring_store::KeyringStore;
pub use memory::MemoryStore;
pub use value::SecretValue;

/// Service name shared by every Selai entry in the OS store.
pub const SERVICE: &str = "selai";

/// Kinds of SSH secret. Fixed strings only: they become part of the store key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshSecretKind {
    Password,
    KeyPassphrase,
    /// The password `sudo` asks for on that host, saved only when the operator
    /// chose "Remember for this host".
    SudoPassword,
    PrivateKey,
}

impl SshSecretKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::KeyPassphrase => "key_passphrase",
            Self::SudoPassword => "sudo_password",
            Self::PrivateKey => "private_key",
        }
    }
}

/// Address of one secret in the store. Built only from validated IDs and fixed
/// kind strings, never from user text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretKey {
    account: String,
}

impl SecretKey {
    pub fn from_account(account: &str) -> Option<Self> {
        let parts: Vec<_> = account.split('/').collect();
        let valid = match parts.as_slice() {
            ["ssh", id, kind] => {
                id.parse::<HostId>().is_ok()
                    && ["password", "key_passphrase", "sudo_password", "private_key"].contains(kind)
            }
            ["provider", id, "api_key"] => id.parse::<ProviderId>().is_ok(),
            _ => false,
        };
        valid.then(|| Self {
            account: account.to_owned(),
        })
    }
    pub fn ssh(host: HostId, kind: SshSecretKind) -> Self {
        Self {
            account: format!("ssh/{host}/{}", kind.as_str()),
        }
    }

    pub fn provider_api_key(provider: ProviderId) -> Self {
        Self {
            account: format!("provider/{provider}/api_key"),
        }
    }

    pub fn account(&self) -> &str {
        &self.account
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// The store exists but refuses access (locked keychain, denied prompt).
    #[error("the system credential store is locked or access was denied")]
    Locked,
    /// The store is missing or failed (no Secret Service, backend error).
    #[error("the system credential store is unavailable: {0}")]
    Unavailable(String),
    /// The store returned a value that is not usable as a secret.
    #[error("stored secret is malformed")]
    Malformed,
}

impl SelaiError for SecretError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Locked => ErrorCode::SecretStoreLocked,
            Self::Unavailable(_) | Self::Malformed => ErrorCode::SecretStoreUnavailable,
        }
    }
}

/// Operations on the credential store. Synchronous because the OS APIs are;
/// callers in async code run these on a blocking thread.
pub trait SecretStore: Send + Sync {
    /// Creates or replaces the value at `key`.
    fn put(&self, key: &SecretKey, value: &SecretValue) -> Result<(), SecretError>;

    /// Returns the value, or `None` if nothing is stored under `key`.
    fn get(&self, key: &SecretKey) -> Result<Option<SecretValue>, SecretError>;

    /// Removes the value. Deleting a missing entry succeeds.
    fn delete(&self, key: &SecretKey) -> Result<(), SecretError>;
}
