use super::{
    claim_runtime::{lease_expired, ClaimRuntimePort, ClaimServices},
    error::{EngineError, EngineResult},
    evidence_status::{evidence_status, EvidenceStatusPort},
    evidence_verification::EvidenceVerification,
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    graph_nodes::task_input,
    graph_payload::branch_result_input,
    graph_recovery::repair_packet,
    graph_response::json_result,
    join_validation::validate_join,
    persistent_store::PersistentSession,
    semantic_state::SemanticContext,
    timing, usage_report,
};
use crate::{
    compat::{self, PyString, Value},
    domain::{errors, python_operations, python_sort},
};

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}

fn optional(value: &Value, key: &str) -> EngineResult<Value> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key).unwrap_or(&Value::Null).clone())
}

fn fields(value: &Value, keys: &[&str]) -> EngineResult<Value> {
    keys.iter()
        .map(|key| Ok((PyString::from(*key), item(value, key)?.clone())))
        .collect::<EngineResult<Vec<_>>>()
        .map(Value::Object)
}

fn without(value: &Value, omitted: &str) -> EngineResult<Value> {
    let items = value
        .as_object()
        .ok_or_else(|| errors::attribute_error(value, "items"))?;
    Ok(Value::Object(
        items
            .iter()
            .filter(|(key, _)| !key.equals(omitted))
            .cloned()
            .collect(),
    ))
}

fn merge(target: &mut Value, source: &Value) -> EngineResult<()> {
    let source = source.as_object().ok_or_else(|| {
        EngineError::python(
            "TypeError",
            format!("'{}' object is not a mapping", source.python_type()),
        )
    })?;
    let Value::Object(target) = target else {
        return Err(errors::attribute_error(target, "update").into());
    };
    for (key, value) in source {
        if let Some((_, old)) = target.iter_mut().find(|(old, _)| old == key) {
            *old = value.clone();
        } else {
            target.push((key.clone(), value.clone()));
        }
    }
    Ok(())
}

fn keyed_item<'a>(value: &'a Value, key: &Value) -> EngineResult<&'a Value> {
    errors::require_hashable(key)?;
    let items = value.as_object().ok_or_else(|| {
        EngineError::python(
            "TypeError",
            format!("'{}' object is not subscriptable", value.python_type()),
        )
    })?;
    items
        .iter()
        .find(|(name, _)| Value::String(name.clone()).py_eq(key))
        .map(|(_, value)| value)
        .ok_or_else(|| match compat::repr::value_repr(key) {
            Ok(message) => EngineError::python("KeyError", message),
            Err(error) => error.into(),
        })
}

fn sorted(values: Vec<Value>) -> EngineResult<Value> {
    let mut order = python_sort::identity_indices(values.len())?;
    python_sort::sort_indices(&mut order, |left, right| {
        python_operations::less(&values[left], &values[right])
    })?;
    Ok(Value::List(
        order
            .into_iter()
            .map(|index| values[index].clone())
            .collect(),
    ))
}

pub fn blocked_recovery_guidance(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
) -> EngineResult<Value> {
    if !item(run, "status")?.is_text("blocked") {
        return Ok(Value::Null);
    }
    let mut origins = Vec::new();
    ledger.visit(
        GraphRead::RootBranches,
        std::slice::from_ref(item(run, "run_id")?),
        &mut |branch| {
            let result_json = item(&branch, "result_json")?;
            let empty_result = "{}".into();
            let result = compat::loads_value(if result_json.truthy() {
                result_json
            } else {
                &empty_result
            })?;
            if !optional(&result, "decision")?.is_text("BLOCK")
                && !optional(&result, "outcome")?.is_text("BLOCK")
            {
                return Ok(());
            }
            let mut origin = fields(&branch, &["branch_id"])?;
            origin.set("gate", item(&branch, "node_key")?.clone());
            origin.set("generation", item(&branch, "generation")?.clone());
            origin.set("result_ref", branch_result_input(ledger, &branch, &result)?);
            origin.set(
                "findings",
                result
                    .get("findings")
                    .cloned()
                    .unwrap_or_else(|| Value::List(vec![])),
            );
            origins.push(origin);
            Ok(())
        },
    )?;
    let schema = item(run, "state_schema_version")?;
    errors::require_hashable(schema)?;
    let equivalent = (schema.py_eq(&6_i64.into()) || schema.py_eq(&7_i64.into()))
        && item(run, "blocked_reason")?.is_text("MISSING_REQUIRED_REVIEW_SKILL");
    let requirements: &[&str] = if equivalent {
        &["Inspect recover authorization requirements; recovery is limited to the equivalent review skill.", "Preserve the original capabilities and mandatory architecture review."]
    } else {
        &["Preserve this blocked run and its evidence.", "Resolve the reported gate findings before proposing a successor task.", "Keep successor capabilities within the approved repository policy; scope text grants no access.", "Obtain approval for the successor execution plan and rerun its required independent gates."]
    };
    Ok(Value::object([
        ("blocked_reason", item(run, "blocked_reason")?.clone()),
        (
            "next_step",
            if equivalent {
                "inspect_equivalent_review_authorization"
            } else {
                "new_plan_required"
            }
            .into(),
        ),
        ("origins", Value::List(origins)),
        (
            "requirements",
            Value::List(requirements.iter().map(|text| (*text).into()).collect()),
        ),
    ]))
}

