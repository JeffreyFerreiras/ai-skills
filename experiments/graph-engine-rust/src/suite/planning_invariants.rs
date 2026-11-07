use super::{assignment, build, rebuild, TaskFixture};
use graph_engine::{
    compat::Value,
    domain::{execution, hosts, ids, planner},
};

fn delegation_fixture() -> Value {
    Value::object([
        (
            "limits",
            Value::object([
                ("max_depth", 1_i64.into()),
                ("max_children_per_request", 3_i64.into()),
                ("max_children_per_run", 6_i64.into()),
                ("max_request_rounds", 2_i64.into()),
                ("max_weighted_dispatch_cost", 15_i64.into()),
            ]),
        ),
        (
            "assignments",
            Value::list([Value::object([
                ("assignment_id", "correctness".into()),
                ("role", "code_reviewer".into()),
                ("model", "gpt-6.1-sol".into()),
                ("reasoning_effort", "high".into()),
                (
                    "review_lens",
                    "Cross-file correctness and concurrency".into(),
                ),
                (
                    "prompt_template",
                    "Review only the approved evidence and scope.".into(),
                ),
                ("allowed_reason_codes", Value::list(["CROSS_FILE".into()])),
                (
                    "allowed_evidence_kinds",
                    Value::list(["finding".into(), "implementation_handoff".into()]),
                ),
                ("scope_refs", Value::list(["repo:docs/".into()])),
                ("max_instances", 2_i64.into()),
                ("dispatch_weight", 3_i64.into()),
            ])]),
        ),
    ])
}
fn change_delegation(task: &mut Value, field: &str, value: &str) {
    let mut config = task["reviewer_delegation"].clone();
    let mut assignments = config["assignments"].as_list().unwrap().to_vec();
    assignments[0].set(field, value.into());
    config.set("assignments", Value::List(assignments));
    task.set("reviewer_delegation", config);
}

pub fn historical_delegation() {
    let fixture = TaskFixture::new();
    for mut task in [fixture.default_task(), fixture.default_v2()] {
        task.set("reviewer_delegation", delegation_fixture());
        let legacy = rebuild(&task, "codex-astra", None, Some("small"));
        let rebuilt =
            execution::reconstruct_execution_plan("RUN-1", &task, &legacy, Some("small")).unwrap();
        assert!(rebuilt.py_eq(&legacy));
        assert_eq!(rebuilt["schema_version"].as_i64(), Some(2));
        assert!(rebuilt.get("catalog_revision").is_none());
        assert_eq!(
            execution::build_execution_plan("RUN-1", &task, Some("small"), "codex-astra")
                .unwrap_err(),
            "CODE_REVIEW_PANEL_EXTRA_REVIEWER_FORBIDDEN"
        );
        change_delegation(&mut task, "role", "security_reviewer");
        let candidate = build(&task, "codex-astra", Some("small"));
        assert!(
            candidate["reviewer_delegation_limits"].py_eq(&legacy["reviewer_delegation_limits"])
        );
        assert!(!candidate["plan_digest"].py_eq(&legacy["plan_digest"]));
    }
}

pub fn retired_delegated_review() {
    let fixture = TaskFixture::new();
    let mut task = fixture.default_v2();
    task.set("reviewer_delegation", delegation_fixture());
    change_delegation(&mut task, "model", "gpt-5.6-sol");
    let plan = rebuild(&task, "codex-astra", Some(7), None);
    assert!(
        execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
            .unwrap()
            .py_eq(&plan)
    );
    assert_eq!(
        execution::build_execution_plan("RUN-1", &task, None, hosts::DEFAULT_HOST).unwrap_err(),
        "MODEL_RETIRED"
    );
}

pub fn medium_delegation_weights() {
    let weights = hosts::supported_dispatch_weights();
    assert_eq!(
        hosts::dispatch_weight_for("gpt-6-astra", "medium", false),
        Some(3)
    );
    assert_eq!(
        weights.get(&("gpt-6-astra".into(), "medium".into())),
        Some(&3)
    );
    for (model, effort) in [("gpt-6-astra", "low"), ("gpt-5.6-sol", "medium")] {
        assert_eq!(hosts::dispatch_weight_for(model, effort, false), None);
        assert!(!weights.contains_key(&(model.into(), effort.into())));
    }
}

