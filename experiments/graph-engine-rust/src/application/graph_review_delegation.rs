use super::{
    artifacts::persist_artifact,
    claim_runtime::{lease_expired, ClaimServices},
    error::{EngineError, EngineResult, ManifestJsonTraceSite, TaskValidationMutation},
    graph_fanout::verify_evidence,
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    graph_record_control::{check_attempt, claim_token_digest, finish_running_attempt},
    graph_review_collection,
    graph_transitions::{promote_review_successors, review_assignment},
    graph_writes::GraphWrite,
    manifest_snapshot::inbox_snapshot,
    persistent_store::{node_values, PersistentSession, StatePlatform},
    semantic_state::{mutation_task_validator, SemanticContext},
    state::mutate_persistent,
};
use crate::{
    compat::{self, SerializationProfile, Value},
    domain::{
        contracts::{opaque, validate_ref, ContractError},
        evidence::canonical_ledger_artifact,
        fanout_contract::validate_fanout_assessment,
        planner::validate_fanout_ordering,
        python_operations::{add, greater, sort_comparable_values},
        review_contracts::{validate_findings, validate_preliminary},
        review_identity::{delegated_identity, request_slot_id},
        review_request::{validate_fanout_request, FanoutRequestContext},
    },
};

pub(super) fn validate_child_result(
    ledger: &dyn GraphLedgerReadPort,
    branch: &Value,
    result: &mut Value,
) -> EngineResult<()> {
    let member_mismatch = || ContractError::new("result", "DELEGATION_MEMBER_MISMATCH");
    let request = ledger
        .first(
            GraphRead::ReviewRequest,
            std::slice::from_ref(&branch["request_slot_id"]),
        )?
        .ok_or_else(member_mismatch)?;
    let artifact = ledger
        .first(
            GraphRead::ArtifactContent,
            std::slice::from_ref(&request["request_ref"]),
        )?
        .ok_or_else(member_mismatch)?;
    let request = compat::loads_value(&artifact["content_json"])?;
    let member = request["members"]
        .as_list()
        .and_then(|members| {
            members.iter().find(|member| {
                member["assignment_id"].py_eq(&branch["assignment_id"])
                    && member["ordinal"].py_eq(&branch["ordinal"])
            })
        })
        .ok_or_else(member_mismatch)?;
    let plan = ledger
        .first(
            GraphRead::ExecutionPlanBody,
            std::slice::from_ref(&branch["run_id"]),
        )?
        .ok_or_else(member_mismatch)?;
    let plan = compat::loads_value(&plan["plan_json"])?;
    let assignment = plan["conditional_review_assignments"]
        .as_list()
        .and_then(|assignments| {
            assignments
                .iter()
                .find(|assignment| assignment["assignment_id"].py_eq(&branch["assignment_id"]))
        })
        .ok_or_else(member_mismatch)?;
    if result["evidence"].truthy() {
        return Err(member_mismatch().into());
    }
    let empty = Value::List(vec![]);
    let findings = validate_findings(
        result.get("findings").unwrap_or(&empty),
        &member["evidence_ids"],
        "findings",
        Some(&member["acceptance_ids"]),
        Some(&assignment["scope_refs"]),
        true,
    )?;
    result.set("findings", Value::List(findings));
    Ok(())
}

pub(super) fn promote_child_successors(
    session: &dyn PersistentSession,
    run: &Value,
    slot: &Value,
    clock: &dyn StatePlatform,
) -> EngineResult<Vec<Value>> {
    if !slot.truthy() {
        return Ok(Vec::new());
    }
    let promoted = promote_review_successors(session, slot)?;
    graph_review_collection::settle(session, run, slot, clock)?;
    Ok(promoted)
}

