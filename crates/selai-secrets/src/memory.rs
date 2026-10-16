use std::{collections::HashMap, sync::Mutex};

use crate::{SecretError, SecretKey, SecretStore, SecretValue};

/// In-process store with the same contract as the OS store. Used by tests in
/// other crates. It can be told to fail, so error paths are testable.
#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<HashMap<String, Vec<u8>>>,
    failure: Mutex<Option<SecretError>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every subsequent call fail with `error`, to simulate a locked store.
    pub fn fail_all_with(&self, error: SecretError) {
        if let Ok(mut slot) = self.failure.lock() {
            *slot = Some(error);
        }
    }

    pub fn clear_failure(&self) {
        if let Ok(mut slot) = self.failure.lock() {
            *slot = None;
        }
    }

    /// Number of stored entries. Used by tests to check cleanup.
    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn check(&self) -> Result<(), SecretError> {
        let slot = self
            .failure
            .lock()
            .map_err(|_| SecretError::Unavailable("memory store lock poisoned".to_owned()))?;
        match slot.as_ref() {
            Some(err) => Err(err.clone()),
            None => Ok(()),
        }
    }
}

impl SecretStore for MemoryStore {
    fn put(&self, key: &SecretKey, value: &SecretValue) -> Result<(), SecretError> {
        self.check()?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| SecretError::Unavailable("memory store lock poisoned".to_owned()))?;
        entries.insert(
            key.account().to_owned(),
            value.expose_for_transport().to_vec(),
        );
        Ok(())
    }

    fn get(&self, key: &SecretKey) -> Result<Option<SecretValue>, SecretError> {
        self.check()?;
        let entries = self
            .entries
            .lock()
            .map_err(|_| SecretError::Unavailable("memory store lock poisoned".to_owned()))?;
        Ok(entries
            .get(key.account())
            .map(|b| SecretValue::new(b.clone())))
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.check()?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| SecretError::Unavailable("memory store lock poisoned".to_owned()))?;
        entries.remove(key.account());
        Ok(())
    }
}
