#[path = "../src/suite/task_fixture.rs"]
mod task_fixture;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        semantic_nodes::validate_nodes,
    },
    compat::{self, SerializationProfile, Value},
    domain::{execution, hosts, ids, planner},
};
use task_fixture::TaskFixture;

fn serialized(value: &Value) -> Value {
    String::from_utf8(compat::dumps(value, SerializationProfile::CLI).unwrap())
        .unwrap()
        .into()
}

struct Fixture {
    run: Value,
    task: Value,
    policy: Value,
    plan: Value,
    node: Value,
    envelope: Value,
    artifacts: Vec<(Value, Value)>,
}

impl Fixture {
    fn new() -> Self {
        let source = TaskFixture::new();
        let task = source.default_task();
        let plan = execution::build_execution_plan("R", &task, None, hosts::DEFAULT_HOST).unwrap();
        let spec = planner::bootstrap(&source.policy).unwrap();
        let envelope = planner::envelope(
            "R",
            &source.policy_digest,
            &source.policy,
            &task,
            &spec,
            "ready",
            &[],
            0,
            Some(&plan),
            cfg!(windows),
        )
        .unwrap();
        let node = Value::object([
            ("run_id", "R".into()),
            ("node_instance_id", envelope["node_instance_id"].clone()),
            ("branch_id", envelope["branch_id"].clone()),
            ("node_key", spec.key.into()),
            ("role", spec.role.into()),
            ("stage", spec.stage.into()),
            ("generation", 0_i64.into()),
            ("mandatory", 1_i64.into()),
            ("status", "ready".into()),
            ("retry_count", 0_i64.into()),
            ("max_retries", envelope["max_retries"].clone()),
            ("specialist_tag", Value::Null),
            ("depth", 0_i64.into()),
            ("parent_branch_id", Value::Null),
            ("request_slot_id", Value::Null),
            ("assignment_id", Value::Null),
            ("ordinal", Value::Null),
            ("failure_code", Value::Null),
            ("result_json", Value::Null),
            ("result_digest", Value::Null),
        ]);
        Self {
            run: Value::object([
                ("run_id", "R".into()),
                ("policy_digest", source.policy_digest.into()),
                (
                    "engine_version",
                    graph_engine::PHASE_DISPOSITIONS_ENGINE_VERSION.into(),
                ),
            ]),
            task,
            policy: source.policy,
            plan,
            node,
            envelope,
            artifacts: Vec::new(),
        }
    }

    fn verify(&self) -> EngineResult<()> {
        validate_nodes(self, &self.run, &self.task, &self.policy, cfg!(windows))
    }

    fn expect(&self, code: &str) {
        match self.verify() {
            Err(EngineError::State(error)) => assert_eq!(error.code, code),
            result => panic!("expected {code}, got {result:?}"),
        }
    }
}

