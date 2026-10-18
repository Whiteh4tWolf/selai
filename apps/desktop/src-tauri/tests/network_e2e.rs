//! SFTP file management and transfers through the production service.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use selai_desktop_lib::{
    network_service::ProxyInput,
    sftp_service::{TransferInput, TransferView},
};
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    sync::Arc,
};

async fn trusted(services: &Services, server: &Sshd) -> String {
    let host = services
        .host_save(password_host("SFTP fixture", server.port))
        .unwrap();
    let probe = services.host_probe(&host.id).await.unwrap();
    services
        .host_trust(
            &host.id,
            &probe.observed.fingerprint_sha256,
            TrustMode::First,
        )
        .unwrap();
    host.id
}
async fn copy(services: &Services, input: TransferInput, cancel_after_data: bool) -> TransferView {
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Production submits this from synchronous IPC, outside an entered Tokio
    // runtime. Calling it directly in #[tokio::test] hid the desktop crash.
    let initial = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                assert!(tokio::runtime::Handle::try_current().is_err());
                services
                    .transfer_start(
                        input,
                        Arc::new(move |event| {
                            let _ = tx.send(event);
                        }),
                    )
                    .unwrap()
            })
            .join()
            .expect("synchronous transfer submission must not panic")
    });
    tokio::time::timeout(LIMIT, async {
        loop {
            let view = rx.recv().await.unwrap();
            if cancel_after_data && view.transferred_bytes > 0 {
                services.transfer_cancel(&initial.id).unwrap();
            }
            if ["completed", "failed", "cancelled"].contains(&view.status.as_str()) {
                return view;
            }
        }
    })
    .await
    .unwrap()
}
fn writable(path: &Path) {
    std::fs::create_dir(path).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o777)).unwrap();
}

#[tokio::test]
async fn sftp_roundtrip_recurses_reports_progress_skips_links_and_preserves_matching_files() {
    let server = Sshd::start();
    let h = harness();
    let host = trusted(&h.services, &server).await;
    let source = h.dir.path().join("payload");
    std::fs::create_dir_all(source.join("nested")).unwrap();
    let bytes = vec![42; 2 * 1024 * 1024];
    std::fs::write(source.join("large.bin"), &bytes).unwrap();
    std::fs::write(source.join("nested/empty.txt"), []).unwrap();
    std::fs::write(source.join("nested/data.txt"), b"native-sftp").unwrap();
    let long_name = format!("{}.bin", "x".repeat(251));
    std::fs::write(source.join(&long_name), b"x").unwrap();
    symlink(source.join("large.bin"), source.join("link")).unwrap();
    let remote = h.dir.path().join("remote");
    writable(&remote);
    let download = h.dir.path().join("download");
    std::fs::create_dir(&download).unwrap();
    let session = h.services.sftp_connect(&host).await.unwrap();
    let upload = TransferInput {
        connection_id: session.id.clone(),
        direction: "upload".into(),
        sources: vec![source.display().to_string()],
        destination: remote.display().to_string(),
        overwrite: false,
    };
    let done = copy(&h.services, upload, false).await;
    assert_eq!(done.status, "completed", "{:?}", done.error);
    assert_eq!(done.completed_files, 4);
    assert_eq!(done.total_files, 4);
    assert_eq!(done.skipped_files, 1);
    assert_eq!(done.transferred_bytes, bytes.len() as u64 + 12);
    let path = remote.join("payload").display().to_string();
    let listed = h
        .services
        .files_list("remote", Some(&session.id), &path)
        .await
        .unwrap();
    assert!(listed
        .entries
        .iter()
        .any(|e| e.name == "nested" && e.kind == "directory"));
    let input = TransferInput {
        connection_id: session.id.clone(),
        direction: "download".into(),
        sources: vec![path.clone()],
        destination: download.display().to_string(),
        overwrite: false,
    };
    let done = copy(&h.services, input, false).await;
    assert_eq!(done.status, "completed", "{:?}", done.error);
    assert_eq!(
        std::fs::read(download.join("payload/large.bin")).unwrap(),
        bytes
    );
    assert_eq!(
        std::fs::read(download.join("payload/nested/data.txt")).unwrap(),
        b"native-sftp"
    );
    std::fs::write(source.join("large.bin"), b"replacement").unwrap();
    let request = |overwrite| TransferInput {
        connection_id: session.id.clone(),
        direction: "upload".into(),
        sources: vec![source.join("large.bin").display().to_string()],
        destination: path.clone(),
        overwrite,
    };
    let skipped = copy(&h.services, request(false), false).await;
    assert_eq!(skipped.status, "completed");
    assert_eq!(skipped.skipped_files, 1);
    assert_eq!(
        std::fs::read(remote.join("payload/large.bin")).unwrap(),
        bytes
    );
    let replaced = copy(&h.services, request(true), false).await;
    assert_eq!(replaced.status, "completed", "{:?}", replaced.error);
    assert_eq!(
        std::fs::read(remote.join("payload/large.bin")).unwrap(),
        b"replacement"
    );
    h.services
        .files_action(
            "remote",
            Some(&session.id),
            "mkdir",
            &path,
            Some("empty-folder"),
        )
        .await
        .unwrap();
    h.services
        .files_action(
            "remote",
            Some(&session.id),
            "rename",
            &format!("{path}/empty-folder"),
            Some("renamed"),
        )
        .await
        .unwrap();
    h.services
        .files_action(
            "remote",
            Some(&session.id),
            "delete",
            &format!("{path}/renamed"),
            None,
        )
        .await
        .unwrap();
    h.services.sftp_disconnect(&session.id).await.unwrap();
    assert!(h.services.sftp_list().unwrap().is_empty());
    assert!(h
        .services
        .files_list("remote", Some(&session.id), &path)
        .await
        .is_err());
}

