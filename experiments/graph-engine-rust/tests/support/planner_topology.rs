use super::{
    hardening_case::HardeningCase,
    planner_case::{approve_plan, initialize, task_v2},
};
use graph_engine::{
    application::{
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        state::GraphSession,
    },
    compat::{self, Value},
    domain::python_operations::sort_comparable_values,
};
use std::collections::{BTreeMap, BTreeSet};

fn sorted(mut values: Vec<Value>) -> Value {
    sort_comparable_values(&mut values).unwrap();
    Value::List(values)
}

fn fields(row: &Value, keys: &[&str]) -> Value {
    Value::list(keys.iter().map(|key| row[*key].clone()))
}

fn object_fields(row: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .map(|key| ((*key).into(), row[*key].clone()))
            .collect(),
    )
}

fn topology(case: &HardeningCase) -> Value {
    case.fixture
        .with_session(|session, _| {
            let run = session.run("RUN-1")?.unwrap().fields;
            let run_id = &[run["run_id"].clone()];
            let plan_row = session.first(GraphRead::ExecutionPlan, run_id)?.unwrap();
            let plan = compat::loads_value(&plan_row["plan_json"])?;
            let task = compat::loads_value(&run["task_json"])?;
            let raw_nodes = session.all(GraphRead::NodesForStatus, run_id)?;
            let node_ids = raw_nodes
                .iter()
                .map(|row| {
                    (
                        row["branch_id"].text().unwrap(),
                        fields(
                            row,
                            &["node_key", "role", "stage", "generation", "specialist_tag"],
                        ),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let node = |id: &Value| node_ids[&id.text().unwrap()].clone();
            let nodes = sorted(
                raw_nodes
                    .iter()
                    .map(|row| {
                        let parent = row["parent_branch_id"]
                            .text()
                            .and_then(|id| node_ids.get(&id))
                            .cloned()
                            .unwrap_or(Value::Null);
                        Value::list([
                            node(&row["branch_id"]),
                            row["mandatory"].truthy().into(),
                            row["status"].clone(),
                            row["retry_count"].clone(),
                            row["max_retries"].clone(),
                            parent,
                            row["depth"].clone(),
                        ])
                    })
                    .collect(),
            );
            let mut joins = Vec::new();
            let mut join_members = Vec::new();
            for row in session.all(GraphRead::JoinsForStatus, run_id)? {
                let id = &row["join_id"];
                let full = session
                    .first(GraphRead::JoinForRun, &[id.clone(), run["run_id"].clone()])?
                    .unwrap();
                let identity = fields(&full, &["join_key", "kind", "stage", "generation"]);
                joins.push(Value::list([
                    identity.clone(),
                    full["status"].clone(),
                    full["degraded"].truthy().into(),
                ]));
                for member in session.all(GraphRead::JoinMembers, std::slice::from_ref(id))? {
                    join_members.push(Value::list([
                        identity.clone(),
                        node(&member["branch_id"]),
                        member["join_mandatory"].truthy().into(),
                    ]));
                }
            }
            let mut fanouts = Vec::new();
            let mut dependencies = Vec::new();
            for fanout in session.all(GraphRead::FanoutsForStatus, run_id)? {
                let identity = fields(&fanout, &["stage", "generation"]);
                let members = compat::loads_value(&fanout["member_branch_ids_json"])?;
                let members = sorted(members.as_list().unwrap().iter().map(node).collect());
                fanouts.push(Value::list([
                    identity.clone(),
                    fanout["status"].clone(),
                    members,
                ]));
                for dependency in session.all(
                    GraphRead::FanoutDependencies,
                    std::slice::from_ref(&fanout["fanout_id"]),
                )? {
                    dependencies.push(Value::list([
                        identity.clone(),
                        node(&dependency["before_branch_id"]),
                        node(&dependency["after_branch_id"]),
                        dependency["reason"].clone(),
                    ]));
                }
            }
            let mut run_summary = object_fields(
                &run,
                &["status", "request_mode", "minimum_route", "selected_route"],
            );
            let tags = if run["selected_tags_json"].truthy() {
                compat::loads_value(&run["selected_tags_json"])?
            } else {
                Value::List(vec![])
            };
            run_summary.set("selected_tags", tags);
            let barrier = Value::object([
                ("plan_schema_version", plan["schema_version"].clone()),
                ("approval_required", plan["approval_required"].clone()),
                ("status", plan_row["status"].clone()),
                (
                    "approved",
                    [
                        "authority_ref",
                        "approved_at",
                        "approved_by",
                        "approval_digest",
                    ]
                    .iter()
                    .all(|key| !plan_row[*key].is_null())
                    .into(),
                ),
            ]);
            Ok(Value::object([
                ("run", run_summary),
                ("approval_barrier", barrier),
                (
                    "closure_requirements",
                    object_fields(
                        &task,
                        &[
                            "acceptance_ids",
                            "required_check_ids",
                            "required_human_decisions",
                        ],
                    ),
                ),
                ("nodes", nodes),
                ("joins", sorted(joins)),
                ("join_members", sorted(join_members)),
                ("fanouts", sorted(fanouts)),
                ("fanout_dependencies", sorted(dependencies)),
            ]))
        })
        .unwrap()
}

fn assess_in_order(case: &mut HardeningCase, operation: &str) {
    let status = case.status().unwrap();
    let action = &status["next_action"];
    let fanout = status["fanouts"]
        .as_list()
        .unwrap()
        .iter()
        .find(|item| item["fanout_id"].py_eq(&action["fanout_id"]))
        .unwrap();
    let members = fanout["member_branch_ids"].as_list().unwrap();
    let resources = Value::object([
        ("writable_paths", Value::List(vec![])),
        ("mutable_state_refs", Value::List(vec![])),
        ("exclusive_device_refs", Value::List(vec![])),
        ("services", Value::List(vec![])),
    ]);
    let evidence = case.repo_artifact("finding", &format!("fanout-evidence-{}", case.counter()));
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", "RUN-1".into()),
        ("fanout_id", action["fanout_id"].clone()),
        (
            "members",
            Value::list(members.iter().map(|id| {
                Value::object([("branch_id", id.clone()), ("resources", resources.clone())])
            })),
        ),
        (
            "dependencies",
            Value::list(members.windows(2).map(|pair| {
                Value::object([
                    ("before_branch_id", pair[0].clone()),
                    ("after_branch_id", pair[1].clone()),
                    ("reason", "topology-order".into()),
                ])
            })),
        ),
        ("evidence", Value::list([evidence])),
    ]);
    let path = case.inbox(&manifest);
    case.run(&[
        "record",
        "fanout-assessment",
        "--run-id",
        "RUN-1",
        "--fanout-id",
        &action["fanout_id"].text().unwrap(),
        "--assessment-manifest",
        path.to_str().unwrap(),
        "--authority-ref",
        "authority:test",
        "--op-id",
        operation,
    ])
    .unwrap();
}

fn settle_reviews(case: &mut HardeningCase, count: usize) {
    for _ in 0..count {
        let reviewer = case.claim_raw().unwrap();
        case.success_with(&reviewer, Some("APPROVE"), vec![])
            .unwrap();
    }
}

fn full_delivery_trace(size: &str) -> (Vec<Value>, Value) {
    let mut case = HardeningCase::stock();
    let task = task_v2(&case, "full_delivery");
    let initialized = initialize(&case, &task, Some("codex"), Some(size), false).unwrap();
    let assignments = initialized["execution_plan"]["assignments"]
        .as_list()
        .unwrap()
        .iter()
        .map(|item| {
            (
                item["node_key"].as_string().unwrap().clone(),
                fields(
                    item,
                    &[
                        "role",
                        "intelligence_class",
                        "model",
                        "reasoning_effort",
                        "dispatch_when",
                    ],
                ),
            )
        })
        .collect();
    let mut trace = vec![topology(&case)];
    approve_plan(
        &case,
        &initialized["execution_plan_digest"],
        "plan-approval-1",
    )
    .unwrap();
    trace.push(topology(&case));
    let mapper = case.claim();
    case.record_mapper(&mapper, &["release_operations"]);
    trace.push(topology(&case));
    assess_in_order(&mut case, "assess-research");
    trace.push(topology(&case));
    for _ in 0..2 {
        let branch = case.claim_raw().unwrap();
        case.success(&branch);
    }
    case.advance("research_collection");
    trace.push(topology(&case));
    let lead = case.claim_raw().unwrap();
    assert!(lead["node_key"].is_text("tech_lead"));
    case.success(&lead);
    case.advance("design_inputs");
    trace.push(topology(&case));
    assess_in_order(&mut case, "assess-design");
    trace.push(topology(&case));
    settle_reviews(&mut case, 2);
    case.advance("design_collection");
    trace.push(topology(&case));
    case.consolidate("design", "APPROVE", 0, vec![]);
    case.advance("design_consolidation");
    trace.push(topology(&case));
    let engineer = case.claim_raw().unwrap();
    assert!(engineer["node_key"].is_text("senior_engineer"));
    case.success_with(&engineer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    trace.push(topology(&case));
    assess_in_order(&mut case, "assess-delivery");
    trace.push(topology(&case));
    settle_reviews(&mut case, 6);
    case.advance("delivery_collection");
    trace.push(topology(&case));
    case.consolidate("delivery", "ACCEPT", 0, vec![]);
    case.advance("delivery_consolidation");
    trace.push(topology(&case));
    (trace, Value::Object(assignments))
}

fn first(row: &Value) -> &[Value] {
    row.as_list().unwrap()[0].as_list().unwrap()
}

fn names(rows: &Value) -> BTreeSet<String> {
    rows.as_list()
        .unwrap()
        .iter()
        .map(|row| first(row)[0].text().unwrap())
        .collect()
}

#[test]
fn test_v2_sizes_change_assignments_without_changing_full_delivery_gates() {
    let (small, small_assignments) = full_delivery_trace("small");
    let (medium, medium_assignments) = full_delivery_trace("medium");
    let (large, large_assignments) = full_delivery_trace("large");
    let same = |other: &[Value]| {
        small.len() == other.len()
            && small
                .iter()
                .zip(other)
                .all(|(left, right)| left.py_eq(right))
    };
    assert!(same(&medium));
    assert!(same(&large));
    let pending = Value::object([
        ("plan_schema_version", 1_i64.into()),
        ("approval_required", true.into()),
        ("status", "pending".into()),
        ("approved", false.into()),
    ]);
    assert!(small[0]["approval_barrier"].py_eq(&pending));
    assert!(small[1]["approval_barrier"]["status"].is_text("approved"));
    assert!(small[1]["approval_barrier"]["approved"].is_true());
    let final_state = small.last().unwrap();
    assert_eq!(
        names(&final_state["nodes"]),
        BTreeSet::from(
            [
                "impact_mapper",
                "design_research_architecture",
                "design_research_validation",
                "tech_lead",
                "architect",
                "release_operations_reviewer",
                "senior_engineer",
                "code_reviewer",
                "code_reviewer_architecture",
                "code_reviewer_naming",
                "code_reviewer_bug_hunter",
                "test_engineer",
                "supervisor_design_consolidation",
                "supervisor_delivery_consolidation",
            ]
            .map(str::to_owned)
        )
    );
    let nodes = final_state["nodes"].as_list().unwrap();
    assert!(nodes.iter().all(|row| row.as_list().unwrap()[1].is_true()));
    assert!(final_state["join_members"]
        .as_list()
        .unwrap()
        .iter()
        .all(|row| row.as_list().unwrap()[2].is_true()));
    let stages = nodes
        .iter()
        .filter(|row| first(row)[0].is_text("release_operations_reviewer"))
        .map(|row| first(row)[2].text().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        stages,
        BTreeSet::from(["design".to_owned(), "delivery".to_owned()])
    );
    assert_eq!(
        names(&final_state["joins"]),
        BTreeSet::from(
            [
                "research_collection",
                "design_inputs",
                "design_collection",
                "design_consolidation",
                "implementation",
                "delivery_collection",
                "delivery_consolidation",
                "closure",
            ]
            .map(str::to_owned)
        )
    );
    assert_eq!(
        names(&final_state["fanouts"]),
        BTreeSet::from(["research", "design", "delivery"].map(str::to_owned))
    );
    assert_eq!(
        final_state["fanout_dependencies"].as_list().unwrap().len(),
        7
    );
    let closure = final_state["joins"]
        .as_list()
        .unwrap()
        .iter()
        .find(|row| first(row)[0].is_text("closure"))
        .unwrap();
    assert!(closure.as_list().unwrap()[1].is_text("open"));
    assert!(final_state["run"]["selected_tags"].py_eq(&Value::list(["release_operations".into()])));
    let closure_requirements = Value::object([
        ("acceptance_ids", Value::list(["AC-001".into()])),
        ("required_check_ids", Value::list(["repo-check".into()])),
        ("required_human_decisions", Value::List(vec![])),
    ]);
    assert!(final_state["closure_requirements"].py_eq(&closure_requirements));
    assert!(small_assignments["senior_engineer"].py_eq(&medium_assignments["senior_engineer"]));
    assert!(medium_assignments["senior_engineer"].py_eq(&large_assignments["senior_engineer"]));
}