pub fn next_action(
    port: &dyn EvidenceStatusPort,
    run: &Value,
    lifecycle: Option<&Value>,
    clock: &dyn ClaimRuntimePort,
    windows: bool,
) -> EngineResult<Value> {
    let status = item(run, "status")?;
    errors::require_hashable(status)?;
    if ["blocked", "complete", "aborted"]
        .iter()
        .any(|text| status.is_text(text))
    {
        let mut action = Value::object([("kind", status.clone())]);
        if status.is_text("blocked") {
            action.set("recovery", blocked_recovery_guidance(port, run)?);
        }
        return Ok(action);
    }
    let parameters = std::slice::from_ref(item(run, "run_id")?);
    let plan = port.first(GraphRead::ExecutionPlanSummary, parameters)?;
    if plan
        .as_ref()
        .is_none_or(|plan| !plan["status"].is_text("approved"))
    {
        return Ok(Value::object([
            ("kind", "await_execution_plan_approval".into()),
            (
                "size",
                plan.as_ref()
                    .map(|plan| item(plan, "size").cloned())
                    .transpose()?
                    .unwrap_or(Value::Null),
            ),
            (
                "plan_digest",
                plan.as_ref()
                    .map(|plan| item(plan, "plan_digest").cloned())
                    .transpose()?
                    .unwrap_or(Value::Null),
            ),
        ]));
    }
    if let Some(row) = port.first(GraphRead::AwaitingFanoutScope, parameters)? {
        let mut action = Value::object([("kind", "record_fanout_assessment".into())]);
        merge(
            &mut action,
            &fields(&row, &["fanout_id", "stage", "generation"])?,
        )?;
        return Ok(action);
    }
    if let Some(row) = port.first(GraphRead::AwaitingReviewScope, parameters)? {
        let mut action = Value::object([("kind", "record_review_fanout_assessment".into())]);
        merge(
            &mut action,
            &fields(&row, &["request_slot_id", "parent_branch_id"])?,
        )?;
        action.set("round", item(&row, "round_number")?.clone());
        return Ok(action);
    }
    if let Some(row) = port.first(GraphRead::FirstReadyBranch, parameters)? {
        return Ok(Value::object([
            ("kind", "claim".into()),
            ("branch_id", item(&row, "branch_id")?.clone()),
        ]));
    }
    let fresh;
    let lifecycle = match lifecycle {
        Some(value) => value,
        None => {
            fresh = evidence_status(port, run)?;
            &fresh
        }
    };
    if lifecycle.truthy() {
        let mut unknown = Vec::new();
        for reservation in errors::iterable_values(item(lifecycle, "reservations")?)? {
            if item(&reservation, "state")?.is_text("open") {
                unknown.push(item(&reservation, "reservation_id")?.clone());
            }
        }
        if !unknown.is_empty() {
            return Ok(Value::object([
                ("kind", "check_execution_unknown".into()),
                ("reservation_ids", sorted(unknown)?),
            ]));
        }
        for check in errors::iterable_values(item(lifecycle, "checks")?)? {
            if !item(&check, "eligibility")?.is_text("eligible") {
                return Ok(Value::object([
                    ("kind", "replace_check".into()),
                    ("check_id", item(&check, "check_id")?.clone()),
                    ("replace_ref", item(&check, "artifact_ref")?.clone()),
                    ("eligibility", item(&check, "eligibility")?.clone()),
                    (
                        "executor_options",
                        item(&check, "executor_options")?.clone(),
                    ),
                ]));
            }
        }
    }
    let mut ready_joins = Vec::new();
    port.visit(GraphRead::OpenJoins, parameters, &mut |join| {
        if validate_join(port, run, &join)?.join_status == "READY" {
            ready_joins.push(item(&join, "join_id")?.clone());
        }
        Ok(())
    })?;
    if let Some(join) = ready_joins.first() {
        return Ok(Value::object([
            ("kind", "advance_join".into()),
            ("join_id", join.clone()),
        ]));
    }
    if let Some(row) = port.first(GraphRead::RetryableReviewChild, parameters)? {
        return Ok(Value::object([
            ("kind", "retry_review_child".into()),
            ("branch_id", item(&row, "branch_id")?.clone()),
        ]));
    }
    if let Some(row) = port.first(GraphRead::FirstFailedRoot, parameters)? {
        let packet = repair_packet(port, run, &row, None, windows)?;
        return Ok(Value::object([
            ("kind", item(&packet, "next_permitted_action")?.clone()),
            ("branch_id", item(&row, "branch_id")?.clone()),
            ("repair_packet", packet),
        ]));
    }
    if item(run, "state_schema_version")?.py_eq(&7_i64.into()) {
        let mut unknown = Vec::new();
        for reservation in port.reservation_states(run)? {
            if item(&reservation, "state")?.is_text("open") {
                unknown.push(item(&reservation, "reservation_id")?.clone());
            }
        }
        if !unknown.is_empty() {
            return Ok(Value::object([
                ("kind", "check_execution_unknown".into()),
                ("reservation_ids", sorted(unknown)?),
            ]));
        }
    }
    let running = port.all(GraphRead::RunningBranches, parameters)?;
    let mut expired = Vec::new();
    for row in &running {
        let envelope = compat::loads_value(item(row, "envelope_json")?)?;
        if lease_expired(&optional(&envelope, "lease_expires_at")?, clock)? {
            expired.push((
                item(row, "branch_id")?.clone(),
                compat::loads_value(item(row, "envelope_json")?)?,
            ));
        }
    }
    if let Some((branch, envelope)) = expired.first() {
        return Ok(Value::object([
            ("kind", "timeout_expired".into()),
            ("branch_id", branch.clone()),
            ("attempt_id", optional(envelope, "attempt_id")?),
        ]));
    }
    let branch_ids = running
        .iter()
        .map(|row| item(row, "branch_id").cloned())
        .collect::<EngineResult<Vec<_>>>()?;
    let mut leases = Vec::new();
    for row in &running {
        leases.push(Value::object([
            ("branch_id", item(row, "branch_id")?.clone()),
            (
                "lease_expires_at",
                optional(
                    &compat::loads_value(item(row, "envelope_json")?)?,
                    "lease_expires_at",
                )?,
            ),
        ]));
    }
    Ok(Value::object([
        ("kind", "await_result".into()),
        ("branch_ids", Value::List(branch_ids)),
        ("leases", Value::List(leases)),
    ]))
}

