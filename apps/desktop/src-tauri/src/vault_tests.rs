//! Synthetic data only: never opens the user's app directory or OS keyring.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::dto::HostInput;
use selai_secrets::{MemoryStore, SshSecretKind};
const PHRASE: &str = "forest river orange telescope";
fn phrase() -> Zeroizing<String> {
    Zeroizing::new(PHRASE.into())
}
fn host() -> HostInput {
    HostInput {
        id: None,
        expected_updated_at_ms: None,
        label: "synthetic-server".into(),
        group_id: None,
        hostname: "example.test".into(),
        port: 22,
        username: "test".into(),
        auth_method: "password".into(),
        private_key_path: None,
        connect_timeout_seconds: 10,
        keep_alive_seconds: 30,
        terminal_type: "xterm-256color".into(),
        notes: Some("private notes marker".into()),
        password: Some("synthetic-password-marker".into()),
        key_passphrase: None,
    }
}
async fn finish_lock(manager: &Arc<VaultManager>, force: bool) {
    let job = tokio::spawn(manager.clone().lock(force));
    loop {
        let status = manager.status().unwrap();
        if status.flush_required {
            manager.flush_complete().unwrap();
        }
        if status.phase == "locked" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    job.await.unwrap().unwrap();
}

#[tokio::test]
async fn migrates_all_data_then_closes_database_and_denies_locked_access() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = Arc::new(MemoryStore::new());
    let old = Services::open(&dir.path().join("selai.sqlite3"), legacy.clone()).unwrap();
    let host = old.host_save(host()).unwrap();
    old.db
        .lock()
        .unwrap()
        .set_setting("test-chat", &serde_json::json!("private chat marker"), 1)
        .unwrap();
    std::fs::create_dir(dir.path().join("exports")).unwrap();
    std::fs::write(
        dir.path().join("exports/old.json"),
        b"private export marker",
    )
    .unwrap();
    drop(old);
    let manager = VaultManager::new(dir.path().into()).unwrap();
    assert!(
        VaultManager::new(dir.path().into()).is_err(),
        "reject second process owner"
    );
    assert!(manager.services().is_err());
    let code = manager.initialize(phrase(), legacy.as_ref()).unwrap();
    assert!(!code.is_empty());
    assert!(!dir.path().join("selai.sqlite3").exists());
    assert!(!dir.path().join("exports/old.json").exists());
    assert!(legacy.is_empty());
    let lease = manager.services().unwrap();
    let retained_db = lease.db.clone();
    assert_eq!(lease.host_get(&host.id).unwrap().label, "synthetic-server");
    let id = host.id.parse().unwrap();
    assert_eq!(
        lease
            .secrets
            .get(&SecretKey::ssh(id, SshSecretKind::Password))
            .unwrap()
            .unwrap()
            .expose_for_transport(),
        b"synthetic-password-marker"
    );
    let doc = lease
        .history_export(
            &host.id,
            &crate::history_service::HistoryFilterInput::default(),
        )
        .unwrap();
    let bytes = std::fs::read(&doc.path).unwrap();
    assert!(doc.path.ends_with(".selai-history"));
    assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-server"));
    assert!(Vault::open_document(&bytes, PHRASE)
        .unwrap()
        .windows(b"synthetic-server".len())
        .any(|v| v == b"synthetic-server"));
    drop(lease);
    finish_lock(&manager, false).await;
    assert!(manager.services().is_err());
    assert!(manager.snapshot_services().is_err());
    assert!(
        retained_db
            .lock()
            .unwrap()
            .vault_secret("anything")
            .is_err(),
        "retained Arc has no decrypted DB"
    );
    assert!(manager
        .unlock(Zeroizing::new("incorrect phrase".into()))
        .is_err());
    manager.inner.lock().unwrap().retry_at = Instant::now();
    manager.unlock(phrase()).unwrap();
    assert_eq!(
        manager
            .services()
            .unwrap()
            .db
            .lock()
            .unwrap()
            .get_setting("test-chat")
            .unwrap(),
        Some(serde_json::json!("private chat marker"))
    );
    finish_lock(&manager, true).await;
}

#[tokio::test]
async fn lock_waits_for_inflight_operations_blocks_admission_and_accepts_final_chat_save() {
    let dir = tempfile::tempdir().unwrap();
    let manager = VaultManager::new(dir.path().into()).unwrap();
    manager.initialize(phrase(), &MemoryStore::new()).unwrap();
    let lease = manager.services().unwrap();
    let job = tokio::spawn(manager.clone().lock(true));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(manager.status().unwrap().phase, "locking");
    assert!(manager.services().is_err());
    assert!(!job.is_finished());
    let snapshot = manager.snapshot_services().unwrap();
    snapshot
        .db
        .lock()
        .unwrap()
        .set_setting("final-snapshot", &serde_json::json!("saved"), 1)
        .unwrap();
    drop(snapshot);
    drop(lease);
    loop {
        if manager.status().unwrap().flush_required {
            manager.flush_complete().unwrap();
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    job.await.unwrap().unwrap();
    assert_eq!(manager.status().unwrap().phase, "locked");
}

#[tokio::test]
async fn lock_cancels_network_rpcs_instead_of_waiting_for_their_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let manager = VaultManager::new(dir.path().into()).unwrap();
    manager.initialize(phrase(), &MemoryStore::new()).unwrap();
    let lease = manager.services().unwrap();
    let operation = tokio::spawn(async move {
        lease
            .run(std::future::pending::<Result<(), CommandError>>())
            .await
    });
    finish_lock(&manager, true).await;
    assert!(operation.await.unwrap().is_err());
    assert_eq!(manager.status().unwrap().phase, "locked");
}

#[cfg(unix)]
#[tokio::test]
async fn lock_closes_a_live_local_terminal_before_releasing_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let manager = VaultManager::new(dir.path().into()).unwrap();
    manager.initialize(phrase(), &MemoryStore::new()).unwrap();
    let lease = manager.services().unwrap();
    let host = lease.local_host().unwrap();
    lease
        .terminal_connect(&host.id, 80, 24, Arc::new(|_| {}))
        .await
        .unwrap();
    assert_eq!(lease.terminal_count(), 1);
    drop(lease);
    tokio::time::timeout(Duration::from_secs(10), finish_lock(&manager, true))
        .await
        .unwrap();
    assert_eq!(manager.status().unwrap().terminals, 0);
    assert_eq!(manager.status().unwrap().phase, "locked");
}

