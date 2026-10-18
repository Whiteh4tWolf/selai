//! SSH behaviour against a real OpenSSH server (ADR-0002, tests T-SSH-001..005).

#![cfg(target_os = "linux")] // Needs OpenSSH sshd and root; see docs/release/milestones.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use std::time::Duration;

use common::{read_until, wait_for_exit};
use selai_secrets::SecretValue;
use selai_ssh::PinnedHostKey;
use selai_ssh::{connect, probe_host_key, Auth, ConnectOptions, Endpoint, SshError};
use selai_testkit::{material, Sshd, USER};

const LIMIT: Duration = Duration::from_secs(15);

fn host_pin() -> PinnedHostKey {
    PinnedHostKey::from_openssh_line(&material().host_pub).expect("host pin")
}

fn other_pin() -> PinnedHostKey {
    PinnedHostKey::from_openssh_line(&material().other_host_pub).expect("other pin")
}

fn password_opts(
    sshd: &Sshd,
    pins: Vec<selai_ssh::PinnedHostKey>,
    password: &str,
) -> ConnectOptions {
    ConnectOptions {
        endpoint: Endpoint::new("127.0.0.1", sshd.port),
        username: USER.to_owned(),
        auth: Auth::Password(SecretValue::new(password)),
        pinned: pins,
        connect_timeout: LIMIT,
        keepalive: Some(Duration::from_secs(30)),
    }
}

#[tokio::test]
async fn probe_reads_the_host_key_and_never_authenticates() {
    let sshd = Sshd::start();
    let observed = probe_host_key(&Endpoint::new("127.0.0.1", sshd.port), LIMIT)
        .await
        .expect("probe");
    assert_eq!(observed.algorithm, "ssh-ed25519");
    assert_eq!(observed.public_key_base64, host_pin().public_key_base64);
    assert_eq!(observed.fingerprint_sha256, host_pin().fingerprint_sha256);
    assert!(
        sshd.auth_decisions().is_empty(),
        "probe must not reach authentication"
    );
}

#[tokio::test]
async fn unpinned_host_is_blocked_before_any_credential_is_sent() {
    let m = material();
    let sshd = Sshd::start();
    let err = connect(password_opts(&sshd, Vec::new(), &m.password))
        .await
        .expect_err("unknown key must not connect");
    match err {
        SshError::HostKeyUnknown(observed) => {
            assert_eq!(observed.fingerprint_sha256, host_pin().fingerprint_sha256);
        }
        other => panic!("expected HostKeyUnknown, got {other:?}"),
    }
    assert!(
        sshd.auth_decisions().is_empty(),
        "server saw an auth attempt: {:?}",
        sshd.auth_decisions()
    );
}

#[tokio::test]
async fn changed_host_key_is_blocked_and_reports_the_pin() {
    let m = material();
    let sshd = Sshd::start();
    let err = connect(password_opts(&sshd, vec![other_pin()], &m.password))
        .await
        .expect_err("changed key must not connect");
    match err {
        SshError::HostKeyChanged {
            observed,
            pinned_fingerprint,
        } => {
            assert_eq!(observed.fingerprint_sha256, host_pin().fingerprint_sha256);
            assert_eq!(pinned_fingerprint, other_pin().fingerprint_sha256);
        }
        other => panic!("expected HostKeyChanged, got {other:?}"),
    }
    assert!(
        sshd.auth_decisions().is_empty(),
        "server saw an auth attempt"
    );
}

#[tokio::test]
async fn pinned_password_login_runs_commands_and_reports_exit_status() {
    let m = material();
    let sshd = Sshd::start();
    let session = connect(password_opts(&sshd, vec![host_pin()], &m.password))
        .await
        .expect("pinned key and right password");
    let mut shell = session
        .open_shell(80, 24, "xterm-256color")
        .await
        .expect("pty");
    shell.write(b"echo selai-$((6*7))\n").await.expect("write");
    read_until(&mut shell, "selai-42", LIMIT).await;
    shell.write(b"exit 0\n").await.expect("exit");
    assert_eq!(wait_for_exit(&mut shell, LIMIT).await, 0);
    assert!(sshd
        .auth_decisions()
        .iter()
        .any(|l| l.contains("Accepted password")));
}

