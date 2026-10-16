# Selai

Selai is an open-source desktop SSH client with a scoped AI agent. An operator
saves SSH hosts, mentions one with `@host`, and the agent runs diagnostics or
approved commands on that host. The model never receives SSH credentials, and
every AI command attempt is recorded per host.

**Status: M0 to M5 are built. v0.1.0 is a preview release; v1.0.0 (S-052) waits
for the operator. Linux is the only platform tested end to end. Windows and macOS
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

The AI Agent is a chat. Without a mention it is plain technical discussion with
the model: multi-turn, Markdown with code blocks, no tools and nothing run on any
server. Mention a host with `@` (labels with spaces work) and that message runs as
one task on that host. Diagnostics run without approval; free-form commands wait
for an approval card, and the model cannot choose the host (M3). Commands show as
one line each in the chat; their output stays in History. Every attempt is
recorded in that host's history, and the audit chain can be verified.

History (M4) opens on tasks: the request, the commands it ran, and the input and
output tokens the provider reported, with per-host totals. A Commands view keeps
the filters, task timelines, the redacted JSON export, and output purge with a
retention window.

When adding a provider, "Check API key" tests the key and loads the endpoint's
model list into a dropdown, with a fallback to type an ID by hand.

The interface is dark by default, with light and system themes in the sidebar.

## Install

Download the installer for your system from the repository's **Releases** page
(Windows `.exe`/`.msi`, Linux `.deb`/`.AppImage`, macOS `.dmg`) and check it
against `SHA256SUMS`. The installers are not code-signed yet, so Windows
SmartScreen and macOS Gatekeeper warn on the first start. The release notes say
how to get past that.

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
