//! Agent tasks through the application service: a real OpenSSH server for the
//! host, a mock provider for the model (tool calling verified first, as the app
//! requires). Covers the approval gate, cancellation, refusals, the rule that no
//! credential reaches the model, and history that survives a restart.

#![cfg(target_os = "linux")] // Needs OpenSSH sshd and root; see docs/release/milestones.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{
    collections::VecDeque,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use selai_agent::{COMMAND_TOOL, DIAGNOSTIC_TOOL};
use selai_desktop_lib::{
    agent_service::{AgentEventView, TaskEventView},
    dto::{CommandError, HostInput, ProviderInput, TrustMode},
    service::Services,
};
use selai_domain::ErrorCode;
use selai_secrets::MemoryStore;
use selai_testkit::{material, Sshd, USER};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, Request, Respond, ResponseTemplate,
};

const LIMIT: Duration = Duration::from_secs(30);
const KEY: &str = "sentinel-provider-key-never-leak";
/// The name stored in the audit and execution tables (schema CHECK), which differs
/// from the provider-safe name the model sees.
const STORED_DIAGNOSTIC: &str = "ssh.run_diagnostic";

// ---- scripted model ---------------------------------------------------------

#[derive(Clone)]
enum Turn {
    Tool(&'static str, Value),
    Text(&'static str),
}

fn sse(payloads: &[Value]) -> String {
    payloads
        .iter()
        .map(|p| format!("data: {p}\n\n"))
        .collect::<String>()
        + "data: [DONE]\n\n"
}

/// Answers the provider probe when it is asked (tool calling is verified before
/// any task), and otherwise plays the scripted turns of the task.
struct AgentModel {
    turns: Arc<Mutex<VecDeque<Turn>>>,
    counter: Arc<Mutex<u32>>,
}

impl Respond for AgentModel {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body_text = String::from_utf8_lossy(&req.body).into_owned();
        let body: Value = serde_json::from_slice(&req.body).expect("json");
        let has_result = body["messages"]
            .as_array()
            .is_some_and(|m| m.iter().any(|x| x["role"] == "tool"));
        let payload = if body_text.contains("selai_echo") {
            if has_result {
                sse(&[
                    json!({"choices":[{"delta":{"content":"done"}}]}),
                    json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
                ])
            } else {
                sse(&[
                    json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"probe","type":"function","function":{"name":"selai_echo","arguments":"{\"text\":\"selai-probe-7f3a\"}"}}]}}]}),
                    json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}),
                ])
            }
        } else {
            let turn = self
                .turns
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or(Turn::Text("done"));
            match turn {
                Turn::Tool(name, args) => {
                    let mut c = self.counter.lock().expect("lock");
                    *c += 1;
                    let call = json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":format!("call_{c}"),"type":"function","function":{"name":name,"arguments":args.to_string()}}]}}]});
                    sse(&[
                        call,
                        json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}),
                    ])
                }
                Turn::Text(t) => sse(&[
                    json!({"choices":[{"delta":{"content":t}}]}),
                    json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
                ]),
            }
        };
        ResponseTemplate::new(200).set_body_raw(payload, "text/event-stream")
    }
}

async fn model(turns: Vec<Turn>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(AgentModel {
            turns: Arc::new(Mutex::new(VecDeque::from(turns))),
            counter: Arc::new(Mutex::new(0)),
        })
        .mount(&server)
        .await;
    server
}

// ---- harness ----------------------------------------------------------------

struct Harness {
    services: Services,
    store: Arc<MemoryStore>,
    db_path: PathBuf,
    dir: tempfile::TempDir,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    // The SSH user writes marker files here, so the directory must be writable by it.
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).expect("chmod");
    let db_path = dir.path().join("selai.sqlite3");
    let store = Arc::new(MemoryStore::new());
    let llm = selai_llm::LlmClient::new(selai_llm::ClientOptions {
        use_system_proxy: false,
        retry_backoff: Duration::from_millis(5),
    })
    .expect("client");
    let services = Services::open(&db_path, store.clone())
        .expect("open")
        .with_llm_client(llm);
    Harness {
        services,
        store,
        db_path,
        dir,
    }
}

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

