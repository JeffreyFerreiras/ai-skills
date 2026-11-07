use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        evidence_lifecycle,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
    },
    compat::{self, SerializationProfile, Value},
};

#[derive(Default)]
struct Ledger {
    artifacts: Vec<Value>,
    events: Vec<Value>,
    plan: Option<Value>,
}
impl GraphLedgerReadPort for Ledger {
    fn first(&self, query: GraphRead, _parameters: &[Value]) -> EngineResult<Option<Value>> {
        assert!(matches!(query, GraphRead::ActivationPlanDigest));
        Ok(self.plan.clone())
    }
    fn all(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        let mut result = vec![];
        self.visit(query, parameters, &mut |value| {
            result.push(value);
            Ok(())
        })?;
        Ok(result)
    }
    fn visit(
        &self,
        query: GraphRead,
        _parameters: &[Value],
        visit: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        let rows = match query {
            GraphRead::LedgerArtifacts => &self.artifacts,
            GraphRead::ReservationEvents => &self.events,
            _ => panic!("unexpected {query:?}"),
        };
        for row in rows {
            visit(row.clone())?;
        }
        Ok(())
    }
}
fn encoded(value: &Value) -> Value {
    String::from_utf8(compat::dumps(value, SerializationProfile::STORED).unwrap())
        .unwrap()
        .into()
}
fn artifact(reference: &str, payload: Value) -> Value {
    Value::object([
        ("ref", reference.into()),
        ("kind", "evidence_manifest".into()),
        ("content_json", encoded(&payload)),
    ])
}
fn reservation(identifier: Value) -> Value {
    Value::object([
        ("kind", "check_reservation".into()),
        ("reservation_id", identifier),
    ])
}
fn event(kind: &str, identifier: Value) -> Value {
    Value::object([
        ("event_type", kind.into()),
        (
            "detail_json",
            encoded(&Value::object([("reservation_id", identifier)])),
        ),
    ])
}

#[test]
fn reservation_history_preserves_replacement_order_and_rejects_duplicate_terminal_events() {
    let mut ledger = Ledger {
        artifacts: vec![
            artifact("first", reservation("R1".into())),
            artifact("other", reservation("R2".into())),
            artifact("replacement", reservation("R1".into())),
        ],
        events: vec![
            Value::object([
                ("event_type", "unrelated".into()),
                ("detail_json", "not JSON".into()),
            ]),
            event("check.finish", "R1".into()),
            event("check.abandon", "R2".into()),
        ],
        plan: None,
    };
    let states = evidence_lifecycle::reservation_states(&ledger, &"RUN-1".into()).unwrap();
    let values = states.values();
    assert!(values[0]["artifact_ref"].is_text("replacement"));
    assert!(values[0]["state"].is_text("finished"));
    assert!(values[1]["state"].is_text("abandoned"));
    ledger.events.push(event("check.finish", "R1".into()));
    let failure = evidence_lifecycle::reservation_states(&ledger, &"RUN-1".into())
        .err()
        .unwrap();
    assert!(
        matches!(failure,EngineError::State(error)if error.to_string()=="CHECK_RESERVATION_INVALID")
    );
}

#[test]
fn reservation_dictionary_preserves_python_numeric_key_equality_and_hashability_errors() {
    let ledger = Ledger {
        artifacts: vec![
            artifact("boolean", reservation(true.into())),
            artifact("integer", reservation(1i64.into())),
        ],
        ..Ledger::default()
    };
    let states = evidence_lifecycle::reservation_states(&ledger, &"RUN-1".into()).unwrap();
    assert_eq!(states.values().len(), 1);
    assert!(states.get(&true.into()).unwrap().unwrap()["artifact_ref"].is_text("integer"));
    let ledger = Ledger {
        artifacts: vec![artifact("invalid", reservation(Value::list([])))],
        ..Ledger::default()
    };
    let failure = evidence_lifecycle::reservation_states(&ledger, &"RUN-1".into())
        .err()
        .unwrap();
    assert!(
        matches!(failure,EngineError::Python{kind:"TypeError",message}if message.equals("unhashable type: 'list'"))
    );
}

