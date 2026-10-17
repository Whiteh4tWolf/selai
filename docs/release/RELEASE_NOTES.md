Selai is a desktop SSH client with a scoped AI agent: save hosts, open terminals,
chat with a model, and mention a host with `@` to let the agent check it. The
model never receives SSH credentials, and every command it runs is recorded.

**This is a preview release.** Automated tests cover Linux SSH/agent flows with
fixtures and mock providers. Windows packaging also checks encryption and the
vault in the release build profile. This is not complete interactive coverage
of every platform and live AI provider. The installers are not code-signed.

**New in 0.1.1:** a passphrase vault with a recovery key, saved chat sessions,
tasks across several hosts, SSH profile import (CSV, JSON, KeePass `.kdbx`,
and OpenSSH config), a reusable SSH key manager, optional root approval for
one request on one host, Markdown and Mermaid in chat, and a refreshed desktop
layout. Windows fixes cover the vault-setup stack overflow, installer artwork
overlapping default text, and startup background/theme handling to reduce
white WebView flashes around the passphrase screen.

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
- SSH key management: generate Ed25519 or ECDSA P-256/P-384/P-521 key pairs,
  import private keys with an optional passphrase, view fingerprints, copy
  public keys, and select a reusable named key in each host profile. Private
  key material stays in the encrypted vault. RSA and DSA are not offered.
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
