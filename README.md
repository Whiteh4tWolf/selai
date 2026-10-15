# Selai

Selai is an open-source desktop SSH client with a scoped AI agent. An operator
saves SSH hosts, mentions one with `@host`, and the agent runs diagnostics or
approved commands on that host. The model never receives SSH credentials, and
every AI command attempt is recorded per host.

**Status: milestone M0 (foundation).** There is no SSH connection, credential
storage or AI execution yet. What exists:

- Tauri 2 desktop shell with Connections, Terminal and AI tabs (placeholders).
- Local SQLite database with migrations, WAL mode, foreign keys, and immutable
  identity and scope rules enforced by triggers.
- Startup reconciliation: work that was live at the last shutdown is marked
  `interrupted` and is never re-run.
- Domain types: typed IDs, host labels, ports, state machines, error codes.

## Build and test

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full command list.

```sh
cargo test --workspace
cd apps/desktop && pnpm install && pnpm test && pnpm tauri build --debug --no-bundle
```

## Documentation

- [Architecture decision records](docs/adr/)
- [Milestone status](docs/release/milestones.md)
- [Security policy](SECURITY.md)

Planning documents (`AGENTS.md`, master plan, engineering spec, backlog) are the
source of truth for scope.
