//! "This computer": a local shell and agent tasks on the operator's machine,
//! with the same approvals and audit as an SSH host, and no host key.

#![cfg(unix)] // Uses sh; Windows runs PowerShell and is not covered here.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{sync::Arc, time::Duration};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use selai_agent::{COMMAND_TOOL, DIAGNOSTIC_TOOL};
use selai_desktop_lib::{dto::TerminalEvent, service::EventSink};

mod common;
use common::*;

const LIMIT: Duration = Duration::from_secs(15);

/// These tests exercise metadata, not SSH authentication; they must not create
/// the root-only Linux sshd fixture on macOS.
fn metadata_host(label: &str) -> HostInput {
    serde_json::from_value(json!({"label":label,"hostname":"127.0.0.1","port":22,
        "username":"fixture","authMethod":"password","connectTimeoutSeconds":15,
        "keepAliveSeconds":30,"terminalType":"xterm-256color"}))
    .expect("metadata-only profile")
}

/// A provider whose tool calling verified, without any SSH host.
async fn provider(h: &Harness, mock: &MockServer) -> String {
    let p = h
        .services
        .provider_save(provider_input(&format!("{}/v1", mock.uri())))
        .expect("save provider");
    let test = h
        .services
        .provider_test(&p.id, "tool_calling")
        .await
        .expect("test");
    assert!(test.verified, "{}", test.message);
    p.id
}

#[tokio::test]
async fn the_local_computer_is_listed_apart_and_cannot_be_edited_or_probed() {
    let h = harness();
    let local = h.services.local_host().expect("local");
    assert_eq!(local.kind, "local");
    assert_eq!(local.label, "This computer");
    assert!(h
        .services
        .host_list(None, false, None)
        .expect("list")
        .items
        .is_empty());

    let probe = h.services.host_probe(&local.id).await;
    assert_eq!(
        err_code(probe.expect_err("no host key")),
        ErrorCode::InvalidInput
    );
    let mut edit = metadata_host("renamed");
    edit.id = Some(local.id.clone());
    edit.expected_updated_at_ms = Some(local.updated_at_ms);
    assert_eq!(
        err_code(h.services.host_save(edit).expect_err("read only")),
        ErrorCode::InvalidInput
    );
    assert!(h.services.host_archive(&local.id).is_err());
}

#[tokio::test]
async fn a_local_terminal_runs_a_shell_and_reports_its_exit() {
    let h = harness();
    let local = h.services.local_host().expect("local");
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sink: EventSink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let session = h
        .services
        .terminal_connect(&local.id, 80, 24, sink)
        .await
        .expect("open");
    assert_eq!(
        h.services.terminal_host(&session).map(|id| id.to_string()),
        Some(local.id.clone())
    );
    h.services
        .terminal_input(&session, "echo selai-$((6*7)); exit 3\n".into())
        .await
        .expect("input");

    let mut text = String::new();
    let deadline = tokio::time::Instant::now() + LIMIT;
    let closed = loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(TerminalEvent::Data { data })) => {
                text.push_str(&String::from_utf8_lossy(&B64.decode(data).expect("b64")));
            }
            Ok(Some(TerminalEvent::Closed { exit_status })) => break exit_status,
            Ok(Some(_)) => {}
            _ => panic!("timed out; output so far: {text:?}"),
        }
    };
    assert!(text.contains("selai-42"), "{text:?}");
    assert_eq!(closed, Some(3));
    assert!(
        h.services.terminal_host(&session).is_none(),
        "the session is gone"
    );
}

#[tokio::test]
async fn the_agent_works_on_this_computer_with_the_same_approval_gate() {
    let h = harness();
    let marker = h.dir.path().join("local-marker");
    let command = format!("touch {}", marker.display());
    let mock = model(vec![
        Turn::Tool(DIAGNOSTIC_TOOL, json!({"operation": "uptime"})),
        Turn::Tool(COMMAND_TOOL, json!({"command": command})),
        Turn::Text("Done."),
    ])
    .await;
    let provider = provider(&h, &mock).await;
    let local = h.services.local_host().expect("local");

    let (on_event, mut rx) = sink();
    h.services
        .agent_start(
            &local.id,
            &provider,
            "check uptime and create the marker",
            &[],
            on_event,
        )
        .await
        .expect("start");
    let waiting = until(&mut rx, |e| {
        matches!(e, TaskEventView::ApprovalRequired { .. })
    })
    .await;
    let approval = match waiting.last() {
        Some(TaskEventView::ApprovalRequired {
            approval_id, sudo, ..
        }) => {
            assert_eq!(*sudo, None, "a plain command does not use sudo");
            approval_id.clone()
        }
        other => panic!("expected an approval, got {other:?}"),
    };
    assert!(!marker.exists(), "nothing runs before approval");
    h.services
        .approval_decide(&approval, true, None)
        .expect("approve");
    assert_eq!(
        terminal_state(&until(&mut rx, is_terminal).await),
        "completed"
    );
    assert!(marker.exists());

    let rows = h
        .services
        .history_list(&local.id, None)
        .expect("history")
        .items;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.state == "completed"));
    assert!(h.services.audit_verify().expect("verify").ok);
}

