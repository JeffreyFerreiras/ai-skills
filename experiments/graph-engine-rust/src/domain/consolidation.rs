use super::{
    contract_values::{fail, get, in_strings},
    contracts,
    errors::{
        iterable_values, require_hashable, set_contains, set_values, string_item,
        ValidationFailure, ValidationResult,
    },
    python_dictionary::{DictionaryValue, PythonDictionary},
    python_operations,
    result_contract::is_actionable_finding,
};
use crate::compat::{self, repr, Value};

fn parse(value: &Value) -> ValidationResult<Value> {
    compat::loads_value(value).map_err(|error| ValidationFailure::python(error.kind, error.message))
}

fn get_or<'a>(value: &'a Value, key: &str, default: &'a Value) -> ValidationResult<&'a Value> {
    get(value, key)?;
    Ok(value.get(key).unwrap_or(default))
}

fn key_error(value: &Value) -> ValidationResult<ValidationFailure> {
    let message = if let Some(text) = value.as_string() {
        repr::string_repr(text)
    } else {
        repr::value_str(value)
            .map_err(|error| ValidationFailure::python(error.kind, error.message))?
    };
    Ok(ValidationFailure::python("KeyError", message))
}

fn design_precedence(value: &Value, default: bool) -> ValidationResult<usize> {
    require_hashable(value)?;
    match value.text().as_deref() {
        Some("APPROVE") => Ok(0),
        Some("REVISE") => Ok(1),
        Some("BLOCK") => Ok(2),
        _ if default => Ok(2),
        _ => Err(key_error(value)?),
    }
}

fn delivery_precedence(value: &Value, default: bool) -> ValidationResult<usize> {
    require_hashable(value)?;
    match value.text().as_deref() {
        Some("accept") => Ok(0),
        Some("repair") => Ok(1),
        Some("redesign") => Ok(2),
        Some("block") => Ok(3),
        _ if default => Ok(0),
        _ => Err(key_error(value)?),
    }
}

fn outcome_precedence(value: &Value) -> ValidationResult<usize> {
    require_hashable(value)?;
    match value.text().as_deref() {
        Some("ACCEPT") => Ok(0),
        Some("REPAIR") => Ok(1),
        Some("REDESIGN") => Ok(2),
        Some("BLOCK") => Ok(3),
        _ => Err(key_error(value)?),
    }
}

fn sorted_findings(findings: &[Value]) -> ValidationResult<Vec<Value>> {
    let mut values = set_values(findings)?;
    python_operations::sort_comparable_values(&mut values)?;
    Ok(values)
}

pub fn compute_design_outcome(members: &[Value]) -> ValidationResult<(Value, Vec<Value>)> {
    let mut outcome = Value::from("APPROVE");
    let mut findings = Vec::new();
    for member in members {
        let decision = if !string_item(member, "status")?.is_text("succeeded") {
            Value::from(if string_item(member, "mandatory")?.truthy() {
                "BLOCK"
            } else {
                "APPROVE"
            })
        } else {
            let result = parse(string_item(member, "result_json")?)?;
            get(&result, "decision")?;
            let default = Value::from(if string_item(member, "mandatory")?.truthy() {
                "BLOCK"
            } else {
                "APPROVE"
            });
            let decision = result.get("decision").unwrap_or(&default).clone();
            let empty = Value::List(Vec::new());
            for finding in iterable_values(get_or(&result, "findings", &empty)?)? {
                findings.push(string_item(&finding, "finding_id")?.clone());
            }
            decision
        };
        if string_item(member, "mandatory")?.truthy()
            && design_precedence(&decision, true)? > design_precedence(&outcome, false)?
        {
            outcome = decision;
        }
    }
    Ok((outcome, sorted_findings(&findings)?))
}

pub fn compute_delivery_outcome(members: &[Value]) -> ValidationResult<(Value, Vec<Value>)> {
    delivery_outcome(members, None)
}

