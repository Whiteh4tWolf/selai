-- Migration 0002: invariants that the v1 handoff schema could not express.
-- Each rule is also enforced by the Rust state machines in selai-domain. These
-- triggers are defence in depth: a buggy caller must not corrupt the audit trail.

-- 1. Identity is immutable.
CREATE TRIGGER IF NOT EXISTS trg_hosts_id_immutable
BEFORE UPDATE OF id ON hosts
BEGIN
  SELECT RAISE(ABORT, 'hosts.id is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_tasks_target_immutable
BEFORE UPDATE OF target_host_id ON agent_tasks
BEGIN
  SELECT RAISE(ABORT, 'agent_tasks.target_host_id is immutable');
END;

-- 2. Single-host scope. An execution must use its task's bound host.
CREATE TRIGGER IF NOT EXISTS trg_exec_scope_insert
BEFORE INSERT ON command_executions
WHEN NOT EXISTS (
  SELECT 1 FROM agent_tasks t WHERE t.id = NEW.task_id AND t.target_host_id = NEW.host_id)
BEGIN
  SELECT RAISE(ABORT, 'command execution host must equal task target host');
END;

CREATE TRIGGER IF NOT EXISTS trg_exec_scope_update
BEFORE UPDATE OF task_id, host_id ON command_executions
WHEN NOT EXISTS (
  SELECT 1 FROM agent_tasks t WHERE t.id = NEW.task_id AND t.target_host_id = NEW.host_id)
BEGIN
  SELECT RAISE(ABORT, 'command execution host must equal task target host');
END;

-- 3. An approval ticket binds one exact execution: same task, host and digest.
CREATE TRIGGER IF NOT EXISTS trg_approval_binding_insert
BEFORE INSERT ON approval_requests
WHEN NOT EXISTS (
  SELECT 1 FROM command_executions e
  WHERE e.id = NEW.execution_id AND e.task_id = NEW.task_id
    AND e.host_id = NEW.host_id AND e.command_sha256 = NEW.command_sha256)
BEGIN
  SELECT RAISE(ABORT, 'approval must match its execution task, host and command digest');
END;

-- 4. The attempt record is append-only in its identity fields.
CREATE TRIGGER IF NOT EXISTS trg_exec_record_immutable
BEFORE UPDATE OF task_id, host_id, tool_name, command_redacted, command_sha256,
                 model_id_snapshot, created_at_ms ON command_executions
WHEN NEW.task_id IS NOT OLD.task_id
  OR NEW.host_id IS NOT OLD.host_id
  OR NEW.tool_name IS NOT OLD.tool_name
  OR NEW.command_redacted IS NOT OLD.command_redacted
  OR NEW.command_sha256 IS NOT OLD.command_sha256
  OR NEW.model_id_snapshot IS NOT OLD.model_id_snapshot
  OR NEW.created_at_ms IS NOT OLD.created_at_ms
BEGIN
  SELECT RAISE(ABORT, 'command execution identity is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_approval_record_immutable
BEFORE UPDATE OF task_id, execution_id, host_id, command_sha256, policy_version,
                 expires_at_ms ON approval_requests
WHEN NEW.task_id IS NOT OLD.task_id
  OR NEW.execution_id IS NOT OLD.execution_id
  OR NEW.host_id IS NOT OLD.host_id
  OR NEW.command_sha256 IS NOT OLD.command_sha256
  OR NEW.policy_version IS NOT OLD.policy_version
  OR NEW.expires_at_ms IS NOT OLD.expires_at_ms
BEGIN
  SELECT RAISE(ABORT, 'approval ticket binding is immutable');
END;

-- 5. State machines (mirror selai_domain::{TaskState, ExecStatus, ApprovalState}).
CREATE TRIGGER IF NOT EXISTS trg_tasks_state_machine
BEFORE UPDATE OF state ON agent_tasks
WHEN NOT (
  (OLD.state = 'created' AND NEW.state IN ('validating_scope','failed','cancelled','interrupted'))
  OR (OLD.state = 'validating_scope' AND NEW.state IN ('ready','failed','cancelled','interrupted'))
  OR (OLD.state = 'ready' AND NEW.state IN ('running','failed','cancelled','interrupted'))
  OR (OLD.state = 'running' AND NEW.state IN ('awaiting_approval','completed','failed','cancelled','interrupted'))
  OR (OLD.state = 'awaiting_approval' AND NEW.state IN ('running','failed','cancelled','interrupted'))
)
BEGIN
  SELECT RAISE(ABORT, 'illegal agent_tasks state transition');
END;

CREATE TRIGGER IF NOT EXISTS trg_exec_state_machine
BEFORE UPDATE OF state ON command_executions
WHEN NOT (
  (OLD.state = 'proposed' AND NEW.state IN ('awaiting_approval','running','denied','failed','cancelled','interrupted'))
  OR (OLD.state = 'awaiting_approval' AND NEW.state IN ('running','denied','failed','cancelled','interrupted'))
  OR (OLD.state = 'running' AND NEW.state IN ('completed','failed','timed_out','cancelled','interrupted'))
)
BEGIN
  SELECT RAISE(ABORT, 'illegal command_executions state transition');
END;

CREATE TRIGGER IF NOT EXISTS trg_approval_state_machine
BEFORE UPDATE OF state ON approval_requests
WHEN NOT (
  (OLD.state = 'pending' AND NEW.state IN ('approved','denied','expired','cancelled'))
  OR (OLD.state = 'approved' AND NEW.state IN ('cancelled','expired'))
)
BEGIN
  SELECT RAISE(ABORT, 'illegal approval_requests state transition');
END;

-- 6. Only one active pinned key per host and algorithm (rotation sets replaced_at_ms first).
CREATE UNIQUE INDEX IF NOT EXISTS ux_known_keys_active
  ON known_host_keys(host_id, algorithm) WHERE replaced_at_ms IS NULL;

-- 7. Bounded preview columns. Limits are in bytes (spec: max_audit_preview 64 KiB).
ALTER TABLE command_executions ADD COLUMN stdout_preview TEXT
  CHECK (stdout_preview IS NULL OR length(CAST(stdout_preview AS BLOB)) <= 65536);
ALTER TABLE command_executions ADD COLUMN stderr_preview TEXT
  CHECK (stderr_preview IS NULL OR length(CAST(stderr_preview AS BLOB)) <= 65536);

-- 8. Metadata needed by later milestones.
ALTER TABLE hosts ADD COLUMN notes TEXT
  CHECK (notes IS NULL OR length(CAST(notes AS BLOB)) <= 4096);
ALTER TABLE provider_profiles ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1));
ALTER TABLE provider_profiles ADD COLUMN tool_probe_fingerprint TEXT;
ALTER TABLE audit_blobs ADD COLUMN mime TEXT NOT NULL DEFAULT 'application/octet-stream';

-- 9. Explicit, gap-free-by-construction ordering for the audit hash chain.
ALTER TABLE audit_events ADD COLUMN seq INTEGER;
CREATE UNIQUE INDEX IF NOT EXISTS ux_audit_events_seq ON audit_events(seq);
