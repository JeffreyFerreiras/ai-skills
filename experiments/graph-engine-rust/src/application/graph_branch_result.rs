use super::{
    artifacts::{persist_artifact, ArtifactResolver},
    claim_runtime::{lease_expired, ClaimServices},
    error::{
        BranchResultTraceSite, EngineError, EngineResult, ManifestJsonTraceSite,
        TaskValidationMutation,
    },
    graph_delivery::{staged_consolidation, verify_delivery_result_source},
    graph_ledger::GraphRead,
    graph_nodes::{
        insert_collection_successors, insert_join_spec, insert_spec, task_input, SuccessorTopology,
    },
    graph_payload::{artifact_reference, sorted_inputs},
    graph_record_control::{check_attempt, claim_token_digest, finish_running_attempt},
    graph_review_delegation::{promote_child_successors, validate_child_result},
    graph_transitions::promote_fanout_successors,
    graph_writes::GraphWrite,
    manifest_snapshot::inbox_snapshot,
    persistent_store::PersistentSession,
    review_binding::validate_review_continuation_result,
    run_context::RunContext,
    semantic_state::{mutation_task_validator, SemanticContext},
    source_capture::SourceFileFactory,
    state::mutate_persistent,
};
use crate::{
    compat::{self, PyString, SerializationProfile, Value},
    domain::{
        contract_values::get,
        contracts, errors,
        evidence::canonical_ledger_artifact,
        planner, python_operations,
        result_contract::validate_result_manifest,
        task_contract::{validate_impact_map_with_site, ImpactMapValidationSite},
    },
};

fn stored_json(value: &Value, profile: SerializationProfile) -> EngineResult<Value> {
    Ok(String::from_utf8(compat::dumps(value, profile)?)
        .expect("stored JSON is UTF-8")
        .into())
}

fn verified_impact_refs(
    resolver: &ArtifactResolver<'_>,
    session: &dyn PersistentSession,
    run_id: &str,
    references: &Value,
) -> EngineResult<Value> {
    let marker = PyString::from("#sha256=");
    let mut normalized = Vec::new();
    for reference in errors::iterable_values(references)? {
        let text = reference.as_string().expect("validated impact reference");
        let artifact = match text
            .0
            .windows(marker.len())
            .rposition(|part| part == marker.0)
        {
            Some(index) => {
                let digest = Value::String(PyString(text.0[index + marker.len()..].to_vec()));
                resolver.resolve_reference(&reference, &digest, &"finding".into())?
            }
            None => resolver.resolve_unhashed_reference(&reference, &"finding".into())?,
        };
        persist_artifact(session, run_id, &artifact)?;
        normalized.push(artifact.reference);
    }
    python_operations::sort_comparable_values(&mut normalized)?;
    Ok(Value::List(normalized))
}

fn insert_initial_successors(
    session: &dyn PersistentSession,
    run: &Value,
    route: &str,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    source_files: &dyn SourceFileFactory,
) -> EngineResult<SuccessorTopology> {
    let inputs = [task_input(session, run)?];
    let specs = planner::initial_route_nodes(&loaded.policy, route)?;
    if ["design_only", "full_delivery"].contains(&route) {
        let collection = planner::collection_join("research", 0, specs);
        return insert_collection_successors(
            session,
            run,
            loaded,
            context,
            &collection,
            &inputs,
            source_files,
        );
    }
    let mut topology = SuccessorTopology::default();
    for spec in &specs {
        let row = insert_spec(
            session,
            run,
            loaded,
            context,
            spec,
            &inputs,
            "ready",
            source_files,
        )?;
        topology.branches.push(row["branch_id"].clone());
    }
    python_operations::sort_comparable_values(&mut topology.branches)?;
    Ok(topology)
}

fn normalize_result(
    session: &dyn PersistentSession,
    run_id: &str,
    resolver: &ArtifactResolver<'_>,
    contract: &Value,
    manifest: &Value,
) -> EngineResult<Value> {
    let mut result = validate_result_manifest(manifest, contract, false)?;
    if contract["depth"].truthy() {
        validate_child_result(session, contract, &mut result)?;
    }
    let resolve = |input: &Value| -> EngineResult<Value> {
        let artifact =
            resolver.resolve_reference(&input["ref"], &input["sha256"], &input["kind"])?;
        persist_artifact(session, run_id, &artifact)?;
        Ok(artifact_reference(&artifact))
    };
    if result["status"].is_text("succeeded") && result["artifact_ref"].as_object().is_some() {
        let artifact = resolve(&result["artifact_ref"])?;
        result.set("artifact_ref", artifact);
    }
    let mut evidence = Vec::new();
    for input in errors::iterable_values(&result["evidence"])? {
        evidence.push(resolve(&input)?);
    }
    result.set("evidence", sorted_inputs(evidence)?);
    Ok(result)
}

