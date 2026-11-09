#[allow(dead_code)]
#[path = "support/evidence_schema.rs"]
mod evidence_schema;
#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[allow(dead_code)]
#[path = "support/planner_case.rs"]
mod planner_case;
#[path = "support/planner_policy_files.rs"]
mod planner_policy_files;
#[path = "support/planner_topology.rs"]
mod planner_topology;

use graph_engine::{
    application::graph_ledger::{GraphLedgerReadPort, GraphRead},
    compat::{self, Value},
    domain::{
        execution::{assignment_for, build_execution_plan, reconstruct_execution_plan, PANEL},
        hosts::{DEFAULT_HOST, MUSE_MODEL},
        planner::{self, NodeSpec},
        task_contract::validate_task_brief,
    },
};
use hardening_case::HardeningCase;
use planner_case::{
    contract_failure, initialize, no_database, preview, resume, selection, task_v2,
};
use std::collections::BTreeSet;

fn assert_selection(branch: &Value, model: &str, effort: &str) {
    assert!(branch["model"].is_text(model));
    assert!(branch["reasoning_effort"].is_text(effort));
}

fn plan_rows(case: &HardeningCase) -> (Value, Vec<(String, String)>) {
    case.fixture
        .with_session(|session, _| {
            let plan = session
                .first(GraphRead::ExecutionPlan, &["RUN-1".into()])?
                .unwrap();
            let mut query = session
                .connection()
                .prepare("SELECT branch_id,envelope_json FROM nodes ORDER BY branch_id")
                .unwrap();
            let nodes = query
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            let record = Value::object([
                ("plan_json", plan["plan_json"].clone()),
                ("plan_digest", plan["plan_digest"].clone()),
                ("approval_digest", plan["approval_digest"].clone()),
            ]);
            Ok((record, nodes))
        })
        .unwrap()
}

#[test]
fn test_terra_approval_claim_and_resume_preserve_every_native_effort() {
    for host in ["codex", "codex-astra"] {
        for effort in ["low", "medium", "high", "xhigh", "max", "ultra"] {
            let mut case = HardeningCase::stock();
            let mut task = task_v2(&case, "fast_path");
            task.set(
                "model_overrides",
                Value::object(
                    [
                        "impact_mapper",
                        "senior_engineer",
                        "supervisor_recommendation",
                        "publication_assignment",
                        "simplification_assignment",
                    ]
                    .map(|key| (key, selection("gpt-6.1-terra", effort))),
                ),
            );
            let plan = preview(&case, &task, Some(host));
            no_database(&case);
            let initialized = initialize(&case, &task, Some(host), None, true).unwrap();
            assert!(initialized["execution_plan_digest"].py_eq(&plan["plan_digest"]));
            let branch = case.claim_raw().unwrap();
            assert_selection(&branch, "gpt-6.1-terra", effort);
            let approved = assignment_for(&plan, &branch["node_key"].text().unwrap()).unwrap();
            assert!(approved["dispatch_model"].py_eq(&branch["model"]));
            assert!(branch.get("dispatch_runtime").is_none());
            let (before, nodes) = plan_rows(&case);
            assert!(compat::loads_value(&before["plan_json"])
                .unwrap()
                .py_eq(&plan));
            assert!(before["approval_digest"].truthy());
            resume(&case).unwrap();
            let resumed = case.status().unwrap();
            for (key, value) in plan.as_object().unwrap() {
                assert!(resumed["execution_plan"][key.utf8().unwrap().as_str()].py_eq(value));
            }
            let (after, after_nodes) = plan_rows(&case);
            assert!(after.py_eq(&before));
            assert_eq!(after_nodes, nodes);
        }
    }
}

#[test]
fn test_invalid_terra_selections_fail_before_state_creation() {
    let case = HardeningCase::stock();
    for (host, model, effort) in [
        ("codex", "gpt-6.1-terra", "minimal"),
        ("codex-astra", "gpt-6.1-terra", "ULTRA"),
        ("codex", "gpt-6.1-terra-high", "high"),
        ("claude", "gpt-6.1-terra", "high"),
        ("cursor", "gpt-6.1-terra", "high"),
    ] {
        let mut task = task_v2(&case, "full_delivery");
        task.set(
            "model_overrides",
            Value::object([("tech_lead", selection(model, effort))]),
        );
        contract_failure(initialize(&case, &task, Some(host), None, true), None);
        no_database(&case);
    }
}