#[test]
fn activation_validation_preserves_missing_plan_and_early_schema_error_precedence() {
    let run = Value::object([
        ("run_id", "RUN-1".into()),
        ("task_digest", "task".into()),
        ("policy_digest", "policy".into()),
    ]);
    let mut activation = Value::object([
        ("kind", "evidence_activation".into()),
        ("schema_version", 0i64.into()),
        ("original_format", 6i64.into()),
        ("task_digest", "task".into()),
        ("policy_digest", "policy".into()),
        ("plan_digest", "plan".into()),
    ]);
    let mut ledger = Ledger {
        artifacts: vec![artifact("activation", activation.clone())],
        ..Ledger::default()
    };
    let error = evidence_lifecycle::evidence_activation(&ledger, &run)
        .err()
        .unwrap();
    assert!(
        matches!(error,EngineError::State(error)if error.to_string()=="EVIDENCE_ACTIVATION_INVALID")
    );
    activation.set("schema_version", true.into());
    ledger.artifacts[0] = artifact("activation", activation);
    let error = evidence_lifecycle::evidence_activation(&ledger, &run)
        .err()
        .unwrap();
    assert!(
        matches!(error,EngineError::Python{kind:"TypeError",message}if message.equals("'NoneType' object is not subscriptable"))
    );
    ledger.plan = Some(Value::object([("plan_digest", "plan".into())]));
    evidence_lifecycle::evidence_activation(&ledger, &run).unwrap();
}

#[test]
fn lifecycle_parsing_and_review_binding_filtering_keep_lazy_field_error_order() {
    let run = Value::object([("run_id", "RUN-1".into())]);
    assert!(
        evidence_lifecycle::current_review_binding(&Ledger::default(), &run)
            .unwrap()
            .is_none()
    );
    let binding = Value::object([
        ("kind", "review_source_binding".into()),
        ("source_generation", 2i64.into()),
    ]);
    let mut ledger = Ledger {
        artifacts: vec![artifact("binding", binding.clone())],
        ..Ledger::default()
    };
    assert!(matches!(
        evidence_lifecycle::current_review_binding(&ledger, &run),
        Err(EngineError::Python {
            kind: "KeyError",
            ..
        })
    ));
    let run = Value::object([
        ("run_id", "RUN-1".into()),
        ("implementation_generation", 2i64.into()),
    ]);
    assert!(evidence_lifecycle::current_review_binding(&ledger, &run)
        .unwrap()
        .unwrap()
        .0["ref"]
        .is_text("binding"));
    ledger.artifacts.push(artifact("duplicate", binding));
    let failure = evidence_lifecycle::current_review_binding(&ledger, &run).unwrap_err();
    assert!(
        matches!(failure,EngineError::State(error)if error.to_string()=="REVIEW_BINDING_MISMATCH")
    );
    ledger.artifacts.push(Value::object([
        ("ref", "malformed".into()),
        ("content_json", "not JSON".into()),
    ]));
    assert!(
        matches!(evidence_lifecycle::current_review_binding(&ledger,&run),Err(EngineError::Json(error))if error.kind=="JSONDecodeError")
    );
}

