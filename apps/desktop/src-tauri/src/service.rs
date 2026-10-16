//! Application services behind the Tauri commands.
//!
//! This layer holds the rules the webview cannot override:
//! - secrets go to the OS store before host metadata is committed;
//! - a host key is pinned only if the user confirmed the key from a recent probe;
//! - a terminal session is bound to one host and never reused for another.
//!
//! It has no Tauri types, so the end-to-end tests drive the same code the
//! commands use.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use selai_domain::{
    AuthMethod, ConnectTimeout, ErrorCode, GroupName, HostGroupId, HostId, HostLabel, HostSettings,
    Hostname, Keepalive, KeyPath, Notes, Page, PageRequest, Port, SelaiError, TerminalType,
    Username,
};
use selai_secrets::{SecretKey, SecretStore, SecretValue, SshSecretKind};
use selai_ssh::{
    connect, probe_host_key, Auth, ConnectOptions, Endpoint, ObservedHostKey, PinnedHostKey,
    Session, Shell, ShellEvent, SshError,
};
use selai_storage::{Database, PinOutcome, PinnedKey, ReconcileReport, StorageError};
use tokio::sync::mpsc;

use crate::dto::{
    AppInfo, CommandError, GroupView, HostInput, HostView, KeyErrorDetails, ObservedKeyView,
    ProbeView, StorageStatus, TerminalEvent, TrustMode, TrustView,
};

/// How long a probed key may be confirmed. Short on purpose.
const PROBE_TTL: Duration = Duration::from_secs(300);
const MAX_TERMINAL_DIM: u32 = 1000;

pub type EventSink = Arc<dyn Fn(TerminalEvent) + Send + Sync>;

struct CachedProbe {
    observed: ObservedHostKey,
    at: Instant,
}

enum TerminalInput {
    Data(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

#[derive(Clone)]
struct TerminalHandle {
    host_id: HostId,
    input: mpsc::Sender<TerminalInput>,
}

pub struct Services {
    pub(crate) db: Arc<Mutex<Database>>,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) llm: Arc<selai_llm::LlmClient>,
    pub(crate) agent: Arc<crate::agent_service::AgentRegistry>,
    /// Where history exports are written. `None` for an in-memory database.
    pub(crate) export_dir: Option<std::path::PathBuf>,
    probes: Mutex<HashMap<HostId, CachedProbe>>,
    terminals: Arc<Mutex<HashMap<String, TerminalHandle>>>,
    reconcile: ReconcileReport,
}

impl Services {
    /// Opens the database at `path` and marks work interrupted by the previous
    /// run. Nothing is re-run.
    pub fn open(path: &Path, secrets: Arc<dyn SecretStore>) -> Result<Self, CommandError> {
        let mut db = Database::open(path).map_err(storage_error)?;
        let reconcile = db
            .reconcile_after_restart(now_ms())
            .map_err(storage_error)?;
        let mut services = Self::with_database(db, secrets, reconcile)?;
        services.export_dir = crate::history_service::export_dir_for(path);
        // A failed retention sweep must not stop the app from opening. It runs again
        // at the next start.
        let _ = services.apply_retention(now_ms());
        Ok(services)
    }

    pub fn in_memory(secrets: Arc<dyn SecretStore>) -> Result<Self, CommandError> {
        let mut db = Database::open_in_memory().map_err(storage_error)?;
        let reconcile = db
            .reconcile_after_restart(now_ms())
            .map_err(storage_error)?;
        Self::with_database(db, secrets, reconcile)
    }

