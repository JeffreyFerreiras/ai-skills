use super::{
    contracts::{self, CResult, ContractError},
    hosts,
};
use crate::compat::{PyString, Value};
use std::collections::BTreeSet;

pub const LIMITS: &[(&str, i64)] = &[
    ("max_depth", 1),
    ("max_children_per_request", 3),
    ("max_children_per_run", 6),
    ("max_request_rounds", 2),
    ("max_weighted_dispatch_cost", 15),
];
fn fail(field: &str, code: &str) -> ContractError {
    ContractError::new(field, code)
}
fn limits(value: &Value, field: &str) -> CResult<Value> {
    if value.as_object().is_none() {
        return Err(fail(field, "INVALID_OBJECT"));
    }
    let keys: Vec<_> = LIMITS.iter().map(|(key, _)| *key).collect();
    contracts::require_keys(value, &keys, &keys, field)?;
    let mut normalized = Value::Object(vec![]);
    for (key, ceiling) in LIMITS {
        let amount = value[key]
            .as_i64()
            .filter(|n| (0..=*ceiling).contains(n))
            .ok_or_else(|| fail(&format!("{field}.{key}"), "LIMIT_MAY_NOT_INCREASE"))?;
        normalized.set(key, amount.into());
    }
    Ok(normalized)
}
pub(super) fn unique_ids(value: &Value, field: &str) -> CResult<Vec<Value>> {
    let items = value.as_list().ok_or_else(|| fail(field, "INVALID_LIST"))?;
    let mut normalized = Vec::new();
    let mut seen = BTreeSet::new();
    for item in items {
        let id = contracts::opaque(item, field)?;
        normalized.push(Value::from(id.clone()));
        seen.insert(id);
    }
    if seen.len() != normalized.len() {
        return Err(fail(field, "DUPLICATE_VALUE"));
    }
    normalized.sort_by(|a, b| a.as_string().cmp(&b.as_string()));
    Ok(normalized)
}
fn refs(value: &Value, field: &str) -> CResult<Vec<PyString>> {
    let items = value.as_list().ok_or_else(|| fail(field, "INVALID_LIST"))?;
    items
        .iter()
        .map(|v| contracts::validate_ref(v, field, false))
        .collect()
}
pub fn validate_policy_config(value: &Value, historical: bool) -> CResult<Option<Value>> {
    if value.is_null() {
        return Ok(None);
    }
    if value.as_object().is_none() {
        return Err(fail("reviewer_delegation", "INVALID_OBJECT"));
    }
    contracts::require_keys(
        value,
        &["limits", "assignments"],
        &["limits", "assignments"],
        "reviewer_delegation",
    )?;
    let limits = limits(&value["limits"], "reviewer_delegation.limits")?;
    let items = value["assignments"]
        .as_list()
        .ok_or_else(|| fail("reviewer_delegation.assignments", "INVALID_LIST"))?;
    let mut assignments = Vec::new();
    let mut seen = BTreeSet::new();
    const KEYS: &[&str] = &[
        "assignment_id",
        "role",
        "model",
        "reasoning_effort",
        "review_lens",
        "prompt_template",
        "allowed_reason_codes",
        "allowed_evidence_kinds",
        "scope_refs",
        "max_instances",
        "dispatch_weight",
    ];
    for (index, item) in items.iter().enumerate() {
        let field = format!("reviewer_delegation.assignments[{index}]");
        if item.as_object().is_none() {
            return Err(fail(&field, "INVALID_OBJECT"));
        }
        contracts::require_keys(item, KEYS, KEYS, &field)?;
        let assignment =
            contracts::opaque(&item["assignment_id"], &format!("{field}.assignment_id"))?;
        if !seen.insert(assignment.clone()) {
            return Err(fail(&field, "DUPLICATE_VALUE"));
        }
        let role = contracts::opaque(&item["role"], &format!("{field}.role"))?;
        if !["code_reviewer", "security_reviewer"].contains(&role.as_str()) {
            return Err(fail(&format!("{field}.role"), "DELEGATION_ROLE_FORBIDDEN"));
        }
        let model =
            contracts::bounded_string(&item["model"], &format!("{field}.model"), 128)?.clone();
        let effort = contracts::bounded_string(
            &item["reasoning_effort"],
            &format!("{field}.reasoning_effort"),
            32,
        )?
        .clone();
        let weight = hosts::dispatch_weight_for(
            &model.utf8().unwrap_or_default(),
            &effort.utf8().unwrap_or_default(),
            historical,
        )
        .ok_or_else(|| {
            fail(
                &format!("{field}.model"),
                "DELEGATION_ASSIGNMENT_UNSUPPORTED",
            )
        })?;
        if item["dispatch_weight"].as_i64() != Some(weight) {
            return Err(fail(
                &format!("{field}.dispatch_weight"),
                "DELEGATION_WEIGHT_INVALID",
            ));
        }
        let max = item["max_instances"]
            .as_i64()
            .filter(|n| (1..=3).contains(n))
            .ok_or_else(|| fail(&format!("{field}.max_instances"), "LIMIT_MAY_NOT_INCREASE"))?;
        let mut scope = refs(&item["scope_refs"], &format!("{field}.scope_refs"))?;
        if scope.iter().collect::<BTreeSet<_>>().len() != scope.len() {
            return Err(fail(&format!("{field}.scope_refs"), "DUPLICATE_VALUE"));
        }
        scope.sort();
        let lens =
            contracts::bounded_string(&item["review_lens"], &format!("{field}.review_lens"), 1024)?
                .clone();
        let prompt = contracts::bounded_string(
            &item["prompt_template"],
            &format!("{field}.prompt_template"),
            4096,
        )?
        .clone();
        let reasons = unique_ids(
            &item["allowed_reason_codes"],
            &format!("{field}.allowed_reason_codes"),
        )?;
        let kinds = unique_ids(
            &item["allowed_evidence_kinds"],
            &format!("{field}.allowed_evidence_kinds"),
        )?;
        assignments.push(Value::object([
            ("assignment_id", assignment.into()),
            ("role", role.into()),
            ("model", Value::String(model)),
            ("reasoning_effort", Value::String(effort)),
            ("review_lens", Value::String(lens)),
            ("prompt_template", Value::String(prompt)),
            ("allowed_reason_codes", Value::List(reasons)),
            ("allowed_evidence_kinds", Value::List(kinds)),
            (
                "scope_refs",
                Value::list(scope.into_iter().map(Value::String)),
            ),
            ("max_instances", max.into()),
            ("dispatch_weight", weight.into()),
        ]));
    }
    assignments.sort_by(|a, b| {
        a["assignment_id"]
            .as_string()
            .cmp(&b["assignment_id"].as_string())
    });
    Ok(Some(Value::object([
        ("limits", limits),
        ("assignments", Value::List(assignments)),
    ])))
}
pub fn validate_task_config(
    value: &Value,
    policy: Option<&Value>,
    acceptance: &[Value],
) -> CResult<Option<Value>> {
    if value.is_null() {
        return Ok(None);
    }
    let policy = policy.ok_or_else(|| fail("reviewer_delegation", "DELEGATION_DISABLED"))?;
    if value.as_object().is_none() {
        return Err(fail("reviewer_delegation", "INVALID_OBJECT"));
    }
    contracts::require_keys(
        value,
        &["limits", "assignments"],
        &["limits", "assignments"],
        "reviewer_delegation",
    )?;
    let task_limits = limits(&value["limits"], "reviewer_delegation.limits")?;
    let mut effective = Value::Object(vec![]);
    for (key, ceiling) in LIMITS {
        effective.set(
            key,
            task_limits[key]
                .as_i64()
                .unwrap()
                .min(policy["limits"][key].as_i64().unwrap())
                .min(*ceiling)
                .into(),
        );
    }
    let items = value["assignments"]
        .as_list()
        .ok_or_else(|| fail("reviewer_delegation.assignments", "INVALID_LIST"))?;
    let mut normalized = Vec::new();
    let mut seen = BTreeSet::new();
    const KEYS: &[&str] = &[
        "assignment_id",
        "allowed_reason_codes",
        "allowed_acceptance_ids",
        "allowed_evidence_kinds",
        "scope_refs",
        "max_instances",
    ];
    for (index, item) in items.iter().enumerate() {
        let field = format!("reviewer_delegation.assignments[{index}]");
        if item.as_object().is_none() {
            return Err(fail(&field, "INVALID_OBJECT"));
        }
        contracts::require_keys(item, KEYS, KEYS, &field)?;
        let id = contracts::opaque(&item["assignment_id"], &format!("{field}.assignment_id"))?;
        if !seen.insert(id.clone()) {
            return Err(fail(&field, "DUPLICATE_VALUE"));
        }
        let configured = policy["assignments"]
            .as_list()
            .unwrap()
            .iter()
            .find(|a| a["assignment_id"].is_text(&id))
            .ok_or_else(|| {
                fail(
                    &format!("{field}.assignment_id"),
                    "DELEGATION_ASSIGNMENT_UNDECLARED",
                )
            })?;
        let subset = |values: &[Value], allowed: &[Value]| {
            values.iter().all(|v| allowed.iter().any(|a| v.py_eq(a)))
        };
        let mut assignment = configured.clone();
        for (key, code, allowed) in [
            (
                "allowed_reason_codes",
                "DELEGATION_REASON_FORBIDDEN",
                configured["allowed_reason_codes"].as_list().unwrap(),
            ),
            (
                "allowed_acceptance_ids",
                "DELEGATION_ACCEPTANCE_FORBIDDEN",
                acceptance,
            ),
            (
                "allowed_evidence_kinds",
                "DELEGATION_EVIDENCE_FORBIDDEN",
                configured["allowed_evidence_kinds"].as_list().unwrap(),
            ),
        ] {
            let values = unique_ids(&item[key], &format!("{field}.{key}"))?;
            if !subset(&values, allowed) {
                return Err(fail(&format!("{field}.{key}"), code));
            }
            assignment.set(key, Value::List(values));
        }
        let mut scope = refs(&item["scope_refs"], &format!("{field}.scope_refs"))?;
        if !scope.iter().all(|value| {
            configured["scope_refs"]
                .as_list()
                .unwrap()
                .iter()
                .any(|allowed| allowed.as_string() == Some(value))
        }) {
            return Err(fail(
                &format!("{field}.scope_refs"),
                "DELEGATION_SCOPE_FORBIDDEN",
            ));
        }
        scope.sort();
        let max = item["max_instances"]
            .as_i64()
            .filter(|n| (1..=configured["max_instances"].as_i64().unwrap()).contains(n))
            .ok_or_else(|| fail(&format!("{field}.max_instances"), "LIMIT_MAY_NOT_INCREASE"))?;
        assignment.set(
            "scope_refs",
            Value::list(scope.into_iter().map(Value::String)),
        );
        assignment.set("max_instances", max.into());
        assignment.set("effect_capabilities", Value::List(vec![]));
        normalized.push(assignment);
    }
    normalized.sort_by(|a, b| {
        a["assignment_id"]
            .as_string()
            .cmp(&b["assignment_id"].as_string())
    });
    Ok(Some(Value::object([
        ("limits", effective),
        ("assignments", Value::List(normalized)),
    ])))
}
pub fn plan_fragment(config: Option<&Value>) -> Vec<Value> {
    config
        .and_then(|value| value["assignments"].as_list())
        .unwrap_or(&[])
        .to_vec()
}
