//! Agent tasks through the application service: a real OpenSSH server for the
//! host, a mock provider for the model (tool calling verified first, as the app
//! requires). Covers the approval gate, cancellation, refusals, the rule that no
//! credential reaches the model, and history that survives a restart.

#![cfg(target_os = "linux")] // Needs OpenSSH sshd and root; see docs/release/milestones.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use selai_agent::{COMMAND_TOOL, DIAGNOSTIC_TOOL};
use selai_domain::ErrorCode;

mod common;
use common::*;

// ---- tests ------------------------------------------------------------------

#[tokio::test]
async fn disk_usage_runs_on_the_bound_host_and_is_recorded() {
    let h = harness();
    let sshd = Sshd::start();
    let mock = model(vec![
        Turn::Tool(DIAGNOSTIC_TOOL, json!({"operation": "disk_usage"})),
        Turn::Text("The root filesystem has free space."),
    ])
    .await;
    let (host, provider) = ready(&h, &sshd, &mock).await;

    let (on_event, mut rx) = sink();
    h.services
        .agent_start(&host, &provider, "check disk usage", &[], on_event)
        .await
        .expect("start");
    let seen = until(&mut rx, is_terminal).await;

    assert_eq!(terminal_state(&seen), "completed");
    assert!(seen.iter().any(|e| matches!(
        e,
        TaskEventView::ToolFinished { state, exit_code: Some(0), .. } if state == "completed"
    )));

    let history = h.services.history_list(&host, None).expect("history");
    assert_eq!(history.items.len(), 1);
    let row = &history.items[0];
    assert_eq!(row.tool, STORED_DIAGNOSTIC);
    assert_eq!(row.command_redacted, "df -hP");
    assert_eq!(row.state, "completed");

    let audit = h.services.audit_verify().expect("verify");
    assert!(audit.ok);
    assert!(audit.events_checked >= 2);
}

#[tokio::test]
async fn five_diagnostics_are_each_recorded_and_the_chain_verifies() {
    let h = harness();
    let sshd = Sshd::start();
    let mock = model(vec![
        Turn::Tool(DIAGNOSTIC_TOOL, json!({"operation": "uptime"})),
        Turn::Tool(DIAGNOSTIC_TOOL, json!({"operation": "disk_usage"})),
        Turn::Tool(DIAGNOSTIC_TOOL, json!({"operation": "memory_summary"})),
        Turn::Tool(DIAGNOSTIC_TOOL, json!({"operation": "os_info"})),
        Turn::Tool(DIAGNOSTIC_TOOL, json!({"operation": "uptime"})),
        Turn::Text("All five checks ran."),
    ])
    .await;
    let (host, provider) = ready(&h, &sshd, &mock).await;

    let (on_event, mut rx) = sink();
    h.services
        .agent_start(&host, &provider, "give me a health summary", &[], on_event)
        .await
        .expect("start");
    let seen = until(&mut rx, is_terminal).await;
    assert_eq!(terminal_state(&seen), "completed");

    let history = h.services.history_list(&host, None).expect("history");
    assert_eq!(history.items.len(), 5);
    assert!(history
        .items
        .iter()
        .all(|r| r.tool == STORED_DIAGNOSTIC && r.state == "completed"));
    assert!(h.services.audit_verify().expect("verify").ok);
}

#[tokio::test]
async fn approved_command_runs_only_after_the_operator_approves() {
    let h = harness();
    let sshd = Sshd::start();
    let marker = h.dir.path().join("approved-marker");
    let command = format!("touch {}", marker.display());
    let mock = model(vec![
        Turn::Tool(COMMAND_TOOL, json!({"command": command})),
        Turn::Text("Created the marker."),
    ])
    .await;
    let (host, provider) = ready(&h, &sshd, &mock).await;

    let (on_event, mut rx) = sink();
    let task = h
        .services
        .agent_start(&host, &provider, "create the marker file", &[], on_event)
        .await
        .expect("start");
    assert!(!task.is_empty());

    let waiting = until(&mut rx, |e| {
        matches!(e, TaskEventView::ApprovalRequired { .. })
    })
    .await;
    let (approval_id, shown) = match waiting.last() {
        Some(TaskEventView::ApprovalRequired {
            approval_id,
            command,
            ..
        }) => (approval_id.clone(), command.clone()),
        other => panic!("expected an approval, got {other:?}"),
    };
    assert_eq!(shown, command, "the operator must see the exact command");
    assert!(!marker.exists(), "nothing runs before approval");

    h.services
        .approval_decide(&approval_id, true, None)
        .expect("approve");
    let seen = until(&mut rx, is_terminal).await;

    assert_eq!(terminal_state(&seen), "completed");
    assert!(marker.exists(), "the approved command should have run");
    assert!(h.services.audit_verify().expect("verify").ok);
}

