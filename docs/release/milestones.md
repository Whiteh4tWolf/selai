# Milestone status

Status values: **done** (test evidence recorded below), **partial** (missing
items listed), **not started**. Evidence is the command and result at the time
of the last update.

## M0 — Foundation

| Task | Status | Gaps |
|---|---|---|
| S-001 Scaffold monorepo and desktop UI | partial | Windows and macOS builds are in CI but were not run here. |
| S-002 CI, ADRs, security baseline | partial | ADR-0004 and ADR-0005 are not written (see `docs/adr/README.md`). Gitleaks has not been run locally. |
| S-003 SQLite repository and migration | partial | Pagination is implemented for hosts only. Other repositories are not written yet. |

## M1 — Working SSH client (`v1.0.0-alpha.1` candidate)

| Task | Status | Evidence | Gaps |
|---|---|---|---|
| S-010 Host CRUD, labels, groups | done | `selai-storage/tests/hosts.rs` (15), `selai-domain` label and host-field tests; UI host list, add, edit, archive exercised through the real app | Group UI is not built (groups exist in the API). |
| S-011 Native credential store | done on Linux | `selai-secrets/tests/keyring_backend.rs` against a live Secret Service; in-app check: secret stored under `service=selai, username=ssh/<id>/password`; database has zero occurrences of the password | macOS Keychain and Windows Credential Manager tests run in CI but were not run here. Locked-store behaviour is tested with an in-memory store only. |
| S-012 SSH transport, host-key verification | done on Linux | 13 tests against OpenSSH 9.6 (`selai-ssh/tests/ssh_integration.rs`): unknown key blocks before auth, changed key blocks before auth, password and Ed25519 and encrypted-key login, wrong password, RSA refused before auth, connect timeout, unreachable port | ECDSA keys are not tested. Algorithm negotiation is tested against OpenSSH 9.6 only. |
| S-013 Interactive PTY | partial | In-app: prompt, command output, exit status 0 reported; restart and reconnect works; service test with five concurrent sessions bound to their own hosts; resize reaches the PTY | `vim` and `top` are not tested. Backpressure is not implemented: the IPC queue is unbounded. Terminal scrollback is bounded by xterm (5000 lines). |

### M1 milestone gate

The gate is: configure `production-web`, trust its fingerprint, connect by
password and by key, use the terminal, restart Selai, and use it again.

Checked on Linux against a real `sshd` (fixture in `selai-testkit`):

- Password host created in the UI, key probed, fingerprint trusted, shell opened,
  command run, exit status reported (`exit 0`).
- Application restarted with the same data directory and keychain. Connect worked
  with no re-trust and no password entry. Shell ran `restarted-42`.
- Encrypted-key login with the passphrase from the OS store: service-level test.

Not yet checked: the gate on macOS and Windows, and a real host other than the
test fixture.

### Test evidence (last run)

- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `scripts/ci/with-keyring.sh cargo test --workspace` (as root, so sshd can run):
  domain 23, secrets 5 (4 contract + 1 keyring), selai-ssh 13, storage 22 unit
  + 15 integration, desktop unit 2, desktop end-to-end 10. All passed.
- Frontend: typecheck, lint, 11 tests, build: pass.
- `cargo deny check advisories licenses bans sources`: pass. One ignored
  advisory, RUSTSEC-2023-0071, with the reason in `deny.toml`.

## M2 — Universal AI provider configuration

| Task | Status | Evidence | Gaps |
|---|---|---|---|
| S-020 Provider CRUD, secure keys | done | `selai-storage/tests/providers.rs` (6), `selai-desktop/tests/provider_e2e.rs` (10): key stored in the OS store, never in the database, responses, or request bodies; locked store fails closed; delete removes the key. In-app: provider created through the UI, key field write-only | Edit and delete were checked in tests; not every edit path was clicked in the app. |
| S-021 OpenAI Responses and Chat adapters | done against mock servers | `selai-llm/tests/adapters.rs`: text streams, fragmented tool arguments, malformed JSON never executes, cut streams fail, redirects not followed, 429 retried only before output, 401 and tool rejections classified, cancellation | Not verified against the live OpenAI API (no key supplied). |
| S-022 Anthropic Messages adapter | done against mock servers | `adapters.rs`: `tool_use` with partial JSON, parallel calls, `tool_result` ordering (merged into one user message), error events | Not verified against the live Anthropic API. |
| S-023 Compatible templates and probes | partial | `probe.rs` (6): verified only after a full round trip; text-only and refusing models reported as not verified. Templates in `selai-llm/src/templates.rs`, with notes | GLM and Kimi base URLs come from third-party docs, not the vendor's. No live smoke test for any compatible server. Model IDs are never hardcoded. |