fn child_envelope(
    run: &Value,
    parent: &Value,
    policy: &Value,
    slot: &Value,
    member: &Value,
    assignment: &Value,
    preliminary: &Value,
) -> EngineResult<Value> {
    let assignment_id = &assignment["assignment_id"];
    let ordinal = &member["ordinal"];
    let branch = delegated_identity(slot, assignment_id, ordinal, &"branch".into())?;
    let node = delegated_identity(slot, assignment_id, ordinal, &"node".into())?;
    let role = &assignment["role"];
    let template_key = if role.is_text("code_reviewer") {
        "code_reviewer"
    } else {
        "security_reviewer"
    };
    let template = &policy["node_templates"][template_key];
    let max_retries = &template["max_retries"];
    let mut output_contract = template["output_contract"].clone();
    output_contract.set("artifact_required", false.into());
    let evidence_ids = member["evidence_ids"].as_list().expect("validated IDs");
    let evidence = preliminary["evidence"]
        .as_list()
        .expect("validated preliminary evidence")
        .iter()
        .filter(|item| evidence_ids.iter().any(|id| id.py_eq(&item["evidence_id"])))
        .map(|item| {
            Value::object([
                ("evidence_id", item["evidence_id"].clone()),
                ("kind", item["kind"].clone()),
                ("sha256", item["sha256"].clone()),
            ])
        });
    let review_assignment = Value::object([
        ("review_lens", assignment["review_lens"].clone()),
        ("prompt_template", assignment["prompt_template"].clone()),
        ("reason_code", member["reason_code"].clone()),
        ("acceptance_ids", member["acceptance_ids"].clone()),
        ("evidence_ids", member["evidence_ids"].clone()),
        ("evidence", Value::list(evidence)),
        ("scope_refs", assignment["scope_refs"].clone()),
    ]);
    Ok(Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", run["run_id"].clone()),
        ("branch_id", branch.into()),
        ("node_instance_id", node.into()),
        ("node_key", "delegated_review".into()),
        ("role", role.clone()),
        ("model", assignment["model"].clone()),
        ("reasoning_effort", assignment["reasoning_effort"].clone()),
        ("mandatory", true.into()),
        ("generation", parent["generation"].clone()),
        ("status", "pending".into()),
        ("inputs", Value::List(vec![])),
        (
            "effect_capabilities",
            assignment["effect_capabilities"].clone(),
        ),
        ("output_contract", output_contract),
        (
            "stopping_condition",
            Value::object([
                ("kind", "valid_result_returned".into()),
                ("max_branch_attempts", add(max_retries, &1_i64.into())?),
            ]),
        ),
        ("artifact_ref", Value::Null),
        ("evidence", Value::List(vec![])),
        ("decision", Value::Null),
        ("retry_count", 0_i64.into()),
        ("max_retries", max_retries.clone()),
        ("attempt_id", Value::Null),
        ("claim_digest", Value::Null),
        ("lease_expires_at", Value::Null),
        ("failure_code", Value::Null),
        ("started_at", Value::Null),
        ("finished_at", Value::Null),
        ("parent_branch_id", parent["branch_id"].clone()),
        ("request_slot_id", slot.clone()),
        ("assignment_id", assignment_id.clone()),
        ("ordinal", ordinal.clone()),
        ("depth", 1_i64.into()),
        ("review_assignment", review_assignment),
    ]))
}