#[test]
fn test_invalid_model_overrides_fail_without_creating_state() {
    let case = HardeningCase::stock();
    for overrides in [
        Value::List(vec![]),
        Value::object([("unknown_role", selection("gpt-5.6-sol", "medium"))]),
        Value::object([("tech_lead", selection("primary-thread", "inherited"))]),
        Value::object([("tech_lead", selection("gpt-5.6-sol", "ultra"))]),
        Value::object([("tech_lead", selection("grok-4.7", "medium"))]),
    ] {
        let mut task = task_v2(&case, "full_delivery");
        task.set("model_overrides", overrides);
        contract_failure(initialize(&case, &task, None, None, true), None);
        no_database(&case);
    }
}

#[test]
fn test_astra_catalog_survives_approval_claim_and_resume() {
    let mut case = HardeningCase::stock();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    let initialized = initialize(&case, &task, Some("codex-astra"), Some("medium"), true).unwrap();
    case.impact("full_delivery");
    let lead = case.claim();
    assert!(lead["node_key"].is_text("tech_lead"));
    assert_selection(&lead, "gpt-6-astra", "medium");
    resume(&case).unwrap();
    let status = case.status().unwrap();
    let plan = &status["execution_plan"];
    assert!(plan["host"].is_text("codex-astra"));
    assert_eq!(plan["catalog_revision"].as_i64(), Some(10));
    assert!(plan["plan_digest"].py_eq(&initialized["execution_plan_digest"]));
    assert!(plan["status"].is_text("approved"));
}

#[test]
fn test_default_cli_plan_survives_approval_claim_and_resume() {
    let mut case = HardeningCase::stock();
    let task = task_v2(&case, "full_delivery");
    let plan = preview(&case, &task, None);
    no_database(&case);
    assert!(plan["host"].is_text("codex"));
    assert_eq!(plan["catalog_revision"].as_i64(), Some(11));
    for node in [
        "impact_mapper",
        "design_research_architecture",
        "design_research_validation",
        "code_reviewer_naming",
        "test_engineer",
    ] {
        let assignment = assignment_for(&plan, node).unwrap();
        assert_selection(assignment, MUSE_MODEL, "xhigh");
        assert!(assignment["dispatch_runtime"].is_text("opencode-cli"));
    }
    for node in ["tech_lead", "senior_engineer", "code_reviewer_architecture"] {
        let assignment = assignment_for(&plan, node).unwrap();
        assert_selection(assignment, "gpt-6.1-sol", "xhigh");
        assert!(assignment.get("dispatch_runtime").is_none());
    }
    let initialized = initialize(&case, &task, None, None, true).unwrap();
    assert!(initialized["execution_plan_digest"].py_eq(&plan["plan_digest"]));
    let mapper = case.claim_raw().unwrap();
    assert!(mapper["node_key"].is_text("impact_mapper"));
    assert_selection(&mapper, MUSE_MODEL, "xhigh");
    assert!(mapper["dispatch_runtime"].is_text("opencode-cli"));
    case.record_mapper(&mapper, &[]);
    let status = case.status().unwrap();
    assert!(status["next_action"]["kind"].is_text("record_fanout_assessment"));
    assert!(status["next_action"]["stage"].is_text("research"));
    assert!(plan.get("conditional_review_assignments").is_none());
    let lead = case.claim();
    assert!(lead["node_key"].is_text("tech_lead"));
    assert_selection(&lead, "gpt-6.1-sol", "xhigh");
    assert!(lead.get("dispatch_runtime").is_none());
    let (before, nodes) = plan_rows(&case);
    assert!(compat::loads_value(&before["plan_json"])
        .unwrap()
        .py_eq(&plan));
    assert!(before["approval_digest"].truthy());
    resume(&case).unwrap();
    let status = case.status().unwrap();
    for (key, value) in plan.as_object().unwrap() {
        assert!(status["execution_plan"][key.utf8().unwrap().as_str()].py_eq(value));
    }
    assert!(status["execution_plan"]["status"].is_text("approved"));
    let (after, after_nodes) = plan_rows(&case);
    assert!(after.py_eq(&before));
    assert_eq!(after_nodes, nodes);
}

