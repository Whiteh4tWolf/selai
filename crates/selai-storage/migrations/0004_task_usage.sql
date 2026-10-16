-- Token usage per task and a redacted preview of the request (UI revamp).
-- Tokens are summed over the task's model turns. NULL means the provider never
-- reported that count, which is different from zero.
ALTER TABLE agent_tasks ADD COLUMN input_tokens INTEGER CHECK (input_tokens IS NULL OR input_tokens >= 0);
ALTER TABLE agent_tasks ADD COLUMN output_tokens INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0);
ALTER TABLE agent_tasks ADD COLUMN model_turns INTEGER NOT NULL DEFAULT 0 CHECK (model_turns >= 0);
-- At most 500 characters, passed through the audit redactor before it is stored.
ALTER TABLE agent_tasks ADD COLUMN prompt_preview TEXT CHECK (prompt_preview IS NULL OR length(prompt_preview) <= 500);
