//! Thin Tauri command layer. Each function forwards to `service::Services`;
//! no decisions are made here.

use std::sync::Arc;

use selai_domain::Page;
use tauri::{ipc::Channel, State};

use crate::{
    agent_service::{AgentEventView, AuditVerifyView, ExecutionView},
    dto::{
        AppInfo, CommandError, GroupView, HostInput, HostView, ProbeView, ProviderInput,
        ProviderTestView, ProviderView, StorageStatus, TemplateView, TerminalEvent, TrustMode,
        TrustView,
    },
    service::{EventSink, Services},
    AppState,
};

fn services(state: &State<'_, AppState>) -> Result<Arc<Services>, CommandError> {
    state.services().cloned()
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    Services::app_info()
}

#[tauri::command]
pub fn storage_status(state: State<'_, AppState>) -> Result<StorageStatus, CommandError> {
    services(&state)?.storage_status()
}

#[tauri::command]
pub fn host_list(
    state: State<'_, AppState>,
    query: Option<String>,
    include_archived: Option<bool>,
    cursor: Option<String>,
) -> Result<Page<HostView>, CommandError> {
    services(&state)?.host_list(query, include_archived.unwrap_or(false), cursor)
}

#[tauri::command]
pub fn host_get(state: State<'_, AppState>, id: String) -> Result<HostView, CommandError> {
    services(&state)?.host_get(&id)
}

#[tauri::command]
pub fn host_save(state: State<'_, AppState>, input: HostInput) -> Result<HostView, CommandError> {
    services(&state)?.host_save(input)
}

#[tauri::command]
pub fn host_archive(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    services(&state)?.host_archive(&id)
}

#[tauri::command]
pub async fn host_probe(state: State<'_, AppState>, id: String) -> Result<ProbeView, CommandError> {
    services(&state)?.host_probe(&id).await
}

#[tauri::command]
pub fn host_trust(
    state: State<'_, AppState>,
    id: String,
    fingerprint_sha256: String,
    mode: TrustMode,
) -> Result<TrustView, CommandError> {
    services(&state)?.host_trust(&id, &fingerprint_sha256, mode)
}

#[tauri::command]
pub fn group_list(state: State<'_, AppState>) -> Result<Vec<GroupView>, CommandError> {
    services(&state)?.group_list()
}

#[tauri::command]
pub fn group_create(
    state: State<'_, AppState>,
    name: String,
    sort_order: Option<i64>,
) -> Result<GroupView, CommandError> {
    services(&state)?.group_create(&name, sort_order.unwrap_or(0))
}

#[tauri::command]
pub async fn terminal_connect(
    state: State<'_, AppState>,
    host_id: String,
    cols: u32,
    rows: u32,
    on_event: Channel<TerminalEvent>,
) -> Result<String, CommandError> {
    let sink: EventSink = Arc::new(move |event| {
        // A closed channel means the window went away; the session ends on its own.
        let _ = on_event.send(event);
    });
    services(&state)?
        .terminal_connect(&host_id, cols, rows, sink)
        .await
}

#[tauri::command]
pub async fn terminal_input(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), CommandError> {
    services(&state)?.terminal_input(&session_id, data).await
}

#[tauri::command]
pub async fn terminal_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), CommandError> {
    services(&state)?
        .terminal_resize(&session_id, cols, rows)
        .await
}

#[tauri::command]
pub async fn terminal_disconnect(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), CommandError> {
    services(&state)?.terminal_disconnect(&session_id).await
}

#[tauri::command]
pub fn provider_templates() -> Vec<TemplateView> {
    Services::provider_templates()
}

#[tauri::command]
pub fn provider_list(
    state: State<'_, AppState>,
    cursor: Option<String>,
) -> Result<Page<ProviderView>, CommandError> {
    services(&state)?.provider_list(cursor)
}

#[tauri::command]
pub fn provider_save(
    state: State<'_, AppState>,
    input: ProviderInput,
) -> Result<ProviderView, CommandError> {
    services(&state)?.provider_save(input)
}

#[tauri::command]
pub fn provider_delete(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    services(&state)?.provider_delete(&id)
}

#[tauri::command]
pub async fn agent_start(
    state: State<'_, AppState>,
    host_id: String,
    provider_id: String,
    prompt: String,
    on_event: Channel<AgentEventView>,
) -> Result<String, CommandError> {
    let sink: Arc<dyn Fn(AgentEventView) + Send + Sync> = Arc::new(move |event| {
        // A closed channel means the window went away; the task keeps running
        // until it finishes or is cancelled, and its audit trail is still written.
        let _ = on_event.send(event);
    });
    services(&state)?
        .agent_start(&host_id, &provider_id, &prompt, sink)
        .await
}

#[tauri::command]
pub fn agent_cancel(state: State<'_, AppState>, task_id: String) -> Result<bool, CommandError> {
    services(&state)?.agent_cancel(&task_id)
}

#[tauri::command]
pub fn approval_decide(
    state: State<'_, AppState>,
    approval_id: String,
    approve: bool,
) -> Result<(), CommandError> {
    services(&state)?.approval_decide(&approval_id, approve)
}

#[tauri::command]
pub fn history_list(
    state: State<'_, AppState>,
    host_id: String,
    cursor: Option<String>,
) -> Result<Page<ExecutionView>, CommandError> {
    services(&state)?.history_list(&host_id, cursor)
}

#[tauri::command]
pub fn audit_verify(state: State<'_, AppState>) -> Result<AuditVerifyView, CommandError> {
    services(&state)?.audit_verify()
}

#[tauri::command]
pub async fn provider_test(
    state: State<'_, AppState>,
    id: String,
    kind: String,
) -> Result<ProviderTestView, CommandError> {
    services(&state)?.provider_test(&id, &kind).await
}