    fn with_database(
        db: Database,
        secrets: Arc<dyn SecretStore>,
        reconcile: ReconcileReport,
    ) -> Result<Self, CommandError> {
        let llm = selai_llm::LlmClient::new(selai_llm::ClientOptions::default())
            .map_err(|e| CommandError::new(e.code, e.message))?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            secrets,
            llm: Arc::new(llm),
            agent: Arc::new(crate::agent_service::AgentRegistry::default()),
            export_dir: None,
            probes: Mutex::new(HashMap::new()),
            terminals: Arc::new(Mutex::new(HashMap::new())),
            reconcile,
        })
    }

    /// Replaces the provider HTTP client. Tests use this to turn off the system
    /// proxy so requests reach a local mock server.
    pub fn with_llm_client(mut self, client: selai_llm::LlmClient) -> Self {
        self.llm = Arc::new(client);
        self
    }

    pub fn app_info() -> AppInfo {
        AppInfo {
            name: "Selai",
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    pub fn storage_status(&self) -> Result<StorageStatus, CommandError> {
        let db = self.lock_db()?;
        Ok(StorageStatus {
            schema_version: db.schema_version().map_err(storage_error)?,
            journal_mode: db.journal_mode().map_err(storage_error)?,
            interrupted_tasks_at_startup: self.reconcile.tasks_interrupted,
            interrupted_executions_at_startup: self.reconcile.executions_interrupted,
        })
    }

    // ---- hosts and groups -------------------------------------------------

    pub fn host_list(
        &self,
        query: Option<String>,
        include_archived: bool,
        cursor: Option<String>,
    ) -> Result<Page<HostView>, CommandError> {
        let db = self.lock_db()?;
        let page = db
            .list_hosts(
                include_archived,
                query.as_deref().filter(|q| !q.trim().is_empty()),
                &PageRequest { cursor, limit: 50 },
            )
            .map_err(storage_error)?;
        Ok(Page {
            items: page.items.into_iter().map(HostView::from).collect(),
            next_cursor: page.next_cursor,
        })
    }

    pub fn host_get(&self, id: &str) -> Result<HostView, CommandError> {
        let id = parse_id::<HostId>(id, "host")?;
        let db = self.lock_db()?;
        db.get_host(id)
            .map_err(storage_error)?
            .map(HostView::from)
            .ok_or_else(|| CommandError::new(ErrorCode::NotFound, "host not found"))
    }

    /// Creates or updates a host. Secrets are written to the OS store first;
    /// the host row is committed after, so a failed store write changes nothing.
    pub fn host_save(&self, input: HostInput) -> Result<HostView, CommandError> {
        let settings = input.to_settings()?;
        let now = now_ms();
        let previous = match input.id.as_deref() {
            Some(raw) => {
                let id = parse_id::<HostId>(raw, "host")?;
                let record = self
                    .lock_db()?
                    .get_host(id)
                    .map_err(storage_error)?
                    .ok_or_else(|| CommandError::new(ErrorCode::NotFound, "host not found"))?;
                Some(record)
            }
            None => None,
        };
        let id = previous.as_ref().map_or_else(HostId::new, |p| p.id);
        let auth_changed = previous.as_ref().is_some_and(|p| p.auth != settings.auth);

        // 1. Stage secrets in the OS store.
        let mut configured = previous.as_ref().is_some_and(|p| p.credential_configured);
        let password_key = SecretKey::ssh(id, SshSecretKind::Password);
        let passphrase_key = SecretKey::ssh(id, SshSecretKind::KeyPassphrase);
        let mut written: Vec<SecretKey> = Vec::new();
        match settings.auth {
            AuthMethod::Password => {
                if let Some(pw) = non_empty(input.password.as_deref()) {
                    self.secrets
                        .put(&password_key, &SecretValue::new(pw))
                        .map_err(secret_error)?;
                    written.push(password_key.clone());
                    configured = true;
                }
                if auth_changed {
                    self.secrets.delete(&passphrase_key).map_err(secret_error)?;
                    configured = written.contains(&password_key);
                }
            }
            AuthMethod::PrivateKey => {
                if let Some(pp) = non_empty(input.key_passphrase.as_deref()) {
                    self.secrets
                        .put(&passphrase_key, &SecretValue::new(pp))
                        .map_err(secret_error)?;
                    written.push(passphrase_key.clone());
                    configured = true;
                }
                if auth_changed {
                    self.secrets.delete(&password_key).map_err(secret_error)?;
                    configured = written.contains(&passphrase_key);
                }
            }
        }

        // 2. Commit the host row.
        let saved = {
            let mut db = self.lock_db()?;
            let result = match previous {
                Some(ref p) => db.update_host(
                    id,
                    &settings,
                    input.expected_updated_at_ms.unwrap_or(p.updated_at_ms),
                    now,
                ),
                None => db.create_host(id, &settings, now),
            };
            match result {
                Ok(record) => record,
                Err(err) => {
                    // A new host has no earlier state to keep, so its secrets are removed.
                    // For an update the store keeps the newer value; the flag stays as it was.
                    if previous.is_none() {
                        for key in &written {
                            let _ = self.secrets.delete(key);
                        }
                    }
                    return Err(storage_error(err));
                }
            }
        };

        // 3. Record whether a credential is configured.
        self.lock_db()?
            .set_credential_configured(id, configured, now)
            .map_err(storage_error)?;
        self.host_get(&saved.id.to_string())
    }

    /// Archives a host, deletes its stored secrets, keeps history and pins.
    pub fn host_archive(&self, id: &str) -> Result<(), CommandError> {
        let id = parse_id::<HostId>(id, "host")?;
        self.lock_db()?
            .archive_host(id, now_ms())
            .map_err(storage_error)?;
        self.secrets
            .delete(&SecretKey::ssh(id, SshSecretKind::Password))
            .map_err(secret_error)?;
        self.secrets
            .delete(&SecretKey::ssh(id, SshSecretKind::KeyPassphrase))
            .map_err(secret_error)?;
        self.lock_db()?
            .set_credential_configured(id, false, now_ms())
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn group_list(&self) -> Result<Vec<GroupView>, CommandError> {
        let db = self.lock_db()?;
        Ok(db
            .list_groups()
            .map_err(storage_error)?
            .into_iter()
            .map(GroupView::from)
            .collect())
    }

    pub fn group_create(&self, name: &str, sort_order: i64) -> Result<GroupView, CommandError> {
        let name = GroupName::parse(name).map_err(invalid)?;
        let db = self.lock_db()?;
        let group = db
            .create_group(HostGroupId::new(), &name, sort_order, now_ms())
            .map_err(storage_error)?;
        Ok(GroupView::from(group))
    }

    // ---- host-key trust ----------------------------------------------------

    /// Connects far enough to read the server key, compares it with the pins,
    /// and remembers it for a short time so `host_trust` can confirm it.
    pub async fn host_probe(&self, host_id: &str) -> Result<ProbeView, CommandError> {
        let id = parse_id::<HostId>(host_id, "host")?;
        let (host, pins) = self.active_host_and_pins(id)?;
        let endpoint = Endpoint::new(host.hostname.clone(), host.port);
        let observed = probe_host_key(
            &endpoint,
            Duration::from_secs(u64::from(host.connect_timeout_seconds)),
        )
        .await
        .map_err(|e| ssh_error(&e))?;

        let (status, pinned_fp) = compare_with_pins(&observed, &pins);
        self.probes.lock().map_err(|_| poisoned())?.insert(
            id,
            CachedProbe {
                observed: observed.clone(),
                at: Instant::now(),
            },
        );
        Ok(ProbeView {
            observed: observed_view(&observed),
            status,
            pinned_fingerprint_sha256: pinned_fp,
        })
    }

    /// When a connection stops at an unknown or changed host key, keeps the key the
    /// server presented as a fresh probe. The user can then review and trust it in
    /// the connect flow, without a separate probe. Trusting still needs the exact
    /// fingerprint that was shown, within the probe lifetime.
    pub(crate) fn remember_observed_key(&self, id: HostId, err: &SshError) {
        let observed = match err {
            SshError::HostKeyUnknown(observed) => observed.clone(),
            SshError::HostKeyChanged { observed, .. } => observed.clone(),
            _ => return,
        };
        if let Ok(mut probes) = self.probes.lock() {
            probes.insert(
                id,
                CachedProbe {
                    observed,
                    at: Instant::now(),
                },
            );
        }
    }

    /// Pins a key that the user confirmed from a fresh probe. The fingerprint
    /// must match the probe; the webview cannot supply a key of its own.
    pub fn host_trust(
        &self,
        host_id: &str,
        fingerprint_sha256: &str,
        mode: TrustMode,
    ) -> Result<TrustView, CommandError> {
        let id = parse_id::<HostId>(host_id, "host")?;
        let observed = {
            let mut probes = self.probes.lock().map_err(|_| poisoned())?;
            let cached = probes.get(&id).ok_or_else(|| {
                CommandError::new(
                    ErrorCode::InvalidInput,
                    "probe the host key before trusting it",
                )
            })?;
            if cached.at.elapsed() > PROBE_TTL {
                probes.remove(&id);
                return Err(CommandError::new(
                    ErrorCode::InvalidInput,
                    "the probed key expired; probe again",
                ));
            }
            if cached.observed.fingerprint_sha256 != fingerprint_sha256 {
                return Err(CommandError::new(
                    ErrorCode::InvalidInput,
                    "the fingerprint does not match the key from the last probe",
                ));
            }
            cached.observed.clone()
        };
        let key = PinnedKey {
            algorithm: observed.algorithm.clone(),
            public_key_base64: observed.public_key_base64.clone(),
            fingerprint_sha256: observed.fingerprint_sha256.clone(),
            first_seen_at_ms: now_ms(),
        };
        let outcome = {
            let mut db = self.lock_db()?;
            match mode {
                TrustMode::First => db.pin_host_key(id, &key).map_err(storage_error)?,
                TrustMode::Replace => db
                    .rotate_host_key(id, &key, now_ms())
                    .map_err(storage_error)?,
            }
        };
        self.probes.lock().map_err(|_| poisoned())?.remove(&id);
        let outcome = match outcome {
            PinOutcome::Pinned => "pinned",
            PinOutcome::Rotated { .. } => "rotated",
            PinOutcome::Unchanged => "unchanged",
        };
        Ok(TrustView {
            outcome,
            fingerprint_sha256: observed.fingerprint_sha256,
        })
    }

    // ---- terminal sessions -------------------------------------------------

    /// Opens an authenticated shell. The returned id is the only handle the
    /// webview gets; it is bound to `host_id` for its whole life.
    pub async fn terminal_connect(
        &self,
        host_id: &str,
        cols: u32,
        rows: u32,
        sink: EventSink,
    ) -> Result<String, CommandError> {
        let id = parse_id::<HostId>(host_id, "host")?;
        let cols = bounded_dim(cols)?;
        let rows = bounded_dim(rows)?;
        let (host, pins) = self.active_host_and_pins(id)?;
        let auth = self.load_auth(id, &host)?;

        let options = ConnectOptions {
            endpoint: Endpoint::new(host.hostname.clone(), host.port),
            username: host.username.clone(),
            auth,
            pinned: pins,
            connect_timeout: Duration::from_secs(u64::from(host.connect_timeout_seconds)),
            keepalive: match host.keepalive_seconds {
                0 => None,
                s => Some(Duration::from_secs(u64::from(s))),
            },
        };
        let session = match connect(options).await {
            Ok(session) => session,
            Err(e) => {
                self.remember_observed_key(id, &e);
                return Err(ssh_error(&e));
            }
        };
        let shell = session
            .open_shell(cols, rows, &host.terminal_type)
            .await
            .map_err(|e| ssh_error(&e))?;

        let session_id = HostId::new().to_string();
        let (tx, rx) = mpsc::channel(64);
        self.terminals.lock().map_err(|_| poisoned())?.insert(
            session_id.clone(),
            TerminalHandle {
                host_id: id,
                input: tx,
            },
        );
        let _ = self.lock_db()?.mark_connected(id, now_ms());

        tokio::spawn(run_terminal(
            session_id.clone(),
            session,
            shell,
            rx,
            sink,
            Arc::clone(&self.terminals),
        ));
        Ok(session_id)
    }

    pub async fn terminal_input(&self, session_id: &str, data: String) -> Result<(), CommandError> {
        let handle = self.terminal(session_id)?;
        handle
            .input
            .send(TerminalInput::Data(data.into_bytes()))
            .await
            .map_err(|_| {
                CommandError::new(ErrorCode::SshChannelClosed, "terminal session has ended")
            })
    }

    pub async fn terminal_resize(
        &self,
        session_id: &str,
        cols: u32,
        rows: u32,
    ) -> Result<(), CommandError> {
        let cols = bounded_dim(cols)?;
        let rows = bounded_dim(rows)?;
        let handle = self.terminal(session_id)?;
        handle
            .input
            .send(TerminalInput::Resize { cols, rows })
            .await
            .map_err(|_| {
                CommandError::new(ErrorCode::SshChannelClosed, "terminal session has ended")
            })
    }

    pub async fn terminal_disconnect(&self, session_id: &str) -> Result<(), CommandError> {
        let handle = self.terminal(session_id)?;
        let _ = handle.input.send(TerminalInput::Close).await;
        Ok(())
    }

    /// The host a live terminal session is bound to. Used by tests and by the
    /// UI to show which host a tab belongs to.
    pub fn terminal_host(&self, session_id: &str) -> Option<HostId> {
        self.terminals
            .lock()
            .ok()?
            .get(session_id)
            .map(|h| h.host_id)
    }

    // ---- helpers -----------------------------------------------------------

    fn lock_db(&self) -> Result<std::sync::MutexGuard<'_, Database>, CommandError> {
        self.db.lock().map_err(|_| poisoned())
    }

    pub(crate) fn lock_db_pub(&self) -> Result<std::sync::MutexGuard<'_, Database>, CommandError> {
        self.lock_db()
    }

    fn terminal(&self, session_id: &str) -> Result<TerminalHandle, CommandError> {
        self.terminals
            .lock()
            .map_err(|_| poisoned())?
            .get(session_id)
            .cloned()
            .ok_or_else(|| CommandError::new(ErrorCode::NotFound, "no such terminal session"))
    }

    pub(crate) fn active_host_and_pins(
        &self,
        id: HostId,
    ) -> Result<(selai_storage::HostRecord, Vec<PinnedHostKey>), CommandError> {
        let db = self.lock_db()?;
        let host = db
            .get_host(id)
            .map_err(storage_error)?
            .ok_or_else(|| CommandError::new(ErrorCode::NotFound, "host not found"))?;
        if host.archived_at_ms.is_some() {
            return Err(CommandError::new(
                ErrorCode::TaskScopeInvalid,
                "host is archived",
            ));
        }
        let pins = db
            .active_pinned_keys(id)
            .map_err(storage_error)?
            .into_iter()
            .map(|k| PinnedHostKey {
                algorithm: k.algorithm,
                public_key_base64: k.public_key_base64,
                fingerprint_sha256: k.fingerprint_sha256,
            })
            .collect();
        Ok((host, pins))
    }

    pub(crate) fn load_auth(
        &self,
        id: HostId,
        host: &selai_storage::HostRecord,
    ) -> Result<Auth, CommandError> {
        match host.auth {
            AuthMethod::Password => {
                let secret = self
                    .secrets
                    .get(&SecretKey::ssh(id, SshSecretKind::Password))
                    .map_err(secret_error)?
                    .ok_or_else(|| {
                        CommandError::new(
                            ErrorCode::InvalidInput,
                            "no password saved for this host; edit the host to add one",
                        )
                    })?;
                Ok(Auth::Password(secret))
            }
            AuthMethod::PrivateKey => {
                let path = host.private_key_path.clone().ok_or_else(|| {
                    CommandError::new(ErrorCode::InvalidInput, "host has no private key path")
                })?;
                let passphrase = self
                    .secrets
                    .get(&SecretKey::ssh(id, SshSecretKind::KeyPassphrase))
                    .map_err(secret_error)?;
                Ok(Auth::PrivateKey {
                    path: path.into(),
                    passphrase,
                })
            }
        }
    }
}

