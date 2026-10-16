-- Per-host history, output retention and startup reconciliation (S-040, S-042).
-- Indexes for filtered history and for the retention sweep.
CREATE INDEX IF NOT EXISTS ix_commands_host_state
  ON command_executions(host_id, state, created_at_ms DESC, id DESC);
CREATE INDEX IF NOT EXISTS ix_commands_finished
  ON command_executions(finished_at_ms)
  WHERE finished_at_ms IS NOT NULL;
CREATE INDEX IF NOT EXISTS ix_tasks_host_time
  ON agent_tasks(target_host_id, created_at_ms DESC, id DESC);
