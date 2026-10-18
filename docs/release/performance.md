# Performance measurements (S-043)

The benchmark table below was measured by
`apps/desktop/src-tauri/tests/perf_pty.rs`. Resource limits are source contracts,
not measurements of desktop memory or CPU.

## Resource controls (2026-10-18)

These changes have not yet been measured in a new executable or installer:

- Terminal panes follow light, dark, or system theme without reconnecting.
  The light palette adjusts ANSI colors and uses a minimum contrast ratio of
  4.5. Programs that explicitly paint RGB backgrounds can still choose their own
  colors. Changing the palette does not change shell commands or remote state.
- Production terminal IPC holds at most 256 KiB of unacknowledged raw output per
  session, in chunks up to 32 KiB. Credits return after xterm's write callback,
  including hidden panes. This bounds that output queue, not SSH transport,
  WebView, PTY, or scrollback memory. Closing wakes blocked producers. Admission
  allows at most 16 terminal sessions, including connections in progress.
- Forwarding permits at most 128 concurrent connections across all tunnels and
  modes, retaining the existing 64-connection limit per tunnel. Excess sockets
  are closed and counted as errors; connection buffers are not queued indefinitely.
- SFTP directory metadata is limited to 8 MiB / 20,000 entries, and transfer plan
  metadata to 32 MiB / 100,000 discovered entries. File contents remain streamed
  in 64 KiB buffers with two transfers at a time. A download checks local free
  space for its temporary copy plus 8 MiB headroom. This is a preflight rather
  than a disk reservation; later write errors are still handled.
- Chat autosave keeps one snapshot in flight per session and coalesces newer
  revisions. Lock flushing waits for the latest revision. The visible transcript
  renders up to 40 turns, with navigation to older turns. Saved history remains
  intact. Persistence still writes full JSON snapshots and retains its existing
  size limit; this is not incremental transcript storage.
- Vault status notifications carry monotonically increasing revisions so a late
  snapshot cannot reopen a locked view. The UI uses notifications and a 10-second
  recovery poll; if subscription fails, it falls back to the former 500 ms poll.
  Native auto-lock checks still run once per second.
- The existing output-preview retention policy runs on production vault
  activation, with a new sweep every six hours while unlocked. It does not delete chat or audit
  records. SQLite reuses freed pages; automatic retention does not shrink the file.
- Vault security settings show allocated/reusable database bytes and offer manual
  compaction while work is idle. Native admission excludes active operations,
  tasks, terminals, transfers, and tunnels. Interactive compaction is limited to
  vaults up to 128 MiB, with free-space checks. SQLCipher, secure deletion,
  durability, and memory-only temporary storage remain enabled. Encrypted backup
  creation also checks space for its temporary database and destination.

`scripts/ci/check-size-budget.mjs` gates frontend assets after CI builds and
Windows packages before release staging. Limits are 6.5 MiB raw frontend,
2 MiB gzip (a regression proxy, not the installer's compression), 1.5 MiB per
asset, 450 KiB startup JS, 40 MiB Windows executable, and 16 MiB per Windows
installer. Mermaid remains lazy-loaded with its supported diagram types. No
runtime dependency was added for these controls. Development `target/` and
`node_modules/` directories are separate from installed application disk usage.

Native contract tests cover credit exhaustion, acknowledgement and cancellation;
a real local PTY test exercises output larger than one credit window. Synthetic
vault tests verify idle compaction preserves readable encrypted data. A new
Windows/WebView workload benchmark and fresh package-size measurement remain
necessary before claiming a particular CPU, RAM, or installer-size reduction.

## Frontend review (2026-10-16)

These are source-level changes, **not new CPU or RAM measurements**:

- The host explorer and `@` picker filter an already loaded host list. Typing no
  longer requests every page from SQLite for each character.
- AI text deltas are collected for up to 32 ms or 32 KiB before a React update.
  Approval, command, failure, and completion events flush preceding text first.
  Completed turns are memoized, avoiding repeated Markdown parsing as a new
  reply streams or the operator types.
- Hidden terminal tabs stop cursor blinking. Terminal scrollback is limited to
  5,000 lines per tab. Panel dragging schedules at most one layout update per
  animation frame.

The measurements below predate these changes. A full desktop CPU/RAM comparison
still needs the native app and webview running on the same machine before and
after the change. At the time of these measurements, terminal output had no UI
acknowledgement or backpressure. The October 18 controls above address that queue;
the old measurements do not establish their performance.

## Machine

- Sandbox VM: 4 vCPU Intel Xeon @ 2.10 GHz, 16 GiB RAM, Linux 6.18 (kernel string
  `6.18.44-fc-v139`), rustc 1.97.0. Debug build of the test binary.
- OpenSSH server: the local `sshd` fixture from `selai-testkit` on the same VM.
  The client and server share the 4 vCPUs, so the numbers include CPU contention.

## Five terminals, four streaming

Scenario: five terminals open on one host. Four stream about 11 MB each
(`seq 1 1000000`, with a prefix per line). The fifth stays idle and is the probe.
The probe types a marker every 100 ms and times how long its echo takes to come back.
The echo uses the same path as output: SSH channel, service, event channel.

| Run | Stream bytes | Wall time | Throughput | Echo samples | Echo p50 | Echo p95 | Echo max | Peak RSS |
|---|---|---|---|---|---|---|---|---|
| 1 | 43.6 MB | 5.55 s | 7.49 MiB/s | 17 | 262 ms | 771 ms | 865 ms | 30.4 MB |
| 2 | 43.6 MB | 5.76 s | 7.21 MiB/s | 20 | 182 ms | 712 ms | 820 ms | 29.3 MB |
| 3 | 43.6 MB | 5.44 s | 7.63 MiB/s | 18 | 233 ms | 679 ms | 717 ms | 28.8 MB |

Reproduce:

```sh
cargo test -p selai-desktop --test perf_pty --locked -- --ignored --nocapture
```

## What these numbers do and do not show

- Typed-echo latency under load is 180 to 260 ms at the median and about
  0.7 s at p95 on this VM, with four other terminals saturating the channel.
  Output queued ahead of the echo on the same SSH channel is part of that time.
- The measuring process is also the consumer. It decodes base64 and searches
  text on one thread, so the numbers include harness overhead.
- Memory: resident set rose from about 19 MB at start to 28 to 30 MB after the
  streams (peak 28.8 to 30.4 MB across the table runs; the sample is taken after the streams
  end, not after the terminals close). Only one run per sample. No soak test has
  been done, so this says nothing about a long session.
- Not measured: `top` and `vim` redraw correctness (T-SSH-006 asks for them),
  input responsiveness while an AI stream is running, macOS and Windows, and
  memory on those platforms.
- Output backpressure was not implemented in the measured terminal event path.
  The numbers above are for the 7 to 8 MiB/s the harness can drain, not for a
  producer that outruns the UI.
