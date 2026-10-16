//! Live provider checks. They call real endpoints with keys the operator supplies,
//! so they are ignored by default and never run in CI.
//!
//! Run them explicitly:
//!
//!     SELAI_LIVE_OPENAI_KEY=... SELAI_LIVE_OPENAI_MODEL=... \
//!       cargo test -p selai-llm --test live -- --ignored --nocapture
//!
//! A test that is ignored has NOT verified that provider. Report it as "not
//! verified against a live endpoint".

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::Duration;

use selai_domain::{
    BaseUrl, MaxOutputTokens, ModelId, ProviderName, ProviderProtocol, ProviderSettings,
    TimeoutSecs,
};
use selai_llm::{probe_tools, ClientOptions, LlmClient};
use selai_secrets::SecretValue;
use tokio_util::sync::CancellationToken;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this live check"))
}

async fn live_probe(protocol: ProviderProtocol, base: &str, key: Option<String>, model: String) {
    let settings = ProviderSettings {
        name: ProviderName::parse("live").expect("name"),
        protocol,
        base_url: BaseUrl::parse(base).expect("base url"),
        model_id: ModelId::parse(&model).expect("model"),
        timeout: TimeoutSecs::new(120).expect("timeout"),
        max_output_tokens: MaxOutputTokens::new(512).expect("max"),
    };
    let client = LlmClient::new(ClientOptions {
        use_system_proxy: true,
        retry_backoff: Duration::from_millis(500),
    })
    .expect("client");
    let secret = key.map(SecretValue::new);
    let probe = probe_tools(
        &client,
        &settings,
        secret.as_ref(),
        &CancellationToken::new(),
    )
    .await
    .expect("the provider answered");
    println!(
        "{protocol:?} {base} model={model}: verified={} code={:?} detail={}",
        probe.verified, probe.code, probe.detail
    );
    assert!(probe.verified, "tool calling not verified: {probe:?}");
}

#[tokio::test]
#[ignore = "needs SELAI_LIVE_OPENAI_KEY and SELAI_LIVE_OPENAI_MODEL"]
async fn live_openai_responses_tool_round_trip() {
    live_probe(
        ProviderProtocol::OpenAiResponses,
        "https://api.openai.com/v1",
        Some(env("SELAI_LIVE_OPENAI_KEY")),
        env("SELAI_LIVE_OPENAI_MODEL"),
    )
    .await;
}

#[tokio::test]
#[ignore = "needs SELAI_LIVE_ANTHROPIC_KEY and SELAI_LIVE_ANTHROPIC_MODEL"]
async fn live_anthropic_messages_tool_round_trip() {
    live_probe(
        ProviderProtocol::AnthropicMessages,
        "https://api.anthropic.com",
        Some(env("SELAI_LIVE_ANTHROPIC_KEY")),
        env("SELAI_LIVE_ANTHROPIC_MODEL"),
    )
    .await;
}

#[tokio::test]
#[ignore = "needs SELAI_LIVE_COMPAT_BASE_URL and SELAI_LIVE_COMPAT_MODEL (key optional for local servers)"]
async fn live_openai_compatible_tool_round_trip() {
    live_probe(
        ProviderProtocol::OpenAiChat,
        &env("SELAI_LIVE_COMPAT_BASE_URL"),
        std::env::var("SELAI_LIVE_COMPAT_KEY").ok(),
        env("SELAI_LIVE_COMPAT_MODEL"),
    )
    .await;
}
