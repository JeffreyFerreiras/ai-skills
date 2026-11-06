#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{
        error::EngineError,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        run_context,
        source_capture::source_roots,
        state::GraphSession,
    },
    compat::{self, PyString, SerializationProfile, Value},
    infrastructure::source_files::NativeSourceFiles,
};
use initialized_graph::InitializedGraph;
use std::{fs, path::PathBuf};

fn fixture(approve: bool) -> InitializedGraph {
    let fixture = InitializedGraph::new();
    fixture
        .initialize(
            &fixture.graph.task("delivery", "full_delivery", &[]),
            approve,
        )
        .unwrap();
    fixture
}
fn coverage(fixture: &InitializedGraph) -> (PathBuf, Value) {
    let value = fixture
        .with_session(|session, context| {
            let loaded = run_context::load(session, &"RUN-1".into())?;
            let files = NativeSourceFiles::new(context.repository, &loaded.policy, context.files);
            let roots = source_roots(&files, &loaded.task["authority"]["capabilities"])?;
            let input = roots.as_list().unwrap().first().unwrap().clone();
            let checks = loaded.task["required_check_ids"]
                .as_list()
                .unwrap()
                .iter()
                .map(|id| {
                    Value::object([
                        ("check_id", id.clone()),
                        ("relevant_inputs", Value::List(vec![input.clone()])),
                        ("complete", true.into()),
                    ])
                })
                .collect();
            Ok(Value::object([
                ("schema_version", 1_i64.into()),
                ("kind", "check_coverage".into()),
                ("checks", Value::List(checks)),
            ]))
        })
        .unwrap();
    let inbox = fixture
        .graph
        .store
        .inbox_root("albanian-live-translate", "RUN-1");
    fs::create_dir_all(&inbox).unwrap();
    let path = inbox.join("coverage.json");
    fs::write(
        &path,
        compat::dumps(&value, SerializationProfile::STORED).unwrap(),
    )
    .unwrap();
    fixture.graph.store.platform.set_mode(&path, 0o600).unwrap();
    (path, value)
}
fn argv(path: &std::path::Path, operation: &str) -> Vec<PyString> {
    vec![
        "evidence".into(),
        "enable".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--contract-version".into(),
        "2".into(),
        "--coverage-manifest".into(),
        path.to_str().unwrap().into(),
        "--op-id".into(),
        operation.into(),
    ]
}
fn state_error(error: EngineError, code: &str) {
    assert!(matches!(error, EngineError::State(error) if error.code == code));
}

#[test]
fn activation_runs_real_schema7_status_claim_and_replay_without_source_capture() {
    let fixture = fixture(true);
    let (path, _) = coverage(&fixture);
    let arguments = argv(&path, "enable-one");
    let enabled = fixture.graphctl(&arguments).unwrap();
    assert!(enabled["code"].is_text("EVIDENCE_ENABLED"));
    assert!(enabled["state_revision"].py_eq(&3_i64.into()));
    fixture
        .with_session(|session, _| {
            let artifact = session
                .first(GraphRead::Artifact, &[enabled["artifact_ref"].clone()])?
                .unwrap();
            let activation = compat::loads_value(&artifact["content_json"])?;
            assert!(activation["kind"].is_text("evidence_activation"));
            assert!(activation["original_format"].py_eq(&6_i64.into()));
            assert!(activation["complete"].truthy());
            Ok(())
        })
        .unwrap();
    let status = fixture
        .graphctl(&["status".into(), "--run-id".into(), "RUN-1".into()])
        .unwrap();
    assert!(status["state_schema_version"].py_eq(&7_i64.into()));
    assert!(status["evidence_lifecycle"]["checks"]
        .as_list()
        .unwrap()
        .is_empty());
    assert!(status["evidence_lifecycle"]["reservations"]
        .as_list()
        .unwrap()
        .is_empty());
    assert!(status["next_action"]["kind"].is_text("claim"));
    let claimed = fixture
        .graphctl(&[
            "next".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--claim".into(),
            "--op-id".into(),
            "schema7-claim".into(),
        ])
        .unwrap();
    assert!(claimed["code"].is_text("CLAIMED"));
    assert!(claimed["state_revision"].py_eq(&4_i64.into()));
    let replayed = fixture.graphctl(&arguments).unwrap();
    assert!(replayed["code"].is_text("REPLAYED"));
    assert!(replayed["artifact_ref"].py_eq(&enabled["artifact_ref"]));
    state_error(
        fixture.graphctl(&argv(&path, "enable-two")).unwrap_err(),
        "EVIDENCE_ALREADY_ENABLED",
    );
    fixture.tear_down();
}

#[test]
fn coverage_requires_boolean_true_and_active_run() {
    let fixture = fixture(false);
    let (path, mut value) = coverage(&fixture);
    state_error(
        fixture
            .graphctl(&argv(&path, "enable-pending"))
            .unwrap_err(),
        // state.py:260 admits evidence.enable only while active, before its action.
        "INVALID_RUN_TRANSITION",
    );
    if let Value::List(checks) = &mut value["checks"].clone() {
        checks[0].set("complete", 1_i64.into());
        value.set("checks", Value::List(checks.clone()));
    }
    fs::write(
        &path,
        compat::dumps(&value, SerializationProfile::STORED).unwrap(),
    )
    .unwrap();
    assert!(
        matches!(fixture.graphctl(&argv(&path, "enable-invalid")), Err(EngineError::Contract(error)) if error.field.equals("coverage") && error.code == "COVERAGE_ATTESTATION_REQUIRED")
    );
    fixture
        .with_session(|session, _| {
            let run = session.run("RUN-1")?.unwrap().fields;
            assert!(run["state_schema_version"].py_eq(&6_i64.into()));
            assert!(run["state_revision"].py_eq(&1_i64.into()));
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn activation_fault_rolls_back_artifact_and_schema_change_and_inbox_check_precedes_op_id() {
    let fixture = fixture(true);
    let (path, _) = coverage(&fixture);
    let before = fixture
        .with_session(|session, _| {
            session.all(GraphRead::ArtifactsForValidation, &["RUN-1".into()])
        })
        .unwrap();
    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("activation fault".into()))
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
    let mut arguments = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
    ];
    arguments.extend(argv(&path, "enable-fault"));
    assert!(
        matches!(fixture.execute_with_store(&arguments, &store), Err(EngineError::Injected(message)) if message == "activation fault")
    );
    fixture
        .with_session(|session, _| {
            let run = session.run("RUN-1")?.unwrap().fields;
            assert!(run["state_schema_version"].py_eq(&6_i64.into()));
            assert!(run["state_revision"].py_eq(&2_i64.into()));
            assert!(session.operation("RUN-1", "enable-fault")?.is_none());
            assert!(Value::List(before.clone()).container_eq(&Value::List(
                session.all(GraphRead::ArtifactsForValidation, &["RUN-1".into()])?
            )));
            Ok(())
        })
        .unwrap();
    let outside = argv(&fixture.graph.repository.join("docs/task.json"), "invalid?");
    assert!(
        matches!(fixture.graphctl(&outside), Err(EngineError::Contract(error)) if error.field.equals("path") && error.code == "OUTSIDE_FIXED_INBOX")
    );
    fixture.tear_down();
}
