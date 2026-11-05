use graph_engine::{compat::Value, domain::merge_readiness::assess_merge_readiness};

fn evidence() -> Value {
    let sha = "a".repeat(40);
    Value::object([
        ("head_sha", sha.clone().into()),
        ("reviewed_sha", sha.clone().into()),
        ("ci_head_sha", sha.into()),
        (
            "required_check_ids",
            Value::list(["unit".into(), "integration".into()]),
        ),
        (
            "checks",
            Value::object([
                ("unit", "success".into()),
                ("integration", "success".into()),
            ]),
        ),
        ("review_decision", "APPROVED".into()),
        ("unresolved_review_count", 0_i64.into()),
        ("graph_status", "complete".into()),
        ("dry_run", true.into()),
    ])
}
fn blocker(result: &Value, expected: &str) -> bool {
    result["blockers"]
        .as_list()
        .unwrap()
        .iter()
        .any(|v| v.is_text(expected))
}
pub fn matching_snapshot() {
    let evidence = evidence();
    let before = evidence.clone();
    let result = assess_merge_readiness(&evidence);
    assert!(result["decision"].is_text("DRY_RUN_READY"));
    assert!(result["blockers"].as_list().unwrap().is_empty());
    assert!(!result["merge_authorized"].truthy());
    assert!(!result["deploy_authorized"].truthy());
    assert!(before.py_eq(&evidence));
}
pub fn changed_heads() {
    let mut evidence = evidence();
    evidence.set("head_sha", "b".repeat(40).into());
    assert!(
        assess_merge_readiness(&evidence)["blockers"].py_eq(&Value::list([
            "REVIEW_HEAD_CHANGED".into(),
            "CI_HEAD_CHANGED".into()
        ]))
    );
    evidence.set("ci_head_sha", evidence["head_sha"].clone());
    assert!(assess_merge_readiness(&evidence)["blockers"]
        .py_eq(&Value::list(["REVIEW_HEAD_CHANGED".into()])));
    evidence.set("reviewed_sha", evidence["head_sha"].clone());
    assert!(assess_merge_readiness(&evidence)["decision"].is_text("DRY_RUN_READY"));
}
pub fn required_checks() {
    for outcome in [
        None,
        Some("pending"),
        Some("failure"),
        Some("cancelled"),
        Some("skipped"),
        Some("neutral"),
        Some("unknown"),
    ] {
        let mut evidence = evidence();
        let mut checks = evidence["checks"].clone();
        if let Some(outcome) = outcome {
            checks.set("integration", outcome.into());
        } else {
            checks.remove("integration");
        }
        evidence.set("checks", checks);
        let result = assess_merge_readiness(&evidence);
        assert!(result["decision"].is_text("BLOCKED"));
        assert!(result["blockers"]
            .as_list()
            .unwrap()
            .iter()
            .any(|v| v.as_string().unwrap().ends_with(":integration")));
    }
    let mut evidence = evidence();
    evidence.set("ci_head_sha", "c".repeat(40).into());
    assert!(assess_merge_readiness(&evidence)["blockers"]
        .py_eq(&Value::list(["CI_HEAD_CHANGED".into()])));
}
pub fn review_and_graph_blockers() {
    for (key, value, expected) in [
        (
            "review_decision",
            "CHANGES_REQUESTED".into(),
            "REVIEW_NOT_APPROVED",
        ),
        ("review_decision", "PENDING".into(), "REVIEW_NOT_APPROVED"),
        (
            "unresolved_review_count",
            1_i64.into(),
            "UNRESOLVED_REVIEWS",
        ),
        ("graph_status", "blocked".into(), "GRAPH_INCOMPLETE"),
        (
            "required_check_ids",
            Value::List(vec![]),
            "REQUIRED_CHECKS_UNSPECIFIED",
        ),
        ("head_sha", "main".into(), "INVALID_HEAD_EVIDENCE"),
        ("dry_run", false.into(), "LIVE_EXECUTION_UNSUPPORTED"),
    ] {
        let mut evidence = evidence();
        evidence.set(key, value);
        let result = assess_merge_readiness(&evidence);
        assert!(result["decision"].is_text("BLOCKED"));
        assert!(blocker(&result, expected));
        assert!(!result["merge_authorized"].truthy());
        assert!(!result["deploy_authorized"].truthy());
    }
}
