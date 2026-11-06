use super::{
    artifacts::persist_artifact,
    claim_runtime::ClaimServices,
    error::{EngineError, EngineResult, ManifestJsonTraceSite, TaskValidationMutation},
    graph_delivery::verified_review_source,
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    graph_payload::{persist_payload, sorted_inputs},
    graph_record_control::reopen_branch,
    graph_run_control::verify_control_evidence,
    graph_writes::GraphWrite,
    manifest_snapshot::inbox_snapshot,
    persistent_store::PersistentSession,
    run_context::RunContext,
    semantic_state::{mutation_task_validator, SemanticContext},
    state::mutate_persistent,
};
use crate::{
    compat::{self, PyString, Value},
    domain::{
        contracts, contracts::ContractError, errors, evidence::canonical_ledger_artifact, ids,
        python_dictionary::PythonDictionary, python_operations, source_paths,
    },
};

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}

fn optional<'a>(value: &'a Value, key: &str, default: &'a Value) -> EngineResult<&'a Value> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key).unwrap_or(default))
}

fn terminal(status: &Value) -> EngineResult<bool> {
    errors::require_hashable(status)?;
    Ok(["blocked", "complete", "aborted"]
        .iter()
        .any(|value| status.is_text(value)))
}

fn starts_with_filesystem(value: &Value) -> EngineResult<bool> {
    match value {
        Value::String(value) => Ok(value.starts_with("filesystem")),
        Value::Bytes(_) => Err(EngineError::python(
            "TypeError",
            "startswith first arg must be bytes or a tuple of bytes, not str",
        )),
        _ => Err(errors::attribute_error(value, "startswith").into()),
    }
}

fn path_text(value: &Value) -> EngineResult<&PyString> {
    match value {
        Value::String(value) => Ok(value),
        Value::Bytes(_) => Err(EngineError::python(
            "TypeError",
            "a bytes-like object is required, not 'str'",
        )),
        _ => Err(errors::attribute_error(value, "rstrip").into()),
    }
}

