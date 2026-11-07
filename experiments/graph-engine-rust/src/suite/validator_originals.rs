#[allow(dead_code)]
#[path = "../../tests/support/graph_case.rs"]
mod graph_case;

use graph_case::GraphFixture;
use graph_engine::{
    application::semantic_nodes::predates_phase_dispositions,
    compat::{self, Value},
    domain::{
        consolidation::{
            compute_delivery_outcome, compute_design_outcome,
            delivery_outcome_without_delegated_echoes, validate_consolidation_manifest,
        },
        errors::{ValidationFailure, ValidationResult},
        result_contract::validate_result_manifest,
    },
};

pub const CASES: &[(&str, fn())] = &[
    (
        "test_decision_precedence_is_order_independent",
        decision_precedence_is_order_independent,
    ),
    ("test_delivery_dominance", delivery_dominance),
    (
        "test_out_of_scope_findings_do_not_raise_delivery_outcome",
        out_of_scope_findings_do_not_raise_delivery_outcome,
    ),
    (
        "test_stored_cross_phase_dispositions_remain_valid_on_reverification",
        stored_cross_phase_dispositions_remain_valid_on_reverification,
    ),
    (
        "test_only_runs_from_earlier_engines_keep_shared_dispositions_on_reverification",
        only_runs_from_earlier_engines_keep_shared_dispositions_on_reverification,
    ),
    (
        "test_delivery_rejects_in_scope_approve_disposition",
        delivery_rejects_in_scope_approve_disposition,
    ),
    (
        "test_unknown_finding_scope_is_rejected",
        unknown_finding_scope_is_rejected,
    ),
    (
        "test_duplicate_and_conflicting_finding_ids_are_rejected",
        duplicate_and_conflicting_finding_ids_are_rejected,
    ),
];

fn json(text: &str) -> Value {
    compat::loads(text.as_bytes()).unwrap()
}

fn outcome(result: (Value, Vec<Value>), expected: &str, findings: &[&str]) {
    assert!(result.0.is_text(expected));
    assert!(Value::List(result.1).py_eq(&Value::list(findings.iter().map(|id| Value::from(*id)))));
}

fn contract(result: ValidationResult<Value>, field: Option<&str>, expected: &str) {
    match result {
        Err(ValidationFailure::Contract(error)) => {
            assert_eq!(error.code, expected);
            if let Some(field) = field {
                assert!(error.field.equals(field));
            }
        }
        result => panic!("expected contract {expected}, got {result:?}"),
    }
}

fn review_branch(stage: &str, node: &str, output: &str) -> Value {
    Value::object([
        ("node_key", node.into()),
        ("stage", stage.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", "branch-1".into()),
        (
            "output_contract",
            Value::object([
                ("artifact_kind", output.into()),
                ("artifact_required", false.into()),
            ]),
        ),
    ])
}

fn review_result(output: &str, decision: &str, findings: Value) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", "branch-1".into()),
        ("status", "succeeded".into()),
        ("output_kind", output.into()),
        ("evidence", Value::List(Vec::new())),
        ("attempt_id", "attempt-1".into()),
        ("claim_digest", "0".repeat(64).into()),
        ("decision", decision.into()),
        ("findings", findings),
    ])
}

pub fn decision_precedence_is_order_independent() {
    let fixture = GraphFixture::new();
    let approve = json(
        r#"{"status":"succeeded","mandatory":1,"result_json":"{\"decision\":\"APPROVE\",\"findings\":[]}","branch_id":"a"}"#,
    );
    let revise = json(
        r#"{"status":"succeeded","mandatory":1,"result_json":"{\"decision\":\"REVISE\",\"findings\":[{\"finding_id\":\"ARCH-001\"}]}","branch_id":"b"}"#,
    );
    let first = compute_design_outcome(&[approve.clone(), revise.clone()]).unwrap();
    let second = compute_design_outcome(&[revise.clone(), approve.clone()]).unwrap();
    assert!(first.0.py_eq(&second.0));
    assert!(Value::List(first.1).py_eq(&Value::List(second.1)));
    assert!(compute_design_outcome(&[approve, revise])
        .unwrap()
        .0
        .is_text("REVISE"));
    fixture.tear_down();
}

pub fn delivery_dominance() {
    let fixture = GraphFixture::new();
    let repair = json(
        r#"{"status":"succeeded","mandatory":1,"result_json":"{\"findings\":[{\"finding_id\":\"REV-001\",\"disposition\":\"repair\"}]}"}"#,
    );
    let redesign = json(
        r#"{"status":"succeeded","mandatory":1,"result_json":"{\"findings\":[{\"finding_id\":\"TEST-001\",\"disposition\":\"redesign\"}]}"}"#,
    );
    assert!(
        compute_delivery_outcome(&[repair.clone(), redesign.clone()])
            .unwrap()
            .0
            .is_text("REDESIGN")
    );
    let block = json(
        r#"{"status":"succeeded","mandatory":1,"result_json":"{\"decision\":\"BLOCK\",\"findings\":[{\"finding_id\":\"SEC-001\",\"disposition\":\"block\"}]}"}"#,
    );
    assert!(compute_delivery_outcome(&[repair, redesign, block])
        .unwrap()
        .0
        .is_text("BLOCK"));
    fixture.tear_down();
}

