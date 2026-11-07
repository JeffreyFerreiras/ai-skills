use super::{
    artifacts::persist_artifact,
    claim_runtime::{lease_expired, ClaimServices},
    error::{
        BranchResultTraceSite, EngineError, EngineResult, ManifestJsonTraceSite,
        TaskValidationMutation,
    },
    graph_claim::branch_lease_seconds,
    graph_ledger::GraphRead,
    graph_payload::{artifact_reference, persist_payload, sorted_inputs},
    graph_recovery::repair_packet,
    graph_review_delegation::promote_child_successors,
    graph_run_control::verify_control_evidence,
    graph_transitions::promote_fanout_successors,
    graph_writes::GraphWrite,
    manifest_snapshot::inbox_snapshot,
    persistent_store::PersistentSession,
    run_context::RunContext,
    semantic_state::{mutation_task_validator, SemanticContext},
    state::mutate_persistent,
};
use crate::{
    compat::{self, integer, SerializationProfile, Value},
    domain::{
        contracts::{self, ContractError},
        control_manifests::validate_control_manifest,
        errors,
        evidence::canonical_ledger_artifact,
        ids, python_operations,
    },
};
use num_bigint::BigInt;

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}
fn optional<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key).unwrap_or(&compat::value::NULL))
}

pub fn claim_token_digest(token: &Value) -> EngineResult<String> {
    if !token.truthy() {
        return Err(ContractError::new("claim_token", "MISSING_FIELD").into());
    }
    let token = token
        .as_string()
        .ok_or_else(|| errors::attribute_error(token, "encode"))?;
    Ok(ids::sha256_bytes(&compat::encode_utf8(token)?))
}

pub fn check_attempt(
    branch: &Value,
    envelope: &Value,
    attempt: &Value,
    token: &Value,
    manifest: Option<&Value>,
) -> EngineResult<String> {
    contracts::opaque(attempt, "attempt_id")?;
    let digest = claim_token_digest(token)?;
    if !optional(envelope, "attempt_id")?.py_eq(attempt)
        || !optional(envelope, "claim_digest")?.is_text(&digest)
    {
        return Err(EngineError::state("ATTEMPT_FENCE_MISMATCH"));
    }
    if let Some(manifest) = manifest {
        let manifest_attempt = optional(manifest, "attempt_id").map_err(|error| {
            error.with_branch_result_trace(BranchResultTraceSite::ManifestAttempt)
        })?;
        if !manifest_attempt.py_eq(attempt) || !optional(manifest, "claim_digest")?.is_text(&digest)
        {
            return Err(EngineError::state("ATTEMPT_FENCE_MISMATCH"));
        }
    }
    if !item(branch, "status")?.is_text("running") {
        return Err(EngineError::state("INVALID_BRANCH_TRANSITION"));
    }
    Ok(digest)
}

fn envelope_json(envelope: &Value) -> EngineResult<Value> {
    Ok(Value::String(compat::dumps_text(
        envelope,
        SerializationProfile::SORTED_STORED,
    )?))
}

pub(super) fn finish_running_attempt(
    session: &dyn PersistentSession,
    run_id: &Value,
    branch_id: &Value,
    attempt_id: &Value,
    finished_at: Value,
    status: &Value,
) -> EngineResult<()> {
    let attempt = session
        .first(
            GraphRead::FenceAttempt,
            &[run_id.clone(), branch_id.clone(), attempt_id.clone()],
        )?
        .filter(|attempt| attempt["finished_at"].is_null())
        .ok_or_else(|| EngineError::state("ATTEMPT_STATE_INVALID"))?;
    session.write(
        GraphWrite::FinishAttempt,
        &[
            finished_at,
            status.clone(),
            branch_id.clone(),
            attempt["attempt_number"].clone(),
        ],
    )
}

fn branch_response(kind: &str, branch_id: &Value, status: &Value, promoted: Vec<Value>) -> Value {
    Value::object([
        (
            "code",
            format!("{}_RECORDED", kind.to_ascii_uppercase()).into(),
        ),
        ("branch_id", branch_id.clone()),
        ("branch_status", status.clone()),
        ("promoted_branch_ids", Value::List(promoted)),
    ])
}