pub fn validate_recovery_manifest(
    value: &Value,
    run: &Value,
    branch: &Value,
    plan: &Value,
    reason_code: &Value,
    authority_ref: &Value,
) -> EngineResult<Value> {
    let fields = [
        "schema_version",
        "kind",
        "run_id",
        "branch_id",
        "blocked_reason",
        "reason_code",
        "authority_ref",
        "task_digest",
        "plan_digest",
        "failure_result_ref",
        "required_skill",
        "replacement_skill",
        "decision",
        "equivalence_basis",
        "prior_input_refs",
        "effect_capabilities_digest",
        "failure_evidence",
        "evidence",
    ];
    contracts::require_keys(value, &fields, &fields, "recovery_manifest")?;
    let blocked_reason = "MISSING_REQUIRED_REVIEW_SKILL";
    if !value["schema_version"].py_eq(&1_i64.into())
        || !value["kind"].is_text("recovery_authorization")
        || !value["run_id"].py_eq(item(run, "run_id")?)
        || !value["branch_id"].py_eq(item(branch, "branch_id")?)
        || !value["blocked_reason"].is_text(blocked_reason)
        || !value["reason_code"].py_eq(reason_code)
        || !value["authority_ref"].py_eq(authority_ref)
        || !value["task_digest"].py_eq(item(run, "task_digest")?)
        || !value["plan_digest"].py_eq(item(plan, "plan_digest")?)
    {
        return Err(ContractError::new("recovery_manifest", "AUTHORIZATION_MISMATCH").into());
    }
    let version = item(run, "state_schema_version")?;
    errors::require_hashable(version)?;
    let denied = || EngineError::state("RECOVERY_NOT_PERMITTED");
    if !(version.py_eq(&6_i64.into()) || version.py_eq(&7_i64.into()))
        || !item(run, "blocked_reason")?.is_text(blocked_reason)
    {
        return Err(denied());
    }
    if !item(plan, "status")?.is_text("approved")
        || !item(plan, "authority_ref")?.py_eq(authority_ref)
    {
        return Err(EngineError::state("RECOVERY_AUTHORITY_MISMATCH"));
    }
    if !item(branch, "node_key")?.is_text("code_reviewer_bug_hunter")
        || !crate::domain::contract_values::in_strings(
            item(branch, "status")?,
            &["failed", "timed_out"],
        )?
        || !item(branch, "failure_code")?.is_text("MISSING_REQUIRED_SKILL")
        || python_operations::greater_equal(
            item(branch, "retry_count")?,
            item(branch, "max_retries")?,
        )?
    {
        return Err(denied());
    }
    let envelope = compat::loads_value(item(branch, "envelope_json")?)?;
    let assignment = optional(&envelope, "code_review_assignment", &compat::value::NULL)?;
    if assignment.as_object().is_none() || !assignment["required_skill"].is_text("review-agent") {
        return Err(denied());
    }
    if !value["required_skill"].is_text("review-agent")
        || !value["replacement_skill"].is_text("bug-hunter-review")
    {
        return Err(ContractError::new("recovery_manifest", "UNSUPPORTED_EQUIVALENT_SKILL").into());
    }
    if !value["decision"].is_text("APPROVE")
        || !value["equivalence_basis"].is_text("user-authorized-equivalent-review")
    {
        return Err(
            ContractError::new("recovery_manifest", "EQUIVALENCE_ACCEPTANCE_REQUIRED").into(),
        );
    }
    contracts::validate_ref(&value["authority_ref"], "authority_ref", false)?;
    contracts::validate_ref(&value["failure_result_ref"], "failure_result_ref", true)?;
    let reference = python_operations::add(&"ledger:".into(), item(branch, "branch_id")?)?;
    let reference = python_operations::add(&reference, &"#sha256=".into())?;
    let reference = python_operations::add(&reference, item(branch, "result_digest")?)?;
    if !value["failure_result_ref"].py_eq(&reference) {
        return Err(ContractError::new("failure_result_ref", "FAILURE_RESULT_MISMATCH").into());
    }
    let mut refs = errors::iterable_values(item(&envelope, "inputs")?)?
        .iter()
        .map(|input| Ok(item(input, "ref")?.clone()))
        .collect::<EngineResult<Vec<_>>>()?;
    python_operations::sort_comparable_values(&mut refs)?;
    if !value["prior_input_refs"].py_eq(&Value::List(refs)) {
        return Err(ContractError::new("prior_input_refs", "IMMUTABLE_SCOPE_MISMATCH").into());
    }
    let capabilities = ids::canonical_bytes(item(&envelope, "effect_capabilities")?)?;
    if !value["effect_capabilities_digest"].is_text(&ids::sha256_bytes(&capabilities)) {
        return Err(
            ContractError::new("effect_capabilities_digest", "IMMUTABLE_SCOPE_MISMATCH").into(),
        );
    }
    if !value["failure_evidence"].py_eq(item(&envelope, "evidence")?) {
        return Err(ContractError::new("failure_evidence", "FAILURE_EVIDENCE_MISMATCH").into());
    }
    let items = value["evidence"]
        .as_list()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| ContractError::new("evidence", "EVIDENCE_REQUIRED"))?;
    let fields = ["kind", "ref", "sha256"];
    for evidence in items {
        contracts::require_keys(evidence, &fields, &fields, "evidence")?;
        contracts::bounded_string(&evidence["kind"], "evidence.kind", 64)?;
        contracts::validate_ref(&evidence["ref"], "evidence.ref", true)?;
        contracts::digest(&evidence["sha256"], "evidence.sha256")?;
    }
    Ok(value.clone())
}