struct ReviewLedger {
    ledger: Ledger,
    node: Value,
    attempt: Value,
    members: Vec<Value>,
}
impl GraphLedgerReadPort for ReviewLedger {
    fn first(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Option<Value>> {
        match query {
            GraphRead::FenceNode => Ok(Some(self.node.clone())),
            GraphRead::FenceAttempt => Ok(Some(self.attempt.clone())),
            _ => self.ledger.first(query, parameters),
        }
    }
    fn all(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        let mut result = vec![];
        self.visit(query, parameters, &mut |value| {
            result.push(value);
            Ok(())
        })?;
        Ok(result)
    }
    fn visit(
        &self,
        query: GraphRead,
        parameters: &[Value],
        visit: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        if matches!(query, GraphRead::DeliveryReviewEnvelopes) {
            for member in &self.members {
                visit(member.clone())?;
            }
            Ok(())
        } else {
            self.ledger.visit(query, parameters, visit)
        }
    }
}
struct ReviewIo {
    generated: std::cell::Cell<usize>,
    captures: std::cell::Cell<usize>,
    digest: std::cell::RefCell<String>,
}
impl graph_engine::application::check_fences::CheckClockPort for ReviewIo {
    fn utc_now(&self) -> EngineResult<String> {
        Err(EngineError::Injected(
            "succeeded review source must not read clock".into(),
        ))
    }
}
impl graph_engine::application::check_eligibility::CheckEligibilityPort for ReviewIo {
    fn verify_fence(&self, _fence: &Value, _executor: bool) -> EngineResult<()> {
        Err(EngineError::Injected(
            "review source must use the real fence service".into(),
        ))
    }
    fn verify_generated_outputs(
        &self,
        outputs: &Value,
        registering: Option<&Value>,
    ) -> EngineResult<()> {
        assert!(outputs.as_list().unwrap().is_empty());
        assert!(registering.is_none());
        self.generated.set(self.generated.get() + 1);
        Ok(())
    }
    fn capture_source(&self, roots: &Value, _outputs: &Value) -> EngineResult<Value> {
        assert!(roots.container_eq(&Value::list(["docs/".into()])));
        self.captures.set(self.captures.get() + 1);
        Ok(Value::object([(
            "sha256",
            self.digest.borrow().clone().into(),
        )]))
    }
    fn current_review_binding(&self) -> EngineResult<Option<(Value, Value)>> {
        Err(EngineError::Injected(
            "review source must load its actual ledger binding".into(),
        ))
    }
}
fn review_fixture() -> (ReviewLedger, ReviewIo, Value) {
    let fence = Value::object([
        ("branch_id", "B1".into()),
        ("attempt_id", "A1".into()),
        ("claim_digest", "c".repeat(64).into()),
        ("result_digest", "f".repeat(64).into()),
    ]);
    let binding = Value::object([
        ("kind", "review_source_binding".into()),
        ("source_generation", 1i64.into()),
        ("source", fence.clone()),
        (
            "coverage",
            Value::object([
                ("roots", Value::list(["docs/".into()])),
                ("outputs", Value::list([])),
            ]),
        ),
        ("source_digest", "a".repeat(64).into()),
    ]);
    let envelope = Value::object([
        ("attempt_id", "A1".into()),
        ("claim_digest", "c".repeat(64).into()),
    ]);
    let node = Value::object([
        ("envelope_json", encoded(&envelope)),
        ("status", "succeeded".into()),
        ("stage", "implementation".into()),
        ("generation", 1i64.into()),
        ("node_key", "senior_engineer".into()),
        ("result_digest", "f".repeat(64).into()),
    ]);
    let member = Value::object([(
        "envelope_json",
        encoded(&Value::object([(
            "inputs",
            Value::list([Value::object([("ref", "binding".into())])]),
        )])),
    )]);
    let ledger = ReviewLedger {
        ledger: Ledger {
            artifacts: vec![artifact("binding", binding)],
            ..Ledger::default()
        },
        node,
        attempt: Value::object([("claim_digest", "c".repeat(64).into())]),
        members: vec![member],
    };
    let io = ReviewIo {
        generated: std::cell::Cell::new(0),
        captures: std::cell::Cell::new(0),
        digest: std::cell::RefCell::new("a".repeat(64)),
    };
    let run = Value::object([
        ("run_id", "RUN-1".into()),
        ("implementation_generation", 1i64.into()),
        ("design_generation", 1i64.into()),
        ("selected_route", "full_delivery".into()),
    ]);
    (ledger, io, run)
}

#[test]
fn review_source_historical_integrity_keeps_present_source_capture_optional() {
    let (ledger, io, run) = review_fixture();
    evidence_lifecycle::verify_review_source(&ledger, &io, &io, &run, None, false).unwrap();
    assert_eq!((io.generated.get(), io.captures.get()), (0, 0));
    evidence_lifecycle::verify_review_source(&ledger, &io, &io, &run, None, true).unwrap();
    assert_eq!((io.generated.get(), io.captures.get()), (1, 1));
    *io.digest.borrow_mut() = "b".repeat(64);
    let failure = evidence_lifecycle::verify_review_source(&ledger, &io, &io, &run, None, true)
        .err()
        .unwrap();
    assert!(
        matches!(failure,EngineError::State(error)if error.to_string()=="REVIEW_SOURCE_CHANGED")
    );
}

#[test]
fn review_source_rejects_missing_member_binding_before_freshness_checks() {
    let (mut ledger, io, run) = review_fixture();
    ledger.members[0].set(
        "envelope_json",
        encoded(&Value::object([("inputs", Value::list([]))])),
    );
    let failure = evidence_lifecycle::verify_review_source(&ledger, &io, &io, &run, None, true)
        .err()
        .unwrap();
    assert!(
        matches!(failure,EngineError::State(error)if error.to_string()=="REVIEW_BINDING_MISMATCH")
    );
    assert_eq!((io.generated.get(), io.captures.get()), (0, 0));
}