#[tokio::test]
async fn remote_exit_status_is_passed_through() {
    let m = material();
    let sshd = Sshd::start();
    let session = connect(password_opts(&sshd, vec![host_pin()], &m.password))
        .await
        .expect("connect");
    let mut shell = session
        .open_shell(80, 24, "xterm-256color")
        .await
        .expect("pty");
    shell.write(b"exit 3\n").await.expect("exit");
    assert_eq!(wait_for_exit(&mut shell, LIMIT).await, 3);
}

#[tokio::test]
async fn wrong_password_is_rejected_after_trust() {
    let sshd = Sshd::start();
    let err = connect(password_opts(&sshd, vec![host_pin()], "not-the-password"))
        .await
        .expect_err("wrong password");
    assert!(matches!(err, SshError::AuthFailed), "{err:?}");
    assert!(sshd
        .auth_decisions()
        .iter()
        .any(|l| l.contains("Failed password")));
}

#[tokio::test]
async fn private_key_login_without_passphrase() {
    let m = material();
    let sshd = Sshd::start();
    let session = connect(ConnectOptions {
        endpoint: Endpoint::new("127.0.0.1", sshd.port),
        username: USER.to_owned(),
        auth: Auth::PrivateKey {
            path: m.user_key.clone(),
            passphrase: None,
        },
        pinned: vec![host_pin()],
        connect_timeout: LIMIT,
        keepalive: None,
    })
    .await
    .expect("key login");
    let mut shell = session
        .open_shell(80, 24, "xterm-256color")
        .await
        .expect("pty");
    shell.write(b"whoami\n").await.expect("write");
    read_until(&mut shell, USER, LIMIT).await;
    assert!(sshd
        .auth_decisions()
        .iter()
        .any(|l| l.contains("Accepted publickey")));
}

#[tokio::test]
async fn encrypted_key_needs_the_right_passphrase() {
    let m = material();
    let sshd = Sshd::start();
    let with = |passphrase: &str| ConnectOptions {
        endpoint: Endpoint::new("127.0.0.1", sshd.port),
        username: USER.to_owned(),
        auth: Auth::PrivateKey {
            path: m.encrypted_key.clone(),
            passphrase: Some(SecretValue::new(passphrase)),
        },
        pinned: vec![host_pin()],
        connect_timeout: LIMIT,
        keepalive: None,
    };

    let err = connect(with("wrong-passphrase"))
        .await
        .expect_err("wrong passphrase");
    assert!(matches!(err, SshError::KeyLoad(_)), "{err:?}");
    assert!(
        sshd.auth_decisions().is_empty(),
        "key failed locally; nothing should reach the server"
    );

    let session = connect(with(&m.encrypted_passphrase))
        .await
        .expect("right passphrase");
    let mut shell = session
        .open_shell(80, 24, "xterm-256color")
        .await
        .expect("pty");
    shell.write(b"whoami\n").await.expect("write");
    read_until(&mut shell, USER, LIMIT).await;
}

#[tokio::test]
async fn pty_resize_is_visible_to_the_remote_program() {
    let m = material();
    let sshd = Sshd::start();
    let session = connect(password_opts(&sshd, vec![host_pin()], &m.password))
        .await
        .expect("connect");
    let mut shell = session
        .open_shell(80, 24, "xterm-256color")
        .await
        .expect("pty");
    shell.resize(100, 30).await.expect("resize");
    shell.write(b"stty size\n").await.expect("write");
    read_until(&mut shell, "30 100", LIMIT).await;
}

