use super::contracts::{self, ContractError};
use super::errors::LocatedValidationFailure;
pub use super::errors::ValidationFailure as ConstraintFailure;
use crate::compat::{PyString, Value};
use std::collections::{BTreeMap, BTreeSet};

const FIELDS: &[&str] = &[
    "id",
    "revision",
    "statement",
    "rationale",
    "state",
    "repository_id",
    "paths",
    "roles",
    "acceptance_ids",
    "finding_id",
    "confirmed_finding_ref",
    "accepted_fix_ref",
    "authority_ref",
    "invalidation_reason",
    "supersedes",
];
type CResult<T> = Result<T, ConstraintFailure>;

#[derive(Clone, Copy, Debug)]
pub enum ConstraintValidationSite {
    RecordState,
    RecordAudience,
    ContextKeys,
    ContextIndex,
}

pub(crate) type LocatedConstraintFailure = LocatedValidationFailure<ConstraintValidationSite>;

fn fail(field: &str, code: &str) -> ConstraintFailure {
    ConstraintFailure::Contract(ContractError::new(field, code))
}
fn require_hashable(value: &Value) -> CResult<()> {
    if value.hashable() {
        Ok(())
    } else {
        Err(ConstraintFailure::python(
            "TypeError",
            format!("unhashable type: '{}'", value.python_type()),
        ))
    }
}
fn list<'a>(value: &'a Value, field: &str, min: usize, max: usize) -> CResult<&'a [Value]> {
    value
        .as_list()
        .filter(|items| (min..=max).contains(&items.len()))
        .ok_or_else(|| fail(field, "INVALID_LIST"))
}
fn identity(record: &Value) -> String {
    format!(
        "{}:{}",
        record["id"].text().unwrap(),
        record["revision"].as_i64().unwrap()
    )
}
fn contains(root: &PyString, path: &PyString) -> bool {
    let trim = |text: &PyString| {
        let mut text = text.clone();
        while text.0.last() == Some(&47) {
            text.0.pop();
        }
        text
    };
    trim(root) == trim(path) || (root.ends_with("/") && path.0.starts_with(&root.0))
}
fn visit(
    identity: &PyString,
    graph: &BTreeMap<PyString, Vec<PyString>>,
    ancestors: &mut BTreeSet<PyString>,
    visited: &mut BTreeSet<PyString>,
) -> CResult<()> {
    if ancestors.contains(identity) {
        return Err(fail("supersedes", "SUPERSESSION_CYCLE"));
    }
    if visited.contains(identity) {
        return Ok(());
    }
    ancestors.insert(identity.clone());
    if let Some(predecessors) = graph.get(identity) {
        for predecessor in predecessors {
            visit(predecessor, graph, ancestors, visited)?;
        }
    }
    ancestors.remove(identity);
    visited.insert(identity.clone());
    Ok(())
}
pub fn validate_bundle(value: &Value) -> CResult<Value> {
    validate_bundle_with_site(value).map_err(|failure| failure.error)
}

