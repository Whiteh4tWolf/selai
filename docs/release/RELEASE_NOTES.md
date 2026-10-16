Selai is a desktop SSH client with a scoped AI agent: save hosts, open terminals,
chat with a model, and mention a host with `@` to let the agent check it. The
model never receives SSH credentials, and every command it runs is recorded.

**This is a preview release.** It is tested end to end on Linux. The Windows
and macOS installers are built by CI but have not been run by the maintainers.
The installers are not code-signed, so your OS will warn before the first start.

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
- Terminal tabs with status, reconnect, and an "Edit host" step after a failed login.
- AI Agent chat: plain technical discussion, or `@host` to run read-only checks
  (other commands wait for your approval).
- History with each request, the commands it ran, and input/output token usage.
- Providers: OpenAI, Anthropic and OpenAI-compatible servers (vLLM, Ollama,
  LiteLLM). "Check API key" loads the model list; one Test button checks the
  connection and tool calling. Plain HTTP to a gateway needs an explicit opt-in.

See `SECURITY.md` for what leaves your machine and the known limits.
`sbom-cyclonedx.tar.gz` holds the CycloneDX SBOMs.
