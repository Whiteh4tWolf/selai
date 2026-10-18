//! End-to-end behaviour of the application service against a real OpenSSH
//! server (tests T-SSH-001..005, T-SSH-006 in part, T-MENTION-002 for identity).
//!
//! The secret store is in memory here. The OS backend is covered by
//! `selai-secrets/tests/keyring_backend.rs`.

#![cfg(target_os = "linux")] // Needs OpenSSH sshd and root; see docs/release/milestones.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{sync::Arc, time::Duration};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use selai_desktop_lib::{
    dto::{CommandError, HostInput, TerminalEvent, TrustMode},
    service::{EventSink, Services},
};
use selai_domain::ErrorCode;
use selai_secrets::{MemoryStore, SecretError};
use selai_storage::{Database, PinnedKey};
use selai_testkit::{material, Sshd, USER};
use tokio::sync::mpsc;

const LIMIT: Duration = Duration::from_secs(15);

struct Harness {
    services: Services,
    store: Arc<MemoryStore>,
    _dir: tempfile::TempDir,
    db_path: std::path::PathBuf,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("selai.sqlite3");
    let store = Arc::new(MemoryStore::new());
    let services = Services::open(&db_path, store.clone()).expect("open services");
    Harness {
        services,
        store,
        _dir: dir,
        db_path,
    }
}

fn password_host(label: &str, port: u16, password: &str) -> HostInput {
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
        password: Some(password.to_owned()),
        key_passphrase: None,
    }
}

fn new_sink() -> (EventSink, mpsc::UnboundedReceiver<TerminalEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        Arc::new(move |event| {
            let _ = tx.send(event);
        }),
        rx,
    )
}

/// Collects decoded terminal output until `needle` appears.
async fn expect_output(rx: &mut mpsc::UnboundedReceiver<TerminalEvent>, needle: &str) -> String {
    let mut text = String::new();
    let deadline = tokio::time::Instant::now() + LIMIT;
    loop {
        if text.contains(needle) {
            return text;
        }
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(TerminalEvent::Data { data })) => {
                text.push_str(&String::from_utf8_lossy(&B64.decode(data).expect("base64")));
            }
            Ok(Some(other)) => panic!("unexpected event {other:?}; output so far {text:?}"),
            Ok(None) | Err(_) => panic!("timed out waiting for {needle:?}; output {text:?}"),
        }
    }
}

async fn expect_closed(rx: &mut mpsc::UnboundedReceiver<TerminalEvent>) -> Option<u32> {
    let deadline = tokio::time::Instant::now() + LIMIT;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(TerminalEvent::Closed { exit_status })) => return exit_status,
            Ok(Some(TerminalEvent::Data { .. } | TerminalEvent::Exit { .. })) => continue,
            Ok(Some(other)) => panic!("expected Closed, got {other:?}"),
            Ok(None) | Err(_) => panic!("timed out waiting for Closed"),
        }
    }
}

fn err_code(err: CommandError) -> ErrorCode {
    err.code
}

/// Creates a password host, probes and trusts its real key, and returns its id.
async fn trusted_password_host(h: &Harness, sshd: &Sshd, label: &str) -> String {
    let view = h
        .services
        .host_save(password_host(label, sshd.port, &material().password))
        .expect("save host");
    let probe = h.services.host_probe(&view.id).await.expect("probe");
    assert_eq!(probe.status, "unknown");
    h.services
        .host_trust(
            &view.id,
            &probe.observed.fingerprint_sha256,
            TrustMode::First,
        )
        .expect("trust");
    view.id
}

#[tokio::test]
async fn imported_rsa_keys_are_selectable_and_authenticate_from_managed_storage() {
    let h = harness();
    let m = material();
    for (label, path, passphrase) in [
        ("RSA OpenSSH", &m.rsa_key, None),
        (
            "RSA encrypted PEM",
            &m.encrypted_rsa_key,
            Some(m.encrypted_passphrase.clone()),
        ),
    ] {
        let bytes = std::fs::read(path).expect("fixture key");
        let key = h
            .services
            .ssh_key_import(label.into(), B64.encode(&bytes), passphrase)
            .expect("import RSA");
        assert_eq!(key.algorithm, "ssh-rsa");
        assert!(key.public_key.starts_with("ssh-rsa "));
        assert!(h
            .services
            .ssh_key_list()
            .expect("keys")
            .iter()
            .any(|saved| saved.id == key.id));
        let sshd = Sshd::start();
        let mut input = password_host(label, sshd.port, "");
        input.auth_method = "private_key".into();
        input.password = None;
        input.private_key_path = Some(format!("vault://key:{}", key.id));
        let host = h
            .services
            .host_save(input)
            .expect("save managed key profile");
        let probe = h.services.host_probe(&host.id).await.expect("probe");
        h.services
            .host_trust(
                &host.id,
                &probe.observed.fingerprint_sha256,
                TrustMode::First,
            )
            .expect("trust");
        let (sink, mut rx) = new_sink();
        let session = h
            .services
            .terminal_connect(&host.id, 80, 24, sink)
            .await
            .expect("RSA terminal login");
        h.services
            .terminal_input(&session, "echo managed-rsa-$((6*7))\n".into())
            .await
            .expect("input");
        expect_output(&mut rx, "managed-rsa-42").await;
        h.services
            .terminal_disconnect(&session)
            .await
            .expect("disconnect");
        assert!(sshd
            .auth_decisions()
            .iter()
            .any(|l| l.contains("Accepted publickey")));
    }
}