#[test]
fn test_approved_muse_mapper_claim_retains_cli_dispatch_route() {
    let mut case = HardeningCase::stock();
    let mut task = task_v2(&case, "fast_path");
    task.set(
        "model_overrides",
        Value::object([("impact_mapper", selection(MUSE_MODEL, "xhigh"))]),
    );
    initialize(&case, &task, None, None, true).unwrap();
    let branch = case.claim_raw().unwrap();
    assert_selection(&branch, MUSE_MODEL, "xhigh");
    assert!(branch["dispatch_runtime"].is_text("opencode-cli"));
    case.status().unwrap();
}

#[test]
fn test_preview_can_be_adjusted_before_initialization_and_approved_selection_survives_resume() {
    let mut case = HardeningCase::stock();
    let mut task = task_v2(&case, "fast_path");
    let before = preview(&case, &task, None);
    no_database(&case);
    task.set(
        "model_overrides",
        Value::object([
            ("senior_engineer", selection("gpt-6.1-sol", "medium")),
            (
                "supervisor_recommendation",
                selection("gpt-6.1-sol", "high"),
            ),
            (
                "simplification_assignment",
                selection("gpt-6.1-sol", "medium"),
            ),
        ]),
    );
    let adjusted = preview(&case, &task, None);
    assert!(!before["plan_digest"].py_eq(&adjusted["plan_digest"]));
    let initialized = initialize(&case, &task, None, None, true).unwrap();
    assert!(initialized["execution_plan_digest"].py_eq(&adjusted["plan_digest"]));
    resume(&case).unwrap();
    assert!(
        case.status().unwrap()["execution_plan"]["simplification_assignment"]
            .py_eq(&adjusted["simplification_assignment"])
    );
    case.impact("fast_path");
    let writer = case.claim();
    assert_selection(&writer, "gpt-6.1-sol", "medium");
    assert!(writer["simplification_review_version"].py_eq(&1_i64.into()));
    evidence_schema::assert_branch_envelope_schema(&writer);
}