fn retry_branch(
    session: &dyn PersistentSession,
    run: &Value,
    branch: &Value,
    envelope: &mut Value,
    operation: &str,
    judgment: Option<&Value>,
    revision: &BigInt,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    if !branch["status"].is_text("failed") && !branch["status"].is_text("timed_out") {
        return Err(EngineError::state("INVALID_BRANCH_TRANSITION"));
    }
    if python_operations::greater_equal(&branch["retry_count"], &branch["max_retries"])? {
        if branch["mandatory"].truthy() {
            session.write(
                GraphWrite::StopRun,
                &[
                    "blocked".into(),
                    "RETRY_LIMIT".into(),
                    context.store.platform.utc_now()?.into(),
                    run["run_id"].clone(),
                ],
            )?;
        }
        return Ok(Value::object([
            ("code", "GRAPH_BLOCKED".into()),
            ("status", "blocked".into()),
            ("reason", "RETRY_LIMIT".into()),
        ]));
    }
    if run["state_schema_version"].py_eq(&7_i64.into()) && !branch["depth"].truthy() {
        let mut packet = repair_packet(session, run, branch, judgment, context.files.windows())?;
        packet.set("state_revision", Value::Integer(revision.clone()));
        let artifact = persist_payload(
            session,
            run,
            context.policy,
            &format!("repair-{operation}").into(),
            &"failure".into(),
            &packet,
        )?;
        let mut inputs = errors::iterable_values(&envelope["inputs"])?;
        inputs.push(artifact.as_input());
        envelope.set("inputs", sorted_inputs(inputs)?);
    }
    let count = item(envelope, "retry_count")?.clone();
    let branch_id = &branch["branch_id"];
    reopen_branch(session, branch_id, envelope, &count)?;
    Ok(branch_response("retry", branch_id, &"ready".into(), vec![]))
}

pub(super) fn reopen_branch(
    session: &dyn PersistentSession,
    branch_id: &Value,
    envelope: &mut Value,
    retry_count: &Value,
) -> EngineResult<()> {
    envelope.set("status", "ready".into());
    envelope.set(
        "retry_count",
        python_operations::add(retry_count, &1_i64.into())?,
    );
    for key in [
        "finished_at",
        "attempt_id",
        "claim_digest",
        "lease_expires_at",
        "artifact_ref",
        "decision",
        "failure_code",
    ] {
        envelope.set(key, Value::Null);
    }
    envelope.set("evidence", Value::List(vec![]));
    session.write(
        GraphWrite::RetryNode,
        &[envelope_json(envelope)?, branch_id.clone()],
    )
}

fn settle_branch(
    session: &dyn PersistentSession,
    run: &Value,
    branch: &Value,
    envelope: &mut Value,
    kind: &str,
    request: &Value,
    manifest: &Value,
    operation: &str,
    context: &SemanticContext<'_>,
    services: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let timeout = kind == "timeout";
    if !timeout
        && ((branch["mandatory"].truthy() && !branch["depth"].truthy())
            || !["pending", "ready"]
                .iter()
                .any(|status| branch["status"].is_text(status)))
    {
        return Err(EngineError::state("INVALID_BRANCH_TRANSITION"));
    }
    let branch_id = &branch["branch_id"];
    let reason = &request["reason_code"];
    let run_id = run["run_id"].text().expect("validated run ID");
    let mut manifest = validate_control_manifest(
        manifest,
        kind,
        &run_id,
        Some(&branch_id.text().expect("validated branch ID")),
        &reason.text().expect("validated reason"),
    )?;
    let mut verified = verify_control_evidence(session, &run_id, &manifest, context)?;
    manifest.set("evidence", sorted_inputs(verified.clone())?);
    let control = canonical_ledger_artifact(
        &format!("evidence-{operation}").into(),
        &"evidence_manifest".into(),
        &manifest,
    )?;
    persist_artifact(session, &run_id, &control)?;
    let mut result = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", kind.into()),
        ("run_id", run["run_id"].clone()),
        ("branch_id", branch_id.clone()),
        ("reason_code", reason.clone()),
        ("evidence_manifest_ref", control.reference.clone()),
    ]);
    if timeout {
        result.set("attempt_id", request["attempt_id"].clone());
        result.set("claim_digest", request["claim_digest"].clone());
    }
    let artifact = canonical_ledger_artifact(
        branch_id.as_string().expect("validated branch ID"),
        &kind.into(),
        &result,
    )?;
    persist_artifact(session, &run_id, &artifact)?;
    let status = Value::from(if timeout { "timed_out" } else { "skipped" });
    let finished_at = Value::from(services.runtime.utc_now()?);
    envelope.set("status", status.clone());
    envelope.set("artifact_ref", artifact_reference(&artifact));
    verified.push(artifact_reference(&control));
    envelope.set("evidence", Value::List(verified));
    envelope.set("finished_at", finished_at.clone());
    session.write(
        GraphWrite::BranchControlResult,
        &[
            status.clone(),
            reason.clone(),
            envelope_json(envelope)?,
            artifact.content_json,
            artifact.sha256,
            finished_at.clone(),
            branch_id.clone(),
        ],
    )?;
    if timeout {
        finish_running_attempt(
            session,
            &run["run_id"],
            branch_id,
            &request["attempt_id"],
            finished_at,
            &status,
        )?;
    }
    let mut promoted = promote_fanout_successors(session, &run["run_id"])?;
    promoted.extend(promote_child_successors(
        session,
        run,
        &branch["request_slot_id"],
        context.store.platform,
    )?);
    Ok(branch_response(kind, branch_id, &status, promoted))
}

