Selai is a desktop SSH client with a scoped AI agent: save hosts, open terminals,
chat with a model, and mention a host with `@` to let the agent check it. The
model never receives SSH credentials, and every command it runs is recorded.

**This is a preview release.** Automated tests cover Linux SSH/agent flows with
fixtures and mock providers. Windows packaging also checks encryption and the
vault in the release build profile. This is not complete interactive coverage
of every platform and live AI provider. The installers are not code-signed.

**New in 0.2.0:** a network workspace with SFTP, saved port forwarding, and
reusable HTTP CONNECT/SOCKS5 proxies.

- **SFTP:** local/remote file browsers, recursive upload/download, byte and file
  progress, speed and ETA, a bounded queue, cancellation, retry, hidden files,
  filtering, folder creation, rename, and file/empty-folder deletion. Existing
  files are skipped by default; replacements require confirmation. Symlinks are
  skipped, and each file is finalized only after its temporary copy finishes.
- **Port forwarding:** saved Local, Remote, and Dynamic SOCKS5 TCP tunnels;
  SSH profile selection; bind and destination fields; automatic free ports;
  start/stop; live connection counts, traffic counters, and error status.
- **Proxies:** named HTTP CONNECT and SOCKS5 profiles with optional authentication;
  select a proxy in an SSH profile for terminal, AI, SFTP, and tunnels. Host-key
  verification is preserved. Proxy passwords are write-only and stored in the vault.
- **Lifecycle/performance:** file bytes stay in Rust, copy buffers are bounded,
  transfer progress is throttled, directory rows are virtualized, and new pages
  are lazy-loaded. Vault locking closes SFTP transports, cancels transfers, and
  stops tunnels. Tunnels never restart automatically after unlock or app restart.
- **SSH key management:** dedicated sidebar page, named key selection in profiles,
  Ed25519/RSA 2048/3072/4096/8192/ECDSA generation, encrypted private-key import,
  and RSA SHA-2 authentication. DSA and RSA/SHA-1 remain unsupported.
- **Host improvements:** create groups with + beside SSH hosts, drag profiles into
  groups, reveal saved passwords explicitly, preserve drafts while visiting key
  or proxy management, and close terminal tabs when their shell exits.

Upgrading migrates the encrypted database to schema 9. Saved profiles and vault
credentials are retained. Create an encrypted backup before a version downgrade;
older builds cannot open a schema newer than they support. Transfer state and
running listeners are session-only. Full interactive platform/provider coverage
is not established; automated SSH network tests use local OpenSSH fixtures.

## Download

| System | File | Notes |
| --- | --- | --- |
| Windows 10/11 (x64) | `Selai_<version>_x64-setup.exe` (or the `.msi`) | SmartScreen: **More info → Run anyway**. Needs WebView2, which Windows 11 includes. |
| Linux (x64), Debian/Ubuntu | `selai_<version>_amd64.deb` | `sudo apt install ./selai_<version>_amd64.deb` |
| Linux (x64), any distro | `Selai_<version>_amd64.AppImage` | `chmod +x` it, then run. Needs a Secret Service keyring (GNOME Keyring or KWallet). |
| macOS (Apple silicon) | `Selai_<version>_aarch64.dmg` | Unsigned: right-click the app → **Open** the first time. |

## Check the download

`SHA256SUMS` lists every file in this release:

```sh
sha256sum -c SHA256SUMS --ignore-missing        # Linux
shasum -a 256 -c SHA256SUMS --ignore-missing    # macOS
```

On Windows: `Get-FileHash -Algorithm SHA256 .\<file>` and compare with `SHA256SUMS`.

## What's in it

- Hosts with labels that may contain spaces; the server key is approved inside
  the first connect, and a changed key needs an explicit confirmation.
- A desktop workbench: host explorer on the left, terminals and History/Providers
  as tabs in the middle, the AI chat on the right (Ctrl+Shift+E / Ctrl+Shift+L).
- Terminal tabs with status, reconnect, and an "Edit host" step after a failed login.
- A local terminal on this computer ("This computer"), which the AI can use under
  the same approvals as a host.
- AI Agent chat: plain technical discussion or independent tasks on multiple
  selected hosts. Follow the focused terminal, pin a host, or mention one or
  more hosts with `@`. Fixed diagnostics run on their own; other commands need
  approval, subject to the optional root grant described below.
- Commands that need root: the approval card asks for the sudo password when sudo
  asks for one (typed, saved, or the SSH login password). The model never sees it.
- The first root approval can enable **Allow further root commands in this
  request on this host**. The in-memory grant ends with the task, cancellation,
  or vault lock; it does not cover another host or request. Commands remain
  recorded in History and the audit log.
- A dedicated **SSH Keys** page in the left sidebar: generate Ed25519,
  RSA 2048/3072/4096/8192-bit, or ECDSA P-256/P-384/P-521 key pairs,
  import private keys with an optional passphrase, view fingerprints, copy
  public keys, and select a reusable named key in each host profile. Private
  key material stays in the encrypted vault. RSA (2048–8192 bits) can be imported
  and used with SHA-2 signatures; DSA is unsupported. Profile forms use a key
  dropdown and preserve drafts when visiting key management.
- Create groups with **+** beside **SSH hosts**, then drag profiles onto a group
  or **Ungrouped**. Moves preserve credentials and connection settings; profile
  forms select existing groups without an inline group-creation flow.
- A passphrase vault: your data is encrypted at rest, with a recovery key,
  auto-lock, an encrypted backup, and encrypted `.selai-history` exports.
- Saved chat sessions, and tasks that span several hosts, each with its own approvals.
- Import SSH profiles from CSV, JSON, KeePass `.kdbx`, or OpenSSH config. Upload
  a config file or read `~/.ssh/config`, review a preview, then save selected
  profiles. Include/Match blocks and proxy connections are not imported.
  Credentials stay local.
- Chat replies render Markdown tables, code blocks and Mermaid diagrams.
- History with each request, the commands it ran, and input/output token usage.
- Providers: OpenAI, Anthropic and OpenAI-compatible servers (vLLM, Ollama,
  LiteLLM). "Check API key" loads the model list; one Test button checks the
  connection and tool calling. Plain HTTP to a gateway needs an explicit opt-in.

See `SECURITY.md` for what leaves your machine and the known limits.
`sbom-cyclonedx.tar.gz` holds the CycloneDX SBOMs.
