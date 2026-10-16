# Contributing to Selai

Read `AGENTS.md`, `SELAI_V1_ENGINEERING_SPEC.md` and the execution backlog first.
They define what is in scope for v1.0 and what is forbidden.

## Local checks

Run these before opening a pull request. CI runs the same commands.

```sh
# Rust (from the repository root)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check advisories licenses bans sources

# Frontend (from apps/desktop, Node 22 and pnpm 10)
pnpm install --frozen-lockfile
pnpm typecheck && pnpm lint && pnpm test && pnpm build
```

Linux needs the Tauri system libraries listed in `.github/workflows/ci.yml`.

## Rules that apply to every change

- Never weaken host-key verification, add a fallback that stores secrets in
  plaintext, or let the webview decide authorization.
- Never log or persist credentials, provider keys or raw request bodies. Tests
  that exercise secret handling must use a sentinel value and assert it does not
  appear in stored or logged output.
- A test that is skipped must be reported as skipped, never as passed.
- Schema changes go in a new numbered migration under
  `crates/selai-storage/migrations/`, with tests for a clean install and for an
  existing database.
- Dependencies need a license review and an advisory check. Commit `Cargo.lock`
  and `pnpm-lock.yaml`.

## Pull requests

Use the PR template. Describe what you ran and what you observed. Do not claim
a feature works unless a test or manual check demonstrates it.

## SSH and keyring tests (Linux)

The SSH suites (`selai-ssh/tests/ssh_integration.rs`, `selai-desktop`'s
`service_e2e.rs`) start a real OpenSSH server. They need root, `openssh-server`
and `gnome-keyring`, and they panic instead of skipping when those are missing.
Run them through the keyring harness, which also unlocks a private Secret
Service:

```sh
sudo -E env "PATH=$PATH" scripts/ci/with-keyring.sh cargo test --workspace
```

They are compiled only on Linux. macOS and Windows CI run the other suites and
the OS credential store test, but do not run SSH.

To drive the desktop app against a test server by hand, start the fixture and
keep it alive while you test:

```sh
sleep 900 | sudo -E target/debug/examples/serve   # build first: cargo build -p selai-testkit --example serve
```

## Live provider tests (not run in CI)

`crates/selai-llm/tests/live.rs` calls real provider APIs with keys you supply.
They are ignored by default. A passing run is the only evidence that a provider
works; nothing else counts as verification.

```sh
SELAI_LIVE_OPENAI_KEY=... SELAI_LIVE_OPENAI_MODEL=... \
SELAI_LIVE_ANTHROPIC_KEY=... SELAI_LIVE_ANTHROPIC_MODEL=... \
SELAI_LIVE_COMPAT_BASE_URL=http://127.0.0.1:8000/v1 SELAI_LIVE_COMPAT_MODEL=... \
  cargo test -p selai-llm --test live -- --ignored --nocapture
```

Record the provider, model and date in `docs/release/milestones.md`. Do not
mark a provider as supported from a run you did not perform.
