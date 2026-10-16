//! Per-host history, output retention, app settings and startup reconciliation
//! (S-040, S-042). Each retention action must leave an audit event.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use selai_domain::{
    ApprovalId, ApprovalState, ExecStatus, ExecutionId, HostId, PageRequest, TaskId, TaskState,
};
use selai_storage::{
    Database, ExecutionOutcome, HistoryFilter, NewApproval, NewExecution, NewTask, PurgeScope,
};
use serde_json::json;

const T0: i64 = 10_000;

fn host_row(db: &mut Database, id: HostId, label: &str) {
    let settings = selai_domain::HostSettings {
        label: selai_domain::HostLabel::parse(label).expect("label"),
        hostname: selai_domain::Hostname::parse("h.example.com").expect("host"),
        port: selai_domain::Port::SSH_DEFAULT,
        username: selai_domain::Username::parse("ops").expect("user"),
        auth: selai_domain::AuthMethod::Password,
        private_key_path: None,
        group_id: None,
        connect_timeout: selai_domain::ConnectTimeout::DEFAULT,
        keepalive: selai_domain::Keepalive::DEFAULT,
        terminal_type: selai_domain::TerminalType::parse("xterm-256color").expect("term"),
        notes: None,
    };
    db.create_host(id, &settings, T0).expect("host");
}

fn task_on(db: &mut Database, host: HostId) -> TaskId {
    let id = TaskId::new();
    db.create_agent_task(
        &NewTask {
            id,
            host,
            provider: None,
            model_id: "model-x".into(),
            limits_json: "{}".into(),
        },
        T0,
    )
    .expect("task");
    db.transition_task(id, TaskState::ValidatingScope, None, T0)
        .expect("v");
    db.transition_task(id, TaskState::Ready, None, T0)
        .expect("r");
    db.transition_task(id, TaskState::Running, None, T0)
        .expect("run");
    id
}

/// Proposes and finishes one execution with the given terminal state and preview.
fn finished_exec(
    db: &mut Database,
    task: TaskId,
    host: HostId,
    status: ExecStatus,
    at: i64,
    preview: &str,
) -> ExecutionId {
    let id = ExecutionId::new();
    db.propose_execution(&NewExecution {
        id,
        task,
        host,
        provider: None,
        model_id: "model-x".into(),
        tool_call_id: Some("call".into()),
        tool_name: "ssh.run_diagnostic".into(),
        command_redacted: "df -hP".into(),
        command_sha256: format!("{:064x}", at),
        policy_decision: "allow".into(),
        now_ms: at,
    })
    .expect("propose");
    db.transition_execution(
        id,
        ExecStatus::Running,
        &ExecutionOutcome::default(),
        at + 1,
    )
    .expect("run");
    db.transition_execution(
        id,
        status,
        &ExecutionOutcome {
            exit_code: Some(0),
            error_code: None,
            stdout_preview: Some(preview.into()),
            stderr_preview: Some("warn".into()),
            output_truncated: false,
        },
        at + 2,
    )
    .expect("finish");
    id
}

fn audit_events(db: &Database, kind: &str) -> i64 {
    db.connection_for_provider_test()
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = ?1",
            [kind],
            |r| r.get(0),
        )
        .expect("count")
}

#[test]
fn filtered_history_pages_without_duplicates_or_gaps() {
    let mut db = Database::open_in_memory().expect("db");
    let host = HostId::new();
    host_row(&mut db, host, "web");
    let task = task_on(&mut db, host);
    for i in 0..5 {
        let status = if i % 2 == 0 {
            ExecStatus::Completed
        } else {
            ExecStatus::Failed
        };
        finished_exec(&mut db, task, host, status, T0 + 100 * (i + 1), "out");
    }

    let only_failed = HistoryFilter {
        states: vec![ExecStatus::Failed],
        ..Default::default()
    };
    let failed = db
        .host_history_filtered(
            host,
            &only_failed,
            &PageRequest {
                cursor: None,
                limit: 10,
            },
        )
        .expect("filter");
    assert_eq!(failed.items.len(), 2);
    assert!(failed.items.iter().all(|e| e.state == ExecStatus::Failed));

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = db
            .host_history_filtered(
                host,
                &HistoryFilter::default(),
                &PageRequest {
                    cursor: cursor.clone(),
                    limit: 2,
                },
            )
            .expect("page");
        seen.extend(page.items.iter().map(|e| e.id));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(seen.len(), 5, "every execution appears once");
    let unique: std::collections::HashSet<_> = seen.iter().collect();
    assert_eq!(unique.len(), 5);
}

