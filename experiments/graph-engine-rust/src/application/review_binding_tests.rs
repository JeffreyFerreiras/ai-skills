use super::*;
use crate::compat::SerializationProfile;
use std::cell::Cell;

struct SealedReview {
    row: Value,
    canonical_reads: Cell<usize>,
}

impl SealedReview {
    fn new(issue_keys: &[&str]) -> Self {
        let issues = Value::list(issue_keys.iter().map(|key| {
            Value::object([
                ("issue_key", (*key).into()),
                ("canonical_branch_id", "BRANCH-CHILD".into()),
                (
                    "provenance",
                    Value::list([Value::object([
                        ("branch_id", "BRANCH-CHILD".into()),
                        ("finding_id", "REV-101".into()),
                        ("assignment_id", "security".into()),
                        ("ordinal", 1_i64.into()),
                    ])]),
                ),
            ])
        }));
        let content = Value::object([("members", Value::list([])), ("issues", issues)]);
        Self {
            row: Value::object([
                ("request_slot_id", "SLOT-1".into()),
                ("collection_digest", "collection-digest".into()),
                (
                    "content_json",
                    Value::String(
                        compat::dumps_text(&content, SerializationProfile::STORED).unwrap(),
                    ),
                ),
            ]),
            canonical_reads: Cell::new(0),
        }
    }

    fn result_context(&self) -> (Value, Value) {
        let mut envelope = Value::object([
            ("branch_id", "BRANCH-PARENT".into()),
            ("generation", 0_i64.into()),
        ]);
        let result =
            cumulative_binding(self, &envelope["branch_id"], &envelope["generation"]).unwrap();
        envelope.set("review_continuation", result.clone());
        (envelope, result)
    }
}

impl GraphLedgerReadPort for SealedReview {
    fn first(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Option<Value>> {
        assert!(matches!(query, GraphRead::ReviewCollection));
        assert_eq!(parameters.len(), 1);
        assert!(parameters[0].is_text("SLOT-1"));
        self.canonical_reads.set(self.canonical_reads.get() + 1);
        Ok(Some(self.row.clone()))
    }

    fn all(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        assert!(matches!(query, GraphRead::SealedReviewCollections));
        assert_eq!(parameters.len(), 2);
        assert!(parameters[0].is_text("BRANCH-PARENT"));
        assert!(parameters[1].py_eq(&0_i64.into()));
        Ok(vec![self.row.clone()])
    }

    fn visit(
        &self,
        _: GraphRead,
        _: &[Value],
        _: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        unreachable!("review binding does not stream rows")
    }
}

#[test]
fn ambiguity_precedes_mutation_dispositions_and_keeps_the_persisted_state_code() {
    let ledger = SealedReview::new(&["issue-1", "issue-2"]);
    let (envelope, result) = ledger.result_context();
    let error =
        validate_review_continuation_result(&ledger, &envelope, &envelope, &result).unwrap_err();
    assert!(
        matches!(error, EngineError::Contract(ref error)
            if error.field.equals("result.finding_sources")
                && error.code == "DELEGATION_FINDING_SOURCE_AMBIGUOUS"),
        "{error:?}"
    );
    assert_eq!(ledger.canonical_reads.get(), 0);
    let error = validate_review_result_binding(&ledger, &envelope, &result).unwrap_err();
    assert!(
        matches!(error, EngineError::State(ref error)
            if error.code == "DELEGATION_FINDING_SOURCE_AMBIGUOUS"),
        "{error:?}"
    );
}

#[test]
fn persisted_binding_and_canonical_disposition_failures_keep_their_state_codes() {
    let ledger = SealedReview::new(&["issue-1"]);
    let (envelope, result) = ledger.result_context();
    let mut mismatched = result.clone();
    mismatched.set("review_request_slots", Value::list([]));
    for (value, code) in [
        (mismatched, "DELEGATION_BINDING_INCOMPLETE"),
        (result, "DELEGATION_DISPOSITION_MISSING"),
    ] {
        let error = validate_review_result_binding(&ledger, &envelope, &value).unwrap_err();
        assert!(
            matches!(error, EngineError::State(ref error) if error.code == code),
            "{error:?}"
        );
    }
    assert_eq!(ledger.canonical_reads.get(), 0);
}
