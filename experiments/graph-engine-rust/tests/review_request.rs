use graph_engine::{
    compat::{PyString, Value},
    domain::{
        errors::ValidationFailure,
        review_request::{self, FanoutRequestContext},
    },
};
use num_bigint::BigInt;

fn assignment(id: &str, model: Value) -> Value {
    Value::object([
        ("assignment_id", id.into()),
        ("model", model),
        ("max_instances", 2i64.into()),
        ("allowed_reason_codes", Value::List(vec!["FOCUS".into()])),
        ("allowed_acceptance_ids", Value::List(vec!["AC-001".into()])),
        ("allowed_evidence_kinds", Value::List(vec!["source".into()])),
        ("scope_refs", Value::List(vec!["repo:docs/".into()])),
    ])
}

fn member(id: &str) -> Value {
    Value::object([
        ("assignment_id", id.into()),
        ("ordinal", 1i64.into()),
        ("reason_code", "FOCUS".into()),
        ("acceptance_ids", Value::List(vec!["AC-001".into()])),
        ("evidence_ids", Value::List(vec!["E1".into()])),
    ])
}
fn request(members: Vec<Value>) -> Value {
    Value::object([
        ("schema_version", 1i64.into()),
        ("kind", "review_fanout_request".into()),
        ("run_id", "RUN".into()),
        ("parent_branch_id", "PARENT".into()),
        ("parent_attempt_id", "ATTEMPT".into()),
        ("round", 1i64.into()),
        ("members", Value::List(members)),
    ])
}

fn validate(
    value: &Value,
    assignments: &Value,
    historical: bool,
    depth: i64,
) -> Result<Value, ValidationFailure> {
    let run = "RUN".into();
    let parent = "PARENT".into();
    let attempt = "ATTEMPT".into();
    let round = 1i64.into();
    let depth = depth.into();
    let preliminary = Value::object([(
        "evidence",
        Value::List(vec![Value::object([
            ("evidence_id", "E1".into()),
            ("kind", "source".into()),
            (
                "ref",
                format!("repo:docs/file#sha256={}", "a".repeat(64)).into(),
            ),
        ])]),
    )]);
    let limits = Value::object([
        ("max_depth", 1i64.into()),
        ("max_children_per_request", 3i64.into()),
    ]);
    review_request::validate_fanout_request(
        value,
        &FanoutRequestContext {
            run_id: &run,
            parent_branch_id: &parent,
            parent_attempt_id: &attempt,
            round_number: &round,
            plan_assignments: assignments,
            preliminary: &preliminary,
            limits: &limits,
            depth: &depth,
            historical,
        },
    )
}

fn code(error: ValidationFailure, field: &str, expected: &str) {
    assert!(
        matches!(error,ValidationFailure::Contract(error)if error.field.equals(field)&&error.code==expected)
    );
}

#[test]
fn fanout_request_preserves_large_ordinals_and_requires_canonical_member_order() {
    let assignments = Value::List(vec![
        assignment("R1", "gpt-6.1".into()),
        assignment("R2", "gpt-6.1".into()),
    ]);
    let mut large = member("R1");
    large.set("ordinal", Value::Integer(BigInt::from(1u64) << 180usize));
    let result = validate(&request(vec![large.clone()]), &assignments, false, 0).unwrap();
    assert!(result["members"].as_list().unwrap()[0].py_eq(&large));
    code(
        validate(
            &request(vec![member("R2"), member("R1")]),
            &assignments,
            false,
            0,
        )
        .unwrap_err(),
        "review_fanout_request.members",
        "NON_CANONICAL_ORDER",
    );
    code(
        validate(&Value::Null, &assignments, false, 1).unwrap_err(),
        "review_fanout_request",
        "DELEGATION_DEPTH_EXCEEDED",
    );
}

#[test]
fn retirement_keeps_raw_model_type_and_surrogate_prefix_semantics() {
    let value = request(vec![member("R1")]);
    let mut retired = PyString::from("gpt-5.6-");
    retired.0.push(0xD800);
    let retired = Value::List(vec![assignment("R1", Value::String(retired))]);
    code(
        validate(&value, &retired, false, 0).unwrap_err(),
        "review_fanout_request.members[0].assignment_id",
        "MODEL_RETIRED",
    );
    validate(&value, &retired, true, 0).unwrap();
    let mut current = PyString::from("gpt-6.1-");
    current.0.push(0xD800);
    validate(
        &value,
        &Value::List(vec![assignment("R1", Value::String(current))]),
        false,
        0,
    )
    .unwrap();
    assert!(
        matches!(validate(&value,&Value::List(vec![assignment("R1",Value::Null)]),true,0).unwrap_err(),ValidationFailure::Python{kind:"AttributeError",message}if message.equals("'NoneType' object has no attribute 'startswith'"))
    );
    assert!(
        matches!(validate(&value,&Value::List(vec![assignment("R1",Value::Bytes(vec![]))]),false,0).unwrap_err(),ValidationFailure::Python{kind:"TypeError",message}if message.equals("startswith first arg must be bytes or a tuple of bytes, not str"))
    );
}

#[test]
fn duplicate_member_and_evidence_gates_keep_source_order() {
    let assignments = Value::List(vec![assignment("R1", "gpt-6.1".into())]);
    code(
        validate(
            &request(vec![member("R1"), member("R1")]),
            &assignments,
            false,
            0,
        )
        .unwrap_err(),
        "review_fanout_request.members[1]",
        "DUPLICATE_VALUE",
    );
    let mut invalid = member("R1");
    invalid.set("ordinal", true.into());
    invalid.set("reason_code", "FORBIDDEN".into());
    code(
        validate(&request(vec![invalid]), &assignments, false, 0).unwrap_err(),
        "review_fanout_request.members[0].ordinal",
        "INVALID_ORDINAL",
    );
    let mut invalid = member("R1");
    invalid.set("evidence_ids", Value::List(vec![]));
    code(
        validate(&request(vec![invalid]), &assignments, false, 0).unwrap_err(),
        "review_fanout_request.members[0].evidence_ids",
        "DELEGATION_EVIDENCE_FORBIDDEN",
    );
}