#[test]
fn time_window_limits_results() {
    let mut db = Database::open_in_memory().expect("db");
    let host = HostId::new();
    host_row(&mut db, host, "web");
    let task = task_on(&mut db, host);
    finished_exec(&mut db, task, host, ExecStatus::Completed, T0 + 100, "a");
    finished_exec(&mut db, task, host, ExecStatus::Completed, T0 + 500, "b");
    let window = HistoryFilter {
        since_ms: Some(T0 + 200),
        until_ms: Some(T0 + 600),
        ..Default::default()
    };
    let page = db
        .host_history_filtered(
            host,
            &window,
            &PageRequest {
                cursor: None,
                limit: 10,
            },
        )
        .expect("window");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].stdout_preview.as_deref(), Some("b"));
}

#[test]
fn timeline_lists_one_task_in_order_and_host_tasks_are_listed() {
    let mut db = Database::open_in_memory().expect("db");
    let host = HostId::new();
    host_row(&mut db, host, "web");
    let first = task_on(&mut db, host);
    let second = task_on(&mut db, host);
    let a = finished_exec(&mut db, first, host, ExecStatus::Completed, T0 + 100, "1");
    let b = finished_exec(&mut db, first, host, ExecStatus::Completed, T0 + 300, "2");
    finished_exec(&mut db, second, host, ExecStatus::Completed, T0 + 400, "3");

    let timeline = db.task_timeline(first).expect("timeline");
    assert_eq!(
        timeline.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![a, b]
    );

    let tasks = db
        .host_tasks(
            host,
            &PageRequest {
                cursor: None,
                limit: 10,
            },
        )
        .expect("tasks");
    assert_eq!(tasks.items.len(), 2);
    assert!(tasks.items.iter().any(|t| t.id == second));
}

#[test]
fn purge_clears_output_keeps_metadata_and_is_audited() {
    let mut db = Database::open_in_memory().expect("db");
    let web = HostId::new();
    let db_host = HostId::new();
    host_row(&mut db, web, "web");
    host_row(&mut db, db_host, "database");
    let task_web = task_on(&mut db, web);
    let task_db = task_on(&mut db, db_host);
    let old = finished_exec(
        &mut db,
        task_web,
        web,
        ExecStatus::Completed,
        T0 + 100,
        "secret-ish output",
    );
    let other = finished_exec(
        &mut db,
        task_db,
        db_host,
        ExecStatus::Completed,
        T0 + 100,
        "other host",
    );

    let cleared = db
        .purge_output(PurgeScope::FinishedBefore(T0 + 1_000), T0 + 2_000)
        .expect("purge");
    assert_eq!(cleared, 2);

    let row = db.get_execution(old).expect("get").expect("row");
    assert_eq!(row.stdout_preview, None);
    assert_eq!(row.stderr_preview, None);
    assert_eq!(row.command_redacted, "df -hP", "command text stays");
    assert_eq!(row.exit_code, Some(0), "exit code stays");
    assert_eq!(row.state, ExecStatus::Completed, "state stays");
    assert!(db
        .get_execution(other)
        .expect("get")
        .expect("row")
        .stdout_preview
        .is_none());

    assert_eq!(audit_events(&db, "output_purged"), 1, "one audited action");
    assert!(db.verify_audit_chain().expect("verify").is_ok());
}

