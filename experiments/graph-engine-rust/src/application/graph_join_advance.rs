use super::{
    artifacts::persist_artifact,
    claim_runtime::ClaimServices,
    error::{EngineError, EngineResult, ManifestJsonTraceSite, TaskValidationMutation},
    evidence_lifecycle::current_review_binding,
    graph_consolidation::{collection_document, delivery_nested_collections},
    graph_delivery::{attach_delivery_repair_packet, bind_review_source, verified_review_source},
    graph_ledger::GraphRead,
    graph_nodes::{
        insert_collection_successors, insert_join_spec, insert_spec, task_input, SuccessorTopology,
    },
    graph_payload::branch_result_input,
    graph_writes::GraphWrite,
    join_validation::{canonical_collection_members, join_members, validate_join},
    manifest_snapshot::inbox_snapshot,
    persistent_store::PersistentSession,
    run_context::RunContext,
    semantic_state::{mutation_task_validator, SemanticContext},
    state::mutate_persistent,
};
use crate::{
    compat::{self, SerializationProfile, Value},
    domain::{
        consolidation::validate_consolidation_manifest, contract_values::get, contracts, evidence,
        planner, python_operations,
    },
};
use num_bigint::BigInt;
use std::collections::BTreeMap;

fn result_inputs(
    session: &dyn PersistentSession,
    run: &Value,
    sources: &[Value],
    context_nodes: &[&str],
) -> EngineResult<Vec<Value>> {
    let mut inputs = vec![task_input(session, run)?];
    let substantive = |reference: &Value| -> EngineResult<Option<Value>> {
        if reference.as_object().is_some() {
            let artifact = session
                .first(GraphRead::Artifact, std::slice::from_ref(&reference["ref"]))?
                .filter(|row| {
                    row["kind"].py_eq(&reference["kind"])
                        && row["sha256"].py_eq(&reference["sha256"])
                })
                .ok_or_else(|| EngineError::state("INPUT_ARTIFACT_INVALID"))?;
            Ok(Some(Value::object([
                ("kind", artifact["kind"].clone()),
                ("ref", artifact["ref"].clone()),
                ("sha256", artifact["sha256"].clone()),
                ("size_bytes", artifact["size_bytes"].clone()),
            ])))
        } else {
            Ok(None)
        }
    };
    for source in sources {
        if source["result_json"].truthy() {
            let result = compat::loads_value(&source["result_json"])?;
            inputs.push(branch_result_input(session, source, &result)?);
            inputs.extend(substantive(get(&result, "artifact_ref")?)?);
        }
    }
    for key in context_nodes {
        if let Some(node) = session.first(
            GraphRead::LatestSucceededContextNode,
            &[run["run_id"].clone(), (*key).into()],
        )? {
            let result = compat::loads_value(&node["result_json"])?;
            inputs.extend(substantive(get(&result, "artifact_ref")?)?);
            inputs.push(branch_result_input(session, &node, &result)?);
        }
    }
    let unique = inputs
        .into_iter()
        .map(|input| {
            (
                input["ref"]
                    .as_string()
                    .expect("validated input reference")
                    .clone(),
                input,
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(unique.into_values().collect())
}

fn seal_join(session: &dyn PersistentSession, join_id: &Value, frozen: &Value) -> EngineResult<()> {
    let json = String::from_utf8(compat::dumps(frozen, SerializationProfile::SORTED_STORED)?)
        .expect("stored JSON is UTF-8");
    session.write(GraphWrite::SealJoin, &[json.into(), join_id.clone()])
}

fn advanced(join: &Value, outcome: Value, topology: SuccessorTopology) -> Value {
    Value::object([
        ("code", "JOIN_ADVANCED".into()),
        ("join_id", join.clone()),
        ("outcome", outcome),
        ("successor_branch_ids", Value::List(topology.branches)),
        ("created_join_ids", Value::List(topology.joins)),
        ("created_fanout_ids", Value::List(topology.fanouts)),
    ])
}

fn blocked(
    session: &dyn PersistentSession,
    run: &Value,
    join: &Value,
    reason: &str,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    session.write(
        GraphWrite::StopRun,
        &[
            "blocked".into(),
            reason.into(),
            context.store.platform.utc_now()?.into(),
            run["run_id"].clone(),
        ],
    )?;
    Ok(Value::object([
        ("code", "GRAPH_BLOCKED".into()),
        ("status", "blocked".into()),
        ("reason", reason.into()),
        ("join_id", join.clone()),
    ]))
}

fn consume_loop_budget(
    session: &dyn PersistentSession,
    run: &Value,
    budget_id: &str,
) -> EngineResult<bool> {
    let budget_id = Value::from(budget_id);
    let budget = session
        .first(
            GraphRead::BudgetForUse,
            &[run["run_id"].clone(), budget_id.clone()],
        )?
        .ok_or_else(|| {
            EngineError::python("TypeError", "'NoneType' object is not subscriptable")
        })?;
    if python_operations::greater_equal(&budget["used"], &budget["limit_value"])? {
        return Ok(false);
    }
    session.write(
        GraphWrite::IncrementBudget,
        &[1_i64.into(), run["run_id"].clone(), budget_id],
    )?;
    Ok(true)
}

fn advance_consolidation(
    session: &dyn PersistentSession,
    run: &Value,
    join: &Value,
    consolidation: &Value,
    revision: &BigInt,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let delivery = join["stage"].is_text("delivery");
    let source_key = if delivery {
        "delivery_collection"
    } else {
        "design_collection"
    };
    let manifest = compat::loads_value(&consolidation["result_json"])?;
    let source = session
        .first(
            GraphRead::JoinForKeyGeneration,
            &[
                run["run_id"].clone(),
                source_key.into(),
                join["generation"].clone(),
            ],
        )?
        .filter(|source| source["status"].is_text("sealed"))
        .ok_or_else(|| EngineError::state("SOURCE_COLLECTION_NOT_SEALED"))?;
    let members = join_members(session, &source["join_id"])?;
    let nested = if delivery {
        delivery_nested_collections(session, &members, &join["generation"])?
            .iter()
            .map(|row| compat::loads_value(&row["content_json"]))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![]
    };
    let outcome = validate_consolidation_manifest(
        &manifest,
        &join["stage"],
        &run["run_id"],
        &source["join_id"],
        &join["generation"],
        &members,
        &nested,
    )?;
    let format7 = run["state_schema_version"].py_eq(&7_i64.into());
    if format7 && delivery && outcome.is_text("ACCEPT") {
        verified_review_source(session, run, context, claims, None)?;
    }
    let consolidation_source = std::slice::from_ref(consolidation);
    let design_inputs = result_inputs(session, run, consolidation_source, &["tech_lead"])?;
    let delivery_inputs = result_inputs(
        session,
        run,
        consolidation_source,
        &["tech_lead", "senior_engineer"],
    )?;
    let mut topology = SuccessorTopology::default();
    let generation = join["generation"]
        .as_i64()
        .expect("validated join generation");
    let repairing = outcome.is_text("REPAIR");
    let redesigning = outcome.is_text("REDESIGN");
    let revising = outcome.is_text("REVISE");
    let budget = if repairing {
        Some(("delivery_repairs", "DELIVERY_REPAIR_LIMIT"))
    } else if revising || redesigning {
        Some(("design_revisions", "DESIGN_REVISION_LIMIT"))
    } else {
        None
    };
    if let Some((budget_id, reason)) = budget {
        if !consume_loop_budget(session, run, budget_id)? {
            blocked(session, run, &join["join_id"], reason, context)?;
            return Ok(advanced(&join["join_id"], "BLOCK".into(), topology));
        }
    }
    let implementation = run["implementation_generation"]
        .as_i64()
        .expect("validated implementation generation");
    if outcome.is_text("BLOCK") {
        blocked(
            session,
            run,
            &join["join_id"],
            "CONSOLIDATION_BLOCK",
            context,
        )?;
    } else if repairing {
        let generation = implementation + 1;
        let spec = planner::implementation_node(&loaded.policy, generation)?;
        let writer = insert_spec(
            session,
            run,
            loaded,
            context,
            &spec,
            &delivery_inputs,
            "ready",
            claims.source_files,
        )?;
        let writer_id = &writer["branch_id"];
        if format7 {
            attach_delivery_repair_packet(
                session,
                run,
                &join["join_id"],
                &source,
                &members,
                consolidation,
                writer_id,
                revision,
                context,
            )?;
        }
        topology.branches.push(writer_id.clone());
        session.write(
            GraphWrite::ImplementationGeneration,
            &[generation.into(), run["run_id"].clone()],
        )?;
    } else if revising || redesigning {
        let (generation, inputs) = if redesigning {
            let previous = run["design_generation"]
                .as_i64()
                .expect("validated design generation");
            (previous + 1, &delivery_inputs)
        } else {
            (generation + 1, &design_inputs)
        };
        let specs = planner::design_research_nodes(&loaded.policy, generation)?;
        let collection = planner::collection_join("research", generation, specs);
        topology = insert_collection_successors(
            session,
            run,
            loaded,
            context,
            &collection,
            inputs,
            claims.source_files,
        )?;
        session.write(
            GraphWrite::DesignGeneration,
            &[generation.into(), run["run_id"].clone()],
        )?;
        if redesigning {
            session.write(
                GraphWrite::ImplementationGeneration,
                &[(implementation + 1).into(), run["run_id"].clone()],
            )?;
        }
    } else if delivery || run["selected_route"].is_text("design_only") {
        let closure = planner::closure_join(planner::as_spec(consolidation), generation);
        topology
            .joins
            .push(insert_join_spec(session, run, &closure)?);
    } else if ["full_delivery", "delivery_only", "fast_path"]
        .iter()
        .any(|route| run["selected_route"].is_text(route))
    {
        let spec = planner::implementation_node(&loaded.policy, implementation)?;
        let successor = insert_spec(
            session,
            run,
            loaded,
            context,
            &spec,
            &design_inputs,
            "ready",
            claims.source_files,
        )?;
        topology.branches.push(successor["branch_id"].clone());
    } else {
        return Err(EngineError::state("ROUTE_TRANSITION_INVALID"));
    }
    Ok(advanced(&join["join_id"], outcome, topology))
}

pub fn advance(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let source_files = claims.source_files;
    let operation = contracts::opaque(&arguments["op_id"], "op_id")?;
    let join_id = Value::from(contracts::opaque(&arguments["join_id"], "join_id")?);
    let mut request = Value::object([
        ("command", "join.advance".into()),
        ("join_id", join_id.clone()),
    ]);
    let output_plan = if arguments["generated_output_plan"].truthy() {
        if !loaded.run.fields["state_schema_version"].py_eq(&7_i64.into()) {
            return Err(EngineError::state("EVIDENCE_ENABLE_REQUIRED"));
        }
        let output = inbox_snapshot(
            context,
            &loaded.policy,
            run_id,
            &arguments["generated_output_plan"],
            (256 * 1024).into(),
            ManifestJsonTraceSite::JoinAdvance,
        )?;
        request.set("generated_output_plan_digest", output.digest.into());
        let parsed = output.parsed.expect("safe JSON snapshot is parsed");
        request.set("generated_outputs", get(&parsed, "outputs")?.clone());
        Some(parsed)
    } else {
        None
    };
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, revision| {
            let run = &current.fields;
            let join = session
                .first(
                    GraphRead::JoinForRun,
                    &[join_id.clone(), run["run_id"].clone()],
                )?
                .ok_or_else(|| EngineError::state("JOIN_NOT_FOUND"))?;
            let research = join["join_key"].is_text("research_collection");
            let design_inputs = join["join_key"].is_text("design_inputs");
            let design_collection = join["join_key"].is_text("design_collection");
            let delivery_collection = join["join_key"].is_text("delivery_collection");
            let design_consolidation = join["join_key"].is_text("design_consolidation");
            let delivery_consolidation = join["join_key"].is_text("delivery_consolidation");
            let implementation = join["join_key"].is_text("implementation");
            let closure = join["kind"].is_text("closure");
            if !research
                && !design_inputs
                && !design_collection
                && !delivery_collection
                && !design_consolidation
                && !delivery_consolidation
                && !implementation
                && !closure
            {
                return Err(EngineError::python(
                    "NotImplementedError",
                    "native join advancement for this gate remains unimplemented",
                ));
            }
            let validation = validate_join(session, run, &join)?;
            if validation.join_status == "BLOCKED" {
                return blocked(session, run, &join_id, "MANDATORY_BRANCH_FAILED", context);
            }
            if validation.join_status != "READY" {
                return Err(EngineError::state(validation.join_status));
            }
            if closure {
                return Err(EngineError::state("COMPLETE_COMMAND_REQUIRED"));
            }
            let members = join_members(session, &join_id)?;
            if research
                && members
                    .iter()
                    .any(|member| !member["status"].is_text("succeeded"))
            {
                let mut response =
                    blocked(session, run, &join_id, "MANDATORY_RESEARCH_FAILED", context)?;
                response.set("outcome", "BLOCK".into());
                for key in [
                    "successor_branch_ids",
                    "created_join_ids",
                    "created_fanout_ids",
                ] {
                    response.set(key, Value::List(vec![]));
                }
                return Ok(response);
            }
            let frozen = canonical_collection_members(session, &members)?;
            let generation = join["generation"]
                .as_i64()
                .expect("validated join generation");
            if design_consolidation || delivery_consolidation {
                seal_join(session, &join_id, &frozen)?;
                return advance_consolidation(
                    session,
                    run,
                    &join,
                    &members[0],
                    revision,
                    loaded,
                    context,
                    claims,
                );
            }
            if design_inputs || implementation {
                seal_join(session, &join_id, &frozen)?;
                let source = &members[0];
                let mut inputs = if implementation {
                    result_inputs(
                        session,
                        run,
                        std::slice::from_ref(source),
                        &["tech_lead", "senior_engineer"],
                    )?
                } else {
                    result_inputs(session, run, &members, &[])?
                };
                let empty = "[]".into();
                let tags = compat::loads_value(if run["selected_tags_json"].truthy() {
                    &run["selected_tags_json"]
                } else {
                    &empty
                })?;
                let tags = tags
                    .as_list()
                    .expect("validated impact tags")
                    .iter()
                    .map(|tag| tag.text().expect("validated impact tag"))
                    .collect::<Vec<_>>();
                let (stage, specs) = if implementation {
                    let plan = session
                        .first(
                            GraphRead::ExecutionPlanBody,
                            std::slice::from_ref(&run["run_id"]),
                        )?
                        .ok_or_else(|| EngineError::state("EXECUTION_PLAN_STATE_INVALID"))?;
                    let plan = compat::loads_value(&plan["plan_json"])?;
                    let specs = planner::delivery_review_nodes(
                        &loaded.policy,
                        &tags,
                        generation,
                        Some(&plan),
                    )?;
                    if run["state_schema_version"].py_eq(&7_i64.into()) {
                        let binding = bind_review_source(
                            session,
                            run,
                            &join,
                            source,
                            loaded,
                            context,
                            claims,
                            &specs,
                            &inputs,
                            output_plan.as_ref(),
                        )?;
                        inputs.push(binding);
                    }
                    ("delivery", specs)
                } else {
                    (
                        "design",
                        planner::design_review_nodes(&loaded.policy, &tags, generation)?,
                    )
                };
                let collection = planner::collection_join(stage, generation, specs);
                let topology = insert_collection_successors(
                    session,
                    run,
                    loaded,
                    context,
                    &collection,
                    &inputs,
                    source_files,
                )?;
                return Ok(advanced(&join_id, Value::Null, topology));
            }
            let collection = collection_document(&join_id, frozen.clone());
            let artifact = evidence::canonical_ledger_artifact(
                join_id.as_string().expect("validated join ID"),
                &"collection".into(),
                &collection,
            )?;
            evidence::enforce_artifact_size(&artifact.kind, &artifact.size_bytes, &loaded.policy)?;
            seal_join(session, &join_id, &frozen)?;
            persist_artifact(session, run_id, &artifact)?;
            let stage = if delivery_collection {
                "delivery"
            } else {
                "design"
            };
            let spec = if research {
                planner::revised_design_node(&loaded.policy, generation)?
            } else {
                planner::consolidation_node(&loaded.policy, stage, generation)?
            };
            let context_nodes: &[&str] = if delivery_collection {
                &["tech_lead", "senior_engineer"]
            } else {
                &[]
            };
            let mut inputs = result_inputs(
                session,
                run,
                if research { &members } else { &[] },
                context_nodes,
            )?;
            let mut collection_input = artifact.as_input();
            collection_input.set("content", collection);
            inputs.push(collection_input);
            if delivery_collection {
                if run["state_schema_version"].py_eq(&7_i64.into()) {
                    if let Some((binding, _)) = current_review_binding(session, run)? {
                        inputs.push(Value::object([
                            ("kind", binding["kind"].clone()),
                            ("ref", binding["ref"].clone()),
                            ("sha256", binding["sha256"].clone()),
                            ("size_bytes", binding["size_bytes"].clone()),
                        ]));
                    }
                }
                for nested in delivery_nested_collections(session, &members, &join["generation"])? {
                    inputs.push(Value::object([
                        ("kind", nested["kind"].clone()),
                        ("ref", nested["collection_ref"].clone()),
                        ("sha256", nested["collection_digest"].clone()),
                        ("size_bytes", nested["size_bytes"].clone()),
                        ("content", compat::loads_value(&nested["content_json"])?),
                    ]));
                }
            }
            let node = insert_spec(
                session,
                run,
                loaded,
                context,
                &spec,
                &inputs,
                "ready",
                source_files,
            )?;
            let mut topology = SuccessorTopology {
                branches: vec![node["branch_id"].clone()],
                ..SuccessorTopology::default()
            };
            let outcome = if research {
                "RESEARCH_SEALED".into()
            } else {
                let spec = planner::consolidation_join(stage, generation, spec);
                topology.joins.push(insert_join_spec(session, run, &spec)?);
                Value::Null
            };
            Ok(advanced(&join_id, outcome, topology))
        },
        mutation_task_validator(context, TaskValidationMutation::JoinAdvance),
        context.store.fault,
    )
}
