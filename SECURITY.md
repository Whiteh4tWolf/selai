# Security policy

Selai handles SSH credentials and AI-driven remote command execution. Please
report suspected vulnerabilities privately.

## Reporting

Use GitHub private vulnerability reporting on this repository
(**Security → Report a vulnerability**). Do not open a public issue for a
vulnerability.

Include: affected version or commit, reproduction steps, and the impact you
observed. Do not include real credentials or private keys in a report.

## Scope of current release work

SSH access, credential storage and the scoped agent (M1 to M3) are built and
tested on Linux. macOS and Windows are not yet verified for SSH. The agent can
run commands on a host only through the rules in ADR-0005: the host is fixed by
the task, diagnostics are fixed commands, and every free-form command needs an
approval that is checked again in the database before it runs. Reports about
behaviour that is documented as planned (for example encrypted audit blobs, S-040)
are handled as design feedback.

## Known limits (v1 design)

- Provider API keys are kept in the OS credential store and sent only in the
  `Authorization` or `x-api-key` header, to the base URL you configured. Redirects
  are not followed. Remote endpoints require https. Plain http is accepted for
  loopback addresses, and for another host only when the operator ticks "Allow plain
  HTTP" on that provider (for example a LiteLLM proxy on a private network). With that
  opt-in the API key and every prompt and tool result travel unencrypted on the
  network.
- Anything you send to a cloud model leaves this machine. That includes the
  agent's request, the text of its replies, and the output of each command it
  runs (bounded and redacted, but not guaranteed free of sensitive data). The SSH
  password, private key and provider key are never sent.
- Plain chat (a message without an `@host` mention) sends the conversation in that
  window to the provider: up to the last 40 messages, 200 kB at most. It offers the
  model no tools and touches no host. It is not stored on disk.
- For each host task, the request is stored as a preview of at most 500 characters,
  after the same pattern-based redaction as command output, so History can show
  what was asked. Token counts reported by the provider are stored with it.
- "Check API key" sends the key typed in the form (or the saved one) once, to list
  the endpoint's models. The typed key is not stored until you save.
- Root commands: the model asks with `as_root`; Selai adds `sudo` itself. The sudo
  password is given in the approval card (typed, saved for the host, or the host's
  SSH password), goes only to sudo's prompt on that host, and is redacted from the
  output. It is never in a model request, the database or a log. "Remember for this
  host" saves it in the OS keychain; archiving the host deletes it.
- "This computer" runs agent commands as your own user on your own machine, with
  the same approvals. An approved command can read anything your account can read,
  and its output goes to the model like output from a server.

- Local logs and the local audit chain are tamper-evident only against
  accidental change. A local administrator can rewrite them.
- The OS keychain protects secrets at rest. It does not protect against malware
  running as the logged-in user.
- Arbitrary shell commands are not classified as safe. Approval is the control.
- Output sent to a cloud model may contain sensitive remote data.
- Output redaction is pattern-based. It misses secrets under neutral key names
  and can redact harmless text. Exports say so. See
  `docs/release/security-regression.md` for the list.
- Installers are unsigned until signing secrets are configured. A release built
  without them says so in `docs/release/packaging.md`.
