//! Provider repository behaviour through the public API.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use selai_domain::{
    BaseUrl, MaxOutputTokens, ModelId, PageRequest, ProviderId, ProviderName, ProviderProtocol,
    ProviderSettings, TimeoutSecs,
};
use selai_storage::{Database, StorageError};

fn settings(model: &str, base: &str) -> ProviderSettings {
    ProviderSettings {
        name: ProviderName::parse("gateway").expect("name"),
        protocol: ProviderProtocol::OpenAiChat,
        base_url: BaseUrl::parse(base).expect("base"),
        model_id: ModelId::parse(model).expect("model"),
        timeout: TimeoutSecs::new(60).expect("timeout"),
        max_output_tokens: MaxOutputTokens::new(1024).expect("max"),
    }
}

#[test]
fn created_provider_has_no_key_and_no_verification() {
    let mut db = Database::open_in_memory().expect("db");
    let id = ProviderId::new();
    let rec = db
        .create_provider(id, &settings("m1", "https://gw.example.com/v1"), 1)
        .expect("create");
    assert!(!rec.key_configured);
    assert!(!rec.tool_probe_passed);
    assert_eq!(rec.timeout_seconds, 60);
    assert_eq!(rec.max_output_tokens, 1024);
}

#[test]
fn verification_is_cleared_when_the_model_changes_but_not_when_the_name_does() {
    let mut db = Database::open_in_memory().expect("db");
    let id = ProviderId::new();
    let created = db
        .create_provider(id, &settings("m1", "https://gw.example.com/v1"), 1)
        .expect("create");
    let identity = "openai_chat|https://gw.example.com/v1|m1";
    db.record_tool_probe(id, true, identity, 2).expect("probe");

    let mut renamed = settings("m1", "https://gw.example.com/v1");
    renamed.name = ProviderName::parse("renamed").expect("name");
    let after_rename = db
        .update_provider(id, &renamed, created.updated_at_ms, 3)
        .expect("rename");
    assert!(
        after_rename.tool_probe_passed,
        "a rename does not change the endpoint"
    );

    let changed = db
        .update_provider(
            id,
            &settings("m2", "https://gw.example.com/v1"),
            after_rename.updated_at_ms,
            4,
        )
        .expect("model change");
    assert!(
        !changed.tool_probe_passed,
        "a different model must not inherit the result"
    );
    assert_eq!(changed.tool_probe_identity, None);
}

#[test]
fn stale_update_is_a_conflict() {
    let mut db = Database::open_in_memory().expect("db");
    let id = ProviderId::new();
    let rec = db
        .create_provider(id, &settings("m1", "https://gw.example.com/v1"), 1)
        .expect("create");
    db.update_provider(
        id,
        &settings("m1", "https://gw.example.com/v1"),
        rec.updated_at_ms,
        2,
    )
    .expect("first");
    let err = db.update_provider(
        id,
        &settings("m1", "https://gw.example.com/v1"),
        rec.updated_at_ms,
        3,
    );
    assert!(matches!(err, Err(StorageError::Conflict(_))));
}

#[test]
fn list_is_paged_by_name() {
    let mut db = Database::open_in_memory().expect("db");
    for (i, name) in ["charlie", "alpha", "bravo"].iter().enumerate() {
        let mut s = settings("m", "https://gw.example.com/v1");
        s.name = ProviderName::parse(name).expect("name");
        db.create_provider(ProviderId::new(), &s, i as i64)
            .expect("create");
    }
    let first = db.list_providers(&PageRequest::first(2)).expect("page");
    let names: Vec<_> = first.items.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["alpha", "bravo"]);
    let second = db
        .list_providers(&PageRequest {
            cursor: first.next_cursor,
            limit: 2,
        })
        .expect("page 2");
    assert_eq!(second.items[0].name, "charlie");
    assert!(second.next_cursor.is_none());
}

#[test]
fn delete_removes_the_row_and_unknown_ids_are_not_found() {
    let mut db = Database::open_in_memory().expect("db");
    let id = ProviderId::new();
    db.create_provider(id, &settings("m", "https://gw.example.com/v1"), 1)
        .expect("create");
    db.delete_provider(id).expect("delete");
    assert!(db.get_provider(id).expect("get").is_none());
    assert!(matches!(
        db.delete_provider(id),
        Err(StorageError::NotFound(_))
    ));
}

#[test]
fn database_check_rejects_a_protocol_outside_the_three_supported() {
    let db = Database::open_in_memory().expect("db");
    let err = db.connection_for_provider_test().execute(
        "INSERT INTO provider_profiles (id, name, protocol, base_url, model_id, created_at_ms, updated_at_ms)
         VALUES ('00000000-0000-4000-8000-0000000000aa', 'x', 'gemini', 'https://a', 'm', 1, 1)",
        [],
    );
    assert!(err.is_err());
}