### M2 milestone gate

The gate is: a model produces a genuine tool call and receives the tool result
through the selected provider, with no SSH involved.

Checked in the app: a provider pointed at a local OpenAI-compatible mock
(`/tmp/mock_provider.py`, a local script, not part of the repo) was created
through the UI. "Test tool calling" showed *tool calling verified for this
model*. The round trip was real: a tool call, the tool result sent back, and a
final answer.

**Not verified against any live provider.** The `live` tests in
`crates/selai-llm/tests/live.rs` are ignored until someone supplies keys (see
CONTRIBUTING.md). Until they pass, every provider is reported as not verified
against a live endpoint.

### Test evidence (last run)

- `cargo test --workspace` through the keyring harness: domain 29, secrets 5,
  selai-llm 9 unit + 21 adapter + 6 probe, storage 22 + 15 + 6, SSH 13, desktop
  unit 2, service e2e 10, provider e2e 10. All passed. Ignored: 3 live provider
  tests.
- Frontend: 14 tests, typecheck, lint, build pass.
- `cargo deny check advisories licenses bans sources`: pass. Added
  CDLA-Permissive-2.0 for the Mozilla CA bundle (reason in `deny.toml`).

## M3 — Scoped AI agent (`v1.0.0-alpha.2` candidate)

| Task | Status | Evidence | Gaps |
|---|---|---|---|
| S-030 Mention picker and host binding | done | Chip stores the host id and a label snapshot; the model never chooses the host (`selai-agent/src/tools.rs` has no host argument). Picker: arrow keys, Enter and click are tested (`AgentPanel.test.tsx`); Escape is not. Archived host refused (`agent_e2e.rs`). The host is bound by id, so a rename cannot retarget a running task by design, but no test covers a rename. | The picker accepts one host per request (multi-target refused). Free text with an `@name` that was not picked is sent as plain text and binds no host. |
| S-031 Policy and approval | done | `selai-policy` (12): diagnostics map to fixed commands (`df -hP`, `uptime`, ...), free-form commands always need approval, out-of-scope host denied. Approval tickets are bound to task, host and command digest, and expire. Runtime tests (`selai-agent/tests/runtime.rs`, 14): tampered command, changed host and expired ticket all refuse. `agent_e2e.rs`: approve runs the exact command, deny never runs it, cancel while waiting never runs it. | There are no policy roles; there is one policy. The UI disables Approve once an approval expires only when the card re-renders; the database refuses the decision either way. |
| S-032 Audit before execution | done | `selai-audit` (11): redaction and sha256 hash chain. `selai-storage/tests/agent.rs` (6): execution row and audit event written in one transaction before any SSH I/O. `agent_e2e.rs`: five diagnostics each recorded, chain verifies, history survives a restart. Model payloads checked for the SSH password and the provider key. | Blob encryption of full output is deferred to S-040. Only bounded, redacted previews are stored. |
| S-033 Agent runtime and SSH bridge | done on Linux | `selai-ssh` exec (in `ssh_integration.rs`, 17 SSH tests in total), output capped per stream, cancellation. `agent_e2e.rs` (7) runs against real OpenSSH and a mock provider that must pass tool calling first. | Checkpoints now written (see M4). No automatic reconnect. No trust grants. Only Linux was run. |

### M3 milestone gate

The gate is: in the app, type `@production-web check disk usage`, see the
result from the real host, approve a free-form command and see its result, and
find the command in that host's history after a restart.

Checked by the desktop end-to-end suite (`apps/desktop/src-tauri/tests/agent_e2e.rs`)
against a real `sshd` and a mock provider: the disk check runs on the bound host
and is recorded; the approved command runs only after approval; the denied and
cancelled commands never run; history and audit survive a restart.

**Not checked in a real window in this session.** The React panel is covered by
component tests with mocked IPC (`AgentPanel.test.tsx`, 7 tests; transcript reducer, 6), not by driving
the running application. **Not verified against any live provider.** The
`live` tests stay ignored until keys are supplied. macOS and Windows were not run.

Bug found and fixed while building M3: an approval click that arrived before the
task began waiting was dropped, and the approval then expired. The registry now
keeps an early decision for the next wait (`agent_service.rs`, tests). Found
during review, not by a failing test.

