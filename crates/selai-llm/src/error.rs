use selai_domain::{ErrorCode, SelaiError};

/// Provider failure with a stable code and a message that is safe to show.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct LlmError {
    pub code: ErrorCode,
    pub message: String,
}

impl LlmError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ProviderProtocolError, message)
    }
}

impl SelaiError for LlmError {
    fn code(&self) -> ErrorCode {
        self.code
    }
}

/// Removes the API key from text that may come back from a provider, and
/// truncates it. Provider error bodies should not contain the key, but this is
/// the last line of defence before a message reaches the UI or a log.
pub fn sanitize(text: &str, api_key: Option<&str>) -> String {
    let mut out = match api_key.filter(|k| !k.is_empty()) {
        Some(key) => text.replace(key, "[redacted]"),
        None => text.to_owned(),
    };
    if out.chars().count() > 300 {
        out = out.chars().take(300).collect::<String>() + "…";
    }
    out
}
