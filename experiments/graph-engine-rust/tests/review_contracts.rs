use graph_engine::{
    compat::{PyString, Value},
    domain::{errors::ValidationFailure, review_contracts},
};

fn finding() -> Value {
    Value::object([
        ("finding_id", "R-001".into()),
        ("acceptance_id", "AC-001".into()),
        ("location", "  .\\Straße//ΔΟΣ/  ".into()),
        ("defect_id", "D-001".into()),
        ("summary", "A concrete defect".into()),
        ("fix_variant", "  Fix this  ".into()),
        ("evidence_ids", Value::List(vec!["E2".into(), "E1".into()])),
    ])
}

fn code(error: ValidationFailure, field: &str, code: &str) {
    assert!(
        matches!(error,ValidationFailure::Contract(error)if error.field.equals(field)&&error.code==code)
    );
}

#[test]
fn location_normalization_reuses_whitespace_and_whole_string_casefold() {
    assert!(
        review_contracts::normalize_location(&"  .\\Straße//ΔΟΣ/  ".into())
            .unwrap()
            .equals("strasse/δοσ")
    );
    assert!(
        review_contracts::normalize_location(&Value::String(PyString(vec![
            0xA0, 100, 111, 99, 115, 0x3000
        ])))
        .unwrap()
        .equals("docs")
    );
    code(
        review_contracts::normalize_location(&"\tdocs".into()).unwrap_err(),
        "finding.location",
        "CONTROL_CHARACTER",
    );
    code(
        review_contracts::normalize_location(&"docs/../secret".into()).unwrap_err(),
        "finding.location",
        "INVALID_LOCATION",
    );
    assert!(review_contracts::normalize_location(&"././docs".into())
        .unwrap()
        .equals("./docs"));
}

#[test]
fn hashed_reference_and_finding_location_scope_have_distinct_prefix_rules() {
    let scope = Value::List(vec!["repo:STRASSE/ΔΟΣ/".into()]);
    assert!(review_contracts::ref_within_scope(
        &"repo:Straße/δος/file#sha256=ignored".into(),
        &scope
    )
    .unwrap());
    assert!(review_contracts::location_within_scope(&"Straße/δος/file".into(), &scope).unwrap());
    assert!(!review_contracts::location_within_scope(
        &"Straße/δος/file".into(),
        &Value::List(vec!["REPO:STRASSE/ΔΟΣ/".into()])
    )
    .unwrap());
    assert!(!review_contracts::ref_within_scope(
        &"repo:docs2/file".into(),
        &Value::List(vec!["repo:docs".into()])
    )
    .unwrap());
}

#[test]
fn finding_evidence_validation_precedes_identifier_and_normalizes_ids() {
    let allowed = Value::List(vec!["E1".into(), "E2".into()]);
    let result = review_contracts::validate_findings(
        &Value::List(vec![finding()]),
        &allowed,
        "findings",
        Some(&Value::List(vec!["AC-001".into()])),
        Some(&Value::List(vec!["repo:strasse/δοσ/".into()])),
        true,
    )
    .unwrap();
    assert!(result[0]["location"].is_text("strasse/δοσ"));
    assert!(result[0]["fix_variant"].is_text("Fix this"));
    assert!(result[0]["evidence_ids"].py_eq(&allowed));
    let mut bad = finding();
    bad.set("finding_id", 12i64.into());
    bad.set("evidence_ids", Value::List(vec!["E3".into()]));
    code(
        review_contracts::validate_findings(
            &Value::List(vec![bad]),
            &allowed,
            "findings",
            None,
            None,
            false,
        )
        .unwrap_err(),
        "findings[0].evidence_ids",
        "FINDING_EVIDENCE_MISSING",
    );
}

