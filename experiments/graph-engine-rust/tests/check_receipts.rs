use graph_engine::{
    application::{
        check_receipts::{self, CheckProvenancePort},
        error::{EngineError, EngineResult},
    },
    compat::{PyString, Value},
    domain::{check_receipts as receipt_contracts, ids},
};
use std::cell::RefCell;

#[derive(Default)]
struct Provenance {
    calls: RefCell<Vec<&'static str>>,
}
impl CheckProvenancePort for Provenance {
    fn actor(&self) -> EngineResult<PyString> {
        self.calls.borrow_mut().push("actor");
        Ok("synthetic-actor".into())
    }
    fn host_identity(&self) -> EngineResult<String> {
        self.calls.borrow_mut().push("host");
        Ok("synthetic-host".into())
    }
    fn repository_worktree_digest(&self) -> EngineResult<String> {
        self.calls.borrow_mut().push("worktree");
        Ok("a".repeat(64))
    }
}

fn set(value: &mut Value, name: &str, replacement: Value) {
    let Value::Object(fields) = value else {
        panic!("test object required")
    };
    if let Some((_, value)) = fields.iter_mut().find(|(key, _)| key.equals(name)) {
        *value = replacement;
    } else {
        fields.push((name.into(), replacement));
    }
}
fn expected() -> Value {
    Value::object([
        ("command_id", "synthetic-command".into()),
        ("argv", Value::list(["synthetic-check".into()])),
    ])
}
fn receipt() -> Value {
    Value::object([
        ("schema_version", 1i64.into()),
        ("kind", "check_receipt".into()),
        ("run_id", "RUN-1".into()),
        ("check_id", "synthetic-check".into()),
        ("command_id", "synthetic-command".into()),
        ("argv", Value::list(["synthetic-check".into()])),
        ("outcome", "PASS".into()),
        ("exit_code", 0i64.into()),
        ("timed_out", false.into()),
        ("stdout_sha256", "b".repeat(64).into()),
        ("stderr_sha256", "c".repeat(64).into()),
        ("repo_worktree_sha256", "a".repeat(64).into()),
        ("started_at", "2024-01-01T00:00:00Z".into()),
        ("finished_at", "2024-01-01T00:00:01Z".into()),
        ("producer_actor", "synthetic-actor".into()),
        ("producer_host_identity", "synthetic-host".into()),
    ])
}
fn validate(value: &Value, provenance: &Provenance, fresh: bool) -> EngineResult<Value> {
    check_receipts::validate_check_receipt(
        value,
        &"RUN-1".into(),
        &"synthetic-check".into(),
        &expected(),
        provenance,
        fresh,
    )
}
fn contract(error: EngineError, field: &str, code: &str) {
    let EngineError::Contract(error) = error else {
        panic!("unexpected {error:?}")
    };
    assert!(error.field.equals(field));
    assert_eq!(error.code, code);
}
fn integrity() -> Value {
    let mut value = receipt();
    set(&mut value, "schema_version", 2i64.into());
    let fence = Value::object([
        ("branch_id", "synthetic-branch".into()),
        ("attempt_id", "synthetic-attempt".into()),
        ("claim_digest", "d".repeat(64).into()),
        ("result_digest", Value::Null),
    ]);
    let coverage = Value::object([
        ("roots", Value::list(["src/".into()])),
        ("outputs", Value::list([])),
        ("activation_ref", Value::Null),
    ]);
    for (key, entry) in [
        ("executor", fence.clone()),
        ("source", fence),
        ("source_generation", 1i64.into()),
        ("reservation_id", "synthetic-reservation".into()),
        ("predecessor_ref", Value::Null),
        ("activation_ref", Value::Null),
        ("review_source_binding_ref", Value::Null),
        (
            "coverage_digest",
            ids::sha256_bytes(&ids::canonical_bytes(&coverage).unwrap()).into(),
        ),
        ("coverage", coverage),
        ("validity", "valid".into()),
    ] {
        set(&mut value, key, entry);
    }
    for key in [
        "task_digest",
        "plan_digest",
        "policy_digest",
        "before_source_digest",
        "after_source_digest",
    ] {
        set(&mut value, key, "e".repeat(64).into());
    }
    value
}

