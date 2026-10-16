use serde::Serialize;

/// A tool call the model asked for. Arguments are a complete JSON object.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Provider-neutral conversation message.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    System(String),
    User(String),
    Assistant {
        text: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
    },
}

/// A tool the model may call. The schema is supplied by local code only.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments object.
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input: Option<u64>,
    pub output: Option<u64>,
}

/// Normalized stream events. Adapters emit these and nothing else.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    TextDelta {
        text: String,
    },
    ToolCallStart {
        call_id: String,
        name: String,
    },
    ToolCallArgumentsDelta {
        call_id: String,
        delta: String,
    },
    /// Emitted once per call, after its arguments parsed as a JSON object.
    ToolCallReady {
        call_id: String,
        name: String,
        arguments: serde_json::Value,
    },
    TokenUsage {
        input: Option<u64>,
        output: Option<u64>,
    },
    Completed {
        finish_reason: String,
    },
    Error {
        code: String,
        message: String,
    },
}

/// Final result of one model turn, assembled from the stream.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnOutcome {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
    pub usage: Usage,
}
