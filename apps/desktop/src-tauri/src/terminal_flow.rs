//! Byte credits held until xterm acknowledges parsing, including hidden panes.
use std::sync::{Condvar, Mutex};
use tokio::sync::Notify;

pub const OUTPUT_CHUNK: usize = 32 * 1024;
pub const OUTPUT_WINDOW: usize = 256 * 1024;

#[derive(Default)]
struct State {
    pending: usize,
    closed: bool,
}

#[derive(Default)]
pub struct TerminalFlow {
    state: Mutex<State>,
    changed: Notify,
    ready: Condvar,
}

impl TerminalFlow {
    pub async fn reserve(&self, bytes: usize) -> bool {
        if bytes > OUTPUT_WINDOW {
            return false;
        }
        loop {
            // Register before checking credits so an acknowledgement cannot be lost.
            let changed = self.changed.notified();
            {
                let Ok(mut state) = self.state.lock() else {
                    return false;
                };
                if state.closed {
                    return false;
                }
                if state.pending + bytes <= OUTPUT_WINDOW {
                    state.pending += bytes;
                    return true;
                }
            }
            changed.await;
        }
    }

    pub fn reserve_blocking(&self, bytes: usize) -> bool {
        if bytes > OUTPUT_WINDOW {
            return false;
        }
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        while !state.closed && state.pending + bytes > OUTPUT_WINDOW {
            state = match self.ready.wait(state) {
                Ok(state) => state,
                Err(_) => return false,
            };
        }
        if state.closed {
            return false;
        }
        state.pending += bytes;
        true
    }

    pub fn acknowledge(&self, bytes: usize) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if bytes > state.pending {
            return false;
        }
        state.pending -= bytes;
        drop(state);
        self.ready.notify_all();
        self.changed.notify_waiters();
        true
    }

    pub fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
        }
        self.ready.notify_all();
        self.changed.notify_waiters();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::Duration};

    #[tokio::test]
    async fn output_waits_for_consumption_and_close_unblocks_it() {
        let flow = Arc::new(TerminalFlow::default());
        assert!(flow.reserve(OUTPUT_WINDOW).await);
        assert!(!flow.acknowledge(OUTPUT_WINDOW + 1));
        assert!(
            tokio::time::timeout(Duration::from_millis(20), flow.reserve(1))
                .await
                .is_err()
        );
        assert!(flow.acknowledge(1));
        assert!(flow.reserve(1).await);
        let blocked = flow.clone();
        let waiter = tokio::spawn(async move { blocked.reserve(1).await });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        flow.close();
        assert!(!tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap());
    }

    #[test]
    fn pty_reader_blocks_at_the_same_budget_and_can_be_cancelled() {
        let flow = Arc::new(TerminalFlow::default());
        assert!(flow.reserve_blocking(OUTPUT_WINDOW));
        let (tx, rx) = std::sync::mpsc::channel();
        let blocked = flow.clone();
        let reader = std::thread::spawn(move || tx.send(blocked.reserve_blocking(1)).unwrap());
        assert!(rx.recv_timeout(Duration::from_millis(20)).is_err());
        flow.close();
        assert!(!rx.recv_timeout(Duration::from_secs(1)).unwrap());
        reader.join().unwrap();
    }
}