/// One terminal session: pumps input to the PTY and PTY output to the sink.
/// It owns the session, so the session always closes when the loop ends.
async fn run_terminal(
    session_id: String,
    session: Session,
    mut shell: Shell,
    mut input: mpsc::Receiver<TerminalInput>,
    sink: EventSink,
    terminals: Arc<Mutex<HashMap<String, TerminalHandle>>>,
) {
    let mut exit_status: Option<u32> = None;
    loop {
        tokio::select! {
            command = input.recv() => match command {
                Some(TerminalInput::Data(bytes)) => {
                    if shell.write(&bytes).await.is_err() {
                        break;
                    }
                }
                Some(TerminalInput::Resize { cols, rows }) => {
                    let _ = shell.resize(cols, rows).await;
                }
                Some(TerminalInput::Close) | None => {
                    let _ = shell.close().await;
                    break;
                }
            },
            event = shell.next_event() => match event {
                ShellEvent::Data(bytes) => {
                    sink(TerminalEvent::Data { data: B64.encode(bytes) });
                }
                ShellEvent::Exit { status } => {
                    exit_status = Some(status);
                    sink(TerminalEvent::Exit { status });
                }
                ShellEvent::Refused(message) => {
                    sink(TerminalEvent::Error {
                        code: ErrorCode::SshChannelClosed,
                        message,
                    });
                }
                ShellEvent::Closed => break,
            },
        }
    }
    // The channel is already gone; a failed disconnect has no further effect.
    let _ = session.disconnect().await;
    if let Ok(mut map) = terminals.lock() {
        map.remove(&session_id);
    }
    sink(TerminalEvent::Closed { exit_status });
}

