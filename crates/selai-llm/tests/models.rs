//! Model discovery, which also checks the API key before a profile is saved.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod support;

use std::time::Duration;

use selai_domain::{BaseUrl, ErrorCode, ProviderProtocol};
use serde_json::json;
use support::{client, key, KEY};
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const LIMIT: Duration = Duration::from_secs(5);

fn base(server: &MockServer, suffix: &str) -> BaseUrl {
    BaseUrl::parse(&format!("{}{suffix}", server.uri())).expect("base")
}

#[tokio::test]
async fn openai_compatible_models_are_listed_sorted_with_the_bearer_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [{"id": "gpt-4o"}, {"id": "claude-via-litellm"}, {"id": "gpt-4o"}, {"id": " "}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let models = client()
        .list_models(
            ProviderProtocol::OpenAiChat,
            &base(&server, "/v1"),
            Some(&key()),
            LIMIT,
        )
        .await
        .expect("models");
    assert_eq!(models, vec!["claude-via-litellm", "gpt-4o"]);
}

#[tokio::test]
async fn anthropic_models_use_its_headers_and_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("x-api-key", KEY))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"id": "claude-opus-5-5", "type": "model"}], "has_more": false
        })))
        .expect(1)
        .mount(&server)
        .await;
    let models = client()
        .list_models(
            ProviderProtocol::AnthropicMessages,
            &base(&server, ""),
            Some(&key()),
            LIMIT,
        )
        .await
        .expect("models");
    assert_eq!(models, vec!["claude-opus-5-5"]);
}

#[tokio::test]
async fn a_rejected_key_is_reported_without_the_key_in_the_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_string(format!("bad key {KEY}")))
        .mount(&server)
        .await;
    let err = client()
        .list_models(
            ProviderProtocol::OpenAiChat,
            &base(&server, "/v1"),
            Some(&key()),
            LIMIT,
        )
        .await
        .expect_err("unauthorized");
    assert_eq!(err.code, ErrorCode::ProviderUnauthorized);
    assert!(!err.message.contains(KEY));
}

#[tokio::test]
async fn an_endpoint_without_a_model_list_says_to_type_the_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let err = client()
        .list_models(
            ProviderProtocol::OpenAiChat,
            &base(&server, "/v1"),
            None,
            LIMIT,
        )
        .await
        .expect_err("404");
    assert!(err.message.contains("type the model ID"), "{}", err.message);
}

#[tokio::test]
async fn ollama_native_names_are_accepted() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"models": [{"name": "qwen2.5:7b"}, {"name": "llama3.1:8b"}]}),
            ),
        )
        .mount(&server)
        .await;
    let models = client()
        .list_models(
            ProviderProtocol::OpenAiChat,
            &base(&server, ""),
            None,
            LIMIT,
        )
        .await
        .expect("models");
    assert_eq!(models, vec!["llama3.1:8b", "qwen2.5:7b"]);
}

#[tokio::test]
async fn a_remote_endpoint_without_a_key_is_refused_before_any_request() {
    let remote = BaseUrl::parse("https://api.example.com/v1").expect("base");
    let err = client()
        .list_models(ProviderProtocol::OpenAiChat, &remote, None, LIMIT)
        .await
        .expect_err("no key");
    assert_eq!(err.code, ErrorCode::ProviderUnauthorized);
}
