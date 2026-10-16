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

## M2 to M5

Not started. See `SELAI_EXECUTION_BACKLOG.md`.
