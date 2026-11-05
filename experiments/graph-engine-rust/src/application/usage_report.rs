use super::{
    error::EngineResult,
    usage_bindings::{load_bindings, optional_corruption},
    usage_store::UsageSession,
};
use crate::{compat::Value, domain::usage_report};

pub fn report(session: &dyn UsageSession, run: &Value) -> EngineResult<Value> {
    let run_id = crate::domain::errors::string_item(run, "run_id")?;
    let bindings = match load_bindings(session, run_id) {
        Ok(bindings) => bindings,
        Err(error) if optional_corruption(&error) => {
            return Ok(Value::object([
                ("usage_schema_version", 1_i64.into()),
                ("coverage", "unavailable".into()),
                ("observed_totals", Value::Null),
                ("complete_totals", Value::Null),
                ("diagnostics", Value::list(["invalid_usage_state".into()])),
            ]))
        }
        Err(error) => return Err(error),
    };
    let attempts = session
        .usage_attempts(run_id)?
        .iter()
        .map(|attempt| attempt.value())
        .collect::<Vec<_>>();
    Ok(usage_report::report(&bindings, &attempts, run)?)
}