pub fn astra_assignment_rejections() {
    for effort in ["none", "minimal", "ultra", "inherited"] {
        assert_eq!(
            execution::validate_model_assignment("tech_lead", "gpt-6-astra", effort, "codex-astra")
                .unwrap_err(),
            "MODEL_ASSIGNMENT_INVALID"
        );
    }
    assert_eq!(
        execution::validate_model_assignment("tech_lead", "gpt-6-astra", "high", "codex")
            .unwrap_err(),
        "MODEL_ASSIGNMENT_INVALID"
    );
}

pub fn new_writer_validation() {
    let fixture = TaskFixture::new();
    for host in hosts::HOSTS {
        let (model, effort) = hosts::resolve_assignment(host, "economy", "max").unwrap();
        assert_eq!(
            execution::validate_new_plan_assignment("senior_engineer", &model, &effort, host)
                .unwrap_err(),
            "IMPLEMENTATION_REASONING_MODEL_REQUIRED"
        );
        execution::validate_model_assignment("senior_engineer", &model, &effort, host).unwrap();
        let mut plan = rebuild(
            &fixture.default_task(),
            host,
            Some(if *host == "claude" { 1 } else { 2 }),
            Some("small"),
        );
        let mut rows = plan["assignments"].as_list().unwrap().to_vec();
        let writer = rows
            .iter_mut()
            .find(|row| row["node_key"].is_text("senior_engineer"))
            .unwrap();
        writer.set("model", model.into());
        writer.set("reasoning_effort", effort.into());
        writer.set("intelligence_class", "economy".into());
        plan.set("assignments", Value::List(rows));
        assert_eq!(
            execution::assignment_for(&plan, "senior_engineer").unwrap_err(),
            "IMPLEMENTATION_REASONING_MODEL_REQUIRED"
        );
    }
}

pub fn assignment_invariants() {
    for (node, model, effort, host, error) in [
        (
            "impact_mapper",
            "gpt-5.6-luna",
            "high",
            "codex",
            "ECONOMY_REASONING_EFFORT_REQUIRED",
        ),
        (
            "tech_lead",
            "gpt-5.6-luna",
            "max",
            "codex",
            "DESIGN_MODEL_REQUIRED",
        ),
        (
            "tech_lead",
            "gpt-5.6-sol",
            "medium",
            "unknown",
            "HOST_UNSUPPORTED",
        ),
    ] {
        assert_eq!(
            execution::validate_model_assignment(node, model, effort, host).unwrap_err(),
            error
        );
    }
    for (model, effort, weight) in [
        ("gpt-5.6-luna", "max", 3),
        ("gpt-5.6-sol", "high", 3),
        ("gpt-5.6-sol", "xhigh", 4),
        ("gpt-5.6-sol", "max", 5),
    ] {
        assert_eq!(
            hosts::dispatch_weight_for(model, effort, true),
            Some(weight)
        );
    }
    assert_eq!(
        hosts::dispatch_weight_for("gpt-5.6-luna", "high", false),
        None
    );
}

pub fn stable_ids() {
    let digest = "a".repeat(64);
    let first = ids::stable_id("RUN-1", &digest, "branch", "architect", 0, None).unwrap();
    let second = ids::stable_id("RUN-1", &digest, "branch", "architect", 0, None).unwrap();
    let changed = ids::stable_id("RUN-1", &digest, "branch", "architect", 1, None).unwrap();
    assert_eq!(first, second);
    assert_ne!(first, changed);
}

pub fn every_host_class_matrix() {
    for host in hosts::HOSTS {
        for (_, roles) in execution::CLASS_ASSIGNMENTS {
            for (_, class, effort) in *roles {
                let (model, resolved) = hosts::resolve_assignment(host, class, effort).unwrap();
                assert!(!model.is_empty());
                assert!(!resolved.is_empty());
            }
        }
    }
}

pub fn impact_mapper_class() {
    let assignments = Value::Object(
        execution::CLASS_ASSIGNMENTS
            .iter()
            .map(|(size, roles)| {
                let (_, class, effort) = roles
                    .iter()
                    .find(|(role, _, _)| *role == "impact_mapper")
                    .unwrap();
                (
                    (*size).into(),
                    Value::list([(*class).into(), (*effort).into()]),
                )
            })
            .collect(),
    );
    let expected = Value::object(
        ["small", "medium", "large"]
            .map(|size| (size, Value::list(["economy".into(), "max".into()]))),
    );
    assert!(assignments.py_eq(&expected));
}

pub fn legacy_model_invariants() {
    for (_, roles) in execution::SIZE_ASSIGNMENTS.as_object().unwrap() {
        for (_, selection) in roles.as_object().unwrap() {
            let pair = selection.as_list().unwrap();
            if pair[0].is_text("gpt-5.6-luna") {
                assert!(pair[1].is_text("max"));
            }
        }
        for key in ["tech_lead", "architect"] {
            assert!(roles[key].as_list().unwrap()[0].is_text("gpt-5.6-sol"));
        }
    }
}

