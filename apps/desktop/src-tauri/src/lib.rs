//! Tauri composition root. Owns the IPC surface and the service instance.
//! Authorization lives in `service` and the Rust crates, never in the webview.

pub mod agent_service;
pub mod chat_service;
pub mod commands;
pub mod dto;
pub mod history_service;
pub mod local_exec;
mod local_terminal;
pub mod provider_service;
pub mod service;

use std::sync::Arc;

use dto::CommandError;
use selai_domain::ErrorCode;
use selai_secrets::KeyringStore;
use service::Services;
use tauri::Manager;

const DB_FILE: &str = "selai.sqlite3";

/// Startup outcome. A failure is kept so the UI can show its code and message
/// instead of the app exiting without explanation.
pub struct AppState {
    services: Result<Arc<Services>, CommandError>,
}

impl AppState {
    pub fn services(&self) -> Result<&Arc<Services>, CommandError> {
        self.services.as_ref().map_err(Clone::clone)
    }
}

fn open_services(dir: &std::path::Path) -> Result<Arc<Services>, CommandError> {
    std::fs::create_dir_all(dir)
        .map_err(|err| CommandError::new(ErrorCode::StorageIo, err.to_string()))?;
    let secrets = Arc::new(KeyringStore::new());
    Services::open(&dir.join(DB_FILE), secrets).map(Arc::new)
}

pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            let services = match app.path().app_data_dir() {
                Ok(dir) => open_services(&dir),
                Err(err) => Err(CommandError::new(ErrorCode::StorageIo, err.to_string())),
            };
            app.manage(AppState { services });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::storage_status,
            commands::host_list,
            commands::host_get,
            commands::host_save,
            commands::host_archive,
            commands::host_probe,
            commands::host_trust,
            commands::group_list,
            commands::group_create,
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
