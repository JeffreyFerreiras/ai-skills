use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        review_binding::validate_review_result_binding,
    },
    compat::{self, SerializationProfile, Value},
};
use std::cell::Cell;

fn json(text: &str) -> Value {
    compat::loads(text.as_bytes()).unwrap()
}

struct Fixture {
    content: Value,
    envelope: Value,
    result: Value,
    queries: Cell<usize>,
}

impl Fixture {
    fn new() -> Self {
        let continuation = json(
            r#"{
            "review_request_slots":[["S","D"]],
            "child_members":[["S","B","A",1]],
            "terminal_non_successes":[{"request_slot_id":"S","branch_id":"B","status":"failed","reason_code":"RC","failure_code":"FC"}],
            "finding_sources":[["S","I","B","F","A",1]]
        }"#,
        );
        let mut result = continuation.clone();
        result.set("findings", json(r#"[{"finding_id":"F"}]"#));
        Self {
            content: json(
                r#"{
                "members":[{"branch_id":"B","assignment_id":"A","ordinal":1,"status":"failed","terminal":{"reason_code":"RC","failure_code":"FC"}}],
                "issues":[{"issue_key":"I","canonical_branch_id":"B","provenance":[{"branch_id":"B","finding_id":"F","assignment_id":"A","ordinal":1}]}]
            }"#,
            ),
            envelope: Value::object([
                ("branch_id", "P".into()),
                ("generation", 2_i64.into()),
                ("review_continuation", continuation),
            ]),
            result,
            queries: Cell::new(0),
        }
    }

    fn verify(&self) -> EngineResult<()> {
        validate_review_result_binding(self, &self.envelope, &self.result)
    }

    fn expect(&self, code: &str) {
        match self.verify() {
            Err(EngineError::State(error)) => assert_eq!(error.code, code),
            result => panic!("expected {code}, got {result:?}"),
        }
    }
}

impl GraphLedgerReadPort for Fixture {
    fn first(&self, _: GraphRead, _: &[Value]) -> EngineResult<Option<Value>> {
        panic!("the binding validator fetches one ordered collection list")
    }

    fn all(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        assert!(matches!(query, GraphRead::SealedReviewCollections));
        assert!(parameters[0].is_text("P"));
        assert!(parameters[1].py_eq(&2_i64.into()));
        self.queries.set(self.queries.get() + 1);
        Ok(vec![Value::object([
            ("request_slot_id", "S".into()),
            ("collection_digest", "D".into()),
            (
                "content_json",
                String::from_utf8(compat::dumps(&self.content, SerializationProfile::CLI).unwrap())
                    .unwrap()
                    .into(),
            ),
        ])])
    }

    fn visit(
        &self,
        _: GraphRead,
        _: &[Value],
        _: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        panic!("the binding validator does not stream collections")
    }
}

#[test]
fn absent_continuation_does_not_read_collections_or_validate_unrelated_result() {
    let mut fixture = Fixture::new();
    fixture.envelope.remove("review_continuation");
    fixture.result = 42_i64.into();
    fixture.verify().unwrap();
    assert_eq!(fixture.queries.get(), 0);
}

#[test]
fn binding_preserves_slots_members_non_success_terminal_and_finding_provenance() {
    let mut fixture = Fixture::new();
    fixture.verify().unwrap();
    fixture.result.set("terminal_non_successes", json("[]"));
    fixture.expect("DELEGATION_BINDING_INCOMPLETE");
    fixture.result = fixture.envelope["review_continuation"].clone();
    fixture.result.set("findings", json("[]"));
    fixture.expect("DELEGATION_DISPOSITION_MISSING");
}

#[test]
fn duplicate_source_identity_is_rejected_after_exact_binding_matches() {
    let mut fixture = Fixture::new();
    fixture.content = json(
        r#"{
        "members":[{"branch_id":"B","assignment_id":"A","ordinal":1,"status":"failed","terminal":{"reason_code":"RC","failure_code":"FC"}}],
        "issues":[{"issue_key":"I","canonical_branch_id":"B","provenance":[
            {"branch_id":"B","finding_id":"F","assignment_id":"A","ordinal":1},
            {"branch_id":"B","finding_id":"F","assignment_id":"A","ordinal":1}
        ]}]
    }"#,
    );
    fixture.expect("DELEGATION_BINDING_INCOMPLETE");
    let sources = json(r#"[["S","I","B","F","A",1],["S","I","B","F","A",1]]"#);
    let mut continuation = fixture.envelope["review_continuation"].clone();
    continuation.set("finding_sources", sources.clone());
    fixture.envelope.set("review_continuation", continuation);
    fixture.result.set("finding_sources", sources);
    fixture.expect("DELEGATION_FINDING_SOURCE_AMBIGUOUS");
}
