//! Validated provider settings (engineering spec section 4.1, ADR-0004).

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, SelaiError};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderFieldError {
    #[error("provider name must be 1 to 64 characters without control characters")]
    Name,
    #[error("model ID must be 1 to 200 printable characters without spaces")]
    ModelId,
    #[error("base URL must be https, http to a loopback address, or http with \"Allow plain HTTP\" turned on")]
    BaseUrlScheme,
    #[error("base URL must not contain credentials, a query, or a fragment")]
    BaseUrlShape,
    #[error("base URL is not a valid URL")]
    BaseUrlParse,
    #[error("timeout must be between 5 and 600 seconds")]
    Timeout,
    #[error("max output tokens must be between 1 and 200000")]
    MaxOutputTokens,
}

impl SelaiError for ProviderFieldError {
    fn code(&self) -> ErrorCode {
        ErrorCode::InvalidInput
    }
}

/// Wire protocol. The strings match the `provider_profiles.protocol` CHECK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProtocol {
    OpenAiResponses,
    OpenAiChat,
    AnthropicMessages,
}

impl ProviderProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai_responses",
            Self::OpenAiChat => "openai_chat",
            Self::AnthropicMessages => "anthropic_messages",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "openai_responses" => Some(Self::OpenAiResponses),
            "openai_chat" => Some(Self::OpenAiChat),
            "anthropic_messages" => Some(Self::AnthropicMessages),
            _ => None,
        }
    }
}

/// Display name, 1 to 64 characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderName(String);

