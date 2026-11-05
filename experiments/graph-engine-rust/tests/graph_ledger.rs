#[path = "support/store.rs"]
mod store_fixture;

use graph_engine::{
    application::{
        error::EngineError,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        run_context,
    },
    compat::{self, SerializationProfile, Value},
    infrastructure::{
        sqlite::SqliteSession, state_database::NativeStateDatabase,
        state_platform::NativeStatePlatform,
    },
};

fn initialized(label: &str) -> (store_fixture::Fixture, SqliteSession) {
    let fixture = store_fixture::Fixture::new(label);
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let fault = |_: &str| Ok(());
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &fault).unwrap();
    store.initialize(&fixture.request()).unwrap();
    let session =
        SqliteSession::connect_existing(&store.db_path("fixture-repository", "RUN-1")).unwrap();
    (fixture, session)
}

#[test]
fn graph_context_loads_persisted_documents_and_reports_missing_run() {
    let (fixture, session) = initialized("graph-context");
    let context = run_context::load(&session, &Value::from("RUN-1")).unwrap();
    for (actual, expected) in [
        (&context.policy, &fixture.policy),
        (&context.task, &fixture.task),
    ] {
        assert_eq!(
            compat::dumps(actual, SerializationProfile::CANONICAL).unwrap(),
            compat::dumps(expected, SerializationProfile::CANONICAL).unwrap()
        );
    }
    assert!(context.run.fields["run_id"].is_text("RUN-1"));
    let error = run_context::load(&session, &Value::from("absent"))
        .err()
        .unwrap();
    assert!(matches!(&error, EngineError::State(_)));
    assert!(format!("{error:?}").contains("RUN_NOT_FOUND"));
}

#[test]
fn graph_rows_preserve_fetchone_fetchall_and_callback_error_order() {
    let (_fixture, session) = initialized("graph-row-order");
    session.execute_values(
        "INSERT INTO operations(run_id,operation_id,request_digest,response_json,resulting_revision) VALUES('RUN-1','later',?,CAST(X'FF' AS TEXT),2)",
        &["b".repeat(64).into()],
    ).unwrap();
    let parameters = [Value::from("RUN-1")];
    let first = session
        .first(GraphRead::OperationResponses, &parameters)
        .unwrap()
        .unwrap();
    assert!(first["response_json"].text().is_some());
    let failure = session
        .all(GraphRead::OperationResponses, &parameters)
        .unwrap_err();
    assert!(matches!(
        failure,
        EngineError::Storage {
            kind: "OperationalError",
            ..
        }
    ));
    let mut callbacks = 0;
    let failure = session
        .visit(GraphRead::OperationResponses, &parameters, &mut |row| {
            callbacks += 1;
            assert_eq!(row["response_json"].text(), first["response_json"].text());
            Err(EngineError::Injected("first-row callback".into()))
        })
        .unwrap_err();
    assert!(matches!(failure, EngineError::Injected(message) if message == "first-row callback"));
    assert_eq!(callbacks, 1);
}

#[test]
fn topology_history_parses_in_revision_order_and_rejects_bad_json_before_later_bad_text() {
    let (_fixture, session) = initialized("topology-history-order");
    let responses =
        graph_engine::application::topology_history::operation_responses(&session, &"RUN-1".into())
            .unwrap();
    assert_eq!(responses.len(), 1);
    assert!(responses[0]["run_id"].is_text("RUN-1"));
    session.execute_values("INSERT INTO operations(run_id,operation_id,request_digest,response_json,resulting_revision) VALUES('RUN-1','null-first',?,'null',0)",&["b".repeat(64).into()]).unwrap();
    let responses =
        graph_engine::application::topology_history::operation_responses(&session, &"RUN-1".into())
            .unwrap();
    assert!(matches!(&responses[0], Value::Null));
    session.execute_values("INSERT INTO operations(run_id,operation_id,request_digest,response_json,resulting_revision) VALUES('RUN-1','bad-json-first',?,'not JSON',-1)",&["c".repeat(64).into()]).unwrap();
    session.execute_values("INSERT INTO operations(run_id,operation_id,request_digest,response_json,resulting_revision) VALUES('RUN-1','bad-text-later',?,CAST(X'FF' AS TEXT),3)",&["d".repeat(64).into()]).unwrap();
    let failure =
        graph_engine::application::topology_history::operation_responses(&session, &"RUN-1".into())
            .unwrap_err();
    assert!(
        matches!(failure,EngineError::State(error)if error.to_string()=="TOPOLOGY_HISTORY_INVALID")
    );
}
