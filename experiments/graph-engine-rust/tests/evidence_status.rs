use graph_engine::{
    application::{
        check_eligibility::{self, CheckEligibilityPort},
        error::{EngineError, EngineResult},
        evidence_status::{self, EvidenceStatusPort},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
    },
    compat::{self, PyString, SerializationProfile, Value},
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

struct Scope {
    receipts: Vec<(&'static str, &'static str, Value)>,
    captures: RefCell<Vec<Value>>,
    fences: RefCell<Vec<(Value, bool)>>,
    generated: Cell<usize>,
    snapshots: RefCell<VecDeque<Value>>,
    rejected_branch: Option<&'static str>,
}
impl Scope {
    fn new(receipts: Vec<(&'static str, &'static str, Value)>) -> Self {
        Self {
            receipts,
            captures: RefCell::new(vec![]),
            fences: RefCell::new(vec![]),
            generated: Cell::new(0),
            snapshots: RefCell::new(VecDeque::new()),
            rejected_branch: None,
        }
    }
    fn status(&self) -> Value {
        let checks = self
            .receipts
            .iter()
            .map(|(id, _, _)| {
                (
                    PyString::from(*id),
                    Value::object([
                        ("argv", Value::list(["true".into()])),
                        ("command_id", format!("cmd-{id}").into()),
                    ]),
                )
            })
            .collect();
        let policy = Value::object([
            ("denied_patterns", Value::list([])),
            ("required_checks", Value::Object(checks)),
        ]);
        let policy_json =
            String::from_utf8(compat::dumps(&policy, SerializationProfile::STORED).unwrap())
                .unwrap();
        evidence_status::evidence_status(
            self,
            &Value::object([
                ("state_schema_version", 7i64.into()),
                ("run_id", "RUN-1".into()),
                ("policy_json", policy_json.into()),
            ]),
        )
        .unwrap()
    }
}
fn snapshot(digest: &str) -> Value {
    Value::object([
        ("sha256", digest.into()),
        ("roots", Value::list(["docs/".into()])),
        ("path_count", 1i64.into()),
    ])
}
fn receipt(branch: &str, validity: &str, digest: &str) -> Value {
    let fence = Value::object([
        ("branch_id", branch.into()),
        ("attempt_id", "A1".into()),
        ("claim_digest", "c".repeat(64).into()),
    ]);
    Value::object([
        ("schema_version", 2i64.into()),
        ("outcome", "PASS".into()),
        ("validity", validity.into()),
        ("source", fence.clone()),
        ("executor", fence),
        (
            "coverage",
            Value::object([
                ("roots", Value::list(["docs/".into()])),
                (
                    "outputs",
                    Value::list([Value::object([
                        ("path", "docs/artifacts/report.json".into()),
                        ("purpose", "review_report".into()),
                        ("producer_node_key", "code_reviewer".into()),
                        ("artifact_kind", "delivery_review".into()),
                    ])]),
                ),
                (
                    "activation_ref",
                    format!("ledger:ACT#sha256={}", "d".repeat(64)).into(),
                ),
            ]),
        ),
        ("before_source_digest", digest.into()),
        ("after_source_digest", digest.into()),
    ])
}
impl GraphLedgerReadPort for Scope {
    fn first(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Option<Value>> {
        assert!(matches!(query, GraphRead::ArtifactContent));
        Ok(self
            .receipts
            .iter()
            .find(|(_, reference, _)| parameters[0].is_text(reference))
            .map(|(_, _, value)| {
                Value::object([(
                    "content_json",
                    String::from_utf8(compat::dumps(value, SerializationProfile::STORED).unwrap())
                        .unwrap()
                        .into(),
                )])
            }))
    }
    fn all(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        let mut values = vec![];
        self.visit(query, parameters, &mut |value| {
            values.push(value);
            Ok(())
        })?;
        Ok(values)
    }
    fn visit(
        &self,
        query: GraphRead,
        _parameters: &[Value],
        visit: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        match query {
            GraphRead::CheckEvidenceOrdered => {
                for (id, reference, _) in &self.receipts {
                    visit(Value::object([
                        ("check_id", (*id).into()),
                        ("artifact_ref", (*reference).into()),
                    ]))?;
                }
            }
            GraphRead::EligibleExecutorNodes => (),
            _ => panic!("unexpected query {query:?}"),
        }
        Ok(())
    }
}
impl CheckEligibilityPort for Scope {
    fn verify_fence(&self, fence: &Value, executor: bool) -> EngineResult<()> {
        self.fences.borrow_mut().push((fence.clone(), executor));
        if self
            .rejected_branch
            .is_some_and(|branch| fence["branch_id"].is_text(branch))
        {
            Err(EngineError::state("ATTEMPT_FENCE_MISMATCH"))
        } else {
            Ok(())
        }
    }
    fn verify_generated_outputs(
        &self,
        _outputs: &Value,
        registering: Option<&Value>,
    ) -> EngineResult<()> {
        assert!(registering.is_none());
        self.generated.set(self.generated.get() + 1);
        Ok(())
    }
    fn capture_source(&self, roots: &Value, outputs: &Value) -> EngineResult<Value> {
        self.captures.borrow_mut().push(Value::object([
            ("roots", roots.clone()),
            ("outputs", outputs.clone()),
        ]));
        Ok(self
            .snapshots
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| snapshot(&"a".repeat(64))))
    }
    fn current_review_binding(&self) -> EngineResult<Option<(Value, Value)>> {
        Ok(None)
    }
}
impl EvidenceStatusPort for Scope {
    fn reservation_states(&self, _run: &Value) -> EngineResult<Vec<Value>> {
        Ok(vec![])
    }
}
fn eligibility(status: &Value) -> Vec<String> {
    status["checks"]
        .as_list()
        .unwrap()
        .iter()
        .map(|check| check["eligibility"].text().unwrap())
        .collect()
}

#[test]
fn test_same_coverage_receipts_share_one_capture() {
    let digest = "a".repeat(64);
    let scope = Scope::new(vec![
        ("check-a", "ref-a", receipt("B1", "valid", &digest)),
        ("check-b", "ref-b", receipt("B1", "valid", &digest)),
    ]);
    assert_eq!(eligibility(&scope.status()), ["eligible", "eligible"]);
    assert_eq!(scope.captures.borrow().len(), 1);
    assert_eq!(scope.fences.borrow().len(), 4);
    assert_eq!(scope.generated.get(), 2);
}

#[test]
fn test_different_coverage_never_aliases() {
    let digest = "a".repeat(64);
    let mut second = receipt("B1", "valid", &digest);
    let mut coverage = second["coverage"].clone();
    coverage.set("roots", Value::list(["other/".into()]));
    second.set("coverage", coverage);
    let scope = Scope::new(vec![
        ("check-a", "ref-a", receipt("B1", "valid", &digest)),
        ("check-b", "ref-b", second),
    ]);
    assert_eq!(eligibility(&scope.status()), ["eligible", "eligible"]);
    assert_eq!(scope.captures.borrow().len(), 2);
    for root in ["docs/", "other/"] {
        assert!(scope
            .captures
            .borrow()
            .iter()
            .any(|capture| capture["roots"].container_eq(&Value::list([root.into()]))));
    }
}

#[test]
fn test_fence_and_validity_checks_rerun_per_receipt_despite_shared_capture() {
    let digest = "a".repeat(64);
    let mut scope = Scope::new(vec![
        ("check-a", "ref-a", receipt("B1", "valid", &digest)),
        ("check-b", "ref-b", receipt("B2", "valid", &digest)),
        ("check-c", "ref-c", receipt("B1", "source_changed", &digest)),
    ]);
    scope.rejected_branch = Some("B2");
    assert_eq!(
        eligibility(&scope.status()),
        ["eligible", "ATTEMPT_FENCE_MISMATCH", "source_changed"]
    );
    assert_eq!(scope.captures.borrow().len(), 1);
    assert_eq!(scope.generated.get(), 1);
    assert_eq!(scope.fences.borrow().len(), 5);
}

#[test]
fn test_default_eligibility_captures_fresh_without_scope() {
    let digest = "a".repeat(64);
    let scope = Scope::new(vec![]);
    let receipt = receipt("B1", "valid", &digest);
    for _ in 0..2 {
        assert!(
            check_eligibility::receipt_eligibility(&receipt, &scope, None, None)
                .unwrap()
                .is_text("eligible")
        );
    }
    assert_eq!(scope.captures.borrow().len(), 2);
}

#[test]
fn test_repeated_status_stays_fresh_after_real_source_change() {
    let (before, after) = {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        std::fs::create_dir(repo.join("docs")).unwrap();
        let path = repo.join("docs").join("file.txt");
        std::fs::write(&path, "first").unwrap();
        hardening_source_git::initial_commit(repo, ".");
        let policy = Value::object([("denied_patterns", Value::list([]))]);
        let capture = || hardening_source_git::capture(repo, &policy, "docs/", &[]).unwrap();
        let before = capture()["sha256"].text().unwrap();
        std::fs::write(&path, "second").unwrap();
        let after = capture()["sha256"].text().unwrap();
        assert_ne!(before, after);
        (before, after)
    };
    let scope = Scope::new(vec![("check-a", "ref-a", receipt("B1", "valid", &before))]);
    scope
        .snapshots
        .borrow_mut()
        .extend([snapshot(&before), snapshot(&after)]);
    assert_eq!(eligibility(&scope.status()), ["eligible"]);
    assert_eq!(eligibility(&scope.status()), ["source_changed"]);
    assert_eq!(scope.captures.borrow().len(), 2);
}
#[path = "support/hardening_source_git.rs"]
mod hardening_source_git;
