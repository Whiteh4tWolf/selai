//! Audit helpers (ADR-0005): redaction, bounded previews and the hash chain.
//!
//! Pure logic. Storage writes the rows. This crate decides what is stored and
//! whether the stored chain is intact.

mod chain;
mod redact;

use sha2::{Digest, Sha256};

pub use chain::{
    canonical_json, event_hash as chain_event_hash, verify_chain, ChainError, ChainEvent, ChainLink,
};
pub use redact::{redact, Redacted};

/// Truncates `text` to at most `max_bytes` without splitting a character.
/// Returns the text and whether anything was cut.
pub fn bounded_preview(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_owned(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}

/// SHA-256 of `bytes`, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_never_splits_a_character() {
        let text = "é".repeat(10); // 20 bytes
        let (cut, truncated) = bounded_preview(&text, 5);
        assert!(truncated);
        assert_eq!(cut, "éé");
        assert_eq!(bounded_preview("ok", 5), ("ok".to_owned(), false));
    }

    #[test]
    fn sha256_matches_a_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
