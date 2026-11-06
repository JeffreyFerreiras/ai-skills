use graph_engine::{
    compat::Value,
    domain::{review_collection, review_findings},
};

fn source(branch: &str, fix: &str, id: &str) -> Value {
    Value::object([
        ("branch_id", branch.into()),
        ("role", "code_reviewer".into()),
        ("model", "model".into()),
        ("assignment_id", "assignment".into()),
        ("ordinal", 1i64.into()),
        ("review_lens", "correctness".into()),
        ("request_slot_id", "SLOT".into()),
        (
            "findings",
            Value::List(vec![Value::object([
                ("finding_id", id.into()),
                ("acceptance_id", "AC-001".into()),
                ("location", "./Straße//FILE".into()),
                ("defect_id", "DEFECT".into()),
                ("fix_variant", fix.into()),
            ])]),
        ),
    ])
}

#[test]
fn consolidation_preserves_all_provenance_and_incompatible_fix_variants() {
    let sources = Value::List(vec![
        source("B", "  REPLACE  CHECK ", "R-002"),
        source("A", "replace check", "R-001"),
        source("C", "Keep the check", "R-003"),
    ]);
    let result = review_findings::consolidate_findings(&sources).unwrap();
    let groups = result.as_list().unwrap();
    assert_eq!(groups.len(), 1);
    let group = &groups[0];
    assert!(group["issue_key"].is_text("AC-001:strasse/file:DEFECT"));
    assert!(group["canonical_branch_id"].is_text("A"));
    assert!(group["conflict"].is_true());
    let provenance = group["provenance"].as_list().unwrap();
    assert_eq!(provenance.len(), 3);
    assert!(provenance[0]["branch_id"].is_text("A"));
    assert!(provenance[2]["branch_id"].is_text("C"));
    let variants = group["fix_variants"].as_list().unwrap();
    assert_eq!(variants.len(), 2);
    assert!(variants[0]["normalized"].is_text("keep the check"));
    assert!(variants[1]["text"].is_text("replace check"));
    assert_eq!(variants[1]["sources"].as_list().unwrap().len(), 2);
    assert!(variants[1]["sources"].as_list().unwrap()[1]["request_slot_id"].is_text("SLOT"));
}

#[test]
fn terminal_freeze_keeps_control_result_and_every_attempt_field() {
    let member = Value::object([
        ("branch_id", "B".into()),
        ("assignment_id", "REVIEW".into()),
        ("ordinal", 2i64.into()),
        ("role", "code_reviewer".into()),
        ("status", "timed_out".into()),
        ("reason_code", "LEASE_EXPIRED".into()),
        ("failure_code", "TIMEOUT".into()),
        ("result_digest", "a".repeat(64).into()),
        ("retry_count", 1i64.into()),
        ("max_retries", 1i64.into()),
        ("started_at", "start".into()),
        ("finished_at", "finish".into()),
    ]);
    let result = Value::object([
        ("kind", "timeout".into()),
        ("findings", Value::Null),
        (
            "control_detail",
            Value::object([("all_fields", "retained".into())]),
        ),
    ]);
    let artifact = Value::object([
        ("kind", "timeout".into()),
        ("ref", "ledger:timeout".into()),
        ("sha256", "a".repeat(64).into()),
    ]);
    let attempt = Value::object([
        ("attempt_number", 1i64.into()),
        ("attempt_id", "ATTEMPT".into()),
        ("claim_digest", "b".repeat(64).into()),
        ("started_at", "start".into()),
        ("finished_at", "finish".into()),
        ("outcome", "timed_out".into()),
    ]);
    let envelope = Value::object([
        ("evidence", Value::List(vec!["exact-evidence".into()])),
        ("artifact_ref", artifact.clone()),
        ("decision", Value::Null),
    ]);
    let frozen = review_collection::freeze_terminal_member(
        &member,
        &envelope,
        &result,
        &artifact,
        &Value::List(vec![attempt.clone()]),
    )
    .unwrap()
    .string_keyed_value()
    .unwrap();
    assert!(frozen["findings"].as_list().unwrap().is_empty());
    assert!(frozen["terminal"]["result"].py_eq(&result));
    assert!(frozen["terminal"]["result_artifact"].py_eq(&artifact));
    assert!(frozen["terminal"]["attempts"].py_eq(&Value::List(vec![attempt])));
    assert!(frozen["terminal"]["failure_code"].is_text("TIMEOUT"));
    assert!(frozen["terminal"]["reason_code"].is_text("LEASE_EXPIRED"));
}
