use super::{
    contract_values::{fail, object, sort_by_fields},
    contracts::{bounded_string, digest, opaque, require_keys, validate_ref},
    errors::{
        attribute_error, iterable_set, iterable_values, require_hashable, set_contains,
        string_item, ValidationFailure, ValidationResult,
    },
    reviewer_delegation::unique_ids,
};
use crate::compat::{unicode, PyString, Value};
use std::collections::BTreeSet;

fn strip_whitespace(text: &PyString) -> PyString {
    let start = text
        .0
        .iter()
        .position(|point| !unicode::whitespace(*point))
        .unwrap_or(text.len());
    let end = text
        .0
        .iter()
        .rposition(|point| !unicode::whitespace(*point))
        .map(|index| index + 1)
        .unwrap_or(start);
    PyString(text.0[start..end].to_vec())
}

fn slash_path(text: &PyString) -> PyString {
    PyString(
        text.0
            .iter()
            .map(|point| if *point == 92 { 47 } else { *point })
            .collect(),
    )
}
fn trim_slashes(text: &PyString, both: bool) -> PyString {
    let start = if both {
        text.0
            .iter()
            .position(|point| *point != 47)
            .unwrap_or(text.len())
    } else {
        0
    };
    let end = text
        .0
        .iter()
        .rposition(|point| *point != 47)
        .map(|index| index + 1)
        .unwrap_or(start);
    PyString(text.0[start..end].to_vec())
}

fn before_digest(text: &PyString) -> PyString {
    text.split_once("#sha256=")
        .map(|(before, _)| before)
        .unwrap_or_else(|| text.clone())
}

