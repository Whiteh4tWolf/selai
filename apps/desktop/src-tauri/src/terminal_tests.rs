//! Real local PTY through the same credit path as desktop IPC. Synthetic output only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::{
    dto::TerminalEvent,
    service::{EventSink, Services},
    terminal_flow::TerminalFlow,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use selai_secrets::MemoryStore;
use std::{sync::Arc, time::Duration};

#[tokio::test]
async fn local_pty_preserves_output_across_credit_windows_and_releases_session() {
    let services = Services::in_memory(Arc::new(MemoryStore::new())).unwrap();
    let host = services.local_host().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let sink: EventSink = Arc::new(move |event| {
        let _ = tx.send(event);
    });
    let id = services
        .terminal_connect_with_flow(&host.id, 100, 24, sink, Arc::new(TerminalFlow::default()))
        .await
        .unwrap();
    #[cfg(windows)]
    let command = "Write-Output ('x' * 400000); Write-Output ('selai-' + (6*7)); exit 3\r";
    #[cfg(not(windows))]
    let command = "head -c 400000 /dev/zero | tr '\\000' x; echo selai-$((6*7)); exit 3\n";
    #[cfg(not(windows))]
    services.terminal_input(&id, command.into()).await.unwrap();
    #[cfg(windows)]
    let mut command_sent = false;
    #[cfg(windows)]
    let mut cursor_queries = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut text = String::new();
    let closed = loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(TerminalEvent::Data { data })) => {
                let bytes = B64.decode(data).unwrap();
                text.push_str(&String::from_utf8_lossy(&bytes));
                services.terminal_ack(&id, bytes.len()).unwrap();
                #[cfg(windows)]
                {
                    // PowerShell/PSReadLine waits for a cursor-position response
                    // before its first prompt. xterm normally sends this onData.
                    let queries = text.matches("\x1b[6n").count();
                    for _ in cursor_queries..queries {
                        services
                            .terminal_input(&id, "\x1b[1;1R".into())
                            .await
                            .unwrap();
                    }
                    cursor_queries = queries;
                    if !command_sent && text.contains("PS ") && text.contains('>') {
                        command_sent = true;
                        services.terminal_input(&id, command.into()).await.unwrap();
                    }
                }
            }
            Ok(Some(TerminalEvent::Closed { exit_status })) => break exit_status,
            Ok(Some(_)) => {}
            _ => {
                let _ = services.terminal_disconnect(&id).await;
                panic!(
                    "terminal timed out after {} output bytes: {:?}",
                    text.len(),
                    text.chars().take(500).collect::<String>()
                );
            }
        }
    };
    assert!(text.bytes().filter(|byte| *byte == b'x').count() >= 400000);
    assert!(text.contains("selai-42"));
    assert_eq!(closed, Some(3));
    assert!(services.terminal_host(&id).is_none());
    // Late callbacks from xterm after native cleanup are harmless.
    services.terminal_ack(&id, 1).unwrap();
}
