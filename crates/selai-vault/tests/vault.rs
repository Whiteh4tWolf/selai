#![allow(clippy::unwrap_used)]
use selai_storage::Database;
use selai_vault::Vault;

const PHRASE: &str = "forest river orange telescope";
const NEW_PHRASE: &str = "mountain ocean violet satellite";
const MARKER: &[u8] = b"selai-synthetic-secret-never-plaintext";

#[test]
fn protects_database_credentials_and_wal_and_rejects_wrong_keys() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, _) = Vault::create(dir.path(), PHRASE).unwrap();
    let db = Database::open_encrypted(&vault.database_path(), vault.key()).unwrap();
    db.put_vault_secret("synthetic", MARKER, false).unwrap();
    let wal = std::fs::read(format!("{}-wal", vault.database_path().display())).unwrap();
    assert!(!wal.windows(MARKER.len()).any(|bytes| bytes == MARKER));
    db.checkpoint().unwrap();
    let bytes = std::fs::read(vault.database_path()).unwrap();
    assert!(!bytes.starts_with(b"SQLite format 3"));
    assert!(!bytes.windows(MARKER.len()).any(|bytes| bytes == MARKER));
    vault.commit().unwrap();
    drop(db);
    drop(vault);
    assert!(Vault::unlock(dir.path(), "incorrect passphrase").is_err());
    let vault = Vault::unlock(dir.path(), PHRASE).unwrap();
    assert!(Database::open_encrypted(&vault.database_path(), &[7; 32]).is_err());
    let mut db = Database::open_encrypted(&vault.database_path(), vault.key()).unwrap();
    assert_eq!(db.vault_secret("synthetic").unwrap().unwrap(), MARKER);
    db.seal().unwrap();
    assert!(db.vault_secret("synthetic").is_err());
}

#[test]
fn rewraps_passphrase_recovers_rotates_recovery_and_restores_portable_backup() {
    let dir = tempfile::tempdir().unwrap();
    let (mut vault, recovery) = Vault::create(dir.path(), PHRASE).unwrap();
    let db = Database::open_encrypted(&vault.database_path(), vault.key()).unwrap();
    db.put_vault_secret("synthetic", MARKER, false).unwrap();
    vault.commit().unwrap();
    vault.change_passphrase(PHRASE, NEW_PHRASE).unwrap();
    assert!(Vault::unlock(dir.path(), PHRASE).is_err());
    assert!(Vault::unlock(dir.path(), NEW_PHRASE).is_ok());
    let backup = dir.path().join("backup.selai");
    vault.backup(&db, &backup).unwrap();
    let restore_dir = tempfile::tempdir().unwrap();
    assert!(Vault::restore(restore_dir.path(), &backup, PHRASE).is_err());
    let restored = Vault::restore(restore_dir.path(), &backup, NEW_PHRASE).unwrap();
    restored.commit().unwrap();
    let restored_db = Database::open_encrypted(&restored.database_path(), restored.key()).unwrap();
    assert_eq!(
        restored_db.vault_secret("synthetic").unwrap().unwrap(),
        MARKER
    );
    let recovery_dir = tempfile::tempdir().unwrap();
    let (recovery_restore, rotated_code) =
        Vault::restore_with_recovery(recovery_dir.path(), &backup, &recovery, PHRASE).unwrap();
    recovery_restore.commit().unwrap();
    assert_ne!(*rotated_code, *recovery);
    assert!(Vault::unlock(recovery_dir.path(), PHRASE).is_ok());
    let recovered_db =
        Database::open_encrypted(&recovery_restore.database_path(), recovery_restore.key())
            .unwrap();
    assert_eq!(
        recovered_db.vault_secret("synthetic").unwrap().unwrap(),
        MARKER
    );
    let (recovered, new_recovery) = Vault::recover(dir.path(), &recovery, PHRASE).unwrap();
    recovered.commit().unwrap();
    assert_ne!(*new_recovery, *recovery);
    assert!(Vault::recover(dir.path(), &recovery, NEW_PHRASE).is_err());
    assert!(Vault::unlock(dir.path(), PHRASE).is_ok());
}

#[test]
fn migrates_plaintext_to_ciphertext_without_putting_credentials_in_source() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("legacy.sqlite3");
    let mut plain = Database::open(&legacy).unwrap();
    plain
        .set_setting(
            "legacy-note",
            &serde_json::json!("example server metadata"),
            1,
        )
        .unwrap();
    let (vault, _) = Vault::create(dir.path(), PHRASE).unwrap();
    plain
        .export_encrypted(&vault.database_path(), vault.key())
        .unwrap();
    let encrypted = Database::open_encrypted(&vault.database_path(), vault.key()).unwrap();
    assert_eq!(
        encrypted.get_setting("legacy-note").unwrap(),
        Some(serde_json::json!("example server metadata"))
    );
    encrypted
        .put_vault_secret("synthetic", MARKER, true)
        .unwrap();
    encrypted.integrity_check().unwrap();
    assert!(plain.vault_secret("synthetic").unwrap().is_none());
}

#[test]
fn encrypted_history_rejects_wrong_passphrase_and_tampering() {
    let dir = tempfile::tempdir().unwrap();
    let (mut vault, recovery) = Vault::create(dir.path(), PHRASE).unwrap();
    let document = vault.seal_document(MARKER).unwrap();
    assert!(!document.windows(MARKER.len()).any(|v| v == MARKER));
    assert_eq!(&*Vault::open_document(&document, PHRASE).unwrap(), MARKER);
    assert!(Vault::open_document(&document, "wrong passphrase").is_err());
    let mut json: serde_json::Value = serde_json::from_slice(&document).unwrap();
    let sealed = json["sealed"]["ciphertext"].as_str().unwrap();
    let mut damaged = sealed.as_bytes().to_vec();
    damaged[0] = if damaged[0] == b'A' { b'B' } else { b'A' };
    json["sealed"]["ciphertext"] = serde_json::json!(String::from_utf8(damaged).unwrap());
    assert!(Vault::open_document(&serde_json::to_vec(&json).unwrap(), PHRASE).is_err());
    vault.commit().unwrap();
    let new_code = vault.rotate_recovery(PHRASE).unwrap();
    assert_ne!(*recovery, *new_code);
    assert!(Vault::recover(dir.path(), &recovery, NEW_PHRASE).is_err());
    assert!(Vault::recover(dir.path(), &new_code, NEW_PHRASE).is_ok());
    let mut header: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("vault.json")).unwrap()).unwrap();
    header["database"] = serde_json::json!("../outside.sqlite3");
    std::fs::write(
        dir.path().join("vault.json"),
        serde_json::to_vec(&header).unwrap(),
    )
    .unwrap();
    assert!(Vault::unlock(dir.path(), PHRASE).is_err());
}
