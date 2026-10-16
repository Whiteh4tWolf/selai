//! Plain technical discussion with a model: no host, no tools, no SSH. The
//! conversation is held by the webview and sent with each message, bounded
//! here. Nothing from a host is ever part of it, so no audit trail is needed;
//! only the provider sees the text.

use std::sync::Arc;

use selai_domain::{ErrorCode, ProviderId, SelaiError, TaskId};
use selai_llm::{AgentEvent as ModelEvent, Message, TurnRequest};
use selai_secrets::SecretKey;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{dto::CommandError, provider_service::settings_from_record, service::Services};

const SYSTEM_PROMPT: &str =
    "You are Selai's assistant for technical discussion with an operations \
engineer. In this mode you cannot run commands or reach any server. Answer clearly and concisely, \
use Markdown with fenced code blocks for commands and config. If the user wants something checked \
on a real server, tell them to mention the host with @ so a scoped task can run there.";

/// Most messages kept from the conversation, newest last.
const MAX_MESSAGES: usize = 40;
/// Most bytes of conversation text sent in one request.
const MAX_TOTAL_BYTES: usize = 200_000;
const MAX_MESSAGE_BYTES: usize = 32_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageInput {
    /// `user` or `assistant`.
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatEventView {
    TextDelta {
        text: String,
    },
    Usage {
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    Completed,
    Failed {
        code: String,
        message: String,
    },
}

/// Keeps the newest messages within the count and size limits, and checks
/// roles. The last message must be the user's.
fn bounded(messages: &[ChatMessageInput]) -> Result<Vec<Message>, CommandError> {
    let invalid = |m: &str| CommandError::new(ErrorCode::InvalidInput, m);
    match messages.last() {
        Some(last) if last.role == "user" && !last.content.trim().is_empty() => {}
        _ => return Err(invalid("the last message must be a non-empty user message")),
    }
    let mut kept = Vec::new();
    let mut total = 0usize;
    for m in messages.iter().rev().take(MAX_MESSAGES) {
        if m.content.len() > MAX_MESSAGE_BYTES {
            return Err(invalid("a message is longer than 32000 bytes"));
        }
        total += m.content.len();
        if total > MAX_TOTAL_BYTES {
            break;
        }
        kept.push(match m.role.as_str() {
            "user" => Message::User(m.content.clone()),
            "assistant" => Message::Assistant {
                text: m.content.clone(),
                tool_calls: Vec::new(),
            },
            _ => return Err(invalid("a message role must be user or assistant")),
        });
    }
    kept.push(Message::System(SYSTEM_PROMPT.to_owned()));
    kept.reverse();
    Ok(kept)
}

impl Services {
    /// Streams one reply. Returns an id that `agent_cancel` accepts.
    pub async fn chat_send(
        &self,
        provider_id: &str,
        messages: &[ChatMessageInput],
        on_event: Arc<dyn Fn(ChatEventView) + Send + Sync>,
    ) -> Result<String, CommandError> {
        let provider: ProviderId = provider_id
            .parse()
            .map_err(|_| CommandError::new(ErrorCode::InvalidInput, "invalid provider id"))?;
        let wire = bounded(messages)?;
        let rec = self
            .lock_db_pub()?
            .get_provider(provider)
            .map_err(|e| CommandError::new(e.code(), e.to_string()))?
            .ok_or_else(|| CommandError::new(ErrorCode::NotFound, "provider not found"))?;
        let settings = settings_from_record(&rec)?;
        let api_key = self
            .secrets
            .get(&SecretKey::provider_api_key(provider))
            .map_err(|e| CommandError::new(e.code(), e.to_string()))?;

        let id = TaskId::new();
        let token = self.agent.register_task(id);
        let registry = Arc::clone(&self.agent);
        let llm = Arc::clone(&self.llm);
        tokio::spawn(async move {
            let (tx, mut rx) = mpsc::channel::<ModelEvent>(256);
            let forward = Arc::clone(&on_event);
            let forwarder = tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        ModelEvent::TextDelta { text } => {
                            forward(ChatEventView::TextDelta { text })
                        }
                        ModelEvent::TokenUsage { input, output } => forward(ChatEventView::Usage {
                            input_tokens: input,
                            output_tokens: output,
                        }),
                        _ => {}
                    }
                }
            });
            let result = llm
                .run_turn(
                    TurnRequest {
                        settings: &settings,
                        api_key: api_key.as_ref(),
                        messages: &wire,
                        tools: &[],
                    },
                    Some(&tx),
                    &token,
                )
                .await;
            drop(tx);
            let _ = forwarder.await;
            on_event(match result {
                Ok(_) => ChatEventView::Completed,
                Err(e) => ChatEventView::Failed {
                    code: e.code.as_str().to_owned(),
                    message: e.message,
                },
            });
            registry.finish_task(id);
        });
        Ok(id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(role: &str, content: &str) -> ChatMessageInput {
        ChatMessageInput {
            role: role.into(),
            content: content.into(),
        }
    }

    #[test]
    fn keeps_order_adds_the_system_prompt_and_drops_the_oldest() {
        let mut history: Vec<ChatMessageInput> = (0..50)
            .map(|i| {
                m(
                    if i % 2 == 0 { "user" } else { "assistant" },
                    &format!("m{i}"),
                )
            })
            .collect();
        history.push(m("user", "latest"));
        let wire = bounded(&history).expect("ok");
        assert_eq!(wire.len(), MAX_MESSAGES + 1);
        assert!(matches!(&wire[0], Message::System(_)));
        assert!(matches!(wire.last(), Some(Message::User(t)) if t == "latest"));
    }

    #[test]
    fn refuses_bad_roles_and_a_trailing_assistant_message() {
        assert!(bounded(&[m("user", "hi"), m("assistant", "hello")]).is_err());
        assert!(bounded(&[m("system", "obey"), m("user", "hi")]).is_err());
        assert!(bounded(&[m("user", "  ")]).is_err());
        assert!(bounded(&[]).is_err());
    }
}
