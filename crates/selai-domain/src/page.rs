//! Cursor pagination contract shared by every list API.

use serde::{Deserialize, Serialize};

pub const DEFAULT_PAGE_SIZE: u32 = 50;
pub const MAX_PAGE_SIZE: u32 = 200;

/// Request for one page. The cursor is opaque to callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    pub cursor: Option<String>,
    pub limit: u32,
}

impl PageRequest {
    pub fn first(limit: u32) -> Self {
        Self {
            cursor: None,
            limit,
        }
    }

    /// The limit clamped to `1..=MAX_PAGE_SIZE`; `0` falls back to the default.
    pub fn effective_limit(&self) -> u32 {
        match self.limit {
            0 => DEFAULT_PAGE_SIZE,
            n => n.min(MAX_PAGE_SIZE),
        }
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self::first(DEFAULT_PAGE_SIZE)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    /// `None` when this is the last page.
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_is_clamped() {
        assert_eq!(PageRequest::first(0).effective_limit(), DEFAULT_PAGE_SIZE);
        assert_eq!(PageRequest::first(7).effective_limit(), 7);
        assert_eq!(PageRequest::first(10_000).effective_limit(), MAX_PAGE_SIZE);
    }
}
