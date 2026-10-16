# Acceptance matrix (S-051)

Status values: **pass** (automated test ran and passed on Linux in this
repository), **partial** (part of the case is covered), **manual** (only a
manual check was done, recorded in `milestones.md`), **not run** (not executed in
this release), **not implemented**.

Live-provider smoke checks were skipped: no provider keys were available. No
case below claims live-provider validation.

| ID | Scenario | Status | Evidence / gap |
|---|---|---|---|
| T-SSH-001 | Create password profile, connect, run shell | pass | `service_e2e.rs` terminal tests; manual in-app check (M1) |
| T-SSH-002 | Private-key profile, optional passphrase | pass | `ssh_integration.rs` Ed25519 and encrypted-key login; `service_e2e.rs`: `private_key_host_uses_the_stored_passphrase` |
| T-SSH-003 | Unknown fingerprint needs explicit trust | pass | `service_e2e.rs`: `first_contact_is_blocked_until_the_user_trusts_the_probed_key` |
| T-SSH-004 | Changed host key blocks the connection | pass | `ssh_integration.rs`: `changed_host_key_is_blocked_and_reports_the_pin`; `service_e2e.rs`: `changed_key_is_refused_and_replacement_is_explicit` |
| T-SSH-005 | Disconnect and reconnect | partial | Manual restart and reconnect (M1). No automatic reconnect is implemented. |
| T-SSH-006 | Five PTYs with `top` and `vim` | partial | Five terminals streaming and echo timed (`perf_pty.rs`, numbers in `performance.md`). `top` and `vim` not run. |
| T-MODEL-001 | OpenAI native adapter tool-call round trip | partial | Mock server (`adapters.rs`, Chat and Responses). Live API not run. |
| T-MODEL-002 | Anthropic native tool use and result sequence | partial | Mock server (`adapters.rs`). Live API not run. |
| T-MODEL-003 | Compatible endpoints (GLM, Kimi, LiteLLM, vLLM, Ollama) | partial | Probe tests with mock servers (`probe.rs`). No live endpoint run. |
| T-MODEL-004 | Unsupported tool model: no SSH execution | pass | `agent_e2e.rs`: `archived_hosts_and_unverified_providers_are_refused_before_any_connection`; `probe.rs` |
| T-MODEL-005 | API key never in SQLite, list, audit or logs | pass | `provider_e2e.rs`: `key_is_stored_outside_the_database_and_never_returned` |
| T-MENTION-001 | `@host` binds an immutable host id | pass | `agent_e2e.rs`; `selai-storage/tests/hosts.rs` (id immutable) |
| T-MENTION-002 | Rename keeps history on the same id | pass | `history_e2e.rs`: `history_survives_rename_and_archive_and_exports_as_archived` |
| T-MENTION-003 | Model requests unmentioned host: denied | pass | `runtime.rs`: `a_host_argument_from_the_model_is_refused_and_audited` |
| T-MENTION-004 | Multiple hosts in one task: UI refuses | partial | The UI holds one host chip, so a second host cannot be added. No test asserts the refusal message. |
| T-AGENT-001 | Disk diagnosis from a real `df` | pass | `agent_e2e.rs`: `disk_usage_runs_on_the_bound_host_and_is_recorded` |
| T-AGENT-002 | Five-step troubleshooting, five records | pass | `agent_e2e.rs`: `five_diagnostics_are_each_recorded_and_the_chain_verifies`; `runtime.rs`: `five_sequential_diagnostics_are_each_recorded_and_the_chain_verifies` |
| T-AGENT-003 | Arbitrary command needs per-execution approval | pass | `agent_e2e.rs`: `approved_command_runs_only_after_the_operator_approves` |
| T-AGENT-004 | Denied approval: no execution, denial recorded | pass | `agent_e2e.rs`: `denied_command_never_runs`; `history_e2e.rs`: denied row in filters |
| T-AGENT-005 | Stop during a slow command: no next tool; honest termination | partial | Next tool stopped (`runtime.rs`: `cancelling_stops_the_next_tool_call`); the honest "remote process may still run" note is in the runner. A slow remote command was not stopped end to end in the app. |
| T-AGENT-006 | App killed during a task: `interrupted` on start, no replay | partial | Startup reconciliation tests (`selai-storage/tests/history.rs`, `tests.rs`). Process kill was not run against the app. |
| T-AUDIT-001 | Pre-execution DB write failure: no command reaches the host | pass | `runtime.rs`: `no_command_runs_when_its_audit_record_cannot_be_written` |
| T-AUDIT-002 | SSH auth error or disconnect: failed attempt saved with code | not implemented | An auth failure happens before a task is created, so no task or execution row is written. The spec wants a saved attempt. |
| T-AUDIT-003 | Massive output: bounds, truncation flag, UI backpressure | partial | Output capped and flagged (`ssh_integration.rs`: `exec_output_is_capped_and_marked_truncated`; `output_truncated` in history). UI backpressure not implemented (unbounded event channel). |
| T-SEC-001 | SSH credential absent from provider payloads | pass | `agent_e2e.rs`: `no_credential_reaches_the_model_and_history_survives_a_restart` |
| T-SEC-002 | Prompt injection in stdout grants nothing | pass | `runtime.rs`: `instructions_in_remote_output_grant_nothing` |
| T-SEC-003 | Keychain unavailable: fail closed | pass | `service_e2e.rs`: `locked_store_fails_closed_and_writes_nothing`; `provider_e2e.rs` |
| T-SEC-004 | Malicious provider redirect does not forward the key | pass | `adapters.rs`: `redirects_are_not_followed_so_the_key_stays_at_the_origin` |
| T-SEC-005 | Incomplete tool JSON never executes | pass | `adapters.rs`: `chat_malformed_arguments_never_execute`; `assembler.rs` tests |

## Platform matrix

| Platform | Build | Tests | Installer |
|---|---|---|---|
| Linux (sandbox VM) | pass | pass, including SSH and desktop end-to-end | `deb` and `AppImage` built by the release workflow; not yet installed and tested here |
| Windows | not run | not run here (CI runs the non-SSH suites) | not run |
| macOS | not run | not run here (CI runs the non-SSH suites) | not run |

Keychain integration was tested only with the Linux Secret Service. Windows
Credential Manager and macOS Keychain are not tested in this release.

## Release criteria status

- P0 and P1 defects open: none recorded in this repository's tests. Gaps above
  are listed, not hidden.
- `cargo fmt`, `clippy -D warnings`, `cargo test --workspace`, frontend typecheck,
  lint, tests, build: pass (see `milestones.md`, M4 evidence).
- `cargo deny check`: pass.
- Installers: the release workflow is written and was checked for syntax. It has
  not been run on GitHub for Windows or macOS.
- Signing and notarization: not configured. Artifacts are unsigned.
- Security signoff by maintainers: **not done**. This document is the input for it.
