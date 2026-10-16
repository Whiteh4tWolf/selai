//! `sudo` through an exec channel against a real OpenSSH server and the real
//! sudo program. The password must reach sudo and nothing else.

#![cfg(target_os = "linux")] // Needs OpenSSH sshd, sudo and root; see docs/release/milestones.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::Duration;

use selai_secrets::SecretValue;
use selai_ssh::{
    connect, sudo::sudo_auth_failed, Auth, ConnectOptions, Endpoint, PinnedHostKey, Session,
    SudoStatus,
};
use selai_testkit::{allow_sudo_with_password, material, Sshd, USER};
use tokio_util::sync::CancellationToken;

const LIMIT: Duration = Duration::from_secs(20);

async fn session(sshd: &Sshd) -> Session {
    allow_sudo_with_password();
    let m = material();
    connect(ConnectOptions {
        endpoint: Endpoint::new("127.0.0.1", sshd.port),
        username: USER.to_owned(),
        auth: Auth::Password(SecretValue::new(m.password.as_str())),
        pinned: vec![PinnedHostKey::from_openssh_line(&m.host_pub).expect("pin")],
        connect_timeout: LIMIT,
        keepalive: None,
    })
    .await
    .expect("connect")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[tokio::test]
async fn the_probe_says_a_password_is_needed_and_runs_nothing_as_root() {
    let sshd = Sshd::start();
    let s = session(&sshd).await;
    assert_eq!(
        s.sudo_status(LIMIT).await.expect("probe"),
        SudoStatus::PasswordRequired
    );
}

#[tokio::test]
async fn the_right_password_runs_the_command_as_root_and_never_appears_in_output() {
    let sshd = Sshd::start();
    let s = session(&sshd).await;
    let password = material().password.clone();
    let out = s
        .exec_as_root(
            "echo \"it's uid $(id -u)\"",
            Some(&SecretValue::new(password.as_str())),
            LIMIT,
            64 * 1024,
            &CancellationToken::new(),
        )
        .await
        .expect("exec");
    assert_eq!(out.exit_status, Some(0), "stderr: {}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "it's uid 0\n");
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    assert!(!all.contains(&password));
    assert!(!all.contains("selai-sudo:"), "the prompt is removed: {all}");
}

#[tokio::test]
async fn a_command_that_reads_stdin_cannot_capture_the_password() {
    let sshd = Sshd::start();
    let s = session(&sshd).await;
    let password = material().password.clone();
    let out = s
        .exec_as_root(
            "cat; echo done",
            Some(&SecretValue::new(password.as_str())),
            LIMIT,
            64 * 1024,
            &CancellationToken::new(),
        )
        .await
        .expect("exec");
    assert_eq!(out.exit_status, Some(0));
    assert_eq!(
        text(&out.stdout),
        "done\n",
        "stdin is closed after the password"
    );
    assert!(!text(&out.stderr).contains(&password));
}

#[tokio::test]
async fn a_wrong_password_fails_without_running_the_command() {
    let sshd = Sshd::start();
    let s = session(&sshd).await;
    let out = s
        .exec_as_root(
            "touch /root/selai-should-not-exist",
            Some(&SecretValue::new("wrong-password")),
            LIMIT,
            64 * 1024,
            &CancellationToken::new(),
        )
        .await
        .expect("exec");
    assert_ne!(out.exit_status, Some(0));
    assert!(
        sudo_auth_failed(&text(&out.stderr)),
        "{}",
        text(&out.stderr)
    );
    assert!(!std::path::Path::new("/root/selai-should-not-exist").exists());
}

#[tokio::test]
async fn without_a_password_sudo_refuses_instead_of_prompting() {
    let sshd = Sshd::start();
    let s = session(&sshd).await;
    let out = s
        .exec_as_root("id -u", None, LIMIT, 4096, &CancellationToken::new())
        .await
        .expect("exec");
    assert_ne!(out.exit_status, Some(0));
    assert!(text(&out.stderr).contains("a password is required"));
}
