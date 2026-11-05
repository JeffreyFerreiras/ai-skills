use super::{
    artifacts::persist_artifact,
    claim_runtime::{new_claim_identity, ClaimServices},
    error::{EngineError, EngineResult, TaskValidationMutation},
    evidence_lifecycle::verify_review_source,
    evidence_verification::EvidenceVerification,
    graph_dispatch::review_dispatch_projection,
    graph_initialization::{constraint_markers, maximum, task_roots},
    graph_ledger::GraphRead,
    graph_payload::sorted_inputs,
    graph_recovery::repair_packet,
    graph_writes::GraphWrite,
    persistent_store::PersistentSession,
    policy,
    semantic_state::{mutation_task_validator, SemanticContext},
    snapshots::{self, SnapshotFailure},
    state::mutate_persistent,
};
use crate::{
    compat::{self, integer, SerializationProfile, Value},
    domain::{contracts, errors, evidence},
};
use num_bigint::BigInt;

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}

fn optional<'a>(value: &'a Value, key: &str, default: &'a Value) -> EngineResult<&'a Value> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key).unwrap_or(default))
}

pub(crate) fn branch_lease_seconds(policy: &Value) -> EngineResult<BigInt> {
    let empty = Value::object([]);
    let limits = optional(policy, "limits", &empty)?;
    let default = Value::from(900_i64);
    let value = optional(limits, "branch_lease_seconds", &default)?;
    Ok(integer::coerce(value).map_err(errors::ValidationFailure::from)?)
}

fn snapshot_error(error: SnapshotFailure) -> EngineError {
    match error {
        SnapshotFailure::Contract(error) => error.into(),
        SnapshotFailure::Io(error) => error.into(),
        SnapshotFailure::Json(error) => error.into(),
    }
}

fn refresh_failure_packet(
    session: &dyn PersistentSession,
    run: &Value,
    row: &Value,
    envelope: &mut Value,
    operation: &str,
    revision: &BigInt,
    context: &SemanticContext<'_>,
) -> EngineResult<()> {
    let mut previous = Vec::new();
    for input in errors::iterable_values(item(envelope, "inputs")?)? {
        if !item(&input, "kind")?.is_text("failure") {
            continue;
        }
        let stored = session
            .first(GraphRead::ArtifactContent, &[item(&input, "ref")?.clone()])?
            .ok_or_else(|| {
                EngineError::python("TypeError", "'NoneType' object is not subscriptable")
            })?;
        let content = item(&stored, "content_json")?;
        let empty = Value::from("{}");
        let payload = compat::loads_value(if content.truthy() { content } else { &empty })?;
        if optional(&payload, "kind", &Value::Null)?.is_text("repair_packet") {
            previous.push(item(&input, "ref")?.clone());
        }
    }
    if previous.is_empty() {
        return Ok(());
    }
    let mut packet = repair_packet(session, run, row, None, context.files.windows())?;
    packet.set("state_revision", Value::Integer(revision.clone()));
    let refreshed = evidence::canonical_ledger_artifact(
        &format!("repair-claim-{operation}").into(),
        &"failure".into(),
        &packet,
    )?;
    evidence::enforce_artifact_size(&refreshed.kind, &refreshed.size_bytes, context.policy)?;
    let run_id = item(run, "run_id")?.text().expect("validated run ID");
    persist_artifact(session, &run_id, &refreshed)?;
    let mut inputs = Vec::new();
    for input in errors::iterable_values(item(envelope, "inputs")?)? {
        let reference = item(&input, "ref")?;
        if !previous.iter().any(|prior| prior.container_eq(reference)) {
            inputs.push(input);
        }
    }
    inputs.push(refreshed.as_input());
    envelope.set("inputs", sorted_inputs(inputs)?);
    Ok(())
}