pub(crate) fn continuation(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    task: &Value,
    status: &Value,
) -> EngineResult<Value> {
    let mut remaining = Value::Object(Vec::new());
    for budget in errors::iterable_values(item(status, "budgets")?)? {
        let key = item(&budget, "budget_id")?
            .text()
            .expect("validated budget ID");
        remaining.set(
            &key,
            python_operations::subtract(item(&budget, "limit_value")?, item(&budget, "used")?)?,
        );
    }
    let approvals = errors::iterable_values(item(status, "approvals")?)?;
    let mut pending = Vec::new();
    for decision in errors::iterable_values(item(task, "required_human_decisions")?)? {
        if !approvals.iter().any(|approval| {
            approval["decision"].is_text("APPROVE") && approval["approval_id"].py_eq(&decision)
        }) && !pending.iter().any(|prior: &Value| prior.py_eq(&decision))
        {
            pending.push(decision);
        }
    }
    Ok(Value::object([
        ("task", task_input(ledger, run)?),
        ("remaining_budgets", remaining),
        ("pending_human_decisions", sorted(pending)?),
    ]))
}

pub(crate) fn resume_next_action(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    status: &Value,
    clock: &dyn ClaimRuntimePort,
    windows: bool,
) -> EngineResult<Value> {
    let action = item(status, "next_action")?;
    if ["blocked", "complete", "aborted"]
        .iter()
        .any(|kind| status["status"].is_text(kind))
    {
        return Ok(action.clone());
    }
    let lifecycle = &status["evidence_lifecycle"];
    let reservations = lifecycle["reservations"].as_list().unwrap_or(&[]);
    let unknown: Vec<_> = reservations
        .iter()
        .filter(|item| item["state"].is_text("open"))
        .map(|item| item["reservation_id"].clone())
        .collect();
    if !unknown.is_empty() {
        return Ok(Value::object([
            ("kind", "check_execution_unknown".into()),
            ("reservation_ids", sorted(unknown)?),
        ]));
    }
    let branches = errors::iterable_values(item(status, "branches")?)?;
    let running: Vec<_> = branches
        .iter()
        .filter(|branch| branch["status"].is_text("running"))
        .collect();
    for branch in &running {
        if lease_expired(item(branch, "lease_expires_at")?, clock)? {
            return Ok(Value::object([
                ("kind", "timeout_expired".into()),
                ("branch_id", branch["branch_id"].clone()),
                ("attempt_id", branch["attempt_id"].clone()),
            ]));
        }
    }
    if !running.is_empty() {
        return Ok(Value::object([
            ("kind", "await_result".into()),
            (
                "branch_ids",
                Value::list(running.iter().map(|branch| branch["branch_id"].clone())),
            ),
            (
                "leases",
                Value::list(running.iter().map(|branch| {
                    Value::object([
                        ("branch_id", branch["branch_id"].clone()),
                        ("lease_expires_at", branch["lease_expires_at"].clone()),
                    ])
                })),
            ),
        ]));
    }
    if let Some(stale) = lifecycle["checks"]
        .as_list()
        .unwrap_or(&[])
        .iter()
        .find(|check| !check["eligibility"].is_text("eligible"))
    {
        if action["kind"].is_text("claim") && !stale["executor_options"].truthy() {
            return Ok(action.clone());
        }
        if let Some(source) =
            ledger.first(GraphRead::Node, &[stale["source"]["branch_id"].clone()])?
        {
            if source["run_id"].py_eq(&run["run_id"])
                && ["failed", "timed_out"]
                    .iter()
                    .any(|kind| source["status"].is_text(kind))
            {
                let packet = repair_packet(ledger, run, &source, None, windows)?;
                return Ok(Value::object([
                    ("kind", packet["next_permitted_action"].clone()),
                    ("branch_id", source["branch_id"].clone()),
                    ("repair_packet", packet),
                ]));
            }
        }
        return Ok(Value::object([
            ("kind", "replace_check".into()),
            ("check_id", stale["check_id"].clone()),
            ("replace_ref", stale["artifact_ref"].clone()),
            ("eligibility", stale["eligibility"].clone()),
            ("executor_options", stale["executor_options"].clone()),
        ]));
    }
    Ok(action.clone())
}