pub fn exact_size_matrix() {
    let roles = [
        "impact_mapper",
        "advisory_reviewer",
        "tech_lead",
        "architect",
        "senior_engineer",
        "code_reviewer",
        "test_engineer",
        "audio_realtime_specialist",
        "ios_platform_specialist",
        "release_operations_reviewer",
        "security_reviewer",
        "supervisor",
    ];
    let expected = Value::Object(
        [
            (
                "small",
                [
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-sol", "medium"),
                    ("gpt-5.6-sol", "medium"),
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-sol", "medium"),
                    ("gpt-5.6-sol", "high"),
                    ("primary-thread", "inherited"),
                ],
            ),
            (
                "medium",
                [
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-sol", "medium"),
                    ("gpt-5.6-sol", "medium"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "medium"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "high"),
                    ("primary-thread", "inherited"),
                ],
            ),
            (
                "large",
                [
                    ("gpt-5.6-luna", "max"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "xhigh"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "xhigh"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "high"),
                    ("gpt-5.6-sol", "xhigh"),
                    ("primary-thread", "inherited"),
                ],
            ),
        ]
        .into_iter()
        .map(|(size, selections)| {
            (
                size.into(),
                Value::Object(
                    roles
                        .into_iter()
                        .zip(selections)
                        .map(|(role, (model, effort))| {
                            (role.into(), Value::list([model.into(), effort.into()]))
                        })
                        .collect(),
                ),
            )
        })
        .collect(),
    );
    assert!(execution::SIZE_ASSIGNMENTS.py_eq(&expected));
}

fn member(id: &str, paths: Vec<Value>, services: Vec<Value>) -> Value {
    Value::object([
        ("branch_id", id.into()),
        (
            "resources",
            Value::object([
                ("writable_paths", Value::List(paths)),
                ("mutable_state_refs", Value::List(vec![])),
                ("exclusive_device_refs", Value::List(vec![])),
                ("services", Value::List(services)),
            ]),
        ),
    ])
}
fn path(name: &str, scope: &str) -> Value {
    Value::object([("path", name.into()), ("scope", scope.into())])
}
fn dependency(before: &str, after: &str, reason: &str) -> Value {
    Value::object([
        ("before_branch_id", before.into()),
        ("after_branch_id", after.into()),
        ("reason", reason.into()),
    ])
}

pub fn transitive_fanout() {
    let members = vec![
        member("A", vec![path("src", "subtree")], vec![]),
        member("B", vec![path("src/app.py", "exact")], vec![]),
        member("C", vec![], vec![]),
    ];
    assert_eq!(
        planner::validate_fanout_ordering(&members, &[], true).unwrap_err(),
        "FANOUT_UNORDERED_CONFLICT"
    );
    let dependencies = vec![dependency("A", "C", "first"), dependency("C", "B", "then")];
    assert_eq!(
        planner::validate_fanout_ordering(&members, &dependencies, true)
            .unwrap()
            .len(),
        2
    );
}

pub fn fanout_capacity_and_cycles() {
    let service = Value::object([
        ("ref", "gpu".into()),
        ("units", 1_i64.into()),
        ("capacity", 1_i64.into()),
    ]);
    let members = vec![
        member("A", vec![], vec![service.clone()]),
        member("B", vec![], vec![service]),
    ];
    assert_eq!(
        planner::validate_fanout_ordering(&members, &[], true).unwrap_err(),
        "FANOUT_CAPACITY_EXCEEDED"
    );
    let mut ordered = vec![dependency("A", "B", "capacity")];
    planner::validate_fanout_ordering(&members, &ordered, true).unwrap();
    ordered.push(dependency("B", "A", "cycle"));
    assert_eq!(
        planner::validate_fanout_ordering(&members, &ordered, true).unwrap_err(),
        "FANOUT_CYCLE"
    );
}

pub fn fanout_case_policy() {
    let members = vec![
        member("A", vec![path("src/Feature.py", "exact")], vec![]),
        member("B", vec![path("src/feature.py", "exact")], vec![]),
    ];
    assert_eq!(
        planner::validate_fanout_ordering(&members, &[], false).unwrap_err(),
        "FANOUT_UNORDERED_CONFLICT"
    );
    assert!(planner::validate_fanout_ordering(&members, &[], true)
        .unwrap()
        .is_empty());
}