#[test]
fn migration_failure_preserves_legacy_database_and_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = Arc::new(MemoryStore::new());
    let old = Services::open(&dir.path().join("selai.sqlite3"), legacy.clone()).unwrap();
    old.host_save(host()).unwrap();
    drop(old);
    legacy.fail_all_with(SecretError::Locked);
    let manager = VaultManager::new(dir.path().into()).unwrap();
    assert!(manager.initialize(phrase(), legacy.as_ref()).is_err());
    assert!(dir.path().join("selai.sqlite3").exists());
    assert!(!dir.path().join(selai_vault::MANIFEST).exists());
    assert_eq!(legacy.len(), 1);
    assert_eq!(manager.status().unwrap().phase, "setup");
}

#[test]
fn idle_is_based_on_user_activity_and_timeout_is_encrypted() {
    let dir = tempfile::tempdir().unwrap();
    let manager = VaultManager::new(dir.path().into()).unwrap();
    manager.initialize(phrase(), &MemoryStore::new()).unwrap();
    manager.timeout(5).unwrap();
    manager.inner.lock().unwrap().activity = Instant::now() - Duration::from_secs(301);
    assert!(manager.idle());
    assert!(manager.services().is_err());
    assert!(
        manager.activity().is_err(),
        "late activity cannot revive expired access"
    );
}

#[test]
fn pending_credential_cleanup_survives_forgetting_and_changed_source_files_are_preserved() {
    struct RefusesDelete(MemoryStore);
    impl SecretStore for RefusesDelete {
        fn get(&self, key: &SecretKey) -> Result<Option<SecretValue>, SecretError> {
            self.0.get(key)
        }
        fn put(&self, key: &SecretKey, value: &SecretValue) -> Result<(), SecretError> {
            self.0.put(key, value)
        }
        fn delete(&self, _: &SecretKey) -> Result<(), SecretError> {
            Err(SecretError::Locked)
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let legacy = Arc::new(RefusesDelete(MemoryStore::new()));
    let old = Services::open(&dir.path().join("selai.sqlite3"), legacy.clone()).unwrap();
    let saved = old.host_save(host()).unwrap();
    drop(old);
    let manager = VaultManager::new(dir.path().into()).unwrap();
    manager.initialize(phrase(), legacy.as_ref()).unwrap();
    assert!(manager.status().unwrap().cleanup_pending);
    let lease = manager.services().unwrap();
    let key = SecretKey::ssh(saved.id.parse().unwrap(), SshSecretKind::Password);
    lease.secrets.delete(&key).unwrap();
    assert!(lease.secrets.get(&key).unwrap().is_none());
    assert_eq!(
        lease
            .db
            .lock()
            .unwrap()
            .pending_legacy_credentials()
            .unwrap(),
        vec![key.account().to_owned()]
    );
    std::fs::write(dir.path().join("selai.sqlite3"), b"changed after migration").unwrap();
    assert!(!cleanup(dir.path(), &lease, &legacy.0));
    assert!(legacy.0.is_empty());
    assert_eq!(
        std::fs::read(dir.path().join("selai.sqlite3")).unwrap(),
        b"changed after migration"
    );
    assert!(lease
        .db
        .lock()
        .unwrap()
        .pending_legacy_credentials()
        .unwrap()
        .is_empty());
}

#[test]
fn private_key_bytes_are_managed_and_backup_restores_to_fresh_directory() {
    let dir = tempfile::tempdir().unwrap();
    let manager = VaultManager::new(dir.path().into()).unwrap();
    manager.initialize(phrase(), &MemoryStore::new()).unwrap();
    let source = tempfile::tempdir().unwrap();
    let key_path = source.path().join("id_ed25519");
    std::fs::write(&key_path, b"synthetic private key bytes").unwrap();
    let lease = manager.services().unwrap();
    let mut input = host();
    input.auth_method = "private_key".into();
    input.password = None;
    input.private_key_path = Some(key_path.display().to_string());
    let saved = lease.host_save(input).unwrap();
    assert_eq!(
        saved.private_key_path,
        Some(format!("vault://{}", saved.id))
    );
    assert!(key_path.exists());
    assert_eq!(
        lease
            .secrets
            .get(&SecretKey::ssh(
                saved.id.parse().unwrap(),
                SshSecretKind::PrivateKey
            ))
            .unwrap()
            .unwrap()
            .expose_for_transport(),
        b"synthetic private key bytes"
    );
    drop(lease);
    let backup = source.path().join("portable.selai");
    manager.backup(phrase(), &backup).unwrap();
    assert!(
        manager.restore(phrase(), &backup).is_err(),
        "never overwrite an existing vault"
    );
    let destination = tempfile::tempdir().unwrap();
    let restored = VaultManager::new(destination.path().into()).unwrap();
    restored.restore(phrase(), &backup).unwrap();
    assert_eq!(
        restored
            .services()
            .unwrap()
            .host_get(&saved.id)
            .unwrap()
            .private_key_path,
        saved.private_key_path
    );
}