#[test]
fn scope_revalidation_preserves_casefold_expansion_limit_and_empty_trimmed_fix() {
    let mut value = finding();
    value.set("location", "ß".repeat(1024).into());
    value.set("fix_variant", " ".into());
    let evidence = Value::List(vec!["E1".into(), "E2".into()]);
    let result = review_contracts::validate_findings(
        &Value::List(vec![value.clone()]),
        &evidence,
        "findings",
        None,
        None,
        false,
    )
    .unwrap();
    assert_eq!(result[0]["location"].as_string().unwrap().len(), 2048);
    assert!(result[0]["fix_variant"].is_text(""));
    code(
        review_contracts::validate_findings(
            &Value::List(vec![value]),
            &evidence,
            "findings",
            None,
            Some(&Value::List(vec![])),
            false,
        )
        .unwrap_err(),
        "finding.location",
        "INVALID_STRING",
    );
}

fn preliminary() -> Value {
    Value::object([
        ("schema_version", true.into()),
        ("kind", "review_preliminary".into()),
        ("run_id", "RUN".into()),
        ("parent_branch_id", "BRANCH".into()),
        ("parent_attempt_id", "ATTEMPT".into()),
        ("generation", 0i64.into()),
        ("findings", Value::List(vec![])),
        (
            "evidence",
            Value::List(vec![Value::object([
                ("evidence_id", "E1".into()),
                ("kind", "source".into()),
                ("sha256", "a".repeat(64).into()),
            ])]),
        ),
    ])
}

fn approved() -> Value {
    Value::List(vec![Value::object([
        ("kind", "source".into()),
        (
            "ref",
            format!("repo:docs/source.md#sha256={}", "a".repeat(64)).into(),
        ),
        ("sha256", "a".repeat(64).into()),
    ])])
}

#[test]
fn preliminary_requires_exactly_one_approved_kind_digest_and_resolved_reference() {
    let value = preliminary();
    let approved = approved();
    let result = review_contracts::validate_preliminary(
        &value,
        &"RUN".into(),
        &"BRANCH".into(),
        &"ATTEMPT".into(),
        &0i64.into(),
        Some(&approved),
        false,
    )
    .unwrap();
    assert!(result["evidence"].as_list().unwrap()[0]["ref"]
        .py_eq(&approved.as_list().unwrap()[0]["ref"]));
    review_contracts::validate_preliminary(
        &result,
        &"RUN".into(),
        &"BRANCH".into(),
        &"ATTEMPT".into(),
        &0i64.into(),
        Some(&approved),
        true,
    )
    .unwrap();
    let duplicate = Value::List(vec![
        approved.as_list().unwrap()[0].clone(),
        approved.as_list().unwrap()[0].clone(),
    ]);
    code(
        review_contracts::validate_preliminary(
            &value,
            &"RUN".into(),
            &"BRANCH".into(),
            &"ATTEMPT".into(),
            &0i64.into(),
            Some(&duplicate),
            false,
        )
        .unwrap_err(),
        "review_preliminary.evidence[0]",
        "DELEGATION_EVIDENCE_UNAPPROVED",
    );
    code(
        review_contracts::validate_preliminary(
            &value,
            &"RUN".into(),
            &"BRANCH".into(),
            &"ATTEMPT".into(),
            &0i64.into(),
            Some(&approved),
            true,
        )
        .unwrap_err(),
        "review_preliminary.evidence[0]",
        "MISSING_FIELD",
    );
}

#[test]
fn preliminary_approved_digest_binding_precedes_registry_key_hashability() {
    let value = preliminary();
    let mut item = approved().as_list().unwrap()[0].clone();
    item.set("kind", Value::List(vec![]));
    item.set("sha256", "b".repeat(64).into());
    code(
        review_contracts::validate_preliminary(
            &value,
            &"RUN".into(),
            &"BRANCH".into(),
            &"ATTEMPT".into(),
            &0i64.into(),
            Some(&Value::List(vec![item.clone()])),
            false,
        )
        .unwrap_err(),
        "approved_evidence.ref",
        "INPUT_DIGEST_MISMATCH",
    );
    item.set("sha256", "a".repeat(64).into());
    assert!(
        matches!(review_contracts::validate_preliminary(&value,&"RUN".into(),&"BRANCH".into(),&"ATTEMPT".into(),&0i64.into(),Some(&Value::List(vec![item])),false).unwrap_err(),ValidationFailure::Python{kind:"TypeError",message}if message.equals("unhashable type: 'list'"))
    );
}
