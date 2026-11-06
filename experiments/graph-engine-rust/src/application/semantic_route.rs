use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    join_validation::join_members,
};
use crate::{
    compat::{self, Value},
    domain::{
        contract_values::{contains, get, in_strings},
        errors::{self, string_item},
        execution, python_operations,
        task_contract::validate_impact_map,
    },
};
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};

fn invalid(code: &str) -> EngineError {
    EngineError::state(code)
}

pub fn sealed_consolidation_outcomes(
    ledger: &dyn GraphLedgerReadPort,
    run_id: &Value,
    stage: &str,
) -> EngineResult<Vec<(Value, Value, Value)>> {
    let joins = ledger.all(
        GraphRead::SealedConsolidationJoins,
        &[run_id.clone(), format!("{stage}_consolidation").into()],
    )?;
    let mut outcomes = Vec::new();
    for join in joins {
        let members = join_members(ledger, string_item(&join, "join_id")?)?;
        if members.len() != 1 || !string_item(&members[0], "result_json")?.truthy() {
            return Err(invalid("TOPOLOGY_STATE_INVALID"));
        }
        let result = compat::loads_value(string_item(&members[0], "result_json")?)?;
        let outcome = get(&result, "outcome")?;
        let allowed: &[&str] = if stage == "design" {
            &["APPROVE", "REVISE", "BLOCK"]
        } else {
            &["ACCEPT", "REPAIR", "REDESIGN", "BLOCK"]
        };
        if !in_strings(outcome, allowed)? {
            return Err(invalid("TOPOLOGY_STATE_INVALID"));
        }
        outcomes.push((
            string_item(&join, "generation")?.clone(),
            outcome.clone(),
            string_item(&members[0], "branch_id")?.clone(),
        ));
    }
    Ok(outcomes)
}

fn add_binding(bindings: &mut Vec<(Value, Value)>, key: Value, stage: &str) -> EngineResult<()> {
    errors::require_hashable(&key)?;
    let stage = Value::from(stage);
    if !bindings
        .iter()
        .any(|(a, b)| a.container_eq(&key) && b.container_eq(&stage))
    {
        bindings.push((key, stage));
    }
    Ok(())
}

fn add_specialists(
    bindings: &mut Vec<(Value, Value)>,
    policy: &Value,
    tags: &Value,
    stage: &str,
) -> EngineResult<()> {
    for tag in errors::iterable_values(tags)? {
        let specialists = string_item(policy, "specialists")?;
        if contains(specialists, &tag)? {
            let config = specialists
                .as_object()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|(key, _)| tag.py_eq(&Value::String(key.clone())))
                })
                .map(|(_, value)| value)
                .expect("selected specialist is present");
            add_binding(bindings, string_item(config, "node_key")?.clone(), stage)?;
        }
    }
    Ok(())
}

fn generations(nodes: &[&Value]) -> EngineResult<Vec<Value>> {
    let values = nodes
        .iter()
        .map(|node| string_item(node, "generation").cloned())
        .collect::<Result<Vec<_>, _>>()?;
    let mut values = errors::set_values(&values)?;
    python_operations::sort_comparable_values(&mut values)?;
    Ok(values)
}

fn range_index(value: &Value) -> EngineResult<BigInt> {
    match value {
        Value::Integer(value) => Ok(value.clone()),
        Value::Bool(value) => Ok(BigInt::from(u8::from(*value))),
        _ => Err(EngineError::python(
            "TypeError",
            format!(
                "'{}' object cannot be interpreted as an integer",
                value.python_type()
            ),
        )),
    }
}

fn contiguous(values: &[Value], first: &Value) -> EngineResult<bool> {
    let stop = python_operations::add(values.last().unwrap(), &1_i64.into())?;
    let first = range_index(first)?;
    let stop = range_index(&stop)?;
    let length = if stop > first {
        &stop - &first
    } else {
        BigInt::zero()
    };
    if length > BigInt::from(isize::MAX) {
        return Err(EngineError::python(
            "OverflowError",
            "Python int too large to convert to C ssize_t",
        ));
    }
    if length.to_usize() != Some(values.len()) {
        return Ok(false);
    }
    Ok(values
        .iter()
        .enumerate()
        .all(|(index, value)| value.py_eq(&Value::Integer(&first + BigInt::from(index)))))
}

fn maximum(values: impl IntoIterator<Item = Value>) -> EngineResult<Value> {
    let mut selected = Value::from(0_i64);
    for value in values {
        if python_operations::greater(&value, &selected)? {
            selected = value;
        }
    }
    Ok(selected)
}