#[tokio::test]
async fn cancelling_a_running_upload_does_not_publish_a_partial_file() {
    let server = Sshd::start();
    let h = harness();
    let host = trusted(&h.services, &server).await;
    let remote = h.dir.path().join("remote");
    writable(&remote);
    let source = h.dir.path().join("large-sparse.bin");
    std::fs::File::create(&source)
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    let session = h.services.sftp_connect(&host).await.unwrap();
    let done = copy(
        &h.services,
        TransferInput {
            connection_id: session.id.clone(),
            direction: "upload".into(),
            sources: vec![source.display().to_string()],
            destination: remote.display().to_string(),
            overwrite: false,
        },
        true,
    )
    .await;
    assert_eq!(done.status, "cancelled");
    assert!(done.transferred_bytes > 0);
    assert!(
        std::fs::read_dir(&remote).unwrap().next().is_none(),
        "partial file was left behind"
    );
    h.services.sftp_disconnect(&session.id).await.unwrap();
}

#[test]
fn proxy_password_is_write_only_and_assigned_profiles_prevent_proxy_deletion() {
    let h = harness();
    let proxy = h
        .services
        .proxy_save(ProxyInput {
            id: None,
            name: "Local gateway".into(),
            kind: "socks5".into(),
            hostname: "127.0.0.1".into(),
            port: 1080,
            username: Some("tester".into()),
            password: Some("proxy-sentinel".into()),
            clear_password: false,
        })
        .unwrap();
    let json = serde_json::to_string(&h.services.proxy_list().unwrap()).unwrap();
    assert!(!json.contains("proxy-sentinel"));
    assert!(proxy.password_configured);
    let mut input = password_host("Proxy profile", 22);
    input.proxy_id = Some(proxy.id.clone());
    let host = h.services.host_save(input).unwrap();
    assert_eq!(host.proxy_id.as_deref(), Some(proxy.id.as_str()));
    assert!(h.services.proxy_delete(&proxy.id).is_err());
    assert_eq!(
        h.services.host_get(&host.id).unwrap().proxy_id,
        host.proxy_id
    );
    let mut input = password_host("Direct profile", 22);
    input.id = Some(host.id);
    input.proxy_id = None;
    h.services.host_save(input).unwrap();
    h.services.proxy_delete(&proxy.id).unwrap();
    assert!(h.services.proxy_list().unwrap().is_empty());
}
