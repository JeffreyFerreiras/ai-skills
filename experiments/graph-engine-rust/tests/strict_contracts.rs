use graph_engine::compat::{PyString, Value};
use graph_engine::domain::errors::ValidationFailure;
use graph_engine::domain::{fanout_contract, result_contract, task_contract};
use num_bigint::BigInt;

fn policy() -> Value {
    Value::object([
        ("schema_version", 1i64.into()),
        (
            "impact_tags",
            Value::List(vec!["security_privacy".into(), "dependency".into()]),
        ),
        (
            "routes",
            Value::List(
                task_contract::ROUTES
                    .iter()
                    .map(|s| Value::from(*s))
                    .collect(),
            ),
        ),
        ("required_checks", Value::object([])),
        ("role_capabilities", Value::object([])),
        ("denied_patterns", Value::List(vec![])),
        (
            "limits",
            Value::object([(
                "inspection",
                Value::object([
                    ("file_reads", 100i64.into()),
                    ("discovery_commands", 20i64.into()),
                ]),
            )]),
        ),
    ])
}

fn task() -> Value {
    Value::object([
        ("schema_version", 1i64.into()),
        ("task_id", "TASK".into()),
        ("objective", "Implement behavior".into()),
        ("user_outcome", "Behavior works".into()),
        ("request_mode", "delivery".into()),
        ("minimum_route", "fast_path".into()),
        ("mandatory_impact_tags", Value::List(vec![])),
        (
            "scope",
            Value::object([
                ("included", Value::List(vec![])),
                ("excluded", Value::List(vec![])),
            ]),
        ),
        ("constraints", Value::List(vec![])),
        (
            "acceptance_criteria",
            Value::List(vec![Value::object([
                ("id", "AC-001".into()),
                ("text", "Works".into()),
            ])]),
        ),
        ("risk_level", "low".into()),
        (
            "authority",
            Value::object([("capabilities", Value::List(vec![]))]),
        ),
        (
            "policy_approval",
            Value::object([
                ("sha256", "a".repeat(64).into()),
                ("authority_ref", "repo:docs/approval.md".into()),
            ]),
        ),
        ("evidence_paths", Value::List(vec![])),
        (
            "inspection_budget",
            Value::object([
                ("file_reads", 10i64.into()),
                ("discovery_commands", 2i64.into()),
            ]),
        ),
        ("required_check_ids", Value::List(vec![])),
        ("required_human_decisions", Value::List(vec![])),
    ])
}

fn contract(error: ValidationFailure, field: &str, code: &str) {
    match error {
        ValidationFailure::Contract(error) => {
            assert!(error.field.equals(field));
            assert_eq!(error.code, code);
        }
        other => panic!("expected {field}:{code}, got {other:?}"),
    }
}

fn validate_task(value: &Value) -> Result<Value, ValidationFailure> {
    task_contract::validate_task_brief(
        value,
        &PyString::from("a".repeat(64)),
        &policy(),
        Some(&1i64.into()),
        cfg!(windows),
    )
}

#[test]
fn task_normalization_preserves_input_and_default_string_boundary() {
    let mut input = task();
    input.set("objective", Value::String(PyString(vec![0xD800; 4096])));
    let normalized = validate_task(&input).unwrap();
    assert!(normalized.py_eq(&input));
    input.set("objective", "x".repeat(4097).into());
    input.set("risk_level", "unknown".into());
    contract(
        validate_task(&input).unwrap_err(),
        "objective",
        "INVALID_STRING",
    );
}

