//! OpenAI Chat Completions, also used by compatible servers (GLM, Kimi, LiteLLM,
//! vLLM, Ollama). Tool calls arrive as indexed fragments on `delta.tool_calls`.

use serde_json::{json, Value};

use super::Accumulator;
use crate::{sse::SseEvent, AgentEvent, LlmError, Message, ToolDefinition};

pub fn body(model: &str, max_tokens: u32, messages: &[Message], tools: &[ToolDefinition]) -> Value {
    let mut wire = Vec::new();
    for m in messages {
        wire.push(match m {
            Message::System(t) => json!({"role": "system", "content": t}),
            Message::User(t) => json!({"role": "user", "content": t}),
            Message::Assistant { text, tool_calls } => {
                let mut o = json!({"role": "assistant"});
                o["content"] = if text.is_empty() && !tool_calls.is_empty() {
                    Value::Null
                } else {
                    json!(text)
                };
                if !tool_calls.is_empty() {
                    o["tool_calls"] = Value::Array(
                        tool_calls
                            .iter()
                            .map(|c| {
                                json!({
                                    "id": c.call_id,
                                    "type": "function",
                                    "function": {"name": c.name, "arguments": c.arguments.to_string()}
                                })
                            })
                            .collect(),
                    );
                }
                o
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => {
                let text = if *is_error {
                    format!("ERROR: {content}")
                } else {
                    content.clone()
                };
                json!({"role": "tool", "tool_call_id": call_id, "content": text})
            }
        });
    }
    let mut body = json!({
        "model": model,
        "messages": wire,
        "stream": true,
        "stream_options": {"include_usage": true},
        "max_tokens": max_tokens,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(
            tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {"name": t.name, "description": t.description, "parameters": t.parameters}
                    })
                })
                .collect(),
        );
        body["tool_choice"] = json!("auto");
    }
    body
}

pub fn feed(
    acc: &mut Accumulator,
    event: &SseEvent,
    out: &mut Vec<AgentEvent>,
) -> Result<(), LlmError> {
    let data = event.data.trim();
    if data == "[DONE]" {
        let calls = acc.assembler.finish_all(out)?;
        acc.calls.extend(calls);
        acc.finished = true;
        return Ok(());
    }
    let v: Value = serde_json::from_str(data)
        .map_err(|_| LlmError::protocol("malformed stream chunk from the provider"))?;
    if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
        let message = err["message"]
            .as_str()
            .unwrap_or("provider returned an error");
        return Err(LlmError::protocol(message.to_owned()));
    }
    if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
        acc.usage.input = u["prompt_tokens"].as_u64();
        acc.usage.output = u["completion_tokens"].as_u64();
    }
    let Some(choice) = v["choices"].as_array().and_then(|c| c.first()) else {
        return Ok(());
    };
    let delta = &choice["delta"];
    if let Some(text) = delta["content"].as_str() {
        acc.push_text(text, out);
    }
    if let Some(calls) = delta["tool_calls"].as_array() {
        for tc in calls {
            let key = tc["index"].as_u64().unwrap_or(0).to_string();
            acc.assembler.declare(
                &key,
                tc["id"].as_str(),
                tc["function"]["name"].as_str(),
                out,
            )?;
            if let Some(args) = tc["function"]["arguments"].as_str() {
                acc.assembler.append(&key, args, out);
            }
        }
    }
    if let Some(reason) = choice["finish_reason"].as_str() {
        let calls = acc.assembler.finish_all(out)?;
        acc.calls.extend(calls);
        acc.finish_reason = Some(reason.to_owned());
    }
    Ok(())
}
