//! Limits apply to metadata as well as streamed file contents.
use crate::{
    dto::CommandError,
    network_service::{failed, invalid},
};
use std::path::Path;

pub(crate) struct MetadataBudget {
    entries: usize,
    bytes: usize,
    max_entries: usize,
    max_bytes: usize,
}

impl MetadataBudget {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: 0,
            bytes: 0,
            max_entries,
            max_bytes,
        }
    }
    pub fn claim(&mut self, bytes: usize) -> Result<(), CommandError> {
        let total = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| invalid("File metadata is too large."))?;
        if self.entries >= self.max_entries || total > self.max_bytes {
            return Err(invalid("Too many files or too much path metadata. Choose a smaller folder or split the transfer."));
        }
        self.entries += 1;
        self.bytes = total;
        Ok(())
    }
}

pub(crate) fn check_space(path: &Path, required: u64) -> Result<(), CommandError> {
    let available = fs2::available_space(path).map_err(failed)?;
    // This is a preflight, not a reservation; write errors are still handled.
    if available < required.saturating_add(8 * 1024 * 1024) {
        return Err(invalid(
            "Not enough free disk space for the temporary copy. Free some space and retry.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn count_and_bytes_are_limited_independently_without_overflow() {
        let mut budget = MetadataBudget::new(3, 100);
        assert!(budget.claim(80).is_ok());
        assert!(budget.claim(21).is_err());
        assert!(budget.claim(10).is_ok());
        assert!(budget.claim(10).is_ok());
        assert!(budget.claim(0).is_err());
        assert!(MetadataBudget::new(3, 100).claim(usize::MAX).is_err());
    }
}
