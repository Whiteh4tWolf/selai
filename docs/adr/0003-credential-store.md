# ADR-0003: Credential store and secret lifecycle

- **Status:** Superseded for production storage by [ADR-0006](0006-passphrase-vault.md). The OS store is now used only to migrate legacy credentials.
- **Scope:** S-011. Also used by S-020 for provider API keys.

## Context

SSH passwords, key passphrases and provider API keys must live only in the OS
credential store. SQLite holds a boolean "configured" flag, never the value.
The spec forbids a plaintext fallback. Linux, macOS and Windows each provide a
different store.

## Decision

1. **Library:** `keyring` 4.2.x with the default `v1` feature. It selects
   Keychain Services on macOS, Credential Manager on Windows and the
   Secret Service (D-Bus, zbus backend) on Linux. We use only `Entry::new`,
   `set_secret`, `get_secret` and `delete_credential`.
2. **Naming.** Service `selai`, account `ssh/{host_uuid}/{kind}` for SSH secrets
   (`kind` is `password` or `key_passphrase`), and `provider/{provider_uuid}/api_key`
   for provider keys. The account string is built only from validated UUIDs
   and fixed kinds, never from user text.
3. **Values are opaque bytes** wrapped in `selai_secrets::SecretValue`. The type
   zeroizes on drop, does not implement `Debug`, `Display`, `Clone` or
   `Serialize`, and exposes bytes only through `expose_for_transport()`. Only
   the SSH and HTTP clients call it.
4. **No read IPC.** No Tauri command returns a stored secret. `host_get` returns
   `credentialConfigured: bool`.
5. **Write ordering.** A save with a new password (1) writes the secret to the
   store, then (2) commits the host row with `credential_configured = 1`. If step
   1 fails, nothing changes. If step 2 fails, the store may hold a newer value
   than the row says; the flag is still correct, and the next save overwrites it.
   Each store entry is replaced atomically by the platform, so no partial value
   is possible.
6. **Archive** deletes the store entries and sets the flag to 0. Audit and history
   rows are kept (they never held secrets).
7. **Fail closed.** If the store is locked, unavailable or refuses access, the
   save or connect returns `SECRET_STORE_LOCKED` or `SECRET_STORE_UNAVAILABLE`
   with a user action ("unlock your keychain"). No plaintext file is written.
8. **Tests.** An in-memory store implements the same trait for unit tests in
   other crates. The real backend is tested per OS: on Linux in CI under a
   private D-Bus session with a freshly created login keyring, and on macOS and
   Windows in their CI jobs when those jobs run.

## Limitations (documented, not hidden)

- A logged-in attacker with the user's session can read keychain entries. The
  store protects against routine exposure, not against malware in the session.
- On Linux the Secret Service is available only with a running keyring daemon.
  Headless servers without one get `SECRET_STORE_UNAVAILABLE`.
- The `keyring` crate returns passwords as `Vec<u8>` or `String`. The bytes are
  copied out of the platform buffer; we zeroize our copy, not the crate's
  intermediate buffers.

## Fallback

If `keyring` cannot be used on a platform, the fallback is a direct call to the
platform API (`security-framework`, `windows` crate, `secret-service`). The
trait stays the same. Any plaintext option requires a new ADR and is not
allowed under the current release gate.

## Impact on other agents

- Agent C (providers) depends on the trait only. It must never build a
  `reqwest` request that includes a secret outside the adapter.
- Agent A (SSH) receives secrets as `SecretValue` and passes them directly to
  the transport.
