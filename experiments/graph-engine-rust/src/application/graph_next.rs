use super::{
    error::EngineResult,
    graph_dispatch::ready_envelopes,
    graph_ledger::GraphRead,
    graph_response::json_result,
    persistent_store::PersistentSession,
    semantic_state::{verify_semantic_state, SemanticContext},
};
use crate::{compat::Value, domain::errors};

fn result(run: &Value, ok: bool, code: &str) -> EngineResult<Value> {
    Ok(json_result(
        ok,
        code,
        errors::string_item(run, "run_id")?.clone(),
        errors::string_item(run, "state_revision")?.clone(),
    ))
}

pub fn next_ready(
    session: &dyn PersistentSession,
    run: &Value,
    all: bool,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    // execute also validates readonly commands; the source validates next again here.
    verify_semantic_state(session, run, context)?;
    let status = errors::string_item(run, "status")?;
    if status.is_text("blocked") || status.is_text("complete") || status.is_text("aborted") {
        let mut response = result(
            run,
            false,
            if status.is_text("blocked") {
                "GRAPH_BLOCKED"
            } else {
                "NOT_READY"
            },
        )?;
        response.set("branches", Value::List(vec![]));
        return Ok(response);
    }
    let run_id = errors::string_item(run, "run_id")?;
    let plan = session.first(
        GraphRead::ExecutionPlanSummary,
        std::slice::from_ref(run_id),
    )?;
    if plan
        .as_ref()
        .is_none_or(|plan| !plan["status"].is_text("approved"))
    {
        let mut response = result(run, false, "EXECUTION_PLAN_APPROVAL_REQUIRED")?;
        let projection = match plan {
            None => Value::Null,
            Some(plan) => Value::object([
                ("size", errors::string_item(&plan, "size")?.clone()),
                (
                    "plan_digest",
                    errors::string_item(&plan, "plan_digest")?.clone(),
                ),
                ("status", errors::string_item(&plan, "status")?.clone()),
            ]),
        };
        response.set("execution_plan", projection);
        response.set("branches", Value::List(vec![]));
        return Ok(response);
    }
    let awaiting = session.first(GraphRead::AwaitingFanout, std::slice::from_ref(run_id))?;
    if let Some(awaiting) = awaiting {
        let mut response = result(run, false, "FANOUT_ASSESSMENT_REQUIRED")?;
        response.set(
            "fanout_id",
            errors::string_item(&awaiting, "fanout_id")?.clone(),
        );
        response.set("branches", Value::List(vec![]));
        return Ok(response);
    }
    let awaiting = session.first(GraphRead::AwaitingReview, std::slice::from_ref(run_id))?;
    if let Some(awaiting) = awaiting {
        let mut response = result(run, false, "FANOUT_ASSESSMENT_REQUIRED")?;
        response.set(
            "request_slot_id",
            errors::string_item(&awaiting, "request_slot_id")?.clone(),
        );
        response.set("branches", Value::List(vec![]));
        return Ok(response);
    }
    let mut ready = ready_envelopes(session, run)?;
    if !all {
        ready.truncate(1);
    }
    let mut response = result(
        run,
        !ready.is_empty(),
        if ready.is_empty() {
            "NOT_READY"
        } else {
            "READY"
        },
    )?;
    response.set("branches", Value::List(ready));
    Ok(response)
}