fn compare_with_pins(
    observed: &ObservedHostKey,
    pins: &[PinnedHostKey],
) -> (&'static str, Option<String>) {
    if let Some(pin) = pins.iter().find(|p| p.algorithm == observed.algorithm) {
        return if pin.public_key_base64 == observed.public_key_base64 {
            ("trusted", None)
        } else {
            ("changed", Some(pin.fingerprint_sha256.clone()))
        };
    }
    match pins.first() {
        Some(other) => ("changed", Some(other.fingerprint_sha256.clone())),
        None => ("unknown", None),
    }
}

fn observed_view(k: &ObservedHostKey) -> ObservedKeyView {
    ObservedKeyView {
        algorithm: k.algorithm.clone(),
        fingerprint_sha256: k.fingerprint_sha256.clone(),
        public_key_base64: k.public_key_base64.clone(),
    }
}

impl HostInput {
    fn to_settings(&self) -> Result<HostSettings, CommandError> {
        let auth = AuthMethod::parse(&self.auth_method).ok_or_else(|| {
            CommandError::new(
                ErrorCode::InvalidInput,
                "authMethod must be password or private_key",
            )
        })?;
        let private_key_path = match self.private_key_path.as_deref() {
            Some(p) if !p.is_empty() => Some(KeyPath::parse(p).map_err(invalid)?),
            _ => None,
        };
        let settings = HostSettings {
            label: HostLabel::parse(&self.label).map_err(invalid)?,
            hostname: Hostname::parse(&self.hostname).map_err(invalid)?,
            port: Port::new(self.port).map_err(invalid)?,
            username: Username::parse(&self.username).map_err(invalid)?,
            auth,
            private_key_path,
            group_id: match self.group_id.as_deref() {
                Some(g) if !g.is_empty() => Some(parse_id::<HostGroupId>(g, "group")?),
                _ => None,
            },
            connect_timeout: ConnectTimeout::new(self.connect_timeout_seconds).map_err(invalid)?,
            keepalive: Keepalive::new(self.keep_alive_seconds).map_err(invalid)?,
            terminal_type: TerminalType::parse(&self.terminal_type).map_err(invalid)?,
            notes: match self.notes.as_deref() {
                Some(n) if !n.is_empty() => Some(Notes::parse(n).map_err(invalid)?),
                _ => None,
            },
        };
        settings.check_auth_shape().map_err(invalid)?;
        if auth == AuthMethod::PrivateKey && self.password.as_deref().is_some_and(|p| !p.is_empty())
        {
            return Err(CommandError::new(
                ErrorCode::InvalidInput,
                "a password cannot be saved for key authentication",
            ));
        }
        if auth == AuthMethod::Password
            && self
                .key_passphrase
                .as_deref()
                .is_some_and(|p| !p.is_empty())
        {
            return Err(CommandError::new(
                ErrorCode::InvalidInput,
                "a key passphrase cannot be saved for password authentication",
            ));
        }
        Ok(settings)
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.is_empty())
}

