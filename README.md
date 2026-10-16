# Selai

Selai is an open-source desktop SSH client with a scoped AI agent. An operator
saves SSH hosts, mentions one with `@host`, and the agent runs diagnostics or
approved commands on that host. The model never receives SSH credentials, and
every AI command attempt is recorded per host.

**Status: M0 to M5 are built. The v1.0.0 tag is not yet published (S-052 waits for
the operator). Linux is the only platform tested end to end. Windows and macOS
installers are built by the release workflow but not run here, and no live AI
provider has been verified.** Working on Linux:

- Host profiles: create, edit, archive, filter, and "Save & connect". Labels may
  contain spaces, are unique ignoring case, and keep the case you typed. Secrets
  are kept in the OS credential store, never in the database.
- Host keys: the first connect stops before any credential is sent and asks you
  to trust the server's fingerprint in the same flow ("Trust & connect"). A
  changed key is refused, and replacing it needs an explicit confirmation.
- Terminal: interactive PTY sessions over SSH in tabs with status, resize,
  exit status, reconnect, and a retry or "Edit host" step after a failed login.
  Sessions are bound to one host.
- AI providers: OpenAI Responses, OpenAI Chat Completions (and compatible
  servers), and Anthropic Messages. One Test button checks the connection and
  then a real tool-call round trip; only that marks a provider as verified.
  Plain `http://` to a non-loopback gateway (for example LiteLLM on a trusted
  network) needs the explicit "Allow plain HTTP" option.
- Startup reconciliation: work that was live at the last shutdown is marked
  `interrupted` and is never re-run.

The AI Agent is a chat console. Type `@` to mention a host (labels with spaces
work), and each message runs as one task on that host. Diagnostics run without
approval; free-form commands wait for an approval card, and the model cannot
choose the host (M3). Every attempt is recorded in that host's history, and the
audit chain can be verified. The History view (M4) shows filtered history, task
timelines, a redacted JSON export, and output purge with a retention window.

The interface is dark by default, with light and system themes in the sidebar.

## Build and test

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full command list, including the
SSH test setup that needs root on Linux.

```sh
cargo test --workspace                 # Linux: run through scripts/ci/with-keyring.sh
cd apps/desktop && pnpm install && pnpm test && pnpm tauri build --debug --no-bundle
cargo test -p selai-desktop --test perf_pty -- --ignored --nocapture   # measurements, Linux
```

## Documentation

- [Architecture decision records](docs/adr/)
- [Milestone status and evidence](docs/release/milestones.md)
- [Security policy](SECURITY.md)

Planning documents (`AGENTS.md`, master plan, engineering spec, backlog) are the
source of truth for scope.