#[tokio::test]
async fn a_root_command_on_this_computer_goes_through_local_sudo() {
    let h = harness();
    let mock = model(vec![
        Turn::Tool(COMMAND_TOOL, json!({"command": "id -u", "as_root": true})),
        Turn::Text("Done."),
    ])
    .await;
    let provider = provider(&h, &mock).await;
    let local = h.services.local_host().expect("local");

    let (on_event, mut rx) = sink();
    h.services
        .agent_start(&local.id, &provider, "what is root's uid?", &[], on_event)
        .await
        .expect("start");
    let waiting = until(&mut rx, |e| {
        matches!(e, TaskEventView::ApprovalRequired { .. })
    })
    .await;
    let (approval, sudo) = match waiting.last() {
        Some(TaskEventView::ApprovalRequired {
            approval_id,
            sudo,
            command,
            ..
        }) => {
            assert_eq!(command, "sudo id -u");
            (approval_id.clone(), *sudo)
        }
        other => panic!("expected an approval, got {other:?}"),
    };
    // The test runs as root, so this machine's sudo needs no password.
    assert_eq!(sudo, Some("no_password"));
    h.services
        .approval_decide(&approval, true, None)
        .expect("approve");
    until(&mut rx, is_terminal).await;
    let row = &h
        .services
        .history_list(&local.id, None)
        .expect("history")
        .items[0];
    assert_eq!(row.state, "completed");
    assert_eq!(row.stdout_preview.as_deref().map(str::trim), Some("0"));
}

#[tokio::test]
async fn a_pinned_server_is_remembered_and_archived_ones_cannot_be_pinned() {
    use selai_desktop_lib::history_service::AiPreferencesInput;
    let h = harness();
    let local = h.services.local_host().expect("local");
    let pin = |id: Option<&str>| AiPreferencesInput {
        last_provider_id: None,
        output_retention_days: None,
        pinned_host_id: id.map(str::to_owned),
    };

    assert_eq!(
        h.services.preferences_get().expect("prefs").pinned_host_id,
        None
    );
    let set = h
        .services
        .preferences_set(pin(Some(&local.id)))
        .expect("pin");
    assert_eq!(set.pinned_host_id.as_deref(), Some(local.id.as_str()));
    assert_eq!(
        h.services
            .preferences_get()
            .expect("prefs")
            .pinned_host_id
            .as_deref(),
        Some(local.id.as_str()),
        "the pin survives a reload of preferences"
    );
    assert_eq!(
        h.services
            .preferences_set(pin(Some("")))
            .expect("unpin")
            .pinned_host_id,
        None
    );

    let unknown = h
        .services
        .preferences_set(pin(Some("0a0a0a0a-0000-4000-8000-00000000000a")));
    assert_eq!(err_code(unknown.expect_err("unknown")), ErrorCode::NotFound);

    let ssh = h
        .services
        .host_save(metadata_host("archived-box"))
        .expect("save");
    h.services.host_archive(&ssh.id).expect("archive");
    let refused = h.services.preferences_set(pin(Some(&ssh.id)));
    assert_eq!(
        err_code(refused.expect_err("archived")),
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn a_host_task_gets_the_earlier_messages_of_its_chat_as_context() {
    use selai_desktop_lib::chat_service::ChatMessageInput;
    let h = harness();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let captured = Arc::clone(&seen);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |req: &Request| {
            let raw = String::from_utf8_lossy(&req.body).into_owned();
            let body: Value = serde_json::from_str(&raw).expect("json");
            if raw.contains("selai_echo") {
                let has_result = body["messages"]
                    .as_array()
                    .is_some_and(|m| m.iter().any(|x| x["role"] == "tool"));
                let payload = if has_result {
                    sse(&[json!({"choices":[{"delta":{"content":"done"}}]}), json!({"choices":[{"delta":{},"finish_reason":"stop"}]})])
                } else {
                    sse(&[
                        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"selai_echo","arguments":"{\"text\":\"selai-probe-7f3a\"}"}}]}}]}),
                        json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}),
                    ])
                };
                return ResponseTemplate::new(200).set_body_raw(payload, "text/event-stream");
            }
            captured.lock().expect("lock").push(body);
            ResponseTemplate::new(200).set_body_raw(
                sse(&[json!({"choices":[{"delta":{"content":"It is 4 GB."}}]}), json!({"choices":[{"delta":{},"finish_reason":"stop"}]})]),
                "text/event-stream",
            )
        })
        .mount(&server)
        .await;
    let provider = provider(&h, &server).await;
    let local = h.services.local_host().expect("local");

    let msg = |role: &str, content: &str| ChatMessageInput {
        role: role.into(),
        content: content.into(),
    };
    let (on_event, mut rx) = sink();
    h.services
        .agent_start(
            &local.id,
            &provider,
            "and the free space?",
            &[
                msg("user", "how much memory is there?"),
                msg("assistant", "Let me check."),
            ],
            on_event,
        )
        .await
        .expect("start");
    assert_eq!(
        terminal_state(&until(&mut rx, is_terminal).await),
        "completed"
    );

    let bodies = seen.lock().expect("lock").clone();
    let messages = bodies[0]["messages"].as_array().expect("messages").clone();
    let roles: Vec<&str> = messages
        .iter()
        .map(|m| m["role"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(roles, vec!["system", "user", "assistant", "user"]);
    assert_eq!(messages[1]["content"], "how much memory is there?");
    assert_eq!(messages[3]["content"], "and the free space?");
}
