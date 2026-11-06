#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{error::EngineError, graph_cli::ExecuteOutcome, state::GraphSession},
    compat::{PyString, Value},
};
use initialized_graph::InitializedGraph;
use std::fs;

fn execute(fixture: &InitializedGraph, arguments: &[&str]) -> (Value, i32) {
    let mut argv: Vec<PyString> = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
    ];
    argv.extend(arguments.iter().map(|argument| (*argument).into()));
    match fixture.execute(&argv).unwrap() {
        ExecuteOutcome::Response { result, exit_code } => (result, exit_code),
        other => panic!("unexpected parser outcome: {other:?}"),
    }
}

fn initialize(approve: bool) -> InitializedGraph {
    let fixture = InitializedGraph::new();
    fixture
        .initialize(
            &fixture.graph.task("delivery", "full_delivery", &[]),
            approve,
        )
        .unwrap();
    fixture
}

#[test]
fn readonly_next_reports_pending_approval_with_the_source_response_exit_code() {
    let fixture = initialize(false);
    let (result, exit_code) = execute(&fixture, &["next", "--run-id", "RUN-1"]);
    assert!(result["code"].is_text("EXECUTION_PLAN_APPROVAL_REQUIRED"));
    assert!(!result["ok"].truthy());
    assert!(result["execution_plan"]["status"].is_text("pending"));
    assert!(result["branches"].as_list().unwrap().is_empty());
    // execute's response mapping differs from main's StateError mapping for this code.
    assert_eq!(exit_code, 0);
    assert!(result["state_revision"].py_eq(&1_i64.into()));
    fixture.tear_down();
}

#[test]
fn ready_and_next_all_return_ordered_branches_without_mutating_state() {
    let fixture = initialize(true);
    let (ready, ready_exit) = execute(&fixture, &["ready", "--run-id", "RUN-1"]);
    let (all, all_exit) = execute(&fixture, &["next", "--run-id", "RUN-1", "--all"]);
    let (first, first_exit) = execute(&fixture, &["next", "--run-id", "RUN-1"]);
    assert_eq!((ready_exit, all_exit, first_exit), (0, 0, 0));
    assert!(ready["code"].is_text("READY"));
    assert!(ready["branches"].container_eq(&all["branches"]));
    let branches = ready["branches"].as_list().unwrap();
    assert!(!branches.is_empty());
    assert_eq!(first["branches"].as_list().unwrap().len(), 1);
    assert!(first["branches"].as_list().unwrap()[0].container_eq(&branches[0]));
    fixture
        .with_session(|session, _| {
            let run = session.run("RUN-1")?.unwrap().fields;
            assert!(run["state_revision"].py_eq(&2_i64.into()));
            assert!(run["status"].is_text("active"));
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn readonly_next_after_abort_returns_not_ready_and_exit_two() {
    let fixture = initialize(true);
    let (aborted, exit_code) = execute(
        &fixture,
        &[
            "abort",
            "--run-id",
            "RUN-1",
            "--reason-code",
            "STOP",
            "--authority-ref",
            "authority:test",
            "--op-id",
            "abort-readonly",
        ],
    );
    assert_eq!(exit_code, 0);
    assert!(aborted["code"].is_text("ABORTED"));
    let (result, exit_code) = execute(&fixture, &["next", "--run-id", "RUN-1"]);
    assert!(result["code"].is_text("NOT_READY"));
    assert!(!result["ok"].truthy());
    assert!(result["branches"].as_list().unwrap().is_empty());
    assert_eq!(exit_code, 2);
    fixture.tear_down();
}

#[test]
fn claim_without_operation_is_rejected_before_any_mutation() {
    let fixture = initialize(true);
    let result = fixture.graphctl(&[
        "next".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--claim".into(),
    ]);
    assert!(
        matches!(result, Err(EngineError::Contract(error)) if error.field.equals("op_id") && error.code == "MISSING_FIELD")
    );
    fixture
        .with_session(|session, _| {
            let run = session.run("RUN-1")?.unwrap().fields;
            assert!(run["state_revision"].py_eq(&2_i64.into()));
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn terminal_readonly_next_still_revalidates_current_task_bytes() {
    let fixture = initialize(true);
    execute(
        &fixture,
        &[
            "abort",
            "--run-id",
            "RUN-1",
            "--reason-code",
            "STOP",
            "--authority-ref",
            "authority:test",
            "--op-id",
            "abort-freshness",
        ],
    );
    let path = fixture.graph.repository.join("docs/task.json");
    let bytes = fs::read(&path).unwrap();
    let mut changed = bytes.clone();
    changed.push(b' ');
    fs::write(&path, changed).unwrap();
    let result = fixture.graphctl(&["next".into(), "--run-id".into(), "RUN-1".into()]);
    fs::write(&path, bytes).unwrap();
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "INPUT_DIGEST_MISMATCH")
    );
    fixture.tear_down();
}
