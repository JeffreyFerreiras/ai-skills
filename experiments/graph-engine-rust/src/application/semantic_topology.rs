use super::{
    error::{EngineError, EngineResult},
    graph_design_evidence::design_evidence_artifacts,
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    join_validation::join_members,
    semantic_route::sealed_consolidation_outcomes,
    topology_history::operation_responses,
};
use crate::{
    compat::{self, Value},
    domain::{
        contract_values::{contains, get},
        errors::{self, string_item},
        execution, ids, planner, python_operations,
    },
};

fn invalid(code: &str) -> EngineError {
    EngineError::state(code)
}

fn insert(set: &mut Vec<Value>, value: &Value) -> EngineResult<()> {
    errors::require_hashable(value)?;
    if !errors::set_contains(set, value) {
        set.push(value.clone());
    }
    Ok(())
}

fn same_set(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len() && left.iter().all(|value| errors::set_contains(right, value))
}

fn selected_ids(
    ledger: &dyn GraphLedgerReadPort,
    query: GraphRead,
    run: &Value,
    key: &str,
) -> EngineResult<Vec<Value>> {
    let mut values = Vec::new();
    ledger.visit(query, &[string_item(run, "run_id")?.clone()], &mut |row| {
        insert(&mut values, string_item(&row, key)?)
    })?;
    Ok(values)
}

fn branch_identity(
    run: &Value,
    key: &str,
    stage: &str,
    generation: &Value,
    tag: &Value,
) -> EngineResult<Value> {
    Ok(ids::stable_id_with_tag_value(
        string_item(run, "run_id")?,
        string_item(run, "policy_digest")?,
        "branch",
        &format!("{key}@{stage}"),
        generation,
        tag,
    )?
    .into())
}

fn join_identity(run: &Value, key: &Value, generation: &Value) -> EngineResult<Value> {
    Ok(ids::stable_id_with_key_value(
        string_item(run, "run_id")?,
        string_item(run, "policy_digest")?,
        "join",
        key,
        generation,
        &Value::Null,
    )?
    .into())
}

fn expected_review_branches(
    policy: &Value,
    tags: &Value,
    join: &Value,
    run: &Value,
    plan: &Value,
) -> EngineResult<Vec<Value>> {
    let stage = string_item(join, "stage")?;
    let generation = string_item(join, "generation")?;
    let mut keys = if stage.is_text("design") {
        vec!["architect"]
    } else if get(plan, "code_review_panel_version")?.py_eq(&1_i64.into()) {
        let mut keys = execution::PANEL
            .iter()
            .map(|(key, _, _)| *key)
            .collect::<Vec<_>>();
        keys.push("test_engineer");
        keys
    } else {
        vec!["code_reviewer", "test_engineer"]
    };
    let selected_stage = if stage.is_text("design") {
        "design"
    } else {
        "delivery"
    };
    let mut branches = Vec::new();
    for key in keys.drain(..) {
        planner::make_node(policy, key, selected_stage, 0, true, None)?;
        branches.push(branch_identity(
            run,
            key,
            selected_stage,
            generation,
            &Value::Null,
        )?);
    }
    let mut tags = errors::iterable_values(tags)?;
    python_operations::sort_comparable_values(&mut tags)?;
    for tag in tags {
        let specialists = string_item(policy, "specialists")?;
        let specialists = specialists
            .as_object()
            .ok_or_else(|| errors::attribute_error(specialists, "get"))?;
        errors::require_hashable(&tag)?;
        let config = specialists
            .iter()
            .find(|(key, _)| tag.py_eq(&Value::String(key.clone())))
            .map(|(_, value)| value);
        if let Some(config) = config.filter(|value| value.truthy()) {
            if contains(string_item(config, "stages")?, &Value::from(selected_stage))? {
                let key = string_item(config, "node_key")?;
                string_item(config, "role")?;
                string_item(config, "mandatory")?;
                let key = compat::repr::value_str(key)?;
                let mut template = key;
                template.0.push(64);
                template.0.extend(selected_stage.chars().map(u32::from));
                branches.push(
                    ids::stable_id_with_key_value(
                        string_item(run, "run_id")?,
                        string_item(run, "policy_digest")?,
                        "branch",
                        &Value::String(template),
                        generation,
                        &tag,
                    )?
                    .into(),
                );
            }
        }
    }
    Ok(branches)
}