impl GraphLedgerReadPort for Fixture {
    fn first(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Option<Value>> {
        Ok(match query {
            GraphRead::ExecutionPlanBody => {
                Some(Value::object([("plan_json", serialized(&self.plan))]))
            }
            GraphRead::Artifact | GraphRead::ImmutableArtifact => self
                .artifacts
                .iter()
                .find(|(reference, record)| {
                    reference.py_eq(&parameters[0])
                        && (!matches!(query, GraphRead::ImmutableArtifact)
                            || record["immutable"].py_eq(&1_i64.into()))
                })
                .map(|(_, record)| record.clone()),
            _ => panic!("unexpected query {query:?}"),
        })
    }

    fn all(&self, _: GraphRead, _: &[Value]) -> EngineResult<Vec<Value>> {
        panic!("node validation streams rows")
    }

    fn visit(
        &self,
        query: GraphRead,
        parameters: &[Value],
        visit: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        assert!(matches!(query, GraphRead::NodesForValidation));
        assert!(parameters[0].is_text("R"));
        let mut node = self.node.clone();
        if node.get("envelope_json").is_none() {
            node.set("envelope_json", serialized(&self.envelope));
        }
        visit(node)
    }
}

#[test]
fn node_identity_binding_state_and_json_errors_keep_source_precedence() {
    let mut fixture = Fixture::new();
    fixture.verify().unwrap();
    let branch = fixture.node["branch_id"].clone();
    fixture.node.set("branch_id", "wrong".into());
    fixture.node.set("envelope_json", "{".into());
    fixture.expect("STABLE_ID_INVALID");
    fixture.node.set("branch_id", branch);
    fixture.node.set("retry_count", (-1_i64).into());
    fixture.expect("BRANCH_STATE_INVALID");
    fixture.node.set("retry_count", 0_i64.into());
    fixture.expect("ENVELOPE_INVALID");
    fixture.node.remove("envelope_json");
    fixture.node.set("failure_code", "changed".into());
    fixture.expect("ENVELOPE_STATE_INVALID");
}

#[test]
fn immutable_envelope_fields_and_exact_key_set_are_reconstructed() {
    let mut fixture = Fixture::new();
    let model = fixture.envelope["model"].clone();
    fixture.envelope.set("model", "different".into());
    fixture.expect("ENVELOPE_INVALID");
    fixture.envelope.set("model", model);
    fixture.envelope.set("extra", true.into());
    fixture.expect("ENVELOPE_INVALID");
    fixture.envelope.remove("extra");
    fixture.envelope.set(
        "started_at",
        "the node validator does not bind this mutable field".into(),
    );
    fixture.verify().unwrap();
}

#[test]
fn running_fence_fields_are_checked_before_input_artifacts() {
    let mut fixture = Fixture::new();
    fixture.node.set("status", "running".into());
    fixture.envelope.set("status", "running".into());
    fixture.envelope.set("attempt_id", "attempt-1".into());
    fixture.envelope.set("claim_digest", "a".repeat(64).into());
    fixture
        .envelope
        .set("lease_expires_at", "2026-11-02T12:00:00Z".into());
    fixture.verify().unwrap();
    fixture.envelope.set("claim_digest", "g".repeat(64).into());
    fixture.envelope.set(
        "inputs",
        Value::list([Value::object([
            ("kind", "task_brief".into()),
            ("ref", "missing".into()),
            ("sha256", "a".repeat(64).into()),
            ("size_bytes", 1_i64.into()),
        ])]),
    );
    fixture.expect("ATTEMPT_STATE_INVALID");
    fixture.envelope.set("claim_digest", "a".repeat(64).into());
    fixture.envelope.set("lease_expires_at", "bad".into());
    fixture.expect("ATTEMPT_STATE_INVALID");
    fixture
        .envelope
        .set("lease_expires_at", "2026-11-02T12:00:00Z".into());
    fixture.expect("INPUT_ARTIFACT_INVALID");
}

#[test]
fn inline_input_content_requires_the_exact_registered_ledger_artifact() {
    let mut fixture = Fixture::new();
    let content = Value::object([("x", 1_i64.into())]);
    let digest = "b".repeat(64);
    let input = Value::object([
        ("kind", "task_brief".into()),
        ("ref", "ledger:input".into()),
        ("sha256", digest.clone().into()),
        ("size_bytes", 7_i64.into()),
        ("content", content.clone()),
    ]);
    fixture.envelope.set("inputs", Value::list([input]));
    fixture.artifacts.push((
        "ledger:input".into(),
        Value::object([
            ("kind", "task_brief".into()),
            ("sha256", digest.into()),
            ("size_bytes", 7_i64.into()),
            ("source_type", "ledger".into()),
            ("content_json", serialized(&content)),
        ]),
    ));
    fixture.verify().unwrap();
    fixture.artifacts[0].1.set("source_type", "file".into());
    fixture.expect("INPUT_ARTIFACT_INVALID");
    fixture.artifacts[0].1.set("source_type", "ledger".into());
    fixture.artifacts[0]
        .1
        .set("content_json", "{\"x\":2}".into());
    fixture.expect("INPUT_ARTIFACT_INVALID");
}

#[test]
fn sqlite_numeric_generation_is_not_narrowed_before_digest_or_envelope_comparison() {
    let mut fixture = Fixture::new();
    let generation = Value::Float(0.5);
    let name = format!(
        "{}@{}",
        fixture.node["node_key"].text().unwrap(),
        fixture.node["stage"].text().unwrap()
    );
    fixture.node.set("generation", generation.clone());
    fixture.envelope.set("generation", generation.clone());
    for (entity, field) in [("branch", "branch_id"), ("node", "node_instance_id")] {
        let identity = ids::stable_id_with_tag_value(
            &fixture.run["run_id"],
            &fixture.run["policy_digest"],
            entity,
            &name,
            &generation,
            &Value::Null,
        )
        .unwrap();
        fixture.node.set(field, identity.clone().into());
        fixture.envelope.set(field, identity.into());
    }
    fixture.verify().unwrap();
}

#[test]
fn invalid_specialist_tag_reaches_canonical_serialization_before_tag_binding() {
    let mut fixture = Fixture::new();
    fixture.node.set("specialist_tag", Value::Bytes(vec![255]));
    assert!(matches!(fixture.verify(), Err(EngineError::Json(error)) if error.kind == "TypeError"));
}

#[test]
fn terminal_control_requires_canonical_digest_immutable_wrapper_and_output_binding() {
    let mut fixture = Fixture::new();
    fixture.run.remove("engine_version");
    fixture.node.set("status", "skipped".into());
    fixture.envelope.set("status", "skipped".into());
    let result = Value::object([("kind", "skip".into())]);
    let digest = ids::sha256_bytes(&ids::canonical_bytes(&result).unwrap());
    fixture.node.set("result_json", serialized(&result));
    fixture.node.set("result_digest", digest.clone().into());
    let ledger = format!(
        "ledger:{}#sha256={digest}",
        fixture.node["branch_id"].text().unwrap()
    );
    let output = Value::object([
        ("kind", "skip".into()),
        ("sha256", digest.clone().into()),
        ("immutable", 1_i64.into()),
    ]);
    fixture.artifacts.push((ledger.clone().into(), output));
    fixture.envelope.set(
        "artifact_ref",
        Value::object([
            ("kind", "skip".into()),
            ("ref", ledger.into()),
            ("sha256", digest.into()),
        ]),
    );
    fixture.verify().unwrap();
    fixture.artifacts[0].1.set("immutable", 0_i64.into());
    fixture.expect("TERMINAL_RESULT_INVALID");
    fixture.artifacts[0].1.set("immutable", 1_i64.into());
    fixture
        .node
        .set("result_json", "{\"kind\":\"timeout\"}".into());
    fixture.expect("TERMINAL_RESULT_INVALID");
}

#[test]
fn historical_version_is_read_only_inside_ordinary_result_validation() {
    let mut fixture = Fixture::new();
    fixture.node.set("status", "failed".into());
    fixture.envelope.set("status", "failed".into());
    fixture.envelope.set("attempt_id", "ATTEMPT".into());
    fixture.envelope.set("claim_digest", "a".repeat(64).into());
    let result = Value::object([]);
    let digest = ids::sha256_bytes(&ids::canonical_bytes(&result).unwrap());
    fixture.node.set("result_json", serialized(&result));
    fixture.node.set("result_digest", digest.clone().into());
    let reference = format!(
        "ledger:{}#sha256={digest}",
        fixture.node["branch_id"].text().unwrap()
    );
    fixture.artifacts.push((
        reference.into(),
        Value::object([
            ("kind", "failure".into()),
            ("sha256", digest.into()),
            ("immutable", 1_i64.into()),
        ]),
    ));
    fixture.run.remove("engine_version");
    assert!(
        matches!(fixture.verify(), Err(EngineError::Python { kind: "KeyError", message }) if message.equals("'engine_version'"))
    );
    fixture.run.set("engine_version", "invalid".into());
    fixture.expect("TERMINAL_RESULT_INVALID");
}
