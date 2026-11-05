#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{error::EngineError, persistent_store::StateStore},
    compat::{PyString, Value},
    domain::ids,
    infrastructure::policy_files::NativePolicyFiles,
};
use initialized_graph::InitializedGraph;
use std::fs;

fn initialize(required: bool) -> InitializedGraph {
    let fixture = InitializedGraph::new();
    let mut task = fixture.graph.task("delivery", "full_delivery", &[]);
    if required {
        task.set(
            "required_human_decisions",
            Value::List(vec!["HUMAN-1".into()]),
        );
    }
    fixture.initialize(&task, true).unwrap();
    fixture
}

fn arguments(fixture: &InitializedGraph, operation: &str) -> Vec<PyString> {
    let path = fixture
        .graph
        .repository
        .join("docs/artifacts/human-approval.json");
    let bytes = b"{\"schema_version\": 1, \"kind\": \"acceptance_evidence\"}";
    fs::write(path, bytes).unwrap();
    let digest = ids::sha256_bytes(bytes);
    vec![
        "record".into(),
        "approval".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--approval-id".into(),
        "HUMAN-1".into(),
        "--scope-ref".into(),
        format!("repo:docs/artifacts/human-approval.json#sha256={digest}").into(),
        "--decision".into(),
        "APPROVE".into(),
        "--authority-ref".into(),
        "authority:test".into(),
        "--artifact-sha256".into(),
        digest.into(),
        "--op-id".into(),
        operation.into(),
    ]
}

fn status(fixture: &InitializedGraph) -> Value {
    fixture
        .graphctl(&["status".into(), "--run-id".into(), "RUN-1".into()])
        .unwrap()
}

#[test]
fn human_approval_attestation_is_immutable_and_replay_precedes_terminal_state() {
    let fixture = initialize(true);
    let arguments = arguments(&fixture, "approval-one");
    let first = fixture.graphctl(&arguments).unwrap();
    assert!(first["code"].is_text("APPROVAL_RECORDED"));
    let approved = status(&fixture);
    let approvals = approved["approvals"].as_list().unwrap();
    assert_eq!(approvals.len(), 1);
    assert!(approvals[0]["approval_id"].is_text("HUMAN-1"));
    assert!(approvals[0]["decision"].is_text("APPROVE"));
    assert!(!approvals[0]["actor"].is_null());
    assert!(!approvals[0]["approved_at"].is_null());
    let mut same = arguments.clone();
    same[15] = "approval-two".into();
    fixture.graphctl(&same).unwrap();
    assert!(status(&fixture)["approvals"].container_eq(&approved["approvals"]));
    fixture
        .graphctl(&[
            "abort".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--reason-code".into(),
            "stop".into(),
            "--authority-ref".into(),
            "authority:test".into(),
            "--op-id".into(),
            "abort".into(),
        ])
        .unwrap();
    let mut expected = first;
    expected.set("code", "REPLAYED".into());
    assert!(fixture
        .graphctl(&arguments)
        .unwrap()
        .container_eq(&expected));
    fixture.tear_down();
}

#[test]
fn changed_human_decision_conflicts_and_preserves_the_revision() {
    let fixture = initialize(true);
    let mut arguments = arguments(&fixture, "approval-one");
    fixture.graphctl(&arguments).unwrap();
    let before = status(&fixture);
    arguments[9] = "REJECT".into();
    arguments[15] = "approval-conflict".into();
    assert!(
        matches!(fixture.graphctl(&arguments), Err(EngineError::State(error)) if error.code == "APPROVAL_CONFLICT")
    );
    let after = status(&fixture);
    assert!(after["state_revision"].py_eq(&before["state_revision"]));
    assert!(after["approvals"].container_eq(&before["approvals"]));
    fixture.tear_down();
}

#[test]
fn unexpected_approval_and_actor_mismatch_precede_evidence_access() {
    let fixture = initialize(false);
    let mut arguments = arguments(&fixture, "unexpected");
    fs::remove_file(
        fixture
            .graph
            .repository
            .join("docs/artifacts/human-approval.json"),
    )
    .unwrap();
    assert!(
        matches!(fixture.graphctl(&arguments), Err(EngineError::State(error)) if error.code == "UNEXPECTED_APPROVAL")
    );
    arguments.extend(["--actor".into(), "another-actor".into()]);
    assert!(
        matches!(fixture.graphctl(&arguments), Err(EngineError::State(error)) if error.code == "APPROVAL_ACTOR_MISMATCH")
    );
    assert!(status(&fixture)["state_revision"].py_eq(&2_i64.into()));
    fixture.tear_down();
}

#[test]
fn before_commit_failure_rolls_back_approval_and_attestation_together() {
    let fixture = initialize(true);
    let arguments = arguments(&fixture, "approval-fault");
    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("approval fault".into()))
        } else {
            Ok(())
        }
    };
    let faulting = StateStore::new(
        Some(&fixture.graph.store.state_root),
        None,
        &fixture.graph.skill_root,
        &NativePolicyFiles::default(),
        fixture.graph.store.platform,
        fixture.graph.store.database,
        &fault,
    )
    .unwrap();
    let mut argv = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
    ];
    argv.extend(arguments);
    assert!(
        matches!(fixture.execute_with_store(&argv, &faulting), Err(EngineError::Injected(message)) if message == "approval fault")
    );
    let after = status(&fixture);
    assert!(after["approvals"].as_list().unwrap().is_empty());
    assert!(after["state_revision"].py_eq(&2_i64.into()));
    fixture
        .with_session(|session, _| {
            assert!(session
                .query_values("SELECT * FROM approvals WHERE run_id=?", &["RUN-1".into()])?
                .is_empty());
            assert!(session
                .query_values(
                    "SELECT * FROM approval_attestations WHERE run_id=?",
                    &["RUN-1".into()],
                )?
                .is_empty());
            Ok(())
        })
        .unwrap();
    drop(faulting);
    fixture.tear_down();
}