fn ledger_reference(member: &Value) -> EngineResult<Value> {
    let prefix =
        python_operations::add(&Value::from("ledger:"), string_item(member, "branch_id")?)?;
    let prefix = python_operations::add(&prefix, &Value::from("#sha256="))?;
    Ok(python_operations::add(
        &prefix,
        string_item(member, "result_digest")?,
    )?)
}

fn validate_direct_design_inputs(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    lead_id: &Value,
    source: &Value,
    prior_specs: &[planner::NodeSpec],
) -> EngineResult<()> {
    let lead = ledger.first(GraphRead::NodeEnvelope, std::slice::from_ref(lead_id))?
        .ok_or_else(|| invalid("TOPOLOGY_NODE_INVALID"))?;
    let envelope = compat::loads_value(string_item(&lead, "envelope_json")?)?;
    let inputs = errors::iterable_values(string_item(&envelope, "inputs")?)?;
    let pages = design_evidence_artifacts(ledger, run, lead_id, source)?;
    let mut tasks = inputs.iter().filter(|input| input["kind"].is_text("task_brief"));
    let has_task = tasks.next().is_some_and(|input| input["ref"].py_eq(&run["task_ref"])
        && input["sha256"].py_eq(&run["task_digest"])) && tasks.next().is_none();
    let has_source = inputs.iter().any(|input| input["kind"].is_text("branch_result")
        && input["ref"].py_eq(&pages[0].1["source_ref"]));
    let expected = pages.into_iter().map(|(artifact, page)| {
        let mut required = artifact.as_input();
        required.set("content", page);
        ids::canonical_bytes(&required)
    }).collect::<Result<Vec<_>, _>>()?;
    let actual = inputs.iter().filter(|input| input["kind"].is_text("design_evidence"))
        .map(ids::canonical_bytes).collect::<Result<Vec<_>, _>>()?;
    if !has_task || !has_source || actual != expected {
        return Err(invalid("TOPOLOGY_EDGE_INVALID"));
    }
    let mut prior_documents = Vec::new();
    for spec in prior_specs {
        let id = branch_identity(run, &spec.key, &spec.stage, &spec.generation.into(), &Value::Null)?;
        let producer = ledger.first(GraphRead::Node, &[id])?
            .filter(|node| node["run_id"].py_eq(&run["run_id"]) && node["status"].is_text("succeeded"))
            .ok_or_else(|| invalid("TOPOLOGY_EDGE_INVALID"))?;
        let reference = ledger_reference(&producer)?;
        if !inputs.iter().any(|item| item["kind"].is_text("branch_result")
            && item["ref"].py_eq(&reference) && item["sha256"].py_eq(&producer["result_digest"]))
        {
            return Err(invalid("TOPOLOGY_EDGE_INVALID"));
        }
        let result = compat::loads_value(&producer["result_json"])?;
        if !result["artifact_ref"].is_null() {
            prior_documents.push(ids::canonical_bytes(&result["artifact_ref"])?);
        }
    }
    let mut documents = inputs.iter().filter(|item| {
        ["technical_design", "implementation_handoff"].iter().any(|kind| item["kind"].is_text(kind))
    }).map(|item| {
        ids::canonical_bytes(&Value::object(["kind", "ref", "sha256"].map(|key| (key, item[key].clone()))))
    }).collect::<Result<Vec<_>, _>>()?;
    prior_documents.sort_unstable();
    documents.sort_unstable();
    if documents != prior_documents {
        return Err(invalid("TOPOLOGY_EDGE_INVALID"));
    }
    Ok(())
}