pub(super) fn record_request(
    session: &dyn PersistentSession,
    run_id: &str,
    args: &Value,
    policy: &Value,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let limit = (256 * 1024).into();
    let preliminary_snapshot = inbox_snapshot(
        context,
        policy,
        run_id,
        &args["preliminary_manifest"],
        limit,
        ManifestJsonTraceSite::ReviewPreliminary,
    )?;
    let request_snapshot = inbox_snapshot(
        context,
        policy,
        run_id,
        &args["request_manifest"],
        (256 * 1024).into(),
        ManifestJsonTraceSite::ReviewRequest,
    )?;
    let authority = Value::String(validate_ref(
        &args["authority_ref"],
        "authority_ref",
        false,
    )?);
    let claim_digest = Value::from(claim_token_digest(&args["claim_token"])?);
    let branch_id = Value::from(opaque(&args["branch_id"], "branch_id")?);
    let attempt_id = Value::from(opaque(&args["attempt_id"], "attempt_id")?);
    let preliminary_digest = Value::from(preliminary_snapshot.digest);
    let request_digest = Value::from(request_snapshot.digest);
    let operation_request = Value::object([
        ("command", "record.review-fanout".into()),
        ("branch_id", branch_id.clone()),
        ("attempt_id", attempt_id.clone()),
        ("claim_digest", claim_digest.clone()),
        ("preliminary_digest", preliminary_digest.clone()),
        ("request_digest", request_digest.clone()),
        ("authority_ref", authority.clone()),
    ]);
    let operation = opaque(&args["op_id"], "op_id")?;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &operation_request,
        |session, current, _| {
            let run = &current.fields;
            let parent = session
                .first(GraphRead::Node, std::slice::from_ref(&branch_id))?
                .ok_or_else(|| EngineError::state("BRANCH_NOT_FOUND"))?;
            if !parent["role"].is_text("code_reviewer")
                || !parent["depth"].py_eq(&0_i64.into())
                || parent["node_key"].is_text("code_reviewer_naming")
                || parent["node_key"].is_text("code_reviewer_bug_hunter")
            {
                return Err(EngineError::state("DELEGATION_PARENT_FORBIDDEN"));
            }
            let plan_row = session
                .first(
                    GraphRead::ExecutionPlan,
                    std::slice::from_ref(&run["run_id"]),
                )?
                .ok_or_else(|| EngineError::state("EXECUTION_PLAN_STATE_INVALID"))?;
            let plan = compat::loads_value(&plan_row["plan_json"])?;
            let limits = &plan["reviewer_delegation_limits"];
            let assignments = &plan["conditional_review_assignments"];
            if !limits.truthy() || !assignments.truthy() {
                return Err(EngineError::state("DELEGATION_DISABLED"));
            }
            let raw_request = request_snapshot
                .parsed
                .as_ref()
                .expect("safe JSON snapshot is parsed");
            let round = &raw_request["round"];
            if round.strict_integer().is_none() {
                return Err(
                    ContractError::new("review_fanout_request.round", "INVALID_ROUND").into(),
                );
            }
            let generation = &parent["generation"];
            let slot_text = request_slot_id([
                &run["run_id"],
                &run["policy_digest"],
                &plan_row["plan_digest"],
                &branch_id,
                &attempt_id,
                &claim_digest,
                generation,
                round,
            ])?;
            let slot = Value::from(slot_text.clone());
            if session
                .first(GraphRead::ReviewRequest, std::slice::from_ref(&slot))?
                .is_some()
            {
                return Err(EngineError::state("DELEGATION_SLOT_CONFLICT"));
            }
            let mut envelope = compat::loads_value(&parent["envelope_json"])?;
            check_attempt(&parent, &envelope, &attempt_id, &args["claim_token"], None)?;
            if lease_expired(&envelope["lease_expires_at"], claims.runtime)? {
                return Err(EngineError::state("LEASE_EXPIRED"));
            }
            let prior = session.all(
                GraphRead::ReviewRequestRounds,
                &[branch_id.clone(), generation.clone()],
            )?;
            if !round.py_eq(&(prior.len() + 1).into())
                || greater(round, &limits["max_request_rounds"])?
            {
                return Err(EngineError::state("DELEGATION_ROUND_LIMIT"));
            }
            if prior
                .last()
                .is_some_and(|row| !row["status"].is_text("sealed"))
            {
                return Err(EngineError::state("DELEGATION_ROUND_NOT_SETTLED"));
            }
            let preliminary = validate_preliminary(
                preliminary_snapshot
                    .parsed
                    .as_ref()
                    .expect("safe JSON snapshot is parsed"),
                &run["run_id"],
                &branch_id,
                &attempt_id,
                generation,
                Some(&envelope["inputs"]),
                false,
            )?;
            let request = validate_fanout_request(
                raw_request,
                &FanoutRequestContext {
                    run_id: &run["run_id"],
                    parent_branch_id: &branch_id,
                    parent_attempt_id: &attempt_id,
                    round_number: round,
                    plan_assignments: assignments,
                    preliminary: &preliminary,
                    limits,
                    depth: &parent["depth"],
                    historical: false,
                },
            )?;
            let members = request["members"].as_list().expect("validated members");
            let usage = session
                .first(
                    GraphRead::ReviewDispatchUsage,
                    std::slice::from_ref(&run["run_id"]),
                )?
                .expect("aggregate returns one row");
            if greater(
                &add(&usage["children"], &members.len().into())?,
                &limits["max_children_per_run"],
            )? {
                return Err(EngineError::state("DELEGATION_RUN_LIMIT"));
            }
            let member_assignments = members
                .iter()
                .map(|member| review_assignment(&plan, &member["assignment_id"]))
                .collect::<EngineResult<Vec<_>>>()?;
            let mut cost = 0_i64.into();
            for assignment in &member_assignments {
                cost = add(&cost, &assignment["dispatch_weight"])?;
            }
            if greater(
                &add(&usage["cost"], &cost)?,
                &limits["max_weighted_dispatch_cost"],
            )? {
                return Err(EngineError::state("DELEGATION_COST_LIMIT"));
            }
            verify_evidence(
                session,
                run_id,
                context,
                policy,
                preliminary["evidence"]
                    .as_list()
                    .expect("validated evidence"),
            )?;
            let preliminary_artifact = canonical_ledger_artifact(
                &format!("{slot_text}-preliminary").into(),
                &"delivery_review".into(),
                &preliminary,
            )?;
            let request_artifact = canonical_ledger_artifact(
                &format!("{slot_text}-request").into(),
                &"evidence_manifest".into(),
                &request,
            )?;
            persist_artifact(session, run_id, &preliminary_artifact)?;
            persist_artifact(session, run_id, &request_artifact)?;
            let mut tuples = Vec::new();
            let mut child_ids = Vec::new();
            for (member, assignment) in members.iter().zip(member_assignments.iter()) {
                let child = child_envelope(
                    run,
                    &parent,
                    policy,
                    &slot,
                    member,
                    assignment,
                    &preliminary,
                )?;
                let mut row = child.clone();
                row.set("stage", "review_delegation".into());
                row.set(
                    "envelope_json",
                    Value::String(compat::dumps_text(&child, SerializationProfile::CLI)?),
                );
                session.insert_node(&node_values(run_id, &row)?)?;
                let child_id = child["branch_id"].clone();
                tuples.push(Value::list([
                    child_id.clone(),
                    member["assignment_id"].clone(),
                    member["ordinal"].clone(),
                ]));
                child_ids.push(child_id);
            }
            let created_at = Value::from(context.store.platform.utc_now()?);
            let encoded_tuples =
                compat::dumps_text(&Value::List(tuples.clone()), SerializationProfile::CLI)?;
            session.write(
                GraphWrite::ReviewRequest,
                &[
                    slot.clone(),
                    run["run_id"].clone(),
                    branch_id.clone(),
                    attempt_id.clone(),
                    claim_digest,
                    generation.clone(),
                    round.clone(),
                    request_digest.clone(),
                    preliminary_artifact.reference,
                    preliminary_digest,
                    request_artifact.reference,
                    authority,
                    Value::String(encoded_tuples),
                    cost.clone(),
                    created_at,
                ],
            )?;
            for tuple in tuples {
                let tuple = tuple.as_list().expect("member tuple");
                session.write(
                    GraphWrite::ReviewMember,
                    &[
                        slot.clone(),
                        tuple[1].clone(),
                        tuple[2].clone(),
                        tuple[0].clone(),
                    ],
                )?;
            }
            session.write(
                GraphWrite::AwaitingReviewFanout,
                std::slice::from_ref(&slot),
            )?;
            session.write(
                GraphWrite::OpenReviewCollection,
                std::slice::from_ref(&slot),
            )?;
            let finished_at = Value::from(context.store.platform.utc_now()?);
            envelope.set("status", "waiting_for_review_children".into());
            envelope.set("finished_at", finished_at.clone());
            for key in ["attempt_id", "claim_digest", "lease_expires_at"] {
                envelope.set(key, Value::Null);
            }
            session.write(
                GraphWrite::WaitForReviewChildren,
                &[
                    Value::String(compat::dumps_text(
                        &envelope,
                        SerializationProfile::SORTED_STORED,
                    )?),
                    finished_at.clone(),
                    branch_id.clone(),
                ],
            )?;
            finish_running_attempt(
                session,
                &run["run_id"],
                &branch_id,
                &attempt_id,
                finished_at,
                &"delegated".into(),
            )?;
            sort_comparable_values(&mut child_ids)?;
            Ok(Value::object([
                ("code", "REVIEW_FANOUT_RECORDED".into()),
                ("request_slot_id", slot),
                ("request_digest", request_digest),
                ("child_branch_ids", Value::List(child_ids)),
                ("dispatch_cost", cost),
            ]))
        },
        mutation_task_validator(context, TaskValidationMutation::ReviewFanout),
        context.store.fault,
    )
}