#[test]
fn test_new_panel_binds_four_independent_scopes_and_model_selections() {
    let case = HardeningCase::stock();
    case.fixture
        .with_context(|context| {
            let mut task = task_v2(&case, "fast_path");
            task.set(
                "model_overrides",
                Value::object([(
                    "code_reviewer_architecture",
                    selection("gpt-6-astra", "medium"),
                )]),
            );
            let digest = context.current_policy_digest;
            let mut normalized = validate_task_brief(
                &task,
                &digest.into(),
                context.policy,
                None,
                context.files.windows(),
            )?;
            let plan = build_execution_plan("RUN-1", &normalized, None, DEFAULT_HOST).unwrap();
            assert!(plan["code_review_panel_version"].py_eq(&1_i64.into()));
            assert!(
                reconstruct_execution_plan("RUN-1", &normalized, &plan, None)
                    .unwrap()
                    .py_eq(&plan)
            );
            let specs = planner::delivery_review_nodes(context.policy, &[], 0, Some(&plan))?;
            let reviewers = specs
                .iter()
                .filter(|spec| spec.role == "code_reviewer")
                .collect::<Vec<_>>();
            assert_eq!(
                reviewers
                    .iter()
                    .map(|spec| spec.key.as_str())
                    .collect::<BTreeSet<_>>(),
                PANEL.iter().map(|(key, _, _)| *key).collect()
            );
            assert!(reviewers.iter().all(|spec| spec.mandatory));
            let mut ids = BTreeSet::new();
            for spec in reviewers {
                let branch = planner::envelope(
                    "RUN-1",
                    digest,
                    context.policy,
                    &normalized,
                    spec,
                    "ready",
                    &[],
                    0,
                    Some(&plan),
                    context.files.windows(),
                )?;
                ids.insert(branch["branch_id"].text().unwrap());
                evidence_schema::assert_branch_envelope_schema(&branch);
                let (_, focus, skill) = PANEL.iter().find(|(key, _, _)| *key == spec.key).unwrap();
                let expected = Value::object([
                    ("focus", (*focus).into()),
                    ("required_skill", skill.map_or(Value::Null, Value::from)),
                ]);
                assert!(branch["code_review_assignment"].py_eq(&expected));
                if spec.key == "code_reviewer_naming" {
                    assert_selection(&branch, MUSE_MODEL, "xhigh");
                    assert!(branch["dispatch_runtime"].is_text("opencode-cli"));
                    assert!(
                        assignment_for(&plan, &spec.key).unwrap()["intelligence_class"]
                            .is_text("economy")
                    );
                    assert!(branch["effect_capabilities"]
                        .as_list()
                        .unwrap()
                        .iter()
                        .all(|cap| {
                            cap["effect"].is_text("filesystem_read")
                                || cap["effect"].is_text("external_read")
                        }));
                }
            }
            assert_eq!(ids.len(), 4);
            let mut overrides = normalized["model_overrides"].clone();
            overrides.set("code_reviewer_naming", selection("gpt-6-astra", "high"));
            normalized.set("model_overrides", overrides);
            let changed = build_execution_plan("RUN-1", &normalized, None, DEFAULT_HOST).unwrap();
            assert_selection(
                assignment_for(&changed, "code_reviewer_naming").unwrap(),
                "gpt-6-astra",
                "high",
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_historical_plan_keeps_one_reviewer_and_no_focus_envelope() {
    let case = HardeningCase::stock();
    case.fixture
        .with_context(|context| {
            let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
            let hint = Value::object([("host", DEFAULT_HOST.into())]);
            let legacy = reconstruct_execution_plan("RUN-1", &task, &hint, None).unwrap();
            let specs = planner::delivery_review_nodes(context.policy, &[], 0, Some(&legacy))?;
            let reviewers = specs
                .iter()
                .filter(|spec| spec.role == "code_reviewer")
                .collect::<Vec<_>>();
            assert_eq!(
                reviewers
                    .iter()
                    .map(|spec| spec.key.as_str())
                    .collect::<Vec<_>>(),
                ["code_reviewer"]
            );
            let branch = planner::envelope(
                "RUN-1",
                context.current_policy_digest,
                context.policy,
                &task,
                reviewers[0],
                "ready",
                &[],
                0,
                Some(&legacy),
                context.files.windows(),
            )?;
            assert!(branch.get("code_review_assignment").is_none());
            assert!(reconstruct_execution_plan("RUN-1", &task, &legacy, None)
                .unwrap()
                .py_eq(&legacy));
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_muse_selection_uses_opencode_and_preserves_previous_catalog() {
    let case = HardeningCase::stock();
    for (host, digest) in [
        (
            "codex",
            "d273f7915dbecda79e789826eb37919981a792caf1cbad0c72c481e56f701a6f",
        ),
        (
            "codex-astra",
            "63fdff6e6791bee4acff9bc725d8d52367b06a561ac3b88f1654151cb8e6de6e",
        ),
    ] {
        let mut task = task_v2(&case, "full_delivery");
        task.set(
            "model_overrides",
            Value::object([
                ("impact_mapper", selection(MUSE_MODEL, "low")),
                ("design_research_architecture", selection(MUSE_MODEL, "low")),
                ("tech_lead", selection(MUSE_MODEL, "medium")),
                ("publication_assignment", selection(MUSE_MODEL, "low")),
            ]),
        );
        let plan = build_execution_plan("RUN-1", &task, None, host).unwrap();
        for (node, effort) in [
            ("impact_mapper", "low"),
            ("design_research_architecture", "low"),
            ("tech_lead", "medium"),
        ] {
            let assignment = assignment_for(&plan, node).unwrap();
            assert_selection(assignment, MUSE_MODEL, effort);
            assert!(assignment["dispatch_model"].is_text(MUSE_MODEL));
            assert!(assignment["dispatch_runtime"].is_text("opencode-cli"));
        }
        assert!(reconstruct_execution_plan("RUN-1", &task, &plan, None)
            .unwrap()
            .py_eq(&plan));
        assert!(plan["publication_assignment"]["dispatch_runtime"].is_text("opencode-cli"));
        case.fixture
            .with_context(|context| {
                let research = planner::design_research_nodes(context.policy, 0)?;
                let branch = planner::envelope(
                    "RUN-1",
                    context.current_policy_digest,
                    context.policy,
                    &task,
                    &research[0],
                    "pending",
                    &[],
                    0,
                    Some(&plan),
                    context.files.windows(),
                )?;
                assert!(branch["dispatch_runtime"].is_text("opencode-cli"));
                Ok(())
            })
            .unwrap();
        let hint = Value::object([("host", host.into()), ("catalog_revision", 8_i64.into())]);
        let current_task = task_v2(&case, "full_delivery");
        let previous = reconstruct_execution_plan("RUN-1", &current_task, &hint, None).unwrap();
        assert!(previous["plan_digest"].is_text(digest));
        assert!(previous["model_options"].get(MUSE_MODEL).is_none());
        assert!(previous.get("external_economy_option").is_none());
        assert!(
            reconstruct_execution_plan("RUN-1", &current_task, &previous, None)
                .unwrap()
                .py_eq(&previous)
        );
        assert_eq!(
            reconstruct_execution_plan("RUN-1", &task, &hint, None).unwrap_err(),
            "MODEL_ASSIGNMENT_INVALID"
        );
    }
}

#[test]
fn test_every_route_has_exact_entry() {
    let case = HardeningCase::stock();
    case.fixture
        .with_context(|context| {
            for (route, first) in [
                ("advisory", "advisory_reviewer"),
                ("delivery_only", "senior_engineer"),
                ("fast_path", "senior_engineer"),
            ] {
                assert_eq!(
                    planner::initial_route_nodes(context.policy, route)?[0].key,
                    first
                );
            }
            for route in ["design_only", "full_delivery"] {
                let nodes = planner::initial_route_nodes(context.policy, route)?;
                assert_eq!(
                    nodes
                        .iter()
                        .map(|spec| spec.key.as_str())
                        .collect::<Vec<_>>(),
                    ["design_research_architecture", "design_research_validation"]
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_research_envelopes_split_budget_and_project_read_only_capabilities() {
    let case = HardeningCase::stock();
    let mut task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    let grants = [
        ("filesystem_read", "read", "repo:docs/"),
        ("filesystem_write", "edit", "repo:docs/"),
        ("external_read", "inspect", "andromeda"),
        ("command", "run", "npm-run-check"),
    ]
    .map(|(effect, action, target)| {
        graph_engine::domain::policy::capability(effect, action, target)
    });
    task.set(
        "authority",
        Value::object([("capabilities", Value::list(grants))]),
    );
    case.fixture
        .with_context(|context| {
            let nodes = planner::design_research_nodes(context.policy, 3)?;
            let envelopes = nodes
                .iter()
                .map(|spec| {
                    planner::envelope(
                        "RUN-1",
                        context.current_policy_digest,
                        context.policy,
                        &task,
                        spec,
                        "pending",
                        &[],
                        0,
                        None,
                        context.files.windows(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(
                envelopes
                    .iter()
                    .map(|branch| branch["research_assignment"]["focus"].text().unwrap())
                    .collect::<Vec<_>>(),
                ["architecture", "validation"]
            );
            for (key, total) in task["inspection_budget"].as_object().unwrap() {
                let key = key.utf8().unwrap();
                let used = envelopes
                    .iter()
                    .map(|branch| {
                        branch["research_assignment"]["inspection_budget"][key.as_str()]
                            .as_i64()
                            .unwrap()
                    })
                    .sum::<i64>();
                assert!(used <= total.as_i64().unwrap());
            }
            for branch in envelopes {
                let effects = branch["effect_capabilities"]
                    .as_list()
                    .unwrap()
                    .iter()
                    .map(|cap| cap["effect"].text().unwrap())
                    .collect::<BTreeSet<_>>();
                assert_eq!(
                    effects,
                    BTreeSet::from(["filesystem_read".to_owned(), "external_read".to_owned()])
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_research_node_assignments_reuse_impact_mapper() {
    let case = HardeningCase::stock();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    let plan = build_execution_plan("RUN-1", &task, Some("small"), DEFAULT_HOST).unwrap();
    case.fixture
        .with_context(|context| {
            for node in planner::design_research_nodes(context.policy, 0)? {
                assert_eq!(node.role, "impact_mapper");
                assert_selection(
                    assignment_for(&plan, &node.key).unwrap(),
                    MUSE_MODEL,
                    "xhigh",
                );
                assert!(
                    assignment_for(&plan, &node.key).unwrap()["dispatch_runtime"]
                        .is_text("opencode-cli")
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_multiple_specialists_are_canonical_and_mandatory() {
    let case = HardeningCase::stock();
    case.fixture
        .with_context(|context| {
            let tags = [
                "security_privacy".to_owned(),
                "audio_realtime_translation".to_owned(),
            ];
            let nodes = planner::design_review_nodes(context.policy, &tags, 0)?;
            assert_eq!(
                nodes[1..]
                    .iter()
                    .map(|spec| spec.specialist_tag.as_deref().unwrap())
                    .collect::<Vec<_>>(),
                ["audio_realtime_translation", "security_privacy"]
            );
            assert!(nodes.iter().all(|spec| spec.mandatory));
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_policy_and_role_bounds_cannot_add_task_authority() {
    let case = HardeningCase::stock();
    let mut task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    let mut authority = task["authority"].clone();
    let mut grants = authority["capabilities"].as_list().unwrap().to_vec();
    grants.push(graph_engine::domain::policy::capability(
        "deploy",
        "deploy",
        "production",
    ));
    authority.set("capabilities", Value::List(grants));
    task.set("authority", authority);
    let spec = NodeSpec {
        key: "senior_engineer".into(),
        role: "senior_engineer".into(),
        stage: "implementation".into(),
        generation: 0,
        mandatory: true,
        specialist_tag: None,
    };
    case.fixture
        .with_context(|context| {
            let branch = planner::envelope(
                "RUN-1",
                context.current_policy_digest,
                context.policy,
                &task,
                &spec,
                "ready",
                &[],
                0,
                None,
                context.files.windows(),
            )?;
            assert!(!branch["effect_capabilities"]
                .as_list()
                .unwrap()
                .iter()
                .any(|item| item["effect"].is_text("deploy")));
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_frozen_legacy_creation_survives_new_status_resume_without_envelope_rewrite() {
    use planner_case::{approve_plan, initialize_historical, HistoricalPlan};
    let mut case = HardeningCase::stock();
    let limits = compat::loads(br#"{"max_depth":1,"max_children_per_request":3,"max_children_per_run":6,"max_request_rounds":2,"max_weighted_dispatch_cost":15}"#).unwrap();
    let scope = Value::list(["repo:docs/technical-designs/".into()]);
    let mut policy_assignment = compat::loads(br#"{"assignment_id":"correctness","role":"security_reviewer","model":"gpt-6.1-sol","reasoning_effort":"high","review_lens":"Cross-file correctness and concurrency","prompt_template":"Review only the approved evidence and scope.","allowed_reason_codes":["CROSS_FILE"],"allowed_evidence_kinds":["finding","implementation_handoff"],"max_instances":2,"dispatch_weight":3}"#).unwrap();
    policy_assignment.set("scope_refs", scope.clone());
    case.edit_policy(|policy| {
        policy.set(
            "reviewer_delegation",
            Value::object([
                ("limits", limits.clone()),
                ("assignments", Value::list([policy_assignment])),
            ]),
        )
    });
    let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
    let mut task_limits = limits;
    task_limits.set("max_request_rounds", 1_i64.into());
    let mut task_assignment = compat::loads(br#"{"assignment_id":"correctness","allowed_reason_codes":["CROSS_FILE"],"allowed_acceptance_ids":["AC-001"],"allowed_evidence_kinds":["finding","implementation_handoff"],"max_instances":2}"#).unwrap();
    task_assignment.set("scope_refs", scope);
    task.set(
        "reviewer_delegation",
        Value::object([
            ("limits", task_limits),
            ("assignments", Value::list([task_assignment])),
        ]),
    );
    let mut authority = task["authority"].clone();
    let mut grants = authority["capabilities"].as_list().unwrap().to_vec();
    grants.extend(
        [
            ("filesystem_write", "edit", "repo:docs/technical-designs/"),
            ("filesystem_write", "edit", "repo:src/"),
            ("filesystem_read", "read", "repo:docs/technical-designs/"),
        ]
        .map(|(effect, action, target)| {
            graph_engine::domain::policy::capability(effect, action, target)
        }),
    );
    authority.set("capabilities", Value::List(grants));
    task.set("authority", authority);
    let initialized = initialize_historical(
        &case,
        &task,
        DEFAULT_HOST,
        None,
        HistoricalPlan::LegacyProjection,
    );
    approve_plan(
        &case,
        &initialized["execution_plan_digest"],
        "plan-approval-1",
    )
    .unwrap();
    case.impact("fast_path");
    let writer = case.claim();
    let writes = writer["effect_capabilities"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|cap| cap["effect"].is_text("filesystem_write"))
        .map(|cap| cap["target_ref"].text().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(writes, ["repo:src/"]);
    let before = plan_rows(&case);
    let task_before = stored_task_text(&case);
    let status = case.status().unwrap();
    let plan = &status["execution_plan"];
    assert!(plan.get("capability_projection_version").is_none());
    let assignment = &plan["conditional_review_assignments"].as_list().unwrap()[0];
    assert!(assignment["effect_capabilities"].py_eq(&Value::List(vec![])));
    assert!(initialized["execution_plan_digest"].py_eq(&plan["plan_digest"]));
    resume(&case).unwrap();
    case.status().unwrap();
    let after = plan_rows(&case);
    assert!(before.0.py_eq(&after.0));
    assert_eq!(before.1, after.1);
    assert_eq!(task_before, stored_task_text(&case));
}

fn stored_task_text(case: &HardeningCase) -> String {
    case.fixture
        .with_session(|session, _| {
            Ok(session
                .connection()
                .query_row("SELECT task_json FROM runs", [], |row| row.get(0))
                .unwrap())
        })
        .unwrap()
}

#[test]
fn test_case_identity_is_rechecked_on_status_resume_and_dispatch() {
    use planner_policy_files::CaseIdentity;
    let mut case = HardeningCase::stock();
    std::fs::create_dir(case.fixture.graph.repository.join("src")).unwrap();
    let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
    let mut authority = task["authority"].clone();
    let mut grants = authority["capabilities"].as_list().unwrap().to_vec();
    grants.push(graph_engine::domain::policy::capability(
        "filesystem_write",
        "edit",
        "repo:SRC/new.py",
    ));
    authority.set("capabilities", Value::List(grants));
    task.set("authority", authority);
    let path = case.fixture.graph.repository.join("docs/task.json");
    hardening_case::write_json(&path, &task);
    let files = CaseIdentity::new(&case);
    let init = [
        "--ack-degraded-permissions",
        "--ack-degraded-durability",
        "init",
        "--run-id",
        "RUN-1",
        "--task-brief",
        path.to_str().unwrap(),
        "--op-id",
        "init-1",
    ];
    contract_failure(
        files.run(&case, &init),
        Some("CAPABILITY_CASE_IDENTITY_UNPROVEN"),
    );
    files.prove(true);
    let initialized = files.run(&case, &init).unwrap();
    planner_case::approve_plan(
        &case,
        &initialized["execution_plan_digest"],
        "plan-approval-1",
    )
    .unwrap();
    case.impact("fast_path");
    files.run(&case, &["status", "--run-id", "RUN-1"]).unwrap();
    let before = plan_rows(&case);
    let task_before = stored_task_text(&case);
    files.prove(false);
    for args in [
        &["status", "--run-id", "RUN-1"][..],
        &[
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
            "resume",
            "--run-id",
            "RUN-1",
        ],
        &[
            "next",
            "--claim",
            "--op-id",
            "case-claim",
            "--run-id",
            "RUN-1",
        ],
    ] {
        contract_failure(
            files.run(&case, args),
            Some("CAPABILITY_CASE_IDENTITY_UNPROVEN"),
        );
    }
    let after = plan_rows(&case);
    assert!(before.0.py_eq(&after.0));
    assert_eq!(before.1, after.1);
    assert_eq!(task_before, stored_task_text(&case));
}
