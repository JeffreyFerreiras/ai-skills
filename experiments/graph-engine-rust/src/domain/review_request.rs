use super::{
    contract_values::{contains, fail, object},
    contracts::{opaque, require_keys},
    errors::{
        container_equal, iterable_set, iterable_values, require_hashable, set_contains,
        string_item, ValidationResult,
    },
    hosts, python_operations,
    review_contracts::ref_within_scope,
    reviewer_delegation::unique_ids,
};
use crate::compat::Value;
use num_bigint::BigInt;
use std::collections::{BTreeMap, BTreeSet};

fn keyed(items: &Value, key: &str) -> ValidationResult<Vec<(Value, Value)>> {
    let mut values: Vec<(Value, Value)> = Vec::new();
    for item in iterable_values(items)? {
        let identity = string_item(&item, key)?.clone();
        require_hashable(&identity)?;
        if let Some((_, value)) = values
            .iter_mut()
            .find(|(previous, _)| container_equal(previous, &identity))
        {
            *value = item;
        } else {
            values.push((identity, item));
        }
    }
    Ok(values)
}

pub struct FanoutRequestContext<'a> {
    pub run_id: &'a Value,
    pub parent_branch_id: &'a Value,
    pub parent_attempt_id: &'a Value,
    pub round_number: &'a Value,
    pub plan_assignments: &'a Value,
    pub preliminary: &'a Value,
    pub limits: &'a Value,
    pub depth: &'a Value,
    pub historical: bool,
}