#[test]
fn receipt_validation_rechecks_freshness_and_preserves_producer_short_circuit() {
    let provenance = Provenance::default();
    validate(&receipt(), &provenance, true).unwrap();
    validate(&receipt(), &provenance, true).unwrap();
    assert_eq!(
        *provenance.calls.borrow(),
        ["actor", "host", "worktree", "actor", "host", "worktree"]
    );
    provenance.calls.borrow_mut().clear();
    let mut value = receipt();
    set(&mut value, "producer_actor", "other".into());
    contract(
        validate(&value, &provenance, true).unwrap_err(),
        "check_receipt",
        "PRODUCER_MISMATCH",
    );
    assert_eq!(*provenance.calls.borrow(), ["actor"]);
    provenance.calls.borrow_mut().clear();
    set(&mut value, "producer_actor", "synthetic-actor".into());
    set(&mut value, "repo_worktree_sha256", "0".repeat(64).into());
    contract(
        validate(&value, &provenance, true).unwrap_err(),
        "repo_worktree_sha256",
        "REPOSITORY_CHANGED",
    );
    validate(&value, &provenance, false).unwrap();
    assert_eq!(
        *provenance.calls.borrow(),
        ["actor", "host", "worktree", "actor", "host"]
    );
}

#[test]
fn receipt_validation_preserves_hashability_timestamp_and_validation_error_order() {
    let provenance = Provenance::default();
    let mut value = receipt();
    set(&mut value, "outcome", Value::list([]));
    set(&mut value, "exit_code", true.into());
    let error = validate(&value, &provenance, true).unwrap_err();
    assert!(
        matches!(error, EngineError::Python { kind: "TypeError", message } if message.equals("unhashable type: 'list'"))
    );
    assert!(provenance.calls.borrow().is_empty());
    let mut value = receipt();
    set(&mut value, "timed_out", true.into());
    set(&mut value, "started_at", "invalid".into());
    contract(
        validate(&value, &provenance, true).unwrap_err(),
        "timed_out",
        "OUTCOME_MISMATCH",
    );
    set(&mut value, "timed_out", false.into());
    contract(
        validate(&value, &provenance, true).unwrap_err(),
        "check_receipt",
        "TIMESTAMP_INVALID",
    );
    set(&mut value, "started_at", "2024-01-01T00:00:00".into());
    let error = validate(&value, &provenance, true).unwrap_err();
    assert!(
        matches!(error, EngineError::Python { kind: "TypeError", message } if message.equals("can't compare offset-naive and offset-aware datetimes"))
    );
    assert!(provenance.calls.borrow().is_empty());
}

#[test]
fn receipt_integrity_keeps_historical_freshness_independent_and_preserves_stage_order() {
    let provenance = Provenance::default();
    let mut value = integrity();
    set(&mut value, "repo_worktree_sha256", "0".repeat(64).into());
    check_receipts::validate_receipt_integrity(
        &value,
        &"RUN-1".into(),
        &"synthetic-check".into(),
        &expected(),
        &provenance,
    )
    .unwrap();
    assert_eq!(*provenance.calls.borrow(), ["actor", "host"]);
    provenance.calls.borrow_mut().clear();
    set(&mut value, "producer_actor", "other".into());
    set(&mut value, "executor", Value::Null);
    contract(
        check_receipts::validate_receipt_integrity(
            &value,
            &"RUN-1".into(),
            &"synthetic-check".into(),
            &expected(),
            &provenance,
        )
        .unwrap_err(),
        "check_receipt",
        "PRODUCER_MISMATCH",
    );
    assert_eq!(*provenance.calls.borrow(), ["actor"]);
    set(&mut value, "producer_actor", "synthetic-actor".into());
    let error = check_receipts::validate_receipt_integrity(
        &value,
        &"RUN-1".into(),
        &"synthetic-check".into(),
        &expected(),
        &provenance,
    )
    .unwrap_err();
    assert!(
        matches!(error, EngineError::Python { kind: "TypeError", message } if message.equals("'NoneType' object is not iterable"))
    );
}

#[test]
fn configured_check_retains_mapping_hashability_and_missing_command_errors() {
    let policy = Value::object([(
        "required_checks",
        Value::object([("synthetic-check", expected())]),
    )]);
    assert!(matches!(
        receipt_contracts::configured_check(&policy, &"absent".into()),
        Err(receipt_contracts::CheckFailure::State("UNKNOWN_CHECK"))
    ));
    let error: EngineError = receipt_contracts::configured_check(&policy, &Value::list([]))
        .unwrap_err()
        .into();
    assert!(matches!(
        error,
        EngineError::Python {
            kind: "TypeError",
            ..
        }
    ));
    let policy = Value::object([(
        "required_checks",
        Value::object([("synthetic-check", Value::object([]))]),
    )]);
    assert!(matches!(
        receipt_contracts::configured_check(&policy, &"synthetic-check".into()),
        Err(receipt_contracts::CheckFailure::State(
            "CHECK_COMMAND_NOT_CONFIGURED"
        ))
    ));
}
