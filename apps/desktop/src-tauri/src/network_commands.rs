//! Unlocked-vault IPC for file management and network profiles.
use crate::{
    dto::CommandError,
    network_service::{ForwardProfile, ForwardView, ProxyInput, ProxyView},
    sftp_service::{DirectoryView, SftpView, TransferInput, TransferView},
    AppState,
};
use std::sync::Arc;
use tauri::{ipc::Channel, State};

#[tauri::command]
pub fn forward_watch(
    state: State<'_, AppState>,
    on_event: Channel<ForwardView>,
) -> Result<Vec<ForwardView>, CommandError> {
    state.services()?.forward_watch(Arc::new(move |view| {
        let _ = on_event.send(view);
    }))
}
#[tauri::command]
pub fn transfer_watch(
    state: State<'_, AppState>,
    on_event: Channel<TransferView>,
) -> Result<Vec<TransferView>, CommandError> {
    state.services()?.transfer_watch(Arc::new(move |view| {
        let _ = on_event.send(view);
    }))
}
#[tauri::command]
pub fn sftp_list(state: State<'_, AppState>) -> Result<Vec<SftpView>, CommandError> {
    state.services()?.sftp_list()
}

#[tauri::command]
pub fn proxy_list(state: State<'_, AppState>) -> Result<Vec<ProxyView>, CommandError> {
    state.services()?.proxy_list()
}
#[tauri::command]
pub fn proxy_save(
    state: State<'_, AppState>,
    input: ProxyInput,
) -> Result<ProxyView, CommandError> {
    state.services()?.proxy_save(input)
}
#[tauri::command]
pub fn proxy_delete(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    state.services()?.proxy_delete(&id)
}
#[tauri::command]
pub fn forward_list(state: State<'_, AppState>) -> Result<Vec<ForwardView>, CommandError> {
    state.services()?.forward_list()
}
#[tauri::command]
pub fn forward_save(
    state: State<'_, AppState>,
    profile: ForwardProfile,
) -> Result<ForwardView, CommandError> {
    state.services()?.forward_save(profile)
}
#[tauri::command]
pub async fn forward_start(
    state: State<'_, AppState>,
    id: String,
    on_event: Channel<ForwardView>,
) -> Result<ForwardView, CommandError> {
    state
        .services()?
        .forward_start(
            &id,
            Arc::new(move |view| {
                let _ = on_event.send(view);
            }),
        )
        .await
}
#[tauri::command]
pub fn forward_stop(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    state.services()?.forward_stop(&id)
}
#[tauri::command]
pub fn forward_delete(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    state.services()?.forward_delete(&id)
}
#[tauri::command]
pub async fn sftp_connect(
    state: State<'_, AppState>,
    host_id: String,
) -> Result<SftpView, CommandError> {
    state.services()?.sftp_connect(&host_id).await
}
#[tauri::command]
pub async fn sftp_disconnect(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    state.services()?.sftp_disconnect(&id).await
}
#[tauri::command]
pub async fn files_list(
    state: State<'_, AppState>,
    side: String,
    connection_id: Option<String>,
    path: String,
) -> Result<DirectoryView, CommandError> {
    state
        .services()?
        .files_list(&side, connection_id.as_deref(), &path)
        .await
}
#[tauri::command]
pub async fn files_action(
    state: State<'_, AppState>,
    side: String,
    connection_id: Option<String>,
    action: String,
    path: String,
    new_name: Option<String>,
) -> Result<(), CommandError> {
    state
        .services()?
        .files_action(
            &side,
            connection_id.as_deref(),
            &action,
            &path,
            new_name.as_deref(),
        )
        .await
}
#[tauri::command]
pub fn transfer_start(
    state: State<'_, AppState>,
    input: TransferInput,
    on_event: Channel<TransferView>,
) -> Result<TransferView, CommandError> {
    state.services()?.transfer_start(
        input,
        Arc::new(move |view| {
            let _ = on_event.send(view);
        }),
    )
}
#[tauri::command]
pub fn transfer_list(state: State<'_, AppState>) -> Result<Vec<TransferView>, CommandError> {
    state.services()?.transfer_list()
}
#[tauri::command]
pub fn transfer_clear(state: State<'_, AppState>) -> Result<Vec<String>, CommandError> {
    state.services()?.transfer_clear()
}
#[tauri::command]
pub fn transfer_cancel(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    state.services()?.transfer_cancel(&id)
}
