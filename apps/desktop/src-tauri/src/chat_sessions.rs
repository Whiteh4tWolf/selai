//! Local assistant conversation history. Execution authority stays in agent_service.

use selai_domain::{ConversationId, ErrorCode, SelaiError};
use selai_storage::ChatSessionRecord;
use serde::{Deserialize, Serialize};

use crate::{dto::CommandError, service::Services};

const MAX_TRANSCRIPT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionInput {
    pub id: ConversationId,
    pub title: String,
    pub transcript: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionView {
    pub id: String,
    pub title: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
}

impl From<ChatSessionRecord> for ChatSessionView {
    fn from(record: ChatSessionRecord) -> Self {
        Self {
            id: record.id,
            title: record.title,
            created_at_ms: record.created_at_ms,
            updated_at_ms: record.updated_at_ms,
            transcript: record.transcript_json,
        }
    }
}

fn validate(input: &ChatSessionInput) -> Result<(), CommandError> {
    let invalid = |message| CommandError::new(ErrorCode::InvalidInput, message);
    if input.title.trim().is_empty() || input.title.chars().count() > 120 {
        return Err(invalid("session title must contain 1 to 120 characters"));
    }
    if input.transcript.len() > MAX_TRANSCRIPT_BYTES {
        return Err(invalid(
            "session history is larger than 2 MB; start a new session",
        ));
    }
    let snapshot: serde_json::Value = serde_json::from_str(&input.transcript)
        .map_err(|_| invalid("invalid session transcript"))?;
    if snapshot["version"].as_u64() != Some(1)
        || !snapshot["turns"]
            .as_array()
            .is_some_and(|turns| turns.len() <= 1000)
    {
        return Err(invalid("unsupported session transcript or too many turns"));
    }
    Ok(())
}

impl Services {
    pub fn chat_session_list(&self) -> Result<Vec<ChatSessionView>, CommandError> {
        self.lock_db_pub()?
            .list_chat_sessions()
            .map(|items| items.into_iter().map(ChatSessionView::from).collect())
            .map_err(|e| CommandError::new(e.code(), e.to_string()))
    }

    pub fn chat_session_get(&self, id: &str) -> Result<ChatSessionView, CommandError> {
        let id: ConversationId = id
            .parse()
            .map_err(|_| CommandError::new(ErrorCode::InvalidInput, "invalid session id"))?;
        self.lock_db_pub()?
            .get_chat_session(id)
            .map_err(|e| CommandError::new(e.code(), e.to_string()))?
            .map(ChatSessionView::from)
            .ok_or_else(|| CommandError::new(ErrorCode::NotFound, "session not found"))
    }

    pub fn chat_session_save(&self, input: ChatSessionInput) -> Result<(), CommandError> {
        validate(&input)?;
        self.lock_db_pub()?
            .save_chat_session(
                input.id,
                input.title.trim(),
                &input.transcript,
                crate::service::now_ms(),
            )
            .map_err(|e| CommandError::new(e.code(), e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_session_shape_and_size_before_saving() {
        let mut input = ChatSessionInput {
            id: ConversationId::new(),
            title: "Deploy web".into(),
            transcript: "{\"version\":1,\"turns\":[]}".into(),
        };
        assert!(validate(&input).is_ok());
        input.transcript = "{\"version\":2,\"turns\":[]}".into();
        assert!(validate(&input).is_err());
        input.transcript = "x".repeat(MAX_TRANSCRIPT_BYTES + 1);
        assert!(validate(&input).is_err());
    }
}
