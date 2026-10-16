# Vault validation — 2026-10-16

All data used for this validation was synthetic. The user's real Selai database,
saved credentials, and history were not migrated or modified.

## Verified

| Check | Result |
| --- | --- |
| Frontend Vitest | 94 passed across 15 files |
| TypeScript, ESLint, Vite production bundle | Passed; existing large Mermaid chunk warning remains |
| Desktop Rust unit tests (Debian/WSL) | 20 passed |
| Storage unit tests | 24 passed |
| Agent runtime with mock provider/executor | 28 passed |
| Vault integration tests | 4 passed |
| Rust Clippy for changed packages, libraries and tests (`-D warnings`) | Passed on Linux |
| Windows platform adapter compilation and Clippy | Passed |
| Windows API compatibility probe | Current-session unlock query and temporary power-notification registration succeeded |

Coverage includes ciphertext/WAL inspection, wrong keys and passphrases,
authenticated history tampering, credential migration, retained metadata and
exports, migration failure preserving sources, pending credential-cleanup
tombstones, changed source files not being deleted, managed private keys,
backup/restore with passphrase or recovery key, recovery rotation, passphrase
change, idle admission, cancellation of network operations, closing a live local
terminal, final transcript persistence, and inaccessible DB references after lock.
The graceful drain test proves the current command's completed result is retained
and the following command never starts.

Reproduce on a machine with the documented native build prerequisites:

```sh
cd apps/desktop
pnpm typecheck
pnpm lint
pnpm test --maxWorkers=2
pnpm build
cd ../..
cargo fmt --all -- --check
cargo test -p selai-desktop -p selai-agent -p selai-storage -p selai-vault --lib --test runtime --test vault -- --test-threads=1
cargo clippy -p selai-storage -p selai-vault -p selai-agent -p selai-desktop -p selai-platform --lib --tests -- -D warnings
```

On a constrained test runner, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, and `CARGO_INCREMENTAL=0` reduce build-cache space.
These are test-build settings, not application runtime settings.

## Remaining manual/platform validation

- The Windows installer and complete Windows UI unlock/lock/sleep flow have not
  been exercised with this build. The platform adapter was compiled on Windows;
  the full Rust backend was built and tested on Linux.
- Native file dialogs and the visual layout need a manual desktop check.
  Browser automation preview was unavailable in this environment.
- The full SSH/sudo/keyring integration suite was not completed. An attempted
  `local_e2e` run passed three tests, including a real local PTY, and failed three
  environment-dependent cases requiring root/sshd or passwordless sudo. Use
  the root/keyring harness in CONTRIBUTING.md and CI for the complete suite.
- Linux/macOS OS session-lock and suspend integration remains unimplemented;
  manual and idle vault locking are supported.
- No independent cryptographic audit or runtime CPU/RAM benchmark is claimed.
