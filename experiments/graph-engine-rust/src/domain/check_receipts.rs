use super::{
    contracts::{self, ContractError},
    errors::{self, ValidationFailure},
    ids,
};
use crate::compat::{datetime, JsonError, Value};

#[derive(Debug)]
pub enum CheckFailure {
    State(&'static str),
    Validation(ValidationFailure),
    Serialization(JsonError),
}
impl From<ValidationFailure> for CheckFailure {
    fn from(error: ValidationFailure) -> Self {
        Self::Validation(error)
    }
}
impl From<ContractError> for CheckFailure {
    fn from(error: ContractError) -> Self {
        Self::Validation(error.into())
    }
}
impl From<JsonError> for CheckFailure {
    fn from(error: JsonError) -> Self {
        Self::Serialization(error)
    }
}
pub type CheckResult<T> = Result<T, CheckFailure>;

pub const RECEIPT_FIELDS: &[&str] = &[
    "schema_version",
    "kind",
    "run_id",
    "check_id",
    "command_id",
    "argv",
    "outcome",
    "exit_code",
    "timed_out",
    "stdout_sha256",
    "stderr_sha256",
    "repo_worktree_sha256",
    "started_at",
    "finished_at",
    "producer_actor",
    "producer_host_identity",
];
pub const INTEGRITY_FIELDS: &[&str] = &[
    "executor",
    "source",
    "source_generation",
    "task_digest",
    "plan_digest",
    "policy_digest",
    "reservation_id",
    "predecessor_ref",
    "coverage",
    "coverage_digest",
    "before_source_digest",
    "after_source_digest",
    "validity",
    "activation_ref",
    "review_source_binding_ref",
];

fn fail<T>(field: &str, code: &str) -> CheckResult<T> {
    Err(ContractError::new(field, code).into())
}

fn mapping_get<'a>(value: &'a Value, key: &str) -> CheckResult<Option<&'a Value>> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key))
}

pub fn configured_check<'a>(policy: &'a Value, check_id: &Value) -> CheckResult<&'a Value> {
    let checks = mapping_get(policy, "required_checks")?;
    if let Some(checks) = checks {
        if checks.as_object().is_none() {
            return Err(errors::attribute_error(checks, "get").into());
        }
    }
    errors::require_hashable(check_id)?;
    let check = checks.and_then(|checks| {
        checks
            .as_object()
            .unwrap()
            .iter()
            .find(|(key, _)| check_id.py_eq(&Value::String(key.clone())))
            .map(|(_, check)| check)
    });
    let Some(check) = check.filter(|check| check.as_object().is_some()) else {
        return Err(CheckFailure::State("UNKNOWN_CHECK"));
    };
    if !matches!(check.get("argv"), Some(Value::List(argv)) if !argv.is_empty()) {
        return Err(CheckFailure::State("CHECK_COMMAND_NOT_CONFIGURED"));
    }
    Ok(check)
}

fn exact_python_keys(value: &Value, fields: &[&str], name: &str) -> CheckResult<()> {
    let keys = Value::list(fields.iter().map(|field| Value::from(*field)));
    Ok(contracts::require_python_keys(value, &keys, &keys, name)?)
}

pub fn validate_body(
    receipt: &Value,
    run_id: &Value,
    check_id: &Value,
    expected: &Value,
) -> CheckResult<Value> {
    if receipt.as_object().is_none() {
        return fail("check_receipt", "INVALID_OBJECT");
    }
    exact_python_keys(receipt, RECEIPT_FIELDS, "check_receipt")?;
    if !receipt["schema_version"].py_eq(&1i64.into()) || !receipt["kind"].is_text("check_receipt") {
        return fail("check_receipt", "SCHEMA_MISMATCH");
    }
    if !receipt["run_id"].py_eq(run_id) || !receipt["check_id"].py_eq(check_id) {
        return fail("check_receipt", "IDENTITY_MISMATCH");
    }
    if !receipt["command_id"].py_eq(errors::string_item(expected, "command_id")?) {
        return fail("command_id", "COMMAND_MISMATCH");
    }
    if !receipt["argv"].py_eq(mapping_get(expected, "argv")?.unwrap_or(&Value::Null)) {
        return fail("argv", "COMMAND_MISMATCH");
    }
    errors::require_hashable(&receipt["outcome"])?;
    if (!receipt["outcome"].is_text("PASS") && !receipt["outcome"].is_text("FAIL"))
        || !matches!(&receipt["exit_code"], Value::Integer(_))
    {
        return fail("check_receipt", "OUTCOME_INVALID");
    }
    let success = receipt["exit_code"].py_eq(&0i64.into());
    if receipt["outcome"].is_text("PASS") != success {
        return fail("outcome", "OUTCOME_MISMATCH");
    }
    let Value::Bool(timed_out) = &receipt["timed_out"] else {
        return fail("timed_out", "INVALID_TYPE");
    };
    if *timed_out != receipt["exit_code"].py_eq(&124i64.into()) {
        return fail("timed_out", "OUTCOME_MISMATCH");
    }
    for field in ["stdout_sha256", "stderr_sha256", "repo_worktree_sha256"] {
        contracts::digest(&receipt[field], &format!("check_receipt.{field}"))?;
    }
    for field in [
        "started_at",
        "finished_at",
        "producer_actor",
        "producer_host_identity",
    ] {
        contracts::bounded_string(&receipt[field], &format!("check_receipt.{field}"), 256)?;
    }
    let timestamp = |value: &Value| -> CheckResult<datetime::DateTime> {
        datetime::receipt_datetime(value).map_err(|error| {
            if matches!(error.kind, "AttributeError" | "ValueError") {
                ContractError::new("check_receipt", "TIMESTAMP_INVALID").into()
            } else {
                ValidationFailure::python(error.kind, error.message).into()
            }
        })
    };
    let started = timestamp(&receipt["started_at"])?;
    let finished = timestamp(&receipt["finished_at"])?;
    let order = finished
        .compare(&started)
        .map_err(|error| ValidationFailure::python(error.kind, error.message))?;
    if order.is_lt() {
        return fail("check_receipt", "TIMESTAMP_INVALID");
    }
    Ok(receipt.clone())
}

