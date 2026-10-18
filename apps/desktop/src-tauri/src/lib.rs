//! Tauri composition root. Owns the IPC surface and the service instance.
//! Authorization lives in `service` and the Rust crates, never in the webview.

pub mod agent_service;
pub mod chat_service;
pub mod chat_sessions;
pub mod commands;
pub mod dto;
pub mod history_service;
pub mod kdbx_import;
pub mod local_exec;
mod local_terminal;
pub mod provider_service;
pub mod service;
pub mod vault_service;

use std::sync::Arc;

use dto::CommandError;
use selai_domain::ErrorCode;
use tauri::Manager;

/// Startup outcome. A failure is kept so the UI can show its code and message
/// instead of the app exiting without explanation.
pub struct AppState {
    vault: Result<Arc<vault_service::VaultManager>, CommandError>,
}

impl AppState {
    pub fn services(&self) -> Result<vault_service::ServiceLease, CommandError> {
        self.vault()?.services()
    }
    pub fn vault(&self) -> Result<Arc<vault_service::VaultManager>, CommandError> {
        self.vault.as_ref().cloned().map_err(Clone::clone)
    }
}

pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            selai_platform::start();
            let vault = match app.path().app_data_dir() {
                Ok(dir) => vault_service::VaultManager::new(dir),
                Err(err) => Err(CommandError::new(ErrorCode::StorageIo, err.to_string())),
            };
            if let Ok(manager) = &vault {
                let manager = manager.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        if manager.idle() {
                            let force = selai_platform::must_lock();
                            let manager = manager.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = manager.lock(force).await;
                            });
                        }
                    }
                });
            }
            app.manage(AppState { vault });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::vault_status,
            commands::vault_activity,
            commands::vault_lock,
            commands::vault_flush_complete,
            commands::vault_action,
            commands::storage_status,
            commands::host_list,
            commands::host_get,
            commands::host_save,
            commands::host_move_group,
            commands::ssh_key_list,
            commands::ssh_key_import,
            commands::ssh_key_generate,
            commands::ssh_key_delete,
            commands::kdbx_preview,
            commands::ssh_config_read,
            commands::host_archive,
            commands::host_probe,
            commands::host_trust,
            commands::group_list,
            commands::group_create,
            commands::group_rename,
            commands::group_delete,
            commands::host_reveal_password,
            commands::terminal_connect,
            commands::terminal_input,
            commands::terminal_resize,
            commands::terminal_disconnect,
            commands::provider_templates,
            commands::provider_list,
            commands::provider_save,
            commands::provider_delete,
            commands::provider_models,
            commands::provider_test,
            commands::agent_start,
            commands::agent_cancel,
            commands::chat_send,
            commands::chat_session_list,
            commands::chat_session_get,
            commands::chat_session_save,
            commands::approval_decide,
            commands::sudo_options,
            commands::host_forget_sudo_password,
            commands::local_host,
            commands::history_list,
            commands::audit_verify,
            commands::history_query,
            commands::history_tasks,
            commands::history_summary,
            commands::task_timeline,
            commands::history_export,
            commands::history_purge_output,
            commands::preferences_get,
            commands::preferences_set,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        // The window system itself failed; there is no UI left to report to.
        eprintln!("selai: failed to start: {err}");
        std::process::exit(1);
    }
}
