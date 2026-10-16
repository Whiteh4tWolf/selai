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

Milestone M0 contains no SSH transport, no credential storage and no AI
execution. Those areas are not yet implemented, so reports about them describe
planned behavior and are handled as design feedback.

## Known limits (v1 design)

- Provider API keys are kept in the OS credential store and sent only in the
  `Authorization` or `x-api-key` header, to the base URL you configured. Redirects
  are not followed. Remote endpoints require https. Plain http is accepted only for
  loopback addresses.
- Anything you send to a cloud model leaves this machine. That includes command
  output in later milestones.

- Local logs and the local audit chain are tamper-evident only against
  accidental change. A local administrator can rewrite them.
- The OS keychain protects secrets at rest. It does not protect against malware
  running as the logged-in user.
- Arbitrary shell commands are not classified as safe. Approval is the control.
- Output sent to a cloud model may contain sensitive remote data.
