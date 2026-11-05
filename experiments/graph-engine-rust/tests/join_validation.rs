use graph_engine::{
    application::{error::EngineError, join_validation},
    compat::Value,
    infrastructure::sqlite::SqliteSession,
};

fn fixture() -> SqliteSession {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("
        CREATE TABLE nodes(branch_id TEXT PRIMARY KEY,status TEXT,retry_count INTEGER,max_retries INTEGER,result_json TEXT,result_digest TEXT,failure_code TEXT,reason_code TEXT);
        CREATE TABLE joins(run_id TEXT,join_id TEXT,status TEXT,kind TEXT,join_key TEXT);
        CREATE TABLE join_members(join_id TEXT,branch_id TEXT,mandatory INTEGER);
        CREATE TABLE artifacts(ref TEXT,sha256 TEXT,kind TEXT);
    ").unwrap();
    SqliteSession::from_connection(connection)
}

#[test]
fn run_join_validation_scopes_lookup_and_returns_ready_or_waiting_groups() {
    let session = fixture();
    session
        .execute_values(
            "INSERT INTO joins VALUES('RUN','JOIN','open','all_of','implementation')",
            &[],
        )
        .unwrap();
    member(&session, "A", "ready".into(), true, 0_i64.into(), 1_i64.into());
    let mut run = Value::object([("run_id", "RUN".into()), ("state_revision", 7_i64.into())]);
    let join_id = "JOIN".into();

    let waiting = join_validation::validate_run_join(&session, &run, &join_id).unwrap();
    assert!(waiting["code"].is_text("NOT_READY"));
    assert!(!waiting["ok"].truthy());
    assert!(waiting["groups"]["ready"].py_eq(&Value::List(vec!["A".into()])));
    assert!(waiting["state_revision"].py_eq(&run["state_revision"]));

    session
        .execute_values("UPDATE nodes SET status='succeeded' WHERE branch_id='A'", &[])
        .unwrap();
    let ready = join_validation::validate_run_join(&session, &run, &join_id).unwrap();
    assert!(ready["code"].is_text("READY"));
    assert!(ready["ok"].truthy());
    assert!(ready["groups"]["succeeded"].py_eq(&Value::List(vec!["A".into()])));
    assert!(ready["run_id"].py_eq(&run["run_id"]));
    assert!(ready["join_id"].py_eq(&join_id));
    assert!(ready["state_revision"].py_eq(&run["state_revision"]));

    run.set("run_id", "OTHER".into());
    let missing = join_validation::validate_run_join(&session, &run, &join_id).unwrap_err();
    assert!(matches!(missing, EngineError::State(error) if error.code == "JOIN_NOT_FOUND"));
}

fn join(key: &str, kind: &str) -> Value {
    Value::object([
        ("join_id", "JOIN".into()),
        ("status", "open".into()),
        ("join_key", key.into()),
        ("kind", kind.into()),
    ])
}

fn member(
    session: &SqliteSession,
    id: &str,
    status: Value,
    mandatory: bool,
    retry: Value,
    maximum: Value,
) {
    session
        .execute_values(
            "INSERT INTO nodes VALUES(?,?,?,?,NULL,NULL,NULL,NULL)",
            &[id.into(), status, retry, maximum],
        )
        .unwrap();
    session
        .execute_values(
            "INSERT INTO join_members VALUES('JOIN',?,?)",
            &[id.into(), i64::from(mandatory).into()],
        )
        .unwrap();
}

#[test]
fn sealed_join_returns_before_database_and_missing_members_are_counted() {
    let session = fixture();
    let mut value = join("implementation", "all_of");
    value.set("status", "sealed".into());
    let result = join_validation::validate_join(&session, &Value::Null, &value)
        .unwrap()
        .into_dictionary()
        .string_keyed_value()
        .unwrap();
    assert!(result["join_status"].is_text("INVALID_STATE"));
    assert!(result["groups"]["sealed"].py_eq(&Value::List(vec!["JOIN".into()])));
    value.set("status", "open".into());
    session
        .execute_values("INSERT INTO join_members VALUES('JOIN','missing',1)", &[])
        .unwrap();
    let result = join_validation::validate_join(&session, &Value::Null, &value)
        .unwrap()
        .into_dictionary()
        .string_keyed_value()
        .unwrap();
    assert!(result["groups"]["missing"].py_eq(&Value::List(vec!["JOIN".into()])));
}

#[test]
fn mandatory_skip_returns_only_groups_processed_so_far() {
    let session = fixture();
    for (id, status) in [("A", "succeeded"), ("B", "skipped"), ("C", "failed")] {
        member(&session, id, status.into(), true, 0i64.into(), 1i64.into());
    }
    let result = join_validation::validate_join(
        &session,
        &Value::Null,
        &join("delivery_collection", "collection"),
    )
    .unwrap()
    .into_dictionary()
    .string_keyed_value()
    .unwrap();
    assert!(result["join_status"].is_text("INVALID_STATE"));
    assert!(result["groups"].get("succeeded").is_some());
    assert!(result["groups"].get("skipped").is_some());
    assert!(result["groups"].get("failed").is_none());
}

#[test]
fn research_retry_list_evaluates_later_members_before_returning() {
    let session = fixture();
    member(
        &session,
        "A",
        "failed".into(),
        true,
        0i64.into(),
        1i64.into(),
    );
    member(
        &session,
        "B",
        "failed".into(),
        true,
        0i64.into(),
        "invalid".into(),
    );
    let result = join_validation::validate_join(
        &session,
        &Value::Null,
        &join("research_collection", "collection"),
    );
    assert!(
        matches!(result.unwrap_err(), EngineError::Python {kind:"TypeError",message} if message.equals("'<' not supported between instances of 'int' and 'str'"))
    );
}

#[test]
fn collections_and_all_of_joins_apply_distinct_retry_and_failure_rules() {
    let session = fixture();
    member(
        &session,
        "A",
        "failed".into(),
        true,
        0i64.into(),
        1i64.into(),
    );
    assert_eq!(
        join_validation::validate_join(
            &session,
            &Value::Null,
            &join("delivery_collection", "collection")
        )
        .unwrap()
        .join_status,
        "READY"
    );
    assert_eq!(
        join_validation::validate_join(&session, &Value::Null, &join("implementation", "all_of"))
            .unwrap()
            .join_status,
        "RETRY_REQUIRED"
    );
    session
        .execute_values("UPDATE nodes SET retry_count=1", &[])
        .unwrap();
    assert_eq!(
        join_validation::validate_join(&session, &Value::Null, &join("implementation", "all_of"))
            .unwrap()
            .join_status,
        "BLOCKED"
    );
    session
        .execute_values("UPDATE nodes SET status='running'", &[])
        .unwrap();
    assert_eq!(
        join_validation::validate_join(&session, &Value::Null, &join("implementation", "all_of"))
            .unwrap()
            .join_status,
        "NOT_READY"
    );
}

#[test]
fn collection_freezes_json_before_artifact_lookup_and_keeps_field_order() {
    let session = fixture();
    member(
        &session,
        "A",
        "succeeded".into(),
        true,
        0i64.into(),
        1i64.into(),
    );
    session
        .execute_values(
            "UPDATE nodes SET result_json='not JSON',result_digest=?",
            &["a".repeat(64).into()],
        )
        .unwrap();
    let members = join_validation::join_members(&session, &"JOIN".into()).unwrap();
    assert!(
        matches!(join_validation::canonical_collection_members(&session,&members).unwrap_err(), EngineError::Json(error) if error.kind=="JSONDecodeError")
    );
    session
        .execute_values("UPDATE nodes SET result_json='null'", &[])
        .unwrap();
    let members = join_validation::join_members(&session, &"JOIN".into()).unwrap();
    assert!(
        matches!(join_validation::canonical_collection_members(&session,&members).unwrap_err(),EngineError::State(error)if error.to_string()=="COLLECTION_ARTIFACT_INVALID")
    );
    session
        .execute_values(
            "INSERT INTO artifacts VALUES(?,?,'branch_result')",
            &[
                format!("ledger:A#sha256={}", "a".repeat(64)).into(),
                "a".repeat(64).into(),
            ],
        )
        .unwrap();
    let frozen = join_validation::canonical_collection_members(&session, &members).unwrap();
    let row = &frozen.as_list().unwrap()[0];
    assert!(row["result"].is_null());
    assert!(row["result_kind"].is_text("branch_result"));
    assert!(row["mandatory"].is_true());
    let keys = row
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, _)| key.utf8().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "branch_id",
            "mandatory",
            "status",
            "result_ref",
            "result_kind",
            "result_digest",
            "result",
            "failure_code",
            "reason_code"
        ]
    );
}
