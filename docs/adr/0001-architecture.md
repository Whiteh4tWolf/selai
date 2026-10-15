# ADR-0001: Foundation architecture and resolved document conflicts

- **Status:** Accepted (M0)
- **Date:** 2026-10-15
- **Scope:** M0 foundation, touching S-001, S-002, S-003

## Context

The handoff pack specifies the v1.0 design across five documents. Building M0
forced several choices that the documents leave open or disagree on. AGENTS.md
requires each conflict to be logged here with the resolution selected.

## Decisions

### D1. Single Tauri desktop process with a Rust workspace

- Tauri 2 (stable 2.x line, `tauri` 2.12 at time of writing). The crates.io index
  currently lists `tauri` 3.0.0-alpha as the newest version; alpha releases are
  not used.
- React 19, TypeScript 5, Vite 8, xterm.js 6. TypeScript 7 is avoided until
  `typescript-eslint` supports it.
- Crate layout follows master plan section E. M0 creates `selai-domain` and
  `selai-storage`. `selai-secrets`, `selai-ssh`, `selai-llm`, `selai-agent`,
  `selai-policy` and `selai-audit` are created by their milestones.
- The Tauri crate (`apps/desktop/src-tauri`) is the composition root only. It
  holds no business rules.

### D2. SQLite via `rusqlite` with bundled SQLite

- `rusqlite` bundles SQLite 3.45.x. The bundled build removes a system-library
  dependency on all three OSes.
- Connection settings are applied in code, not in migrations, because
  `PRAGMA journal_mode` and `foreign_keys` have no effect inside a transaction.
  Every file database runs `journal_mode=WAL`, `synchronous=FULL` and
  `foreign_keys=ON`, and checks each value after applying it.
- `synchronous=FULL` is chosen so a committed audit row survives power loss
  before any SSH I/O. This is slower than `NORMAL`; the cost is accepted.

### D3. Migrations are numbered SQL files

- `0001_initial.sql` is the handoff `schema_v1.sql` with two changes: the
  connection pragmas are removed, and the trailing bookkeeping insert is removed.
  Table definitions are otherwise unchanged.
- `0002_invariants.sql` adds rules the handoff schema could not express. These
  are enforced by triggers and mirror the Rust state machines:
  - `hosts.id` and `agent_tasks.target_host_id` cannot change.
  - An execution and an approval must use the task's bound host. An approval
    must also match its execution's task, host and command digest.
  - Command identity fields (`command_redacted`, `command_sha256`,
    `tool_name`, `model_id_snapshot`, `created_at_ms`) are immutable.
  - Task, execution and approval `state` changes follow the transition tables.
    Terminal states are final, so a completed command cannot run again.
  - One active pinned key per `(host, algorithm)`.
  - Byte-length limits on previews (64 KiB) and host notes (4 KiB).
  - Columns for `notes`, `enabled`, `tool_probe_fingerprint`, `mime` and the
    audit `seq` column.

### D4. Startup reconciliation runs before anything else

- `reconcile_after_restart` marks every live task, execution and approval as
  `interrupted` or `cancelled` in one transaction. Nothing is re-run.
- Interrupted executions carry `error_code = TASK_INTERRUPTED`.

### D5. Error codes

- The spec §13 taxonomy is used unchanged. The spec defines no code for
  storage errors or for an interrupted task, so the following extensions were
  added: `TASK_INTERRUPTED`, `STORAGE_BUSY`, `STORAGE_CONFLICT`,
  `STORAGE_CONSTRAINT`, `STORAGE_IO`, `INVALID_INPUT`, `NOT_FOUND`, `INTERNAL`.
- Codes serialize to the same string as `as_str()`. A test enforces this.

## Conflicts found in the handoff documents and how they were resolved

| # | Documents | Conflict | Resolution |
|---|---|---|---|
| C1 | Engineering spec §3.1 vs backlog S-010 | Spec requires lower-case slug `[a-z][a-z0-9_-]{1,47}`; backlog requires `production-web` and `PRODUCTION-WEB` to conflict. | Input is case-folded to lower case on parse, so both forms are one label. Matches both documents. |
| C3 | Schema file vs spec §8 | Schema has no audit-output preview columns, but F-03 requires stdout/stderr previews in history. | Added bounded `stdout_preview` / `stderr_preview` (migration 0002). |
| C4 | Schema file vs spec §3.1, §8 | Host `notes` and provider `enabled` are missing. | Added in migration 0002. |
| C5 | Spec §8 vs AGENTS.md | `audit_events` has no ordering column for the hash chain; `created_at_ms, id` is ambiguous. | Added `seq` with a unique index. The writer must assign it. |
| C6 | Spec §3.2 vs schema | No rule for host hard delete. | Deferred. `ON DELETE RESTRICT` keeps history; hard delete needs an explicit confirmed action and is decided in ADR-00xx before S-010 ships. |
| C7 | Spec §6.3 vs schema | Trust grants (bounded per-host approval without a prompt) have no table. | Not implemented in v1.0 M0. Autorun is limited to the fixed diagnostic set until a grant ADR is accepted. |
| C8 | Backlog S-002 vs repository state | S-002 lists ADR-001 to ADR-005 (platform, SSH, credentials, provider protocols, policy/audit). | This ADR covers platform and storage. ADR-002 to ADR-005 are **not yet written** and are required before S-011, S-012 and S-020 start. See `docs/adr/README.md`. |
| C9 | Master plan vs spec | Master plan date 2026-10-16 is after the current date 2026-10-15. | Treated as a typo; no effect on scope. |
| C10 | Schema file vs AGENTS.md "pagination API" | Backlog S-003 asks for a pagination API; no repository exists yet to paginate. | Pagination types exist in `selai-domain::page`; the repository API is part of S-010, so S-003 is marked partial. |

## Consequences

- Other agents can rely on: typed IDs with strict parsing, the normalized label,
  the transition tables, and the database invariants above.
- Migrations are forward-only. A database from a newer build is refused, not
  downgraded (`SchemaTooNew`).
- Any change to a trigger in `0002` needs a new migration, not an edit.

## Fallback

If the bundled `rusqlite` build fails on a target OS, the fallback is a system
SQLite with the same version floor. Do not drop the settings checks in
`Database::open`.

## Impact on other agents

- Agent B (storage and secrets) owns migrations and must add tests for each new
  one. Agent D (agent and policy) must write `command_executions` rows through
  the repository, not with raw SQL.
