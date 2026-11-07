use super::{
    contracts::{self, ProjectionError},
    errors::{ValidationFailure, ValidationResult},
    execution, hosts, ids, policy, DomainResult,
};
use crate::compat::{unicode, PyString, Value};
use num_bigint::BigInt;
use num_traits::Zero;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeSpec {
    pub key: String,
    pub role: String,
    pub stage: String,
    pub generation: i64,
    pub mandatory: bool,
    pub specialist_tag: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JoinSpec {
    pub key: String,
    pub kind: String,
    pub stage: String,
    pub generation: i64,
    pub members: Vec<NodeSpec>,
}
pub fn node_id(
    run: &str,
    digest: &str,
    spec: &NodeSpec,
) -> Result<String, crate::compat::JsonError> {
    ids::stable_id(
        run,
        digest,
        "node",
        &format!("{}@{}", spec.key, spec.stage),
        spec.generation,
        spec.specialist_tag.as_deref(),
    )
}
pub fn branch_id(
    run: &str,
    digest: &str,
    spec: &NodeSpec,
) -> Result<String, crate::compat::JsonError> {
    ids::stable_id(
        run,
        digest,
        "branch",
        &format!("{}@{}", spec.key, spec.stage),
        spec.generation,
        spec.specialist_tag.as_deref(),
    )
}
pub fn join_id(
    run: &str,
    digest: &str,
    spec: &JoinSpec,
) -> Result<String, crate::compat::JsonError> {
    ids::stable_id(run, digest, "join", &spec.key, spec.generation, None)
}
pub fn fanout_id(
    run: &str,
    digest: &str,
    stage: &str,
    generation: i64,
) -> Result<String, crate::compat::JsonError> {
    ids::stable_id(run, digest, "fanout", stage, generation, None)
}
fn resource_name(value: &Value, case_sensitive: bool) -> PyString {
    let mut name = value.as_string().unwrap().clone();
    for point in &mut name.0 {
        if *point == 92 {
            *point = 47;
        }
    }
    while name.ends_with("/") {
        name.0.pop();
    }
    if case_sensitive {
        name
    } else {
        unicode::casefold(&name)
    }
}
fn paths_overlap(left: &Value, right: &Value, case_sensitive: bool) -> bool {
    let left_path = resource_name(&left["path"], case_sensitive);
    let right_path = resource_name(&right["path"], case_sensitive);
    if left["scope"].is_text("exact") && right["scope"].is_text("exact") {
        return left_path == right_path;
    }
    let mut left_prefix = left_path.clone();
    left_prefix.0.push(47);
    let mut right_prefix = right_path.clone();
    right_prefix.0.push(47);
    if left["scope"].is_text("subtree")
        && (left_path == right_path || right_path.0.starts_with(&left_prefix.0))
    {
        return true;
    }
    right["scope"].is_text("subtree")
        && (left_path == right_path || left_path.0.starts_with(&right_prefix.0))
}
pub fn validate_fanout_ordering(
    members: &[Value],
    dependencies: &[Value],
    case_sensitive: bool,
) -> DomainResult<Vec<Value>> {
    let ids: Vec<_> = members
        .iter()
        .map(|m| m["branch_id"].as_string().unwrap().clone())
        .collect();
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err("FANOUT_MEMBER_INVALID".into());
    }
    let mut edges = BTreeSet::new();
    let mut normalized = Vec::new();
    let mut reach = vec![vec![false; ids.len()]; ids.len()];
    for dependency in dependencies {
        let before = dependency["before_branch_id"].as_string().unwrap();
        let after = dependency["after_branch_id"].as_string().unwrap();
        let left = ids
            .iter()
            .position(|id| id == before)
            .ok_or("FANOUT_MEMBER_INVALID")?;
        let right = ids
            .iter()
            .position(|id| id == after)
            .ok_or("FANOUT_MEMBER_INVALID")?;
        if left == right || !edges.insert((left, right)) {
            return Err("FANOUT_DEPENDENCY_INVALID".into());
        }
        reach[left][right] = true;
        normalized.push(Value::object([
            ("before_branch_id", Value::String(before.clone())),
            ("after_branch_id", Value::String(after.clone())),
            ("reason", dependency["reason"].clone()),
        ]));
    }
    for pivot in 0..ids.len() {
        for before in 0..ids.len() {
            if reach[before][pivot] {
                for after in 0..ids.len() {
                    reach[before][after] |= reach[pivot][after];
                }
            }
        }
    }
    if (0..ids.len()).any(|index| reach[index][index]) {
        return Err("FANOUT_CYCLE".into());
    }
    let mut capacities: BTreeMap<PyString, BigInt> = BTreeMap::new();
    let mut units: BTreeMap<PyString, BTreeMap<usize, BigInt>> = BTreeMap::new();
    for (index, member) in members.iter().enumerate() {
        for service in member["resources"]["services"].as_list().unwrap() {
            let name = resource_name(&service["ref"], case_sensitive);
            let capacity = service["capacity"].strict_integer().unwrap();
            if capacities
                .get(&name)
                .is_some_and(|stored| stored != capacity)
            {
                return Err("FANOUT_CAPACITY_INVALID".into());
            }
            capacities.insert(name.clone(), capacity.clone());
            units
                .entry(name)
                .or_default()
                .insert(index, service["units"].strict_integer().unwrap().clone());
        }
    }
    for left in 0..members.len() {
        for right in left + 1..members.len() {
            let a = &members[left]["resources"];
            let b = &members[right]["resources"];
            let path = a["writable_paths"].as_list().unwrap().iter().any(|x| {
                b["writable_paths"]
                    .as_list()
                    .unwrap()
                    .iter()
                    .any(|y| paths_overlap(x, y, case_sensitive))
            });
            let overlap = |field: &str| {
                let first: BTreeSet<_> = a[field]
                    .as_list()
                    .unwrap()
                    .iter()
                    .map(|v| resource_name(v, case_sensitive))
                    .collect();
                b[field]
                    .as_list()
                    .unwrap()
                    .iter()
                    .any(|v| first.contains(&resource_name(v, case_sensitive)))
            };
            if (path || overlap("mutable_state_refs") || overlap("exclusive_device_refs"))
                && !reach[left][right]
                && !reach[right][left]
            {
                return Err("FANOUT_UNORDERED_CONFLICT".into());
            }
        }
    }
    let mut subsets = vec![(0_usize, Vec::<usize>::new())];
    while let Some((next, subset)) = subsets.pop() {
        if next == members.len() {
            for (name, capacity) in &capacities {
                let sum = subset.iter().fold(BigInt::zero(), |sum, index| {
                    sum + units
                        .get(name)
                        .and_then(|members| members.get(index))
                        .cloned()
                        .unwrap_or_default()
                });
                if &sum > capacity {
                    return Err("FANOUT_CAPACITY_EXCEEDED".into());
                }
            }
            continue;
        }
        subsets.push((next + 1, subset.clone()));
        if !subset
            .iter()
            .any(|&index| reach[index][next] || reach[next][index])
        {
            let mut included = subset;
            included.push(next);
            subsets.push((next + 1, included));
        }
    }
    normalized.sort_by(|a, b| {
        ["before_branch_id", "after_branch_id", "reason"]
            .into_iter()
            .map(|key| a[key].as_string().cmp(&b[key].as_string()))
            .find(|order| !order.is_eq())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(normalized)
}
pub fn node_template<'a>(policy: &'a Value, key: &str) -> ValidationResult<&'a Value> {
    let key = if execution::panel(key).is_some() {
        "code_reviewer"
    } else {
        key
    };
    policy["node_templates"]
        .get(key)
        .ok_or_else(|| ValidationFailure::python("KeyError", format!("'{key}'")))
}
pub fn make_node(
    policy: &Value,
    key: &str,
    stage: &str,
    generation: i64,
    mandatory: bool,
    specialist_tag: Option<&str>,
) -> ValidationResult<NodeSpec> {
    let template = node_template(policy, key)?;
    Ok(NodeSpec {
        key: key.into(),
        role: template["role"].text().unwrap(),
        stage: stage.into(),
        generation,
        mandatory,
        specialist_tag: specialist_tag.map(str::to_owned),
    })
}
pub fn selected_specialists(
    policy: &Value,
    tags: &[String],
    stage: &str,
    generation: i64,
) -> Vec<NodeSpec> {
    let mut tags = tags.to_vec();
    tags.sort();
    let mut nodes = Vec::new();
    for tag in tags {
        if let Some(config) = policy["specialists"].get(&tag).filter(|v| v.truthy()) {
            if config["stages"]
                .as_list()
                .unwrap()
                .iter()
                .any(|v| v.is_text(stage))
            {
                nodes.push(NodeSpec {
                    key: config["node_key"].text().unwrap(),
                    role: config["role"].text().unwrap(),
                    stage: stage.into(),
                    generation,
                    mandatory: config["mandatory"].truthy(),
                    specialist_tag: Some(tag),
                });
            }
        }
    }
    nodes
}
pub fn bootstrap(policy: &Value) -> ValidationResult<NodeSpec> {
    make_node(policy, "impact_mapper", "bootstrap", 0, true, None)
}
pub fn design_research_nodes(policy: &Value, generation: i64) -> ValidationResult<Vec<NodeSpec>> {
    policy::RESEARCH
        .iter()
        .map(|(key, _)| make_node(policy, key, "research", generation, true, None))
        .collect()
}
pub fn initial_route_nodes(policy: &Value, route: &str) -> ValidationResult<Vec<NodeSpec>> {
    match route {
        "design_only" | "full_delivery" => design_research_nodes(policy, 0),
        "fast_path" | "delivery_only" => Ok(vec![implementation_node(policy, 0)?]),
        _ => Ok(vec![make_node(
            policy,
            "advisory_reviewer",
            "advisory",
            0,
            true,
            None,
        )?]),
    }
}
pub fn as_spec(row: &Value) -> NodeSpec {
    NodeSpec {
        key: row["node_key"].text().unwrap(),
        role: row["role"].text().unwrap(),
        stage: row["stage"].text().unwrap(),
        generation: row["generation"].as_i64().unwrap(),
        mandatory: row["mandatory"].truthy(),
        specialist_tag: row["specialist_tag"].text(),
    }
}
pub fn next_join_for_success(node: &Value) -> Option<JoinSpec> {
    let (key, kind, stage) = match node["node_key"].text().as_deref() {
        Some("tech_lead") => ("design_inputs", "dependency", "design"),
        Some("senior_engineer") => ("implementation", "dependency", "delivery"),
        Some("advisory_reviewer") => ("closure", "closure", "closure"),
        _ => return None,
    };
    Some(JoinSpec {
        key: key.into(),
        kind: kind.into(),
        stage: stage.into(),
        generation: node["generation"].as_i64().unwrap(),
        members: vec![as_spec(node)],
    })
}
pub fn design_review_nodes(
    policy: &Value,
    tags: &[String],
    generation: i64,
) -> ValidationResult<Vec<NodeSpec>> {
    let mut result = vec![make_node(
        policy,
        "architect",
        "design",
        generation,
        true,
        None,
    )?];
    result.extend(selected_specialists(policy, tags, "design", generation));
    Ok(result)
}
pub fn delivery_review_nodes(
    policy: &Value,
    tags: &[String],
    generation: i64,
    plan: Option<&Value>,
) -> ValidationResult<Vec<NodeSpec>> {
    let keys = if plan.is_none()
        || plan.is_some_and(|p| p["code_review_panel_version"].py_eq(&1_i64.into()))
    {
        execution::PANEL
            .iter()
            .map(|(key, _, _)| *key)
            .collect::<Vec<_>>()
    } else {
        vec!["code_reviewer"]
    };
    let mut result = keys
        .into_iter()
        .map(|key| make_node(policy, key, "delivery", generation, true, None))
        .collect::<ValidationResult<Vec<_>>>()?;
    result.push(make_node(
        policy,
        "test_engineer",
        "delivery",
        generation,
        true,
        None,
    )?);
    result.extend(selected_specialists(policy, tags, "delivery", generation));
    Ok(result)
}
pub fn consolidation_node(
    policy: &Value,
    stage: &str,
    generation: i64,
) -> ValidationResult<NodeSpec> {
    make_node(
        policy,
        if stage == "design" {
            "supervisor_design_consolidation"
        } else {
            "supervisor_delivery_consolidation"
        },
        stage,
        generation,
        true,
        None,
    )
}
pub fn collection_join(stage: &str, generation: i64, members: Vec<NodeSpec>) -> JoinSpec {
    JoinSpec {
        key: format!("{stage}_collection"),
        kind: "collection".into(),
        stage: stage.into(),
        generation,
        members,
    }
}
pub fn consolidation_join(stage: &str, generation: i64, member: NodeSpec) -> JoinSpec {
    JoinSpec {
        key: format!("{stage}_consolidation"),
        kind: "consolidation".into(),
        stage: stage.into(),
        generation,
        members: vec![member],
    }
}
pub fn implementation_node(policy: &Value, generation: i64) -> ValidationResult<NodeSpec> {
    make_node(
        policy,
        "senior_engineer",
        "implementation",
        generation,
        true,
        None,
    )
}
pub fn revised_design_node(policy: &Value, generation: i64) -> ValidationResult<NodeSpec> {
    make_node(policy, "tech_lead", "design", generation, true, None)
}
pub fn closure_join(member: NodeSpec, generation: i64) -> JoinSpec {
    JoinSpec {
        key: "closure".into(),
        kind: "closure".into(),
        stage: "closure".into(),
        generation,
        members: vec![member],
    }
}
pub fn effective_role_capabilities(
    policy: &Value,
    task: &Value,
    role: &str,
    version: Option<&Value>,
    windows: bool,
) -> ValidationResult<Vec<Value>> {
    let mut configured = Vec::new();
    for capability in policy["role_capabilities"][role].as_list().unwrap_or(&[]) {
        if super::policy_validation::role_capability_allowed(
            policy,
            &Value::from(role),
            capability,
            windows,
        )? {
            configured.push(capability);
        }
    }
    let mut result = Vec::new();
    for cap in task["authority"]["capabilities"].as_list().unwrap() {
        let mut allowed = false;
        for ceiling in &configured {
            let within = contracts::projected_capability_within(cap, ceiling, version, windows)
                .map_err(|error| match error {
                    ProjectionError::Validation(error) => error,
                    ProjectionError::InvalidVersion => ValidationFailure::python(
                        "ValueError",
                        "CAPABILITY_PROJECTION_VERSION_INVALID",
                    ),
                })?;
            if within {
                allowed = true;
                break;
            }
        }
        if allowed {
            result.push(cap.clone());
        }
    }
    result.sort_by(|a, b| {
        ["effect", "action", "target_ref"]
            .into_iter()
            .map(|key| a[key].as_string().cmp(&b[key].as_string()))
            .find(|order| !order.is_eq())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(result)
}
pub fn envelope(
    run: &str,
    digest: &str,
    policy: &Value,
    task: &Value,
    spec: &NodeSpec,
    status: &str,
    inputs: &[Value],
    retry_count: i64,
    plan: Option<&Value>,
    windows: bool,
) -> ValidationResult<Value> {
    let template = node_template(policy, &spec.key)?;
    let default_plan;
    if plan.is_none_or(|p| !p.truthy()) {
        default_plan = execution::build_execution_plan(run, task, None, hosts::DEFAULT_HOST)
            .map_err(|message| ValidationFailure::python("ValueError", message))?;
    } else {
        default_plan = Value::Null;
    }
    let plan = plan.filter(|p| p.truthy()).unwrap_or(&default_plan);
    let assignment = execution::assignment_for(plan, &spec.key)
        .map_err(|message| ValidationFailure::python("ValueError", message))?;
    let capabilities = effective_role_capabilities(
        policy,
        task,
        &spec.role,
        plan.get("capability_projection_version"),
        windows,
    )?
    .into_iter()
    .filter(|cap| {
        (!["advisory", "research"].contains(&spec.stage.as_str())
            || ["filesystem_read", "external_read"]
                .iter()
                .any(|effect| cap["effect"].is_text(effect)))
            && (!assignment["code_review_assignment"]["focus"].is_text("naming")
                || ["filesystem_read", "external_read"]
                    .iter()
                    .any(|effect| cap["effect"].is_text(effect)))
    })
    .collect();
    let mut inputs = inputs.to_vec();
    inputs.sort_by(|a, b| {
        a["kind"]
            .as_string()
            .cmp(&b["kind"].as_string())
            .then_with(|| a["ref"].as_string().cmp(&b["ref"].as_string()))
    });
    let retries = crate::compat::integer::coerce(&template["max_retries"])?;
    let json_error =
        |error: crate::compat::JsonError| ValidationFailure::python(error.kind, error.message);
    let mut result = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", run.into()),
        (
            "branch_id",
            branch_id(run, digest, spec).map_err(json_error)?.into(),
        ),
        (
            "node_instance_id",
            node_id(run, digest, spec).map_err(json_error)?.into(),
        ),
        ("node_key", spec.key.clone().into()),
        ("role", spec.role.clone().into()),
        ("model", assignment["model"].clone()),
        ("reasoning_effort", assignment["reasoning_effort"].clone()),
        ("mandatory", spec.mandatory.into()),
        ("generation", spec.generation.into()),
        ("status", status.into()),
        ("inputs", Value::List(inputs)),
        ("effect_capabilities", Value::List(capabilities)),
        ("output_contract", template["output_contract"].clone()),
        (
            "stopping_condition",
            Value::object([
                ("kind", "valid_result_returned".into()),
                (
                    "max_branch_attempts",
                    Value::Integer(&retries + BigInt::from(1)),
                ),
            ]),
        ),
        ("artifact_ref", Value::Null),
        ("evidence", Value::List(vec![])),
        ("decision", Value::Null),
        ("retry_count", retry_count.into()),
        ("max_retries", Value::Integer(retries)),
        ("attempt_id", Value::Null),
        ("claim_digest", Value::Null),
        ("lease_expires_at", Value::Null),
        ("failure_code", Value::Null),
        ("started_at", Value::Null),
        ("finished_at", Value::Null),
    ]);
    for key in ["dispatch_runtime", "code_review_assignment"] {
        if let Some(value) = assignment.get(key) {
            result.set(key, value.clone());
        }
    }
    if spec.key == "senior_engineer" {
        if let Some(version) = plan.get("simplification_review_version") {
            result.set("simplification_review_version", version.clone());
        }
    }
    if let Some((_, focus)) = policy::RESEARCH.iter().find(|(key, _)| *key == spec.key) {
        let first = spec.key == "design_research_architecture";
        result.set(
            "research_assignment",
            Value::object([
                ("focus", (*focus).into()),
                (
                    "inspection_budget",
                    Value::Object(
                        task["inspection_budget"]
                            .as_object()
                            .ok_or_else(|| {
                                super::errors::attribute_error(&task["inspection_budget"], "items")
                            })?
                            .iter()
                            .map(|(key, value)| {
                                Ok((key.clone(), split_research_budget(value, first)?))
                            })
                            .collect::<ValidationResult<Vec<_>>>()?,
                    ),
                ),
            ]),
        );
    }
    Ok(result)
}

fn split_research_budget(value: &Value, first: bool) -> ValidationResult<Value> {
    let mut amount = crate::compat::integer::coerce(value)?;
    if first {
        amount += BigInt::from(1);
    }
    let quotient = &amount / BigInt::from(2);
    let remainder = &amount % BigInt::from(2);
    Ok(Value::Integer(
        if amount < BigInt::zero() && !remainder.is_zero() {
            quotient - BigInt::from(1)
        } else {
            quotient
        },
    ))
}
