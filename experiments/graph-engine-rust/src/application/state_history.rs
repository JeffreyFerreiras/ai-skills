use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    timing::timestamp_microseconds,
};
use crate::{
    compat::{self, Value},
    domain::{
        contract_values::{get, in_strings},
        errors::string_item,
        python_operations as python,
    },
};
use num_bigint::BigInt;
use num_traits::ToPrimitive;

fn invalid(code: &str) -> EngineError {
    EngineError::state(code)
}

pub fn validate_attempts(connection: &dyn GraphLedgerReadPort, run: &Value) -> EngineResult<()> {
    let started = timestamp_microseconds(string_item(run, "started_at")?)?;
    let finished_value = string_item(run, "finished_at")?;
    let finished = if finished_value.is_null() {
        None
    } else {
        Some(timestamp_microseconds(finished_value)?)
    };
    if finished.is_some_and(|finish| finish < started) {
        return Err(invalid("TIMESTAMP_ORDER_INVALID"));
    }
    if in_strings(
        string_item(run, "status")?,
        &["blocked", "complete", "aborted"],
    )? != finished.is_some()
    {
        return Err(invalid("TIMESTAMP_STATE_INVALID"));
    }
    let run_id = string_item(run, "run_id")?;
    connection.visit(
        GraphRead::NodesForValidation,
        &[run_id.clone()],
        &mut |node| {
            let node_started_value = string_item(&node, "started_at")?;
            let node_finished_value = string_item(&node, "finished_at")?;
            let node_started = if node_started_value.truthy() {
                Some(timestamp_microseconds(node_started_value)?)
            } else {
                None
            };
            let node_finished = if node_finished_value.truthy() {
                Some(timestamp_microseconds(node_finished_value)?)
            } else {
                None
            };
            if node_started.is_some_and(|start| start < started) {
                return Err(invalid("TIMESTAMP_ORDER_INVALID"));
            }
            if node_finished
                .zip(node_started)
                .is_some_and(|(finish, start)| finish < start)
            {
                return Err(invalid("TIMESTAMP_ORDER_INVALID"));
            }
            if finished
                .zip(node_finished)
                .is_some_and(|(finish, node_finish)| node_finish > finish)
            {
                return Err(invalid("TIMESTAMP_ORDER_INVALID"));
            }
            let envelope = compat::loads_value(string_item(&node, "envelope_json")?)?;
            if !get(&envelope, "started_at")?.py_eq(node_started_value)
                || !get(&envelope, "finished_at")?.py_eq(node_finished_value)
            {
                return Err(invalid("TIMESTAMP_STATE_INVALID"));
            }
            let attempts = connection.all(
                GraphRead::AttemptsForValidation,
                &[run_id.clone(), string_item(&node, "branch_id")?.clone()],
            )?;
            let delegated = attempts
                .iter()
                .filter(|attempt| attempt["outcome"].is_text("delegated"))
                .count();
            let status = string_item(&node, "status")?;
            let expected_count = if in_strings(status, &["pending", "skipped"])? {
                0i64.into()
            } else {
                let count = python::add(string_item(&node, "retry_count")?, &delegated.into())?;
                if in_strings(status, &["ready", "waiting_for_review_children"])? {
                    count
                } else {
                    python::add(&count, &1i64.into())?
                }
            };
            if !Value::from(attempts.len()).py_eq(&expected_count)
                || attempts
                    .iter()
                    .enumerate()
                    .any(|(index, row)| !row["attempt_number"].py_eq(&Value::from(index + 1)))
            {
                return Err(invalid("ATTEMPT_HISTORY_INVALID"));
            }
            validate_attempt_sequence(&attempts, started, finished)?;
            let open = attempts
                .iter()
                .filter(|attempt| attempt["finished_at"].is_null())
                .collect::<Vec<_>>();
            if status.is_text("running") {
                if open.len() != 1 || !open[0].py_eq(attempts.last().unwrap()) {
                    return Err(invalid("ATTEMPT_STATE_INVALID"));
                }
            } else if !open.is_empty() {
                return Err(invalid("ATTEMPT_STATE_INVALID"));
            }
            if let Some(first) = attempts.first() {
                let envelope = compat::loads_value(string_item(&node, "envelope_json")?)?;
                if !node_started_value.py_eq(string_item(first, "started_at")?)
                    || !string_item(&envelope, "started_at")?.py_eq(node_started_value)
                {
                    return Err(invalid("ATTEMPT_TIMESTAMP_INVALID"));
                }
                if !in_strings(status, &["ready", "waiting_for_review_children"])? {
                    let last = attempts.last().unwrap();
                    if !get(&envelope, "attempt_id")?.py_eq(string_item(last, "attempt_id")?)
                        || !get(&envelope, "claim_digest")?
                            .py_eq(string_item(last, "claim_digest")?)
                    {
                        return Err(invalid("ATTEMPT_STATE_INVALID"));
                    }
                }
            } else if !node_started_value.is_null() {
                return Err(invalid("ATTEMPT_TIMESTAMP_INVALID"));
            }
            if in_strings(status, &["succeeded", "failed", "timed_out"])? {
                let Some(last) = attempts.last() else {
                    return Err(invalid("ATTEMPT_TIMESTAMP_INVALID"));
                };
                if !node_finished_value.py_eq(string_item(last, "finished_at")?)
                    || !string_item(last, "outcome")?.py_eq(status)
                {
                    return Err(invalid("ATTEMPT_TIMESTAMP_INVALID"));
                }
            } else if status.is_text("ready")
                && !attempts.is_empty()
                && !in_strings(
                    string_item(attempts.last().unwrap(), "outcome")?,
                    &["failed", "timed_out", "delegated"],
                )?
            {
                return Err(invalid("ATTEMPT_HISTORY_INVALID"));
            } else if !in_strings(status, &["skipped", "waiting_for_review_children"])?
                && !node_finished_value.is_null()
            {
                return Err(invalid("ATTEMPT_TIMESTAMP_INVALID"));
            }
            Ok(())
        },
    )
}

