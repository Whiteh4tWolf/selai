//! Anthropic Messages API. Tool use is a `tool_use` content block whose input
//! arrives as `input_json_delta` fragments. Tool results are `tool_result` blocks
//! in the following user message.

use serde_json::{json, Value};

use super::Accumulator;
use crate::{sse::SseEvent, AgentEvent, LlmError, Message, ToolDefinition};

pub fn body(model: &str, max_tokens: u32, messages: &[Message], tools: &[ToolDefinition]) -> Value {
    let mut system = Vec::new();
    let mut wire: Vec<Value> = Vec::new();
    for m in messages {
        match m {
            Message::System(t) => system.push(t.clone()),
            Message::User(t) => {
                wire.push(json!({"role": "user", "content": [{"type": "text", "text": t}]}))
            }
            Message::Assistant { text, tool_calls } => {
                let mut content = Vec::new();
                if !text.is_empty() {
                    content.push(json!({"type": "text", "text": text}));
                }
                for c in tool_calls {
                    content.push(json!({"type": "tool_use", "id": c.call_id, "name": c.name, "input": c.arguments}));
                }
                wire.push(json!({"role": "assistant", "content": content}));
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => {
                let block = json!({
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": content,
                    "is_error": is_error,
                });
                // Results for one assistant turn share a single user message.
                let merge = wire.last().is_some_and(|last| {
                    last["role"] == "user"
                        && last["content"].as_array().is_some_and(|c| {
                            !c.is_empty() && c.iter().all(|b| b["type"] == "tool_result")
                        })
                });
                if merge {
                    if let Some(arr) = wire.last_mut().and_then(|l| l["content"].as_array_mut()) {
                        arr.push(block);
                    }
                } else {
                    wire.push(json!({"role": "user", "content": [block]}));
                }
            }
        }
    }
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": wire,
        "stream": true,
    });
    if !system.is_empty() {
        body["system"] = json!(system.join("\n\n"));
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(
            tools
                .iter()
                .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.parameters}))
                .collect(),
        );
    }
    body
}

pub fn feed(
    acc: &mut Accumulator,
    event: &SseEvent,
    out: &mut Vec<AgentEvent>,
) -> Result<(), LlmError> {
    let data = event.data.trim();
    if data.is_empty() {
        return Ok(());
    }
    let v: Value = serde_json::from_str(data)
        .map_err(|_| LlmError::protocol("malformed stream event from the provider"))?;
    let key = v["index"]
        .as_u64()
        .map(|i| i.to_string())
        .unwrap_or_default();
    match v["type"].as_str().unwrap_or("") {
        "message_start" => {
            acc.usage.input = v["message"]["usage"]["input_tokens"].as_u64();
        }
        "content_block_start" => {
            let block = &v["content_block"];
            if block["type"] == "tool_use" {
                acc.assembler
                    .declare(&key, block["id"].as_str(), block["name"].as_str(), out)?;
            }
        }
        "content_block_delta" => {
            let delta = &v["delta"];
            match delta["type"].as_str() {
                Some("text_delta") => acc.push_text(delta["text"].as_str().unwrap_or(""), out),
                Some("input_json_delta") => {
                    acc.assembler
                        .append(&key, delta["partial_json"].as_str().unwrap_or(""), out);
                }
                _ => {}
            }
        }
        "content_block_stop" => {
            let call = acc.assembler.finish(&key, out)?;
            acc.complete_call(call);
        }
        "message_delta" => {
            if let Some(reason) = v["delta"]["stop_reason"].as_str() {
                acc.finish_reason = Some(reason.to_owned());
            }
            if let Some(output) = v["usage"]["output_tokens"].as_u64() {
                acc.usage.output = Some(output);
            }
        }
        "message_stop" => {
            let calls = acc.assembler.finish_all(out)?;
            acc.calls.extend(calls);
            acc.finished = true;
        }
        "error" => {
            let message = v["error"]["message"]
                .as_str()
                .unwrap_or("provider reported an error");
            return Err(LlmError::protocol(message.to_owned()));
        }
        _ => {}
    }
    Ok(())
}