fn bounded_dim(value: u32) -> Result<u32, CommandError> {
    if (1..=MAX_TERMINAL_DIM).contains(&value) {
        Ok(value)
    } else {
        Err(CommandError::new(
            ErrorCode::InvalidInput,
            "terminal size is out of range",
        ))
    }
}

fn parse_id<T: std::str::FromStr>(raw: &str, what: &str) -> Result<T, CommandError> {
    raw.parse::<T>()
        .map_err(|_| CommandError::new(ErrorCode::InvalidInput, format!("invalid {what} id")))
}

fn invalid(err: impl SelaiError + std::fmt::Display) -> CommandError {
    CommandError::new(err.code(), err.to_string())
}

fn storage_error(err: StorageError) -> CommandError {
    let code = err.code();
    let message = match err {
        StorageError::Conflict(msg) => msg,
        StorageError::NotFound(_) => "not found".to_owned(),
        other => other.to_string(),
    };
    CommandError::new(code, message)
}

fn secret_error(err: selai_secrets::SecretError) -> CommandError {
    CommandError::new(err.code(), err.to_string())
}

fn poisoned() -> CommandError {
    CommandError::new(ErrorCode::Internal, "internal state is unavailable")
}

pub(crate) fn ssh_error(err: &SshError) -> CommandError {
    let mut out = CommandError::new(err.code(), err.to_string());
    match err {
        SshError::HostKeyUnknown(observed) => {
            out.details = Some(KeyErrorDetails {
                algorithm: observed.algorithm.clone(),
                fingerprint_sha256: observed.fingerprint_sha256.clone(),
                pinned_fingerprint_sha256: None,
            });
        }
        SshError::HostKeyChanged {
            observed,
            pinned_fingerprint,
        } => {
            out.details = Some(KeyErrorDetails {
                algorithm: observed.algorithm.clone(),
                fingerprint_sha256: observed.fingerprint_sha256.clone(),
                pinned_fingerprint_sha256: Some(pinned_fingerprint.clone()),
            });
        }
        _ => {}
    }
    out
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}
