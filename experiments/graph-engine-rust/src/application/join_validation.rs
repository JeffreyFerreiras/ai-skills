use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    timing::TERMINAL,
};
use crate::{
    compat::{self, repr, PyString, Value},
    domain::{
        contract_values::in_strings,
        errors::string_item,
        python_dictionary::{DictionaryValue, PythonDictionary},
        python_operations,
    },
};

pub fn join_members(
    connection: &dyn GraphLedgerReadPort,
    join_id: &Value,
) -> EngineResult<Vec<Value>> {
    connection.all(GraphRead::JoinMembers, &[join_id.clone()])
}

pub fn canonical_collection_members(
    connection: &dyn GraphLedgerReadPort,
    members: &[Value],
) -> EngineResult<Value> {
    let mut frozen = Vec::new();
    for member in members {
        let body = string_item(member, "result_json")?;
        let result = if body.truthy() {
            compat::loads_value(body)?
        } else {
            Value::Null
        };
        let mut result_ref = Value::Null;
        let mut result_kind = Value::Null;
        let digest = string_item(member, "result_digest")?;
        if digest.truthy() {
            let mut reference = PyString::from("ledger:");
            reference
                .0
                .extend(repr::value_str(string_item(member, "branch_id")?)?.0);
            reference.0.extend(PyString::from("#sha256=").0);
            reference.0.extend(repr::value_str(digest)?.0);
            let reference = Value::String(reference);
            let artifact = connection
                .first(GraphRead::Artifact, &[reference.clone()])?
                .ok_or_else(|| EngineError::state("COLLECTION_ARTIFACT_INVALID"))?;
            if !string_item(&artifact, "sha256")?.py_eq(digest) {
                return Err(EngineError::state("COLLECTION_ARTIFACT_INVALID"));
            }
            result_kind = string_item(&artifact, "kind")?.clone();
            result_ref = reference;
        }
        frozen.push(Value::object([
            ("branch_id", string_item(member, "branch_id")?.clone()),
            (
                "mandatory",
                string_item(member, "join_mandatory")?.truthy().into(),
            ),
            ("status", string_item(member, "status")?.clone()),
            ("result_ref", result_ref),
            ("result_kind", result_kind),
            ("result_digest", digest.clone()),
            ("result", result),
            ("failure_code", string_item(member, "failure_code")?.clone()),
            ("reason_code", string_item(member, "reason_code")?.clone()),
        ]));
    }
    Ok(Value::List(frozen))
}

#[derive(Debug)]
pub struct JoinValidation {
    pub join_status: &'static str,
    pub groups: PythonDictionary,
}

impl JoinValidation {
    fn new(status: &'static str, groups: PythonDictionary) -> Self {
        Self {
            join_status: status,
            groups,
        }
    }
    pub fn into_dictionary(self) -> PythonDictionary {
        let mut result = PythonDictionary::default();
        result.set("join_status", Value::from(self.join_status));
        result.set("groups", self.groups);
        result
    }
}

pub fn validate_run_join(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    join_id: &Value,
) -> EngineResult<Value> {
    let run_id = string_item(run, "run_id")?;
    let join = ledger
        .first(GraphRead::JoinForRun, &[join_id.clone(), run_id.clone()])?
        .ok_or_else(|| EngineError::state("JOIN_NOT_FOUND"))?;
    let validation = validate_join(ledger, run, &join)?;
    let mut result = super::graph_response::json_result(
        validation.join_status == "READY",
        validation.join_status,
        run_id.clone(),
        string_item(run, "state_revision")?.clone(),
    );
    result.set("join_id", join_id.clone());
    let groups = match validation.groups.string_keyed_value() {
        Some(groups) => groups,
        None => crate::compat::loads(
            &validation
                .groups
                .serialize(crate::compat::SerializationProfile::CLI)?,
        )?,
    };
    result.set("groups", groups);
    Ok(result)
}

fn invalid_group(key: &str, join_id: &Value) -> JoinValidation {
    let mut groups = PythonDictionary::default();
    groups.set(key, Value::List(vec![join_id.clone()]));
    JoinValidation::new("INVALID_STATE", groups)
}

fn retryable(member: &Value) -> EngineResult<bool> {
    Ok(string_item(member, "join_mandatory")?.truthy()
        && in_strings(string_item(member, "status")?, &["failed", "timed_out"])?
        && python_operations::less(
            string_item(member, "retry_count")?,
            string_item(member, "max_retries")?,
        )?)
}

pub fn validate_join(
    connection: &dyn GraphLedgerReadPort,
    _run: &Value,
    join: &Value,
) -> EngineResult<JoinValidation> {
    let sealed = string_item(join, "status")?.is_text("sealed");
    let join_id = string_item(join, "join_id")?;
    if sealed {
        return Ok(invalid_group("sealed", join_id));
    }
    let members = join_members(connection, join_id)?;
    let count = connection
        .first(GraphRead::JoinMemberCount, &[join_id.clone()])?
        .ok_or_else(|| {
            EngineError::python("TypeError", "'NoneType' object is not subscriptable")
        })?;
    if !Value::from(members.len()).py_eq(string_item(&count, "COUNT(*)")?) || members.is_empty() {
        return Ok(invalid_group("missing", join_id));
    }
    let mut groups = PythonDictionary::default();
    for member in &members {
        let status = string_item(member, "status")?;
        match groups.get_key_mut(status)? {
            Some(DictionaryValue::Value(Value::List(branches))) => {
                branches.push(string_item(member, "branch_id")?.clone())
            }
            None => groups.insert(
                status.clone(),
                Value::List(vec![string_item(member, "branch_id")?.clone()]).into(),
            )?,
            _ => unreachable!("join groups are constructed as lists"),
        }
        if string_item(member, "join_mandatory")?.truthy() && status.is_text("skipped") {
            return Ok(JoinValidation::new("INVALID_STATE", groups));
        }
    }
    if string_item(join, "kind")?.is_text("collection") {
        if string_item(join, "join_key")?.is_text("research_collection") {
            // The source list comprehension evaluates all members before testing emptiness.
            let mut found = false;
            for member in &members {
                if retryable(member)? {
                    found = true;
                }
            }
            if found {
                return Ok(JoinValidation::new("RETRY_REQUIRED", groups));
            }
        }
        let mut complete = true;
        for member in &members {
            if !in_strings(string_item(member, "status")?, TERMINAL)? {
                complete = false;
                break;
            }
        }
        return Ok(JoinValidation::new(
            if complete { "READY" } else { "NOT_READY" },
            groups,
        ));
    }
    for member in &members {
        if in_strings(
            string_item(member, "status")?,
            &["pending", "ready", "running"],
        )? {
            return Ok(JoinValidation::new("NOT_READY", groups));
        }
    }
    let mut found = false;
    for member in &members {
        if retryable(member)? {
            found = true;
        }
    }
    if found {
        return Ok(JoinValidation::new("RETRY_REQUIRED", groups));
    }
    let mut exhausted = false;
    for member in &members {
        if string_item(member, "join_mandatory")?.truthy()
            && in_strings(string_item(member, "status")?, &["failed", "timed_out"])?
        {
            exhausted = true;
        }
    }
    Ok(JoinValidation::new(
        if exhausted { "BLOCKED" } else { "READY" },
        groups,
    ))
}
