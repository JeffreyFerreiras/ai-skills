use super::{
    error::{EngineError, EngineResult, TaskValidationMutation},
    graph_ledger::GraphRead,
    graph_writes::GraphWrite,
    persistent_store::PersistentSession,
    semantic_state::{mutation_task_validator, SemanticContext},
    state::mutate_persistent,
};
use crate::{
    compat::Value,
    domain::{contracts, errors::string_item, execution},
};

pub fn record_plan_approval(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    let operation = contracts::opaque(string_item(arguments, "op_id")?, "op_id")?;
    let mut request = Value::object([("command", "record.plan-approval".into())]);
    if let Some(branch) = arguments.get("branch_id") {
        request.set("branch_id", branch.clone());
    }
    request.set(
        "plan_digest",
        contracts::digest(string_item(arguments, "plan_digest")?, "plan_digest")?.into(),
    );
    request.set("decision", string_item(arguments, "decision")?.clone());
    request.set(
        "authority_ref",
        Value::String(contracts::validate_ref(
            string_item(arguments, "authority_ref")?,
            "authority_ref",
            false,
        )?),
    );
    let actor = string_item(arguments, "actor")?;
    let actor = if actor.is_null() {
        context.provenance.actor()?
    } else {
        contracts::bounded_string(actor, "actor", 256)?.clone()
    };
    request.set("actor", Value::String(actor));
    if !request["actor"].py_eq(&Value::String(context.provenance.actor()?)) {
        return Err(EngineError::state("APPROVAL_ACTOR_MISMATCH"));
    }
    let mut semantic = *context;
    semantic.evidence_enable = false;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let current = &current.fields;
            let run = string_item(current, "run_id")?;
            let plan = session.first(GraphRead::ExecutionPlan, &[run.clone()])?;
            let Some(plan) = plan else {
                return Err(EngineError::state("EXECUTION_PLAN_APPROVAL_CONFLICT"));
            };
            if !string_item(&plan, "status")?.is_text("pending") {
                return Err(EngineError::state("EXECUTION_PLAN_APPROVAL_CONFLICT"));
            }
            if !request["plan_digest"].py_eq(string_item(&plan, "plan_digest")?) {
                return Err(EngineError::state("EXECUTION_PLAN_DIGEST_MISMATCH"));
            }
            let approved_at = Value::from(context.store.platform.utc_now()?);
            let host = Value::from(context.store.platform.host_identity()?);
            let digest = execution::plan_approval_digest_values([
                run,
                &request["plan_digest"],
                &request["decision"],
                &request["authority_ref"],
                &request["actor"],
                &host,
                &approved_at,
            ])?;
            let approved = request["decision"].is_text("APPROVE");
            let status = Value::from(if approved { "active" } else { "blocked" });
            let plan_status = Value::from(if approved { "approved" } else { "rejected" });
            let blocked = if approved {
                Value::Null
            } else {
                "EXECUTION_PLAN_REJECTED".into()
            };
            session.write(
                GraphWrite::PlanApproval,
                &[
                    plan_status,
                    request["authority_ref"].clone(),
                    approved_at,
                    request["actor"].clone(),
                    digest.into(),
                    run.clone(),
                ],
            )?;
            session.write(
                GraphWrite::PlanApprovalRunState,
                &[
                    status.clone(),
                    blocked,
                    status.clone(),
                    context.store.platform.utc_now()?.into(),
                    run.clone(),
                ],
            )?;
            Ok(Value::object([
                (
                    "code",
                    if approved {
                        "EXECUTION_PLAN_APPROVED"
                    } else {
                        "EXECUTION_PLAN_REJECTED"
                    }
                    .into(),
                ),
                ("status", status),
                ("plan_digest", request["plan_digest"].clone()),
            ]))
        },
        mutation_task_validator(&semantic, TaskValidationMutation::RecordControl),
        context.store.fault,
    )
}
