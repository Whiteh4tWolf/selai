//! Provider adapters for Selai (ADR-0004).
//!
//! Three wire protocols are implemented: OpenAI Responses, OpenAI Chat
//! Completions (also used by compatible servers), and Anthropic Messages. They
//! emit the same normalized [`AgentEvent`] stream. A tool call is reported only
//! once its arguments are complete JSON. Nothing here executes a tool.

mod adapters;
mod assembler;
mod client;
mod error;
mod probe;
mod sse;
mod templates;
mod types;

pub use client::{ClientOptions, LlmClient, TurnRequest};
pub use error::LlmError;
pub use probe::{
    echo_tool, probe_tools, test_connectivity, ConnectionTest, ToolProbe, PROBE_TOKEN, PROBE_TOOL,
};
pub use templates::{ProviderTemplate, TEMPLATES};
pub use types::{AgentEvent, Message, ToolCall, ToolDefinition, TurnOutcome, Usage};