pub fn ref_within_scope(reference: &Value, scope_refs: &Value) -> ValidationResult<bool> {
    let reference = reference
        .as_string()
        .ok_or_else(|| attribute_error(reference, "partition"))?;
    let path = unicode::casefold(&trim_slashes(&slash_path(&before_digest(reference)), false));
    for scope in iterable_values(scope_refs)? {
        let text = scope
            .as_string()
            .ok_or_else(|| attribute_error(&scope, "replace"))?;
        let normalized = unicode::casefold(&trim_slashes(&slash_path(text), false));
        if path == normalized {
            return Ok(true);
        }
        let mut prefix = normalized;
        prefix.0.push(47);
        if path.0.starts_with(&prefix.0) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn normalize_location(value: &Value) -> ValidationResult<PyString> {
    let location = strip_whitespace(&slash_path(bounded_string(
        value,
        "finding.location",
        1024,
    )?));
    let mut points = Vec::new();
    for point in location.0 {
        if point != 47 || points.last() != Some(&47) {
            points.push(point);
        }
    }
    if points.starts_with(&[46, 47]) {
        points.drain(..2);
    }
    while points.last() == Some(&47) {
        points.pop();
    }
    if points.is_empty()
        || points.first() == Some(&47)
        || points
            .split(|point| *point == 47)
            .any(|part| part == [46, 46])
    {
        return Err(fail("finding.location", "INVALID_LOCATION"));
    }
    Ok(unicode::casefold(&PyString(points)))
}

pub fn location_within_scope(location: &Value, scope_refs: &Value) -> ValidationResult<bool> {
    let normalized = normalize_location(location)?;
    for reference in iterable_values(scope_refs)? {
        let text = reference
            .as_string()
            .ok_or_else(|| attribute_error(&reference, "startswith"))?;
        if !text.starts_with("repo:") {
            continue;
        }
        let path = unicode::casefold(&trim_slashes(
            &slash_path(&before_digest(&PyString(text.0[5..].to_vec()))),
            true,
        ));
        if normalized == path {
            return Ok(true);
        }
        let mut prefix = path;
        prefix.0.push(47);
        if normalized.0.starts_with(&prefix.0) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn validate_findings(
    value: &Value,
    evidence_ids: &Value,
    field: &str,
    acceptance_ids: Option<&Value>,
    scope_refs: Option<&Value>,
    exact_evidence: bool,
) -> ValidationResult<Vec<Value>> {
    let items = value.as_list().ok_or_else(|| fail(field, "INVALID_LIST"))?;
    let allowed = iterable_set(evidence_ids)?;
    let mut findings = Vec::new();
    let mut seen = BTreeSet::new();
    const KEYS: &[&str] = &[
        "finding_id",
        "acceptance_id",
        "location",
        "defect_id",
        "summary",
        "fix_variant",
        "evidence_ids",
    ];
    for (index, item) in items.iter().enumerate() {
        let item_field = format!("{field}[{index}]");
        object(item, &item_field, "INVALID_OBJECT")?;
        require_keys(item, KEYS, KEYS, &item_field)?;
        let evidence = unique_ids(&item["evidence_ids"], &format!("{item_field}.evidence_ids"))?;
        let subset = evidence.iter().all(|id| set_contains(&allowed, id));
        if (exact_evidence && (!subset || evidence.len() != allowed.len()))
            || (!exact_evidence && !subset)
        {
            return Err(fail(
                &format!("{item_field}.evidence_ids"),
                "FINDING_EVIDENCE_MISSING",
            ));
        }
        let id = &item["finding_id"];
        if id.as_string().is_none() || super::result_contract::finding_id(id).is_err() {
            return Err(fail(&format!("{item_field}.finding_id"), "INVALID_ID"));
        }
        let id = id.as_string().unwrap().clone();
        if !seen.insert(id.clone()) {
            return Err(fail(&format!("{item_field}.finding_id"), "DUPLICATE_ID"));
        }
        let acceptance = opaque(
            &item["acceptance_id"],
            &format!("{item_field}.acceptance_id"),
        )?;
        if let Some(allowed) = acceptance_ids {
            if !set_contains(&iterable_set(allowed)?, &Value::from(acceptance.clone())) {
                return Err(fail(
                    &format!("{item_field}.acceptance_id"),
                    "DELEGATION_ACCEPTANCE_FORBIDDEN",
                ));
            }
        }
        let location = normalize_location(&item["location"])?;
        if let Some(scope) = scope_refs {
            if !location_within_scope(&Value::String(location.clone()), scope)? {
                return Err(fail(
                    &format!("{item_field}.location"),
                    "DELEGATION_SCOPE_FORBIDDEN",
                ));
            }
        }
        let defect = opaque(
            string_item(item, "defect_id")?,
            &format!("{item_field}.defect_id"),
        )?;
        let summary =
            bounded_string(&item["summary"], &format!("{item_field}.summary"), 4096)?.clone();
        let fix = strip_whitespace(bounded_string(
            &item["fix_variant"],
            &format!("{item_field}.fix_variant"),
            4096,
        )?);
        findings.push(Value::object([
            ("finding_id", Value::String(id)),
            ("acceptance_id", acceptance.into()),
            ("location", Value::String(location)),
            ("defect_id", defect.into()),
            ("summary", Value::String(summary)),
            ("fix_variant", Value::String(fix)),
            ("evidence_ids", Value::List(evidence)),
        ]));
    }
    Ok(findings)
}

pub fn validate_preliminary(
    value: &Value,
    run_id: &Value,
    branch_id: &Value,
    attempt_id: &Value,
    generation: &Value,
    approved_evidence: Option<&Value>,
    resolved: bool,
) -> ValidationResult<Value> {
    object(value, "review_preliminary", "INVALID_OBJECT")?;
    const KEYS: &[&str] = &[
        "schema_version",
        "kind",
        "run_id",
        "parent_branch_id",
        "parent_attempt_id",
        "generation",
        "findings",
        "evidence",
    ];
    require_keys(value, KEYS, KEYS, "review_preliminary")?;
    if !value["schema_version"].py_eq(&1i64.into())
        || !value["kind"].is_text("review_preliminary")
        || !value["run_id"].py_eq(run_id)
        || !value["parent_branch_id"].py_eq(branch_id)
        || !value["parent_attempt_id"].py_eq(attempt_id)
        || !value["generation"].py_eq(generation)
    {
        return Err(fail("review_preliminary", "PRELIMINARY_FENCE_MISMATCH"));
    }
    let mut registry: Vec<(Value, String, Vec<PyString>)> = Vec::new();
    if let Some(approved) = approved_evidence {
        for item in iterable_values(approved)? {
            if item.as_object().is_none() {
                continue;
            }
            let empty = Value::from("");
            let rendered = crate::compat::repr::value_str(item.get("ref").unwrap_or(&empty))
                .map_err(|error| ValidationFailure::python(error.kind, error.message))?;
            if !["repo:", "runtime:", "profile:"]
                .iter()
                .any(|prefix| rendered.starts_with(prefix))
            {
                continue;
            }
            let reference =
                validate_ref(string_item(&item, "ref")?, "approved_evidence.ref", true)?;
            let approved_digest =
                digest(string_item(&item, "sha256")?, "approved_evidence.sha256")?;
            let marker = PyString::from("#sha256=");
            let suffix = reference
                .0
                .windows(marker.len())
                .rposition(|part| part == marker.0)
                .map(|position| PyString(reference.0[position + marker.len()..].to_vec()))
                .unwrap_or_default();
            if !suffix.equals(&approved_digest) {
                return Err(fail("approved_evidence.ref", "INPUT_DIGEST_MISMATCH"));
            }
            let kind = string_item(&item, "kind")?;
            require_hashable(kind)?;
            if let Some((_, _, references)) = registry.iter_mut().find(|(candidate, hash, _)| {
                candidate.container_eq(kind) && hash == &approved_digest
            }) {
                references.push(reference);
            } else {
                registry.push((kind.clone(), approved_digest, vec![reference]));
            }
        }
    }
    let mut evidence = Vec::new();
    let mut evidence_ids = BTreeSet::new();
    for (index, item) in iterable_values(&value["evidence"])?.iter().enumerate() {
        let field = format!("review_preliminary.evidence[{index}]");
        object(item, &field, "INVALID_OBJECT")?;
        let mut keys = vec!["evidence_id", "kind", "sha256"];
        if resolved {
            keys.push("ref");
        }
        require_keys(item, &keys, &keys, &field)?;
        let id = opaque(&item["evidence_id"], &format!("{field}.evidence_id"))?;
        if !evidence_ids.insert(id.clone()) {
            return Err(fail(&field, "DUPLICATE_VALUE"));
        }
        let kind = opaque(&item["kind"], &format!("{field}.kind"))?;
        let sha256 = digest(&item["sha256"], &format!("{field}.sha256"))?;
        let matches = registry
            .iter()
            .find(|(candidate, hash, _)| candidate.is_text(&kind) && hash == &sha256)
            .map(|(_, _, references)| references.as_slice())
            .unwrap_or(&[]);
        if matches.len() != 1 {
            return Err(fail(&field, "DELEGATION_EVIDENCE_UNAPPROVED"));
        }
        let reference = if resolved {
            validate_ref(&item["ref"], &format!("{field}.ref"), true)?
        } else {
            matches[0].clone()
        };
        if reference != matches[0] {
            return Err(fail(&field, "DELEGATION_EVIDENCE_UNAPPROVED"));
        }
        evidence.push(Value::object([
            ("evidence_id", id.into()),
            ("kind", kind.into()),
            ("ref", Value::String(reference)),
            ("sha256", sha256.into()),
        ]));
    }
    let ids = Value::List(evidence_ids.into_iter().map(Value::from).collect());
    let findings = validate_findings(
        &value["findings"],
        &ids,
        "review_preliminary.findings",
        None,
        None,
        false,
    )?;
    sort_by_fields(&mut evidence, &["evidence_id"]);
    let mut result = value.clone();
    result.set("evidence", Value::List(evidence));
    result.set("findings", Value::List(findings));
    Ok(result)
}