pub(super) fn record_assessment(
    session: &dyn PersistentSession,
    run_id: &str,
    args: &Value,
    policy: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    let snapshot = inbox_snapshot(
        context,
        policy,
        run_id,
        &args["assessment_manifest"],
        (256 * 1024).into(),
        ManifestJsonTraceSite::ReviewAssessment,
    )?;
    let authority = Value::String(validate_ref(
        &args["authority_ref"],
        "authority_ref",
        false,
    )?);
    let slot_text = opaque(&args["request_slot_id"], "request_slot_id")?;
    let slot = Value::from(slot_text.clone());
    let request = Value::object([
        ("command", "record.review-fanout-assessment".into()),
        ("request_slot_id", slot.clone()),
        ("assessment_digest", snapshot.digest.into()),
        ("authority_ref", authority.clone()),
    ]);
    let operation = opaque(&args["op_id"], "op_id")?;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let fanout = session
                .first(GraphRead::ReviewFanout, std::slice::from_ref(&slot))?
                .ok_or_else(|| EngineError::state("FANOUT_NOT_FOUND"))?;
            if !fanout["status"].is_text("awaiting") {
                return Err(EngineError::state("FANOUT_ASSESSMENT_EXISTS"));
            }
            let members = session
                .all(GraphRead::ReviewChildBranches, std::slice::from_ref(&slot))?
                .into_iter()
                .map(|member| member["child_branch_id"].clone())
                .collect::<Vec<_>>();
            let mut manifest = validate_fanout_assessment(
                snapshot
                    .parsed
                    .as_ref()
                    .expect("safe JSON snapshot is parsed"),
                &current.fields["run_id"],
                &slot,
                &members,
            )?;
            let members = manifest["members"].as_list().expect("validated members");
            for member in members {
                let resources = &member["resources"];
                if [
                    "writable_paths",
                    "mutable_state_refs",
                    "exclusive_device_refs",
                ]
                .iter()
                .any(|key| resources[*key].truthy())
                {
                    return Err(EngineError::state("DELEGATION_READ_ONLY_REQUIRED"));
                }
            }
            let dependencies = validate_fanout_ordering(
                members,
                manifest["dependencies"]
                    .as_list()
                    .expect("validated dependencies"),
                context.case_sensitive,
            )
            .map_err(|error| EngineError::state(&error))?;
            let evidence = verify_evidence(
                session,
                run_id,
                context,
                policy,
                manifest["evidence"].as_list().expect("validated evidence"),
            )?;
            let assessed_at = Value::from(context.store.platform.utc_now()?);
            let actor = Value::String(context.provenance.actor()?);
            let host = Value::from(context.provenance.host_identity()?);
            manifest.set("dependencies", Value::List(dependencies.clone()));
            manifest.set("evidence", Value::List(evidence));
            manifest.set("authority_ref", authority.clone());
            manifest.set("actor", actor.clone());
            manifest.set("host_identity", host.clone());
            manifest.set("assessed_at", assessed_at.clone());
            let artifact = canonical_ledger_artifact(
                &format!("{slot_text}-assessment").into(),
                &"evidence_manifest".into(),
                &manifest,
            )?;
            persist_artifact(session, run_id, &artifact)?;
            for dependency in dependencies {
                session.write(
                    GraphWrite::ReviewDependency,
                    &[
                        slot.clone(),
                        dependency["before_branch_id"].clone(),
                        dependency["after_branch_id"].clone(),
                        dependency["reason"].clone(),
                    ],
                )?;
            }
            session.write(
                GraphWrite::ReviewFanoutAssessment,
                &[
                    artifact.reference,
                    artifact.sha256,
                    authority,
                    actor,
                    host,
                    assessed_at,
                    slot.clone(),
                ],
            )?;
            session.write(GraphWrite::StartReviewRequest, std::slice::from_ref(&slot))?;
            let ready = promote_review_successors(session, &slot)?;
            Ok(Value::object([
                ("code", "REVIEW_FANOUT_ASSESSMENT_RECORDED".into()),
                ("request_slot_id", slot),
                ("ready_branch_ids", Value::List(ready)),
            ]))
        },
        mutation_task_validator(context, TaskValidationMutation::ReviewFanoutAssessment),
        context.store.fault,
    )
}
