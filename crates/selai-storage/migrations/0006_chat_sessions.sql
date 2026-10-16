ALTER TABLE conversations ADD COLUMN transcript_json TEXT NOT NULL DEFAULT '{"version":1,"turns":[]}';
CREATE INDEX ix_conversations_recent ON conversations(updated_at_ms DESC, id);
