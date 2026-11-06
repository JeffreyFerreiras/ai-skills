use super::{
    error::{EngineError, EngineResult},
    graph_ledger::GraphRead,
    graph_writes::GraphWrite,
    persistent_store::PersistentSession,
};
use crate::{
    compat::{self, SerializationProfile, Value},
    domain::{
        contract_values::{get, in_strings},
        errors::{iterable_values, string_item},
        python_operations,
    },
};

pub fn branch_settled(branch: &Value) -> EngineResult<bool> {
    let status = string_item(branch, "status")?;
    Ok(in_strings(status, &["succeeded", "skipped"])?
        || (in_strings(status, &["failed", "timed_out"])?
            && python_operations::greater_equal(
                string_item(branch, "retry_count")?,
                string_item(branch, "max_retries")?,
            )?))
}

pub fn review_assignment(plan: &Value, assignment_id: &Value) -> EngineResult<Value> {
    let empty = Value::List(vec![]);
    get(plan, "conditional_review_assignments")?;
    for assignment in iterable_values(plan.get("conditional_review_assignments").unwrap_or(&empty))?
    {
        if string_item(&assignment, "assignment_id")?.py_eq(assignment_id) {
            return Ok(assignment);
        }
    }
    Err(EngineError::state("DELEGATION_ASSIGNMENT_UNDECLARED"))
}

fn promote_ready(session: &dyn PersistentSession, branch: &Value) -> EngineResult<Value> {
    let mut envelope = compat::loads_value(string_item(branch, "envelope_json")?)?;
    if envelope.as_object().is_none() {
        let message = if matches!(envelope, Value::List(_)) {
            "list indices must be integers or slices, not str".to_string()
        } else {
            format!(
                "'{}' object does not support item assignment",
                envelope.python_type()
            )
        };
        return Err(EngineError::python("TypeError", message));
    }
    envelope.set("status", "ready".into());
    let serialized = compat::dumps(&envelope, SerializationProfile::SORTED_STORED)?;
    let text = String::from_utf8(serialized).expect("ensure_ascii JSON is UTF-8");
    let branch_id = string_item(branch, "branch_id")?.clone();
    session.write(GraphWrite::ReadyNode, &[text.into(), branch_id.clone()])?;
    Ok(branch_id)
}

pub fn promote_review_successors(
    session: &dyn PersistentSession,
    slot_id: &Value,
) -> EngineResult<Vec<Value>> {
    let members = session.all(GraphRead::ReviewChildBranches, &[slot_id.clone()])?;
    let mut promoted = Vec::new();
    for member in members {
        let branch = session.first(
            GraphRead::Node,
            &[string_item(&member, "child_branch_id")?.clone()],
        )?;
        let Some(branch) = branch else {
            continue;
        };
        if !string_item(&branch, "status")?.is_text("pending") {
            continue;
        }
        let predecessors = session.all(
            GraphRead::ReviewPredecessors,
            &[slot_id.clone(), string_item(&branch, "branch_id")?.clone()],
        )?;
        let mut settled = true;
        for predecessor in &predecessors {
            if !branch_settled(predecessor)? {
                settled = false;
                break;
            }
        }
        if settled {
            promoted.push(promote_ready(session, &branch)?);
        }
    }
    Ok(promoted)
}

pub fn promote_fanout_successors(
    session: &dyn PersistentSession,
    run_id: &Value,
) -> EngineResult<Vec<Value>> {
    let mut promoted = Vec::new();
    session.visit(
        GraphRead::AssessedFanouts,
        &[run_id.clone()],
        &mut |fanout| {
            let members = compat::loads_value(string_item(&fanout, "member_branch_ids_json")?)?;
            for branch_id in iterable_values(&members)? {
                let Some(branch) = session.first(GraphRead::Node, &[branch_id.clone()])? else {
                    continue;
                };
                if !string_item(&branch, "status")?.is_text("pending") {
                    continue;
                }
                let predecessors = session.all(
                    GraphRead::FanoutPredecessors,
                    &[string_item(&fanout, "fanout_id")?.clone(), branch_id],
                )?;
                let mut settled = true;
                for predecessor in &predecessors {
                    if !branch_settled(predecessor)? {
                        settled = false;
                        break;
                    }
                }
                if settled {
                    promoted.push(promote_ready(session, &branch)?);
                }
            }
            Ok(())
        },
    )?;
    python_operations::sort_comparable_values(&mut promoted)?;
    Ok(promoted)
}
