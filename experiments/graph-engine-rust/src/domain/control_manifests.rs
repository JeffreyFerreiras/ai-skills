use super::{
    contracts::{self, ContractError},
    errors::ValidationResult,
};
use crate::compat::Value;

pub fn validate_control_manifest(
    value: &Value,
    kind: &str,
    run_id: &str,
    branch_id: Option<&str>,
    reason_code: &str,
) -> ValidationResult<Value> {
    if value.as_object().is_none() {
        return Err(ContractError::new("evidence_manifest", "INVALID_OBJECT").into());
    }
    let mut required = vec![
        "schema_version",
        "kind",
        "run_id",
        "reason_code",
        "evidence",
    ];
    if branch_id.is_some() {
        required.push("branch_id");
    }
    let mut allowed = required.clone();
    if kind == "timeout" {
        allowed.extend(["attempt_id", "claim_digest"]);
    }
    contracts::require_keys(value, &required, &allowed, "evidence_manifest")?;
    if !value["schema_version"].py_eq(&1_i64.into())
        || !value["kind"].is_text(kind)
        || !value["run_id"].is_text(run_id)
        || !value["reason_code"].is_text(reason_code)
        || branch_id.is_some_and(|branch| !value["branch_id"].is_text(branch))
    {
        return Err(ContractError::new("evidence_manifest", "CONTROL_MANIFEST_MISMATCH").into());
    }
    if kind == "timeout" {
        required.extend(["attempt_id", "claim_digest"]);
        contracts::require_keys(value, &required, &required, "evidence_manifest")?;
        contracts::opaque(&value["attempt_id"], "attempt_id")?;
        contracts::digest(&value["claim_digest"], "claim_digest")?;
    }
    if value["evidence"].as_list().is_none() {
        return Err(ContractError::new("evidence", "INVALID_LIST").into());
    }
    Ok(value.clone())
}