pub fn delivery_outcome_without_delegated_echoes(
    members: &[Value],
    collections: &[Value],
) -> ValidationResult<(Value, Vec<Value>)> {
    let empty = Value::List(Vec::new());
    let mut delegated: Vec<(Value, Vec<Value>)> = Vec::new();
    for collection in collections {
        let parent = get(collection, "parent_branch_id")?;
        contracts::opaque(parent, "parent_branch_id")?;
        for issue in iterable_values(get_or(collection, "issues", &empty)?)? {
            for source in iterable_values(get_or(&issue, "provenance", &empty)?)? {
                let finding = get(&source, "finding_id")?;
                contracts::opaque(finding, "finding_id")?;
                if let Some((_, ids)) = delegated.iter_mut().find(|(id, _)| id.container_eq(parent))
                {
                    if !set_contains(ids, finding) {
                        ids.push(finding.clone());
                    }
                } else {
                    delegated.push((parent.clone(), vec![finding.clone()]));
                }
            }
        }
    }
    delivery_outcome(members, Some(&delegated))
}

fn delivery_outcome(
    members: &[Value],
    delegated: Option<&[(Value, Vec<Value>)]>,
) -> ValidationResult<(Value, Vec<Value>)> {
    let mut precedence = 0;
    let mut findings = Vec::new();
    let empty = Value::List(Vec::new());
    for member in members {
        if !string_item(member, "status")?.is_text("succeeded") {
            if string_item(member, "mandatory")?.truthy() {
                precedence = 3;
            }
            continue;
        }
        let result = parse(string_item(member, "result_json")?)?;
        let delegated_ids = if let Some(delegated) = delegated {
            let branch = string_item(member, "branch_id")?;
            require_hashable(branch)?;
            delegated
                .iter()
                .find(|(parent, _)| parent.container_eq(branch))
                .map(|(_, ids)| ids)
        } else {
            None
        };
        if delegated_ids.is_none() && get(&result, "decision")?.is_text("BLOCK") {
            precedence = 3;
        }
        if delegated_ids.is_none() && get(&result, "decision")?.is_text("REDESIGN_REQUIRED") {
            precedence = precedence.max(2);
        }
        for finding in iterable_values(get_or(&result, "findings", &empty)?)? {
            let finding_id = string_item(&finding, "finding_id")?;
            findings.push(finding_id.clone());
            if string_item(member, "mandatory")?.truthy() && is_actionable_finding(&finding)? {
                if delegated.is_some() {
                    require_hashable(finding_id)?;
                }
                if let Some(delegated_ids) = delegated_ids {
                    if set_contains(delegated_ids, finding_id) {
                        continue;
                    }
                }
                precedence = precedence.max(delivery_precedence(
                    string_item(&finding, "disposition")?,
                    true,
                )?);
            }
        }
    }
    Ok((
        ["ACCEPT", "REPAIR", "REDESIGN", "BLOCK"][precedence].into(),
        sorted_findings(&findings)?,
    ))
}

fn dictionary_get<'a>(
    dictionary: &'a PythonDictionary,
    key: &Value,
) -> ValidationResult<Option<&'a Value>> {
    require_hashable(key)?;
    Ok(dictionary
        .entries
        .iter()
        .find(|(other, _)| other.container_eq(key))
        .map(|(_, value)| {
            let DictionaryValue::Value(value) = value else {
                unreachable!("owned consolidation dictionary")
            };
            value
        }))
}

