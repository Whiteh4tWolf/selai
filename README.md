# Selai

Selai is an open-source desktop SSH client with a scoped AI agent. An operator
saves SSH hosts, mentions one with `@host`, and the agent runs diagnostics or
approved commands on that host. The model never receives SSH credentials, and
every AI command attempt is recorded per host.

**Status: milestone M1 (working SSH client), partially complete.** Working on
Linux:

- Host profiles: create, edit, archive, filter. Labels are unique and
  case-insensitive. Secrets are kept in the OS credential store, never in the
  database.
- Host keys: read the server key, trust it explicitly, pin it, and replace a
  changed key only with a separate confirmed action. Connections to unpinned or
  changed keys are refused before any credential is sent.
- Terminal: interactive PTY sessions over SSH, with resize, several tabs, and
  exit status. Sessions are bound to one host.
- Startup reconciliation: work that was live at the last shutdown is marked
  `interrupted` and is never re-run.

Not yet built: AI providers (M2), the `@host` agent and command approval (M3),
the per-host AI history UI (M4), and installers (M5). The AI tab is a
placeholder.

## Build and test

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full command list, including the
SSH test setup that needs root on Linux.

```sh
cargo test --workspace                 # Linux: run through scripts/ci/with-keyring.sh
cd apps/desktop && pnpm install && pnpm test && pnpm tauri build --debug --no-bundle
```

## Documentation

- [Architecture decision records](docs/adr/)
- [Milestone status and evidence](docs/release/milestones.md)
- [Security policy](SECURITY.md)

Planning documents (`AGENTS.md`, master plan, engineering spec, backlog) are the
source of truth for scope.
