//! Named assistant sessions. Only the selected session loads its transcript.

use rusqlite::{params, OptionalExtension};
use selai_domain::ConversationId;

use crate::{Database, StorageError};

#[derive(Debug, Clone)]
pub struct ChatSessionRecord {
    pub id: String,
    pub title: String,
    pub transcript_json: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl Database {
    pub fn list_chat_sessions(&self) -> Result<Vec<ChatSessionRecord>, StorageError> {
        let mut stmt = self.conn().prepare(
            "SELECT id, title, created_at_ms, updated_at_ms FROM conversations
             ORDER BY updated_at_ms DESC, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ChatSessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                transcript_json: None,
                created_at_ms: row.get(2)?,
                updated_at_ms: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_chat_session(
        &self,
        id: ConversationId,
    ) -> Result<Option<ChatSessionRecord>, StorageError> {
        Ok(self.conn().query_row(
            "SELECT id, title, transcript_json, created_at_ms, updated_at_ms FROM conversations WHERE id = ?1",
            [id.to_string()],
            |row| Ok(ChatSessionRecord {
                id: row.get(0)?, title: row.get(1)?, transcript_json: Some(row.get(2)?),
                created_at_ms: row.get(3)?, updated_at_ms: row.get(4)?,
            }),
        ).optional()?)
    }

    pub fn save_chat_session(
        &self,
        id: ConversationId,
        title: &str,
        transcript: &str,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.conn().execute(
            "INSERT INTO conversations (id, title, transcript_json, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title,
               transcript_json = excluded.transcript_json, updated_at_ms = excluded.updated_at_ms",
            params![id.to_string(), title, transcript, now_ms],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_survive_reopen_and_list_only_loads_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sessions.sqlite3");
        let first = ConversationId::new();
        let second = ConversationId::new();
        {
            let db = Database::open(&path).expect("open");
            db.save_chat_session(first, "Deploy web", "{\"version\":1,\"turns\":[]}", 10)
                .expect("save");
            db.save_chat_session(second, "Check disks", "{}", 20)
                .expect("save");
            db.save_chat_session(
                first,
                "Deploy production",
                "{\"version\":1,\"turns\":[1]}",
                30,
            )
            .expect("rename");
        }
        let db = Database::open(&path).expect("reopen");
        let list = db.list_chat_sessions().expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, first.to_string());
        assert!(list.iter().all(|session| session.transcript_json.is_none()));
        let saved = db.get_chat_session(first).expect("get").expect("session");
        assert_eq!(saved.title, "Deploy production");
        assert_eq!(saved.created_at_ms, 10);
        assert_eq!(saved.updated_at_ms, 30);
        assert_eq!(
            saved.transcript_json.as_deref(),
            Some("{\"version\":1,\"turns\":[1]}")
        );
        assert!(db
            .get_chat_session(ConversationId::new())
            .expect("get")
            .is_none());
    }
}
