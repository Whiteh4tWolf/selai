Selai is a desktop SSH client with a scoped AI agent: save hosts, open terminals,
chat with a model, and mention a host with `@` to let the agent check it. The
model never receives SSH credentials, and every command it runs is recorded.

**This is a preview release.** It is tested end to end on Linux. The Windows
and macOS installers are built by CI but have not been run by the maintainers.
The installers are not code-signed, so your OS will warn before the first start.

**New in 0.1.1:** a passphrase vault with a recovery key, saved chat sessions,
tasks across several hosts, SSH profile import (CSV, JSON, KeePass `.kdbx`),
Markdown and Mermaid in chat, and a refreshed desktop layout. 0.1.1 also fixes
a Windows crash when creating the vault passphrase (stack overflow). That fix
is covered by the Windows CI tests, but nobody has yet confirmed it on a real
Windows desktop. If setup still closes the app, please open an issue.

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
- AI Agent chat: plain technical discussion. The chat works on the server of the
  terminal in focus, and you can pin one; `@host` sends a single message elsewhere.
  Read-only checks run on their own; other commands wait for your approval.
- Commands that need root: the approval card asks for the sudo password when sudo
  asks for one (typed, saved, or the SSH login password). The model never sees it.
- A passphrase vault: your data is encrypted at rest, with a recovery key,
  auto-lock, an encrypted backup, and encrypted `.selai-history` exports.
- Saved chat sessions, and tasks that span several hosts, each with its own approvals.
- Import SSH profiles from CSV, JSON or a KeePass `.kdbx` file; credentials stay local.
- Chat replies render Markdown tables, code blocks and Mermaid diagrams.
- History with each request, the commands it ran, and input/output token usage.
- Providers: OpenAI, Anthropic and OpenAI-compatible servers (vLLM, Ollama,
  LiteLLM). "Check API key" loads the model list; one Test button checks the
  connection and tool calling. Plain HTTP to a gateway needs an explicit opt-in.

See `SECURITY.md` for what leaves your machine and the known limits.
`sbom-cyclonedx.tar.gz` holds the CycloneDX SBOMs.