fn validate_design_start_history(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    policy: &Value,
    responses: &[Value],
    mapper: &Value,
    initial: &[planner::NodeSpec],
    legacy_research: bool,
) -> EngineResult<()> {
    let run_id = string_item(run, "run_id")?;
    let mut starters = Vec::new();
    ledger.visit(GraphRead::NodesForStatus, &[run_id.clone()], &mut |node| {
        let design_start = if legacy_research {
            string_item(&node, "stage")?.is_text("research")
        } else {
            string_item(&node, "node_key")?.is_text("tech_lead")
        };
        if design_start {
            starters.push(string_item(&node, "branch_id")?.clone());
        }
        Ok(())
    })?;
    let mut transitions = Vec::new();
    for (stage, next) in [("design", "REVISE"), ("delivery", "REDESIGN")] {
        for (generation, outcome, _) in sealed_consolidation_outcomes(ledger, run_id, stage)? {
            if outcome.is_text(next) {
                let join = join_identity(run, &format!("{stage}_consolidation").into(), &generation)?;
                transitions.push((join, generation, next));
            }
        }
    }
    let budget = ledger
        .first(GraphRead::BudgetForUse, &[run_id.clone(), "design_revisions".into()])?
        .ok_or_else(|| invalid("BUDGET_STATE_INVALID"))?;
    let mut mapper_recorded = false;
    let mut expected_starters = Vec::new();
    let mut generation = 0_i64;
    let mut exhausted = false;
    let empty = Value::List(vec![]);
    for response in responses {
        let code = get(response, "code")?;
        let successors = response.get("successor_branch_ids").unwrap_or(&empty);
        let mut consolidation = None;
        let mut prior_specs = Vec::new();
        let specs = if code.is_text("BRANCH_RESULT_RECORDED")
            && get(response, "branch_id")?.py_eq(string_item(mapper, "branch_id")?)
            && get(response, "branch_status")?.is_text("succeeded")
        {
            if mapper_recorded {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            }
            mapper_recorded = true;
            initial.to_vec()
        } else if let Some(index) = transitions.iter().position(|(id, _, _)| {
            code.is_text("JOIN_ADVANCED") && response["join_id"].py_eq(id)
        }) {
            let (_, source_generation, next) = transitions.swap_remove(index);
            if !mapper_recorded || exhausted || (next == "REVISE" && !source_generation.py_eq(&generation.into())) {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            }
            let outcome = get(response, "outcome")?;
            if outcome.is_text("BLOCK") && budget["used"].py_eq(&budget["limit_value"]) {
                exhausted = true;
                Vec::new()
            } else if outcome.is_text(next) {
                if !legacy_research {
                    consolidation = Some(join_members(ledger, &response["join_id"])?
                        .into_iter().next().ok_or_else(|| invalid("TOPOLOGY_HISTORY_INVALID"))?);
                    // Resolve the producers that existed at this gate, even after later work succeeds.
                    if initial[0].key == "tech_lead" || generation > 0 {
                        prior_specs.push(planner::revised_design_node(policy, generation)?);
                    }
                    if next == "REDESIGN" {
                        let source = source_generation.as_i64()
                            .ok_or_else(|| invalid("TOPOLOGY_HISTORY_INVALID"))?;
                        prior_specs.push(planner::implementation_node(policy, source)?);
                    }
                }
                generation += 1;
                planner::design_generation_nodes(policy, generation, legacy_research)?
            } else {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            }
        } else {
            if successors.as_list().unwrap().iter().any(|id| errors::set_contains(&starters, id)) {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            }
            continue;
        };
        let mut expected = specs.iter().map(|spec| {
            branch_identity(run, &spec.key, &spec.stage, &spec.generation.into(), &Value::Null)
        }).collect::<Result<Vec<_>, _>>()?;
        python_operations::sort_comparable_values(&mut expected)?;
        let mut joins = Vec::new();
        let mut fanouts = Vec::new();
        if specs.first().is_some_and(|spec| spec.stage == "research") {
            joins.push(join_identity(run, &"research_collection".into(), &generation.into())?);
            fanouts.push(ids::stable_id_with_tag_value(
                run_id, string_item(run, "policy_digest")?, "fanout", "research",
                &generation.into(), &Value::Null,
            )?.into());
        }
        if specs.first().is_some_and(|spec| spec.stage == "research" || spec.key == "tech_lead") {
            expected_starters.extend(expected.iter().cloned());
        }
        if specs.first().is_some_and(|spec| spec.key == "tech_lead") {
            validate_direct_design_inputs(
                ledger, run, &expected[0], consolidation.as_ref().unwrap_or(mapper), &prior_specs,
            )?;
        }
        if !successors.py_eq(&Value::List(expected))
            || !response.get("created_join_ids").unwrap_or(&empty).py_eq(&Value::List(joins))
            || !response.get("created_fanout_ids").unwrap_or(&empty).py_eq(&Value::List(fanouts))
        {
            return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
        }
    }
    if !mapper_recorded || !transitions.is_empty() {
        return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
    }
    if !same_set(&starters, &expected_starters) {
        return Err(invalid("TOPOLOGY_NODE_INVALID"));
    }
    Ok(())
}

