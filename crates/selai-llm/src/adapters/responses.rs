//! OpenAI Responses API. Function calls arrive as output items keyed by `item_id`,
//! with argument deltas and a final `arguments.done`.

use serde_json::{json, Value};

use super::Accumulator;
use crate::{sse::SseEvent, AgentEvent, LlmError, Message, ToolDefinition};

pub fn body(model: &str, max_tokens: u32, messages: &[Message], tools: &[ToolDefinition]) -> Value {
    let mut instructions = Vec::new();
    let mut input = Vec::new();
    for m in messages {
        match m {
            Message::System(t) => instructions.push(t.clone()),
            Message::User(t) => input.push(json!({"role": "user", "content": t})),
            Message::Assistant { text, tool_calls } => {
                if !text.is_empty() {
                    input.push(json!({"role": "assistant", "content": text}));
                }
                for c in tool_calls {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": c.call_id,
                        "name": c.name,
                        "arguments": c.arguments.to_string(),
                    }));
                }
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => {
                let output = if *is_error {
                    format!("ERROR: {content}")
                } else {
                    content.clone()
                };
                input.push(
                    json!({"type": "function_call_output", "call_id": call_id, "output": output}),
                );
            }
        }
    }
    let mut body = json!({
        "model": model,
        "input": input,
        "stream": true,
        "store": false,
        "max_output_tokens": max_tokens,
    });
    if !instructions.is_empty() {
        body["instructions"] = json!(instructions.join("\n\n"));
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(
            tools
                .iter()
                .map(|t| json!({"type": "function", "name": t.name, "description": t.description, "parameters": t.parameters}))
                .collect(),
        );
        body["tool_choice"] = json!("auto");
    }
    body
}

fn error_message(v: &Value) -> String {
    v["response"]["error"]["message"]
        .as_str()
        .or_else(|| v["error"]["message"].as_str())
        .or_else(|| v["message"].as_str())
        .unwrap_or("provider reported a failed response")
        .to_owned()
}

pub fn feed(
    acc: &mut Accumulator,
    event: &SseEvent,
    out: &mut Vec<AgentEvent>,
) -> Result<(), LlmError> {
    let data = event.data.trim();
    if data.is_empty() || data == "[DONE]" {
        return Ok(());
    }
    let v: Value = serde_json::from_str(data)
        .map_err(|_| LlmError::protocol("malformed stream event from the provider"))?;
    match v["type"].as_str().unwrap_or("") {
        "response.output_text.delta" => {
            acc.push_text(v["delta"].as_str().unwrap_or(""), out);
        }
        "response.output_item.added" => {
            let item = &v["item"];
            if item["type"] == "function_call" {
                let key = item["id"].as_str().unwrap_or_default();
                acc.assembler
                    .declare(key, item["call_id"].as_str(), item["name"].as_str(), out)?;
                if let Some(args) = item["arguments"].as_str() {
                    acc.assembler.append(key, args, out);
                }
            }
        }
        "response.function_call_arguments.delta" => {
            let key = v["item_id"].as_str().unwrap_or_default();
            acc.assembler
                .append(key, v["delta"].as_str().unwrap_or(""), out);
        }
        "response.function_call_arguments.done" => {
            let key = v["item_id"].as_str().unwrap_or_default();
            if let Some(args) = v["arguments"].as_str() {
                acc.assembler.replace(key, args);
            }
            let call = acc.assembler.finish(key, out)?;
            acc.complete_call(call);
        }
        "response.output_item.done" => {
            let item = &v["item"];
            if item["type"] == "function_call" {
                let key = item["id"].as_str().unwrap_or_default();
                if let Some(args) = item["arguments"].as_str() {
                    acc.assembler.replace(key, args);
                }
                let call = acc.assembler.finish(key, out)?;
                acc.complete_call(call);
            }
        }
        "response.completed" => {
            let calls = acc.assembler.finish_all(out)?;
            acc.calls.extend(calls);
            acc.usage.input = v["response"]["usage"]["input_tokens"].as_u64();
            acc.usage.output = v["response"]["usage"]["output_tokens"].as_u64();
            acc.finish_reason = Some("completed".to_owned());
            acc.finished = true;
        }
        "response.incomplete" => {
            let calls = acc.assembler.finish_all(out)?;
            acc.calls.extend(calls);
            let reason = v["response"]["incomplete_details"]["reason"]
                .as_str()
                .unwrap_or("unknown");
            acc.finish_reason = Some(format!("incomplete: {reason}"));
            acc.finished = true;
        }
        "response.failed" | "error" => return Err(LlmError::protocol(error_message(&v))),
        _ => {}
    }
    Ok(())
}
