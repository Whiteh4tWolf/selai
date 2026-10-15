-- Migration 0001: initial schema.
-- Derived from the handoff file schema_v1.sql. Two deliberate differences:
--   * PRAGMA foreign_keys / journal_mode are NOT here. They are connection
--     level settings and do not work inside a migration transaction; they are
--     applied (and verified) by selai_storage::Database::open.
--   * The schema_migrations bookkeeping row is written by the migration runner.
-- NO secrets are stored in this database.

CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS host_groups (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS hosts (
  id TEXT PRIMARY KEY,
  label TEXT NOT NULL COLLATE NOCASE,
  hostname TEXT NOT NULL,
  port INTEGER NOT NULL DEFAULT 22 CHECK(port BETWEEN 1 AND 65535),
  username TEXT NOT NULL,
  auth_method TEXT NOT NULL CHECK(auth_method IN ('password', 'private_key')),
  private_key_path TEXT,
  group_id TEXT REFERENCES host_groups(id) ON DELETE SET NULL,
  connect_timeout_seconds INTEGER NOT NULL DEFAULT 15 CHECK(connect_timeout_seconds BETWEEN 1 AND 120),
  keepalive_seconds INTEGER NOT NULL DEFAULT 30 CHECK(keepalive_seconds BETWEEN 0 AND 300),
  terminal_type TEXT NOT NULL DEFAULT 'xterm-256color',
  credential_configured INTEGER NOT NULL DEFAULT 0 CHECK(credential_configured IN (0, 1)),
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  last_connected_at_ms INTEGER,
  archived_at_ms INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS ux_hosts_active_label ON hosts(label COLLATE NOCASE)
  WHERE archived_at_ms IS NULL;
CREATE INDEX IF NOT EXISTS ix_hosts_group ON hosts(group_id, archived_at_ms);

CREATE TABLE IF NOT EXISTS known_host_keys (
  id TEXT PRIMARY KEY,
  host_id TEXT NOT NULL REFERENCES hosts(id) ON DELETE RESTRICT,
  algorithm TEXT NOT NULL,
  public_key_base64 TEXT NOT NULL,
  fingerprint_sha256 TEXT NOT NULL,
  first_seen_at_ms INTEGER NOT NULL,
  replaced_at_ms INTEGER
);
CREATE INDEX IF NOT EXISTS ix_known_keys_host ON known_host_keys(host_id, replaced_at_ms);

CREATE TABLE IF NOT EXISTS provider_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  protocol TEXT NOT NULL CHECK(protocol IN ('openai_responses', 'openai_chat', 'anthropic_messages')),
  base_url TEXT NOT NULL,
  model_id TEXT NOT NULL,
  options_json TEXT NOT NULL DEFAULT '{}',
  key_configured INTEGER NOT NULL DEFAULT 0 CHECK(key_configured IN (0, 1)),
  tool_probe_passed INTEGER NOT NULL DEFAULT 0 CHECK(tool_probe_passed IN (0, 1)),
  last_tool_probe_at_ms INTEGER,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS conversations (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  provider_profile_id TEXT REFERENCES provider_profiles(id) ON DELETE SET NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_tasks (
  id TEXT PRIMARY KEY,
  conversation_id TEXT REFERENCES conversations(id) ON DELETE SET NULL,
  target_host_id TEXT NOT NULL REFERENCES hosts(id) ON DELETE RESTRICT,
  provider_profile_id TEXT REFERENCES provider_profiles(id) ON DELETE SET NULL,
  model_id_snapshot TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN (
    'created','validating_scope','ready','running','awaiting_approval',
    'completed','failed','cancelled','interrupted')),
  policy_snapshot_json TEXT NOT NULL DEFAULT '{}',
  limits_json TEXT NOT NULL DEFAULT '{}',
  error_code TEXT,
  created_at_ms INTEGER NOT NULL,
  started_at_ms INTEGER,
  finished_at_ms INTEGER
);
CREATE INDEX IF NOT EXISTS ix_tasks_host_time ON agent_tasks(target_host_id, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS conversation_messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  task_id TEXT REFERENCES agent_tasks(id) ON DELETE SET NULL,
  role TEXT NOT NULL CHECK(role IN ('user','assistant','tool','system_note')),
  content_redacted TEXT NOT NULL,
  call_id TEXT,
  created_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_messages_thread ON conversation_messages(conversation_id, created_at_ms);

CREATE TABLE IF NOT EXISTS agent_steps (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
  sequence_no INTEGER NOT NULL,
  kind TEXT NOT NULL,
  state TEXT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms INTEGER NOT NULL,
  UNIQUE(task_id, sequence_no)
);

CREATE TABLE IF NOT EXISTS audit_blobs (
  id TEXT PRIMARY KEY,
  encrypted_blob BLOB NOT NULL,
  encryption_format TEXT NOT NULL DEFAULT 'AES-256-GCM-v1',
  byte_count INTEGER NOT NULL CHECK(byte_count >= 0),
  content_sha256 TEXT NOT NULL,
  truncated INTEGER NOT NULL DEFAULT 0 CHECK(truncated IN (0, 1)),
  created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS command_executions (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE RESTRICT,
  host_id TEXT NOT NULL REFERENCES hosts(id) ON DELETE RESTRICT,
  provider_profile_id TEXT REFERENCES provider_profiles(id) ON DELETE SET NULL,
  model_id_snapshot TEXT NOT NULL,
  tool_call_id TEXT,
  tool_name TEXT NOT NULL CHECK(tool_name IN ('ssh.run_diagnostic', 'ssh.run_command')),
  command_redacted TEXT NOT NULL,
  command_sha256 TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN (
    'proposed','awaiting_approval','denied','running','completed',
    'failed','timed_out','cancelled','interrupted')),
  policy_decision TEXT,
  exit_code INTEGER,
  error_code TEXT,
  started_at_ms INTEGER,
  finished_at_ms INTEGER,
  stdout_blob_id TEXT REFERENCES audit_blobs(id) ON DELETE SET NULL,
  stderr_blob_id TEXT REFERENCES audit_blobs(id) ON DELETE SET NULL,
  output_truncated INTEGER NOT NULL DEFAULT 0 CHECK(output_truncated IN (0, 1)),
  created_at_ms INTEGER NOT NULL,
  CHECK(finished_at_ms IS NULL OR finished_at_ms >= created_at_ms)
);
CREATE INDEX IF NOT EXISTS ix_commands_host_time ON command_executions(host_id, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS ix_commands_task ON command_executions(task_id, created_at_ms);
CREATE INDEX IF NOT EXISTS ix_commands_status ON command_executions(state, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS approval_requests (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE RESTRICT,
  execution_id TEXT NOT NULL UNIQUE REFERENCES command_executions(id) ON DELETE RESTRICT,
  host_id TEXT NOT NULL REFERENCES hosts(id) ON DELETE RESTRICT,
  command_sha256 TEXT NOT NULL,
  policy_version TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('pending','approved','denied','expired','cancelled')),
  created_at_ms INTEGER NOT NULL,
  expires_at_ms INTEGER NOT NULL,
  decided_at_ms INTEGER
);
CREATE INDEX IF NOT EXISTS ix_approvals_pending ON approval_requests(state, expires_at_ms);

CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  task_id TEXT REFERENCES agent_tasks(id) ON DELETE SET NULL,
  execution_id TEXT REFERENCES command_executions(id) ON DELETE SET NULL,
  event_type TEXT NOT NULL,
  sanitized_payload_json TEXT NOT NULL DEFAULT '{}',
  prior_hash TEXT,
  event_hash TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_audit_events_time ON audit_events(created_at_ms, id);

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

