//! Assembles streamed tool-call arguments. A call becomes visible to the caller
//! only after its buffer parses as a JSON object, so a partial or malformed
//! call can never be executed.

use std::collections::{BTreeMap, HashSet};

use crate::{AgentEvent, LlmError, ToolCall};

#[derive(Debug)]
struct Pending {
    call_id: Option<String>,
    name: Option<String>,
    buffer: String,
    started: bool,
}

#[derive(Debug, Default)]
pub struct Assembler {
    pending: BTreeMap<String, Pending>,
    seen_ids: HashSet<String>,
    allowed: Vec<String>,
}

impl Assembler {
    pub fn new(allowed_tools: Vec<String>) -> Self {
        Self {
            allowed: allowed_tools,
            ..Self::default()
        }
    }

    fn entry(&mut self, key: &str) -> &mut Pending {
        self.pending
            .entry(key.to_owned())
            .or_insert_with(|| Pending {
                call_id: None,
                name: None,
                buffer: String::new(),
                started: false,
            })
    }

    /// Records the id and name of a call. Emits `ToolCallStart` once both are known.
    pub fn declare(
        &mut self,
        key: &str,
        call_id: Option<&str>,
        name: Option<&str>,
        out: &mut Vec<AgentEvent>,
    ) -> Result<(), LlmError> {
        let entry = self.entry(key);
        if let Some(id) = call_id.filter(|s| !s.is_empty()) {
            entry.call_id.get_or_insert_with(|| id.to_owned());
        }
        if let Some(n) = name.filter(|s| !s.is_empty()) {
            entry.name.get_or_insert_with(|| n.to_owned());
        }
        if entry.started {
            return Ok(());
        }
        let (Some(id), Some(n)) = (entry.call_id.clone(), entry.name.clone()) else {
            return Ok(());
        };
        if !self.allowed.contains(&n) {
            return Err(LlmError::protocol(format!(
                "the model requested a tool that was not offered: {n}"
            )));
        }
        if !self.seen_ids.insert(id.clone()) {
            return Err(LlmError::protocol("the model reused a tool call ID"));
        }
        if let Some(entry) = self.pending.get_mut(key) {
            entry.started = true;
        }
        out.push(AgentEvent::ToolCallStart {
            call_id: id,
            name: n,
        });
        Ok(())
    }

    pub fn append(&mut self, key: &str, delta: &str, out: &mut Vec<AgentEvent>) {
        if delta.is_empty() {
            return;
        }
        let entry = self.entry(key);
        entry.buffer.push_str(delta);
        if entry.started {
            if let Some(id) = entry.call_id.clone() {
                out.push(AgentEvent::ToolCallArgumentsDelta {
                    call_id: id,
                    delta: delta.to_owned(),
                });
            }
        }
    }

    /// Replaces the buffer with the provider's final argument string. Used when
    /// a protocol sends the complete arguments at the end.
    /// Only affects a call that is still open, so a late duplicate event cannot
    /// reopen a finished call.
    pub fn replace(&mut self, key: &str, arguments: &str) {
        if let Some(entry) = self.pending.get_mut(key) {
            entry.buffer = arguments.to_owned();
        }
    }

    /// Validates and completes one call. An empty argument buffer means no arguments.
    pub fn finish(
        &mut self,
        key: &str,
        out: &mut Vec<AgentEvent>,
    ) -> Result<Option<ToolCall>, LlmError> {
        let Some(entry) = self.pending.remove(key) else {
            return Ok(None);
        };
        let (Some(call_id), Some(name)) = (entry.call_id, entry.name) else {
            return Err(LlmError::protocol(
                "tool call finished without an ID or name",
            ));
        };
        if !entry.started {
            // Declared but never started: the name was not offered or the ID repeated.
            return Err(LlmError::protocol("tool call could not be started"));
        }
        let text = if entry.buffer.trim().is_empty() {
            "{}".to_owned()
        } else {
            entry.buffer
        };
        let arguments: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
            LlmError::protocol("tool call arguments are not valid JSON; the call was not run")
        })?;
        if !arguments.is_object() {
            return Err(LlmError::protocol(
                "tool call arguments must be a JSON object",
            ));
        }
        out.push(AgentEvent::ToolCallReady {
            call_id: call_id.clone(),
            name: name.clone(),
            arguments: arguments.clone(),
        });
        Ok(Some(ToolCall {
            call_id,
            name,
            arguments,
        }))
    }

    /// Completes every open call in key order. A call still open at the end of
    /// a turn is an error, because nothing can complete it later.
    pub fn finish_all(&mut self, out: &mut Vec<AgentEvent>) -> Result<Vec<ToolCall>, LlmError> {
        let keys: Vec<String> = self.pending.keys().cloned().collect();
        let mut calls = Vec::new();
        for key in keys {
            if let Some(call) = self.finish(&key, out)? {
                calls.push(call);
            }
        }
        Ok(calls)
    }

    pub fn has_open(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<String> {
        vec!["selai_echo".to_owned()]
    }

    #[test]
    fn complete_fragmented_arguments_become_one_ready_call() {
        let mut a = Assembler::new(tools());
        let mut out = Vec::new();
        a.declare("0", Some("c1"), Some("selai_echo"), &mut out)
            .expect("declare");
        a.append("0", "{\"te", &mut out);
        a.append("0", "xt\":\"hi\"}", &mut out);
        let call = a.finish("0", &mut out).expect("finish").expect("call");
        assert_eq!(call.arguments["text"], "hi");
        assert!(out
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolCallReady { .. })));
    }

    #[test]
    fn truncated_json_never_becomes_a_call() {
        let mut a = Assembler::new(tools());
        let mut out = Vec::new();
        a.declare("0", Some("c1"), Some("selai_echo"), &mut out)
            .expect("declare");
        a.append("0", "{\"text\":\"unterminated", &mut out);
        assert!(a.finish("0", &mut out).is_err());
        assert!(!out
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolCallReady { .. })));
    }

    #[test]
    fn arguments_must_be_an_object() {
        let mut a = Assembler::new(tools());
        let mut out = Vec::new();
        a.declare("0", Some("c1"), Some("selai_echo"), &mut out)
            .expect("declare");
        a.append("0", "[1,2]", &mut out);
        assert!(a.finish("0", &mut out).is_err());
    }

    #[test]
    fn unoffered_tools_are_refused() {
        let mut a = Assembler::new(tools());
        let mut out = Vec::new();
        assert!(a
            .declare("0", Some("c1"), Some("ssh.run_command"), &mut out)
            .is_err());
    }

    #[test]
    fn repeated_call_ids_are_refused() {
        let mut a = Assembler::new(tools());
        let mut out = Vec::new();
        a.declare("0", Some("dup"), Some("selai_echo"), &mut out)
            .expect("first");
        a.append("0", "{}", &mut out);
        a.finish("0", &mut out).expect("finish");
        assert!(a
            .declare("1", Some("dup"), Some("selai_echo"), &mut out)
            .is_err());
    }

    #[test]
    fn empty_arguments_mean_an_empty_object() {
        let mut a = Assembler::new(tools());
        let mut out = Vec::new();
        a.declare("0", Some("c1"), Some("selai_echo"), &mut out)
            .expect("declare");
        let call = a.finish("0", &mut out).expect("finish").expect("call");
        assert_eq!(call.arguments, serde_json::json!({}));
    }
}
