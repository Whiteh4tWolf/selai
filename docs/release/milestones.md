# Milestone status

Status values: **done** (has passing test evidence), **partial** (some
deliverables missing, listed), **not started**. Test evidence is the command and
result recorded at the time of the last change.

## M0 — Foundation

| Task | Status | Evidence / gaps |
|---|---|---|
| S-001 Scaffold monorepo and desktop UI | partial | Tauri 2 app builds and launches under Xvfb; DB opens at startup; three tabs render. Gaps: Windows and macOS not built here; CI matrix runs them but has not run in this environment. Terminal is a placeholder with no PTY (S-013). |
| S-002 CI, ADRs, security baseline | partial | CI workflow, `deny.toml`, SECURITY.md, CONTRIBUTING.md, PR template, ADR-0001 written. Gaps: ADR-0002 to 0005 not written; `cargo deny` run locally (see below); gitleaks not run here; `CODEOWNERS` not set. |
| S-003 SQLite repository and migration | partial | Migrations 0001 and 0002 apply on a clean install; WAL, foreign keys and busy mapping tested; startup reconciliation tested; label uniqueness tested. Gaps: repository and pagination API deferred to S-010. |

### Test evidence (M0)

Recorded when this section was last updated:

- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `cargo test --workspace`: selai-domain 17 passed; selai-storage 22 passed
- `pnpm typecheck`, `pnpm lint`, `pnpm test` (3 passed), `pnpm build`: pass
- `pnpm tauri build --debug --no-bundle` then launch under Xvfb for 25 s: no
  crash; database created with schema versions 1 and 2, `journal_mode = wal`,
  `foreign_key_check` empty; screenshot shows the UI with schema and WAL status
  read from Rust.

## M1 to M5

Not started. See `SELAI_EXECUTION_BACKLOG.md` for the task list.