#[test]
fn encrypted_rsa_import_with_wrong_passphrase_saves_nothing() {
    let h = harness();
    let bytes = std::fs::read(&material().encrypted_rsa_key).expect("fixture key");
    let err = h
        .services
        .ssh_key_import(
            "Invalid RSA".into(),
            B64.encode(bytes),
            Some("wrong-passphrase".into()),
        )
        .expect_err("invalid passphrase");
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(h.services.ssh_key_list().expect("keys").is_empty());
}

#[tokio::test]
async fn first_contact_is_blocked_until_the_user_trusts_the_probed_key() {
    let h = harness();
    let sshd = Sshd::start();
    let m = material();
    let host = h
        .services
        .host_save(password_host("production-web", sshd.port, &m.password))
        .expect("save");

    let (sink, _rx) = new_sink();
    let err = h
        .services
        .terminal_connect(&host.id, 80, 24, sink)
        .await
        .expect_err("unknown key must block");
    assert_eq!(err.code, ErrorCode::SshHostkeyUnknown);
    let details = err.details.expect("fingerprint is shown to the user");
    let probe = h.services.host_probe(&host.id).await.expect("probe");
    assert_eq!(
        details.fingerprint_sha256,
        probe.observed.fingerprint_sha256
    );
    assert!(
        sshd.auth_decisions().is_empty(),
        "no credential may reach the server"
    );

    // A fingerprint that was never probed cannot be pinned.
    let forged = h
        .services
        .host_trust(&host.id, "SHA256:forged", TrustMode::First);
    assert_eq!(forged.expect_err("forged").code, ErrorCode::InvalidInput);

    let trusted = h
        .services
        .host_trust(
            &host.id,
            &probe.observed.fingerprint_sha256,
            TrustMode::First,
        )
        .expect("trust");
    assert_eq!(trusted.outcome, "pinned");

    let (sink, mut rx) = new_sink();
    let session = h
        .services
        .terminal_connect(&host.id, 80, 24, sink)
        .await
        .expect("connect");
    assert_eq!(
        h.services.terminal_host(&session),
        Some(host.id.parse().expect("id"))
    );
    h.services
        .terminal_input(&session, "echo selai-$((6*7))\n".into())
        .await
        .expect("input");
    expect_output(&mut rx, "selai-42").await;
    h.services
        .terminal_input(&session, "exit 0\n".into())
        .await
        .expect("exit");
    assert_eq!(expect_closed(&mut rx).await, Some(0));
    assert!(sshd
        .auth_decisions()
        .iter()
        .any(|l| l.contains("Accepted password")));
}

#[tokio::test]
async fn the_key_shown_by_a_refused_connect_can_be_trusted_without_a_separate_probe() {
    let h = harness();
    let sshd = Sshd::start();
    let host = h
        .services
        .host_save(password_host(
            "Production Web",
            sshd.port,
            &material().password,
        ))
        .expect("save");

    let (sink, _rx) = new_sink();
    let err = h
        .services
        .terminal_connect(&host.id, 80, 24, sink)
        .await
        .expect_err("unknown key must block");
    assert_eq!(err.code, ErrorCode::SshHostkeyUnknown);
    let details = err.details.expect("fingerprint is shown to the user");
    assert!(sshd.auth_decisions().is_empty());

    // Only the exact key the refused connect observed can be pinned.
    let other = h
        .services
        .host_trust(&host.id, "SHA256:not-what-was-shown", TrustMode::First);
    assert_eq!(other.expect_err("mismatch").code, ErrorCode::InvalidInput);

    let trusted = h
        .services
        .host_trust(&host.id, &details.fingerprint_sha256, TrustMode::First)
        .expect("trust the key from the connect error");
    assert_eq!(trusted.outcome, "pinned");

    let (sink, mut rx) = new_sink();
    let session = h
        .services
        .terminal_connect(&host.id, 80, 24, sink)
        .await
        .expect("connect after approval");
    h.services
        .terminal_input(&session, "echo approved-$((6*7))\n".into())
        .await
        .expect("input");
    expect_output(&mut rx, "approved-42").await;
    h.services
        .terminal_disconnect(&session)
        .await
        .expect("disconnect");
}

