//! Shared harness for the desktop agent suites: an OpenSSH fixture, a mock
//! provider that answers the tool-calling probe and scripted task turns, and
//! helpers to drive a task to its end.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{
    collections::VecDeque,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

pub use selai_desktop_lib::{
    agent_service::{AgentEventView, TaskEventView},
    dto::{CommandError, HostInput, ProviderInput, TrustMode},
    service::Services,
};
pub use selai_domain::ErrorCode;
pub use selai_secrets::MemoryStore;
pub use selai_testkit::{material, Sshd, USER};
pub use serde_json::{json, Value};
pub use tokio::sync::mpsc;
pub use wiremock::{
    matchers::{method, path},
    Mock, MockServer, Request, Respond, ResponseTemplate,
};

pub const LIMIT: Duration = Duration::from_secs(30);
/// The name stored in the audit and execution tables (schema CHECK), which differs
/// from the provider-safe name the model sees.
pub const STORED_DIAGNOSTIC: &str = "ssh.run_diagnostic";
pub const KEY: &str = "sentinel-provider-key-never-leak";
// ---- scripted model ---------------------------------------------------------

#[derive(Clone)]
pub enum Turn {
    Tool(&'static str, Value),
    Text(&'static str),
}

pub fn sse(payloads: &[Value]) -> String {
    payloads
        .iter()
        .map(|p| format!("data: {p}\n\n"))
        .collect::<String>()
        + "data: [DONE]\n\n"
}

/// Answers the provider probe when it is asked (tool calling is verified before
/// any task), and otherwise plays the scripted turns of the task.
pub struct AgentModel {
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

pub async fn model(turns: Vec<Turn>) -> MockServer {
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

pub struct Harness {
    pub services: Services,
    pub store: Arc<MemoryStore>,
    pub db_path: PathBuf,
    pub dir: tempfile::TempDir,
}

pub fn harness() -> Harness {
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

pub fn password_host(label: &str, port: u16) -> HostInput {
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

pub fn provider_input(base: &str) -> ProviderInput {
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
        allow_insecure_http: None,
    }
}

/// Saves a password host, trusts its probed key, saves a provider and verifies
/// its tool calling. Returns both ids.
pub async fn ready(h: &Harness, sshd: &Sshd, model: &MockServer) -> (String, String) {
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

pub type Events = mpsc::UnboundedReceiver<AgentEventView>;

pub fn sink() -> (Arc<dyn Fn(AgentEventView) + Send + Sync>, Events) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        Arc::new(move |event| {
            let _ = tx.send(event);
        }),
        rx,
    )
}

/// Collects events until `stop` matches the last one. Panics with what was seen.
pub async fn until(
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

pub fn is_terminal(event: &TaskEventView) -> bool {
    matches!(
        event,
        TaskEventView::State { state, .. } if matches!(state.as_str(), "completed" | "failed" | "cancelled")
    )
}

pub fn terminal_state(events: &[TaskEventView]) -> String {
    events
        .iter()
        .rev()
        .find_map(|e| match e {
            TaskEventView::State { state, .. } => Some(state.clone()),
            _ => None,
        })
        .expect("a terminal state")
}

pub fn err_code(err: CommandError) -> ErrorCode {
    err.code
}