fn validate_attempt_sequence(
    attempts: &[Value],
    run_start: i64,
    run_finish: Option<i64>,
) -> EngineResult<()> {
    let mut previous_start = None;
    let mut previous_finish = None;
    for (index, attempt) in attempts.iter().enumerate() {
        let start = timestamp_microseconds(string_item(attempt, "started_at")?)?;
        let finished = string_item(attempt, "finished_at")?;
        let finish = if finished.truthy() {
            Some(timestamp_microseconds(finished)?)
        } else {
            None
        };
        if start < run_start
            || previous_start.is_some_and(|previous| start < previous)
            || previous_finish.is_some_and(|previous| start < previous)
            || python::length(string_item(attempt, "claim_digest")?)? != 64
            || !string_item(attempt, "attempt_id")?.truthy()
        {
            return Err(invalid("ATTEMPT_TIMESTAMP_INVALID"));
        }
        if finish.is_some_and(|finish| finish < start) {
            return Err(invalid("ATTEMPT_TIMESTAMP_INVALID"));
        }
        let outcome = string_item(attempt, "outcome")?;
        if finish.is_some()
            && !in_strings(outcome, &["succeeded", "failed", "timed_out", "delegated"])?
        {
            return Err(invalid("ATTEMPT_STATE_INVALID"));
        }
        if finish.is_some()
            && index < attempts.len() - 1
            && !in_strings(outcome, &["failed", "timed_out", "delegated"])?
        {
            return Err(invalid("ATTEMPT_HISTORY_INVALID"));
        }
        if run_finish
            .zip(finish)
            .is_some_and(|(run_finish, finish)| finish > run_finish)
        {
            return Err(invalid("ATTEMPT_TIMESTAMP_INVALID"));
        }
        previous_start = Some(start);
        previous_finish = finish;
    }
    Ok(())
}