#[tokio::test]
async fn denied_command_never_runs() {
    let h = harness();
    let sshd = Sshd::start();
    let marker = h.dir.path().join("denied-marker");
    let command = format!("touch {}", marker.display());
    let mock = model(vec![
        Turn::Tool(COMMAND_TOOL, json!({"command": command})),
        Turn::Text("Understood, I did not run it."),
    ])
    .await;
    let (host, provider) = ready(&h, &sshd, &mock).await;

    let (on_event, mut rx) = sink();
    h.services
        .agent_start(&host, &provider, "create the marker file", &[], on_event)
        .await
        .expect("start");
    let waiting = until(&mut rx, |e| {
        matches!(e, TaskEventView::ApprovalRequired { .. })
    })
    .await;
    let approval_id = match waiting.last() {
        Some(TaskEventView::ApprovalRequired { approval_id, .. }) => approval_id.clone(),
        other => panic!("expected an approval, got {other:?}"),
    };

    h.services
        .approval_decide(&approval_id, false, None)
        .expect("deny");
    let seen = until(&mut rx, is_terminal).await;

    assert_eq!(terminal_state(&seen), "completed");
    assert!(!marker.exists(), "a denied command must not run");
    let history = h.services.history_list(&host, None).expect("history");
    assert_eq!(history.items.len(), 1);
    assert_eq!(history.items[0].state, "denied");
}

#[tokio::test]
async fn cancel_while_awaiting_approval_stops_the_task_without_running() {
    let h = harness();
    let sshd = Sshd::start();
    let marker = h.dir.path().join("cancelled-marker");
    let command = format!("touch {}", marker.display());
    let mock = model(vec![Turn::Tool(COMMAND_TOOL, json!({"command": command}))]).await;
    let (host, provider) = ready(&h, &sshd, &mock).await;

    let (on_event, mut rx) = sink();
    let task = h
        .services
        .agent_start(&host, &provider, "create the marker file", &[], on_event)
        .await
        .expect("start");
    until(&mut rx, |e| {
        matches!(e, TaskEventView::ApprovalRequired { .. })
    })
    .await;

    assert!(h.services.agent_cancel(&task).expect("cancel"));
    let seen = until(&mut rx, is_terminal).await;

    assert_eq!(terminal_state(&seen), "cancelled");
    assert!(
        !marker.exists(),
        "a cancelled task must not run its command"
    );
}

#[tokio::test]
async fn archived_hosts_and_unverified_providers_are_refused_before_any_connection() {
    let h = harness();
    let sshd = Sshd::start();
    let mock = model(vec![]).await;
    let (host, provider) = ready(&h, &sshd, &mock).await;

    // An untested provider is refused even for a valid host.
    let untested = h
        .services
        .provider_save(ProviderInput {
            name: "untested".to_owned(),
            ..provider_input(&format!("{}/v1", mock.uri()))
        })
        .expect("save untested provider");
    let (on_event, _rx) = sink();
    let err = h
        .services
        .agent_start(&host, &untested.id, "check disk usage", &[], on_event)
        .await
        .unwrap_err();
    assert_eq!(err_code(err), ErrorCode::ProviderToolUnsupported);

    // An archived host is refused even with a verified provider.
    h.services.host_archive(&host).expect("archive");
    let (on_event, _rx) = sink();
    let err = h
        .services
        .agent_start(&host, &provider, "check disk usage", &[], on_event)
        .await
        .unwrap_err();
    assert_eq!(err_code(err), ErrorCode::TaskScopeInvalid);
}

#[tokio::test]
async fn no_credential_reaches_the_model_and_history_survives_a_restart() {
    let h = harness();
    let sshd = Sshd::start();
    let mock = model(vec![
        Turn::Tool(DIAGNOSTIC_TOOL, json!({"operation": "uptime"})),
        Turn::Text("Up and running."),
    ])
    .await;
    let (host, provider) = ready(&h, &sshd, &mock).await;

    let (on_event, mut rx) = sink();
    h.services
        .agent_start(&host, &provider, "how long has it been up", &[], on_event)
        .await
        .expect("start");
    let seen = until(&mut rx, is_terminal).await;
    assert_eq!(terminal_state(&seen), "completed");

    // Every request the model received, in full: the SSH password and the
    // provider key must appear in none of them.
    let received = mock.received_requests().await.expect("recording enabled");
    assert!(!received.is_empty());
    let password = material().password.clone();
    for req in &received {
        let text = String::from_utf8_lossy(&req.body);
        assert!(
            !text.contains(&password),
            "the SSH password reached the model"
        );
        assert!(!text.contains(KEY), "the provider key reached the model");
    }

    // Reopen the database as after an app restart: the history is still there.
    let Harness {
        services,
        store,
        db_path,
        dir,
    } = h;
    drop(services);
    let reopened = Services::open(&db_path, store).expect("reopen");
    let history = reopened.history_list(&host, None).expect("history");
    assert_eq!(history.items.len(), 1);
    assert_eq!(history.items[0].command_redacted, "uptime");
    drop(dir);
}
