//! Tauri composition root. Owns the IPC surface and the single local database
//! handle. Authorization lives in the Rust crates, never in the webview.

use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use selai_domain::{ErrorCode, SelaiError};
use selai_storage::{Database, ReconcileReport, StorageError};
use serde::Serialize;
use tauri::{Manager, State};

const DB_FILE: &str = "selai.sqlite3";

/// Startup outcome. A failure is kept rather than aborting, so the UI can show
/// the error code and message.
pub struct AppState {
    storage: Result<StorageHandle, CommandError>,
}

pub struct StorageHandle {
    db: Mutex<Database>,
    reconcile: ReconcileReport,
}

/// Error payload for every failing command: `{ code, message }`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
}

impl CommandError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn from_storage(err: &StorageError) -> Self {
        Self::new(err.code(), err.to_string())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStatus {
    pub schema_version: i64,
    pub journal_mode: String,
    pub interrupted_tasks_at_startup: usize,
    pub interrupted_executions_at_startup: usize,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Selai",
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
fn storage_status(state: State<'_, AppState>) -> Result<StorageStatus, CommandError> {
    let handle = state.storage.as_ref().map_err(Clone::clone)?;
    let db = handle
        .db
        .lock()
        .map_err(|_| CommandError::new(ErrorCode::Internal, "database lock poisoned"))?;
    let map = |err: StorageError| CommandError::from_storage(&err);
    Ok(StorageStatus {
        schema_version: db.schema_version().map_err(map)?,
        journal_mode: db.journal_mode().map_err(map)?,
        interrupted_tasks_at_startup: handle.reconcile.tasks_interrupted,
        interrupted_executions_at_startup: handle.reconcile.executions_interrupted,
    })
}

fn open_storage(dir: &Path) -> Result<StorageHandle, CommandError> {
    std::fs::create_dir_all(dir)
        .map_err(|err| CommandError::new(ErrorCode::StorageIo, err.to_string()))?;
    let mut db =
        Database::open(&dir.join(DB_FILE)).map_err(|err| CommandError::from_storage(&err))?;
    let reconcile = db
        .reconcile_after_restart(now_ms())
        .map_err(|err| CommandError::from_storage(&err))?;
    Ok(StorageHandle {
        db: Mutex::new(db),
        reconcile,
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            let storage = match app.path().app_data_dir() {
                Ok(dir) => open_storage(&dir),
                Err(err) => Err(CommandError::new(ErrorCode::StorageIo, err.to_string())),
            };
            app.manage(AppState { storage });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_info, storage_status])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        // The window system itself failed; there is no UI left to report to.
        eprintln!("selai: failed to start: {err}");
        std::process::exit(1);
    }
}
