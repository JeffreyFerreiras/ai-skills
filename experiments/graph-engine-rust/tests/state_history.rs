use graph_engine::{
    application::{error::EngineError, state_history},
    compat::Value,
    infrastructure::sqlite::SqliteSession,
};

fn fixture() -> SqliteSession {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("
        CREATE TABLE nodes(branch_id TEXT,run_id TEXT,status TEXT,retry_count INTEGER,started_at TEXT,finished_at TEXT,envelope_json TEXT);
        CREATE TABLE branch_attempts(run_id TEXT,branch_id TEXT,attempt_number INTEGER,attempt_id TEXT,claim_digest TEXT,started_at TEXT,finished_at TEXT,outcome TEXT);
        CREATE TABLE operations(run_id TEXT,request_digest TEXT,response_json TEXT,resulting_revision INTEGER);
        CREATE TABLE budgets(run_id TEXT,budget_id TEXT,limit_value INTEGER,used INTEGER);
        CREATE TABLE budget_consumptions(run_id TEXT,budget_id TEXT,source_branch_id TEXT,amount INTEGER);
    ").unwrap();
    SqliteSession::from_connection(connection)
}

fn run() -> Value {
    Value::object([
        ("run_id", "RUN".into()),
        ("status", "active".into()),
        ("started_at", "2026-01-01T00:00:00Z".into()),
        ("finished_at", Value::Null),
        ("state_revision", 1i64.into()),
    ])
}

fn state(error: EngineError, expected: &str) {
    assert!(matches!(error, EngineError::State(error) if error.to_string()==expected));
}

fn node(session: &SqliteSession) {
    session.execute_values("INSERT INTO nodes VALUES('B','RUN','running',0,'2026-01-01T00:00:01Z',NULL,?)", &[
        "{\"started_at\":\"2026-01-01T00:00:01Z\",\"finished_at\":null,\"attempt_id\":\"ATTEMPT\",\"claim_digest\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}".into(),
    ]).unwrap();
    session.execute_values("INSERT INTO branch_attempts VALUES('RUN','B',1,'ATTEMPT',?,'2026-01-01T00:00:01Z',NULL,NULL)", &["a".repeat(64).into()]).unwrap();
}

#[test]
fn attempt_validation_checks_run_timestamp_before_any_database_access() {
    let session = SqliteSession::from_connection(rusqlite::Connection::open_in_memory().unwrap());
    let mut run = run();
    run.set("finished_at", "2025-12-31T23:59:59Z".into());
    state(
        state_history::validate_attempts(&session, &run).unwrap_err(),
        "TIMESTAMP_ORDER_INVALID",
    );
}

#[test]
fn running_attempt_rejects_reversed_time_and_preserves_envelope_binding() {
    let session = fixture();
    node(&session);
    state_history::validate_attempts(&session, &run()).unwrap();
    session
        .execute_values(
            "UPDATE branch_attempts SET finished_at='2026-01-01T00:00:00Z',outcome='failed'",
            &[],
        )
        .unwrap();
    state(
        state_history::validate_attempts(&session, &run()).unwrap_err(),
        "ATTEMPT_TIMESTAMP_INVALID",
    );
    session
        .execute_values(
            "UPDATE branch_attempts SET finished_at=NULL,outcome=NULL",
            &[],
        )
        .unwrap();
    session
        .execute_values("UPDATE nodes SET envelope_json='{}'", &[])
        .unwrap();
    state(
        state_history::validate_attempts(&session, &run()).unwrap_err(),
        "TIMESTAMP_STATE_INVALID",
    );
}

#[test]
fn attempt_fetchall_decodes_later_text_before_processing_first_attempt() {
    let session = fixture();
    node(&session);
    session
        .execute_values("UPDATE nodes SET retry_count=1", &[])
        .unwrap();
    session
        .execute_values("UPDATE branch_attempts SET started_at='invalid'", &[])
        .unwrap();
    session.execute_values("INSERT INTO branch_attempts VALUES('RUN','B',2,'SECOND',?,CAST(X'FF' AS TEXT),NULL,NULL)", &["b".repeat(64).into()]).unwrap();
    assert!(matches!(
        state_history::validate_attempts(&session, &run()).unwrap_err(),
        EngineError::Storage {
            kind: "OperationalError",
            ..
        }
    ));
    session
        .execute_values("DELETE FROM branch_attempts WHERE attempt_number=2", &[])
        .unwrap();
    state(
        state_history::validate_attempts(&session, &run()).unwrap_err(),
        "ATTEMPT_HISTORY_INVALID",
    );
    session
        .execute_values("UPDATE nodes SET retry_count=0", &[])
        .unwrap();
    state(
        state_history::validate_attempts(&session, &run()).unwrap_err(),
        "TIMESTAMP_INVALID",
    );
}

#[test]
fn operation_fetchall_and_json_catch_boundaries_remain_distinct() {
    let session = fixture();
    session
        .execute_values(
            "INSERT INTO operations VALUES('RUN',?,'not JSON',1)",
            &["a".repeat(64).into()],
        )
        .unwrap();
    session
        .execute_values(
            "INSERT INTO operations VALUES('RUN',?,CAST(X'FF' AS TEXT),2)",
            &["b".repeat(64).into()],
        )
        .unwrap();
    assert!(matches!(
        state_history::validate_operation_ledger(&session, &run()).unwrap_err(),
        EngineError::Storage {
            kind: "OperationalError",
            ..
        }
    ));
    session
        .execute_values("DELETE FROM operations WHERE resulting_revision=2", &[])
        .unwrap();
    state(
        state_history::validate_operation_ledger(&session, &run()).unwrap_err(),
        "OPERATION_LEDGER_INVALID",
    );
    session
        .execute_values("UPDATE operations SET response_json='null'", &[])
        .unwrap();
    assert!(
        matches!(state_history::validate_operation_ledger(&session, &run()).unwrap_err(), EngineError::Python {kind:"AttributeError",message} if message.equals("'NoneType' object has no attribute 'get'"))
    );
    session
        .execute_values("UPDATE operations SET response_json=NULL", &[])
        .unwrap();
    assert!(
        matches!(state_history::validate_operation_ledger(&session, &run()).unwrap_err(), EngineError::Json(error) if error.kind=="TypeError")
    );
}

#[test]
fn operation_ledger_accepts_encoded_blob_and_rejects_revision_gaps() {
    let session = fixture();
    let body = b"{\"state_revision\":1,\"run_id\":\"RUN\",\"schema_version\":true,\"ok\":true,\"code\":\"INITIALIZED\"}";
    session
        .execute_values(
            "INSERT INTO operations VALUES('RUN',?,?,1)",
            &["a".repeat(64).into(), Value::Bytes(body.to_vec())],
        )
        .unwrap();
    state_history::validate_operation_ledger(&session, &run()).unwrap();
    let mut run = run();
    run.set("state_revision", 2i64.into());
    state(
        state_history::validate_operation_ledger(&session, &run).unwrap_err(),
        "OPERATION_LEDGER_INVALID",
    );
}

#[test]
fn budget_consumptions_match_exact_totals_and_require_run_owned_nodes() {
    let session = fixture();
    let policy = Value::object([(
        "limits",
        Value::object([
            ("design_revisions", 2i64.into()),
            ("delivery_repairs", 3i64.into()),
        ]),
    )]);
    let task = Value::object([(
        "inspection_budget",
        Value::object([
            ("file_reads", 10i64.into()),
            ("discovery_commands", 4i64.into()),
        ]),
    )]);
    for (id, limit, used) in [
        ("design_revisions", 2, 1),
        ("delivery_repairs", 3, 0),
        ("file_reads", 10, 2),
        ("discovery_commands", 4, 0),
    ] {
        session
            .execute_values(
                "INSERT INTO budgets VALUES('RUN',?,?,?)",
                &[id.into(), i64::from(limit).into(), i64::from(used).into()],
            )
            .unwrap();
    }
    node(&session);
    session
        .execute_values(
            "INSERT INTO budget_consumptions VALUES('RUN','file_reads','B',2)",
            &[],
        )
        .unwrap();
    state_history::validate_budget_state(&session, &run(), &task, &policy).unwrap();
    session
        .execute_values("UPDATE budget_consumptions SET amount=3", &[])
        .unwrap();
    state(
        state_history::validate_budget_state(&session, &run(), &task, &policy).unwrap_err(),
        "BUDGET_STATE_INVALID",
    );
    session
        .execute_values(
            "UPDATE budgets SET used=3 WHERE budget_id='file_reads'",
            &[],
        )
        .unwrap();
    state_history::validate_budget_state(&session, &run(), &task, &policy).unwrap();
    session
        .execute_values(
            "UPDATE budget_consumptions SET source_branch_id='foreign'",
            &[],
        )
        .unwrap();
    state(
        state_history::validate_budget_state(&session, &run(), &task, &policy).unwrap_err(),
        "BUDGET_STATE_INVALID",
    );
}
