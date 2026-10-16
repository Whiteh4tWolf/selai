//! Agent repository behaviour: transitions, approval binding, audit chain.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use selai_domain::{ApprovalId, ExecStatus, ExecutionId, HostId, PageRequest, TaskId, TaskState};
use selai_storage::{
    ApprovalDecision, Database, ExecutionOutcome, NewApproval, NewExecution, NewTask, StorageError,
};

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

fn new_task(host: HostId) -> NewTask {
    NewTask {
        id: TaskId::new(),
        host,
        provider: None,
        model_id: "model-x".into(),
        limits_json: "{}".into(),
    }
}

fn exec(task: TaskId, host: HostId, sha: &str) -> NewExecution {
    NewExecution {
        id: ExecutionId::new(),
        task,
        host,
        provider: None,
        model_id: "model-x".into(),
        tool_call_id: Some("call_1".into()),
        tool_name: "ssh.run_command".into(),
        command_redacted: "uname -a".into(),
        command_sha256: sha.into(),
        policy_decision: "needs_approval".into(),
        now_ms: T0 + 1,
    }
}

fn audit_count(db: &Database) -> i64 {
    db.connection_for_provider_test()
        .query_row("SELECT COUNT(*) FROM audit_events", [], |r| r.get(0))
        .expect("count")
}

#[test]
fn task_walks_its_states_and_rejects_illegal_jumps() {
    let mut db = Database::open_in_memory().expect("db");
    let host = HostId::new();
    host_row(&mut db, host, "web");
    let task = new_task(host);
    db.create_agent_task(&task, T0).expect("create");
    assert!(matches!(
        db.transition_task(task.id, TaskState::Running, None, T0),
        Err(StorageError::Conflict(_))
    ));
    for next in [
        TaskState::ValidatingScope,
        TaskState::Ready,
        TaskState::Running,
        TaskState::Completed,
    ] {
        db.transition_task(task.id, next, None, T0 + 2)
            .expect("legal step");
    }
    let rec = db.get_agent_task(task.id).expect("get").expect("row");
    assert_eq!(rec.state, TaskState::Completed);
    assert!(rec.finished_at_ms.is_some());
}

#[test]
fn a_command_for_another_host_is_refused_and_leaves_no_audit_trace() {
    let mut db = Database::open_in_memory().expect("db");
    let (bound, other) = (HostId::new(), HostId::new());
    host_row(&mut db, bound, "web");
    host_row(&mut db, other, "db");
    let task = new_task(bound);
    db.create_agent_task(&task, T0).expect("task");
    let before = audit_count(&db);
    assert!(db.propose_execution(&exec(task.id, other, "aa")).is_err());
    assert_eq!(
        audit_count(&db),
        before,
        "a refused proposal must not leave an audit row"
    );
}

#[test]
fn approval_binds_to_one_execution_digest_and_host() {
    let mut db = Database::open_in_memory().expect("db");
    let host = HostId::new();
    host_row(&mut db, host, "web");
    let task = new_task(host);
    db.create_agent_task(&task, T0).expect("task");
    let e = exec(task.id, host, "digest-1");
    db.propose_execution(&e).expect("propose");
    db.transition_execution(
        e.id,
        ExecStatus::AwaitingApproval,
        &ExecutionOutcome::default(),
        T0 + 2,
    )
    .expect("await");

    let approval = ApprovalId::new();
    db.create_approval(&NewApproval {
        id: approval,
        execution: e.id,
        policy_version: "policy-v1".into(),
        now_ms: T0 + 2,
        expires_at_ms: T0 + 300_000,
    })
    .expect("ticket");
    let decided = db
        .decide_approval(approval, ApprovalDecision::Approve, T0 + 3)
        .expect("approve");
    assert_eq!(decided.command_sha256, "digest-1");
    assert!(
        matches!(
            db.decide_approval(approval, ApprovalDecision::Deny, T0 + 4),
            Err(StorageError::Conflict(_))
        ),
        "a ticket is decided once"
    );
}

#[test]
fn an_expired_ticket_cannot_be_approved() {
    let mut db = Database::open_in_memory().expect("db");
    let host = HostId::new();
    host_row(&mut db, host, "web");
    let task = new_task(host);
    db.create_agent_task(&task, T0).expect("task");
    let e = exec(task.id, host, "d");
    db.propose_execution(&e).expect("propose");
    let approval = ApprovalId::new();
    db.create_approval(&NewApproval {
        id: approval,
        execution: e.id,
        policy_version: "policy-v1".into(),
        now_ms: T0,
        expires_at_ms: T0 + 10,
    })
    .expect("ticket");
    assert!(db
        .decide_approval(approval, ApprovalDecision::Approve, T0 + 11)
        .is_err());
    let state = db.get_approval(approval).expect("get").expect("row").state;
    assert_eq!(
        state.as_str(),
        "expired",
        "expiry is recorded, not silently ignored"
    );
}

#[test]
fn the_audit_chain_verifies_and_detects_an_edited_row() {
    let mut db = Database::open_in_memory().expect("db");
    let host = HostId::new();
    host_row(&mut db, host, "web");
    let task = new_task(host);
    db.create_agent_task(&task, T0).expect("task");
    let e = exec(task.id, host, "d");
    db.propose_execution(&e).expect("propose");
    db.transition_execution(
        e.id,
        ExecStatus::Running,
        &ExecutionOutcome::default(),
        T0 + 5,
    )
    .expect("run");
    assert!(matches!(db.verify_audit_chain().expect("verify"), Ok(n) if n >= 3));

    db.connection_for_provider_test()
        .execute(
            "UPDATE audit_events SET sanitized_payload_json = '{\"forged\":true}' WHERE seq = 2",
            [],
        )
        .expect("tamper");
    assert!(db.verify_audit_chain().expect("verify").is_err());
}

#[test]
fn host_history_lists_only_that_host_newest_first_with_paging() {
    let mut db = Database::open_in_memory().expect("db");
    let (a, b) = (HostId::new(), HostId::new());
    host_row(&mut db, a, "alpha");
    host_row(&mut db, b, "bravo");
    let ta = new_task(a);
    let tb = new_task(b);
    db.create_agent_task(&ta, T0).expect("ta");
    db.create_agent_task(&tb, T0).expect("tb");
    for i in 0..3 {
        let mut e = exec(ta.id, a, &format!("d{i}"));
        e.now_ms = T0 + 10 + i;
        db.propose_execution(&e).expect("propose");
    }
    let mut other = exec(tb.id, b, "x");
    other.now_ms = T0 + 99;
    db.propose_execution(&other).expect("other host");

    let first = db
        .host_command_history(a, &PageRequest::first(2))
        .expect("page 1");
    assert_eq!(first.items.len(), 2);
    assert!(first.items.iter().all(|e| e.host == a));
    assert!(first.items[0].created_at_ms >= first.items[1].created_at_ms);
    let second = db
        .host_command_history(
            a,
            &PageRequest {
                cursor: first.next_cursor,
                limit: 2,
            },
        )
        .expect("page 2");
    assert_eq!(second.items.len(), 1);
    assert!(second.next_cursor.is_none());
}
