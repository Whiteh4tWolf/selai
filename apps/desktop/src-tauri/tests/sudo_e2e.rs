//! Root commands through the application service with the real `sudo`: a host
//! that signs in with a password and one that signs in with a key. The sudo
//! password goes from the approval to sudo's prompt and nowhere else.

#![cfg(target_os = "linux")] // Needs OpenSSH sshd, sudo and root; see docs/release/milestones.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use selai_agent::COMMAND_TOOL;
use selai_desktop_lib::agent_service::SudoAnswerInput;
use selai_domain::ErrorCode;
use selai_testkit::allow_sudo_with_password;

mod common;
use common::*;

fn root_turns(times: usize) -> Vec<Turn> {
    (0..times)
        .flat_map(|_| {
            [
                Turn::Tool(COMMAND_TOOL, json!({"command": "id -u", "as_root": true})),
                Turn::Text("Checked."),
            ]
        })
        .collect()
}

fn answer(source: &str, password: Option<&str>, remember: bool) -> SudoAnswerInput {
    SudoAnswerInput {
        source: source.to_owned(),
        password: password.map(str::to_owned),
        remember: Some(remember),
    }
}

/// Starts a root task and returns the approval id and the sudo status it showed.
async fn start_root_task(
    h: &Harness,
    host: &str,
    provider: &str,
) -> (String, Option<&'static str>, Events) {
    let (on_event, mut rx) = sink();
    h.services
        .agent_start(host, provider, "what is my uid as root?", &[], on_event)
        .await
        .expect("start");
    let waiting = until(&mut rx, |e| {
        matches!(e, TaskEventView::ApprovalRequired { .. })
    })
    .await;
    match waiting.last() {
        Some(TaskEventView::ApprovalRequired {
            approval_id,
            command,
            sudo,
            ..
        }) => {
            assert_eq!(command, "sudo id -u", "the card shows that it runs as root");
            (approval_id.clone(), *sudo, rx)
        }
        other => panic!("expected an approval, got {other:?}"),
    }
}

fn file_contains(path: &std::path::Path, needle: &str) -> bool {
    let bytes = std::fs::read(path).unwrap_or_default();
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[tokio::test]
async fn a_password_host_can_reuse_its_login_password_for_sudo() {
    allow_sudo_with_password();
    let h = harness();
    let sshd = Sshd::start();
    let mock = model(root_turns(1)).await;
    let (host, provider) = ready(&h, &sshd, &mock).await;

    let (approval, sudo, mut rx) = start_root_task(&h, &host, &provider).await;
    assert_eq!(sudo, Some("password_required"));
    let options = h.services.sudo_options(&approval).expect("options");
    assert!(options.login_password && !options.saved_password);

    h.services
        .approval_decide(&approval, true, Some(answer("login", None, false)))
        .expect("approve");
    let seen = until(&mut rx, is_terminal).await;
    assert_eq!(terminal_state(&seen), "completed");

    let row = &h.services.history_list(&host, None).expect("history").items[0];
    assert_eq!(row.command_redacted, "sudo id -u");
    assert_eq!(row.state, "completed");
    assert_eq!(row.stdout_preview.as_deref().map(str::trim), Some("0"));
    let password = &material().password;
    assert!(
        !file_contains(&h.db_path, password),
        "no password in the database"
    );
}

#[tokio::test]
async fn a_key_host_takes_a_typed_password_and_can_remember_it() {
    allow_sudo_with_password();
    let h = harness();
    let sshd = Sshd::start();
    let mock = model(root_turns(2)).await;
    let (host, provider) = ready_with(&h, &mock, key_host("key-box", sshd.port)).await;
    let password = material().password.clone();

    let (approval, sudo, mut rx) = start_root_task(&h, &host, &provider).await;
    assert_eq!(sudo, Some("password_required"));
    let options = h.services.sudo_options(&approval).expect("options");
    assert!(
        !options.login_password,
        "a key host has no login password to reuse"
    );
    let login = h
        .services
        .approval_decide(&approval, true, Some(answer("login", None, false)));
    assert_eq!(
        err_code(login.expect_err("no login password")),
        ErrorCode::InvalidInput
    );

    h.services
        .approval_decide(
            &approval,
            true,
            Some(answer("typed", Some(&password), true)),
        )
        .expect("approve");
    assert_eq!(
        terminal_state(&until(&mut rx, is_terminal).await),
        "completed"
    );

    // The second task uses the remembered password.
    let (approval, _, mut rx) = start_root_task(&h, &host, &provider).await;
    assert!(
        h.services
            .sudo_options(&approval)
            .expect("options")
            .saved_password
    );
    h.services
        .approval_decide(&approval, true, Some(answer("saved", None, false)))
        .expect("approve");
    assert_eq!(
        terminal_state(&until(&mut rx, is_terminal).await),
        "completed"
    );

    let rows = h.services.history_list(&host, None).expect("history").items;
    assert!(
        rows.iter()
            .all(|r| r.state == "completed"
                && r.stdout_preview.as_deref().map(str::trim) == Some("0"))
    );
    assert!(!file_contains(&h.db_path, &password));

    h.services.host_forget_sudo_password(&host).expect("forget");
}

#[tokio::test]
async fn a_wrong_sudo_password_fails_the_command_and_says_why() {
    allow_sudo_with_password();
    let h = harness();
    let sshd = Sshd::start();
    let mock = model(root_turns(1)).await;
    let (host, provider) = ready_with(&h, &mock, key_host("key-box", sshd.port)).await;

    let (approval, _, mut rx) = start_root_task(&h, &host, &provider).await;
    h.services
        .approval_decide(
            &approval,
            true,
            Some(answer("typed", Some("definitely-wrong"), false)),
        )
        .expect("approve");
    until(&mut rx, is_terminal).await;
    let row = &h.services.history_list(&host, None).expect("history").items[0];
    assert_eq!(row.state, "failed");
    assert_eq!(row.error_code.as_deref(), Some("SUDO_AUTH_FAILED"));
    assert!(!file_contains(&h.db_path, "definitely-wrong"));
}
