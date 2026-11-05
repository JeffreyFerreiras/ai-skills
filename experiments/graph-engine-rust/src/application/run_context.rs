use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    state::RunRecord,
};
use crate::compat::{self, Value};

pub struct RunContext {
    pub run: RunRecord,
    pub policy: Value,
    pub task: Value,
}

pub fn load(session: &dyn GraphLedgerReadPort, run_id: &Value) -> EngineResult<RunContext> {
    let run = session
        .first(GraphRead::Run, std::slice::from_ref(run_id))?
        .ok_or_else(|| EngineError::state("RUN_NOT_FOUND"))?;
    let policy = compat::loads_value(&run["policy_json"])?;
    let task = compat::loads_value(&run["task_json"])?;
    Ok(RunContext {
        run: RunRecord { fields: run },
        policy,
        task,
    })
}
