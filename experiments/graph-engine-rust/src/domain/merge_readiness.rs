use crate::compat::Value;

pub fn valid_commit_sha(value: &Value) -> bool {
    value.text().is_some_and(|s| {
        [40, 64].contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn assess_merge_readiness(input: &Value) -> Value {
    let mut blockers: Vec<String> = Vec::new();
    if !["head_sha", "reviewed_sha", "ci_head_sha"]
        .iter()
        .all(|key| valid_commit_sha(&input[key]))
    {
        blockers.push("INVALID_HEAD_EVIDENCE".into());
    }
    if !input["head_sha"].py_eq(&input["reviewed_sha"]) {
        blockers.push("REVIEW_HEAD_CHANGED".into());
    }
    if !input["head_sha"].py_eq(&input["ci_head_sha"]) {
        blockers.push("CI_HEAD_CHANGED".into());
    }
    if !input["graph_status"].is_text("complete") {
        blockers.push("GRAPH_INCOMPLETE".into());
    }
    if !input["review_decision"].is_text("APPROVED") {
        blockers.push("REVIEW_NOT_APPROVED".into());
    }
    if input["unresolved_review_count"].as_i64() != Some(0) {
        blockers.push("UNRESOLVED_REVIEWS".into());
    }
    let checks = input["required_check_ids"].as_list();
    let valid = checks.is_some_and(|items| {
        !items.is_empty()
            && items
                .iter()
                .all(|v| v.as_string().is_some_and(|s| !s.is_empty()))
            && items
                .iter()
                .enumerate()
                .all(|(i, v)| !items[..i].iter().any(|w| v.py_eq(w)))
    });
    if !valid {
        blockers.push("REQUIRED_CHECKS_UNSPECIFIED".into());
    } else {
        let mut ids: Vec<_> = checks
            .unwrap()
            .iter()
            .map(|v| v.as_string().unwrap())
            .collect();
        ids.sort();
        for id in ids {
            let name = id.utf8().unwrap_or_default();
            let outcome = input["checks"].get(&name);
            if !outcome.is_some_and(|v| v.is_text("success")) {
                blockers.push(format!(
                    "CHECK_{}:{name}",
                    if outcome.is_none_or(Value::is_null) {
                        "MISSING"
                    } else {
                        "NOT_PASSING"
                    }
                ));
            }
        }
    }
    if !input.get("dry_run").is_none_or(Value::is_true) {
        blockers.push("LIVE_EXECUTION_UNSUPPORTED".into());
    }
    Value::object([
        (
            "decision",
            if blockers.is_empty() {
                "DRY_RUN_READY"
            } else {
                "BLOCKED"
            }
            .into(),
        ),
        ("head_sha", input["head_sha"].clone()),
        (
            "blockers",
            Value::list(blockers.into_iter().map(Value::from)),
        ),
        ("merge_authorized", false.into()),
        ("deploy_authorized", false.into()),
    ])
}
