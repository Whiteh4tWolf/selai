//! Starting points for common providers. A template gives a protocol and a
//! default base URL. It never gives a model ID, because model names change.
//! The `note` says how much the entry has been verified.

use selai_domain::ProviderProtocol;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderTemplate {
    pub id: &'static str,
    pub display_name: &'static str,
    pub protocol: ProviderProtocol,
    pub default_base_url: &'static str,
    pub note: &'static str,
}

pub const TEMPLATES: &[ProviderTemplate] = &[
    ProviderTemplate {
        id: "openai",
        display_name: "OpenAI",
        protocol: ProviderProtocol::OpenAiResponses,
        default_base_url: "https://api.openai.com/v1",
        note: "Official endpoint. Enter a model ID from your account. Not verified against a live endpoint in this build.",
    },
    ProviderTemplate {
        id: "anthropic",
        display_name: "Anthropic",
        protocol: ProviderProtocol::AnthropicMessages,
        default_base_url: "https://api.anthropic.com",
        note: "Official endpoint. Enter a model ID from your account. Not verified against a live endpoint in this build.",
    },
    ProviderTemplate {
        id: "zai-glm",
        display_name: "Z.ai (GLM)",
        protocol: ProviderProtocol::OpenAiChat,
        default_base_url: "https://api.z.ai/api/paas/v4",
        note: "URL taken from third-party integration docs (2026-10-16). Confirm with Z.ai. Global and China consoles issue different keys.",
    },
    ProviderTemplate {
        id: "moonshot-kimi",
        display_name: "Moonshot (Kimi)",
        protocol: ProviderProtocol::OpenAiChat,
        default_base_url: "https://api.moonshot.ai/v1",
        note: "URL taken from third-party docs (2026-10-16). Confirm with Moonshot. China-region keys use api.moonshot.cn.",
    },
    ProviderTemplate {
        id: "litellm",
        display_name: "LiteLLM proxy",
        protocol: ProviderProtocol::OpenAiChat,
        default_base_url: "http://localhost:4000",
        note: "Your gateway. Use the virtual key as the API key and the model group name as the model ID.",
    },
    ProviderTemplate {
        id: "vllm",
        display_name: "vLLM",
        protocol: ProviderProtocol::OpenAiChat,
        default_base_url: "http://localhost:8000/v1",
        note: "Tool calling needs the server's tool parser for the model. Without it the tool probe fails.",
    },
    ProviderTemplate {
        id: "ollama",
        display_name: "Ollama",
        protocol: ProviderProtocol::OpenAiChat,
        default_base_url: "http://localhost:11434/v1",
        note: "Use a model that supports tool calls. The API key can stay empty for a local server.",
    },
    ProviderTemplate {
        id: "custom",
        display_name: "Custom (OpenAI-compatible)",
        protocol: ProviderProtocol::OpenAiChat,
        default_base_url: "",
        note: "Any Chat Completions endpoint. Run the tool-calling test before relying on it.",
    },
];