### Test evidence (last run)

- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.
- `scripts/ci/with-keyring.sh cargo test --workspace --locked` (as root, so sshd
  runs): all passed. Per target: domain 29, selai-llm 9 unit + 21 adapter + 6
  probe, policy 12, audit 11, agent runtime 14, storage 22 unit + 15 hosts + 6
  providers + 6 agent, SSH 17, secrets 1 keyring + 4 contract, desktop unit 5,
  desktop end-to-end: service 10, provider 10, agent 7. Ignored: 3 live provider
  tests.
- Frontend: typecheck, lint, 27 tests, `vite build`: pass.
- `cargo deny check`: advisories, bans, licenses, sources ok.

## M4 — History, privacy and hardening (`v1.0.0-beta.1` candidate)

| Task | Status | Evidence | Gaps |
|---|---|---|---|
| S-040 Per-host history UI and export | done (not run in a real window) | `apps/desktop/src/tabs/HistoryTab.test.tsx` (5); `apps/desktop/src-tauri/tests/history_e2e.rs` (6): filters, pagination, timeline, rename and archive keep history, export without the secret, purge, preferences. `selai-storage/tests/history.rs` (8). | Encrypted audit blobs deferred (S-040 spec). Export is capped at 1000 rows and says so. The `cancelled` filter is not covered end to end. |
| S-041 Provider and host security regression | done, with limits | `docs/release/security-regression.md` maps each spec threat to a test. New: hostile output (`runtime.rs`, 3 tests), hostile diagnostic inputs (`selai-policy`, 3 tests), webview hardening (3 tests). | Redaction is pattern-based; limits are documented there. |
| S-042 Persistence, recovery, budgets | partial | Startup reconciliation marks running work `interrupted`, expires stale approvals, and writes an audit event (`history.rs`). Output retention: a sweep at startup (default 30 days, configurable) and a manual purge; both audited, metadata kept. History indexes in migration 0003. Task budgets in `Limits`. Provider choice remembered. | No total size cap, only age. Process kill during a task was not tested against the app. |
| S-043 Performance and cross-platform validation | partial | `apps/desktop/src-tauri/tests/perf_pty.rs` and `docs/release/performance.md`: five terminals, four streaming, echo latency and throughput on the sandbox VM. | Input responsiveness during an AI stream not measured. Keychain tested on Linux only. Memory measured on Linux only. |

## M5 — RC / GA release (`v1.0.0` candidate)

| Task | Status | Evidence | Gaps |
|---|---|---|---|
| S-050 Installer packaging and signing pipeline | partial | `.github/workflows/release.yml` builds Linux (deb, AppImage), Windows and macOS bundles. Checksums (`SHA256SUMS`), CycloneDX SBOMs (Rust and frontend), and build provenance on tags. `actionlint` and `shellcheck` clean. `docs/release/packaging.md`. SBOMs generated locally from a clean archive. | Workflow not run on GitHub. Windows and macOS installers not built or installed here. Not signed or notarised: signing runs only when secrets are set. `.ps1` scripts not executed. No auto-updater (by design). |
| S-051 Acceptance matrix and security signoff | partial | `docs/release/acceptance-matrix.md`: every `T-*` case from the spec, with its status. | Live-provider checks skipped (no keys). Several cases partial or not implemented (T-AUDIT-002, T-SSH-005, T-AGENT-006 process kill, T-AUDIT-003 UI backpressure). Maintainer security signoff not done. |
| S-052 Tag and publish v1.0.0 | not done | — | Waiting for the operator's confirmation. Tagging and publishing are outward-facing. |

### Test evidence (last run)

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.
- `scripts/ci/with-keyring.sh cargo test --workspace --locked`: exit 0, 39 test binaries passed. New in M4: `history_e2e` 6, storage `history` 8, storage upgrade test 1, runtime 19 (3 security, 2 checkpoint), policy 15, desktop unit 5, `webview_hardening` 3, `perf_pty` ignored (run separately).
- Frontend: typecheck, lint, 32 tests, `vite build`: pass.
- `cargo deny check advisories licenses bans sources`: pass.
- `actionlint` on `.github/workflows/release.yml` and `ci.yml`: pass.

## M6 — Not planned

Nothing beyond M5 is started. The master plan lists v1.1 and later work (SFTP, tunnels, multi-host AI, and so on).
