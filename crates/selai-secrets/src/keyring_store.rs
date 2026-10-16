use keyring::{Entry, Error as KeyringError};

use crate::{SecretError, SecretKey, SecretStore, SecretValue, SERVICE};

/// The operating-system credential store: Keychain Services (macOS),
/// Credential Manager (Windows), Secret Service over D-Bus (Linux).
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringStore;

impl KeyringStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(key: &SecretKey) -> Result<Entry, SecretError> {
        Entry::new(SERVICE, key.account()).map_err(map_error)
    }
}

fn map_error(err: KeyringError) -> SecretError {
    match err {
        KeyringError::NoStorageAccess(_) => SecretError::Locked,
        other => SecretError::Unavailable(other.to_string()),
    }
}

impl SecretStore for KeyringStore {
    fn put(&self, key: &SecretKey, value: &SecretValue) -> Result<(), SecretError> {
        Self::entry(key)?
            .set_secret(value.expose_for_transport())
            .map_err(map_error)
    }

    fn get(&self, key: &SecretKey) -> Result<Option<SecretValue>, SecretError> {
        match Self::entry(key)?.get_secret() {
            Ok(bytes) => Ok(Some(SecretValue::new(bytes))),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(err) => Err(map_error(err)),
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(err) => Err(map_error(err)),
        }
    }
}