pub(super) fn recover(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let limit = compat::integer::coerce(item(item(&loaded.policy, "limits")?, "manifest_bytes")?)?;
    let snapshot = inbox_snapshot(
        context,
        &loaded.policy,
        run_id,
        &arguments["recovery_manifest"],
        limit,
        ManifestJsonTraceSite::Recover,
    )?;
    let request = Value::object([
        ("command", "recover".into()),
        (
            "branch_id",
            contracts::opaque(&arguments["branch_id"], "branch_id")?.into(),
        ),
        (
            "reason_code",
            contracts::opaque(&arguments["reason_code"], "reason_code")?.into(),
        ),
        (
            "authority_ref",
            Value::String(contracts::validate_ref(
                &arguments["authority_ref"],
                "authority_ref",
                false,
            )?),
        ),
        ("manifest_digest", snapshot.digest.into()),
    ]);
    let operation = contracts::opaque(&arguments["op_id"], "op_id")?;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, revision| {
            let run = &current.fields;
            let branch_id = &request["branch_id"];
            let missing = || EngineError::state("BRANCH_NOT_FOUND");
            let plan = session.first(
                GraphRead::ExecutionPlan,
                std::slice::from_ref(&run["run_id"]),
            )?;
            let branch = session
                .first(
                    GraphRead::FenceNode,
                    &[run["run_id"].clone(), branch_id.clone()],
                )?
                .ok_or_else(missing)?;
            let plan = plan.ok_or_else(missing)?;
            let mut manifest = validate_recovery_manifest(
                snapshot
                    .parsed
                    .as_ref()
                    .expect("safe JSON snapshot is parsed"),
                run,
                &branch,
                &plan,
                &request["reason_code"],
                &request["authority_ref"],
            )?;
            let mut envelope = compat::loads_value(&branch["envelope_json"])?;
            let mut inputs = errors::iterable_values(&envelope["inputs"])?;
            if run["state_schema_version"].py_eq(&7_i64.into()) {
                let (binding, _) = verified_review_source(session, run, context, claims, None)?;
                if !inputs
                    .iter()
                    .any(|input| input["ref"].py_eq(&binding["ref"]))
                {
                    return Err(EngineError::state("REVIEW_BINDING_MISMATCH"));
                }
            }
            let evidence = verify_control_evidence(session, run_id, &manifest, context)?;
            manifest.set("evidence", sorted_inputs(evidence)?);
            let digest = ids::sha256_bytes(operation.as_bytes());
            let authorization = canonical_ledger_artifact(
                &format!("recovery-{}", &digest[..55]).into(),
                &"evidence_manifest".into(),
                &manifest,
            )?;
            persist_artifact(session, run_id, &authorization)?;
            let mut active = run.clone();
            active.set("status", "active".into());
            let mut packet =
                repair_packet(session, &active, &branch, None, context.files.windows())?;
            packet.set("state_revision", Value::Integer(revision.clone()));
            packet.set(
                "recovery_authorization_ref",
                authorization.reference.clone(),
            );
            packet.set("next_permitted_action", "retry".into());
            let repair = persist_payload(
                session,
                run,
                context.policy,
                &format!("repair-{}", &digest[..57]).into(),
                &"failure".into(),
                &packet,
            )?;
            inputs.extend([authorization.as_input(), repair.as_input()]);
            envelope.set("inputs", sorted_inputs(inputs)?);
            reopen_branch(session, branch_id, &mut envelope, &branch["retry_count"])?;
            session.write(GraphWrite::RecoverRun, std::slice::from_ref(&run["run_id"]))?;
            Ok(Value::object([
                ("code", "GRAPH_RECOVERED".into()),
                ("status", "active".into()),
                ("branch_id", branch_id.clone()),
                ("branch_status", "ready".into()),
                ("recovery_authorization_ref", authorization.reference),
                ("repair_packet_ref", repair.reference),
                (
                    "required_return_gates",
                    Value::list([
                        branch["node_key"].clone(),
                        "code_reviewer_architecture".into(),
                    ]),
                ),
            ]))
        },
        mutation_task_validator(context, TaskValidationMutation::Recover),
        context.store.fault,
    )
}

