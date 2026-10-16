//! Shell-reading helpers for the SSH integration tests.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::{Duration, Instant};

use selai_ssh::{Shell, ShellEvent};

/// Reads shell output until `needle` appears. Panics with what was seen on
/// timeout or on an unexpected event.
pub async fn read_until(shell: &mut Shell, needle: &str, limit: Duration) -> String {
    let mut seen: Vec<u8> = Vec::new();
    let deadline = Instant::now() + limit;
    loop {
        if seen.windows(needle.len()).any(|w| w == needle.as_bytes()) {
            return String::from_utf8_lossy(&seen).into_owned();
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(
            !left.is_zero(),
            "timed out waiting for {needle:?}; output: {:?}",
            String::from_utf8_lossy(&seen)
        );
        match tokio::time::timeout(left, shell.next_event()).await {
            Ok(ShellEvent::Data(bytes)) => seen.extend(bytes),
            Ok(other) => panic!(
                "unexpected event {other:?}; output so far: {:?}",
                String::from_utf8_lossy(&seen)
            ),
            Err(_) => panic!(
                "timed out waiting for {needle:?}; output: {:?}",
                String::from_utf8_lossy(&seen)
            ),
        }
    }
}

/// Waits for the shell to exit and returns its status.
pub async fn wait_for_exit(shell: &mut Shell, limit: Duration) -> u32 {
    let deadline = Instant::now() + limit;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "timed out waiting for shell exit");
        match tokio::time::timeout(left, shell.next_event()).await {
            Ok(ShellEvent::Exit { status }) => return status,
            Ok(ShellEvent::Data(_)) => continue,
            Ok(other) => panic!("expected exit, got {other:?}"),
            Err(_) => panic!("timed out waiting for shell exit"),
        }
    }
}