pub(crate) fn validate_bundle_with_site(value: &Value) -> Result<Value, LocatedConstraintFailure> {
    if value.as_object().is_none() {
        return Err(fail("planning_constraints", "INVALID_OBJECT").into());
    }
    contracts::require_keys(
        value,
        &["kind", "schema_version", "records"],
        &["kind", "schema_version", "records"],
        "planning_constraints",
    )?;
    if !value["kind"].is_text("planning_constraints")
        || !value["schema_version"].py_eq(&1_i64.into())
        || value["records"]
            .as_list()
            .is_none_or(|items| items.len() > 32)
    {
        return Err(fail("planning_constraints", "SCHEMA_MISMATCH").into());
    }
    let mut identities = BTreeSet::new();
    let mut graph = BTreeMap::new();
    for record in value["records"].as_list().unwrap() {
        if record.as_object().is_none() {
            return Err(fail("constraint", "INVALID_OBJECT").into());
        }
        contracts::require_keys(record, FIELDS, FIELDS, "constraint")?;
        contracts::opaque(&record["id"], "constraint.id")?;
        if record["revision"]
            .as_i64()
            .is_none_or(|n| !(1..=1_000_000).contains(&n))
        {
            return Err(fail("constraint.revision", "INVALID_VALUE").into());
        }
        let key = identity(record);
        if !identities.insert(key.clone()) {
            return Err(fail("constraint", "DUPLICATE_ID").into());
        }
        for field in ["statement", "rationale"] {
            contracts::bounded_string(&record[field], &format!("constraint.{field}"), 4096)?;
        }
        contracts::opaque(&record["repository_id"], "repository_id")?;
        contracts::opaque(&record["finding_id"], "finding_id")?;
        require_hashable(&record["state"]).map_err(|error| {
            LocatedConstraintFailure::at(error, ConstraintValidationSite::RecordState)
        })?;
        if !["candidate", "active", "invalidated", "superseded"]
            .iter()
            .any(|state| record["state"].is_text(state))
        {
            return Err(fail("constraint.state", "INVALID_VALUE").into());
        }
        for path in list(&record["paths"], "constraint.paths", 1, 16)? {
            let normalized = contracts::lexical_relative(path, "ref")?;
            if path.as_string() != Some(&normalized)
                || normalized
                    .0
                    .iter()
                    .any(|&point| matches!(point, 42 | 63 | 91 | 93 | 123 | 125))
            {
                return Err(fail("constraint.paths", "INVALID_PATH").into());
            }
        }
        for field in ["roles", "acceptance_ids"] {
            let name = format!("constraint.{field}");
            let values = list(&record[field], &name, 1, 32)?;
            for item in values {
                require_hashable(item).map_err(|error| {
                    LocatedConstraintFailure::at(error, ConstraintValidationSite::RecordAudience)
                })?;
            }
            if values
                .iter()
                .enumerate()
                .any(|(i, v)| values[..i].iter().any(|w| v.container_eq(w)))
            {
                return Err(fail(&name, "INVALID_LIST").into());
            }
            for item in values {
                contracts::opaque(item, &name)?;
            }
        }
        for field in ["confirmed_finding_ref", "accepted_fix_ref"] {
            if !record[field].is_null() {
                contracts::validate_ref(&record[field], field, true)?;
            }
        }
        contracts::validate_ref(&record["authority_ref"], "authority_ref", false)?;
        if !record["invalidation_reason"].is_null() {
            contracts::bounded_string(&record["invalidation_reason"], "invalidation_reason", 1024)?;
        }
        if record["state"].is_text("invalidated") && !record["invalidation_reason"].truthy() {
            return Err(fail("invalidation_reason", "MISSING_FIELD").into());
        }
        let predecessors = list(&record["supersedes"], "supersedes", 0, 32)?;
        let mut links = Vec::new();
        for predecessor in predecessors {
            links.push(contracts::bounded_string(predecessor, "supersedes", 160)?.clone());
        }
        graph.insert(key.into(), links);
    }
    let mut visited = BTreeSet::new();
    for identity in graph.keys() {
        visit(identity, &graph, &mut BTreeSet::new(), &mut visited)?;
    }
    Ok(value.clone())
}
pub struct Selection<'a> {
    pub activated: bool,
    pub supported: &'a [Value],
    pub authority_refs: &'a [Value],
    pub normalize: &'a dyn Fn(&PyString) -> PyString,
}
pub fn select_constraints(
    bundle: &Value,
    context: &Value,
    options: Selection<'_>,
) -> CResult<Value> {
    select_constraints_with_site(bundle, context, options).map_err(|failure| failure.error)
}