impl ProviderName {
    pub fn parse(input: &str) -> Result<Self, ProviderFieldError> {
        let trimmed = input.trim();
        let len = trimmed.chars().count();
        if (1..=64).contains(&len) && !trimmed.chars().any(char::is_control) {
            Ok(Self(trimmed.to_owned()))
        } else {
            Err(ProviderFieldError::Name)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Model identifier as the provider names it. Never hardcoded by Selai.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelId(String);

impl ModelId {
    pub fn parse(input: &str) -> Result<Self, ProviderFieldError> {
        let ok =
            (1..=200).contains(&input.len()) && input.bytes().all(|b| (0x21..=0x7e).contains(&b));
        if ok {
            Ok(Self(input.to_owned()))
        } else {
            Err(ProviderFieldError::ModelId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// API root for a provider, without a trailing slash and without the endpoint
/// suffix. For example `https://api.openai.com/v1` or `http://127.0.0.1:8000/v1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl(String);

impl BaseUrl {
    /// Accepts https anywhere and plain http only on this machine.
    pub fn parse(input: &str) -> Result<Self, ProviderFieldError> {
        Self::parse_with(input, false)
    }

    /// Like [`BaseUrl::parse`], but with `allow_insecure_http` plain http is also
    /// accepted for other hosts, such as a LiteLLM proxy on a private network. The
    /// operator opts in per provider; the API key then travels unencrypted.
    pub fn parse_with(input: &str, allow_insecure_http: bool) -> Result<Self, ProviderFieldError> {
        let url = url::Url::parse(input.trim()).map_err(|_| ProviderFieldError::BaseUrlParse)?;
        match url.scheme() {
            "https" => {}
            "http" if allow_insecure_http || is_loopback(&url) => {}
            _ => return Err(ProviderFieldError::BaseUrlScheme),
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.host_str().is_none()
        {
            return Err(ProviderFieldError::BaseUrlShape);
        }
        Ok(Self(url.as_str().trim_end_matches('/').to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether requests go over plain http (unencrypted).
    pub fn is_plain_http(&self) -> bool {
        self.0.starts_with("http://")
    }

    /// Whether the endpoint is on this machine. Only then may a request omit the key.
    pub fn is_loopback(&self) -> bool {
        url::Url::parse(&self.0)
            .map(|u| is_loopback(&u))
            .unwrap_or(false)
    }
}

fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
        None => false,
    }
}

/// Request timeout in seconds, 5 to 600.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutSecs(u32);

impl TimeoutSecs {
    pub const DEFAULT: Self = Self(120);

    pub fn new(seconds: u32) -> Result<Self, ProviderFieldError> {
        if (5..=600).contains(&seconds) {
            Ok(Self(seconds))
        } else {
            Err(ProviderFieldError::Timeout)
        }
    }

    pub fn seconds(self) -> u32 {
        self.0
    }
}

/// Output budget per turn, 1 to 200000 tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxOutputTokens(u32);

impl MaxOutputTokens {
    pub const DEFAULT: Self = Self(4096);

    pub fn new(tokens: u32) -> Result<Self, ProviderFieldError> {
        if (1..=200_000).contains(&tokens) {
            Ok(Self(tokens))
        } else {
            Err(ProviderFieldError::MaxOutputTokens)
        }
    }

    pub fn tokens(self) -> u32 {
        self.0
    }
}

/// Everything a user can set on a provider. The API key is not part of this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSettings {
    pub name: ProviderName,
    pub protocol: ProviderProtocol,
    pub base_url: BaseUrl,
    pub model_id: ModelId,
    pub timeout: TimeoutSecs,
    pub max_output_tokens: MaxOutputTokens,
    /// Operator opt-in for plain http to a host other than this machine.
    pub allow_insecure_http: bool,
}

impl ProviderSettings {
    /// Identity of the endpoint and model. A tool-calling result is valid only
    /// for the identity it was measured against.
    pub fn probe_identity(&self) -> String {
        format!(
            "{}|{}|{}",
            self.protocol.as_str(),
            self.base_url.as_str(),
            self.model_id.as_str()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_accepts_https_and_loopback_http_only() {
        assert!(BaseUrl::parse("https://api.openai.com/v1").is_ok());
        assert!(BaseUrl::parse("http://127.0.0.1:8000/v1").is_ok());
        assert!(BaseUrl::parse("http://localhost:4000").is_ok());
        assert!(BaseUrl::parse("http://[::1]:11434/v1").is_ok());
        assert_eq!(
            BaseUrl::parse("http://api.example.com/v1"),
            Err(ProviderFieldError::BaseUrlScheme)
        );
        assert_eq!(
            BaseUrl::parse("http://10.0.0.5:8000/v1"),
            Err(ProviderFieldError::BaseUrlScheme)
        );
        assert_eq!(
            BaseUrl::parse("ftp://example.com"),
            Err(ProviderFieldError::BaseUrlScheme)
        );
        assert_eq!(
            BaseUrl::parse("not a url"),
            Err(ProviderFieldError::BaseUrlParse)
        );
    }

    #[test]
    fn base_url_rejects_credentials_query_and_fragment() {
        for bad in [
            "https://user:pass@api.example.com/v1",
            "https://api.example.com/v1?key=abc",
            "https://api.example.com/v1#frag",
        ] {
            assert_eq!(
                BaseUrl::parse(bad),
                Err(ProviderFieldError::BaseUrlShape),
                "{bad}"
            );
        }
    }

    #[test]
    fn base_url_is_normalized_and_reports_loopback() {
        let b = BaseUrl::parse("HTTPS://API.Example.com/v1/").expect("ok");
        assert_eq!(b.as_str(), "https://api.example.com/v1");
        assert!(!b.is_loopback());
        assert!(BaseUrl::parse("http://127.0.0.1:1/x")
            .expect("ok")
            .is_loopback());
    }

    #[test]
    fn model_ids_are_printable_without_spaces() {
        assert!(ModelId::parse("gpt-4.1").is_ok());
        assert!(ModelId::parse("org/model:tag").is_ok());
        assert!(ModelId::parse("").is_err());
        assert!(ModelId::parse("two words").is_err());
    }

    #[test]
    fn bounds_are_enforced() {
        assert!(TimeoutSecs::new(4).is_err());
        assert!(TimeoutSecs::new(601).is_err());
        assert!(MaxOutputTokens::new(0).is_err());
        assert!(MaxOutputTokens::new(200_001).is_err());
        assert!(ProviderName::parse("  ").is_err());
    }

    #[test]
    fn protocol_strings_round_trip() {
        for p in [
            ProviderProtocol::OpenAiResponses,
            ProviderProtocol::OpenAiChat,
            ProviderProtocol::AnthropicMessages,
        ] {
            assert_eq!(ProviderProtocol::parse(p.as_str()), Some(p));
        }
        assert_eq!(ProviderProtocol::parse("openai"), None);
    }

    #[test]
    fn plain_http_to_other_hosts_needs_the_opt_in() {
        assert_eq!(
            BaseUrl::parse("http://10.0.0.5:4000/v1"),
            Err(ProviderFieldError::BaseUrlScheme),
            "no opt-in, no plain http to another host"
        );
        let litellm = BaseUrl::parse_with("http://10.0.0.5:4000/v1", true).expect("opt-in");
        assert!(litellm.is_plain_http());
        assert!(!litellm.is_loopback());
        // The opt-in widens http only; other schemes and shapes stay refused.
        assert_eq!(
            BaseUrl::parse_with("ftp://example.com", true),
            Err(ProviderFieldError::BaseUrlScheme)
        );
        assert_eq!(
            BaseUrl::parse_with("http://user:pw@10.0.0.5:4000", true),
            Err(ProviderFieldError::BaseUrlShape)
        );
        let tls = BaseUrl::parse_with("https://api.example.com/v1", true).expect("https");
        assert!(!tls.is_plain_http());
    }
}
