#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_dispatch::{
            effective_code_review_assignment, ready_envelopes, review_dispatch_projection,
        },
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        state::GraphSession,
    },
    compat::{self, SerializationProfile, Value},
    domain::ids,
};
use initialized_graph::InitializedGraph;
use std::{cell::RefCell, collections::VecDeque};

struct Step {
    method: &'static str,
    query: GraphRead,
    parameters: Vec<Value>,
    rows: Vec<Value>,
}

struct Ledger(RefCell<VecDeque<Step>>);

impl Ledger {
    fn take(&self, method: &str, query: GraphRead, parameters: &[Value]) -> Vec<Value> {
        let step = self
            .0
            .borrow_mut()
            .pop_front()
            .expect("unexpected ledger call");
        assert_eq!(method, step.method);
        assert_eq!(
            std::mem::discriminant(&query),
            std::mem::discriminant(&step.query)
        );
        assert!(Value::List(parameters.to_vec()).container_eq(&Value::List(step.parameters)));
        step.rows
    }

    fn finished(&self) {
        assert!(self.0.borrow().is_empty());
    }
}

impl GraphLedgerReadPort for Ledger {
    fn first(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Option<Value>> {
        Ok(self.take("first", query, parameters).into_iter().next())
    }

    fn all(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        Ok(self.take("all", query, parameters))
    }

    fn visit(
        &self,
        _: GraphRead,
        _: &[Value],
        _: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        panic!("projection source uses first/all, not visit")
    }
}

fn json(bytes: &[u8]) -> Value {
    compat::loads(bytes).unwrap()
}

fn stored(value: &Value) -> Value {
    String::from_utf8(compat::dumps(value, SerializationProfile::STORED).unwrap())
        .unwrap()
        .into()
}

fn first(query: GraphRead, parameters: Vec<Value>, row: Value) -> Step {
    Step {
        method: "first",
        query,
        parameters,
        rows: vec![row],
    }
}

#[test]
fn ordinary_projection_copies_the_envelope_and_adds_only_the_claim_token() {
    let ledger = Ledger(RefCell::new(VecDeque::new()));
    let envelope = json(br#"{"branch_id":"B","inputs":[],"retained":{"value":1}}"#);
    let projected =
        review_dispatch_projection(&ledger, &envelope, Some(&"synthetic-token".into())).unwrap();
    assert!(projected["retained"].container_eq(&envelope["retained"]));
    assert!(projected["claim_token"].is_text("synthetic-token"));
    assert!(envelope.get("claim_token").is_none());
    assert_eq!(
        projected.as_object().unwrap().len(),
        envelope.as_object().unwrap().len() + 1
    );
    ledger.finished();
}

#[test]
fn authorized_review_skill_override_skips_only_json_decode_errors() {
    let envelope = json(br#"{"branch_id":"B","inputs":[{"kind":"evidence_manifest","ref":"bad"},{"kind":"evidence_manifest","ref":"authorization"}]}"#);
    let recovery = json(br#"{"kind":"recovery_authorization","branch_id":"B","required_skill":"review-agent","replacement_skill":"bug-hunter-review","decision":"APPROVE","equivalence_basis":"user-authorized-equivalent-review"}"#);
    let ledger = Ledger(RefCell::new(VecDeque::from([
        first(
            GraphRead::ImmutableArtifactContent,
            vec!["bad".into()],
            Value::object([("content_json", "{".into())]),
        ),
        first(
            GraphRead::ImmutableArtifactContent,
            vec!["authorization".into()],
            Value::object([("content_json", stored(&recovery))]),
        ),
    ])));
    let assignment = json(br#"{"required_skill":"review-agent","retained":true}"#);
    let projected = effective_code_review_assignment(&ledger, &envelope, &assignment).unwrap();
    assert!(projected["required_skill"].is_text("bug-hunter-review"));
    assert!(projected["retained"].truthy());
    assert!(projected["skill_override"].container_eq(&json(
        br#"{"from":"review-agent","to":"bug-hunter-review","authorization_ref":"authorization"}"#
    )));
    assert!(assignment["required_skill"].is_text("review-agent"));
    ledger.finished();
}

#[test]
fn malformed_utf8_artifact_is_not_silently_skipped_as_bad_json() {
    let ledger = Ledger(RefCell::new(VecDeque::from([first(
        GraphRead::ImmutableArtifactContent,
        vec!["bad".into()],
        Value::object([("content_json", Value::Bytes(vec![0xff]))]),
    )])));
    let result = effective_code_review_assignment(
        &ledger,
        &json(br#"{"inputs":[{"kind":"evidence_manifest","ref":"bad"}]}"#),
        &json(br#"{"required_skill":"review-agent"}"#),
    );
    assert!(
        matches!(&result, Err(EngineError::Json(error)) if error.kind == "UnicodeDecodeError")
            || matches!(
                &result,
                Err(EngineError::Python {
                    kind: "UnicodeDecodeError",
                    ..
                })
            )
    );
    ledger.finished();
}

#[test]
fn continuation_projection_retains_fences_and_exposes_only_review_context() {
    let envelope = json(br#"{"schema_version":1,"run_id":"R","branch_id":"B","node_instance_id":"N","node_key":"code_review","role":"code_reviewer","model":"gpt-6.1-sol","reasoning_effort":"high","mandatory":true,"generation":1,"status":"running","effect_capabilities":[{"effect":"filesystem_read","action":"read","target_ref":"repo:src/"},{"effect":"command","action":"run","target_ref":"command:test"}],"output_contract":{"kind":"review"},"review_continuation":{"round":1},"attempt_id":"attempt-1","claim_digest":"digest-1","lease_expires_at":"2026-11-03T12:00:00Z","inputs":[{"kind":"evidence","content":"omit"}],"unprojected":"omit"}"#);
    let task = json(br#"{"task_id":"T","acceptance_ids":["AC-1"],"request_mode":"delivery","minimum_route":"full_delivery","risk_level":"low"}"#);
    let plan = json(br#"{"conditional_review_assignments":[{"assignment_id":"A","scope_refs":["repo:src/"],"review_lens":"correctness","effect_capabilities":[{"effect":"filesystem_read","action":"read","target_ref":"repo:src/"},{"effect":"command","action":"run","target_ref":"command:test"}]}]}"#);
    let preliminary = json(br#"{"findings":[{"finding_id":"F"}],"evidence":[{"evidence_id":"E","kind":"reference","sha256":"sha","content":"omit"}]}"#);
    let request = json(br#"{"members":[{"assignment_id":"A","ordinal":1,"reason_code":"bounded","acceptance_ids":["AC-1"],"evidence_ids":["E"]}]}"#);
    let collection = json(br#"{"preliminary_findings":[{"finding_id":"F"}],"members":[{"branch_id":"C","assignment_id":"A","ordinal":1,"role":"review_helper","status":"succeeded","findings":[],"terminal":{"reason_code":"OK","failure_code":null,"result_digest":"result","unprojected":"omit"}}],"issues":[]}"#);
    let ledger = Ledger(RefCell::new(VecDeque::from([
        first(
            GraphRead::DispatchTask,
            vec!["R".into()],
            Value::object([("task_json", stored(&task))]),
        ),
        first(
            GraphRead::ExecutionPlanBody,
            vec!["R".into()],
            Value::object([("plan_json", stored(&plan))]),
        ),
        Step {
            method: "all",
            query: GraphRead::DispatchCollections,
            parameters: vec!["B".into(), 1_i64.into()],
            rows: vec![Value::object([
                ("request_slot_id", "S".into()),
                ("preliminary_ref", "P".into()),
                ("request_ref", "Q".into()),
                ("collection_digest", "collection".into()),
                ("content_json", stored(&collection)),
            ])],
        },
        first(
            GraphRead::ArtifactContent,
            vec!["P".into()],
            Value::object([("content_json", stored(&preliminary))]),
        ),
        first(
            GraphRead::ArtifactContent,
            vec!["Q".into()],
            Value::object([("content_json", stored(&request))]),
        ),
    ])));
    let projected =
        review_dispatch_projection(&ledger, &envelope, Some(&"synthetic-token".into())).unwrap();
    assert!(projected.get("unprojected").is_none());
    assert_eq!(projected["effect_capabilities"].as_list().unwrap().len(), 1);
    assert!(projected["attempt_id"].container_eq(&envelope["attempt_id"]));
    assert!(projected["claim_digest"].container_eq(&envelope["claim_digest"]));
    assert!(projected["lease_expires_at"].container_eq(&envelope["lease_expires_at"]));
    assert!(projected["started_at"].is_null());
    assert!(projected["max_retries"].py_eq(&0_i64.into()));
    let input = &projected["inputs"].as_list().unwrap()[0];
    let context = &input["content"];
    assert!(context["task_scope"].container_eq(&task));
    assert!(context["preliminary"].as_list().unwrap()[0]["evidence"]
        .as_list()
        .unwrap()[0]
        .container_eq(&json(
            br#"{"evidence_id":"E","kind":"reference","sha256":"sha"}"#
        )));
    let assignment = &context["assignments"].as_list().unwrap()[0];
    assert_eq!(
        assignment["effect_capabilities"].as_list().unwrap().len(),
        1
    );
    let member = &context["nested_collections"].as_list().unwrap()[0]["members"]
        .as_list()
        .unwrap()[0];
    assert!(member["reason_code"].is_text("OK"));
    assert!(member.get("terminal").is_none());
    assert!(member.get("unprojected").is_none());
    let bytes = ids::canonical_bytes(context).unwrap();
    let digest = ids::sha256_bytes(&bytes);
    assert!(input["sha256"].is_text(&digest));
    assert!(input["ref"].is_text(&format!("thread:review-context-{}", &digest[..24])));
    assert!(input["size_bytes"].py_eq(&(bytes.len() as i64).into()));
    ledger.finished();
}

#[test]
fn ready_projection_queries_the_actual_sqlite_adapter_in_branch_order() {
    let fixture = InitializedGraph::new();
    fixture
        .initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true)
        .unwrap();
    fixture.with_session(|session, _| {
        let run = session.run("RUN-1")?.unwrap().fields;
        let projected = ready_envelopes(session, &run)?;
        let rows = session.query_values("SELECT envelope_json FROM nodes WHERE run_id=? AND status='ready' ORDER BY branch_id", &["RUN-1".into()])?;
        assert!(!projected.is_empty());
        let expected = rows.iter().map(|row| Ok(compat::loads_value(&row["envelope_json"])?)).collect::<EngineResult<Vec<_>>>()?;
        assert!(Value::List(projected).container_eq(&Value::List(expected)));
        Ok(())
    }).unwrap();
    fixture.tear_down();
}
