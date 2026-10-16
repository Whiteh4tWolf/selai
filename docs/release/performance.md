# Performance measurements (S-043)

Numbers in this file were measured by `apps/desktop/src-tauri/tests/perf_pty.rs`.
Nothing here is estimated or copied from another machine.

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
after the change. The terminal event path still has no UI acknowledgement or
backpressure; a producer that outruns the webview can grow queued output in
memory. This remains the main unbounded-RAM risk under sustained output.

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
- Output backpressure is still not implemented on the terminal event channel
  (see milestones M1 gaps). Under sustained flooding the channel is unbounded.
  The numbers above are for the 7 to 8 MiB/s the harness can drain, not for a
  producer that outruns the UI.
