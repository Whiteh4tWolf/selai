#![allow(clippy::expect_used, clippy::unwrap_used)] // tests may panic on setup failure
//! Round trip against the real OS credential store.
//!
//! On Linux this needs a running Secret Service (see `.github/workflows/ci.yml`).
//! The test fails rather than skipping when the store is unavailable, so a
//! green run always means the backend was exercised.

use selai_domain::HostId;
use selai_secrets::{KeyringStore, SecretKey, SecretStore, SecretValue, SshSecretKind};

#[test]
fn os_store_roundtrip_and_cleanup() {
    let store = KeyringStore::new();
    let key = SecretKey::ssh(HostId::new(), SshSecretKind::Password);
    let sentinel = "sentinel-keyring-roundtrip";

    store
        .put(&key, &SecretValue::new(sentinel))
        .expect("store must accept the value");
    let read = store
        .get(&key)
        .expect("store must answer")
        .expect("value must be present after put");
    assert_eq!(read.expose_for_transport(), sentinel.as_bytes());

    store
        .put(&key, &SecretValue::new("replaced"))
        .expect("replace");
    let replaced = store.get(&key).expect("get").expect("present");
    assert_eq!(replaced.expose_for_transport(), b"replaced");

    store.delete(&key).expect("delete");
    assert!(store.get(&key).expect("get after delete").is_none());
}
