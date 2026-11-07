use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
};
use crate::{
    compat::{self, PyString, Value},
    domain::errors,
};

pub trait CheckClockPort {
    fn utc_now(&self) -> EngineResult<String>;
}
pub struct VerifiedFence {
    pub branch: Value,
    pub envelope: Value,
}

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}
fn get<'a>(value: &'a Value, key: &str) -> EngineResult<Option<&'a Value>> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key))
}
fn mismatch<T>() -> EngineResult<T> {
    Err(EngineError::state("ATTEMPT_FENCE_MISMATCH"))
}

pub fn verify_check_fence(
    ledger: &dyn GraphLedgerReadPort,
    clock: &dyn CheckClockPort,
    run: &Value,
    fence: &Value,
    executor: bool,
) -> EngineResult<VerifiedFence> {
    let run_id = item(run, "run_id")?;
    let branch_id = item(fence, "branch_id")?;
    let Some(branch) = ledger.first(GraphRead::FenceNode, &[run_id.clone(), branch_id.clone()])?
    else {
        return mismatch();
    };
    let envelope = compat::loads_value(item(&branch, "envelope_json")?)?;
    let attempt_id = item(fence, "attempt_id")?;
    let attempt = ledger.first(
        GraphRead::FenceAttempt,
        &[run_id.clone(), branch_id.clone(), attempt_id.clone()],
    )?;
    let Some(attempt) = attempt else {
        return mismatch();
    };
    if !get(&envelope, "attempt_id")?
        .unwrap_or(&compat::value::NULL)
        .py_eq(attempt_id)
        || !get(&envelope, "claim_digest")?
            .unwrap_or(&compat::value::NULL)
            .py_eq(item(fence, "claim_digest")?)
        || !item(&attempt, "claim_digest")?.py_eq(item(fence, "claim_digest")?)
    {
        return mismatch();
    }
    let status = item(&branch, "status")?;
    errors::require_hashable(status)?;
    if !status.is_text("running") && !status.is_text("succeeded") {
        return mismatch();
    }
    if status.is_text("running") {
        let empty = Value::from("");
        let lease = get(&envelope, "lease_expires_at")?.unwrap_or(&empty);
        let now = PyString::from(clock.utc_now()?);
        let Value::String(lease) = lease else {
            return Err(EngineError::python(
                "TypeError",
                format!(
                    "'<=' not supported between instances of '{}' and 'str'",
                    lease.python_type()
                ),
            ));
        };
        if lease <= &now {
            return mismatch();
        }
    }
    let stage = item(&branch, "stage")?;
    errors::require_hashable(stage)?;
    let generation = item(
        run,
        if stage.is_text("implementation") || stage.is_text("delivery") {
            "implementation_generation"
        } else {
            "design_generation"
        },
    )?;
    if !item(&branch, "generation")?.py_eq(generation) {
        return mismatch();
    }
    if !executor {
        let route = item(run, "selected_route")?;
        errors::require_hashable(route)?;
        let source = if ["full_delivery", "delivery_only", "fast_path"]
            .iter()
            .any(|name| route.is_text(name))
        {
            "senior_engineer".into()
        } else if route.is_text("design_only") {
            "tech_lead".into()
        } else if route.is_text("advisory") {
            "advisory_reviewer".into()
        } else {
            Value::Null
        };
        if !item(&branch, "node_key")?.py_eq(&source) {
            return Err(EngineError::state("SOURCE_ATTEMPT_INVALID"));
        }
    }
    Ok(VerifiedFence { branch, envelope })
}