fn has_recovery_context(branch: &Value) -> EngineResult<bool> {
    let status = item(branch, "status")?;
    errors::require_hashable(status)?;
    if status.is_text("failed") || status.is_text("timed_out") {
        return Ok(true);
    }
    if !["ready", "pending", "running"]
        .iter()
        .any(|state| status.is_text(state))
    {
        return Ok(false);
    }
    let envelope = compat::loads_value(item(branch, "envelope_json")?)?;
    for input in errors::iterable_values(item(&envelope, "inputs")?)? {
        if item(&input, "kind")?.is_text("failure") {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn recovery_packets(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    branch_id: &Value,
    windows: bool,
) -> EngineResult<Vec<Value>> {
    let branches = ledger.all(
        GraphRead::RootBranches,
        std::slice::from_ref(item(run, "run_id")?),
    )?;
    let mut packets = Vec::new();
    for branch in &branches {
        let include = if branch_id.truthy() {
            branch_id.py_eq(item(branch, "branch_id")?)
        } else {
            has_recovery_context(branch)?
        };
        if include {
            packets.push(repair_packet(ledger, run, branch, None, windows)?);
        }
    }
    if branch_id.truthy() && packets.is_empty() {
        return Err(EngineError::state("BRANCH_NOT_FOUND"));
    }
    Ok(packets)
}

pub fn repair_packet(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    branch: &Value,
    judgment: Option<&Value>,
    windows: bool,
) -> EngineResult<Value> {
    let envelope = compat::loads_value(item(branch, "envelope_json")?)?;
    let plan = ledger.first(
        GraphRead::ActivationPlanDigest,
        &[item(run, "run_id")?.clone()],
    )?;
    let mut allowances = PythonDictionary::default();
    for budget in ledger.all(
        GraphRead::BudgetsForValidation,
        &[item(run, "run_id")?.clone()],
    )? {
        let key = item(&budget, "budget_id")?.clone();
        let remaining =
            python_operations::subtract(item(&budget, "limit_value")?, item(&budget, "used")?)?;
        allowances.insert(key, remaining.into())?;
    }
    let sealed = ledger.first(
        GraphRead::SealedBranchMembership,
        &[item(branch, "branch_id")?.clone()],
    )?;
    let retryable = !terminal(item(run, "status")?)?
        && {
            let status = item(branch, "status")?;
            errors::require_hashable(status)?;
            status.is_text("failed") || status.is_text("timed_out")
        }
        && python_operations::less(item(branch, "retry_count")?, item(branch, "max_retries")?)?
        && sealed.is_none();
    let mut packet = Value::object([
        ("kind", "repair_packet".into()),
        ("schema_version", 1_i64.into()),
    ]);
    packet.set("run_id", item(run, "run_id")?.clone());
    packet.set("task_digest", item(run, "task_digest")?.clone());
    let plan = plan.ok_or_else(|| {
        EngineError::python("TypeError", "'NoneType' object is not subscriptable")
    })?;
    packet.set("plan_digest", item(&plan, "plan_digest")?.clone());
    packet.set("branch_id", item(branch, "branch_id")?.clone());
    packet.set(
        "attempt_id",
        optional(&envelope, "attempt_id", &Value::Null)?.clone(),
    );
    packet.set("generation", item(branch, "generation")?.clone());
    packet.set("state_revision", item(run, "state_revision")?.clone());
    packet.set("origin_gate", item(branch, "node_key")?.clone());
    let empty = Value::List(vec![]);
    packet.set(
        "failure_evidence",
        Value::List(errors::iterable_values(optional(
            &envelope, "evidence", &empty,
        )?)?),
    );
    let result_digest = item(branch, "result_digest")?;
    let failure_result = if result_digest.truthy() {
        let reference = python_operations::add(&"ledger:".into(), item(branch, "branch_id")?)?;
        let reference = python_operations::add(&reference, &"#sha256=".into())?;
        python_operations::add(&reference, result_digest)?
    } else {
        Value::Null
    };
    packet.set("failure_result_ref", failure_result);
    packet.set("failed_criterion_ids", Value::List(vec![]));
    packet.set("cause", "unknown".into());
    packet.set("source_changes", Value::List(vec![]));
    let mut authorized = Vec::new();
    for capability in errors::iterable_values(item(&envelope, "effect_capabilities")?)? {
        if starts_with_filesystem(item(&capability, "effect")?)? {
            authorized.push(capability);
        }
    }
    packet.set("authorized_scope", Value::List(authorized));
    packet.set(
        "relevant_inputs",
        Value::List(errors::iterable_values(item(&envelope, "inputs")?)?),
    );
    packet.set(
        "return_gates",
        Value::List(vec![item(branch, "node_key")?.clone()]),
    );
    packet.set(
        "remaining_retries",
        python_operations::subtract(item(branch, "max_retries")?, item(branch, "retry_count")?)?,
    );
    packet.set(
        "remaining_allowances",
        allowances.string_keyed_value().ok_or_else(|| {
            EngineError::python(
                "NotImplementedError",
                "general-key repair allowance mapping remains unimplemented",
            )
        })?,
    );
    packet.set(
        "next_permitted_action",
        if retryable {
            "retry"
        } else {
            "supervisor_assessment"
        }
        .into(),
    );
    packet.set(
        "unresolved_judgments",
        Value::list(
            [
                "failure_cause",
                "failed_criteria",
                "repair_scope",
                "source_change_summary_unavailable",
            ]
            .into_iter()
            .map(Value::from),
        ),
    );
    if !item(branch, "result_digest")?.truthy() {
        for input in errors::iterable_values(item(&envelope, "inputs")?)? {
            if !item(&input, "kind")?.is_text("failure") {
                continue;
            }
            let stored = ledger
                .first(GraphRead::ArtifactContent, &[item(&input, "ref")?.clone()])?
                .ok_or_else(|| {
                    EngineError::python("TypeError", "'NoneType' object is not subscriptable")
                })?;
            let content = item(&stored, "content_json")?;
            let empty = Value::from("{}");
            let prior = compat::loads_value(if content.truthy() { content } else { &empty })?;
            if optional(&prior, "kind", &Value::Null)?.is_text("repair_packet")
                && optional(&prior, "branch_id", &Value::Null)?.py_eq(item(branch, "branch_id")?)
            {
                for key in [
                    "attempt_id",
                    "failure_evidence",
                    "failure_result_ref",
                    "failed_criterion_ids",
                    "cause",
                    "authorized_scope",
                    "relevant_inputs",
                    "return_gates",
                    "unresolved_judgments",
                    "origin_gate",
                ] {
                    packet.set(key, item(&prior, key)?.clone());
                }
                if optional(&prior, "origin_kind", &Value::Null)?.is_text("delivery_review") {
                    for key in [
                        "origin_kind",
                        "origin_collection_id",
                        "origin_consolidation_ref",
                        "origin_reports",
                    ] {
                        packet.set(key, item(&prior, key)?.clone());
                    }
                    if !terminal(item(run, "status")?)? {
                        packet.set("next_permitted_action", "implement_repair".into());
                    }
                }
            }
        }
    }
    if let Some(judgment) = judgment {
        let fields = &[
            "schema_version",
            "kind",
            "cause",
            "failed_criterion_ids",
            "authorized_scope",
        ];
        contracts::require_keys(judgment, fields, fields, "repair_judgment")?;
        if !item(judgment, "schema_version")?.py_eq(&1_i64.into())
            || !item(judgment, "kind")?.is_text("repair_judgment")
            || {
                let cause = item(judgment, "cause")?;
                errors::require_hashable(cause)?;
                ![
                    "implementation",
                    "design",
                    "dependency",
                    "infrastructure",
                    "unknown",
                ]
                .iter()
                .any(|value| cause.is_text(value))
            }
        {
            return Err(ContractError::new("repair_judgment", "SCHEMA_MISMATCH").into());
        }
        let task = compat::loads_value(item(run, "task_json")?)?;
        let criteria = item(judgment, "failed_criterion_ids")?;
        let Some(criteria) = criteria.as_list() else {
            return Err(ContractError::new("repair_judgment", "UNKNOWN_CRITERION").into());
        };
        if errors::set_values(criteria)?.len() != criteria.len() {
            return Err(ContractError::new("repair_judgment", "UNKNOWN_CRITERION").into());
        }
        let criteria_set = errors::set_values(criteria)?;
        let acceptance =
            errors::set_values(&errors::iterable_values(item(&task, "acceptance_ids")?)?)?;
        if criteria_set.iter().any(|criterion| {
            !acceptance
                .iter()
                .any(|allowed| errors::container_equal(allowed, criterion))
        }) {
            return Err(ContractError::new("repair_judgment", "UNKNOWN_CRITERION").into());
        }
        let scope = item(judgment, "authorized_scope")?;
        let Some(capabilities) = scope.as_list() else {
            return Err(ContractError::new("repair_judgment", "INVALID_SCOPE").into());
        };
        if capabilities.len() > 32 {
            return Err(ContractError::new("repair_judgment", "INVALID_SCOPE").into());
        }
        for capability in capabilities {
            let fields = &["effect", "action", "target_ref"];
            contracts::require_keys(capability, fields, fields, "repair_scope")?;
            contracts::validate_ref(item(capability, "target_ref")?, "repair_scope", false)?;
            let mut permitted = false;
            for allowed in errors::iterable_values(item(&packet, "authorized_scope")?)? {
                if item(capability, "effect")?.py_eq(item(&allowed, "effect")?)
                    && item(capability, "action")?.py_eq(item(&allowed, "action")?)
                    && source_paths::contains(
                        path_text(item(&allowed, "target_ref")?)?,
                        path_text(item(capability, "target_ref")?)?,
                        windows,
                    )
                {
                    permitted = true;
                    break;
                }
            }
            if !permitted {
                return Err(ContractError::new("repair_scope", "AUTHORITY_EXCEEDED").into());
            }
        }
        packet.set("cause", item(judgment, "cause")?.clone());
        let mut criteria = criteria.to_vec();
        python_operations::sort_comparable_values(&mut criteria)?;
        packet.set("failed_criterion_ids", Value::List(criteria));
        packet.set("authorized_scope", scope.clone());
        packet.set(
            "unresolved_judgments",
            Value::List(vec!["source_change_summary_unavailable".into()]),
        );
        let cause = item(judgment, "cause")?;
        if cause.is_text("design") {
            packet.set(
                "return_gates",
                Value::list(["tech_lead", "architect"].into_iter().map(Value::from)),
            );
        } else if ["dependency", "infrastructure", "unknown"]
            .iter()
            .any(|value| cause.is_text(value))
        {
            packet.set("return_gates", Value::List(vec!["supervisor".into()]));
        }
    }
    Ok(packet)
}
