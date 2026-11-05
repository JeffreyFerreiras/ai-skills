use super::{
    artifacts::{persist_artifact, ArtifactResolver},
    check_eligibility::receipt_eligibility,
    check_fences::verify_check_fence,
    check_receipts::{validate_check_receipt, validate_receipt_integrity},
    claim_runtime::ClaimServices,
    error::{EngineError, EngineResult, TaskValidationMutation},
    evidence_lifecycle::verify_review_source,
    evidence_verification::EvidenceVerification,
    graph_consolidation::collection_document,
    graph_ledger::GraphRead,
    graph_payload::persist_payload,
    graph_writes::GraphWrite,
    join_validation::{canonical_collection_members, join_members, validate_join},
    persistent_store::PersistentSession,
    run_context::RunContext,
    semantic_state::{mutation_task_validator, SemanticContext},
    state::mutate_persistent,
};
use crate::{
    compat::{self, SerializationProfile, Value},
    domain::{check_receipts::configured_check, contract_values::get, contracts, errors, evidence},
};

fn artifact_content(session: &dyn PersistentSession, reference: &Value) -> EngineResult<Value> {
    let row = session
        .first(GraphRead::ArtifactContent, std::slice::from_ref(reference))?
        .ok_or_else(|| {
            EngineError::python("TypeError", "'NoneType' object is not subscriptable")
        })?;
    Ok(row["content_json"].clone())
}

fn eligible_required_checks(
    verification: &EvidenceVerification<'_>,
    task: &Value,
    registering: Option<&Value>,
) -> EngineResult<Value> {
    let mut checks = Vec::new();
    let session = verification.session;
    for row in session.all(
        GraphRead::CheckEvidenceOrdered,
        std::slice::from_ref(&verification.run["run_id"]),
    )? {
        let reference = &row["artifact_ref"];
        let receipt = compat::loads_value(&artifact_content(session, reference)?)?;
        if receipt_eligibility(&receipt, verification, registering, None)?.is_text("eligible") {
            checks.push((
                row["check_id"]
                    .as_string()
                    .expect("validated check ID")
                    .clone(),
                reference.clone(),
            ));
        }
    }
    let required = errors::iterable_set(&task["required_check_ids"])?;
    if required.len() != checks.len()
        || checks
            .iter()
            .any(|(id, _)| !errors::set_contains(&required, &Value::String(id.clone())))
    {
        return Err(EngineError::state("REQUIRED_CHECKS_INCOMPLETE"));
    }
    Ok(Value::Object(checks))
}

struct AcceptanceGate {
    review_ref: Value,
    binding: Value,
    collection_ref: Value,
    consolidation_digest: Value,
}

fn non_delivery_binding(
    verification: &EvidenceVerification<'_>,
    checks: &Value,
) -> EngineResult<Value> {
    let receipts = checks
        .as_object()
        .expect("selected check mapping")
        .iter()
        .map(|(_, reference)| {
            Ok(compat::loads_value(&artifact_content(
                verification.session,
                reference,
            )?)?)
        })
        .collect::<EngineResult<Vec<_>>>()?;
    let first = receipts
        .first()
        .ok_or_else(|| EngineError::python("IndexError", "list index out of range"))?;
    let source = &first["source"];
    let source_digest = &first["after_source_digest"];
    let coverage = &first["coverage_digest"];
    if receipts.iter().skip(1).any(|receipt| {
        !receipt["source"].py_eq(source)
            || !receipt["after_source_digest"].py_eq(source_digest)
            || !receipt["coverage_digest"].py_eq(coverage)
    }) {
        return Err(EngineError::state("ACCEPTANCE_BINDING_MISMATCH"));
    }
    let verified = verify_check_fence(
        verification.session,
        verification.clock,
        verification.run,
        source,
        false,
    )?;
    if !verified.branch["status"].is_text("succeeded")
        || !verified.branch["result_digest"].py_eq(&source["result_digest"])
    {
        return Err(EngineError::state("ACCEPTANCE_BINDING_MISMATCH"));
    }
    Ok(Value::object([
        ("source", source.clone()),
        ("source_digest", source_digest.clone()),
        ("coverage_digest", coverage.clone()),
        ("plan_digest", first["plan_digest"].clone()),
    ]))
}

