# Security regression suite (S-041)

Each threat in the engineering spec (section 9) maps to a test below, or is
marked as not covered. Test names are the ones in the repository.

| Threat | Control | Test |
|---|---|---|
| Model asks for another host | Tool schema has no host field; task host is fixed | `selai-agent/tests/runtime.rs`: `a_host_argument_from_the_model_is_refused_and_audited`; `selai-policy`: `a_request_for_another_host_is_denied_whatever_the_tool` |
| Remote output says "ignore instructions" | Output is data; it grants nothing | `runtime.rs`: `instructions_in_remote_output_grant_nothing` |
| Tool call written as JSON inside output | Tool calls come only from the provider's structured response | `runtime.rs`: `tool_call_json_inside_output_is_not_executed` |
| Secret in output reaches the model | Output redacted before it is stored or sent | `runtime.rs`: `redacted_output_is_what_the_model_receives`; `history_e2e.rs`: `stored_output_is_redacted_and_the_export_never_contains_the_secret` |
| Shell syntax in a diagnostic argument | Fixed operations; service names checked against a strict pattern | `selai-policy/src/diagnostics.rs`: `hostile_service_names_are_refused_for_every_operation`, `hostile_operations_and_argument_mixes_are_refused` |
| Credential in provider requests | Secret store only in Rust; key in header only | `agent_e2e.rs`: `no_credential_reaches_the_model_and_history_survives_a_restart`; `selai-llm/tests/adapters.rs`: `key_travels_in_the_header_and_never_in_the_body` |
| Key in logs or debug output | Secret types have no `Debug`; DTOs with secrets derive only `Deserialize` | Compile-time: `selai-secrets/src/value.rs` (no `Debug`); `dto.rs` (`HostInput`, `ProviderInput`). No log output exists to scan yet |
| Free-form command without approval | Approval bound to digest, host and task; re-read from the database | `agent_e2e.rs`: `approved_command_runs_only_after_the_operator_approves`, `denied_command_never_runs`; `runtime.rs`: `an_expired_approval_never_reaches_the_host` |
| Approval swapped after review (TOCTOU) | Digest and host checked again before running | `storage/tests/agent.rs`: `approval_binds_to_one_execution_digest_and_host`; `runtime.rs`: `a_denied_command_never_reaches_the_host` |
| Audit write fails before a command | Command is refused when its record cannot be written | `runtime.rs`: `no_command_runs_when_its_audit_record_cannot_be_written` |
| Step cannot be checkpointed | Task stops before any command runs | `runtime.rs`: `a_step_that_cannot_be_recorded_stops_the_task_before_any_command` |
| Audit edited on disk | Hash chain detects an edited row | `selai-storage/src/tests.rs` and `tests/agent.rs`: `the_audit_chain_verifies_and_detects_an_edited_row` |
| MITM or changed host key | Pinned key; connection blocked before any credential | `selai-ssh/tests/ssh_integration.rs`: `unpinned_host_is_blocked_before_any_credential_is_sent`, `changed_host_key_is_blocked_and_reports_the_pin` |
| Credential exfiltration by redirect | Redirects not followed | `selai-llm/tests/adapters.rs`: `redirects_are_not_followed_so_the_key_stays_at_the_origin` |
| Compromised webview | Strict CSP; only `core:default` capability; main window only | `apps/desktop/src-tauri/tests/webview_hardening.rs` (3 tests, read from the shipped config) |
| Incomplete or malformed tool JSON | Nothing executes until the call is complete and valid | `selai-llm/tests/adapters.rs`: `chat_malformed_arguments_never_execute`, `anthropic_invalid_partial_json_is_refused`; `selai-llm/src/assembler.rs`: `truncated_json_never_becomes_a_call` |
| Keychain unavailable | Fail closed, no plaintext fallback | `service_e2e.rs`: `locked_store_fails_closed_and_writes_nothing`; `provider_e2e.rs`: locked store test |
| Hostile slow server | Timeouts, capped output, cancellation | `ssh_integration.rs`: `silent_server_hits_the_connect_timeout`, `exec_output_is_capped_and_marked_truncated`, `exec_timeout_closes_the_channel_and_says_so` |
| Agent continues after Stop | Cancellation checked before the next tool | `runtime.rs`: `cancelling_stops_the_next_tool_call` |

## Known gaps

- **Output redaction is pattern-based.** `selai-audit/src/redact.rs` replaces
  values of keys whose name contains `password`, `passwd`, `pwd`, `passphrase`,
  `token`, `secret`, `api key` or `authorization`; PEM private key blocks; and
  `Bearer` tokens.
  - False negatives: a secret with a neutral key name (for example
    `DATABASE_URL=postgres://user:pass@host` or a bare token printed by a tool),
    a password in a plain-text prompt, personal data, and certificates.
  - False positives: keys that merely contain a listed word (for example
    `tokenizer=…`) are redacted even when harmless.
  - The export says it cannot find every sensitive value. Operators should read
    an export before sharing it. Neither output previews nor exports are
    guaranteed clean.
- Remote output is sent to the configured model provider as data. A cloud
  provider sees it. This is the designed trade-off, documented in `SECURITY.md`.
- Audit blobs are not encrypted (S-040 deferred); only redacted previews are
  stored.
- No log scan exists because the app writes no logs. The rule "no secret in
  logs" is enforced by types, not by a test.
- Not covered: approval TOCTOU across an app restart mid-approval (a restart
  cancels open approvals; the test for that is in `selai-storage/tests/history.rs`
  for expiry only), and concurrent tasks on one host.