fn apply_branch_control(
    session: &dyn PersistentSession,
    run: &Value,
    arguments: &Value,
    request: &Value,
    manifest: Option<&Value>,
    operation: &str,
    revision: &BigInt,
    context: &SemanticContext<'_>,
    services: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let kind = item(arguments, "record_kind")?
        .text()
        .expect("parser validates record kind");
    let branch_id = item(arguments, "branch_id")?;
    let branch = session
        .first(GraphRead::Node, std::slice::from_ref(branch_id))?
        .ok_or_else(|| EngineError::state("BRANCH_NOT_FOUND"))?;
    if session
        .first(
            GraphRead::BranchSealedForControl,
            std::slice::from_ref(branch_id),
        )?
        .is_some()
    {
        return Err(EngineError::state("JOIN_ALREADY_SEALED"));
    }
    let mut envelope = compat::loads_value(item(&branch, "envelope_json")?)?;
    if kind == "timeout" || kind == "heartbeat" {
        check_attempt(
            &branch,
            &envelope,
            item(arguments, "attempt_id")?,
            item(arguments, "claim_token")?,
            None,
        )?;
    }
    match kind.as_str() {
        "heartbeat" => {
            if lease_expired(optional(&envelope, "lease_expires_at")?, services.runtime)? {
                return Err(EngineError::state("LEASE_EXPIRED"));
            }
            envelope.set(
                "lease_expires_at",
                services
                    .runtime
                    .utc_after(&branch_lease_seconds(context.policy)?)?
                    .into(),
            );
            session.write(
                GraphWrite::UpdateNodeEnvelope,
                &[envelope_json(&envelope)?, branch_id.clone()],
            )?;
            Ok(Value::object([
                ("code", "HEARTBEAT_RECORDED".into()),
                ("branch_id", branch_id.clone()),
                (
                    "lease_expires_at",
                    item(&envelope, "lease_expires_at")?.clone(),
                ),
            ]))
        }
        "retry" => retry_branch(
            session,
            run,
            &branch,
            &mut envelope,
            operation,
            manifest,
            revision,
            context,
        ),
        _ => settle_branch(
            session,
            run,
            &branch,
            &mut envelope,
            &kind,
            request,
            manifest.expect("terminal controls have a manifest"),
            operation,
            context,
            services,
        ),
    }
}

