#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{
        artifacts::persist_artifact,
        check_eligibility::CheckEligibilityPort,
        error::{EngineError, EngineResult},
        evidence_status::EvidenceStatusPort,
        evidence_verification::EvidenceVerification,
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
        source_capture::{SourceFilePort, SourceMetadata, MAX_FILE_BYTES},
        state::GraphSession,
    },
    compat::{self, PyString, SerializationProfile, Value},
    domain::{evidence::canonical_ledger_artifact, ids},
    infrastructure::{check_runtime::NativeCheckRuntime, source_files::NativeSourceFiles},
};
use initialized_graph::InitializedGraph;
use std::{fs, time::Duration};

struct NoProcess;
impl ProcessPort for NoProcess {
    fn capture(&self, _: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome> {
        panic!("generated-output/fence tests must not invoke Git or configured commands")
    }
}

fn initialize() -> InitializedGraph {
    let fixture = InitializedGraph::new();
    fixture
        .initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true)
        .unwrap();
    fixture
}

fn with_verification<T>(
    fixture: &InitializedGraph,
    action: impl FnOnce(&EvidenceVerification<'_>) -> EngineResult<T>,
) -> EngineResult<T> {
    fixture.with_session(|session, context| {
        let run = session.run("RUN-1")?.unwrap().fields;
        let source_files =
            NativeSourceFiles::new(context.repository, context.policy, context.files);
        let clock = NativeCheckRuntime {
            repository: context.repository,
        };
        action(&EvidenceVerification {
            session,
            run: &run,
            context: *context,
            clock: &clock,
            source_files: &source_files,
            process: &NoProcess,
        })
    })
}

fn generated(fixture: &InitializedGraph) -> (Value, Value) {
    let bytes = b"{\"value\":1}";
    fs::write(
        fixture
            .graph
            .repository
            .join("docs/artifacts/generated.json"),
        bytes,
    )
    .unwrap();
    let digest = ids::sha256_bytes(bytes);
    let proposed = Value::object([
        (
            "ref",
            format!("repo:docs/artifacts/generated.json#sha256={digest}").into(),
        ),
        ("sha256", digest.into()),
        ("kind", "evidence_manifest".into()),
    ]);
    let output = Value::object([
        ("path", "docs/artifacts/generated.json".into()),
        ("producer_node_key", "impact_mapper".into()),
        ("artifact_kind", "evidence_manifest".into()),
        ("purpose", "artifact".into()),
    ]);
    (proposed, output)
}

#[test]
fn absent_output_is_skipped_before_producer_or_registration_fields_are_read() {
    let fixture = initialize();
    let output = Value::object([("path", "docs/artifacts/absent.json".into())]);
    with_verification(&fixture, |verification| {
        verification.verify_generated_outputs(&Value::List(vec![output]), Some(&1_i64.into()))
    })
    .unwrap();
    fixture.tear_down();
}

#[test]
fn existing_unregistered_output_fails_closed() {
    let fixture = initialize();
    let (_, output) = generated(&fixture);
    let result = with_verification(&fixture, |verification| {
        verification.verify_generated_outputs(&Value::List(vec![output]), None)
    });
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "GENERATED_OUTPUT_UNREGISTERED")
    );
    fixture.tear_down();
}

#[test]
fn matching_registration_resolves_current_bytes_and_later_calls_recheck_them() {
    let fixture = initialize();
    let (proposed, output) = generated(&fixture);
    let registering = Value::Tuple(vec!["impact_mapper".into(), proposed]);
    let outputs = Value::List(vec![output]);
    with_verification(&fixture, |verification| {
        verification.verify_generated_outputs(&outputs, Some(&registering))
    })
    .unwrap();
    fs::write(
        fixture
            .graph
            .repository
            .join("docs/artifacts/generated.json"),
        b"{\"value\":2}",
    )
    .unwrap();
    let result = with_verification(&fixture, |verification| {
        verification.verify_generated_outputs(&outputs, Some(&registering))
    });
    assert!(
        matches!(result, Err(EngineError::Contract(error)) if error.field.equals("artifact_ref") && error.code == "INPUT_DIGEST_MISMATCH")
    );
    fixture.tear_down();
}