pub fn validate_budget_state(
    connection: &dyn GraphLedgerReadPort,
    run: &Value,
    task: &Value,
    policy: &Value,
) -> EngineResult<()> {
    let limits = string_item(policy, "limits")?;
    let budget = string_item(task, "inspection_budget")?;
    let expected = Value::object([
        (
            "design_revisions",
            string_item(limits, "design_revisions")?.clone(),
        ),
        (
            "delivery_repairs",
            string_item(limits, "delivery_repairs")?.clone(),
        ),
        ("file_reads", string_item(budget, "file_reads")?.clone()),
        (
            "discovery_commands",
            string_item(budget, "discovery_commands")?.clone(),
        ),
    ]);
    let run_id = string_item(run, "run_id")?;
    let rows = connection.all(GraphRead::BudgetsForValidation, &[run_id.clone()])?;
    if rows.len() != 4 {
        return Err(invalid("BUDGET_STATE_INVALID"));
    }
    let mut ids = Vec::new();
    for row in &rows {
        let id = string_item(row, "budget_id")?;
        crate::domain::errors::require_hashable(id)?;
        if !ids.iter().any(|previous: &Value| previous.container_eq(id)) {
            ids.push(id.clone());
        }
    }
    if ids.len() != 4
        || ids.iter().any(|id| {
            ![
                "design_revisions",
                "delivery_repairs",
                "file_reads",
                "discovery_commands",
            ]
            .iter()
            .any(|key| id.is_text(key))
        })
    {
        return Err(invalid("BUDGET_STATE_INVALID"));
    }
    for row in rows {
        let id = string_item(&row, "budget_id")?;
        let limit = string_item(&row, "limit_value")?;
        let used = string_item(&row, "used")?;
        let expected_limit = &expected[&id.text().unwrap()];
        if !limit.py_eq(expected_limit)
            || !(python::less_equal(&0i64.into(), used)? && python::less_equal(used, limit)?)
        {
            return Err(invalid("BUDGET_STATE_INVALID"));
        }
        let consumed = connection
            .first(GraphRead::BudgetConsumed, &[run_id.clone(), id.clone()])?
            .ok_or_else(|| {
                EngineError::python("TypeError", "'NoneType' object is not subscriptable")
            })?;
        let consumed = string_item(&consumed, "COALESCE(SUM(amount),0)")?;
        if in_strings(id, &["file_reads", "discovery_commands"])? && !consumed.py_eq(used) {
            return Err(invalid("BUDGET_STATE_INVALID"));
        }
        if in_strings(id, &["design_revisions", "delivery_repairs"])?
            && python::greater(consumed, used)?
        {
            return Err(invalid("BUDGET_STATE_INVALID"));
        }
    }
    if connection
        .first(GraphRead::InvalidBudgetConsumption, &[run_id.clone()])?
        .is_some()
    {
        return Err(invalid("BUDGET_STATE_INVALID"));
    }
    Ok(())
}

pub fn validate_operation_ledger(
    connection: &dyn GraphLedgerReadPort,
    run: &Value,
) -> EngineResult<()> {
    let run_id = string_item(run, "run_id")?;
    let rows = connection.all(GraphRead::OperationsForValidation, &[run_id.clone()])?;
    let mut revisions = Vec::new();
    for row in rows {
        if python::length(string_item(&row, "request_digest")?)? != 64 {
            return Err(invalid("OPERATION_LEDGER_INVALID"));
        }
        let response = match compat::loads_value(string_item(&row, "response_json")?) {
            Ok(value) => value,
            Err(error) if error.kind == "JSONDecodeError" => {
                return Err(invalid("OPERATION_LEDGER_INVALID"))
            }
            Err(error) => return Err(error.into()),
        };
        let revision = string_item(&row, "resulting_revision")?;
        if !get(&response, "state_revision")?.py_eq(revision)
            || !get(&response, "run_id")?.py_eq(run_id)
            || !get(&response, "schema_version")?.py_eq(&1i64.into())
            || !get(&response, "ok")?.is_true()
            || get(&response, "code")?.as_string().is_none()
        {
            return Err(invalid("OPERATION_LEDGER_INVALID"));
        }
        revisions.push(revision.clone());
    }
    let end = python::add(string_item(run, "state_revision")?, &1i64.into())?;
    let end = end.strict_integer().cloned().ok_or_else(|| {
        EngineError::python(
            "TypeError",
            format!(
                "'{}' object cannot be interpreted as an integer",
                end.python_type()
            ),
        )
    })?;
    let length = (end - BigInt::from(1))
        .max(BigInt::from(0))
        .to_isize()
        .ok_or_else(|| {
            EngineError::python(
                "OverflowError",
                "Python int too large to convert to C ssize_t",
            )
        })? as usize;
    let mut expected = Vec::new();
    expected
        .try_reserve_exact(length)
        .map_err(|_| EngineError::python("MemoryError", ""))?;
    expected.extend((1..=length).map(Value::from));
    if revisions.len() != expected.len()
        || !revisions
            .iter()
            .zip(&expected)
            .all(|(left, right)| left.py_eq(right))
    {
        return Err(invalid("OPERATION_LEDGER_INVALID"));
    }
    Ok(())
}
