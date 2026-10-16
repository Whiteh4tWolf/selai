#![allow(clippy::expect_used, clippy::unwrap_used)] // tests may panic on setup failure
//! Contract tests run against the in-memory store. The OS backend is covered by
//! `keyring_backend.rs`, which needs a live credential store.

use selai_domain::{HostId, ProviderId};
use selai_secrets::{MemoryStore, SecretError, SecretKey, SecretStore, SecretValue, SshSecretKind};

fn host_key() -> SecretKey {
    SecretKey::ssh(HostId::new(), SshSecretKind::Password)
}

#[test]
fn put_get_replace_delete_roundtrip() {
    let store = MemoryStore::new();
    let key = host_key();
    assert!(store.get(&key).expect("get").is_none());

    store.put(&key, &SecretValue::new("first")).expect("put");
    assert_eq!(
        store
            .get(&key)
            .expect("get")
            .expect("some")
            .expose_for_transport(),
        b"first"
    );

    store
        .put(&key, &SecretValue::new("second"))
        .expect("replace");
    assert_eq!(
        store
            .get(&key)
            .expect("get")
            .expect("some")
            .expose_for_transport(),
        b"second"
    );

    store.delete(&key).expect("delete");
    assert!(store.get(&key).expect("get").is_none());
    store
        .delete(&key)
        .expect("deleting a missing entry succeeds");
}

#[test]
fn keys_are_scoped_per_host_and_kind() {
    let store = MemoryStore::new();
    let host = HostId::new();
    let other = HostId::new();
    store
        .put(
            &SecretKey::ssh(host, SshSecretKind::Password),
            &SecretValue::new("pw"),
        )
        .expect("put");
    assert!(store
        .get(&SecretKey::ssh(host, SshSecretKind::KeyPassphrase))
        .expect("get")
        .is_none());
    assert!(store
        .get(&SecretKey::ssh(other, SshSecretKind::Password))
        .expect("get")
        .is_none());
}

#[test]
fn account_strings_are_fixed_format() {
    let host: HostId = "6f1c2c1e-0b8e-4c2a-9a55-3d2f1a7b9c10"
        .parse()
        .expect("uuid");
    assert_eq!(
        SecretKey::ssh(host, SshSecretKind::Password).account(),
        "ssh/6f1c2c1e-0b8e-4c2a-9a55-3d2f1a7b9c10/password"
    );
    let provider: ProviderId = "6f1c2c1e-0b8e-4c2a-9a55-3d2f1a7b9c11"
        .parse()
        .expect("uuid");
    assert_eq!(
        SecretKey::provider_api_key(provider).account(),
        "provider/6f1c2c1e-0b8e-4c2a-9a55-3d2f1a7b9c11/api_key"
    );
}

#[test]
fn locked_store_fails_closed_for_every_operation() {
    let store = MemoryStore::new();
    let key = host_key();
    store.put(&key, &SecretValue::new("pw")).expect("put");
    store.fail_all_with(SecretError::Locked);

    assert!(matches!(store.get(&key), Err(SecretError::Locked)));
    assert!(matches!(
        store.put(&key, &SecretValue::new("x")),
        Err(SecretError::Locked)
    ));
    assert!(matches!(store.delete(&key), Err(SecretError::Locked)));

    store.clear_failure();
    assert_eq!(
        store
            .get(&key)
            .expect("get")
            .expect("kept")
            .expose_for_transport(),
        b"pw"
    );
}
