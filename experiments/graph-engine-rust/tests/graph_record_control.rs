#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{
        error::EngineError,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        state::GraphSession,
    },
    compat::{PyString, Value},
    domain::python_operations,
};
use initialized_graph::InitializedGraph;

fn initialize() -> InitializedGraph {
    let fixture = InitializedGraph::new();
    fixture
        .initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true)
        .unwrap();
    fixture
}
fn claim(fixture: &InitializedGraph) -> Value {
    fixture
        .graphctl(&[
            "next".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--claim".into(),
            "--op-id".into(),
            "record-claim".into(),
        ])
        .unwrap()
}
fn heartbeat(claim: &Value, operation: &str) -> Vec<PyString> {
    vec![
        "record".into(),
        "heartbeat".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--branch-id".into(),
        claim["branch"]["branch_id"].as_string().unwrap().clone(),
        "--attempt-id".into(),
        claim["branch"]["attempt_id"].as_string().unwrap().clone(),
        "--claim-token".into(),
        claim["branch"]["claim_token"].as_string().unwrap().clone(),
        "--op-id".into(),
        operation.into(),
    ]
}
fn budget(fixture: &InitializedGraph, operation: &str) -> Vec<PyString> {
    let status = fixture
        .graphctl(&["status".into(), "--run-id".into(), "RUN-1".into()])
        .unwrap();
    let budget = status["budgets"]
        .as_list()
        .unwrap()
        .iter()
        .find(|budget| python_operations::greater(&budget["limit_value"], &1_i64.into()).unwrap())
        .unwrap();
    vec![
        "record".into(),
        "budget-use".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--budget-id".into(),
        budget["budget_id"].as_string().unwrap().clone(),
        "--source-branch-id".into(),
        status["branches"].as_list().unwrap()[0]["branch_id"]
            .as_string()
            .unwrap()
            .clone(),
        "--amount".into(),
        "1".into(),
        "--op-id".into(),
        operation.into(),
    ]
}
fn state_error(error: EngineError, code: &str) {
    assert!(matches!(error, EngineError::State(error) if error.code == code));
}

#[test]
fn heartbeat_updates_the_lease_and_replays_before_terminal_run_validation() {
    let fixture = initialize();
    let claim = claim(&fixture);
    let argv = heartbeat(&claim, "heartbeat-one");
    let first = fixture.graphctl(&argv).unwrap();
    assert!(first["code"].is_text("HEARTBEAT_RECORDED"));
    assert!(first["state_revision"].py_eq(&4_i64.into()));
    fixture
        .graphctl(&[
            "abort".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--reason-code".into(),
            "STOP".into(),
            "--authority-ref".into(),
            "authority:test".into(),
            "--op-id".into(),
            "heartbeat-abort".into(),
        ])
        .unwrap();
    let mut replayed = first.clone();
    replayed.set("code", "REPLAYED".into());
    assert!(fixture.graphctl(&argv).unwrap().container_eq(&replayed));
    fixture.tear_down();
}

#[test]
fn heartbeat_fence_failure_preserves_revision_and_operation_ledger() {
    let fixture = initialize();
    let claim = claim(&fixture);
    let mut argv = heartbeat(&claim, "heartbeat-wrong-token");
    let token = argv
        .iter()
        .position(|value| value.equals("--claim-token"))
        .unwrap()
        + 1;
    argv[token] = "claim-wrong".into();
    state_error(
        fixture.graphctl(&argv).unwrap_err(),
        "ATTEMPT_FENCE_MISMATCH",
    );
    fixture
        .with_session(|session, _| {
            assert!(session.run("RUN-1")?.unwrap().fields["state_revision"].py_eq(&3_i64.into()));
            assert!(session
                .operation("RUN-1", "heartbeat-wrong-token")?
                .is_none());
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn heartbeat_before_commit_failure_rolls_back_its_envelope_write() {
    let fixture = initialize();
    let claim = claim(&fixture);
    let before = fixture
        .with_session(|session, _| {
            session.first(GraphRead::Node, &[claim["branch"]["branch_id"].clone()])
        })
        .unwrap()
        .unwrap();
    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("heartbeat fault".into()))
        } else {
            Ok(())
        }
    };
    let store = StateStore {
        state_root: fixture.graph.store.state_root.clone(),
        platform: fixture.graph.store.platform,
        database: fixture.graph.store.database,
        fault: &fault,
    };
    let mut argv = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
    ];
    argv.extend(heartbeat(&claim, "heartbeat-fault"));
    assert!(
        matches!(fixture.execute_with_store(&argv, &store), Err(EngineError::Injected(message)) if message == "heartbeat fault")
    );
    fixture
        .with_session(|session, _| {
            let after = session
                .first(GraphRead::Node, &[claim["branch"]["branch_id"].clone()])?
                .unwrap();
            assert!(before.container_eq(&after));
            assert!(session.operation("RUN-1", "heartbeat-fault")?.is_none());
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn budget_consumption_is_atomic_replayable_and_rejects_a_second_source_use() {
    let fixture = initialize();
    let argv = budget(&fixture, "budget-one");
    let first = fixture.graphctl(&argv).unwrap();
    assert!(first["code"].is_text("BUDGET_USE_RECORDED"));
    assert!(first["state_revision"].py_eq(&3_i64.into()));
    let mut replayed = first.clone();
    replayed.set("code", "REPLAYED".into());
    assert!(fixture.graphctl(&argv).unwrap().container_eq(&replayed));
    let mut duplicate = argv.clone();
    *duplicate.last_mut().unwrap() = "budget-two".into();
    state_error(
        fixture.graphctl(&duplicate).unwrap_err(),
        "BUDGET_CONSUMPTION_CONFLICT",
    );
    fixture.tear_down();
}

#[test]
fn budget_limit_precedes_branch_lookup_and_positive_validation_precedes_mutation() {
    let fixture = initialize();
    let mut argv: Vec<PyString> = [
        "record",
        "budget-use",
        "--run-id",
        "RUN-1",
        "--budget-id",
        "absent",
        "--source-branch-id",
        "absent",
        "--amount",
        "1",
        "--op-id",
        "budget-missing",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    state_error(fixture.graphctl(&argv).unwrap_err(), "BUDGET_LIMIT");
    argv[9] = "0".into();
    assert!(
        matches!(fixture.graphctl(&argv), Err(EngineError::Contract(error)) if error.field.equals("amount") && error.code == "POSITIVE_INTEGER_REQUIRED")
    );
    fixture
        .with_session(|session, _| {
            assert!(session.run("RUN-1")?.unwrap().fields["state_revision"].py_eq(&2_i64.into()));
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn huge_budget_amount_is_compared_before_any_sqlite_integer_binding() {
    let fixture = initialize();
    let mut argv = budget(&fixture, "budget-huge");
    let amount = argv
        .iter()
        .position(|value| value.equals("--amount"))
        .unwrap()
        + 1;
    argv[amount] = ("1".to_owned() + &"0".repeat(100)).into();
    state_error(fixture.graphctl(&argv).unwrap_err(), "BUDGET_LIMIT");
    fixture.tear_down();
}