pub fn status(
    session: &dyn PersistentSession,
    run: &Value,
    context: &SemanticContext<'_>,
    services: &ClaimServices<'_>,
) -> EngineResult<Value> {
    status_with_evidence(session, run, context, services, evidence_status)
}

fn status_with_evidence(
    session: &dyn PersistentSession,
    run: &Value,
    context: &SemanticContext<'_>,
    services: &ClaimServices<'_>,
    report: impl FnOnce(&dyn EvidenceStatusPort, &Value) -> EngineResult<Value>,
) -> EngineResult<Value> {
    let parameters = std::slice::from_ref(item(run, "run_id")?);
    let branches = session.all(GraphRead::NodesForStatus, parameters)?;
    let joins = session.all(GraphRead::JoinsForStatus, parameters)?;
    let budgets = session.all(GraphRead::BudgetsForStatus, parameters)?;
    let approvals = session.all(GraphRead::ApprovalsForStatus, parameters)?;
    let acceptance = session.all(GraphRead::AcceptanceForStatus, parameters)?;
    let checks = session.all(GraphRead::ChecksForStatus, parameters)?;
    let fanouts = session.all(GraphRead::FanoutsForStatus, parameters)?;
    let review_requests = session.all(GraphRead::ReviewRequestsForStatus, parameters)?;
    let timing = timing::compute_timing(session, run)?;
    let plan = session.first(GraphRead::ExecutionPlan, parameters)?;
    let execution_plan = match plan {
        None => Value::Null,
        Some(plan) => {
            let mut value = compat::loads_value(item(&plan, "plan_json")?)?;
            if value.as_object().is_none() {
                return Err(errors::attribute_error(&value, "update").into());
            }
            merge(
                &mut value,
                &fields(
                    &plan,
                    &[
                        "plan_digest",
                        "status",
                        "authority_ref",
                        "approved_at",
                        "approved_by",
                    ],
                )?,
            )?;
            value
        }
    };
    let source_files =
        services
            .source_files
            .create(context.repository, context.policy, context.files);
    let verification = EvidenceVerification {
        session,
        run,
        context: *context,
        clock: services.runtime,
        source_files: &*source_files,
        process: services.process,
    };
    let schema7 = item(run, "state_schema_version")?.py_eq(&7_i64.into());
    let lifecycle = if schema7 {
        report(&verification, run)?
    } else {
        Value::Null
    };
    let mut result = json_result(
        true,
        "STATUS",
        item(run, "run_id")?.clone(),
        item(run, "state_revision")?.clone(),
    );
    result.set("sensitive", true.into());
    result.set("status", item(run, "status")?.clone());
    result.set("route", item(run, "selected_route")?.clone());
    let tags = item(run, "selected_tags_json")?;
    let empty_tags = "[]".into();
    result.set(
        "impact_tags",
        compat::loads_value(if tags.truthy() { tags } else { &empty_tags })?,
    );
    merge(
        &mut result,
        &fields(
            run,
            &[
                "policy_digest",
                "engine_version",
                "state_schema_version",
                "local_filesystem",
                "durability",
                "durability_detail",
                "permission_verification",
            ],
        )?,
    )?;
    result.set("execution_plan", execution_plan);
    result.set("usage", usage_report::report(session, run)?);
    if schema7 {
        result.set("evidence_lifecycle", lifecycle);
    }
    result.set(
        "acknowledgments",
        Value::object([
            ("host_identity", item(run, "host_identity")?.clone()),
            (
                "degraded_permissions",
                item(run, "degraded_permissions_ack")?.truthy().into(),
            ),
            (
                "degraded_durability",
                item(run, "degraded_durability_ack")?.truthy().into(),
            ),
        ]),
    );
    let mut projected = Vec::new();
    for row in branches {
        let mut branch = fields(
            &row,
            &[
                "branch_id",
                "node_key",
                "role",
                "stage",
                "specialist_tag",
                "generation",
                "status",
                "retry_count",
                "max_retries",
            ],
        )?;
        for key in [
            "model",
            "reasoning_effort",
            "attempt_id",
            "lease_expires_at",
        ] {
            // The source parses the envelope separately for each projected field.
            branch.set(
                key,
                optional(&compat::loads_value(item(&row, "envelope_json")?)?, key)?,
            );
        }
        merge(
            &mut branch,
            keyed_item(item(&timing, "branches")?, item(&row, "branch_id")?)?,
        )?;
        projected.push(branch);
    }
    result.set("branches", Value::List(projected));
    for (key, rows) in [
        ("joins", joins),
        ("budgets", budgets),
        ("approvals", approvals),
        ("acceptance_evidence", acceptance),
        ("check_evidence", checks),
    ] {
        result.set(key, Value::List(rows));
    }
    let project_json = |rows: Vec<Value>, encoded: &str, decoded: &str| -> EngineResult<Value> {
        rows.iter()
            .map(|row| {
                let mut value = without(row, encoded)?;
                value.set(decoded, compat::loads_value(item(row, encoded)?)?);
                Ok(value)
            })
            .collect::<EngineResult<Vec<_>>>()
            .map(Value::List)
    };
    result.set(
        "fanouts",
        project_json(fanouts, "member_branch_ids_json", "member_branch_ids")?,
    );
    result.set(
        "reviewer_delegations",
        project_json(review_requests, "child_member_tuples_json", "child_members")?,
    );
    result.set("timing", without(&timing, "branches")?);
    let action = next_action(
        &verification,
        run,
        Some(&result["evidence_lifecycle"]),
        services.runtime,
        context.store.platform.windows(),
    )?;
    result.set("next_action", action);
    result.set("blocked_reason", item(run, "blocked_reason")?.clone());
    Ok(result)
}

#[cfg(test)]
#[path = "graph_status_tests.rs"]
mod tests;
