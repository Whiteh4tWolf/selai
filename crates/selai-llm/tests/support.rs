//! Helpers shared by the adapter and probe tests. Mock servers speak the real
//! SSE shapes of each protocol, so the parsers are exercised as in production.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::Duration;

use selai_domain::{
    BaseUrl, MaxOutputTokens, ModelId, ProviderName, ProviderProtocol, ProviderSettings,
    TimeoutSecs,
};
use selai_llm::{ClientOptions, LlmClient};
use selai_secrets::SecretValue;

pub const KEY: &str = "sentinel-key-do-not-leak";

pub fn client() -> LlmClient {
    LlmClient::new(ClientOptions {
        use_system_proxy: false,
        retry_backoff: Duration::from_millis(5),
    })
    .expect("client")
}

pub fn settings(protocol: ProviderProtocol, base: &str) -> ProviderSettings {
    ProviderSettings {
        name: ProviderName::parse("mock").expect("name"),
        protocol,
        base_url: BaseUrl::parse(base).expect("base"),
        model_id: ModelId::parse("mock-model").expect("model"),
        timeout: TimeoutSecs::new(10).expect("timeout"),
        max_output_tokens: MaxOutputTokens::new(256).expect("max"),
        allow_insecure_http: false,
    }
}

pub fn key() -> SecretValue {
    SecretValue::new(KEY)
}

/// Builds an SSE body from JSON payloads, each as a `data:` line.
pub fn sse(payloads: &[serde_json::Value]) -> String {
    payloads.iter().map(|p| format!("data: {p}\n\n")).collect()
}

pub fn sse_raw(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("{l}\n\n")).collect()
}