pub fn validate_fanout_request(
    value: &Value,
    context: &FanoutRequestContext<'_>,
) -> ValidationResult<Value> {
    if python_operations::greater_equal(context.depth, string_item(context.limits, "max_depth")?)? {
        return Err(fail("review_fanout_request", "DELEGATION_DEPTH_EXCEEDED"));
    }
    object(value, "review_fanout_request", "INVALID_OBJECT")?;
    const KEYS: &[&str] = &[
        "schema_version",
        "kind",
        "run_id",
        "parent_branch_id",
        "parent_attempt_id",
        "round",
        "members",
    ];
    require_keys(value, KEYS, KEYS, "review_fanout_request")?;
    if !value["schema_version"].py_eq(&1i64.into())
        || !value["kind"].is_text("review_fanout_request")
        || !value["run_id"].py_eq(context.run_id)
        || !value["parent_branch_id"].py_eq(context.parent_branch_id)
        || !value["parent_attempt_id"].py_eq(context.parent_attempt_id)
        || !value["round"].py_eq(context.round_number)
    {
        return Err(fail("review_fanout_request", "REQUEST_FENCE_MISMATCH"));
    }
    let members = value["members"]
        .as_list()
        .ok_or_else(|| fail("review_fanout_request.members", "DELEGATION_REQUEST_LIMIT"))?;
    if members.is_empty()
        || python_operations::greater(
            &members.len().into(),
            string_item(context.limits, "max_children_per_request")?,
        )?
    {
        return Err(fail(
            "review_fanout_request.members",
            "DELEGATION_REQUEST_LIMIT",
        ));
    }
    let assignments = keyed(context.plan_assignments, "assignment_id")?;
    let evidence = keyed(string_item(context.preliminary, "evidence")?, "evidence_id")?;
    let mut normalized = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    const MEMBER_KEYS: &[&str] = &[
        "assignment_id",
        "ordinal",
        "reason_code",
        "acceptance_ids",
        "evidence_ids",
    ];
    for (index, member) in members.iter().enumerate() {
        let field = format!("review_fanout_request.members[{index}]");
        object(member, &field, "INVALID_OBJECT")?;
        require_keys(member, MEMBER_KEYS, MEMBER_KEYS, &field)?;
        let assignment_id = opaque(&member["assignment_id"], &format!("{field}.assignment_id"))?;
        let assignment = assignments
            .iter()
            .find(|(key, _)| key.is_text(&assignment_id))
            .map(|(_, value)| value)
            .ok_or_else(|| {
                fail(
                    &format!("{field}.assignment_id"),
                    "DELEGATION_ASSIGNMENT_UNDECLARED",
                )
            })?;
        let retired = hosts::is_retired_python_model(string_item(assignment, "model")?)?;
        if retired && !context.historical {
            return Err(fail(&format!("{field}.assignment_id"), "MODEL_RETIRED"));
        }
        let ordinal = member["ordinal"]
            .strict_integer()
            .filter(|ordinal| *ordinal >= &BigInt::from(1))
            .ok_or_else(|| fail(&format!("{field}.ordinal"), "INVALID_ORDINAL"))?;
        if !seen.insert((assignment_id.clone(), ordinal.clone())) {
            return Err(fail(&field, "DUPLICATE_VALUE"));
        }
        let count = counts.entry(assignment_id.clone()).or_insert(0);
        *count += 1;
        if python_operations::greater(
            &Value::from(*count),
            string_item(assignment, "max_instances")?,
        )? {
            return Err(fail(&field, "DELEGATION_INSTANCE_LIMIT"));
        }
        let reason = opaque(&member["reason_code"], &format!("{field}.reason_code"))?;
        if !contains(
            string_item(assignment, "allowed_reason_codes")?,
            &Value::from(reason.clone()),
        )? {
            return Err(fail(
                &format!("{field}.reason_code"),
                "DELEGATION_REASON_FORBIDDEN",
            ));
        }
        let acceptance = unique_ids(
            &member["acceptance_ids"],
            &format!("{field}.acceptance_ids"),
        )?;
        if acceptance.is_empty() {
            return Err(fail(
                &format!("{field}.acceptance_ids"),
                "DELEGATION_ACCEPTANCE_FORBIDDEN",
            ));
        }
        let allowed = iterable_set(string_item(assignment, "allowed_acceptance_ids")?)?;
        if !acceptance.iter().all(|id| set_contains(&allowed, id)) {
            return Err(fail(
                &format!("{field}.acceptance_ids"),
                "DELEGATION_ACCEPTANCE_FORBIDDEN",
            ));
        }
        let ids = unique_ids(&member["evidence_ids"], &format!("{field}.evidence_ids"))?;
        if ids.is_empty()
            || !ids
                .iter()
                .all(|id| evidence.iter().any(|(key, _)| container_equal(key, id)))
        {
            return Err(fail(
                &format!("{field}.evidence_ids"),
                "DELEGATION_EVIDENCE_FORBIDDEN",
            ));
        }
        for id in &ids {
            let item = &evidence
                .iter()
                .find(|(key, _)| container_equal(key, id))
                .unwrap()
                .1;
            let kind = string_item(item, "kind")?;
            if !contains(string_item(assignment, "allowed_evidence_kinds")?, kind)? {
                return Err(fail(
                    &format!("{field}.evidence_ids"),
                    "DELEGATION_EVIDENCE_FORBIDDEN",
                ));
            }
        }
        for id in &ids {
            let item = &evidence
                .iter()
                .find(|(key, _)| container_equal(key, id))
                .unwrap()
                .1;
            if !ref_within_scope(
                string_item(item, "ref")?,
                string_item(assignment, "scope_refs")?,
            )? {
                return Err(fail(
                    &format!("{field}.evidence_ids"),
                    "DELEGATION_SCOPE_FORBIDDEN",
                ));
            }
        }
        normalized.push(Value::object([
            ("assignment_id", assignment_id.into()),
            ("ordinal", member["ordinal"].clone()),
            ("reason_code", reason.into()),
            ("acceptance_ids", Value::List(acceptance)),
            ("evidence_ids", Value::List(ids)),
        ]));
    }
    if !normalized.windows(2).all(|pair| {
        pair[0]["assignment_id"]
            .as_string()
            .cmp(&pair[1]["assignment_id"].as_string())
            .then_with(|| {
                pair[0]["ordinal"]
                    .strict_integer()
                    .cmp(&pair[1]["ordinal"].strict_integer())
            })
            .is_le()
    }) {
        return Err(fail("review_fanout_request.members", "NON_CANONICAL_ORDER"));
    }
    let mut result = value.clone();
    result.set("members", Value::List(normalized));
    Ok(result)
}
