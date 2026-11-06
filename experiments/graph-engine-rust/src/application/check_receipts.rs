use super::error::{EngineError, EngineResult};
use crate::{
    compat::{PyString, Value},
    domain::check_receipts::{self, CheckFailure},
};

pub trait CheckProvenancePort {
    fn actor(&self) -> EngineResult<PyString>;
    fn host_identity(&self) -> EngineResult<String>;
    fn repository_worktree_digest(&self) -> EngineResult<String>;
}

impl From<CheckFailure> for EngineError {
    fn from(error: CheckFailure) -> Self {
        match error {
            CheckFailure::State(code) => Self::state(code),
            CheckFailure::Validation(error) => error.into(),
            CheckFailure::Serialization(error) => error.into(),
        }
    }
}

pub fn validate_check_receipt(
    receipt: &Value,
    run_id: &Value,
    check_id: &Value,
    expected: &Value,
    provenance: &dyn CheckProvenancePort,
    check_freshness: bool,
) -> EngineResult<Value> {
    let receipt = check_receipts::validate_body(receipt, run_id, check_id, expected)?;
    if !receipt["producer_actor"].py_eq(&Value::String(provenance.actor()?))
        || !receipt["producer_host_identity"].is_text(&provenance.host_identity()?)
    {
        return Err(crate::domain::contracts::ContractError::new(
            "check_receipt",
            "PRODUCER_MISMATCH",
        )
        .into());
    }
    if check_freshness
        && !receipt["repo_worktree_sha256"].is_text(&provenance.repository_worktree_digest()?)
    {
        return Err(crate::domain::contracts::ContractError::new(
            "repo_worktree_sha256",
            "REPOSITORY_CHANGED",
        )
        .into());
    }
    Ok(receipt)
}

pub fn validate_receipt_integrity(
    receipt: &Value,
    run_id: &Value,
    check_id: &Value,
    expected: &Value,
    provenance: &dyn CheckProvenancePort,
) -> EngineResult<Value> {
    let Some(base) = check_receipts::integrity_base(receipt)? else {
        return validate_check_receipt(receipt, run_id, check_id, expected, provenance, false);
    };
    validate_check_receipt(&base, run_id, check_id, expected, provenance, false)?;
    Ok(check_receipts::validate_integrity_additions(receipt)?)
}
