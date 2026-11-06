use super::{hosts, ids, DomainResult};
use crate::compat::{PyString, Value};

pub const SIZES: &[&str] = &["small", "medium", "large"];
pub const CLASS_ASSIGNMENTS: &[(&str, &[(&str, &str, &str)])] = &[
    (
        "small",
        &[
            ("impact_mapper", "economy", "max"),
            ("advisory_reviewer", "economy", "max"),
            ("tech_lead", "reasoning", "medium"),
            ("architect", "reasoning", "medium"),
            ("senior_engineer", "economy", "max"),
            ("code_reviewer", "economy", "max"),
            ("test_engineer", "economy", "max"),
            ("audio_realtime_specialist", "economy", "max"),
            ("ios_platform_specialist", "economy", "max"),
            ("release_operations_reviewer", "reasoning", "medium"),
            ("security_reviewer", "reasoning", "high"),
            ("supervisor", "primary-thread", "inherited"),
        ],
    ),
    (
        "medium",
        &[
            ("impact_mapper", "economy", "max"),
            ("advisory_reviewer", "reasoning", "medium"),
            ("tech_lead", "reasoning", "medium"),
            ("architect", "reasoning", "high"),
            ("senior_engineer", "reasoning", "medium"),
            ("code_reviewer", "reasoning", "high"),
            ("test_engineer", "economy", "max"),
            ("audio_realtime_specialist", "reasoning", "high"),
            ("ios_platform_specialist", "reasoning", "high"),
            ("release_operations_reviewer", "reasoning", "high"),
            ("security_reviewer", "reasoning", "high"),
            ("supervisor", "primary-thread", "inherited"),
        ],
    ),
    (
        "large",
        &[
            ("impact_mapper", "economy", "max"),
            ("advisory_reviewer", "reasoning", "high"),
            ("tech_lead", "reasoning", "high"),
            ("architect", "reasoning", "xhigh"),
            ("senior_engineer", "reasoning", "high"),
            ("code_reviewer", "reasoning", "xhigh"),
            ("test_engineer", "reasoning", "high"),
            ("audio_realtime_specialist", "reasoning", "high"),
            ("ios_platform_specialist", "reasoning", "high"),
            ("release_operations_reviewer", "reasoning", "high"),
            ("security_reviewer", "reasoning", "xhigh"),
            ("supervisor", "primary-thread", "inherited"),
        ],
    ),
];
pub static SIZE_ASSIGNMENTS: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
    Value::Object(
        CLASS_ASSIGNMENTS
            .iter()
            .map(|(size, roles)| {
                (
                    (*size).into(),
                    Value::Object(
                        roles
                            .iter()
                            .map(|(role, class, effort)| {
                                let (model, resolved) =
                                    hosts::resolve_assignment(hosts::LEGACY_HOST, class, effort)
                                        .expect("the frozen legacy assignment matrix is valid");
                                ((*role).into(), Value::list([model.into(), resolved.into()]))
                            })
                            .collect(),
                    ),
                )
            })
            .collect(),
    )
});
pub const PANEL: &[(&str, &str, Option<&str>)] = &[
    ("code_reviewer", "clean_code", Some("clean-code-review")),
    (
        "code_reviewer_architecture",
        "clean_architecture",
        Some("clean-architecture-review"),
    ),
    ("code_reviewer_naming", "naming", None),
    (
        "code_reviewer_bug_hunter",
        "bug_hunting",
        Some("review-agent"),
    ),
];
pub const NODE_ROLES: &[(&str, &str)] = &[
    ("impact_mapper", "impact_mapper"),
    ("design_research_architecture", "impact_mapper"),
    ("design_research_validation", "impact_mapper"),
    ("advisory_reviewer", "code_reviewer"),
    ("tech_lead", "tech_lead"),
    ("architect", "software_architect"),
    ("senior_engineer", "senior_engineer"),
    ("code_reviewer", "code_reviewer"),
    ("code_reviewer_architecture", "code_reviewer"),
    ("code_reviewer_naming", "code_reviewer"),
    ("code_reviewer_bug_hunter", "code_reviewer"),
    ("test_engineer", "test_engineer"),
    ("audio_realtime_specialist", "audio_realtime_specialist"),
    ("ios_platform_specialist", "ios_platform_specialist"),
    ("release_operations_reviewer", "release_operations_reviewer"),
    ("security_reviewer", "security_reviewer"),
    ("supervisor_design_consolidation", "supervisor"),
    ("supervisor_delivery_consolidation", "supervisor"),
];
pub fn node_role(key: &str) -> Option<&'static str> {
    NODE_ROLES
        .iter()
        .find(|(node, _)| *node == key)
        .map(|(_, role)| *role)
}
pub fn panel(key: &str) -> Option<(&'static str, Option<&'static str>)> {
    PANEL
        .iter()
        .find(|(node, _, _)| *node == key)
        .map(|(_, focus, skill)| (*focus, *skill))
}
pub fn class_for_node(node: &str, size: &str) -> DomainResult<(&'static str, &'static str)> {
    if !SIZES.contains(&size) {
        return Err("invalid execution size".into());
    }
    let key = if ["advisory_reviewer", "architect", "tech_lead"].contains(&node) {
        node
    } else {
        node_role(node).ok_or("MODEL_ASSIGNMENT_INVALID")?
    };
    CLASS_ASSIGNMENTS
        .iter()
        .find(|(candidate, _)| *candidate == size)
        .and_then(|(_, roles)| roles.iter().find(|(role, _, _)| *role == key))
        .map(|(_, class, effort)| (*class, *effort))
        .ok_or_else(|| "MODEL_ASSIGNMENT_INVALID".into())
}
fn dispatch_when(node: &str, task: &Value) -> String {
    let node = if panel(node).is_some() {
        "code_reviewer"
    } else {
        node
    };
    if task["minimum_route"].is_text("delivery_only")
        && ["senior_engineer", "code_reviewer", "test_engineer"].contains(&node)
    {
        return "delivery_only or full_delivery route".into();
    }
    match node {
        "impact_mapper" => "always, before route selection",
        "design_research_architecture" | "design_research_validation" => {
            "design route, before each Tech Lead generation"
        }
        "advisory_reviewer" => "advisory route",
        "tech_lead" | "architect" => "design_only or full_delivery route",
        "senior_engineer" | "code_reviewer" | "test_engineer" => "fast_path or full_delivery route",
        "supervisor_design_consolidation" => "design consolidation",
        "supervisor_delivery_consolidation" => "delivery consolidation",
        _ => "matching impact tag at design or delivery",
    }
    .into()
}
pub fn validate_model_assignment(
    node: &str,
    model: &str,
    effort: &str,
    host: &str,
) -> DomainResult<()> {
    let class = hosts::classify(host, model)?.ok_or("MODEL_ASSIGNMENT_INVALID")?;
    if class == "primary-thread" && effort != "inherited" {
        return Err("SUPERVISOR_EFFORT_INVALID".into());
    }
    if class == "economy" && effort != hosts::economy_effort(host)? {
        return Err("ECONOMY_REASONING_EFFORT_REQUIRED".into());
    }
    if ["tech_lead", "architect"].contains(&node) && class != "reasoning" {
        return Err("DESIGN_MODEL_REQUIRED".into());
    }
    if node_role(node) == Some("impact_mapper") && class != "economy" {
        return Err("IMPACT_MAPPER_ASSIGNMENT_REQUIRED".into());
    }
    hosts::dispatch_model(host, model, effort)?;
    Ok(())
}
pub fn validate_new_plan_assignment(
    node: &str,
    model: &str,
    effort: &str,
    host: &str,
) -> DomainResult<()> {
    validate_model_assignment(node, model, effort, host)?;
    if node == "senior_engineer" && hosts::classify(host, model)? != Some("reasoning") {
        return Err("IMPLEMENTATION_REASONING_MODEL_REQUIRED".into());
    }
    Ok(())
}
pub fn validate_model_overrides(value: &Value) -> DomainResult<Value> {
    let items = value.as_object().ok_or("MODEL_OVERRIDES_INVALID")?;
    let mut result = Vec::new();
    for (key, pair) in items {
        let name = key.utf8().map_err(|_| "MODEL_OVERRIDES_INVALID")?;
        if !(node_role(&name).is_some_and(|role| role != "supervisor")
            || [
                "supervisor_recommendation",
                "publication_assignment",
                "simplification_assignment",
            ]
            .contains(&name.as_str()))
        {
            return Err("MODEL_OVERRIDES_INVALID".into());
        }
        let fields = pair.as_object().ok_or("MODEL_OVERRIDES_INVALID")?;
        if fields.len() != 2
            || pair.get("model").is_none()
            || pair.get("reasoning_effort").is_none()
            || fields.iter().any(|(_, item)| {
                item.as_string()
                    .is_none_or(|s| s.is_empty() || s.len() > 128)
            })
            || pair["model"].is_text("primary-thread")
            || pair["reasoning_effort"].is_text("inherited")
        {
            return Err("MODEL_OVERRIDES_INVALID".into());
        }
        result.push((key.clone(), pair.clone()));
    }
    result.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(Value::Object(result))
}
fn selected_pair(
    host: &str,
    revision: i64,
    overrides: &Value,
    key: &str,
    workload: &str,
) -> DomainResult<(String, String)> {
    let (model, effort) = if let Some(pair) = overrides.get(key) {
        (
            pair["model"].text().ok_or("MODEL_OVERRIDES_INVALID")?,
            pair["reasoning_effort"]
                .text()
                .ok_or("MODEL_OVERRIDES_INVALID")?,
        )
    } else {
        hosts::recommended_assignment(host, workload, Some(revision))?
    };
    if key == "supervisor_recommendation" && model == hosts::MUSE_MODEL {
        return Err("SUPERVISOR_MODEL_RUNTIME_UNSUPPORTED".into());
    }
    hosts::selected_dispatch_model(host, &model, &effort, Some(revision))?;
    Ok((model, effort))
}
pub fn recommend_size(task: &Value) -> (&'static str, &'static str) {
    if task["risk_level"].is_text("critical") || task["minimum_route"].is_text("full_delivery") {
        return ("large", "critical risk or full-delivery route floor");
    }
    if task["risk_level"].is_text("high")
        || task["mandatory_impact_tags"].truthy()
        || ["design_only", "delivery_only", "fast_path"]
            .iter()
            .any(|route| task["minimum_route"].is_text(route))
    {
        return (
            "medium",
            "elevated risk, impact tags, or a delivery/design route",
        );
    }
    (
        "small",
        "low-risk advisory work with no mandatory impact tags",
    )
}
pub fn recommend_size_v2(task: &Value) -> DomainResult<(&'static str, Vec<&'static str>, Value)> {
    let mut tags = task["mandatory_impact_tags"]
        .as_list()
        .ok_or("mandatory impact tags must be a list")?
        .to_vec();
    tags.sort_by(|a, b| a.as_string().cmp(&b.as_string()));
    let inputs = Value::object([
        ("risk_level", task["risk_level"].clone()),
        ("mandatory_impact_tags", Value::List(tags)),
        (
            "model_sizing",
            Value::object([
                ("scope_extent", task["model_sizing"]["scope_extent"].clone()),
                ("uncertainty", task["model_sizing"]["uncertainty"].clone()),
            ]),
        ),
    ]);
    let mut reasons = Vec::new();
    if ["high", "critical"]
        .iter()
        .any(|risk| inputs["risk_level"].is_text(risk))
    {
        reasons.push("risk_high_or_critical");
    }
    if inputs["mandatory_impact_tags"]
        .as_list()
        .unwrap()
        .iter()
        .any(|tag| tag.is_text("security_privacy"))
    {
        reasons.push("security_privacy_required");
    }
    if inputs["model_sizing"]["uncertainty"].is_text("high") {
        reasons.push("uncertainty_high");
    }
    if inputs["model_sizing"]["scope_extent"].is_text("broadly_cross_cutting") {
        reasons.push("scope_broadly_cross_cutting");
    }
    if !reasons.is_empty() {
        return Ok(("large", reasons, inputs));
    }
    if inputs["risk_level"].is_text("medium") {
        reasons.push("risk_medium");
    }
    if inputs["model_sizing"]["scope_extent"].is_text("cross_file") {
        reasons.push("scope_cross_file");
    }
    if inputs["model_sizing"]["uncertainty"].is_text("medium") {
        reasons.push("uncertainty_medium");
    }
    if inputs["mandatory_impact_tags"]
        .as_list()
        .unwrap()
        .iter()
        .any(|tag| !tag.is_text("security_privacy"))
    {
        reasons.push("mandatory_nonsecurity_impact_tag");
    }
    if reasons.is_empty() {
        Ok(("small", vec!["bounded_low_risk_low_uncertainty"], inputs))
    } else {
        Ok(("medium", reasons, inputs))
    }
}
#[derive(Clone, Copy)]
struct PlanVintage {
    revision: Option<i64>,
    panel: Option<i64>,
    simplification: Option<i64>,
    projection: Option<i64>,
}
pub fn build_execution_plan(
    run: &str,
    task: &Value,
    requested: Option<&str>,
    host: &str,
) -> DomainResult<Value> {
    let revision = hosts::current_revision(host)?;
    let overrides = validate_model_overrides(
        task.get("model_overrides")
            .unwrap_or(&Value::Object(vec![])),
    )?;
    for (_, pair) in overrides.as_object().unwrap() {
        hosts::require_active_model(&pair["model"].text().ok_or("MODEL_OVERRIDES_INVALID")?)?;
    }
    if let Some(delegation) = task.get("reviewer_delegation").filter(|v| v.truthy()) {
        for assignment in delegation["assignments"]
            .as_list()
            .ok_or("MODEL_ASSIGNMENT_INVALID")?
        {
            hosts::require_active_model(
                &assignment["model"]
                    .text()
                    .ok_or("MODEL_ASSIGNMENT_INVALID")?,
            )?;
        }
    }
    build_plan(
        run,
        task,
        requested,
        host,
        PlanVintage {
            revision: Some(revision),
            panel: Some(1),
            simplification: task["request_mode"].is_text("delivery").then_some(1),
            projection: Some(1),
        },
    )
}
pub fn reconstruct_execution_plan(
    run: &str,
    task: &Value,
    stored: &Value,
    requested: Option<&str>,
) -> DomainResult<Value> {
    let host = stored["host"]
        .text()
        .unwrap_or_else(|| hosts::LEGACY_HOST.into());
    let revision = stored["catalog_revision"].as_i64();
    if stored.get("catalog_revision").is_some()
        && revision.is_none_or(|n| !hosts::supported_revisions(&host).contains(&n))
    {
        return Err("CATALOG_REVISION_INVALID".into());
    }
    for (key, error) in [
        (
            "code_review_panel_version",
            "CODE_REVIEW_PANEL_VERSION_INVALID",
        ),
        (
            "capability_projection_version",
            "CAPABILITY_PROJECTION_VERSION_INVALID",
        ),
        (
            "simplification_review_version",
            "SIMPLIFICATION_REVIEW_VERSION_INVALID",
        ),
    ] {
        if let Some(value) = stored.get(key) {
            if value.as_i64() != Some(1)
                || (key == "simplification_review_version"
                    && !task["request_mode"].is_text("delivery"))
            {
                return Err(error.into());
            }
        }
    }
    build_plan(
        run,
        task,
        requested,
        &host,
        PlanVintage {
            revision,
            panel: stored["code_review_panel_version"].as_i64(),
            simplification: stored["simplification_review_version"].as_i64(),
            projection: stored["capability_projection_version"].as_i64(),
        },
    )
}
fn selection(model: String, effort: String, dispatch: String) -> Value {
    Value::object([
        ("model", model.into()),
        ("reasoning_effort", effort.into()),
        ("dispatch_model", dispatch.into()),
    ])
}
fn build_plan(
    run: &str,
    task: &Value,
    requested: Option<&str>,
    host: &str,
    vintage: PlanVintage,
) -> DomainResult<Value> {
    let schema = if task["schema_version"].py_eq(&1_i64.into()) {
        1
    } else if task["schema_version"].py_eq(&2_i64.into()) {
        2
    } else if task["schema_version"].py_eq(&3_i64.into()) {
        3
    } else {
        return Err("unsupported task brief schema".into());
    };
    let (recommended, reason, codes, inputs) = if schema == 1 {
        let (size, reason) = recommend_size(task);
        (size, reason.to_owned(), vec![], Value::Null)
    } else {
        let (size, codes, inputs) = recommend_size_v2(task)?;
        (size, codes.join(", "), codes, inputs)
    };
    let size = requested.filter(|s| !s.is_empty()).unwrap_or(recommended);
    let size_index = SIZES
        .iter()
        .position(|s| *s == size)
        .ok_or("invalid execution size")?;
    if schema >= 2 && size_index < SIZES.iter().position(|s| *s == recommended).unwrap() {
        return Err("EXECUTION_SIZE_BELOW_SAFETY_FLOOR".into());
    }
    let overrides = validate_model_overrides(
        task.get("model_overrides")
            .unwrap_or(&Value::Object(vec![])),
    )?;
    let modern = vintage.revision.is_some_and(|n| (3..=10).contains(&n));
    if vintage.simplification.is_some() && !modern {
        return Err("SIMPLIFICATION_REVIEW_REQUIRES_CATALOG_3".into());
    }
    if overrides.get("simplification_assignment").is_some() && vintage.simplification.is_none() {
        return Err("SIMPLIFICATION_REVIEW_NOT_ENABLED".into());
    }
    let delegation = task.get("reviewer_delegation").filter(|v| !v.is_null());
    if vintage.panel.is_some()
        && delegation.is_some_and(|d| {
            d["assignments"]
                .as_list()
                .is_some_and(|items| items.iter().any(|a| a["role"].is_text("code_reviewer")))
        })
    {
        return Err("CODE_REVIEW_PANEL_EXTRA_REVIEWER_FORBIDDEN".into());
    }
    if overrides.truthy() && !modern {
        return Err("MODEL_OVERRIDES_REQUIRE_CATALOG_3".into());
    }
    let mut nodes = NODE_ROLES.to_vec();
    nodes.sort_by_key(|(node, _)| *node);
    let mut assignments = Vec::new();
    for (node, role) in nodes {
        if panel(node).is_some() && node != "code_reviewer" && vintage.panel.is_none() {
            continue;
        }
        let (mut class, mut effort) = class_for_node(node, size)?;
        if host == "codex-astra" && vintage.revision == Some(2) {
            match node {
                "tech_lead" | "senior_engineer" | "test_engineer" => {
                    class = "reasoning";
                    effort = "low";
                }
                "architect" | "code_reviewer" | "security_reviewer" => {
                    class = "reasoning";
                    effort = "medium";
                }
                _ => (),
            }
        } else if vintage.revision.is_some() && node == "senior_engineer" && class == "economy" {
            (class, effort) = class_for_node(node, "medium")?;
        }
        let (model, reasoning_effort, dispatch) = if modern && role != "supervisor" {
            let workload = if role == "impact_mapper" || node == "code_reviewer_naming" {
                "helper"
            } else if panel(node).is_some()
                || [
                    "architect",
                    "security_reviewer",
                    "release_operations_reviewer",
                ]
                .contains(&node)
            {
                "review"
            } else {
                "implementation"
            };
            let (model, effort) =
                selected_pair(host, vintage.revision.unwrap(), &overrides, node, workload)?;
            class = if workload == "helper" {
                "economy"
            } else {
                "reasoning"
            };
            let dispatch = hosts::selected_dispatch_model(host, &model, &effort, vintage.revision)?;
            (model, effort, dispatch)
        } else {
            let (model, effort) = hosts::resolve_assignment(host, class, effort)?;
            if vintage.revision.is_some() {
                validate_new_plan_assignment(node, model, effort, host)?;
            } else {
                validate_model_assignment(node, model, effort, host)?;
            }
            (
                model.to_owned(),
                effort.to_owned(),
                hosts::dispatch_model(host, model, effort)?,
            )
        };
        let mut assignment = Value::object([
            ("node_key", node.into()),
            ("role", role.into()),
            ("intelligence_class", class.into()),
            ("model", model.clone().into()),
            ("reasoning_effort", reasoning_effort.into()),
            ("dispatch_model", dispatch.into()),
            ("dispatch_when", dispatch_when(node, task).into()),
        ]);
        if vintage.revision.is_some_and(|n| [9, 10].contains(&n)) && model == hosts::MUSE_MODEL {
            assignment.set("dispatch_runtime", "opencode-cli".into());
        }
        if vintage.panel.is_some() && (panel(node).is_some() || node == "advisory_reviewer") {
            let key = if node == "advisory_reviewer" {
                "code_reviewer"
            } else {
                node
            };
            let (focus, mut skill) = panel(key).unwrap();
            if key == "code_reviewer_bug_hunter" {
                skill = Some(match host {
                    "cursor" => "review-bugbot",
                    "claude" => "bug-hunter-review",
                    _ => "review-agent",
                });
            }
            assignment.set(
                "code_review_assignment",
                Value::object([
                    ("focus", focus.into()),
                    (
                        "required_skill",
                        skill.map(Value::from).unwrap_or(Value::Null),
                    ),
                ]),
            );
        }
        assignments.push(assignment);
    }
    let supervisor = hosts::supervisor_recommendation(host)?;
    let publication = hosts::publication_assignment(host)?;
    let mut supervisor = selection(
        supervisor.model.into(),
        supervisor.effort.into(),
        supervisor.dispatch.into(),
    );
    let mut publication = selection(
        publication.model.into(),
        publication.effort.into(),
        publication.dispatch.into(),
    );
    if modern {
        let (model, effort) = selected_pair(
            host,
            vintage.revision.unwrap(),
            &overrides,
            "supervisor_recommendation",
            "review",
        )?;
        let dispatch = hosts::selected_dispatch_model(host, &model, &effort, vintage.revision)?;
        supervisor = selection(model, effort, dispatch);
        let (model, effort) = selected_pair(
            host,
            vintage.revision.unwrap(),
            &overrides,
            "publication_assignment",
            "helper",
        )?;
        let dispatch = hosts::selected_dispatch_model(host, &model, &effort, vintage.revision)?;
        publication = selection(model, effort, dispatch);
    }
    if vintage.revision.is_some_and(|n| [9, 10].contains(&n))
        && publication["model"].is_text(hosts::MUSE_MODEL)
    {
        publication.set("dispatch_runtime", "opencode-cli".into());
    }
    let mut plan = Value::object([
        (
            "schema_version",
            (if schema == 3 {
                3_i64
            } else if delegation.is_some() {
                2
            } else {
                1
            })
            .into(),
        ),
        ("run_id", run.into()),
        ("task_id", task["task_id"].clone()),
        ("host", host.into()),
        ("size", size.into()),
        (
            "size_source",
            if requested.is_some_and(|s| !s.is_empty()) {
                "supervisor_override"
            } else {
                "supervisor_recommendation"
            }
            .into(),
        ),
        ("size_recommendation", recommended.into()),
        ("size_recommendation_reason", reason.into()),
        ("minimum_route", task["minimum_route"].clone()),
        (
            "mandatory_impact_tags",
            task["mandatory_impact_tags"].clone(),
        ),
        ("supervisor_recommendation", supervisor),
        ("publication_assignment", publication),
        ("assignments", Value::List(assignments)),
        ("approval_id", "execution_plan".into()),
        ("approval_required", true.into()),
    ]);
    if let Some(delegation) = delegation {
        let assignments = delegation["assignments"]
            .as_list()
            .ok_or("MODEL_ASSIGNMENT_INVALID")?
            .to_vec();
        plan.set("conditional_review_assignments", Value::List(assignments));
        plan.set("reviewer_delegation_limits", delegation["limits"].clone());
    }
    if schema >= 2 {
        plan.set("size_policy_version", 2_i64.into());
        plan.set("size_recommendation_inputs", inputs);
        plan.set(
            "size_recommendation_reason_codes",
            Value::list(codes.into_iter().map(Value::from)),
        );
    }
    if schema == 3 {
        plan.set("helper_allowance", task["helper_allowance"].clone());
    }
    if let Some(revision) = vintage.revision {
        plan.set("catalog_revision", revision.into());
    }
    if modern {
        plan.set("model_overrides", overrides.clone());
        let options = hosts::model_options(host, vintage.revision)?;
        plan.set(
            "model_options",
            Value::Object(
                options
                    .into_iter()
                    .map(|(model, efforts)| {
                        (
                            PyString::from(model),
                            Value::list(efforts.into_iter().map(Value::from)),
                        )
                    })
                    .collect(),
            ),
        );
        let (model, effort) = hosts::recommended_assignment(host, "helper", vintage.revision)?;
        plan.set(
            "helper_recommendation",
            Value::object([("model", model.into()), ("reasoning_effort", effort.into())]),
        );
    }
    if ["codex", "codex-astra"].contains(&host) {
        if vintage.revision.is_some_and(|n| [6, 7].contains(&n)) {
            plan.set(
                "economy_fanout_option",
                Value::object([
                    ("model", "gpt-6-luna".into()),
                    ("reasoning_effort", "max".into()),
                ]),
            );
        }
        if vintage.revision.is_some_and(|n| [8, 9, 10].contains(&n)) {
            plan.set(
                "economy_fanout_option",
                Value::object([
                    ("model", "gpt-6.1-sol".into()),
                    ("reasoning_effort", "low".into()),
                ]),
            );
        }
        if vintage.revision.is_some_and(|n| [9, 10].contains(&n)) {
            plan.set(
                "external_economy_option",
                Value::object([
                    ("model", hosts::MUSE_MODEL.into()),
                    ("reasoning_effort", "xhigh".into()),
                    ("dispatch_runtime", "opencode-cli".into()),
                ]),
            );
        }
    }
    if let Some(version) = vintage.panel {
        plan.set("code_review_panel_version", version.into());
    }
    if let Some(version) = vintage.simplification {
        let (model, effort) = selected_pair(
            host,
            vintage.revision.unwrap(),
            &overrides,
            "simplification_assignment",
            "review",
        )?;
        let dispatch = hosts::selected_dispatch_model(host, &model, &effort, vintage.revision)?;
        let mut assignment = Value::object([
            ("role", "code_reviewer".into()),
            ("model", model.clone().into()),
            ("reasoning_effort", effort.into()),
            ("dispatch_model", dispatch.into()),
            (
                "code_review_assignment",
                Value::object([
                    ("focus", "simplification".into()),
                    ("required_skill", Value::Null),
                ]),
            ),
            (
                "dispatch_when",
                "before recording each Senior Engineer implementation result".into(),
            ),
            ("max_passes_per_generation", 1_i64.into()),
        ]);
        if vintage.revision.is_some_and(|n| [9, 10].contains(&n)) && model == hosts::MUSE_MODEL {
            assignment.set("dispatch_runtime", "opencode-cli".into());
        }
        plan.set("simplification_review_version", version.into());
        plan.set("simplification_assignment", assignment);
    }
    if let Some(version) = vintage.projection {
        plan.set("capability_projection_version", version.into());
    }
    let digest = ids::sha256_bytes(&ids::canonical_bytes(&plan).map_err(|e| e.to_string())?);
    plan.set("plan_digest", digest.into());
    Ok(plan)
}
pub fn assignment_for<'a>(plan: &'a Value, node: &str) -> DomainResult<&'a Value> {
    let host = plan["host"]
        .text()
        .unwrap_or_else(|| hosts::LEGACY_HOST.into());
    for assignment in plan["assignments"]
        .as_list()
        .ok_or("missing execution assignment")?
    {
        if assignment["node_key"].is_text(node) {
            let model = assignment["model"]
                .text()
                .ok_or("MODEL_ASSIGNMENT_INVALID")?;
            let effort = assignment["reasoning_effort"]
                .text()
                .ok_or("MODEL_ASSIGNMENT_INVALID")?;
            if plan["catalog_revision"]
                .as_i64()
                .is_some_and(|n| (3..=10).contains(&n))
            {
                if node_role(node) == Some("supervisor") {
                    if (model.as_str(), effort.as_str()) != ("primary-thread", "inherited") {
                        return Err("SUPERVISOR_EFFORT_INVALID".into());
                    }
                } else if model == "primary-thread" {
                    return Err("MODEL_ASSIGNMENT_INVALID".into());
                } else {
                    hosts::selected_dispatch_model(
                        &host,
                        &model,
                        &effort,
                        plan["catalog_revision"].as_i64(),
                    )?;
                }
            } else if plan.get("catalog_revision").is_some() {
                validate_new_plan_assignment(node, &model, &effort, &host)?;
            } else {
                validate_model_assignment(node, &model, &effort, &host)?;
            }
            return Ok(assignment);
        }
    }
    Err("missing execution assignment".into())
}
pub fn plan_approval_digest(
    run: &str,
    plan: &str,
    decision: &str,
    authority: &str,
    actor: &str,
    host: &str,
    at: &str,
) -> DomainResult<String> {
    let value = Value::object([
        ("run_id", run.into()),
        ("approval_id", "execution_plan".into()),
        ("plan_digest", plan.into()),
        ("decision", decision.into()),
        ("authority_ref", authority.into()),
        ("actor", actor.into()),
        ("host_identity", host.into()),
        ("approved_at", at.into()),
    ]);
    Ok(ids::sha256_bytes(
        &ids::canonical_bytes(&value).map_err(|e| e.to_string())?,
    ))
}
pub fn plan_approval_digest_values(
    fields: [&Value; 7],
) -> Result<String, crate::compat::JsonError> {
    let value = Value::object([
        ("run_id", fields[0].clone()),
        ("approval_id", "execution_plan".into()),
        ("plan_digest", fields[1].clone()),
        ("decision", fields[2].clone()),
        ("authority_ref", fields[3].clone()),
        ("actor", fields[4].clone()),
        ("host_identity", fields[5].clone()),
        ("approved_at", fields[6].clone()),
    ]);
    Ok(ids::sha256_bytes(&ids::canonical_bytes(&value)?))
}
