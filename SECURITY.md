# Security policy

Selai handles SSH credentials and AI-driven remote command execution. Please
report suspected vulnerabilities privately.

## Reporting

Use GitHub private vulnerability reporting on this repository
(**Security → Report a vulnerability**). Do not open a public issue for a
vulnerability.

Include: affected version or commit, reproduction steps, and the impact you
observed. Do not include real credentials or private keys in a report.

## SSH private keys

RSA keys of 2048-8192 bits use an OpenSSL signer with SHA-256 or SHA-512.
RSA generation (2048/3072/4096/8192 bits) also uses OpenSSL, with temporary
private PEM buffers wiped on drop. Generation runs in a background worker,
limited to one job at a time, and holds its vault lease until it finishes.
The RustCrypto `rsa` dependency is used for decoding and public verification,
not RSA private signing or generation, avoiding the private operations covered
by RUSTSEC-2023-0071. Import and login share RSA component and size validation.
Legacy `ssh-rsa`/SHA-1 and DSA user authentication are refused. Ed25519 and ECDSA
use russh's existing signing path. Host-key trust is still required before any
user authentication.

## Local vault

The desktop starts locked. Argon2id (64 MiB, three iterations, one lane) derives
a passphrase key; AES-256-GCM wraps a random data key. A separate random recovery
key wraps the same data key. Bundled SQLCipher encrypts the entire database and
WAL: hosts, notes, credentials, managed private keys, saved chat, command history,
audit, settings, and retained legacy exports. SQLite temporary data stays in
memory. The backend saves no passphrase, plaintext recovery key, or unwrapped
data key on disk or in the OS credential store.

The unencrypted `vault.json` contains format/KDF parameters, salt, a random
database filename, nonces, and wrapped keys. Display preferences alone remain
in localStorage. `.selai` backups and `.selai-history` exports are encrypted.
Explicitly downloaded recovery keys and decrypted JSON exports are plaintext.

The profile editor can explicitly reveal a saved SSH login password through a
dedicated command admitted only while the vault is unlocked. Ordinary host
metadata and lists still exclude secrets. The renderer fetches the password only
when the eye button is clicked, keeps it in component state, and clears the
preview after 30 seconds, on hide, loss of focus, vault-lock flushing, or editor teardown. It is not
copied into chat, localStorage, logs, or audit records. JS/runtime copies cannot
be guaranteed to be physically erased from memory. Private keys, key passphrases,
sudo passwords, and provider API keys have no equivalent reveal command.

Locking denies new operations, disconnects terminals, drains/cancels agent
work, closes the encrypted connection, and drops the vault key. The UI reports
**locking** until that finishes. Idle timeout defaults to 15 minutes of app
inactivity. Windows session lock and suspend notifications request locking;
a suspended process can finish releasing keys only when it runs again. Linux
and macOS support manual and idle locking; OS session/suspend integration is pending.

Migration validates an encrypted copy before committing and removing unchanged
legacy files or copied OS credentials. Cleanup failures remain visible and
retryable in Security. External private key originals and import files are
not deleted. Missing key files remain external references until replaced.
Files written by your own shell, shell history, approved local commands, and
remote servers are outside Selai's vault and retain their normal permissions.
Migration cannot erase earlier copies from SSD remnants, snapshots, backups,
quarantine, or another running legacy application.

Passphrase/recovery changes update the current vault only. Earlier backups and
exports retain their earlier credentials. Restore requires a fresh data folder.
Passphrase changes rewrap the same data key. Possession of an earlier header or
backup together with its credentials can expose that key and newer data copies;
changing the passphrase alone does not revoke a compromised data key.
Keep a backup and its recovery key separately. Losing both the passphrase and
recovery key makes the data unrecoverable.

## Scope of current release work

SSH access, credential storage and the scoped agent (M1 to M3) are built and
tested on Linux. macOS and Windows are not yet verified for SSH. The agent can
run commands on a host only through the rules in ADR-0005: the host is fixed by
the task and diagnostics are fixed commands. By default, free-form commands
need an approval checked again in the database before execution. The first
root approval can optionally grant further root commands for that task and host;
this in-memory grant is described below. Reports about
behaviour that is documented as planned (for example encrypted audit blobs, S-040)
are handled as design feedback.

## Known limits (v1 design)

- Provider API keys are kept in the encrypted vault and sent only in the
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
  model no tools and touches no host. Named sessions are saved in the encrypted
  vault. Their transcripts are not guaranteed free of secrets typed by a user
  or received from a model.
- For each host task, the request is stored as a preview of at most 500 characters,
  after the same pattern-based redaction as command output, so History can show
  what was asked. Token counts reported by the provider are stored with it.
- "Check API key" sends the key typed in the form (or the saved one) once, to list
  the endpoint's models. The typed key is not stored until you save.
- Root commands: the model asks with `as_root`; Selai adds `sudo` itself. The sudo
  password is given in the approval card (typed, saved for the host, or the host's
  SSH password), goes only to sudo's prompt on that host, and is redacted from the
  output. It is never in a model request or plaintext log. "Remember for this
  host" saves it inside the encrypted vault; archiving the host deletes it.
- **Allow further root commands in this request on this host** approves later
  root commands only within that running task on its fixed host. The initial
  command still needs its exact approval. The grant and any sudo password held
  for it stay in memory and are cleared when the task ends or the vault locks.
  Non-root free-form commands, other requests, and other hosts need separate
  approval. Grant creation and use are audited; each command remains in History.
- "This computer" runs agent commands as your own user on your own machine, with
  the same approvals. An approved command can read anything your account can read,
  and its output goes to the model like output from a server.

- Local logs and the local audit chain are tamper-evident only against
  accidental change. A local administrator can rewrite them.
- Encryption protects stored vault files while locked. Malware, debuggers, or
  administrators can still observe an unlocked process, keystrokes, screenshots,
  swap/pagefiles, hibernation files, and crash dumps. OS disk encryption protects
  those system files. Rust key buffers are zeroized, but complete erasure of all
  library/webview/runtime copies is not guaranteed. This implementation has not
  undergone an independent cryptographic audit.
- Arbitrary shell commands are not classified as safe. Approval is the control.
- Output sent to a cloud model may contain sensitive remote data.
- Output redaction is pattern-based. It misses secrets under neutral key names
  and can redact harmless text. Exports say so. See
  `docs/release/security-regression.md` for the list.
- Installers are unsigned until signing secrets are configured. A release built
  without them says so in `docs/release/packaging.md`.