fn acceptance_gate(
    verification: &EvidenceVerification<'_>,
    checks: &Value,
    registering: Option<&Value>,
) -> EngineResult<AcceptanceGate> {
    let session = verification.session;
    let run = verification.run;
    let route = &run["selected_route"];
    let delivery = ["full_delivery", "delivery_only", "fast_path"]
        .iter()
        .any(|label| route.is_text(label));
    let (review_ref, binding) = if delivery {
        let (row, binding) = verify_review_source(
            session,
            verification.clock,
            verification,
            run,
            registering,
            true,
        )?;
        (row["ref"].clone(), binding)
    } else {
        (Value::Null, non_delivery_binding(verification, checks)?)
    };
    let incomplete = || EngineError::state("ACCEPTANCE_GATE_INCOMPLETE");
    let run_id = &run["run_id"];
    if route.is_text("advisory") {
        let terminal = session
            .first(
                GraphRead::LatestSucceededContextNode,
                &[run_id.clone(), "advisory_reviewer".into()],
            )?
            .ok_or_else(incomplete)?;
        return Ok(AcceptanceGate {
            review_ref,
            binding,
            collection_ref: Value::Null,
            consolidation_digest: terminal["result_digest"].clone(),
        });
    }
    let (collection_key, consolidation_key, generation, outcome) = if delivery {
        (
            "delivery_collection",
            "supervisor_delivery_consolidation",
            &run["implementation_generation"],
            "ACCEPT",
        )
    } else {
        (
            "design_collection",
            "supervisor_design_consolidation",
            &run["design_generation"],
            "APPROVE",
        )
    };
    let collection = session
        .first(
            GraphRead::JoinForKeyGeneration,
            &[run_id.clone(), collection_key.into(), generation.clone()],
        )?
        .filter(|join| join["status"].is_text("sealed"))
        .ok_or_else(incomplete)?;
    let consolidation = session
        .first(
            GraphRead::SucceededNodeForGeneration,
            &[run_id.clone(), consolidation_key.into(), generation.clone()],
        )?
        .ok_or_else(incomplete)?;
    let result = compat::loads_value(&consolidation["result_json"])?;
    if !errors::string_item(&result, "outcome")?.is_text(outcome) {
        return Err(incomplete());
    }
    let join_id = &collection["join_id"];
    let members = canonical_collection_members(session, &join_members(session, join_id)?)?;
    let document = collection_document(join_id, members);
    let artifact = evidence::canonical_ledger_artifact(
        join_id.as_string().expect("validated join ID"),
        &"collection".into(),
        &document,
    )?;
    Ok(AcceptanceGate {
        review_ref,
        binding,
        collection_ref: artifact.reference,
        consolidation_digest: consolidation["result_digest"].clone(),
    })
}

fn record_bound_acceptance(
    verification: &EvidenceVerification<'_>,
    task: &Value,
    request: &Value,
    operation: &str,
    underlying: Value,
) -> EngineResult<Value> {
    let registering = Value::Tuple(vec![
        "supervisor_delivery_consolidation".into(),
        underlying.clone(),
    ]);
    let checks = eligible_required_checks(verification, task, Some(&registering))?;
    let gate = acceptance_gate(verification, &checks, Some(&registering))?;
    let run = verification.run;
    let wrapper = Value::object([
        ("kind", "acceptance_binding".into()),
        ("schema_version", 1_i64.into()),
        ("criterion_id", request["criterion_id"].clone()),
        ("run_id", run["run_id"].clone()),
        ("task_digest", run["task_digest"].clone()),
        ("plan_digest", gate.binding["plan_digest"].clone()),
        ("source", gate.binding["source"].clone()),
        ("source_digest", gate.binding["source_digest"].clone()),
        ("coverage_digest", gate.binding["coverage_digest"].clone()),
        ("review_source_binding_ref", gate.review_ref),
        ("collection_ref", gate.collection_ref),
        ("consolidation_digest", gate.consolidation_digest),
        ("checks", checks),
        ("underlying_evidence", underlying),
        (
            "actor",
            Value::String(verification.context.provenance.actor()?),
        ),
        ("predecessor_ref", request["predecessor_ref"].clone()),
    ]);
    let session = verification.session;
    let artifact = persist_payload(
        session,
        run,
        verification.context.policy,
        &format!("acceptance-{operation}").into(),
        &"acceptance_evidence".into(),
        &wrapper,
    )?;
    session.write(
        GraphWrite::ReplaceAcceptanceEvidence,
        &[
            run["run_id"].clone(),
            request["criterion_id"].clone(),
            artifact.reference.clone(),
            artifact.sha256,
        ],
    )?;
    Ok(Value::object([
        ("code", "ACCEPTANCE_EVIDENCE_RECORDED".into()),
        ("artifact_ref", artifact.reference),
    ]))
}