#[test]
fn host_scope_purge_touches_only_that_host() {
    let mut db = Database::open_in_memory().expect("db");
    let web = HostId::new();
    let other_host = HostId::new();
    host_row(&mut db, web, "web");
    host_row(&mut db, other_host, "database");
    let task_web = task_on(&mut db, web);
    let task_other = task_on(&mut db, other_host);
    let kept = finished_exec(
        &mut db,
        task_other,
        other_host,
        ExecStatus::Completed,
        T0 + 100,
        "keep",
    );
    finished_exec(
        &mut db,
        task_web,
        web,
        ExecStatus::Completed,
        T0 + 100,
        "drop",
    );

    assert_eq!(
        db.purge_output(PurgeScope::Host(web), T0 + 2_000)
            .expect("purge"),
        1
    );
    assert_eq!(
        db.get_execution(kept)
            .expect("get")
            .expect("row")
            .stdout_preview
            .as_deref(),
        Some("keep")
    );
}

#[test]
fn purge_with_nothing_to_remove_writes_no_audit_event() {
    let mut db = Database::open_in_memory().expect("db");
    assert_eq!(
        db.purge_output(PurgeScope::FinishedBefore(T0), T0)
            .expect("purge"),
        0
    );
    assert_eq!(audit_events(&db, "output_purged"), 0);
}

#[test]
fn settings_round_trip_and_refuse_secret_like_keys() {
    let mut db = Database::open_in_memory().expect("db");
    assert_eq!(db.get_setting("ai.retention_days").expect("get"), None);
    db.set_setting("ai.retention_days", &json!(30), T0)
        .expect("set");
    db.set_setting("ai.retention_days", &json!(14), T0 + 1)
        .expect("update");
    assert_eq!(
        db.get_setting("ai.retention_days").expect("get"),
        Some(json!(14))
    );

    for key in ["provider_api_key", "ssh.password", "auth_token"] {
        assert!(
            db.set_setting(key, &json!("x"), T0).is_err(),
            "{key} must not be stored as a setting"
        );
    }
}

#[test]
fn startup_expires_stale_approvals_and_is_audited() {
    let mut db = Database::open_in_memory().expect("db");
    let host = HostId::new();
    host_row(&mut db, host, "web");
    let task = task_on(&mut db, host);
    let exec = ExecutionId::new();
    db.propose_execution(&NewExecution {
        id: exec,
        task,
        host,
        provider: None,
        model_id: "model-x".into(),
        tool_call_id: None,
        tool_name: "ssh.run_command".into(),
        command_redacted: "touch /tmp/x".into(),
        command_sha256: format!("{:064x}", 7),
        policy_decision: "needs_approval".into(),
        now_ms: T0 + 1,
    })
    .expect("propose");
    db.transition_execution(
        exec,
        ExecStatus::AwaitingApproval,
        &ExecutionOutcome::default(),
        T0 + 2,
    )
    .expect("await");
    let approval = ApprovalId::new();
    db.create_approval(&NewApproval {
        id: approval,
        execution: exec,
        policy_version: "policy-v1".into(),
        now_ms: T0 + 2,
        expires_at_ms: T0 + 500,
    })
    .expect("approval");

    let report = db.reconcile_after_restart(T0 + 10_000).expect("reconcile");
    assert_eq!(report.approvals_closed, 1);
    assert_eq!(report.executions_interrupted, 1);
    assert_eq!(
        db.get_approval(approval).expect("get").expect("row").state,
        ApprovalState::Expired
    );
    assert_eq!(audit_events(&db, "startup_reconciled"), 1);
    assert!(db.verify_audit_chain().expect("verify").is_ok());

    let again = db.reconcile_after_restart(T0 + 20_000).expect("second run");
    assert_eq!(again.approvals_closed, 0);
    assert_eq!(
        audit_events(&db, "startup_reconciled"),
        1,
        "a clean start writes nothing"
    );
}