pub fn out_of_scope_findings_do_not_raise_delivery_outcome() {
    let fixture = GraphFixture::new();
    let member = json(
        r#"{"status":"succeeded","mandatory":1,"branch_id":"a","result_json":"{\"decision\":\"APPROVE\",\"findings\":[{\"finding_id\":\"REL-001\",\"disposition\":\"approve\",\"scope\":\"out_of_scope\"}]}"}"#,
    );
    outcome(
        compute_delivery_outcome(&[member.clone()]).unwrap(),
        "ACCEPT",
        &["REL-001"],
    );
    outcome(
        delivery_outcome_without_delegated_echoes(
            &[member],
            &[json(r#"{"parent_branch_id":"other","issues":[]}"#)],
        )
        .unwrap(),
        "ACCEPT",
        &["REL-001"],
    );
    fixture.tear_down();
}

pub fn stored_cross_phase_dispositions_remain_valid_on_reverification() {
    let fixture = GraphFixture::new();
    let branch = review_branch("design", "architect", "design_review");
    let result = review_result(
        "design_review",
        "REVISE",
        json(
            r#"[{"finding_id":"ARCH-001","disposition":"revise"},{"finding_id":"ARCH-002","disposition":"accept"}]"#,
        ),
    );
    contract(
        validate_result_manifest(&result, &branch, false),
        None,
        "PHASE_DISPOSITION_MISMATCH",
    );
    validate_result_manifest(&result, &branch, true).unwrap();
    let branch = json(
        r#"{"node_key":"supervisor_design_consolidation","run_id":"RUN-1","branch_id":"branch-2","generation":0}"#,
    );
    let mut consolidation = json(
        r#"{"schema_version":1,"kind":"design_consolidation","run_id":"RUN-1","join_id":"join-1","generation":0,"source_branch_ids":["branch-1"],"finding_dispositions":[{"finding_id":"ARCH-002","disposition":"accept"}],"outcome":"REVISE","attempt_id":"attempt-2"}"#,
    );
    consolidation.set("claim_digest", "0".repeat(64).into());
    validate_result_manifest(&consolidation, &branch, false).unwrap();
    consolidation.set(
        "finding_dispositions",
        json(r#"[{"finding_id":"ARCH-002","disposition":"invented"}]"#),
    );
    contract(
        validate_result_manifest(&consolidation, &branch, false),
        Some("disposition"),
        "UNKNOWN_VALUE",
    );
    fixture.tear_down();
}

pub fn only_runs_from_earlier_engines_keep_shared_dispositions_on_reverification() {
    let fixture = GraphFixture::new();
    assert!(predates_phase_dispositions(&json(r#"{"engine_version":"2.7.0"}"#)).unwrap());
    assert!(!predates_phase_dispositions(&json(r#"{"engine_version":"2.8.0"}"#)).unwrap());
    assert!(!predates_phase_dispositions(&json(r#"{"engine_version":"2.9.0"}"#)).unwrap());
    fixture.tear_down();
}

pub fn delivery_rejects_in_scope_approve_disposition() {
    let fixture = GraphFixture::new();
    let branch = review_branch("delivery", "code_reviewer", "delivery_review");
    let result = review_result(
        "delivery_review",
        "REVISE",
        json(
            r#"[{"finding_id":"REV-001","disposition":"repair"},{"finding_id":"REV-002","disposition":"approve","scope":"in_scope"}]"#,
        ),
    );
    contract(
        validate_result_manifest(&result, &branch, false),
        None,
        "PHASE_DISPOSITION_MISMATCH",
    );
    fixture.tear_down();
}

pub fn unknown_finding_scope_is_rejected() {
    let fixture = GraphFixture::new();
    let branch = review_branch("design", "architect", "design_review");
    let result = review_result(
        "design_review",
        "APPROVE",
        json(r#"[{"finding_id":"ARCH-001","disposition":"approve","scope":"OUT_OF_SCOPE"}]"#),
    );
    contract(
        validate_result_manifest(&result, &branch, false),
        Some("scope"),
        "UNKNOWN_VALUE",
    );
    fixture.tear_down();
}

pub fn duplicate_and_conflicting_finding_ids_are_rejected() {
    let fixture = GraphFixture::new();
    let mut members = vec![
        json(
            r#"{"status":"succeeded","mandatory":1,"branch_id":"a","result_json":"{\"decision\":\"REVISE\",\"findings\":[{\"finding_id\":\"REV-001\",\"disposition\":\"repair\"}]}"}"#,
        ),
        json(
            r#"{"status":"succeeded","mandatory":1,"branch_id":"b","result_json":"{\"decision\":\"REVISE\",\"findings\":[{\"finding_id\":\"REV-001\",\"disposition\":\"repair\"}]}"}"#,
        ),
    ];
    let manifest = json(
        r#"{"kind":"delivery_consolidation","run_id":"RUN-1","join_id":"join","generation":0,"source_branch_ids":["a","b"],"finding_dispositions":[{"finding_id":"REV-001","disposition":"repair"}],"outcome":"REPAIR"}"#,
    );
    let validate = |members: &[Value]| {
        validate_consolidation_manifest(
            &manifest,
            &Value::from("delivery"),
            &Value::from("RUN-1"),
            &Value::from("join"),
            &0_i64.into(),
            members,
            &[],
        )
    };
    contract(validate(&members), None, "DUPLICATE_FINDING_ID");
    members[1].set(
        "result_json",
        r#"{"decision":"REVISE","findings":[{"finding_id":"REV-001","disposition":"redesign"}]}"#
            .into(),
    );
    contract(validate(&members), None, "CONFLICTING_FINDING_DISPOSITION");
    fixture.tear_down();
}