pub fn validate_route_and_topology(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    task: &Value,
    policy: &Value,
) -> EngineResult<()> {
    let run_id = string_item(run, "run_id")?;
    let nodes = ledger.all(GraphRead::NodesForStatus, &[run_id.clone()])?;
    let mappers = nodes
        .iter()
        .filter(|node| node["node_key"].is_text("impact_mapper"))
        .collect::<Vec<_>>();
    if mappers.len() != 1 {
        return Err(invalid("TOPOLOGY_STATE_INVALID"));
    }
    let mapper = mappers[0];
    if !string_item(mapper, "status")?.is_text("succeeded") {
        if !string_item(run, "selected_route")?.is_null()
            || !string_item(run, "selected_tags_json")?.is_null()
        {
            return Err(invalid("ROUTE_EVIDENCE_MISMATCH"));
        }
        if nodes.len() != 1
            || ledger
                .first(GraphRead::AnyJoin, &[run_id.clone()])?
                .is_some()
        {
            return Err(invalid("TOPOLOGY_STATE_INVALID"));
        }
        if !string_item(run, "design_generation")?.py_eq(&0_i64.into())
            || !string_item(run, "implementation_generation")?.py_eq(&0_i64.into())
        {
            return Err(invalid("GENERATION_STATE_INVALID"));
        }
        return Ok(());
    }
    let classification = (|| -> EngineResult<(Value, Value)> {
        let classification = validate_impact_map(
            &compat::loads_value(string_item(mapper, "result_json")?)?,
            task,
            policy,
        )?;
        let raw = string_item(run, "selected_tags_json")?;
        let fallback = Value::from("[]");
        Ok((
            classification,
            compat::loads_value(if raw.truthy() { raw } else { &fallback })?,
        ))
    })();
    let (classification, selected_tags) = match classification {
        Err(EngineError::Contract(_)) => return Err(invalid("ROUTE_EVIDENCE_MISMATCH")),
        Err(EngineError::Json(error)) if error.kind == "JSONDecodeError" => {
            return Err(invalid("ROUTE_EVIDENCE_MISMATCH"))
        }
        other => other?,
    };
    let route = string_item(&classification, "route_label")?;
    let tags = string_item(&classification, "impact_tags")?;
    if !string_item(run, "selected_route")?.py_eq(route) || !selected_tags.py_eq(tags) {
        return Err(invalid("ROUTE_EVIDENCE_MISMATCH"));
    }
    let _design_outcomes = sealed_consolidation_outcomes(ledger, run_id, "design")?;
    let delivery_outcomes = sealed_consolidation_outcomes(ledger, run_id, "delivery")?;
    let fast = in_strings(route, &["fast_path", "delivery_only"])?;
    let design = in_strings(route, &["design_only", "full_delivery"])?;
    let skipping_redesign = fast
        && delivery_outcomes
            .iter()
            .any(|(_, outcome, _)| outcome.is_text("REDESIGN"));
    let mut bindings = vec![("impact_mapper".into(), "bootstrap".into())];
    if route.is_text("advisory") {
        add_binding(&mut bindings, "advisory_reviewer".into(), "advisory")?;
    }
    if design || skipping_redesign {
        for (key, stage) in [
            ("design_research_architecture", "research"),
            ("design_research_validation", "research"),
            ("tech_lead", "design"),
            ("architect", "design"),
            ("supervisor_design_consolidation", "design"),
        ] {
            add_binding(&mut bindings, key.into(), stage)?;
        }
        add_specialists(&mut bindings, policy, tags, "design")?;
    }
    if in_strings(route, &["fast_path", "delivery_only", "full_delivery"])? {
        for (key, stage) in [
            ("senior_engineer", "implementation"),
            ("code_reviewer", "delivery"),
            ("test_engineer", "delivery"),
            ("supervisor_delivery_consolidation", "delivery"),
        ] {
            add_binding(&mut bindings, key.into(), stage)?;
        }
        let row = ledger
            .first(GraphRead::ExecutionPlanBody, &[run_id.clone()])?
            .unwrap_or(Value::Null);
        let plan = compat::loads_value(string_item(&row, "plan_json")?)?;
        if get(&plan, "code_review_panel_version")?.py_eq(&1_i64.into()) {
            for (key, _, _) in execution::PANEL {
                add_binding(&mut bindings, (*key).into(), "delivery")?;
            }
        }
        add_specialists(&mut bindings, policy, tags, "delivery")?;
    }
    for node in &nodes {
        if string_item(node, "depth")?.py_eq(&1_i64.into()) {
            continue;
        }
        let key = string_item(node, "node_key")?;
        let stage = string_item(node, "stage")?;
        errors::require_hashable(key)?;
        errors::require_hashable(stage)?;
        if !bindings
            .iter()
            .any(|(a, b)| a.container_eq(key) && b.container_eq(stage))
        {
            return Err(invalid("TOPOLOGY_STATE_INVALID"));
        }
        let tag = string_item(node, "specialist_tag")?;
        if !tag.is_null() && !contains(tags, tag)? {
            return Err(invalid("TOPOLOGY_STATE_INVALID"));
        }
    }
    let tech = nodes
        .iter()
        .filter(|node| node["node_key"].is_text("tech_lead"))
        .collect::<Vec<_>>();
    let research = nodes
        .iter()
        .filter(|node| node["stage"].is_text("research"))
        .collect::<Vec<_>>();
    let senior = nodes
        .iter()
        .filter(|node| node["node_key"].is_text("senior_engineer"))
        .collect::<Vec<_>>();
    let tech_generations = generations(&tech)?;
    let research_generations = generations(&research)?;
    let senior_generations = generations(&senior)?;
    if design
        && !research_generations
            .first()
            .is_some_and(|value| value.py_eq(&0_i64.into()))
    {
        return Err(invalid("TOPOLOGY_STATE_INVALID"));
    }
    if skipping_redesign
        && !research_generations
            .first()
            .is_some_and(|value| value.py_eq(&1_i64.into()))
    {
        return Err(invalid("TOPOLOGY_STATE_INVALID"));
    }
    if let Some(first) = tech_generations.first() {
        if (design && !first.py_eq(&0_i64.into()))
            || (skipping_redesign && !first.py_eq(&1_i64.into()))
        {
            return Err(invalid("TOPOLOGY_STATE_INVALID"));
        }
    }
    if fast
        && !senior_generations
            .first()
            .is_some_and(|value| value.py_eq(&0_i64.into()))
    {
        return Err(invalid("TOPOLOGY_STATE_INVALID"));
    }
    if route.is_text("full_delivery")
        && senior_generations
            .first()
            .is_some_and(|value| !value.py_eq(&0_i64.into()))
    {
        return Err(invalid("TOPOLOGY_STATE_INVALID"));
    }
    if fast
        && tech_generations
            .first()
            .is_some_and(|value| !value.py_eq(&1_i64.into()))
    {
        return Err(invalid("TOPOLOGY_STATE_INVALID"));
    }
    if let Some(first) = tech_generations.first() {
        if !contiguous(&tech_generations, first)? {
            return Err(invalid("GENERATION_STATE_INVALID"));
        }
    }
    if let Some(first) = research_generations.first() {
        if !contiguous(&research_generations, first)? {
            return Err(invalid("GENERATION_STATE_INVALID"));
        }
        for generation in &research_generations {
            let row = ledger
                .first(
                    GraphRead::ResearchJoinStatus,
                    &[run_id.clone(), generation.clone()],
                )?
                .ok_or_else(|| invalid("TOPOLOGY_STATE_INVALID"))?;
            if !string_item(&row, "status")?.is_text("sealed")
                && tech.iter().any(|node| node["generation"].py_eq(generation))
            {
                return Err(invalid("TOPOLOGY_STATE_INVALID"));
            }
        }
    }
    if !tech_generations.is_empty()
        && tech_generations
            .iter()
            .any(|generation| !errors::set_contains(&research_generations, generation))
    {
        return Err(invalid("GENERATION_STATE_INVALID"));
    }
    if !senior_generations.is_empty() && !contiguous(&senior_generations, &0_i64.into())? {
        return Err(invalid("GENERATION_STATE_INVALID"));
    }
    let design_generation = maximum(
        tech_generations
            .iter()
            .chain(&research_generations)
            .cloned(),
    )?;
    if !string_item(run, "design_generation")?.py_eq(&design_generation) {
        return Err(invalid("GENERATION_STATE_INVALID"));
    }
    let mut reserved = Vec::new();
    for (generation, outcome, branch) in &delivery_outcomes {
        if !outcome.is_text("REDESIGN") {
            continue;
        }
        let prefix = python_operations::add(
            &python_operations::add(&Value::from("ledger:"), branch)?,
            &Value::from("#sha256="),
        )?;
        let mut found = false;
        for node in tech.iter().chain(&research) {
            let envelope = compat::loads_value(string_item(node, "envelope_json")?)?;
            for input in errors::iterable_values(string_item(&envelope, "inputs")?)? {
                let reference = string_item(&input, "ref")?;
                let reference = reference
                    .as_string()
                    .ok_or_else(|| errors::attribute_error(reference, "startswith"))?;
                if reference.0.starts_with(&prefix.as_string().unwrap().0) {
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }
        if found {
            reserved.push(python_operations::add(generation, &1_i64.into())?);
        }
    }
    let implementation_generation = maximum(senior_generations.iter().cloned().chain(reserved))?;
    if !string_item(run, "implementation_generation")?.py_eq(&implementation_generation) {
        return Err(invalid("GENERATION_STATE_INVALID"));
    }
    for node in &nodes {
        if string_item(node, "depth")?.py_eq(&1_i64.into()) {
            continue;
        }
        for (stage, generations) in [
            ("research", &research_generations),
            ("design", &tech_generations),
            ("delivery", &senior_generations),
        ] {
            if string_item(node, "stage")?.is_text(stage) {
                let generation = string_item(node, "generation")?;
                errors::require_hashable(generation)?;
                if !errors::set_contains(generations, generation) {
                    return Err(invalid("GENERATION_STATE_INVALID"));
                }
            }
        }
    }
    Ok(())
}