#[test]
fn succeeded_producer_result_is_read_through_the_real_sqlite_projection() {
    let fixture = initialize();
    let (proposed, output) = generated(&fixture);
    fixture
        .with_session(|session, _| {
            let result = Value::object([("artifact_ref", proposed)]);
            let json =
                String::from_utf8(compat::dumps(&result, SerializationProfile::STORED)?).unwrap();
            // This source helper independently verifies artifacts; its fixture directly seeds the producer row.
            session.execute_values(
                "UPDATE nodes SET status='succeeded',result_json=? WHERE run_id=? AND node_key=?",
                &[json.into(), "RUN-1".into(), "impact_mapper".into()],
            )?;
            Ok(())
        })
        .unwrap();
    with_verification(&fixture, |verification| {
        verification.verify_generated_outputs(&Value::List(vec![output]), None)
    })
    .unwrap();
    fixture.tear_down();
}

#[test]
fn acceptance_wrapper_is_consulted_after_no_producer_matches() {
    let fixture = initialize();
    let (proposed, mut output) = generated(&fixture);
    output.set("purpose", "acceptance_wrapper".into());
    fixture.with_session(|session, _| {
        let wrapper = canonical_ledger_artifact(&"generated-wrapper".into(), &"acceptance_evidence".into(), &Value::object([("underlying_evidence", proposed)]))?;
        persist_artifact(session, "RUN-1", &wrapper)?;
        session.execute_values("INSERT INTO acceptance_evidence(run_id,criterion_id,artifact_ref,artifact_sha256) VALUES(?,?,?,?)", &["RUN-1".into(), "AC-001".into(), wrapper.reference.clone(), wrapper.sha256.clone()])?;
        Ok(())
    }).unwrap();
    with_verification(&fixture, |verification| {
        verification.verify_generated_outputs(&Value::List(vec![output]), None)
    })
    .unwrap();
    fixture.tear_down();
}

struct Oversize<'a>(&'a dyn SourceFilePort);
impl SourceFilePort for Oversize<'_> {
    fn normalize_target(&self, path: &PyString) -> EngineResult<PyString> {
        self.0.normalize_target(path)
    }
    fn metadata(&self, path: &PyString) -> EngineResult<Option<SourceMetadata>> {
        let mut metadata = self.0.metadata(path)?.unwrap();
        metadata.size = MAX_FILE_BYTES + 1;
        Ok(Some(metadata))
    }
    fn snapshot(&self, path: &PyString, maximum: usize) -> EngineResult<(usize, String)> {
        self.0.snapshot(path, maximum)
    }
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
    fn windows(&self) -> bool {
        self.0.windows()
    }
}

#[test]
fn oversized_output_fails_before_missing_producer_fields_are_read() {
    let fixture = initialize();
    generated(&fixture);
    let result = fixture.with_session(|session, context| {
        let run = session.run("RUN-1")?.unwrap().fields;
        let files = NativeSourceFiles::new(context.repository, context.policy, context.files);
        let source = Oversize(&files);
        let clock = NativeCheckRuntime {
            repository: context.repository,
        };
        EvidenceVerification {
            session,
            run: &run,
            context: *context,
            clock: &clock,
            source_files: &source,
            process: &NoProcess,
        }
        .verify_generated_outputs(
            &Value::List(vec![Value::object([(
                "path",
                "docs/artifacts/generated.json".into(),
            )])]),
            None,
        )
    });
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "SOURCE_UNVERIFIABLE")
    );
    fixture.tear_down();
}

#[test]
fn composed_fences_and_activation_use_the_ledger_and_fail_before_any_process() {
    let fixture = initialize();
    let fence = with_verification(&fixture, |verification| {
        verification.verify_fence(&Value::object([("branch_id", "missing".into())]), false)
    });
    assert!(
        matches!(fence, Err(EngineError::State(error)) if error.code == "ATTEMPT_FENCE_MISMATCH")
    );
    let source = with_verification(&fixture, |verification| {
        verification.capture_source(&Value::List(vec!["docs/".into()]), &Value::List(vec![]))
    });
    assert!(
        matches!(source, Err(EngineError::State(error)) if error.code == "EVIDENCE_ACTIVATION_INVALID")
    );
    with_verification(&fixture, |verification| {
        assert!(verification.current_review_binding()?.is_none());
        assert!(verification
            .reservation_states(verification.run)?
            .is_empty());
        Ok(())
    })
    .unwrap();
    fixture.tear_down();
}
