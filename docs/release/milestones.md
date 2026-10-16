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

## M3 to M5

Not started. See `SELAI_EXECUTION_BACKLOG.md`.
