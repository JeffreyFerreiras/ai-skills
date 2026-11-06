use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
};
use crate::{
    compat::{self, Value},
    domain::errors,
};

pub fn operation_responses(
    ledger: &dyn GraphLedgerReadPort,
    run_id: &Value,
) -> EngineResult<Vec<Value>> {
    let mut responses = Vec::new();
    ledger.visit(
        GraphRead::OperationResponses,
        std::slice::from_ref(run_id),
        &mut |row| {
            let response = errors::string_item(&row, "response_json")?;
            match compat::loads_value(response) {
                Ok(response) => responses.push(response),
                Err(error) if matches!(error.kind, "TypeError" | "JSONDecodeError") => {
                    return Err(EngineError::state("TOPOLOGY_HISTORY_INVALID"))
                }
                Err(error) => return Err(error.into()),
            }
            Ok(())
        },
    )?;
    Ok(responses)
}