pub fn validate_reconstructed_graph(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
) -> EngineResult<()> {
    let run_id = string_item(run, "run_id")?;
    let plan = ledger
        .first(GraphRead::ExecutionPlanBody, &[run_id.clone()])?
        .unwrap_or(Value::Null);
    let plan = compat::loads_value(string_item(&plan, "plan_json")?)?;
    let legacy_research = plan.get("design_workflow_version").is_none();
    let mut expected_nodes = Vec::new();
    let mut expected_joins = Vec::new();
    let mut expected_fanouts = Vec::new();
    let mut advanced: Vec<(Value, Value)> = Vec::new();
    let responses = operation_responses(ledger, run_id)?;
    for response in &responses {
        let branch = get(response, "branch")?;
        if branch.as_object().is_some() {
            let identifier = get(branch, "branch_id")?;
            if identifier.as_string().is_some() {
                insert(&mut expected_nodes, identifier)?;
            }
        }
        for key in [
            "successor_branch_ids",
            "child_branch_ids",
            "created_join_ids",
            "created_fanout_ids",
        ] {
            get(response, key)?;
            let empty = Value::List(Vec::new());
            let values = response.get(key).unwrap_or(&empty);
            let Some(values) = values.as_list() else {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            };
            if values.iter().any(|value| value.as_string().is_none()) {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            }
            let target = match key {
                "created_join_ids" => &mut expected_joins,
                "created_fanout_ids" => &mut expected_fanouts,
                _ => &mut expected_nodes,
            };
            for value in values {
                insert(target, value)?;
            }
        }
        if get(response, "code")?.is_text("JOIN_ADVANCED") {
            let identifier = get(response, "join_id")?;
            if identifier.as_string().is_none()
                || advanced.iter().any(|(key, _)| key.py_eq(identifier))
            {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            }
            advanced.push((identifier.clone(), response.clone()));
        }
    }
    let actual_nodes = selected_ids(ledger, GraphRead::TopologyNodeIds, run, "branch_id")?;
    let actual_joins = selected_ids(ledger, GraphRead::TopologyJoinIds, run, "join_id")?;
    let actual_fanouts = selected_ids(ledger, GraphRead::TopologyFanoutIds, run, "fanout_id")?;
    let policy = compat::loads_value(string_item(run, "policy_json")?)?;
    let mut research_successors: Vec<(Value, Value)> = Vec::new();
    ledger.visit(
        GraphRead::ResearchJoinGenerations,
        &[run_id.clone()],
        &mut |join| {
            planner::revised_design_node(&policy, 0)?;
            let branch = branch_identity(
                run,
                "tech_lead",
                "design",
                string_item(&join, "generation")?,
                &Value::Null,
            )?;
            let identifier = string_item(&join, "join_id")?;
            errors::require_hashable(identifier)?;
            if let Some((_, previous)) = research_successors
                .iter_mut()
                .find(|(key, _)| key.container_eq(identifier))
            {
                *previous = branch;
            } else {
                research_successors.push((identifier.clone(), branch));
            }
            Ok(())
        },
    )?;
    if !legacy_research && !research_successors.is_empty() {
        return Err(invalid("TOPOLOGY_STATE_INVALID"));
    }
    for response in &responses {
        let empty = Value::List(Vec::new());
        get(response, "successor_branch_ids")?;
        let successors = response.get("successor_branch_ids").unwrap_or(&empty);
        let response_join = get(response, "join_id")?;
        for (identifier, successor) in &research_successors {
            if contains(successors, successor)?
                && (!get(response, "code")?.is_text("JOIN_ADVANCED")
                    || !response_join.py_eq(identifier))
            {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            }
        }
    }
    if let Some(mapper) = ledger.first(GraphRead::ImpactMapper, &[run_id.clone()])? {
        if string_item(&mapper, "status")?.is_text("succeeded") {
            let result = (|| -> EngineResult<()> {
                let impact = compat::loads_value(string_item(&mapper, "result_json")?)?;
                let route = string_item(&impact, "route_label")?;
                errors::require_hashable(route)?;
                let specs = if ["design_only", "full_delivery"].iter().any(|name| route.is_text(name)) {
                    planner::design_generation_nodes(&policy, 0, legacy_research)?
                } else {
                    planner::initial_route_nodes(&policy, &route.text().unwrap_or_default())?
                };
                for spec in &specs {
                    insert(
                        &mut expected_nodes,
                        &branch_identity(
                            run,
                            &spec.key,
                            &spec.stage,
                            &Value::from(spec.generation),
                            &Value::from(spec.specialist_tag.as_deref().unwrap_or("")),
                        )?,
                    )?;
                }
                validate_design_start_history(ledger, run, &policy, &responses, &mapper, &specs, legacy_research)?;
                Ok(())
            })();
            match result {
                Err(EngineError::Python {
                    kind: "TypeError" | "KeyError",
                    ..
                }) => return Err(invalid("TOPOLOGY_HISTORY_INVALID")),
                Err(EngineError::Json(error))
                    if error.kind == "JSONDecodeError" || error.kind == "TypeError" =>
                {
                    return Err(invalid("TOPOLOGY_HISTORY_INVALID"))
                }
                other => other?,
            }
        }
    }
    for (identifier, successor) in &research_successors {
        if let Some(row) = ledger.first(GraphRead::JoinStatus, &[identifier.clone()])? {
            if string_item(&row, "status")?.is_text("sealed") {
                insert(&mut expected_nodes, successor)?;
            }
        }
    }
    ledger.visit(GraphRead::SucceededNodes, &[run_id.clone()], &mut |node| {
        let key = string_item(&node, "node_key")?;
        let required = if key.is_text("tech_lead") {
            Some("design_inputs")
        } else if key.is_text("advisory_reviewer") {
            Some("closure")
        } else if key.is_text("senior_engineer") {
            let result = compat::loads_value(string_item(&node, "result_json")?)?;
            Some(if get(&result, "decision")?.is_text("REDESIGN_REQUIRED") {
                "delivery_collection"
            } else {
                "implementation"
            })
        } else {
            None
        };
        if let Some(key) = required {
            insert(
                &mut expected_joins,
                &join_identity(run, &Value::from(key), string_item(&node, "generation")?)?,
            )?;
        }
        Ok(())
    })?;
    let raw_tags = string_item(run, "selected_tags_json")?;
    let empty_tags = Value::from("[]");
    let tags = compat::loads_value(if raw_tags.truthy() {
        raw_tags
    } else {
        &empty_tags
    })?;
    ledger.visit(GraphRead::SealedJoins, &[run_id.clone()], &mut |join| {
        if string_item(&join, "kind")?.is_text("dependency") {
            for identifier in expected_review_branches(&policy, &tags, &join, run, &plan)? {
                insert(&mut expected_nodes, &identifier)?;
            }
            let key =
                python_operations::add(string_item(&join, "stage")?, &Value::from("_collection"))?;
            insert(
                &mut expected_joins,
                &join_identity(run, &key, string_item(&join, "generation")?)?,
            )?;
        }
        Ok(())
    })?;
    if !same_set(&actual_nodes, &expected_nodes) {
        return Err(invalid("TOPOLOGY_NODE_INVALID"));
    }
    if !same_set(&actual_joins, &expected_joins) {
        return Err(invalid("TOPOLOGY_JOIN_INVALID"));
    }
    if !same_set(&actual_fanouts, &expected_fanouts) {
        return Err(invalid("TOPOLOGY_EDGE_INVALID"));
    }
    ledger.visit(
        GraphRead::JoinsForValidation,
        &[run_id.clone()],
        &mut |join| {
            let identifier = string_item(&join, "join_id")?;
            errors::require_hashable(identifier)?;
            let witness = advanced
                .iter()
                .find(|(key, _)| key.container_eq(identifier))
                .map(|(_, value)| value);
            if string_item(&join, "status")?.is_text("sealed")
                && !string_item(&join, "kind")?.is_text("closure")
                && witness.is_none()
            {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            }
            let Some(witness) = witness else {
                return Ok(());
            };
            if !string_item(&join, "status")?.is_text("sealed") {
                return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
            }
            let empty = Value::List(Vec::new());
            get(witness, "successor_branch_ids")?;
            let successors = witness.get("successor_branch_ids").unwrap_or(&empty);
            if string_item(&join, "join_key")?.is_text("research_collection") {
                let expected = research_successors
                    .iter()
                    .find(|(key, _)| key.container_eq(identifier))
                    .map(|(_, value)| value)
                    .unwrap_or(&compat::value::NULL);
                if !successors.py_eq(&Value::list([expected.clone()])) {
                    return Err(invalid("TOPOLOGY_HISTORY_INVALID"));
                }
            }
            let members = join_members(ledger, identifier)?;
            let mut references = Vec::new();
            if string_item(&join, "kind")?.is_text("dependency")
                || string_item(&join, "kind")?.is_text("consolidation")
            {
                for member in &members {
                    insert(&mut references, &ledger_reference(member)?)?;
                }
            } else if string_item(&join, "kind")?.is_text("collection") {
                let prefix = python_operations::add(&Value::from("ledger:"), identifier)?;
                let pattern = python_operations::add(&prefix, &Value::from("#sha256=%"))?;
                let artifacts = ledger.all(
                    GraphRead::CollectionArtifactRefs,
                    &[run_id.clone(), pattern],
                )?;
                for artifact in artifacts {
                    insert(&mut references, string_item(&artifact, "ref")?)?;
                }
            }
            for successor in errors::iterable_values(successors)? {
                let successor = ledger
                    .first(GraphRead::NodeEnvelope, &[successor])?
                    .ok_or_else(|| invalid("TOPOLOGY_NODE_INVALID"))?;
                let envelope = compat::loads_value(string_item(&successor, "envelope_json")?)?;
                let mut inputs = Vec::new();
                for input in errors::iterable_values(string_item(&envelope, "inputs")?)? {
                    insert(&mut inputs, string_item(&input, "ref")?)?;
                }
                if !references.is_empty()
                    && references
                        .iter()
                        .any(|reference| !errors::set_contains(&inputs, reference))
                {
                    return Err(invalid("TOPOLOGY_EDGE_INVALID"));
                }
            }
            Ok(())
        },
    )?;
    if string_item(run, "status")?.is_text("complete")
        && ledger
            .all(GraphRead::SealedClosureJoins, &[run_id.clone()])?
            .len()
            != 1
    {
        return Err(invalid("TOPOLOGY_JOIN_INVALID"));
    }
    Ok(())
}