fn action(
    session: &dyn PersistentSession,
    current: &Value,
    revision: &BigInt,
    operation: &str,
    context: &SemanticContext<'_>,
    services: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let run_id = item(current, "run_id")?;
    let plan = session.first(GraphRead::ExecutionPlanStatus, std::slice::from_ref(run_id))?;
    if plan
        .as_ref()
        .is_none_or(|plan| !plan["status"].is_text("approved"))
    {
        return Err(EngineError::state("EXECUTION_PLAN_APPROVAL_REQUIRED"));
    }
    if session
        .first(GraphRead::AwaitingFanout, std::slice::from_ref(run_id))?
        .is_some()
    {
        return Err(EngineError::state("FANOUT_ASSESSMENT_REQUIRED"));
    }
    if session
        .first(GraphRead::AwaitingReview, std::slice::from_ref(run_id))?
        .is_some()
    {
        return Err(EngineError::state("FANOUT_ASSESSMENT_REQUIRED"));
    }
    let row = session
        .first(GraphRead::FirstReadyNode, std::slice::from_ref(run_id))?
        .ok_or_else(|| EngineError::state("CLAIM_CONFLICT"))?;
    let mut envelope = compat::loads_value(item(&row, "envelope_json")?)?;
    let dependency = session.first(
        GraphRead::ClaimFanoutDependency,
        &[run_id.clone(), item(&row, "branch_id")?.clone()],
    )?;
    let review_dependency = session.first(
        GraphRead::ClaimReviewDependency,
        &[item(&row, "branch_id")?.clone()],
    )?;
    if dependency.is_some() || review_dependency.is_some() {
        return Err(EngineError::state("FANOUT_DEPENDENCY_STATE_INVALID"));
    }
    if envelope.as_object().is_none() {
        let message = if matches!(envelope, Value::List(_)) {
            "list indices must be integers or slices, not str".to_owned()
        } else {
            format!(
                "'{}' object does not support item assignment",
                envelope.python_type()
            )
        };
        return Err(EngineError::python("TypeError", message));
    }
    envelope.set("status", "running".into());
    if item(current, "state_schema_version")?.py_eq(&7_i64.into()) && !item(&row, "depth")?.truthy()
    {
        refresh_failure_packet(
            session,
            current,
            &row,
            &mut envelope,
            operation,
            revision,
            context,
        )?;
    }
    let path = policy::path_from_value(context.files, item(current, "task_path")?)?;
    let roots = task_roots(context)?;
    let limit = item(
        item(item(context.policy, "artifact_kinds")?, "task_brief")?,
        "max_bytes",
    )?;
    let full_task =
        snapshots::safe_json_snapshot_integer(context.snapshots, &path, &roots, maximum(limit)?)
            .map_err(snapshot_error)?
            .parsed
            .expect("safe JSON snapshot has parsed value");
    if !constraint_markers(&full_task)?.is_empty()
        && !item(current, "state_schema_version")?.py_eq(&7_i64.into())
    {
        return Err(EngineError::state("EVIDENCE_ENABLE_REQUIRED"));
    }
    if item(current, "state_schema_version")?.py_eq(&7_i64.into())
        && item(&row, "stage")?.is_text("delivery")
    {
        let source_files =
            services
                .source_files
                .create(context.repository, context.policy, context.files);
        let verification = EvidenceVerification {
            session,
            run: current,
            context: *context,
            clock: services.runtime,
            source_files: &*source_files,
            process: services.process,
        };
        let (binding, _) = verify_review_source(
            session,
            services.runtime,
            &verification,
            current,
            None,
            !item(&row, "node_key")?.is_text("supervisor_delivery_consolidation"),
        )?;
        let mut inputs = Vec::new();
        for input in errors::iterable_values(item(&envelope, "inputs")?)? {
            let reference = item(&input, "ref")?.clone();
            errors::require_hashable(&reference)?;
            inputs.push(reference);
        }
        let reference = item(&binding, "ref")?;
        errors::require_hashable(reference)?;
        if !inputs
            .iter()
            .any(|input| errors::container_equal(input, reference))
        {
            return Err(EngineError::state("REVIEW_BINDING_MISMATCH"));
        }
    }
    let attempt_started = Value::from(services.runtime.utc_now()?);
    let started = item(&row, "started_at")?;
    envelope.set(
        "started_at",
        if started.truthy() {
            started.clone()
        } else {
            attempt_started.clone()
        },
    );
    let identity = new_claim_identity(services.runtime)?;
    envelope.set("attempt_id", identity.attempt_id.clone().into());
    envelope.set("claim_digest", identity.claim_digest.clone().into());
    envelope.set(
        "lease_expires_at",
        services
            .runtime
            .utc_after(&branch_lease_seconds(context.policy)?)?
            .into(),
    );
    let attempt = session
        .first(
            GraphRead::NextAttemptNumber,
            &[item(&row, "branch_id")?.clone()],
        )?
        .ok_or_else(|| {
            EngineError::python("TypeError", "'NoneType' object is not subscriptable")
        })?;
    session.write(
        GraphWrite::ClaimAttempt,
        &[
            run_id.clone(),
            item(&row, "branch_id")?.clone(),
            item(&attempt, "attempt_number")?.clone(),
            identity.attempt_id.into(),
            identity.claim_digest.into(),
            attempt_started.clone(),
        ],
    )?;
    let json = String::from_utf8(compat::dumps(
        &envelope,
        SerializationProfile::SORTED_STORED,
    )?)
    .expect("ensure_ascii JSON is UTF-8");
    session.write(
        GraphWrite::ClaimNode,
        &[
            attempt_started,
            json.into(),
            item(&row, "branch_id")?.clone(),
        ],
    )?;
    let dispatch =
        review_dispatch_projection(session, &envelope, Some(&identity.claim_token.into()))?;
    Ok(Value::object([
        ("code", "CLAIMED".into()),
        ("branch", dispatch),
    ]))
}

pub fn claim_next(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    context: &SemanticContext<'_>,
    services: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let operation = contracts::opaque(item(arguments, "op_id")?, "op_id")?;
    let request = Value::object([("command", "next.claim".into())]);
    let mut semantic = *context;
    semantic.evidence_enable = false;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, revision| {
            action(
                session,
                &current.fields,
                revision,
                &operation,
                &semantic,
                services,
            )
        },
        mutation_task_validator(&semantic, TaskValidationMutation::NextClaim),
        context.store.fault,
    )
}
