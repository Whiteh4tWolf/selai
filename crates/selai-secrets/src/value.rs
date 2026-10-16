use zeroize::Zeroize;

/// Secret bytes that are wiped from memory when dropped.
///
/// Deliberately has no `Debug`, `Display`, `Clone`, `PartialEq` or `Serialize`
/// implementation, so it cannot be logged, copied silently, compared with
/// timing leaks, or returned to the webview. Only transport code calls
/// [`SecretValue::expose_for_transport`].
pub struct SecretValue {
    bytes: Vec<u8>,
}

impl SecretValue {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Raw bytes for the SSH or HTTP client that needs them. Do not store or log
    /// the returned slice.
    pub fn expose_for_transport(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}
