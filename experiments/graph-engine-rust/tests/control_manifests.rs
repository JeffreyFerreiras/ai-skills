use graph_engine::{
    compat::Value,
    domain::{control_manifests::validate_control_manifest, errors::ValidationFailure},
};

fn manifest(kind: &str) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", kind.into()),
        ("run_id", "RUN".into()),
        ("reason_code", "STOP".into()),
        ("evidence", Value::list([])),
    ])
}
fn expect(error: ValidationFailure, field: &str, code: &str) {
    assert!(
        matches!(error,ValidationFailure::Contract(error) if error.field.equals(field) && error.code==code)
    );
}

#[test]
fn empty_evidence_and_python_numeric_schema_equality_are_preserved() {
    let mut value = manifest("block");
    value.set("schema_version", true.into());
    let result = validate_control_manifest(&value, "block", "RUN", None, "STOP").unwrap();
    assert!(result.py_eq(&value));
}

#[test]
fn required_keys_precede_unknown_keys_and_binding_mismatches() {
    let mut value = manifest("block");
    value.remove("reason_code");
    value.set("extra", Value::Null);
    value.set("schema_version", 2_i64.into());
    expect(
        validate_control_manifest(&value, "block", "RUN", None, "STOP").unwrap_err(),
        "evidence_manifest",
        "MISSING_FIELD",
    );
    value.set("reason_code", "STOP".into());
    expect(
        validate_control_manifest(&value, "block", "RUN", None, "STOP").unwrap_err(),
        "evidence_manifest",
        "UNKNOWN_FIELD",
    );
}

#[test]
fn timeout_binding_mismatch_precedes_its_second_required_key_check() {
    let mut value = manifest("timeout");
    value.set("schema_version", 2_i64.into());
    expect(
        validate_control_manifest(&value, "timeout", "RUN", None, "STOP").unwrap_err(),
        "evidence_manifest",
        "CONTROL_MANIFEST_MISMATCH",
    );
    value.set("schema_version", 1_i64.into());
    expect(
        validate_control_manifest(&value, "timeout", "RUN", None, "STOP").unwrap_err(),
        "evidence_manifest",
        "MISSING_FIELD",
    );
    value.set("attempt_id", "bad id".into());
    value.set("claim_digest", "bad digest".into());
    expect(
        validate_control_manifest(&value, "timeout", "RUN", None, "STOP").unwrap_err(),
        "attempt_id",
        "INVALID_ID",
    );
}

#[test]
fn branch_binding_and_strict_list_type_remain_distinct() {
    let mut value = manifest("skip");
    expect(
        validate_control_manifest(&value, "skip", "RUN", Some("branch"), "STOP").unwrap_err(),
        "evidence_manifest",
        "MISSING_FIELD",
    );
    value.set("branch_id", "other".into());
    expect(
        validate_control_manifest(&value, "skip", "RUN", Some("branch"), "STOP").unwrap_err(),
        "evidence_manifest",
        "CONTROL_MANIFEST_MISMATCH",
    );
    value.set("branch_id", "branch".into());
    value.set("evidence", Value::tuple([]));
    expect(
        validate_control_manifest(&value, "skip", "RUN", Some("branch"), "STOP").unwrap_err(),
        "evidence",
        "INVALID_LIST",
    );
}

#[test]
fn valid_timeout_retains_all_original_fields_and_order() {
    let mut value = manifest("timeout");
    value.set("branch_id", "branch".into());
    value.set("attempt_id", "attempt".into());
    value.set("claim_digest", "a".repeat(64).into());
    let result =
        validate_control_manifest(&value, "timeout", "RUN", Some("branch"), "STOP").unwrap();
    assert_eq!(
        result
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>(),
        value
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>()
    );
}
