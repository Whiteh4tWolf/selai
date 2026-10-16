<p align="center"><img src="apps/desktop/public/selai-mark.svg" alt="Selai logo" width="88" height="88"></p>

<h1 align="center">Selai</h1>

<p align="center">A desktop SSH workbench with a host-scoped AI assistant.</p>

Selai brings SSH hosts, terminal tabs, an AI chat, and command history into one
desktop app. You choose the machine the assistant may work on. Routine
diagnostics use fixed commands; other commands need your approval before they
run. SSH credentials stay in your operating system's credential store and are
never included in requests to an AI provider.

> **Version 0.1.0 is a preview.** The core SSH and agent flows have been tested
> end to end on Linux with an OpenSSH fixture and a local mock AI provider.
> Windows and macOS SSH use, installer runs, and live provider integrations have
> not been verified by the maintainers. See [current milestone evidence](docs/release/milestones.md)
> and [known security limits](SECURITY.md) before using Selai with sensitive
> systems.

## Features

| Area | What you can do |
| --- | --- |
| **SSH hosts** | Add, edit, filter, and archive profiles; use password or local private key authentication; save a host and connect in one flow. Labels can contain spaces. |
| **Host verification** | Inspect and explicitly trust a fingerprint on first connection. A changed host key blocks connection until you confirm its replacement. |
| **Terminal** | Open interactive SSH terminals in tabs, resize them, see connection and exit status, reconnect, and edit a host after a failed login. Open a terminal for **This computer** too. |
| **AI chat** | Discuss a question without a server, or bind a task to one SSH host or This computer. Mention a host with `@` for one message, follow the focused terminal, or pin a server in the chat. |
| **Command control** | Fixed diagnostic commands may run without a prompt. Every free-form command needs approval for the exact command and host. Denied, expired, and interrupted work is not replayed automatically. |
| **Providers** | Configure OpenAI Responses, OpenAI Chat Completions and compatible endpoints, or Anthropic Messages. Check an API key, choose or enter a model ID, and test a real tool-call round trip. |
| **History** | Review tasks and individual command attempts by host, their status and provider token counts. Filter the timeline, export redacted JSON, purge saved output previews, and verify the local audit chain. |
| **Profile import** | Review and map CSV or plaintext JSON SSH profiles, select valid rows, and import them without replacing existing hosts. Use a shared CSV template for exports from other tools. |
| **Workbench** | Use the host explorer, terminal and History/Providers tabs, and the assistant side by side. Choose dark, light, or system theme. |

### SSH hosts and terminals

Create a profile with a label, address, port, username, and authentication
method. Passwords and key passphrases are saved through the OS credential store;
for key authentication, the private key is a local file path. Selai asks you to
trust an unknown server key *before* sending credentials. It refuses a changed
key until you inspect and explicitly replace the saved fingerprint.

Each terminal tab stays bound to its host. The host explorer also includes
**This computer** for a local terminal. Terminal tabs report connection state
and exit status, and offer a reconnect action. If login fails, you can open the
host editor from the error flow.

### Assistant and approvals

The assistant has two useful modes:

1. With **No server** selected, chat is a multi-turn conversation with no
   command tools. No host is contacted. This conversation is not stored on
   disk, but its messages are sent to the configured AI provider.
2. With a server selected, a message starts one task bound to that server. Use
   the `@` picker for a specific host, or use the server associated with the
   focused terminal. You can pin a server while switching tabs. The model
   cannot switch the task to a different host.

For example, select a configured provider, type `@`, choose **Production Web**
from the picker, and ask it to `check disk usage`. Selai can run its fixed disk
diagnostic and show the result. If the assistant proposes a free-form command,
an approval card shows the exact
command, target, and expiry. You can approve or deny it. A request for root
access is handled through `sudo`; the sudo password goes to that machine's
prompt, never to the model. The same approval rules apply to **This computer**,
where an approved command runs with your local user account's access.

Command output is available in **History**, along with the task, outcome, and
token usage. Selai stores bounded, pattern-redacted output previews and an
audit record before remote execution. Redaction can miss sensitive content, so
review what a task may expose before using a cloud provider.

### AI providers