#[test]
fn task_schema_and_hashability_errors_keep_source_order() {
    let mut input = task();
    input.set("schema_version", 8i64.into());
    input.set("extra", true.into());
    contract(
        validate_task(&input).unwrap_err(),
        "task_brief",
        "UNKNOWN_FIELD",
    );
    input.remove("extra");
    contract(
        validate_task(&input).unwrap_err(),
        "schema_version",
        "UNSUPPORTED_SCHEMA",
    );
    input.set("schema_version", 1i64.into());
    input.set("request_mode", Value::List(vec![]));
    match validate_task(&input).unwrap_err() {
        ValidationFailure::Python { kind, message } => {
            assert_eq!(kind, "TypeError");
            assert!(message.equals("unhashable type: 'list'"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn task_budget_accepts_arbitrary_integer_limit_without_narrowing() {
    let huge: BigInt = BigInt::from(1u64) << 200usize;
    let mut input = task();
    input.set(
        "inspection_budget",
        Value::object([
            ("file_reads", Value::Integer(huge.clone())),
            ("discovery_commands", 1i64.into()),
        ]),
    );
    let mut policy = policy();
    policy.set(
        "limits",
        Value::object([(
            "inspection",
            Value::object([
                ("file_reads", Value::Integer(huge)),
                ("discovery_commands", 2i64.into()),
            ]),
        )]),
    );
    task_contract::validate_task_brief(
        &input,
        &PyString::from("a".repeat(64)),
        &policy,
        Some(&1i64.into()),
        cfg!(windows),
    )
    .unwrap();
    let mut budget = input["inspection_budget"].clone();
    budget.set("file_reads", true.into());
    input.set("inspection_budget", budget);
    contract(
        validate_task(&input).unwrap_err(),
        "inspection_budget.file_reads",
        "BUDGET_LIMIT",
    );
}

#[test]
fn task_fast_path_and_delivery_readiness_gates_precede_later_fields() {
    let mut input = task();
    input.set(
        "mandatory_impact_tags",
        Value::List(vec!["dependency".into()]),
    );
    input.set("scope", Value::Null);
    contract(
        validate_task(&input).unwrap_err(),
        "minimum_route",
        "FAST_PATH_INVARIANT",
    );
    input.set("mandatory_impact_tags", Value::List(vec![]));
    input.set("minimum_route", "delivery_only".into());
    contract(
        validate_task(&input).unwrap_err(),
        "delivery_readiness",
        "DELIVERY_READINESS_REQUIRED",
    );
}

fn assessment() -> Value {
    Value::object([
        ("schema_version", true.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", "RUN".into()),
        ("fanout_id", "FAN".into()),
        (
            "members",
            Value::List(vec![Value::object([
                ("branch_id", "B".into()),
                (
                    "resources",
                    Value::object([
                        (
                            "writable_paths",
                            Value::List(vec![Value::object([
                                ("path", "docs/".into()),
                                ("scope", "subtree".into()),
                            ])]),
                        ),
                        (
                            "mutable_state_refs",
                            Value::List(vec!["z".into(), "a".into()]),
                        ),
                        ("exclusive_device_refs", Value::List(vec![])),
                        (
                            "services",
                            Value::List(vec![Value::object([
                                ("ref", "service".into()),
                                ("units", Value::Integer(BigInt::from(1u64) << 180)),
                                ("capacity", Value::Integer(BigInt::from(1u64) << 181)),
                            ])]),
                        ),
                    ]),
                ),
            ])]),
        ),
        ("dependencies", Value::List(vec![])),
        (
            "evidence",
            Value::List(vec![Value::object([
                ("kind", "observation".into()),
                (
                    "ref",
                    format!("repo:docs/resource.json#sha256={}", "a".repeat(64)).into(),
                ),
                ("sha256", "a".repeat(64).into()),
            ])]),
        ),
    ])
}

#[test]
fn fanout_normalizes_paths_and_resources_with_unbounded_capacity() {
    let result = fanout_contract::validate_fanout_assessment(
        &assessment(),
        &"RUN".into(),
        &"FAN".into(),
        &["B".into()],
    )
    .unwrap();
    assert_eq!(result["schema_version"].as_i64(), Some(1));
    let resources = &result["members"].as_list().unwrap()[0]["resources"];
    assert!(resources["writable_paths"].as_list().unwrap()[0]["path"].is_text("docs"));
    assert!(resources["mutable_state_refs"].as_list().unwrap()[0].is_text("a"));
}

fn branch() -> Value {
    Value::object([
        ("node_key", "reviewer".into()),
        ("run_id", "RUN".into()),
        ("branch_id", "BRANCH".into()),
        ("stage", "delivery".into()),
        (
            "output_contract",
            Value::object([
                ("artifact_kind", "review".into()),
                ("artifact_required", false.into()),
                (
                    "decision_values",
                    Value::List(vec!["APPROVE".into(), "REVISE".into(), "BLOCK".into()]),
                ),
            ]),
        ),
    ])
}

fn result() -> Value {
    Value::object([
        ("schema_version", 1i64.into()),
        ("run_id", "RUN".into()),
        ("branch_id", "BRANCH".into()),
        ("status", "succeeded".into()),
        ("output_kind", "review".into()),
        ("evidence", Value::List(vec![])),
        ("attempt_id", "ATTEMPT".into()),
        ("claim_digest", "a".repeat(64).into()),
        ("decision", "APPROVE".into()),
        (
            "findings",
            Value::List(vec![Value::object([
                ("finding_id", "R-001".into()),
                ("disposition", "accept".into()),
            ])]),
        ),
    ])
}

#[test]
fn result_phase_and_historical_dispositions_keep_decision_consistency() {
    result_contract::validate_result_manifest(&result(), &branch(), false).unwrap();
    let mut value = result();
    value.set(
        "findings",
        Value::List(vec![Value::object([
            ("finding_id", "R-001".into()),
            ("disposition", "approve".into()),
        ])]),
    );
    contract(
        result_contract::validate_result_manifest(&value, &branch(), false).unwrap_err(),
        "disposition",
        "PHASE_DISPOSITION_MISMATCH",
    );
    contract(
        result_contract::validate_result_manifest(&value, &branch(), true).unwrap_err(),
        "decision",
        "DECISION_FINDING_MISMATCH",
    );
    value.set(
        "findings",
        Value::List(vec![Value::object([
            ("finding_id", "R-001".into()),
            ("disposition", "approve".into()),
            ("scope", "out_of_scope".into()),
        ])]),
    );
    result_contract::validate_result_manifest(&value, &branch(), false).unwrap();
}

#[test]
fn delegated_result_returns_before_regular_finding_rules_but_forbids_artifacts() {
    let mut branch = branch();
    branch.set("depth", true.into());
    let mut value = result();
    value.set("findings", Value::List(vec![Value::Null]));
    result_contract::validate_result_manifest(&value, &branch, false).unwrap();
    value.set("artifact_ref", Value::object([]));
    contract(
        result_contract::validate_result_manifest(&value, &branch, false).unwrap_err(),
        "artifact_ref",
        "DELEGATION_ARTIFACT_FORBIDDEN",
    );
}

#[test]
fn finding_regex_failure_is_a_python_error_before_scope_validation() {
    let mut value = result();
    value.set(
        "findings",
        Value::List(vec![Value::object([
            ("finding_id", 12i64.into()),
            ("disposition", "accept".into()),
            ("scope", Value::List(vec![])),
        ])]),
    );
    match result_contract::validate_result_manifest(&value, &branch(), false).unwrap_err() {
        ValidationFailure::Python { kind, message } => {
            assert_eq!(kind, "TypeError");
            assert!(message.equals("expected string or bytes-like object, got 'int'"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn authoritative_subset_keeps_declared_order_and_excludes_free_text() {
    let input = validate_task(&task()).unwrap();
    let subset = task_contract::authoritative_task_subset(&input)
        .unwrap()
        .string_keyed_value()
        .unwrap();
    assert!(subset.get("objective").is_none());
    assert!(subset.get("user_outcome").is_none());
    assert!(subset["acceptance_ids"].py_eq(&Value::List(vec!["AC-001".into()])));
    let keys = subset
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, _)| key.utf8().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "schema_version",
            "task_id",
            "request_mode",
            "minimum_route",
            "mandatory_impact_tags",
            "risk_level",
            "acceptance_ids",
            "authority",
            "policy_approval",
            "evidence_paths",
            "inspection_budget",
            "required_check_ids",
            "required_human_decisions"
        ]
    );
}

#[test]
fn private_subset_dictionary_coercion_preserves_nonstring_keys_and_first_key_identity() {
    use graph_engine::domain::python_dictionary::DictionaryValue;
    let mut input = task();
    input.set(
        "inspection_budget",
        Value::List(vec![
            Value::List(vec![true.into(), "first".into()]),
            Value::List(vec![1i64.into(), "replacement".into()]),
            Value::List(vec![Value::Float(-0.0), "zero".into()]),
            Value::List(vec![false.into(), "zero-replacement".into()]),
            Value::List(vec![Value::ConstantNaN, "nan-first".into()]),
            Value::List(vec![Value::ConstantNaN, "nan-second".into()]),
            Value::List(vec![Value::Null, "none".into()]),
            Value::List(vec![Value::Bytes(vec![65]), "bytes".into()]),
        ]),
    );
    let result = task_contract::authoritative_task_subset(&input).unwrap();
    let DictionaryValue::Dictionary(budget) = result.get("inspection_budget").unwrap() else {
        panic!("owned dictionary required")
    };
    assert_eq!(budget.entries.len(), 5);
    assert!(matches!(&budget.entries[0].0, Value::Bool(true)));
    assert!(
        matches!(&budget.entries[0].1, DictionaryValue::Value(value) if value.is_text("replacement"))
    );
    assert!(
        matches!(&budget.entries[1].0, Value::Float(value) if value.to_bits()==(-0.0f64).to_bits())
    );
    assert!(
        matches!(&budget.entries[1].1, DictionaryValue::Value(value) if value.is_text("zero-replacement"))
    );
    assert!(matches!(&budget.entries[2].0, Value::ConstantNaN));
    assert!(
        matches!(&budget.entries[2].1, DictionaryValue::Value(value) if value.is_text("nan-second"))
    );
    assert!(matches!(&budget.entries[3].0, Value::Null));
    assert!(matches!(&budget.entries[4].0, Value::Bytes(value) if value==&[65]));
    assert!(result.string_keyed_value().is_none());
}

#[test]
fn dictionary_pair_length_precedes_hashability_and_later_input_errors() {
    use graph_engine::domain::python_dictionary::PythonDictionary;
    let bad_length = Value::List(vec![
        Value::List(vec![Value::List(vec![]), Value::Null, Value::Null]),
        Value::Null,
    ]);
    match PythonDictionary::coerce(&bad_length).unwrap_err() {
        ValidationFailure::Python { kind, message } => {
            assert_eq!(kind, "ValueError");
            assert!(
                message.equals("dictionary update sequence element #0 has length 3; 2 is required")
            );
        }
        other => panic!("{other:?}"),
    }
    let unhashable = Value::List(vec![
        Value::List(vec![Value::Object(vec![]), Value::Null]),
        Value::Null,
    ]);
    match PythonDictionary::coerce(&unhashable).unwrap_err() {
        ValidationFailure::Python { kind, message } => {
            assert_eq!(kind, "TypeError");
            assert!(message.equals("unhashable type: 'dict'"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn owned_dictionary_serializes_nonstring_keys_without_changing_the_owned_keys() {
    use graph_engine::domain::python_dictionary::PythonDictionary;
    let input = Value::List(vec![
        Value::List(vec![true.into(), "first".into()]),
        Value::List(vec![1i64.into(), "replacement".into()]),
        Value::List(vec![Value::Float(-0.0), "zero".into()]),
        Value::List(vec![Value::ConstantNaN, "nan".into()]),
        Value::List(vec![Value::Null, "none".into()]),
        Value::List(vec![Value::Float(f64::INFINITY), "infinite".into()]),
    ]);
    let dictionary = PythonDictionary::coerce(&input).unwrap();
    assert_eq!(dictionary.serialize_insertion_order(true,true).unwrap(),b"{\"true\": \"replacement\", \"-0.0\": \"zero\", \"NaN\": \"nan\", \"null\": \"none\", \"Infinity\": \"infinite\"}");
    assert!(matches!(dictionary.entries[0].0, Value::Bool(true)));
    assert!(
        matches!(dictionary.entries[1].0,Value::Float(value)if value.to_bits()==(-0.0f64).to_bits())
    );
    let bytes = PythonDictionary::coerce(&Value::List(vec![Value::List(vec![
        Value::Bytes(vec![1]),
        Value::Null,
    ])]))
    .unwrap();
    let error = bytes.serialize_insertion_order(true, true).unwrap_err();
    assert_eq!(error.kind, "TypeError");
    assert_eq!(
        error.message,
        "keys must be str, int, float, bool or None, not bytes"
    );
}

#[test]
fn dictionary_utf8_failure_keeps_whole_document_position() {
    use graph_engine::domain::python_dictionary::PythonDictionary;
    let mut nested = PythonDictionary::default();
    nested
        .insert(
            Value::String(PyString(vec![0xD800])),
            Value::from("value").into(),
        )
        .unwrap();
    let mut outer = PythonDictionary::default();
    outer.set("nested", nested);
    assert_eq!(
        outer.serialize_insertion_order(true, true).unwrap(),
        b"{\"nested\": {\"\\ud800\": \"value\"}}"
    );
    let error = outer.serialize_insertion_order(false, true).unwrap_err();
    assert_eq!(error.kind, "UnicodeEncodeError");
    assert_eq!(error.position, 13);
}