pub(super) fn record_acceptance(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let operation = contracts::opaque(&arguments["op_id"], "op_id")?;
    let mut request = Value::object([
        ("command", "record.acceptance-evidence".into()),
        (
            "artifact_ref",
            Value::String(contracts::validate_ref(
                &arguments["artifact_ref"],
                "artifact_ref",
                true,
            )?),
        ),
        (
            "artifact_sha256",
            contracts::digest(&arguments["artifact_sha256"], "artifact_sha256")?.into(),
        ),
        (
            "criterion_id",
            contracts::opaque(&arguments["criterion_id"], "criterion_id")?.into(),
        ),
    ]);
    if loaded.run.fields["state_schema_version"].py_eq(&7_i64.into()) {
        request.set("predecessor_ref", arguments["replace_ref"].clone());
    }
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let run = &current.fields;
            let criterion = &request["criterion_id"];
            let ids = errors::iterable_set(&loaded.task["acceptance_ids"])?;
            if !errors::set_contains(&ids, criterion) {
                return Err(EngineError::state("UNKNOWN_CRITERION"));
            }
            let resolver = ArtifactResolver {
                files: context.files,
                snapshots: context.snapshots,
                repository: context.repository,
                skill: context.skill,
                installation: context.installation,
                policy: &loaded.policy,
                runtime: context.runtime,
                ledger: Some(session),
            };
            let artifact = resolver.resolve_reference(
                &request["artifact_ref"],
                &request["artifact_sha256"],
                &"acceptance_evidence".into(),
            )?;
            persist_artifact(session, run_id, &artifact)?;
            let existing = session.first(
                GraphRead::AcceptanceForCriterion,
                &[run["run_id"].clone(), criterion.clone()],
            )?;
            if run["state_schema_version"].py_eq(&7_i64.into()) {
                let previous = existing
                    .as_ref()
                    .map_or(&compat::value::NULL, |row| &row["artifact_ref"]);
                if !previous.py_eq(&request["predecessor_ref"]) {
                    return Err(EngineError::state("ACCEPTANCE_PREDECESSOR_CONFLICT"));
                }
                let files =
                    claims
                        .source_files
                        .create(context.repository, context.policy, context.files);
                let verification = EvidenceVerification {
                    session,
                    run,
                    context: *context,
                    clock: claims.runtime,
                    source_files: &*files,
                    process: claims.process,
                };
                return record_bound_acceptance(
                    &verification,
                    &loaded.task,
                    &request,
                    &operation,
                    artifact.as_input(),
                );
            }
            if let Some(existing) = existing {
                if !existing["artifact_ref"].py_eq(&artifact.reference)
                    || !existing["artifact_sha256"].py_eq(&artifact.sha256)
                {
                    return Err(EngineError::state("EVIDENCE_CONFLICT"));
                }
            } else {
                session.write(
                    GraphWrite::AcceptanceEvidence,
                    &[
                        run["run_id"].clone(),
                        criterion.clone(),
                        artifact.reference,
                        artifact.sha256,
                    ],
                )?;
            }
            Ok(Value::object([(
                "code",
                "ACCEPTANCE_EVIDENCE_RECORDED".into(),
            )]))
        },
        mutation_task_validator(context, TaskValidationMutation::RecordControl),
        context.store.fault,
    )
}

fn passing_checks(
    session: &dyn PersistentSession,
    run_id: &Value,
    context: &SemanticContext<'_>,
    verification: Option<&EvidenceVerification<'_>>,
) -> EngineResult<Vec<Value>> {
    let mut passed = Vec::new();
    for check in session.all(
        GraphRead::CheckEvidenceOrdered,
        std::slice::from_ref(run_id),
    )? {
        let artifact = session
            .first(
                GraphRead::Artifact,
                std::slice::from_ref(&check["artifact_ref"]),
            )?
            .filter(|artifact| {
                artifact["source_type"].is_text("ledger") && artifact["content_json"].truthy()
            });
        let Some(artifact) = artifact else {
            continue;
        };
        let id = &check["check_id"];
        let receipt = (|| -> EngineResult<Value> {
            let receipt = compat::loads_value(&artifact["content_json"])?;
            let configured = configured_check(context.policy, id)?;
            if let Some(verification) = verification {
                validate_receipt_integrity(&receipt, run_id, id, configured, context.provenance)?;
                if !receipt_eligibility(&receipt, verification, None, None)?.is_text("eligible") {
                    return Ok(Value::Null);
                }
            } else {
                validate_check_receipt(&receipt, run_id, id, configured, context.provenance, true)?;
            }
            Ok(receipt)
        })();
        match receipt {
            Ok(receipt)
                if check["outcome"].is_text("PASS") && receipt["outcome"].is_text("PASS") =>
            {
                passed.push(id.clone());
            }
            Ok(_) | Err(EngineError::State(_) | EngineError::Contract(_)) => {}
            Err(EngineError::Json(error)) if error.kind == "JSONDecodeError" => {}
            Err(error) => return Err(error),
        }
    }
    Ok(passed)
}