Add a provider and its base URL, API key, and model. **Check API key** queries
the endpoint for available models; you can also enter a model ID manually.
**Test** checks connectivity and whether the selected model completes a tool
call and receives a tool result. Provider keys are stored in the OS credential
store. HTTPS is required for remote endpoints unless you explicitly enable
plain HTTP for a trusted gateway; loopback HTTP is allowed.

The provider receives chat messages and, during a server task, bounded and
redacted command results. See [Security](SECURITY.md) for the data flow and
limits. Built-in adapters have been exercised against mock servers; no live
OpenAI, Anthropic, or compatible service has been verified for this preview.

### Import SSH profiles

Open **Hosts → Import SSH profiles**. Select a `.csv` or plaintext `.json`
file, check the detected column mapping and preview, select rows, and import.
The drawer provides a downloadable CSV template. It recognizes common host
columns, Bitwarden login entries with `ssh://` or `sftp://` URIs, and compatible
host-list JSON such as Termius API Bridge-shaped host objects. Passbolt CSV
and exports from other tools can be mapped to the same fields. Selai skips
non-SSH entries and existing labels or matching host/port/username
combinations; it reports failures per row.

The preview hides passwords. Imported passwords and key passphrases use the
same OS credential-store path as a manually added host. Notes are excluded by
default because host notes are stored in the local database. Private key
contents, encrypted vault files such as KDBX, and direct account connections
are not supported. Supply a path to an existing local key file instead.
Imports are limited to **2 MB and 500 rows** and do not connect to a server or
trust its host key. The source export remains wherever you saved it; handle a
plaintext export as a file containing secrets. See the [format and vendor
examples](docs/import-ssh-profiles.md).

## Get started

### Install a preview build

If a build is available, download it from [GitHub Releases](https://github.com/f3ci/selai/releases)
or the **Release packaging** workflow artifacts. The workflow is configured to
produce these packages:

| Platform | Package |
| --- | --- |
| Windows x64 | `.exe` setup or `.msi` |
| Linux x64 | `.deb` or `.AppImage` |
| macOS Apple silicon | `.dmg` |

Check the downloaded file against its accompanying `SHA256SUMS`. Linux needs a
Secret Service keyring such as GNOME Keyring or KWallet. Signing and
notarization are optional in the release workflow; an unsigned build may be
blocked or warned about by Windows or macOS. The app has no automatic updater.
See [release packaging](docs/release/packaging.md) for build outputs, checksum
commands, signing status, and workflow instructions.

### First session

1. Open **Hosts**, add an SSH profile, and choose **Save & connect**.
2. Check the server fingerprint against a trusted source, then choose **Trust & connect**.
3. Use the terminal. Open more hosts in separate tabs if needed.
4. For AI features, open **Providers**, save a provider, choose a model, and run **Test**.
5. Open the assistant with **Ctrl+Shift+L**. Select a server or type `@` to
   choose a host for one message. Review any command approval before accepting.
6. Open **History** to inspect the task and command records.

**Ctrl+Shift+E** toggles the host explorer. The status bar lets you switch
between dark, light, and system theme.

## Build from source

You need **Node.js 22**, **pnpm 10**, the stable Rust toolchain, and the native
dependencies for Tauri 2 on your platform. On Linux, the CI workflow lists the
system packages used for WebKitGTK, the keyring, and OpenSSH tests.

```sh
cd apps/desktop
pnpm install --frozen-lockfile
pnpm tauri dev
```

Run the frontend checks from `apps/desktop`:

```sh
pnpm typecheck
pnpm lint
pnpm test
pnpm build
```

Run Rust checks from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The full Linux SSH test suite starts a real `sshd` and needs root and an
unlocked Secret Service. Use the harness and setup instructions in
[CONTRIBUTING.md](CONTRIBUTING.md). A desktop bundle can be built from
`apps/desktop` with `pnpm tauri build` once the platform's Tauri dependencies
are installed.

## Documentation

- [Import format and examples](docs/import-ssh-profiles.md)
- [Security model and disclosure policy](SECURITY.md)
- [Milestone status and test evidence](docs/release/milestones.md)
- [Installer packaging and verification](docs/release/packaging.md)
- [Architecture decisions](docs/adr/)
- [Contributing and local checks](CONTRIBUTING.md)

Selai is licensed under [Apache 2.0](LICENSE).