pub fn integrity_base(receipt: &Value) -> CheckResult<Option<Value>> {
    let Some(fields) = receipt.as_object() else {
        return Ok(None);
    };
    if receipt["schema_version"].py_eq(&1i64.into()) {
        return Ok(None);
    }
    let mut base = fields
        .iter()
        .filter(|(key, _)| !INTEGRITY_FIELDS.iter().any(|field| key.equals(field)))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    if !receipt["schema_version"].py_eq(&2i64.into())
        || INTEGRITY_FIELDS
            .iter()
            .any(|field| receipt.get(field).is_none())
    {
        return fail("check_receipt", "SCHEMA_MISMATCH");
    }
    base.iter_mut()
        .find(|(key, _)| key.equals("schema_version"))
        .unwrap()
        .1 = 1i64.into();
    Ok(Some(Value::Object(base)))
}

pub fn validate_integrity_additions(receipt: &Value) -> CheckResult<Value> {
    for field in ["executor", "source"] {
        let fence = &receipt[field];
        exact_python_keys(
            fence,
            &["branch_id", "attempt_id", "claim_digest", "result_digest"],
            field,
        )?;
        contracts::opaque(errors::string_item(fence, "branch_id")?, field)?;
        contracts::opaque(errors::string_item(fence, "attempt_id")?, field)?;
        contracts::digest(errors::string_item(fence, "claim_digest")?, field)?;
        let result = errors::string_item(fence, "result_digest")?;
        if !matches!(result, Value::Null) {
            contracts::digest(result, field)?;
        }
    }
    for field in [
        "task_digest",
        "plan_digest",
        "policy_digest",
        "coverage_digest",
        "before_source_digest",
        "after_source_digest",
    ] {
        contracts::digest(&receipt[field], field)?;
    }
    contracts::opaque(&receipt["reservation_id"], "reservation_id")?;
    if !matches!(&receipt["source_generation"], Value::Integer(value) if value.sign() != num_bigint::Sign::Minus)
    {
        return fail("source_generation", "INVALID_VALUE");
    }
    for field in [
        "activation_ref",
        "predecessor_ref",
        "review_source_binding_ref",
    ] {
        if !matches!(&receipt[field], Value::Null) {
            contracts::validate_ref(&receipt[field], field, true)?;
        }
    }
    let coverage = &receipt["coverage"];
    exact_python_keys(
        coverage,
        &["roots", "outputs", "activation_ref"],
        "coverage",
    )?;
    if !errors::string_item(coverage, "activation_ref")?.py_eq(&receipt["activation_ref"]) {
        return fail("coverage", "ACTIVATION_MISMATCH");
    }
    if !receipt["coverage_digest"].is_text(&ids::sha256_bytes(&ids::canonical_bytes(coverage)?)) {
        return fail("coverage", "DIGEST_DISAGREEMENT");
    }
    errors::require_hashable(&receipt["validity"])?;
    if !["valid", "source_changed", "unverifiable"]
        .iter()
        .any(|value| receipt["validity"].is_text(value))
    {
        return fail("validity", "INVALID_VALUE");
    }
    if receipt["validity"].is_text("valid")
        && !receipt["before_source_digest"].py_eq(&receipt["after_source_digest"])
    {
        return fail("validity", "SOURCE_CHANGED");
    }
    Ok(receipt.clone())
}