pub(super) fn complete(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let operation = contracts::opaque(&arguments["op_id"], "op_id")?;
    let request = Value::object([("command", "complete".into())]);
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let run = &current.fields;
            let run_id = &run["run_id"];
            let parameters = std::slice::from_ref(run_id);
            let files = run["state_schema_version"].py_eq(&7_i64.into()).then(|| {
                claims
                    .source_files
                    .create(context.repository, context.policy, context.files)
            });
            let verification = files.as_deref().map(|files| EvidenceVerification {
                session,
                run,
                context: *context,
                clock: claims.runtime,
                source_files: files,
                process: claims.process,
            });
            if let Some(verification) = &verification {
                let checks = eligible_required_checks(verification, &loaded.task, None)?;
                let gate = acceptance_gate(verification, &checks, None)?;
                for selected in session.all(GraphRead::AcceptanceForStatus, parameters)? {
                    let content = artifact_content(session, &selected["artifact_ref"])?;
                    let wrapper = if content.truthy() {
                        compat::loads_value(&content)?
                    } else {
                        Value::object([])
                    };
                    if !get(&wrapper, "kind")?.is_text("acceptance_binding")
                        || !wrapper["review_source_binding_ref"].py_eq(&gate.review_ref)
                        || !wrapper["source_digest"].py_eq(&gate.binding["source_digest"])
                        || !wrapper["checks"].py_eq(&checks)
                        || !wrapper["collection_ref"].py_eq(&gate.collection_ref)
                        || !wrapper["consolidation_digest"].py_eq(&gate.consolidation_digest)
                    {
                        return Err(EngineError::state("ACCEPTANCE_BINDING_MISMATCH"));
                    }
                }
            }
            let closure = session
                .first(GraphRead::OpenClosure, parameters)?
                .ok_or_else(|| EngineError::state("CLOSURE_NOT_READY"))?;
            if validate_join(session, run, &closure)?.join_status != "READY" {
                return Err(EngineError::state("CLOSURE_NOT_READY"));
            }
            let criteria = errors::iterable_set(&loaded.task["acceptance_ids"])?;
            let evidenced = session.all(GraphRead::AcceptanceForStatus, parameters)?;
            if criteria.len() != evidenced.len()
                || evidenced
                    .iter()
                    .any(|row| !errors::set_contains(&criteria, &row["criterion_id"]))
            {
                return Err(EngineError::state("ACCEPTANCE_EVIDENCE_INCOMPLETE"));
            }
            let passed = passing_checks(session, run_id, context, verification.as_ref())?;
            let required = errors::iterable_set(&loaded.task["required_check_ids"])?;
            if required
                .iter()
                .any(|check| !errors::set_contains(&passed, check))
            {
                return Err(EngineError::state("REQUIRED_CHECKS_INCOMPLETE"));
            }
            let approvals = session.all(GraphRead::ApprovalsForValidation, parameters)?;
            if approvals
                .iter()
                .any(|approval| approval["decision"].is_text("REJECT"))
            {
                return Err(EngineError::state("APPROVAL_REJECTED"));
            }
            let required = errors::iterable_set(&loaded.task["required_human_decisions"])?;
            if required.iter().any(|id| {
                !approvals.iter().any(|approval| {
                    approval["decision"].is_text("APPROVE") && approval["approval_id"].py_eq(id)
                })
            }) {
                return Err(EngineError::state("REQUIRED_APPROVALS_INCOMPLETE"));
            }
            let sealed = compat::dumps_text(
                &Value::object([("complete", true.into())]),
                SerializationProfile::STORED,
            )?;
            session.write(
                GraphWrite::SealJoin,
                &[Value::String(sealed), closure["join_id"].clone()],
            )?;
            session.write(
                GraphWrite::CompleteRun,
                &[context.store.platform.utc_now()?.into(), run_id.clone()],
            )?;
            Ok(Value::object([
                ("code", "COMPLETE".into()),
                ("status", "complete".into()),
            ]))
        },
        mutation_task_validator(context, TaskValidationMutation::Complete),
        context.store.fault,
    )
}