#[tokio::test]
async fn multibyte_output_arrives_intact() {
    let m = material();
    let sshd = Sshd::start();
    let session = connect(password_opts(&sshd, vec![host_pin()], &m.password))
        .await
        .expect("connect");
    let mut shell = session
        .open_shell(80, 24, "xterm-256color")
        .await
        .expect("pty");
    shell
        .write(b"printf '%s\\n' \"$(printf '\\xc3\\xa9\\xe2\\x9c\\x93')\"\n")
        .await
        .expect("write");
    read_until(&mut shell, "é✓", LIMIT).await;
}

#[tokio::test]
async fn closed_port_is_a_connect_failure_not_a_key_error() {
    let m = material();
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        l.local_addr().expect("addr").port()
    };
    let err = connect(ConnectOptions {
        endpoint: Endpoint::new("127.0.0.1", port),
        username: USER.to_owned(),
        auth: Auth::Password(SecretValue::new(m.password.clone())),
        pinned: vec![host_pin()],
        connect_timeout: LIMIT,
        keepalive: None,
    })
    .await
    .expect_err("nothing listening");
    assert!(matches!(err, SshError::ConnectFailed(_)), "{err:?}");
}

#[tokio::test]
async fn silent_server_hits_the_connect_timeout() {
    let m = material();
    // Accepts TCP connections but never speaks SSH.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let started = std::time::Instant::now();
    let err = connect(ConnectOptions {
        endpoint: Endpoint::new("127.0.0.1", port),
        username: USER.to_owned(),
        auth: Auth::Password(SecretValue::new(m.password.clone())),
        pinned: vec![host_pin()],
        connect_timeout: Duration::from_secs(2),
        keepalive: None,
    })
    .await
    .expect_err("no banner");
    assert!(matches!(err, SshError::ConnectTimeout), "{err:?}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "timeout must be bounded"
    );
    drop(listener);
}

#[tokio::test]
async fn rsa_private_key_authenticates_with_sha2_only() {
    let m = material();
    for algorithm in ["rsa-sha2-512", "rsa-sha2-256"] {
        let sshd = Sshd::start_with_pubkey_algorithms(algorithm);
        let session = connect(ConnectOptions {
            endpoint: Endpoint::new("127.0.0.1", sshd.port),
            username: USER.to_owned(),
            auth: Auth::PrivateKey {
                path: m.rsa_key.clone(),
                passphrase: None,
            },
            pinned: vec![host_pin()],
            connect_timeout: LIMIT,
            keepalive: None,
        })
        .await
        .expect("RSA SHA-2 login");
        let mut shell = session
            .open_shell(80, 24, "xterm-256color")
            .await
            .expect("pty");
        shell.write(b"whoami\n").await.expect("write");
        read_until(&mut shell, USER, LIMIT).await;
        assert!(sshd
            .auth_decisions()
            .iter()
            .any(|l| l.contains("Accepted publickey")));
    }
}

#[tokio::test]
async fn encrypted_rsa_pem_authenticates_from_vault_data_and_rejects_wrong_passphrase() {
    let m = material();
    let sshd = Sshd::start();
    let pem = std::fs::read(&m.encrypted_rsa_key).expect("read fixture");
    let with = |passphrase: &str| ConnectOptions {
        endpoint: Endpoint::new("127.0.0.1", sshd.port),
        username: USER.to_owned(),
        auth: Auth::PrivateKeyData {
            pem: SecretValue::new(pem.clone()),
            passphrase: Some(SecretValue::new(passphrase)),
        },
        pinned: vec![host_pin()],
        connect_timeout: LIMIT,
        keepalive: None,
    };
    let err = connect(with("wrong-passphrase"))
        .await
        .expect_err("wrong passphrase");
    assert!(matches!(err, SshError::KeyLoad(_)), "{err:?}");
    assert!(
        sshd.auth_decisions().is_empty(),
        "wrong passphrase must fail locally"
    );
    let session = connect(with(&m.encrypted_passphrase))
        .await
        .expect("encrypted RSA login");
    let mut shell = session
        .open_shell(80, 24, "xterm-256color")
        .await
        .expect("pty");
    shell.write(b"whoami\n").await.expect("write");
    read_until(&mut shell, USER, LIMIT).await;
}

