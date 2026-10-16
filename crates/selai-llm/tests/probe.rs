//! Capability probes against mock providers. A probe passes only with a real
//! tool call followed by a final answer after the tool result.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod support;

use std::time::Duration;

use selai_domain::{ErrorCode, ProviderProtocol};
use selai_llm::{probe_tools, test_connectivity};
use serde_json::{json, Value};
use support::{client, key, settings, sse};
use tokio_util::sync::CancellationToken;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, Request, Respond, ResponseTemplate,
};

/// Answers like a model that supports tools: a tool call first, then text once a
/// tool result is present in the conversation.
struct ToolCapableChat;

impl Respond for ToolCapableChat {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).expect("json");
        let has_result = body["messages"]
            .as_array()
            .is_some_and(|m| m.iter().any(|x| x["role"] == "tool"));
        let payload = if has_result {
            sse(&[
                json!({"choices":[{"delta":{"content":"The tool returned the probe text."}}]}),
                json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
            ]) + "data: [DONE]\n\n"
        } else {
            sse(&[
                json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_p","type":"function","function":{"name":"selai_echo","arguments":"{\"text\":\"selai-probe-7f3a\"}"}}]}}]}),
                json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}),
            ]) + "data: [DONE]\n\n"
        };
        ResponseTemplate::new(200).set_body_raw(payload, "text/event-stream")
    }
}

#[tokio::test]
async fn tool_calling_provider_passes_the_probe_after_a_full_round_trip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ToolCapableChat)
        .mount(&server)
        .await;
    let s = settings(
        ProviderProtocol::OpenAiChat,
        &format!("{}/v1", server.uri()),
    );
    let probe = probe_tools(&client(), &s, Some(&key()), &CancellationToken::new())
        .await
        .expect("probe runs");
    assert!(probe.verified, "{probe:?}");
    assert_eq!(probe.code, None);

    // Two model turns: the call, then the answer after the result.
    assert_eq!(server.received_requests().await.expect("recorded").len(), 2);
}

#[tokio::test]
async fn text_only_model_is_reported_as_not_verified() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            sse(&[
                json!({"choices":[{"delta":{"content":"I cannot call tools, but hello."}}]}),
                json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
            ]) + "data: [DONE]\n\n",
            "text/event-stream",
        ))
        .mount(&server)
        .await;
    let s = settings(
        ProviderProtocol::OpenAiChat,
        &format!("{}/v1", server.uri()),
    );
    let probe = probe_tools(&client(), &s, Some(&key()), &CancellationToken::new())
        .await
        .expect("probe runs");
    assert!(!probe.verified);
    assert_eq!(probe.code, Some(ErrorCode::ProviderToolUnsupported));
}

#[tokio::test]
async fn server_refusing_tools_is_not_verified_but_bad_keys_are_errors() {
    let refusing = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(
                json!({"error":{"message":"tools are not supported by this model"}}),
            ),
        )
        .mount(&refusing)
        .await;
    let s = settings(
        ProviderProtocol::OpenAiChat,
        &format!("{}/v1", refusing.uri()),
    );
    let probe = probe_tools(&client(), &s, Some(&key()), &CancellationToken::new())
        .await
        .expect("classified, not an error");
    assert_eq!(probe.code, Some(ErrorCode::ProviderToolUnsupported));

    let unauthorized = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&unauthorized)
        .await;
    let s2 = settings(
        ProviderProtocol::OpenAiChat,
        &format!("{}/v1", unauthorized.uri()),
    );
    let err = probe_tools(&client(), &s2, Some(&key()), &CancellationToken::new())
        .await
        .expect_err("a bad key is an error, not a capability result");
    assert_eq!(err.code, ErrorCode::ProviderUnauthorized);
}

#[tokio::test]
async fn anthropic_probe_round_trip_passes_with_tool_result_blocks() {
    struct AnthropicToolModel;
    impl Respond for AnthropicToolModel {
        fn respond(&self, req: &Request) -> ResponseTemplate {
            let body: Value = serde_json::from_slice(&req.body).expect("json");
            let has_result = body.to_string().contains("tool_result");
            let payload = if has_result {
                "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Done.\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_owned()
            } else {
                "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_p\",\"name\":\"selai_echo\",\"input\":{}}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"text\\\":\\\"selai-probe-7f3a\\\"}\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_owned()
            };
            ResponseTemplate::new(200).set_body_raw(payload, "text/event-stream")
        }
    }
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(AnthropicToolModel)
        .mount(&server)
        .await;
    let s = settings(ProviderProtocol::AnthropicMessages, &server.uri());
    let probe = probe_tools(&client(), &s, Some(&key()), &CancellationToken::new())
        .await
        .expect("probe");
    assert!(probe.verified, "{probe:?}");
}

#[tokio::test]
async fn connectivity_test_reports_latency_and_a_safe_preview() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(
                    sse(&[
                        json!({"choices":[{"delta":{"content":"ok"}}]}),
                        json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
                    ]) + "data: [DONE]\n\n",
                    "text/event-stream",
                )
                .set_delay(Duration::from_millis(5)),
        )
        .mount(&server)
        .await;
    let s = settings(
        ProviderProtocol::OpenAiChat,
        &format!("{}/v1", server.uri()),
    );
    let test = test_connectivity(&client(), &s, Some(&key()), &CancellationToken::new())
        .await
        .expect("connects");
    assert_eq!(test.reply_preview, "ok");
    assert!(test.latency_ms >= 1);
}

#[tokio::test]
async fn connectivity_with_empty_reply_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            sse(&[json!({"choices":[{"delta":{},"finish_reason":"stop"}]})]) + "data: [DONE]\n\n",
            "text/event-stream",
        ))
        .mount(&server)
        .await;
    let s = settings(
        ProviderProtocol::OpenAiChat,
        &format!("{}/v1", server.uri()),
    );
    assert!(
        test_connectivity(&client(), &s, Some(&key()), &CancellationToken::new())
            .await
            .is_err()
    );
}