pub fn record_control(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    services: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let kind = item(arguments, "record_kind")?;
    let operation = contracts::opaque(item(arguments, "op_id")?, "op_id")?;
    let mut request =
        Value::object([("command", python_operations::add(&"record.".into(), kind)?)]);
    if let Some(branch) = arguments.get("branch_id") {
        request.set("branch_id", branch.clone());
    }
    let timeout = kind.is_text("timeout");
    let skip = kind.is_text("skip");
    let retry = kind.is_text("retry");
    let manifest = if timeout || skip {
        let snapshot = inbox_snapshot(
            context,
            &loaded.policy,
            run_id,
            &arguments["evidence_manifest"],
            (128 * 1024).into(),
            ManifestJsonTraceSite::TerminalEvidence,
        )?;
        request.set(
            "reason_code",
            contracts::opaque(&arguments["reason_code"], "reason_code")?.into(),
        );
        request.set("evidence_digest", snapshot.digest.into());
        snapshot.parsed
    } else if retry {
        request.set(
            "reason_code",
            contracts::opaque(&arguments["reason_code"], "reason_code")?.into(),
        );
        if arguments["repair_manifest"].truthy() {
            if !loaded.run.fields["state_schema_version"].py_eq(&7_i64.into()) {
                return Err(EngineError::state("EVIDENCE_ENABLE_REQUIRED"));
            }
            let limit = integer::coerce(&loaded.policy["limits"]["manifest_bytes"])?;
            let snapshot = inbox_snapshot(
                context,
                &loaded.policy,
                run_id,
                &arguments["repair_manifest"],
                limit,
                ManifestJsonTraceSite::Retry,
            )?;
            request.set("repair_judgment_digest", snapshot.digest.into());
            snapshot.parsed
        } else {
            None
        }
    } else {
        None
    };
    if timeout || kind.is_text("heartbeat") {
        request.set(
            "attempt_id",
            contracts::opaque(item(arguments, "attempt_id")?, "attempt_id")?.into(),
        );
        request.set(
            "claim_digest",
            claim_token_digest(item(arguments, "claim_token")?)?.into(),
        );
    } else if kind.is_text("budget-use") {
        if python_operations::less_equal(item(arguments, "amount")?, &0_i64.into())? {
            return Err(ContractError::new("amount", "POSITIVE_INTEGER_REQUIRED").into());
        }
        request.set(
            "budget_id",
            contracts::opaque(item(arguments, "budget_id")?, "budget_id")?.into(),
        );
        request.set("amount", item(arguments, "amount")?.clone());
        request.set(
            "source_branch_id",
            contracts::opaque(item(arguments, "source_branch_id")?, "source_branch_id")?.into(),
        );
    } else if !skip && !retry {
        return Err(EngineError::python(
            "NotImplementedError",
            "native record control handler remains unimplemented",
        ));
    }
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, revision| {
            let current_run = item(&current.fields, "run_id")?;
            if !kind.is_text("budget-use") {
                return apply_branch_control(
                    session,
                    &current.fields,
                    arguments,
                    &request,
                    manifest.as_ref(),
                    &operation,
                    revision,
                    context,
                    services,
                );
            }
            let budget_id = item(&request, "budget_id")?;
            let amount = item(&request, "amount")?;
            let budget = session.first(
                GraphRead::BudgetForUse,
                &[current_run.clone(), budget_id.clone()],
            )?;
            match budget {
                None => return Err(EngineError::state("BUDGET_LIMIT")),
                Some(budget) => {
                    if python_operations::greater(
                        &python_operations::add(item(&budget, "used")?, amount)?,
                        item(&budget, "limit_value")?,
                    )? {
                        return Err(EngineError::state("BUDGET_LIMIT"));
                    }
                }
            }
            let source = item(&request, "source_branch_id")?;
            if session
                .first(GraphRead::BranchForBudgetUse, std::slice::from_ref(source))?
                .is_none()
            {
                return Err(EngineError::state("BRANCH_NOT_FOUND"));
            }
            if session
                .first(
                    GraphRead::BudgetUseConflict,
                    &[current_run.clone(), budget_id.clone(), source.clone()],
                )?
                .is_some()
            {
                return Err(EngineError::state("BUDGET_CONSUMPTION_CONFLICT"));
            }
            session.write(
                GraphWrite::ConsumeBudget,
                &[
                    current_run.clone(),
                    budget_id.clone(),
                    source.clone(),
                    amount.clone(),
                ],
            )?;
            session.write(
                GraphWrite::IncrementBudget,
                &[amount.clone(), current_run.clone(), budget_id.clone()],
            )?;
            Ok(Value::object([("code", "BUDGET_USE_RECORDED".into())]))
        },
        mutation_task_validator(context, TaskValidationMutation::RecordControl),
        context.store.fault,
    )
}
