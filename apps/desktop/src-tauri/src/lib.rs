//! Tauri composition root. Owns the IPC surface and the service instance.
//! Authorization lives in `service` and the Rust crates, never in the webview.

pub mod commands;
pub mod dto;
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
            commands::provider_test,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        // The window system itself failed; there is no UI left to report to.
        eprintln!("selai: failed to start: {err}");
        std::process::exit(1);
    }
}