#[tokio::test]
async fn rsa_sha1_only_server_is_rejected() {
    let m = material();
    let sshd = Sshd::start_with_pubkey_algorithms("ssh-rsa");
    let err = connect(ConnectOptions {
        endpoint: Endpoint::new("127.0.0.1", sshd.port),
        username: USER.to_owned(),
        auth: Auth::PrivateKey {
            path: m.rsa_key.clone(),
            passphrase: None,
        },
        pinned: vec![host_pin()],
        connect_timeout: LIMIT,
        keepalive: None,
    })
    .await
    .expect_err("SHA-1 must not be used");
    assert!(
        matches!(err, SshError::KeyLoad(_) | SshError::AuthFailed),
        "{err:?}"
    );
    assert!(!sshd.auth_decisions().iter().any(|l| l.contains("Accepted")));
}

#[tokio::test]
async fn exec_keeps_streams_apart_and_reports_the_exit_status() {
    use selai_ssh::ExecEnd;
    use tokio_util::sync::CancellationToken;
    let sshd = Sshd::start();
    let m = material();
    let session = connect(password_opts(&sshd, vec![host_pin()], &m.password))
        .await
        .expect("connect");
    let out = session
        .exec(
            "printf out; printf err >&2; exit 7",
            LIMIT,
            64 * 1024,
            &CancellationToken::new(),
        )
        .await
        .expect("exec");
    assert_eq!(out.stdout, b"out");
    assert_eq!(out.stderr, b"err");
    assert_eq!(out.exit_status, Some(7));
    assert_eq!(out.end, ExecEnd::Completed);
    assert!(!out.stdout_truncated && !out.stderr_truncated);
}

#[tokio::test]
async fn exec_output_is_capped_and_marked_truncated() {
    use tokio_util::sync::CancellationToken;
    let sshd = Sshd::start();
    let m = material();
    let session = connect(password_opts(&sshd, vec![host_pin()], &m.password))
        .await
        .expect("connect");
    let out = session
        .exec(
            "head -c 200000 /dev/zero | tr '\\0' a",
            LIMIT,
            65_536,
            &CancellationToken::new(),
        )
        .await
        .expect("exec");
    assert_eq!(out.stdout.len(), 65_536);
    assert!(
        out.stdout_truncated,
        "truncation must be visible to the caller"
    );
}

#[tokio::test]
async fn exec_timeout_closes_the_channel_and_says_so() {
    use selai_ssh::ExecEnd;
    use tokio_util::sync::CancellationToken;
    let sshd = Sshd::start();
    let m = material();
    let session = connect(password_opts(&sshd, vec![host_pin()], &m.password))
        .await
        .expect("connect");
    let started = std::time::Instant::now();
    let out = session
        .exec(
            "sleep 30",
            Duration::from_secs(1),
            65_536,
            &CancellationToken::new(),
        )
        .await
        .expect("exec");
    assert_eq!(out.end, ExecEnd::TimedOut);
    assert_eq!(
        out.exit_status, None,
        "no exit status is invented for a timed-out command"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn exec_cancellation_stops_waiting_immediately() {
    use selai_ssh::ExecEnd;
    use tokio_util::sync::CancellationToken;
    let sshd = Sshd::start();
    let m = material();
    let session = connect(password_opts(&sshd, vec![host_pin()], &m.password))
        .await
        .expect("connect");
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        trigger.cancel();
    });
    let started = std::time::Instant::now();
    let out = session
        .exec("sleep 30", LIMIT, 65_536, &cancel)
        .await
        .expect("exec");
    assert_eq!(out.end, ExecEnd::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(5));
}
