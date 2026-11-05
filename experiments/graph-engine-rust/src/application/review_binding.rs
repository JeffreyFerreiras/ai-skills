use super::{
    error::{BranchResultTraceSite, EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    graph_review_collection::cumulative_binding,
};
use crate::{
    compat::{self, Value},
    domain::{
        contract_values::get,
        contracts::ContractError,
        errors::{iterable_values, require_hashable, set_contains, string_item},
        python_operations::sort_comparable_values,
    },
};

const BINDING_FIELDS: [(&str, &str); 4] = [
    ("review_request_slots", "DELEGATION_COLLECTION_MISMATCH"),
    ("child_members", "DELEGATION_MEMBER_MISMATCH"),
    ("terminal_non_successes", "DELEGATION_TERMINAL_MISMATCH"),
    ("finding_sources", "DELEGATION_FINDING_SOURCE_MISMATCH"),
];

fn insert(values: &mut Vec<Value>, value: &Value) -> EngineResult<()> {
    require_hashable(value)?;
    if !set_contains(values, value) {
        values.push(value.clone());
    }
    Ok(())
}

pub(super) fn validate_review_continuation_result(
    connection: &dyn GraphLedgerReadPort,
    branch: &Value,
    envelope: &Value,
    result: &Value,
) -> EngineResult<()> {
    let continuation = get(envelope, "review_continuation")?;
    if !continuation.truthy() {
        return Ok(());
    }
    let trace_fields = |error| {
        EngineError::from(error).with_branch_result_trace(BranchResultTraceSite::ContinuationFields)
    };
    let keys = iterable_values(result).map_err(trace_fields)?;
    for key in &keys {
        require_hashable(key).map_err(trace_fields)?;
    }
    if BINDING_FIELDS
        .iter()
        .any(|(field, _)| !set_contains(&keys, &Value::from(*field)))
    {
        return Err(ContractError::new("result", "DELEGATION_BINDING_INCOMPLETE").into());
    }
    let expected = cumulative_binding(
        connection,
        string_item(branch, "branch_id")?,
        string_item(branch, "generation")?,
    )?;
    if !continuation.py_eq(&expected) {
        return Err(ContractError::new(
            "result.review_request_slots",
            "DELEGATION_COLLECTION_MISMATCH",
        )
        .into());
    }
    for (field, code) in BINDING_FIELDS {
        if !string_item(result, field)?.py_eq(&expected[field]) {
            return Err(ContractError::new(&format!("result.{field}"), code).into());
        }
    }
    let sources = expected["finding_sources"]
        .as_list()
        .expect("constructed provenance rows");
    if ambiguous_source_identities(sources)? {
        return Err(ContractError::new(
            "result.finding_sources",
            "DELEGATION_FINDING_SOURCE_AMBIGUOUS",
        )
        .into());
    }

    let mut canonical_ids = Vec::new();
    for slot in expected["review_request_slots"]
        .as_list()
        .expect("constructed slot rows")
    {
        let slot_id = &slot.as_list().expect("constructed slot row")[0];
        let row = connection
            .first(GraphRead::ReviewCollection, std::slice::from_ref(slot_id))?
            .ok_or_else(|| {
                EngineError::python("TypeError", "'NoneType' object is not subscriptable")
            })?;
        let content = compat::loads_value(string_item(&row, "content_json")?)?;
        for issue in iterable_values(string_item(&content, "issues")?)? {
            for source in iterable_values(string_item(&issue, "provenance")?)? {
                if string_item(&source, "branch_id")?
                    .py_eq(string_item(&issue, "canonical_branch_id")?)
                {
                    insert(&mut canonical_ids, string_item(&source, "finding_id")?)?;
                }
            }
        }
    }
    if missing_canonical_disposition(
        result,
        &canonical_ids,
        EngineError::with_branch_result_trace,
    )? {
        return Err(ContractError::new("result.findings", "DELEGATION_DISPOSITION_MISSING").into());
    }
    Ok(())
}

pub fn validate_review_result_binding(
    connection: &dyn GraphLedgerReadPort,
    envelope: &Value,
    result: &Value,
) -> EngineResult<()> {
    let continuation = get(envelope, "review_continuation")?;
    if !continuation.truthy() {
        return Ok(());
    }
    let rows = connection.all(
        GraphRead::SealedReviewCollections,
        &[
            string_item(envelope, "branch_id")?.clone(),
            string_item(envelope, "generation")?.clone(),
        ],
    )?;
    let slots = Value::list(
        rows.iter()
            .map(|row| {
                Ok(Value::list([
                    string_item(row, "request_slot_id")?.clone(),
                    string_item(row, "collection_digest")?.clone(),
                ]))
            })
            .collect::<EngineResult<Vec<_>>>()?,
    );
    let mut members = Vec::new();
    let mut terminal = Vec::new();
    let mut sources = Vec::new();
    let mut canonical_ids = Vec::new();
    for row in &rows {
        let content = compat::loads_value(string_item(row, "content_json")?)?;
        for member in iterable_values(string_item(&content, "members")?)? {
            members.push(Value::list([
                string_item(row, "request_slot_id")?.clone(),
                string_item(&member, "branch_id")?.clone(),
                string_item(&member, "assignment_id")?.clone(),
                string_item(&member, "ordinal")?.clone(),
            ]));
            if !string_item(&member, "status")?.is_text("succeeded") {
                terminal.push(Value::object([
                    (
                        "request_slot_id",
                        string_item(row, "request_slot_id")?.clone(),
                    ),
                    ("branch_id", string_item(&member, "branch_id")?.clone()),
                    ("status", string_item(&member, "status")?.clone()),
                    (
                        "reason_code",
                        string_item(string_item(&member, "terminal")?, "reason_code")?.clone(),
                    ),
                    (
                        "failure_code",
                        string_item(string_item(&member, "terminal")?, "failure_code")?.clone(),
                    ),
                ]));
            }
        }
        for issue in iterable_values(string_item(&content, "issues")?)? {
            for source in iterable_values(string_item(&issue, "provenance")?)? {
                // Validate .get before applying its absent-key default.
                get(&source, "request_slot_id")?;
                sources.push(Value::list([
                    source
                        .get("request_slot_id")
                        .unwrap_or(string_item(row, "request_slot_id")?)
                        .clone(),
                    string_item(&issue, "issue_key")?.clone(),
                    string_item(&source, "branch_id")?.clone(),
                    string_item(&source, "finding_id")?.clone(),
                    string_item(&source, "assignment_id")?.clone(),
                    string_item(&source, "ordinal")?.clone(),
                ]));
                if string_item(&source, "branch_id")?
                    .py_eq(string_item(&issue, "canonical_branch_id")?)
                {
                    insert(&mut canonical_ids, string_item(&source, "finding_id")?)?;
                }
            }
        }
    }
    sort_comparable_values(&mut sources)?;
    let expected = Value::object([
        ("review_request_slots", slots),
        ("child_members", Value::List(members)),
        ("terminal_non_successes", Value::List(terminal)),
        ("finding_sources", Value::List(sources.clone())),
    ]);
    let mut matches = continuation.py_eq(&expected);
    for (field, _) in BINDING_FIELDS {
        if matches {
            matches = get(result, field)?.py_eq(&expected[field]);
        }
    }
    if !matches {
        return Err(EngineError::state("DELEGATION_BINDING_INCOMPLETE"));
    }
    if ambiguous_source_identities(&sources)? {
        return Err(EngineError::state("DELEGATION_FINDING_SOURCE_AMBIGUOUS"));
    }
    if missing_canonical_disposition(result, &canonical_ids, |error, _| error)? {
        return Err(EngineError::state("DELEGATION_DISPOSITION_MISSING"));
    }
    Ok(())
}

fn ambiguous_source_identities(sources: &[Value]) -> EngineResult<bool> {
    let mut identities: Vec<Vec<Value>> = Vec::new();
    let mut ambiguous = false;
    for source in sources {
        let fields = source.as_list().expect("constructed provenance row");
        let identity = [0, 2, 3].map(|index| fields[index].clone()).to_vec();
        for field in &identity {
            require_hashable(field)?;
        }
        if identities.iter().any(|previous| {
            previous
                .iter()
                .zip(&identity)
                .all(|(a, b)| a.container_eq(b))
        }) {
            ambiguous = true;
        }
        identities.push(identity);
    }
    Ok(ambiguous)
}

fn missing_canonical_disposition(
    result: &Value,
    canonical_ids: &[Value],
    trace: impl Fn(EngineError, BranchResultTraceSite) -> EngineError,
) -> EngineResult<bool> {
    let empty = Value::List(Vec::new());
    get(result, "findings")?;
    let findings = result.get("findings").unwrap_or(&empty);
    let mut final_ids = Vec::new();
    let findings = iterable_values(findings)
        .map_err(|error| trace(error.into(), BranchResultTraceSite::FindingsIterable))?;
    for finding in findings {
        if finding.as_object().is_some() {
            insert(&mut final_ids, get(&finding, "finding_id")?)
                .map_err(|error| trace(error, BranchResultTraceSite::FindingId))?;
        }
    }
    Ok(canonical_ids
        .iter()
        .any(|value| !set_contains(&final_ids, value)))
}

#[cfg(test)]
#[path = "review_binding_tests.rs"]
mod tests;
