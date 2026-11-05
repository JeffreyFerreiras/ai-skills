use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
};
use crate::{
    compat::{self, Value},
    domain::{
        contract_values::{get, in_strings},
        contracts,
        errors::string_item,
        execution, ids,
    },
};

fn invalid() -> EngineError {
    EngineError::state("EXECUTION_PLAN_STATE_INVALID")
}

pub fn validate_execution_plan(
    connection: &dyn GraphLedgerReadPort,
    run: &Value,
    task: &Value,
) -> EngineResult<Value> {
    let run_id = string_item(run, "run_id")?;
    let row = connection
        .first(GraphRead::ExecutionPlan, &[run_id.clone()])?
        .ok_or_else(invalid)?;
    let status = string_item(&row, "status")?;
    if !in_strings(status, &["pending", "approved", "rejected"])? {
        return Err(invalid());
    }
    let plan = match compat::loads_value(string_item(&row, "plan_json")?) {
        Ok(plan) => plan,
        Err(error) if matches!(error.kind, "TypeError" | "JSONDecodeError") => {
            return Err(invalid())
        }
        Err(error) => return Err(error.into()),
    };
    let requested = if get(&plan, "size_source")?.is_text("supervisor_override") {
        Some(string_item(&row, "size")?)
    } else {
        None
    };
    let requested_text = requested
        .filter(|value| !value.is_null())
        .map(|value| value.text().unwrap_or_else(|| "__invalid_size__".into()));
    let run_text = run_id.text().unwrap_or_default();
    let mut expected =
        execution::reconstruct_execution_plan(&run_text, task, &plan, requested_text.as_deref())
            .map_err(|_| invalid())?;
    let plan_digest = contracts::digest(
        string_item(&row, "plan_digest")?,
        "execution_plan.plan_digest",
    )
    .map_err(|_| invalid())?;
    if !run_id.is_text(&run_text) {
        expected.set("run_id", run_id.clone());
        expected.remove("plan_digest");
        let digest = match ids::canonical_bytes(&expected) {
            Ok(bytes) => ids::sha256_bytes(&bytes),
            Err(error)
                if matches!(
                    error.kind,
                    "ValueError" | "UnicodeEncodeError" | "UnicodeDecodeError"
                ) =>
            {
                return Err(invalid())
            }
            Err(error) => return Err(error.into()),
        };
        expected.set("plan_digest", digest.into());
    }
    let mut unsigned = plan.clone();
    unsigned.remove("plan_digest");
    if !plan.py_eq(&expected)
        || plan_digest != ids::sha256_bytes(&ids::canonical_bytes(&unsigned)?)
        || !get(&plan, "plan_digest")?.py_eq(string_item(&row, "plan_digest")?)
    {
        return Err(invalid());
    }
    let run_status = string_item(run, "status")?;
    if status.is_text("pending") {
        if !in_strings(run_status, &["initialized", "blocked", "aborted"])? {
            return Err(invalid());
        }
        for key in ["approved_at", "approved_by", "approval_digest"] {
            if !string_item(&row, key)?.is_null() {
                return Err(invalid());
            }
        }
    } else {
        if status.is_text("rejected") && !run_status.is_text("blocked") {
            return Err(invalid());
        }
        if status.is_text("approved") && run_status.is_text("initialized") {
            return Err(invalid());
        }
        for key in [
            "authority_ref",
            "approved_at",
            "approved_by",
            "approval_digest",
        ] {
            if !string_item(&row, key)?.truthy() {
                return Err(EngineError::state("EXECUTION_PLAN_APPROVAL_INVALID"));
            }
        }
        let decision = Value::from(if status.is_text("approved") {
            "APPROVE"
        } else {
            "REJECT"
        });
        let expected = execution::plan_approval_digest_values([
            run_id,
            string_item(&row, "plan_digest")?,
            &decision,
            string_item(&row, "authority_ref")?,
            string_item(&row, "approved_by")?,
            string_item(run, "host_identity")?,
            string_item(&row, "approved_at")?,
        ])?;
        if !string_item(&row, "approval_digest")?.is_text(&expected) {
            return Err(EngineError::state("EXECUTION_PLAN_APPROVAL_INVALID"));
        }
    }
    Ok(plan)
}