pub fn record(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let snapshot = inbox_snapshot(
        context,
        &loaded.policy,
        run_id,
        &arguments["result_manifest"],
        (256 * 1024).into(),
        ManifestJsonTraceSite::BranchResult,
    )?;
    let manifest = snapshot.parsed.expect("safe JSON snapshot is parsed");
    let token_digest = claim_token_digest(&arguments["claim_token"])?;
    let branch_id = &arguments["branch_id"];
    let request = Value::object([
        ("command", "record.branch-result".into()),
        ("branch_id", branch_id.clone()),
        (
            "attempt_id",
            contracts::opaque(&arguments["attempt_id"], "attempt_id")?.into(),
        ),
        ("claim_digest", token_digest.into()),
        ("manifest_digest", snapshot.digest.into()),
    ]);
    let operation = contracts::opaque(&arguments["op_id"], "op_id")?;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let branch = session
                .first(GraphRead::Node, std::slice::from_ref(branch_id))?
                .ok_or_else(|| EngineError::state("BRANCH_NOT_FOUND"))?;
            if !branch["status"].is_text("running") {
                return Err(EngineError::state("INVALID_BRANCH_TRANSITION"));
            }
            let mut envelope = compat::loads_value(&branch["envelope_json"])?;
            check_attempt(
                &branch,
                &envelope,
                &arguments["attempt_id"],
                &arguments["claim_token"],
                (!manifest.is_null()).then_some(&manifest),
            )?;
            if lease_expired(&envelope["lease_expires_at"], claims.runtime)? {
                return Err(EngineError::state("LEASE_EXPIRED"));
            }
            let mapper = branch["node_key"].is_text("impact_mapper");
            let tech_lead = branch["node_key"].is_text("tech_lead");
            let writer = branch["node_key"].is_text("senior_engineer");
            let advisory = branch["node_key"].is_text("advisory_reviewer");
            let delivery_consolidation =
                branch["node_key"].is_text("supervisor_delivery_consolidation");
            let delivery_reviewer = branch["stage"].is_text("delivery")
                && !branch["depth"].truthy()
                && !delivery_consolidation;
            let delegated_child =
                branch["stage"].is_text("review_delegation") && branch["depth"].truthy();
            let review_slot = branch["request_slot_id"].clone();
            if !mapper
                && !writer
                && !advisory
                && !delivery_reviewer
                && !delivery_consolidation
                && !delegated_child
                && !branch["stage"].is_text("research")
                && !branch["stage"].is_text("design")
            {
                return Err(EngineError::python(
                    "NotImplementedError",
                    "native branch result for this node remains unimplemented",
                ));
            }
            let run = &current.fields;
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
            let branch_text = branch_id.as_string().expect("parsed branch identifier");
            let mut topology = SuccessorTopology::default();
            let classified = mapper
                && !get(&manifest, "status")
                    .map_err(|error| {
                        EngineError::from(error)
                            .with_branch_result_trace(BranchResultTraceSite::MapperStatus)
                    })?
                    .is_text("failed");
            let mut result = if classified {
                let mut impact =
                    validate_impact_map_with_site(&manifest, &loaded.task, &loaded.policy)
                        .map_err(|failure| {
                            let error = EngineError::from(failure.error);
                            match failure.site {
                                Some(ImpactMapValidationSite::Route) => error
                                    .with_branch_result_trace(BranchResultTraceSite::MapperRoute),
                                Some(ImpactMapValidationSite::EvidenceReferences) => error
                                    .with_branch_result_trace(
                                        BranchResultTraceSite::MapperEvidenceReferences,
                                    ),
                                None => error,
                            }
                        })?;
                let route = impact["route_label"].text().expect("validated route label");
                let refs =
                    verified_impact_refs(&resolver, session, run_id, &impact["evidence_refs"])?;
                impact.set("evidence_refs", refs);
                session.write(
                    GraphWrite::SelectRoute,
                    &[
                        impact["route_label"].clone(),
                        stored_json(&impact["impact_tags"], SerializationProfile::STORED)?,
                        run["run_id"].clone(),
                    ],
                )?;
                topology = insert_initial_successors(
                    session,
                    run,
                    &route,
                    loaded,
                    context,
                    claims.source_files,
                )?;
                impact
            } else {
                validate_review_continuation_result(session, &branch, &envelope, &manifest)?;
                let mut contract = branch;
                contract.set("run_id", run["run_id"].clone());
                contract.set("output_contract", envelope["output_contract"].clone());
                normalize_result(session, run_id, &resolver, &contract, &manifest)?
            };
            let status = if classified {
                "succeeded".into()
            } else {
                result["status"].clone()
            };
            let succeeded = status.is_text("succeeded");
            let redesign = writer && result["decision"].is_text("REDESIGN_REQUIRED");
            let format7 = run["state_schema_version"].py_eq(&7_i64.into());
            if format7 && delivery_consolidation {
                if let Some(staged) =
                    staged_consolidation(session, run, &manifest, &resolver, context)?
                {
                    result.set("artifact_ref", staged);
                }
            }
            let mut artifact = result["artifact_ref"].clone();
            if succeeded
                && !redesign
                && artifact.is_null()
                && envelope["output_contract"]
                    .get("artifact_required")
                    .is_some_and(|required| !required.truthy())
            {
                let mut identifier = branch_text.clone();
                identifier.0.extend(PyString::from("-output").0);
                let output_kind = if classified {
                    "impact_map".into()
                } else {
                    result["output_kind"].clone()
                };
                let output = canonical_ledger_artifact(&identifier, &output_kind, &result)?;
                persist_artifact(session, run_id, &output)?;
                artifact = artifact_reference(&output);
            }
            let wrapper_kind = if succeeded {
                "branch_result"
            } else {
                "failure"
            };
            let wrapper = canonical_ledger_artifact(branch_text, &wrapper_kind.into(), &result)?;
            persist_artifact(session, run_id, &wrapper)?;
            if !succeeded && artifact.is_null() {
                artifact = artifact_reference(&wrapper);
            }
            if (delivery_reviewer || delivery_consolidation)
                && succeeded
                && format7
                && ["APPROVE", "PASS", "ACCEPT"]
                    .iter()
                    .any(|decision| result["decision"].is_text(decision))
            {
                verify_delivery_result_source(session, run, &envelope, &artifact, context, claims)?;
            }
            envelope.set("artifact_ref", artifact);
            envelope.set("decision", result["decision"].clone());
            envelope.set("failure_code", result["failure_code"].clone());
            envelope.set(
                "evidence",
                if classified {
                    Value::List(vec![])
                } else {
                    result["evidence"].clone()
                },
            );
            let finished_at = Value::from(claims.runtime.utc_now()?);
            envelope.set("status", status.clone());
            envelope.set("finished_at", finished_at.clone());
            session.write(
                GraphWrite::BranchResult,
                &[
                    status.clone(),
                    stored_json(&envelope, SerializationProfile::SORTED_STORED)?,
                    wrapper.content_json,
                    wrapper.sha256,
                    envelope["failure_code"].clone(),
                    finished_at.clone(),
                    branch_id.clone(),
                ],
            )?;
            finish_running_attempt(
                session,
                &run["run_id"],
                branch_id,
                &request["attempt_id"],
                finished_at,
                &status,
            )?;
            if succeeded && (tech_lead || writer || advisory) {
                let updated = session
                    .first(GraphRead::Node, std::slice::from_ref(branch_id))?
                    .ok_or_else(|| EngineError::state("BRANCH_NOT_FOUND"))?;
                let join = if redesign {
                    let member = planner::as_spec(&updated);
                    planner::collection_join("delivery", member.generation, vec![member])
                } else {
                    planner::next_join_for_success(&updated).expect("node creates a successor join")
                };
                topology.joins.push(insert_join_spec(session, run, &join)?);
            }
            let mut promoted = promote_fanout_successors(session, &run["run_id"])?;
            promoted.extend(promote_child_successors(
                session,
                run,
                &review_slot,
                context.store.platform,
            )?);
            Ok(Value::object([
                ("code", "BRANCH_RESULT_RECORDED".into()),
                ("branch_id", branch_id.clone()),
                ("branch_status", status),
                ("successor_branch_ids", Value::List(topology.branches)),
                ("created_join_ids", Value::List(topology.joins)),
                ("created_fanout_ids", Value::List(topology.fanouts)),
                ("promoted_branch_ids", Value::List(promoted)),
            ]))
        },
        mutation_task_validator(context, TaskValidationMutation::BranchResult),
        context.store.fault,
    )
}
