//! Measurement harness for five active terminals (S-043). Ignored by default: it
//! takes seconds and prints numbers, it asserts only that every stream finished.
//!
//! Run on Linux as root with the sshd fixture:
//!   cargo test -p selai-desktop --test perf_pty -- --ignored --nocapture
//!
//! The numbers belong in docs/release/performance.md with the machine they came
//! from. Do not copy numbers from another machine.

#![cfg(target_os = "linux")] // Needs OpenSSH sshd and root; see docs/release/milestones.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use selai_desktop_lib::{
    dto::{HostInput, TerminalEvent, TrustMode},
    service::{EventSink, Services},
};
use selai_secrets::MemoryStore;
use selai_testkit::{material, Sshd, USER};
use tokio::sync::mpsc;

const SESSIONS: usize = 5;
const LINES_PER_SESSION: usize = 1_000_000;
const LIMIT: Duration = Duration::from_secs(120);

fn password_host(label: &str, port: u16) -> HostInput {
    HostInput {
        id: None,
        expected_updated_at_ms: None,
        label: label.to_owned(),
        group_id: None,
        hostname: "127.0.0.1".to_owned(),
        port: u32::from(port),
        username: USER.to_owned(),
        auth_method: "password".to_owned(),
        private_key_path: None,
        connect_timeout_seconds: 10,
        keep_alive_seconds: 30,
        terminal_type: "xterm-256color".to_owned(),
        notes: None,
        password: Some(material().password.clone()),
        key_passphrase: None,
    }
}

/// Resident set size in KiB from /proc, or None where /proc is absent.
fn rss_kib(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find(|l| l.starts_with(field))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[tokio::test]
#[ignore = "measurement harness; run with --ignored --nocapture"]
async fn five_terminals_streaming_keep_input_responsive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let services = Services::open(
        &dir.path().join("selai.sqlite3"),
        Arc::new(MemoryStore::new()),
    )
    .expect("open");
    let sshd = Sshd::start();
    let view = services
        .host_save(password_host("perf-host", sshd.port))
        .expect("save");
    let probe = services.host_probe(&view.id).await.expect("probe");
    services
        .host_trust(
            &view.id,
            &probe.observed.fingerprint_sha256,
            TrustMode::First,
        )
        .expect("trust");

    // Every session reports to one channel, tagged with its index.
    let (tx, mut rx) = mpsc::unbounded_channel::<(usize, TerminalEvent)>();
    let mut sessions = Vec::new();
    for i in 0..SESSIONS {
        let tx = tx.clone();
        let sink: EventSink = Arc::new(move |event| {
            let _ = tx.send((i, event));
        });
        let id = services
            .terminal_connect(&view.id, 120, 40, sink)
            .await
            .expect("connect");
        sessions.push(id);
    }
    drop(tx);

    let rss_before = rss_kib("VmRSS:");
    let mut bytes = [0usize; SESSIONS];
    let mut done = [false; SESSIONS];
    let mut pings: Vec<Duration> = Vec::new();
    let mut ping_sent_at: Option<Instant> = None;
    let mut ping_count = 0usize;
    // Last ~256 bytes of each session's output, to search for the markers.
    let mut tails: Vec<String> = vec![String::new(); SESSIONS];

    let started = Instant::now();
    for (i, id) in sessions.iter().enumerate() {
        // printf builds the marker, so the echoed input never contains the full
        // marker and only the program output can match it.
        let cmd =
            format!("seq 1 {LINES_PER_SESSION} | sed 's/^/s{i}-/'; printf 'PERF-%s\\n' DONE-{i}\n");
        services.terminal_input(id, cmd).await.expect("input");
    }

    let deadline = tokio::time::Instant::now() + LIMIT;
    // While session 0 streams, type a marker every 250 ms and time its echo. The
    // echo travels the same path as output: SSH channel, IPC, and back.
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    while !done.iter().all(|d| *d) {
        tokio::select! {
            _ = tick.tick(), if ping_sent_at.is_none() && !done[0] => {
                ping_count += 1;
                ping_sent_at = Some(Instant::now());
                services
                    // The line discipline echoes typed text at once. Ctrl-U (0x15) then erases the
                    // line, so nothing runs.
                    .terminal_input(&sessions[0], format!("PING-{ping_count:03}\u{15}"))
                    .await
                    .expect("ping");
            }
            msg = rx.recv() => {
                let Some((i, event)) = msg else { break };
                let TerminalEvent::Data { data } = event else { continue };
                let chunk = B64.decode(data).expect("base64");
                bytes[i] += chunk.len();
                tails[i].push_str(&String::from_utf8_lossy(&chunk));
                if tails[i].len() > 512 {
                    let mut cut = tails[i].len() - 256;
                    while !tails[i].is_char_boundary(cut) {
                        cut += 1;
                    }
                    tails[i] = tails[i][cut..].to_owned();
                }
                if tails[i].contains(&format!("PERF-DONE-{i}")) {
                    done[i] = true;
                }
                if i == 0 {
                    if let Some(sent) = ping_sent_at {
                        if tails[0].contains(&format!("PING-{ping_count:03}")) {
                            pings.push(sent.elapsed());
                            ping_sent_at = None;
                        }
                    }
                }
            }
            () = tokio::time::sleep_until(deadline) => {
                panic!("timed out; finished sessions: {done:?}");
            }
        }
    }
    let elapsed = started.elapsed();
    let total: usize = bytes.iter().sum();
    let rss_after = rss_kib("VmRSS:");
    let peak = rss_kib("VmHWM:");

    pings.sort();
    let pct = |p: f64| -> Option<f64> {
        if pings.is_empty() {
            return None;
        }
        let idx = ((pings.len() as f64 - 1.0) * p).round() as usize;
        Some(pings[idx].as_secs_f64() * 1000.0)
    };

    println!("sessions={SESSIONS} lines_per_session={LINES_PER_SESSION}");
    println!(
        "total_bytes={total} wall_seconds={:.2}",
        elapsed.as_secs_f64()
    );
    println!(
        "throughput_mib_per_s={:.2}",
        total as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0)
    );
    println!("per_session_bytes={bytes:?}");
    println!(
        "echo_samples={} p50_ms={:?} p95_ms={:?} max_ms={:?}",
        pings.len(),
        pct(0.5),
        pct(0.95),
        pct(1.0)
    );
    println!("rss_before_kib={rss_before:?} rss_after_kib={rss_after:?} peak_kib={peak:?}");

    for id in &sessions {
        let _ = services.terminal_disconnect(id).await;
    }
    assert!(done.iter().all(|d| *d), "every stream must finish");
}