fn dictionaries_equal(left: &PythonDictionary, right: &PythonDictionary) -> ValidationResult<bool> {
    if left.entries.len() != right.entries.len() {
        return Ok(false);
    }
    for (key, value) in &left.entries {
        let DictionaryValue::Value(value) = value else {
            unreachable!("owned consolidation dictionary")
        };
        if !dictionary_get(right, key)?.is_some_and(|other| value.container_eq(other)) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn source_finding_dispositions(members: &[Value]) -> ValidationResult<PythonDictionary> {
    let mut dispositions = PythonDictionary::default();
    let empty = Value::List(Vec::new());
    for member in members {
        if !string_item(member, "status")?.is_text("succeeded")
            || !string_item(member, "result_json")?.truthy()
        {
            continue;
        }
        let result = parse(string_item(member, "result_json")?)?;
        for finding in iterable_values(get_or(&result, "findings", &empty)?)? {
            let id = string_item(&finding, "finding_id")?;
            let disposition = string_item(&finding, "disposition")?;
            if let Some(previous) =
                dictionary_get(&dispositions, id)?.filter(|value| !value.is_null())
            {
                return Err(fail(
                    "finding_dispositions",
                    if previous.py_eq(disposition) {
                        "DUPLICATE_FINDING_ID"
                    } else {
                        "CONFLICTING_FINDING_DISPOSITION"
                    },
                ));
            }
            dispositions.insert(id.clone(), disposition.clone().into())?;
        }
    }
    Ok(dispositions)
}

pub fn delegated_issue_sources(collections: &[Value]) -> ValidationResult<PythonDictionary> {
    let mismatch = || fail("nested_collections", "DELEGATION_COLLECTION_MISMATCH");
    let mut grouped: Vec<(Value, Vec<Value>)> = Vec::new();
    let mut composite: Vec<(Vec<Value>, Value)> = Vec::new();
    for collection in collections {
        if !get(collection, "kind")?.is_text("review_nested_collection") {
            return Err(mismatch());
        }
        let slot = get(collection, "request_slot_id")?;
        contracts::opaque(slot, "request_slot_id")?;
        let issues = get(collection, "issues")?.as_list().ok_or_else(mismatch)?;
        for issue in issues {
            if issue.as_object().is_none() || get(issue, "provenance")?.as_list().is_none() {
                return Err(mismatch());
            }
            let key = get(issue, "issue_key")?;
            contracts::bounded_string(key, "issue_key", 4096)?;
            for source in string_item(issue, "provenance")?
                .as_list()
                .expect("checked provenance list")
            {
                if source.as_object().is_none() {
                    return Err(mismatch());
                }
                let branch = get(source, "branch_id")?;
                contracts::opaque(branch, "source_branch_id")?;
                let finding = get(source, "finding_id")?;
                contracts::opaque(finding, "finding_id")?;
                let identity = vec![slot.clone(), branch.clone(), finding.clone()];
                if let Some((_, previous)) = composite
                    .iter()
                    .find(|(other, _)| other.iter().zip(&identity).all(|(a, b)| a.container_eq(b)))
                {
                    return Err(fail(
                        "nested_collections",
                        if previous.py_eq(key) {
                            "DELEGATION_FINDING_SOURCE_DUPLICATE"
                        } else {
                            "DELEGATION_FINDING_SOURCE_AMBIGUOUS"
                        },
                    ));
                }
                composite.push((identity.clone(), key.clone()));
                if let Some((_, identities)) = grouped
                    .iter_mut()
                    .find(|(other, _)| other.container_eq(key))
                {
                    identities.push(Value::List(identity));
                } else {
                    grouped.push((key.clone(), vec![Value::List(identity)]));
                }
            }
        }
    }
    let mut result = PythonDictionary::default();
    for (key, mut identities) in grouped {
        python_operations::sort_comparable_values(&mut identities)?;
        result.insert(key, Value::List(identities).into())?;
    }
    Ok(result)
}

pub fn delegated_terminal_precedence(collections: &[Value]) -> ValidationResult<usize> {
    let mut precedence = 0;
    for collection in collections {
        let members = get(collection, "members")?
            .as_list()
            .ok_or_else(|| fail("nested_collections", "DELEGATION_COLLECTION_MISMATCH"))?;
        for member in members {
            if member.as_object().is_none()
                || get(member, "terminal")?.as_object().is_none()
                || !in_strings(
                    get(member, "status")?,
                    &["succeeded", "failed", "timed_out", "skipped"],
                )?
            {
                return Err(fail("nested_collections", "DELEGATION_COLLECTION_MISMATCH"));
            }
            if in_strings(get(member, "status")?, &["failed", "timed_out"])? {
                precedence = 3;
            }
        }
    }
    Ok(precedence)
}

pub fn validate_consolidation_manifest(
    manifest: &Value,
    stage: &Value,
    run_id: &Value,
    join_id: &Value,
    generation: &Value,
    members: &[Value],
    nested: &[Value],
) -> ValidationResult<Value> {
    let expected_kind = python_operations::add(stage, &Value::from("_consolidation"))?;
    if !get(manifest, "kind")?.py_eq(&expected_kind)
        || !get(manifest, "run_id")?.py_eq(run_id)
        || !get(manifest, "join_id")?.py_eq(join_id)
        || !get(manifest, "generation")?.py_eq(generation)
    {
        return Err(fail("consolidation", "CONSOLIDATION_ID_MISMATCH"));
    }
    let mut expected_sources = members
        .iter()
        .map(|member| string_item(member, "branch_id").cloned())
        .collect::<ValidationResult<Vec<_>>>()?;
    python_operations::sort_comparable_values(&mut expected_sources)?;
    if !get(manifest, "source_branch_ids")?.py_eq(&Value::List(expected_sources)) {
        return Err(fail("source_branch_ids", "SOURCE_SET_MISMATCH"));
    }
    let (expected_outcome, findings) = if stage.is_text("design") {
        compute_design_outcome(members)?
    } else if nested.is_empty() {
        compute_delivery_outcome(members)?
    } else {
        delivery_outcome_without_delegated_echoes(members, nested)?
    };
    let empty = Value::List(Vec::new());
    let dispositions = get_or(manifest, "finding_dispositions", &empty)?;
    let mut actual = PythonDictionary::default();
    for item in iterable_values(dispositions)? {
        if item.as_object().is_some() {
            actual.insert(
                get(&item, "finding_id")?.clone(),
                get(&item, "disposition")?.clone().into(),
            )?;
        }
    }
    let expected = source_finding_dispositions(members)?;
    if actual.entries.len() != python_operations::length(dispositions)?
        || !dictionaries_equal(&actual, &expected)?
    {
        return Err(fail("finding_dispositions", "FINDING_DISPOSITION_MISMATCH"));
    }
    let mut keys = expected
        .entries
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    python_operations::sort_comparable_values(&mut keys)?;
    if !Value::List(keys).py_eq(&Value::List(findings)) {
        return Err(fail("finding_dispositions", "FINDING_DISPOSITION_MISMATCH"));
    }
    if nested.is_empty() {
        if !get(manifest, "outcome")?.py_eq(&expected_outcome) {
            return Err(fail("outcome", "OUTCOME_PRECEDENCE_MISMATCH"));
        }
        if get_or(manifest, "delegated_finding_dispositions", &empty)?.truthy() {
            return Err(fail(
                "delegated_finding_dispositions",
                "DELEGATION_DISPOSITION_MISMATCH",
            ));
        }
        return Ok(expected_outcome);
    }
    let expected = delegated_issue_sources(nested)?;
    let delegated = get_or(manifest, "delegated_finding_dispositions", &empty)?;
    let mut actual = PythonDictionary::default();
    for item in iterable_values(delegated)? {
        if item.as_object().is_some() {
            actual.insert(get(&item, "issue_key")?.clone(), item.into())?;
        }
    }
    if actual.entries.len() != python_operations::length(delegated)?
        || actual.entries.len() != expected.entries.len()
        || actual.entries.iter().any(|(key, _)| {
            !expected
                .entries
                .iter()
                .any(|(other, _)| other.container_eq(key))
        })
    {
        return Err(fail(
            "delegated_finding_dispositions",
            "DELEGATION_DISPOSITION_MISMATCH",
        ));
    }
    for (key, item) in &actual.entries {
        let DictionaryValue::Value(item) = item else {
            unreachable!("owned consolidation dictionary")
        };
        if !get(item, "source_identities")?
            .py_eq(dictionary_get(&expected, key)?.expect("matching delegated issue key"))
            || !in_strings(
                get(item, "disposition")?,
                &["accept", "repair", "redesign", "block"],
            )?
        {
            return Err(fail(
                "delegated_finding_dispositions",
                "DELEGATION_DISPOSITION_MISMATCH",
            ));
        }
    }
    let mut precedence = outcome_precedence(&expected_outcome)?;
    for (_, item) in &actual.entries {
        let DictionaryValue::Value(item) = item else {
            unreachable!("owned consolidation dictionary")
        };
        precedence = precedence.max(delivery_precedence(
            string_item(item, "disposition")?,
            false,
        )?);
    }
    precedence = precedence.max(delegated_terminal_precedence(nested)?);
    let combined = Value::from(["ACCEPT", "REPAIR", "REDESIGN", "BLOCK"][precedence]);
    if !get(manifest, "outcome")?.py_eq(&combined) {
        return Err(fail("outcome", "OUTCOME_PRECEDENCE_MISMATCH"));
    }
    Ok(combined)
}
