//! In-memory profiles only: no real servers, OS credentials, or app data.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use selai_desktop_lib::{dto::HostInput, service::Services};
use selai_domain::ErrorCode;
use selai_secrets::{MemoryStore, SecretError};
use std::sync::Arc;

fn input(label: &str) -> HostInput {
    HostInput {
        id: None,
        expected_updated_at_ms: None,
        label: label.into(),
        group_id: None,
        hostname: "example.test".into(),
        port: 22,
        username: "deploy".into(),
        auth_method: "password".into(),
        private_key_path: None,
        connect_timeout_seconds: 15,
        keep_alive_seconds: 30,
        terminal_type: "xterm-256color".into(),
        notes: None,
        password: Some("demo-only-password".into()),
        key_passphrase: None,
    }
}

#[test]
fn saved_password_is_only_available_through_explicit_reveal_and_store_failures_are_closed() {
    let store = Arc::new(MemoryStore::new());
    let services = Services::in_memory(store.clone()).expect("services");
    let host = services.host_save(input("Web")).expect("host");
    assert!(
        !serde_json::to_string(&services.host_get(&host.id).expect("metadata"))
            .expect("json")
            .contains("demo-only-password")
    );
    assert_eq!(
        services
            .host_reveal_password(&host.id)
            .expect("reveal")
            .as_deref(),
        Some("demo-only-password")
    );
    store.fail_all_with(SecretError::Locked);
    assert_eq!(
        services
            .host_reveal_password(&host.id)
            .expect_err("store locked")
            .code,
        ErrorCode::SecretStoreLocked
    );
    store.clear_failure();
    let local = services.local_host().expect("local");
    assert!(services.host_reveal_password(&local.id).is_err());
    services.host_archive(&host.id).expect("archive");
    assert!(services.host_reveal_password(&host.id).is_err());
}

#[test]
fn deleting_a_group_only_ungroups_profiles_and_keeps_their_credentials() {
    let services = Services::in_memory(Arc::new(MemoryStore::new())).expect("services");
    let group = services.group_create("Production", 0).expect("group");
    let mut profile = input("Web");
    profile.group_id = Some(group.id.clone());
    let host = services.host_save(profile).expect("host");
    services.group_rename(&group.id, "Servers").expect("rename");
    assert_eq!(services.group_list().expect("groups")[0].name, "Servers");
    services.group_delete(&group.id).expect("delete");
    assert!(services
        .host_get(&host.id)
        .expect("host kept")
        .group_id
        .is_none());
    assert_eq!(
        services
            .host_reveal_password(&host.id)
            .expect("credential kept")
            .as_deref(),
        Some("demo-only-password")
    );
    assert!(services.group_create("  ", 0).is_err());
}

#[test]
fn moving_profiles_preserves_credentials_and_rejects_stale_or_invalid_moves() {
    let store = Arc::new(MemoryStore::new());
    let services = Services::in_memory(store.clone()).expect("services");
    let group = services.group_create("Production", 0).expect("group");
    let host = services.host_save(input("Web")).expect("host");
    // Moving metadata must not depend on reading or updating a secret store.
    store.fail_all_with(SecretError::Locked);
    let moved = services
        .host_move_group(&host.id, Some(&group.id), host.updated_at_ms)
        .expect("move");
    assert_eq!(moved.group_id.as_deref(), Some(group.id.as_str()));
    assert!(moved.updated_at_ms > host.updated_at_ms);
    assert_eq!(moved.hostname, host.hostname);
    assert_eq!(moved.username, host.username);
    assert!(moved.credential_configured);
    assert_eq!(
        services
            .host_move_group(&host.id, None, host.updated_at_ms)
            .unwrap_err()
            .code,
        ErrorCode::StorageConflict
    );
    let missing = selai_domain::HostGroupId::new().to_string();
    assert!(services
        .host_move_group(&host.id, Some(&missing), moved.updated_at_ms)
        .is_err());
    assert_eq!(
        services.host_get(&host.id).unwrap().group_id,
        moved.group_id
    );
    let ungrouped = services
        .host_move_group(&host.id, None, moved.updated_at_ms)
        .expect("ungroup");
    assert!(ungrouped.group_id.is_none());
    let local = services.local_host().unwrap();
    assert!(services
        .host_move_group(&local.id, Some(&group.id), local.updated_at_ms)
        .is_err());
    store.clear_failure();
    assert_eq!(
        services.host_reveal_password(&host.id).unwrap().as_deref(),
        Some("demo-only-password")
    );
    services.host_archive(&host.id).unwrap();
    assert!(services
        .host_move_group(&host.id, Some(&group.id), ungrouped.updated_at_ms)
        .is_err());
}
