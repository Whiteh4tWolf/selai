//! Per-protocol request builders and stream decoders. Each adapter builds the
//! request body and endpoint for one wire protocol, and decodes its SSE events
//! into the normalized stream. No adapter executes anything.

pub mod anthropic;
pub mod chat;
pub mod responses;

use selai_domain::ProviderProtocol;

use crate::{assembler::Assembler, sse::SseEvent, AgentEvent, LlmError, ToolCall, Usage};

/// State shared by every protocol while one turn streams.
#[derive(Debug)]
pub struct Accumulator {
    pub text: String,
    pub calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub usage: Usage,
    pub finished: bool,
    pub assembler: Assembler,
}

impl Accumulator {
    pub fn new(allowed_tools: Vec<String>) -> Self {
        Self {
            text: String::new(),
            calls: Vec::new(),
            finish_reason: None,
            usage: Usage::default(),
            finished: false,
            assembler: Assembler::new(allowed_tools),
        }
    }

    pub fn push_text(&mut self, text: &str, out: &mut Vec<AgentEvent>) {
        if !text.is_empty() {
            self.text.push_str(text);
            out.push(AgentEvent::TextDelta {
                text: text.to_owned(),
            });
        }
    }

    pub fn complete_call(&mut self, call: Option<ToolCall>) {
        if let Some(c) = call {
            self.calls.push(c);
        }
    }
}

/// Stream decoder for one protocol.
pub struct Decoder {
    protocol: ProviderProtocol,
    pub acc: Accumulator,
}

impl Decoder {
    pub fn new(protocol: ProviderProtocol, allowed_tools: Vec<String>) -> Self {
        Self {
            protocol,
            acc: Accumulator::new(allowed_tools),
        }
    }

    pub fn feed(&mut self, event: &SseEvent, out: &mut Vec<AgentEvent>) -> Result<(), LlmError> {
        match self.protocol {
            ProviderProtocol::OpenAiChat => chat::feed(&mut self.acc, event, out),
            ProviderProtocol::OpenAiResponses => responses::feed(&mut self.acc, event, out),
            ProviderProtocol::AnthropicMessages => anthropic::feed(&mut self.acc, event, out),
        }
    }
}

/// Endpoint for a protocol under an API root.
pub fn endpoint(protocol: ProviderProtocol, base: &str) -> String {
    match protocol {
        ProviderProtocol::OpenAiChat => format!("{base}/chat/completions"),
        ProviderProtocol::OpenAiResponses => format!("{base}/responses"),
        ProviderProtocol::AnthropicMessages => format!("{base}/v1/messages"),
    }
}
