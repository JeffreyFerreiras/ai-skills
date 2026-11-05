use graph_engine::{
    application::{
        error::EngineError,
        state::{self, GraphSession, RunPatch},
    },
    compat::Value,
    domain::state::{validate_mutation_state, MUTATIONS},
    infrastructure::sqlite::SqliteSession,
};
use rusqlite::types::Value as SqlValue;
use std::cell::RefCell;

fn fixture(status: &str) -> SqliteSession {
    let session = SqliteSession::in_memory().unwrap();
    let mut statement = session
        .connection()
        .prepare("PRAGMA table_info(runs)")
        .unwrap();
    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let mut names = Vec::new();
    let mut values = Vec::new();
    for (name, kind, required, default) in columns {
        if name != "run_id" && (required == 0 || default.is_some()) {
            continue;
        }
        let value = match name.as_str() {
            "run_id" => SqlValue::Text("RUN-1".into()),
            "status" => SqlValue::Text(status.into()),
            "state_revision" => SqlValue::Integer(2),
            _ => {
                if kind == "INTEGER" {
                    SqlValue::Integer(0)
                } else {
                    SqlValue::Text("fixture".into())
                }
            }
        };
        names.push(name);
        values.push(value);
    }
    let sql = format!(
        "INSERT INTO runs({}) VALUES({})",
        names.join(","),
        vec!["?"; values.len()].join(",")
    );
    session
        .connection()
        .execute(&sql, rusqlite::params_from_iter(values))
        .unwrap();
    drop(statement);
    session
}
fn error_code(error: EngineError) -> String {
    match error {
        EngineError::State(error) => error.code,
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn semantic_after_sees_all_action_revision_operation_and_event_writes() {
    let session = fixture("active");
    let observations = RefCell::new(Vec::new());
    let result = state::mutate(
        &session,
        "RUN-1",
        "metadata-op",
        &Value::object([("command", "next.claim".into())]),
        |database, _, _| {
            database.patch_run("RUN-1", &RunPatch::BlockedReason(Some("temporary".into())))?;
            Ok(Value::object([("code", "RECORDED".into())]))
        },
        |database, run| {
            let revision = run.fields["state_revision"].as_i64().unwrap();
            let operation = database.operation("RUN-1", "metadata-op")?.is_some();
            let event = database.event_exists("RUN-1", "metadata-op")?;
            observations.borrow_mut().push((
                revision,
                run.fields["blocked_reason"].text(),
                operation,
                event,
            ));
            Ok(())
        },
        &|point: &str| {
            assert_eq!(point, "before_commit");
            let run = session.run("RUN-1")?.unwrap();
            assert_eq!(run.fields["state_revision"].as_i64(), Some(2));
            assert!(run.fields["blocked_reason"].is_text("temporary"));
            assert!(session.operation("RUN-1", "metadata-op")?.is_none());
            assert!(!session.event_exists("RUN-1", "metadata-op")?);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(result["state_revision"].as_i64(), Some(3));
    assert_eq!(
        *observations.borrow(),
        vec![
            (2, None, false, false),
            (3, Some("temporary".into()), true, true)
        ]
    );
    let response: String = session
        .connection()
        .query_row(
            "SELECT response_json FROM operations WHERE operation_id='metadata-op'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(response,"{\"code\": \"RECORDED\", \"ok\": true, \"run_id\": \"RUN-1\", \"schema_version\": 1, \"state_revision\": 3}");
    let detail: String = session
        .connection()
        .query_row(
            "SELECT detail_json FROM events WHERE source_id='metadata-op'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(detail, "{\"command\": \"next.claim\"}");
}
#[test]
fn semantic_after_failure_rolls_back_action_revision_operation_and_event() {
    let session = fixture("active");
    let calls = RefCell::new(0);
    let result = state::mutate(
        &session,
        "RUN-1",
        "rollback-op",
        &Value::object([("command", "next.claim".into())]),
        |database, _, _| {
            database.patch_run("RUN-1", &RunPatch::BlockedReason(Some("temporary".into())))?;
            Ok(Value::object([("code", "RECORDED".into())]))
        },
        |database, run| {
            *calls.borrow_mut() += 1;
            if *calls.borrow() == 2 {
                assert_eq!(run.fields["state_revision"].as_i64(), Some(3));
                assert!(database.operation("RUN-1", "rollback-op")?.is_some());
                assert!(database.event_exists("RUN-1", "rollback-op")?);
                return Err(EngineError::state("POST_ACTION_INVALID"));
            }
            Ok(())
        },
        &|_: &str| Ok(()),
    );
    assert_eq!(error_code(result.unwrap_err()), "POST_ACTION_INVALID");
    let run = session.run("RUN-1").unwrap().unwrap();
    assert_eq!(run.fields["state_revision"].as_i64(), Some(2));
    assert!(run.fields["blocked_reason"].is_null());
    assert!(session.operation("RUN-1", "rollback-op").unwrap().is_none());
    assert!(!session.event_exists("RUN-1", "rollback-op").unwrap());
}
#[test]
fn existing_before_commit_hook_has_old_metadata_and_failure_rolls_back() {
    let session = fixture("active");
    let calls = RefCell::new(0);
    let result = state::mutate(
        &session,
        "RUN-1",
        "fault-op",
        &Value::object([("command", "next.claim".into())]),
        |database, _, _| {
            database.patch_run("RUN-1", &RunPatch::BlockedReason(Some("temporary".into())))?;
            Ok(Value::object([("code", "RECORDED".into())]))
        },
        |_, _| {
            *calls.borrow_mut() += 1;
            Ok(())
        },
        &|point: &str| {
            assert_eq!(point, "before_commit");
            assert_eq!(
                session.run("RUN-1")?.unwrap().fields["state_revision"].as_i64(),
                Some(2)
            );
            assert!(session.operation("RUN-1", "fault-op")?.is_none());
            assert!(!session.event_exists("RUN-1", "fault-op")?);
            Err(EngineError::Injected("fault".into()))
        },
    );
    assert!(matches!(result, Err(EngineError::Injected(_))));
    assert_eq!(*calls.borrow(), 1);
    assert!(session.run("RUN-1").unwrap().unwrap().fields["blocked_reason"].is_null());
    assert!(session.operation("RUN-1", "fault-op").unwrap().is_none());
}
#[test]
fn replay_and_conflict_precede_state_matrix_and_skip_actions_and_validators_for_all_25_mutations() {
    assert_eq!(MUTATIONS.len(), 25);
    for (command, allowed) in MUTATIONS {
        let session = fixture(allowed[0]);
        let request = Value::object([("command", (*command).into())]);
        let first = state::mutate(
            &session,
            "RUN-1",
            "matrix-op",
            &request,
            |_, _, _| Ok(Value::object([("code", "RECORDED".into())])),
            |_, _| Ok(()),
            &|_: &str| Ok(()),
        )
        .unwrap();
        session
            .patch_run("RUN-1", &RunPatch::Status("complete".into()))
            .unwrap();
        let replay = state::mutate(
            &session,
            "RUN-1",
            "matrix-op",
            &request,
            |_, _, _| panic!("replay action ran"),
            |_, _| panic!("replay validator ran"),
            &|_: &str| panic!("replay fault hook ran"),
        )
        .unwrap();
        assert!(replay["code"].is_text("REPLAYED"));
        assert!(replay["state_revision"].py_eq(&first["state_revision"]));
        let different = Value::object([("command", (*command).into()), ("changed", true.into())]);
        let conflict = state::mutate(
            &session,
            "RUN-1",
            "matrix-op",
            &different,
            |_, _, _| panic!("conflict action ran"),
            |_, _| panic!("conflict validator ran"),
            &|_: &str| Ok(()),
        )
        .unwrap_err();
        assert_eq!(error_code(conflict), "OPERATION_CONFLICT");
    }
}
#[test]
fn all_actual_mutation_state_rules_and_unknown_commands_fail_in_source_order() {
    for (command, allowed) in MUTATIONS {
        for status in [
            "initialized",
            "active",
            "blocked",
            "complete",
            "aborted",
            "invalid",
        ] {
            let result = validate_mutation_state(command, status);
            if allowed.contains(&status) {
                assert!(result.is_ok());
            } else {
                assert_eq!(
                    result.unwrap_err().code,
                    match status {
                        "complete" | "aborted" => "TERMINAL_RUN",
                        "blocked" => "GRAPH_BLOCKED",
                        _ => "INVALID_RUN_TRANSITION",
                    }
                );
            }
        }
    }
    assert_eq!(
        validate_mutation_state("unknown", "complete")
            .unwrap_err()
            .code,
        "UNKNOWN_MUTATION"
    );
}