#[tokio::test]
async fn saved_secret_is_not_in_ordinary_host_responses_or_the_legacy_database() {
    let h = harness();
    let sshd = Sshd::start();
    let sentinel = "sentinel-password-never-leak";
    let view = h
        .services
        .host_save(password_host("web", sshd.port, sentinel))
        .expect("save");
    let serialized = serde_json::to_string(&view).expect("json");
    assert!(!serialized.contains(sentinel));
    assert!(view.credential_configured);
    assert_eq!(h.store.len(), 1, "password is in the OS store");

    let listed = serde_json::to_string(&h.services.host_list(None, false, None).expect("list"))
        .expect("json");
    assert!(!listed.contains(sentinel));
    let raw = std::fs::read(&h.db_path).expect("db bytes");
    let wal = std::fs::read(h.db_path.with_extension("sqlite3-wal")).unwrap_or_default();
    assert!(
        !contains(&raw, sentinel.as_bytes()) && !contains(&wal, sentinel.as_bytes()),
        "secret found in database files"
    );
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[tokio::test]
async fn locked_store_fails_closed_and_writes_nothing() {
    let h = harness();
    let sshd = Sshd::start();
    h.store.fail_all_with(SecretError::Locked);
    let err = h
        .services
        .host_save(password_host("web", sshd.port, "pw"))
        .expect_err("locked store");
    assert_eq!(err.code, ErrorCode::SecretStoreLocked);
    assert!(
        h.services
            .host_list(None, false, None)
            .expect("list")
            .items
            .is_empty(),
        "no host row without its secret"
    );

    h.store.clear_failure();
    h.services
        .host_save(password_host("web", sshd.port, "pw"))
        .expect("works after unlock");
}

#[tokio::test]
async fn label_conflicts_are_reported_with_a_stable_code() {
    let h = harness();
    let sshd = Sshd::start();
    h.services
        .host_save(password_host("production-web", sshd.port, "pw"))
        .expect("first");
    let err = h
        .services
        .host_save(password_host("PRODUCTION-WEB", sshd.port, "pw"))
        .expect_err("case variant");
    assert_eq!(err.code, ErrorCode::StorageConflict);
}

#[tokio::test]
async fn changed_key_is_refused_and_replacement_is_explicit() {
    let h = harness();
    let sshd = Sshd::start();
    let m = material();
    let id = trusted_password_host(&h, &sshd, "web").await;

    // Simulate an operator-visible key change by pinning a different key.
    {
        let mut db = Database::open(&h.db_path).expect("second connection");
        let other = selai_ssh::PinnedHostKey::from_openssh_line(&m.other_host_pub).expect("other");
        db.rotate_host_key(
            id.parse().expect("id"),
            &PinnedKey {
                algorithm: other.algorithm,
                public_key_base64: other.public_key_base64,
                fingerprint_sha256: other.fingerprint_sha256,
                first_seen_at_ms: 1,
            },
            2,
        )
        .expect("rotate to other");
    }

    let (sink, _rx) = new_sink();
    let err = h
        .services
        .terminal_connect(&id, 80, 24, sink)
        .await
        .expect_err("changed key");
    assert_eq!(err.code, ErrorCode::SshHostkeyChanged);
    let details = err.details.expect("details");
    assert_eq!(details.fingerprint_sha256, material_host_fp());
    assert!(details.pinned_fingerprint_sha256.is_some());

    let probe = h.services.host_probe(&id).await.expect("probe");
    assert_eq!(probe.status, "changed");
    assert_eq!(
        h.services
            .host_trust(&id, &probe.observed.fingerprint_sha256, TrustMode::First)
            .expect_err("first mode must not overwrite")
            .code,
        ErrorCode::StorageConflict
    );
    let replaced = h
        .services
        .host_trust(&id, &probe.observed.fingerprint_sha256, TrustMode::Replace)
        .expect("explicit replace");
    assert_eq!(replaced.outcome, "rotated");

    let (sink, mut rx) = new_sink();
    let session = h
        .services
        .terminal_connect(&id, 80, 24, sink)
        .await
        .expect("connects after replace");
    h.services
        .terminal_input(&session, "exit 0\n".into())
        .await
        .expect("exit");
    expect_closed(&mut rx).await;
}

fn material_host_fp() -> String {
    selai_ssh::PinnedHostKey::from_openssh_line(&material().host_pub)
        .expect("host")
        .fingerprint_sha256
}

#[tokio::test]
async fn archive_removes_secrets_keeps_history_and_frees_the_label() {
    let h = harness();
    let sshd = Sshd::start();
    let id = trusted_password_host(&h, &sshd, "old-web").await;
    assert_eq!(h.store.len(), 1);

    h.services.host_archive(&id).expect("archive");
    assert!(h.store.is_empty(), "secrets are removed on archive");
    let archived = h.services.host_get(&id).expect("history kept");
    assert!(archived.archived);
    assert!(!archived.credential_configured);

    let (sink, _rx) = new_sink();
    let err = h
        .services
        .terminal_connect(&id, 80, 24, sink)
        .await
        .expect_err("archived host");
    assert_eq!(err.code, ErrorCode::TaskScopeInvalid);

    h.services
        .host_save(password_host("old-web", sshd.port, "pw"))
        .expect("label is free again");
}

#[tokio::test]
async fn five_terminals_stay_bound_to_their_own_hosts() {
    let h = harness();
    let sshd = Sshd::start();
    let mut ids = Vec::new();
    for n in 0..5 {
        ids.push(trusted_password_host(&h, &sshd, &format!("node-{n}")).await);
    }

    let mut sessions = Vec::new();
    for (n, id) in ids.iter().enumerate() {
        let (sink, rx) = new_sink();
        let session = h
            .services
            .terminal_connect(id, 80, 24, sink)
            .await
            .expect("connect");
        assert_eq!(
            h.services.terminal_host(&session),
            Some(id.parse().expect("id")),
            "session {n} bound to its host"
        );
        sessions.push((session, rx));
    }

    for (n, (session, rx)) in sessions.iter_mut().enumerate() {
        h.services
            .terminal_input(
                session,
                format!("echo marker-{n}-$((n+1))\n").replace("$((n+1))", &(n + 1).to_string()),
            )
            .await
            .expect("input");
        expect_output(rx, &format!("marker-{n}-{}", n + 1)).await;
    }

    for (session, rx) in sessions.iter_mut() {
        h.services
            .terminal_input(session, "exit 0\n".into())
            .await
            .expect("exit");
        expect_closed(rx).await;
    }
}

#[tokio::test]
async fn resize_reaches_the_remote_pty() {
    let h = harness();
    let sshd = Sshd::start();
    let id = trusted_password_host(&h, &sshd, "web").await;
    let (sink, mut rx) = new_sink();
    let session = h
        .services
        .terminal_connect(&id, 80, 24, sink)
        .await
        .expect("connect");
    h.services
        .terminal_resize(&session, 120, 40)
        .await
        .expect("resize");
    h.services
        .terminal_input(&session, "stty size\n".into())
        .await
        .expect("input");
    expect_output(&mut rx, "40 120").await;
    h.services
        .terminal_disconnect(&session)
        .await
        .expect("disconnect");
    expect_closed(&mut rx).await;
    let after = h
        .services
        .terminal_input(&session, "echo late\n".into())
        .await;
    assert_eq!(
        err_code(after.expect_err("closed session")),
        ErrorCode::NotFound
    );
}

#[tokio::test]
async fn private_key_host_uses_the_stored_passphrase() {
    let h = harness();
    let sshd = Sshd::start();
    let m = material();
    let mut input = password_host("keyed", sshd.port, "");
    input.password = None;
    input.auth_method = "private_key".to_owned();
    input.private_key_path = Some(m.encrypted_key.display().to_string());
    input.key_passphrase = Some(m.encrypted_passphrase.clone());
    let view = h.services.host_save(input).expect("save key host");
    assert!(view.credential_configured);
    assert_eq!(
        h.store.len(),
        1,
        "passphrase is in the OS store, not the database"
    );

    let probe = h.services.host_probe(&view.id).await.expect("probe");
    h.services
        .host_trust(
            &view.id,
            &probe.observed.fingerprint_sha256,
            TrustMode::First,
        )
        .expect("trust");
    let (sink, mut rx) = new_sink();
    let session = h
        .services
        .terminal_connect(&view.id, 80, 24, sink)
        .await
        .expect("key login");
    h.services
        .terminal_input(&session, "whoami\n".into())
        .await
        .expect("input");
    expect_output(&mut rx, USER).await;
    h.services
        .terminal_input(&session, "exit 0\n".into())
        .await
        .expect("exit");
    expect_closed(&mut rx).await;
}

#[tokio::test]
async fn invalid_input_is_rejected_before_anything_is_written() {
    let h = harness();
    let mut bad = password_host("web", 22, "pw");
    bad.hostname = "bad host name".to_owned();
    assert_eq!(
        h.services.host_save(bad).expect_err("hostname").code,
        ErrorCode::InvalidInput
    );
    let mut mixed = password_host("web", 22, "pw");
    mixed.key_passphrase = Some("x".to_owned());
    assert_eq!(
        h.services.host_save(mixed).expect_err("mixed auth").code,
        ErrorCode::InvalidInput
    );
    assert!(h.store.is_empty());
    assert!(h
        .services
        .host_list(None, false, None)
        .expect("list")
        .items
        .is_empty());
}