pub(crate) fn select_constraints_with_site(
    bundle: &Value,
    context: &Value,
    options: Selection<'_>,
) -> Result<Value, LocatedConstraintFailure> {
    validate_bundle_with_site(bundle)?;
    const CONTEXT: &[&str] = &["repository_id", "acceptance_ids", "role", "scope_paths"];
    if context.as_object().is_none() {
        let keys = match context {
            Value::List(items) => items.clone(),
            Value::String(s) => {
                s.0.iter()
                    .map(|&p| Value::String(PyString(vec![p])))
                    .collect()
            }
            _ => {
                return Err(LocatedConstraintFailure::at(
                    ConstraintFailure::python(
                        "TypeError",
                        format!("'{}' object is not iterable", context.python_type()),
                    ),
                    ConstraintValidationSite::ContextKeys,
                ))
            }
        };
        for key in &keys {
            require_hashable(key).map_err(|error| {
                LocatedConstraintFailure::at(error, ConstraintValidationSite::ContextKeys)
            })?;
        }
        if CONTEXT
            .iter()
            .any(|key| !keys.iter().any(|value| value.is_text(key)))
        {
            return Err(fail("constraint_context", "MISSING_FIELD").into());
        }
        if keys
            .iter()
            .any(|value| !CONTEXT.iter().any(|key| value.is_text(key)))
        {
            return Err(fail("constraint_context", "UNKNOWN_FIELD").into());
        }
        return Err(LocatedConstraintFailure::at(
            ConstraintFailure::python(
                "TypeError",
                "list indices must be integers or slices, not str",
            ),
            ConstraintValidationSite::ContextIndex,
        ));
    }
    contracts::require_keys(context, CONTEXT, CONTEXT, "constraint_context")?;
    contracts::opaque(
        &context["repository_id"],
        "constraint_context.repository_id",
    )?;
    contracts::opaque(&context["role"], "constraint_context.role")?;
    for field in ["acceptance_ids", "scope_paths"] {
        for item in list(
            &context[field],
            &format!("constraint_context.{field}"),
            0,
            128,
        )? {
            if field == "acceptance_ids" {
                contracts::opaque(item, "constraint_context.acceptance_ids")?;
            } else if item.as_string() != Some(&contracts::lexical_relative(item, "ref")?) {
                return Err(fail("constraint_context.scope_paths", "INVALID_PATH").into());
            }
        }
    }
    let records = bundle["records"].as_list().unwrap();
    let superseded: BTreeSet<_> = records
        .iter()
        .flat_map(|r| {
            r["supersedes"]
                .as_list()
                .unwrap()
                .iter()
                .map(|s| s.as_string().unwrap().clone())
        })
        .collect();
    let mut selected = Vec::new();
    let mut candidates = Vec::new();
    let mut excluded = Vec::new();
    let includes = |values: &[Value], item: &Value| values.iter().any(|v| v.py_eq(item));
    for record in records {
        let key = identity(record);
        let state = record["state"].text().unwrap();
        let reason = if ["invalidated", "superseded"].contains(&state.as_str())
            || superseded.contains(&PyString::from(key.as_str()))
        {
            Some(if state == "active" {
                "superseded".to_owned()
            } else {
                state.clone()
            })
        } else if !record["repository_id"].py_eq(&context["repository_id"])
            || !includes(record["roles"].as_list().unwrap(), &context["role"])
        {
            Some("not_applicable".into())
        } else if !record["acceptance_ids"]
            .as_list()
            .unwrap()
            .iter()
            .all(|v| includes(context["acceptance_ids"].as_list().unwrap(), v))
        {
            Some("acceptance_mismatch".into())
        } else if context["scope_paths"].as_list().unwrap().is_empty() {
            Some("unresolved_scope".into())
        } else if !record["paths"].as_list().unwrap().iter().any(|root| {
            context["scope_paths"]
                .as_list()
                .unwrap()
                .iter()
                .any(|path| {
                    let root = (options.normalize)(root.as_string().unwrap());
                    let path = (options.normalize)(path.as_string().unwrap());
                    contains(&root, &path) || contains(&path, &root)
                })
        }) {
            Some("path_mismatch".into())
        } else if records.iter().any(|other| {
            other["id"].py_eq(&record["id"])
                && !other["revision"].py_eq(&record["revision"])
                && !superseded.contains(&PyString::from(identity(other)))
                && ["active", "candidate"]
                    .iter()
                    .any(|s| other["state"].is_text(s))
        }) {
            Some("conflicting_revisions".into())
        } else if !includes(options.supported, &record["confirmed_finding_ref"])
            || !includes(options.supported, &record["accepted_fix_ref"])
        {
            Some("missing_support".into())
        } else if options.activated && !includes(options.authority_refs, &record["authority_ref"]) {
            Some("authority_mismatch".into())
        } else {
            None
        };
        if let Some(reason) = reason {
            excluded.push(Value::object([
                ("id", key.into()),
                ("reason", reason.into()),
            ]));
        } else if state == "candidate" || !options.activated {
            candidates.push(record.clone());
        } else {
            selected.push(record.clone());
        }
    }
    let order = |a: &Value, b: &Value| {
        a["id"]
            .as_string()
            .cmp(&b["id"].as_string())
            .then_with(|| a["revision"].as_i64().cmp(&b["revision"].as_i64()))
    };
    selected.sort_by(order);
    candidates.sort_by(order);
    excluded.sort_by(|a, b| a["id"].as_string().cmp(&b["id"].as_string()));
    Ok(Value::object([
        ("active", Value::List(selected)),
        ("candidates", Value::List(candidates)),
        ("excluded", Value::List(excluded)),
    ]))
}
