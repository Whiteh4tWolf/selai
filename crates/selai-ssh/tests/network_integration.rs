//! Real OpenSSH forwarding and proxy transport tests, isolated to local fixtures.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use selai_secrets::SecretValue;
use selai_ssh::{
    connect, connect_via, Auth, ConnectOptions, Endpoint, ForwardCounters, ForwardSpec, Forwarder,
    PinnedHostKey, ProxyKind, ProxyOptions, SshError,
};
use selai_testkit::{material, Sshd, USER};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;
fn opts(server: &Sshd, pinned: bool) -> ConnectOptions {
    ConnectOptions {
        endpoint: Endpoint::new("127.0.0.1", server.port),
        username: USER.into(),
        auth: Auth::Password(SecretValue::new(material().password.as_str())),
        pinned: if pinned {
            vec![PinnedHostKey::from_openssh_line(&material().host_pub).unwrap()]
        } else {
            vec![]
        },
        connect_timeout: Duration::from_secs(10),
        keepalive: None,
    }
}

#[tokio::test]
async fn local_remote_and_dynamic_forwarding_copy_bytes_and_release_ports() {
    let server = Sshd::start();
    for mode in ["local", "remote", "dynamic"] {
        let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        let echo_task = tokio::spawn(async move {
            let (mut socket, _) = echo.accept().await.unwrap();
            let mut data = [0; 12];
            socket.read_exact(&mut data).await.unwrap();
            assert_eq!(&data, b"hello-tunnel");
            socket.write_all(&data).await.unwrap();
            socket.shutdown().await.unwrap();
        });
        let session = Arc::new(connect(opts(&server, true)).await.unwrap());
        let forward = Forwarder::start(
            session,
            ForwardSpec {
                mode: mode.into(),
                bind: "127.0.0.1".into(),
                port: 0,
                target: "127.0.0.1".into(),
                target_port: echo_port,
            },
        )
        .await
        .unwrap();
        let port = forward.port();
        assert!(port > 0);
        let stop = CancellationToken::new();
        let stats = Arc::new(ForwardCounters::default());
        let task = tokio::spawn(forward.run(stop.clone(), stats.clone()));
        let mut socket = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        if mode == "dynamic" {
            socket.write_all(&[5, 1, 0]).await.unwrap();
            let mut greeting = [0; 2];
            socket.read_exact(&mut greeting).await.unwrap();
            assert_eq!(greeting, [5, 0]);
            let mut request = vec![5, 1, 0, 3, 9];
            request.extend_from_slice(b"127.0.0.1");
            request.extend_from_slice(&echo_port.to_be_bytes());
            socket.write_all(&request).await.unwrap();
            let mut reply = [0; 10];
            socket.read_exact(&mut reply).await.unwrap();
            assert_eq!(reply[1], 0);
        }
        socket.write_all(b"hello-tunnel").await.unwrap();
        let mut reply = [0; 12];
        tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut reply))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&reply, b"hello-tunnel");
        socket.shutdown().await.unwrap();
        drop(socket);
        echo_task.await.unwrap();
        stop.cancel();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(stats.sent.load(Ordering::Relaxed), 12);
        assert_eq!(stats.received.load(Ordering::Relaxed), 12);
        assert!(
            TcpStream::connect(("127.0.0.1", port)).await.is_err(),
            "{mode} did not release listener"
        );
    }
}

#[tokio::test]
async fn http_and_socks_proxies_preserve_host_pin_verification_and_authenticate() {
    let server = Sshd::start();
    for kind in [ProxyKind::Http, ProxyKind::Socks5] {
        for pinned in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let ssh_port = server.port;
            let relay = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                match kind {
                    ProxyKind::Http => {
                        let mut header = Vec::new();
                        while !header.ends_with(b"\r\n\r\n") {
                            header.push(socket.read_u8().await.unwrap());
                            assert!(header.len() < 4096);
                        }
                        let text = String::from_utf8(header).unwrap();
                        assert!(
                            text.starts_with(&format!("CONNECT 127.0.0.1:{ssh_port} HTTP/1.1\r\n"))
                        );
                        assert!(text.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"));
                        socket
                            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                            .await
                            .unwrap();
                    }
                    ProxyKind::Socks5 => {
                        let mut request = [0; 3];
                        socket.read_exact(&mut request).await.unwrap();
                        assert_eq!(request, [5, 1, 2]);
                        socket.write_all(&[5, 2]).await.unwrap();
                        assert_eq!(socket.read_u8().await.unwrap(), 1);
                        let u = socket.read_u8().await.unwrap();
                        let mut user = vec![0; u as usize];
                        socket.read_exact(&mut user).await.unwrap();
                        let p = socket.read_u8().await.unwrap();
                        let mut pass = vec![0; p as usize];
                        socket.read_exact(&mut pass).await.unwrap();
                        assert_eq!(user, b"user");
                        assert_eq!(pass, b"pass");
                        socket.write_all(&[1, 0]).await.unwrap();
                        let mut h = [0; 5];
                        socket.read_exact(&mut h).await.unwrap();
                        assert_eq!(&h[..4], &[5, 1, 0, 3]);
                        let mut host = vec![0; h[4] as usize];
                        socket.read_exact(&mut host).await.unwrap();
                        assert_eq!(host, b"127.0.0.1");
                        assert_eq!(socket.read_u16().await.unwrap(), ssh_port);
                        socket
                            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
                            .await
                            .unwrap();
                    }
                }
                let mut target = TcpStream::connect(("127.0.0.1", ssh_port)).await.unwrap();
                let _ = tokio::io::copy_bidirectional(&mut socket, &mut target).await;
            });
            let proxy = ProxyOptions {
                endpoint: Endpoint::new("127.0.0.1", port),
                kind,
                username: Some("user".into()),
                password: Some(SecretValue::new("pass")),
            };
            let result = connect_via(opts(&server, pinned), Some(proxy)).await;
            if pinned {
                result.unwrap().disconnect().await.unwrap();
            } else {
                assert!(matches!(result, Err(SshError::HostKeyUnknown(_))));
            }
            tokio::time::timeout(Duration::from_secs(5), relay)
                .await
                .unwrap()
                .unwrap();
        }
    }
}