fn provider_input(base: &str) -> ProviderInput {
    ProviderInput {
        id: None,
        expected_updated_at_ms: None,
        name: "local-model".to_owned(),
        protocol: "openai_chat".to_owned(),
        base_url: base.to_owned(),
        model_id: "mock-model".to_owned(),
        timeout_seconds: 10,
        max_output_tokens: 256,
        api_key: Some(KEY.to_owned()),
        clear_api_key: None,
    }
}

/// Saves a password host, trusts its probed key, saves a provider and verifies
/// its tool calling. Returns both ids.
async fn ready(h: &Harness, sshd: &Sshd, model: &MockServer) -> (String, String) {
    let view = h
        .services
        .host_save(password_host("production-web", sshd.port))
        .expect("save host");
    let probe = h.services.host_probe(&view.id).await.expect("probe");
    h.services
        .host_trust(
            &view.id,
            &probe.observed.fingerprint_sha256,
            TrustMode::First,
        )
        .expect("trust");

    let provider = h
        .services
        .provider_save(provider_input(&format!("{}/v1", model.uri())))
        .expect("save provider");
    let test = h
        .services
        .provider_test(&provider.id, "tool_calling")
        .await
        .expect("test runs");
    assert!(
        test.verified,
        "tool calling should verify: {}",
        test.message
    );
    (view.id, provider.id)
}

type Events = mpsc::UnboundedReceiver<AgentEventView>;

fn sink() -> (Arc<dyn Fn(AgentEventView) + Send + Sync>, Events) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        Arc::new(move |event| {
            let _ = tx.send(event);
        }),
        rx,
    )
}

/// Collects events until `stop` matches the last one. Panics with what was seen.
async fn until(
    rx: &mut Events,
    mut stop: impl FnMut(&TaskEventView) -> bool,
) -> Vec<TaskEventView> {
    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + LIMIT;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(ev)) => {
                let done = stop(&ev.event);
                seen.push(ev.event);
                if done {
                    return seen;
                }
            }
            Ok(None) | Err(_) => panic!("timed out; events seen: {seen:?}"),
        }
    }
}

fn is_terminal(event: &TaskEventView) -> bool {
    matches!(
        event,
        TaskEventView::State { state, .. } if matches!(state.as_str(), "completed" | "failed" | "cancelled")
    )
}

fn terminal_state(events: &[TaskEventView]) -> String {
    events
        .iter()
        .rev()
        .find_map(|e| match e {
            TaskEventView::State { state, .. } => Some(state.clone()),
            _ => None,
        })
        .expect("a terminal state")
}

fn err_code(err: CommandError) -> ErrorCode {
    err.code
}

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
        .agent_start(&host, &provider, "check disk usage", on_event)
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
        .agent_start(&host, &provider, "give me a health summary", on_event)
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
        .agent_start(&host, &provider, "create the marker file", on_event)
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
        .approval_decide(&approval_id, true)
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
        .agent_start(&host, &provider, "create the marker file", on_event)
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
        .approval_decide(&approval_id, false)
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
        .agent_start(&host, &provider, "create the marker file", on_event)
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
        .agent_start(&host, &untested.id, "check disk usage", on_event)
        .await
        .unwrap_err();
    assert_eq!(err_code(err), ErrorCode::ProviderToolUnsupported);

    // An archived host is refused even with a verified provider.
    h.services.host_archive(&host).expect("archive");
    let (on_event, _rx) = sink();
    let err = h
        .services
        .agent_start(&host, &provider, "check disk usage", on_event)
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
        .agent_start(&host, &provider, "how long has it been up", on_event)
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
