use super::task_fixture::TaskFixture;
#[path = "planning_contracts.rs"]
mod contracts;
#[path = "planning_history.rs"]
mod history;
#[path = "planning_invariants.rs"]
mod invariants;
use graph_engine::{
    compat::Value,
    domain::{execution, hosts, planner, policy},
};

fn stored(host: &str, revision: Option<i64>) -> Value {
    let mut value = Value::object([("host", host.into())]);
    if let Some(revision) = revision {
        value.set("catalog_revision", revision.into());
    }
    value
}
fn rebuild(task: &Value, host: &str, revision: Option<i64>, size: Option<&str>) -> Value {
    execution::reconstruct_execution_plan("RUN-1", task, &stored(host, revision), size).unwrap()
}
fn build(task: &Value, host: &str, size: Option<&str>) -> Value {
    execution::build_execution_plan("RUN-1", task, size, host).unwrap()
}
fn assignment<'a>(plan: &'a Value, node: &str) -> &'a Value {
    execution::assignment_for(plan, node).unwrap()
}
fn pair(model: &str, effort: &str) -> Value {
    Value::object([("model", model.into()), ("reasoning_effort", effort.into())])
}
fn overrides(task: &mut Value, nodes: &[&str], model: &str, effort: &str) {
    task.set(
        "model_overrides",
        Value::Object(
            nodes
                .iter()
                .map(|&node| (node.into(), pair(model, effort)))
                .collect(),
        ),
    );
}

pub fn terra_selections() {
    let fixture = TaskFixture::new();
    for host in ["codex", "codex-astra"] {
        for effort in ["low", "medium", "high", "xhigh", "max", "ultra"] {
            let mut task = fixture.default_v2();
            overrides(
                &mut task,
                &[
                    "tech_lead",
                    "senior_engineer",
                    "impact_mapper",
                    "supervisor_recommendation",
                    "publication_assignment",
                    "simplification_assignment",
                ],
                "gpt-6.1-terra",
                effort,
            );
            let plan = build(&task, host, None);
            assert_eq!(
                plan["catalog_revision"].as_i64(),
                Some(hosts::current_revision(host).unwrap())
            );
            assert!(plan["model_options"]["gpt-6.1-terra"].py_eq(&Value::list(
                ["low", "medium", "high", "xhigh", "max", "ultra"].map(Value::from)
            )));
            for key in [
                "tech_lead",
                "senior_engineer",
                "impact_mapper",
                "supervisor_recommendation",
                "publication_assignment",
                "simplification_assignment",
            ] {
                let selection = if ["tech_lead", "senior_engineer", "impact_mapper"].contains(&key)
                {
                    assignment(&plan, key)
                } else {
                    &plan[key]
                };
                assert!(selection["model"].is_text("gpt-6.1-terra"));
                assert!(selection["reasoning_effort"].is_text(effort));
                assert!(selection["dispatch_model"].is_text("gpt-6.1-terra"));
                assert!(selection.get("dispatch_runtime").is_none());
            }
            assert!(
                execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                    .unwrap()
                    .py_eq(&plan)
            );
            assert_eq!(
                hosts::dispatch_weight_for("gpt-6.1-terra", effort, false),
                None
            );
            assert!(!hosts::supported_dispatch_weights()
                .contains_key(&("gpt-6.1-terra".into(), effort.into())));
            for &revision in hosts::supported_revisions(host) {
                if revision < 10 {
                    assert_eq!(
                        hosts::selected_dispatch_model(
                            host,
                            "gpt-6.1-terra",
                            effort,
                            Some(revision)
                        )
                        .unwrap_err(),
                        "MODEL_ASSIGNMENT_INVALID"
                    );
                }
            }
            let mut old = stored(host, Some(9));
            old.set("simplification_review_version", 1_i64.into());
            assert_eq!(
                execution::reconstruct_execution_plan("RUN-1", &task, &old, None).unwrap_err(),
                "MODEL_ASSIGNMENT_INVALID"
            );
        }
    }
}
pub fn terra_rejections() {
    let fixture = TaskFixture::new();
    for host in ["codex", "codex-astra"] {
        for effort in ["none", "minimal", "inherited", "HIGH", "unknown"] {
            let mut task = fixture.default_v2();
            overrides(&mut task, &["tech_lead"], "gpt-6.1-terra", effort);
            assert_eq!(
                execution::build_execution_plan("RUN-1", &task, None, host).unwrap_err(),
                if effort == "inherited" {
                    "MODEL_OVERRIDES_INVALID"
                } else {
                    "MODEL_ASSIGNMENT_INVALID"
                }
            );
        }
        for model in ["gpt-6.1-terra-high", "gpt-6.1-Terra", "gpt-6.1-tera"] {
            assert_eq!(
                hosts::selected_dispatch_model(host, model, "high", None).unwrap_err(),
                "MODEL_ASSIGNMENT_INVALID"
            );
        }
    }
    for host in ["claude", "cursor", "unsupported"] {
        assert_eq!(
            hosts::selected_dispatch_model(host, "gpt-6.1-terra", "high", None).unwrap_err(),
            if host == "unsupported" {
                "HOST_UNSUPPORTED"
            } else {
                "MODEL_ASSIGNMENT_INVALID"
            }
        );
    }
}
pub fn projection_version() {
    let fixture = TaskFixture::new();
    let task = fixture.default_task();
    let plan = build(&task, hosts::DEFAULT_HOST, None);
    assert_eq!(plan["capability_projection_version"].as_i64(), Some(1));
    assert!(
        execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
            .unwrap()
            .py_eq(&plan)
    );
    for marker in [
        Value::Null,
        0_i64.into(),
        2_i64.into(),
        true.into(),
        "1".into(),
    ] {
        let mut invalid = plan.clone();
        invalid.set("capability_projection_version", marker);
        assert_eq!(
            execution::reconstruct_execution_plan("RUN-1", &task, &invalid, None).unwrap_err(),
            "CAPABILITY_PROJECTION_VERSION_INVALID"
        );
    }
}
pub fn role_projection() {
    let fixture = TaskFixture::new();
    let mut task = fixture.default_task();
    let grants = vec![
        policy::capability("filesystem_write", "edit", "repo:src/slots.py"),
        policy::capability("filesystem_write", "edit", "repo:src/test_slots.py"),
    ];
    let mut authority = task["authority"]["capabilities"]
        .as_list()
        .unwrap()
        .to_vec();
    authority.extend(grants.clone());
    task.set(
        "authority",
        Value::object([("capabilities", Value::List(authority))]),
    );
    let version = 1_i64.into();
    let engineer = planner::effective_role_capabilities(
        &fixture.policy,
        &task,
        "senior_engineer",
        Some(&version),
        cfg!(windows),
    )
    .unwrap();
    let writes: Vec<_> = engineer
        .iter()
        .filter(|cap| cap["effect"].is_text("filesystem_write"))
        .cloned()
        .collect();
    assert!(Value::List(writes).py_eq(&Value::List(grants)));
    assert!(!engineer
        .iter()
        .any(|cap| cap["target_ref"].is_text("repo:src/")));
    for role in ["code_reviewer", "security_reviewer", "tech_lead"] {
        assert!(!planner::effective_role_capabilities(
            &fixture.policy,
            &task,
            role,
            Some(&version),
            cfg!(windows)
        )
        .unwrap()
        .iter()
        .any(|cap| cap["effect"].is_text("filesystem_write")));
    }
}
pub fn simplification_separate() {
    let fixture = TaskFixture::new();
    for host in hosts::HOSTS {
        for task in [
            fixture.default_task(),
            fixture.task_v2("delivery", "fast_path", &[], "low", "bounded", "low"),
            fixture.delivery_only(),
        ] {
            let plan = build(&task, host, None);
            let review = &plan["simplification_assignment"];
            let panel = assignment(&plan, "code_reviewer");
            assert_eq!(plan["simplification_review_version"].as_i64(), Some(1));
            assert!(review["role"].is_text("code_reviewer"));
            assert!(review["code_review_assignment"].py_eq(&Value::object([
                ("focus", "simplification".into()),
                ("required_skill", Value::Null)
            ])));
            assert!(review["model"].py_eq(&panel["model"]));
            assert!(review["reasoning_effort"].py_eq(&panel["reasoning_effort"]));
            assert_eq!(review["max_passes_per_generation"].as_i64(), Some(1));
            assert_eq!(
                plan["assignments"]
                    .as_list()
                    .unwrap()
                    .iter()
                    .filter(|row| row.get("code_review_assignment").is_some()
                        && !row["node_key"].is_text("advisory_reviewer"))
                    .count(),
                4
            );
            assert!(
                execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                    .unwrap()
                    .py_eq(&plan)
            );
        }
    }
}
pub fn simplification_override() {
    let fixture = TaskFixture::new();
    let mut task = fixture.default_v2();
    let before = build(&task, hosts::DEFAULT_HOST, None);
    for (model, effort) in [("gpt-6.1-sol", "medium"), (hosts::MUSE_MODEL, "low")] {
        overrides(&mut task, &["simplification_assignment"], model, effort);
        let plan = build(&task, hosts::DEFAULT_HOST, None);
        assert!(plan["simplification_assignment"]["model"].is_text(model));
        assert!(plan["simplification_assignment"]["reasoning_effort"].is_text(effort));
        assert!(!plan["plan_digest"].py_eq(&before["plan_digest"]));
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
        if model == hosts::MUSE_MODEL {
            assert!(plan["simplification_assignment"]["dispatch_runtime"].is_text("opencode-cli"));
        }
    }
    overrides(
        &mut task,
        &["simplification_assignment"],
        hosts::MUSE_MODEL,
        "max",
    );
    assert_eq!(
        execution::build_execution_plan("RUN-1", &task, None, hosts::DEFAULT_HOST).unwrap_err(),
        "MODEL_ASSIGNMENT_INVALID"
    );
}
pub fn simplification_history() {
    let fixture = TaskFixture::new();
    let task = fixture.default_v2();
    let mut old = stored(
        hosts::DEFAULT_HOST,
        Some(hosts::current_revision(hosts::DEFAULT_HOST).unwrap()),
    );
    old.set("code_review_panel_version", 1_i64.into());
    let old = execution::reconstruct_execution_plan("RUN-1", &task, &old, None).unwrap();
    assert!(old.get("simplification_assignment").is_none());
    assert!(old.get("simplification_review_version").is_none());
    assert!(
        execution::reconstruct_execution_plan("RUN-1", &task, &old, None)
            .unwrap()
            .py_eq(&old)
    );
    for (mode, route) in [("advisory", "advisory"), ("design", "design_only")] {
        let mut task = fixture.task_v2(mode, route, &[], "low", "bounded", "low");
        let plan = build(&task, hosts::DEFAULT_HOST, None);
        assert!(plan.get("simplification_assignment").is_none());
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
        overrides(
            &mut task,
            &["simplification_assignment"],
            "gpt-6.1-sol",
            "low",
        );
        assert_eq!(
            execution::build_execution_plan("RUN-1", &task, None, hosts::DEFAULT_HOST).unwrap_err(),
            "SIMPLIFICATION_REVIEW_NOT_ENABLED"
        );
    }
}
pub fn invalid_simplification() {
    let fixture = TaskFixture::new();
    for marker in [
        Value::Null,
        true.into(),
        0_i64.into(),
        2_i64.into(),
        "1".into(),
        Value::List(vec![]),
    ] {
        let stored = Value::object([("simplification_review_version", marker)]);
        assert_eq!(
            execution::reconstruct_execution_plan("RUN-1", &fixture.default_v2(), &stored, None)
                .unwrap_err(),
            "SIMPLIFICATION_REVIEW_VERSION_INVALID"
        );
    }
    assert_eq!(
        execution::reconstruct_execution_plan(
            "RUN-1",
            &fixture.task_v2("advisory", "advisory", &[], "low", "bounded", "low"),
            &Value::object([("simplification_review_version", 1_i64.into())]),
            None
        )
        .unwrap_err(),
        "SIMPLIFICATION_REVIEW_VERSION_INVALID"
    );
}
pub fn bug_hunter_skill() {
    let fixture = TaskFixture::new();
    for (host, skill) in [
        ("codex", "review-agent"),
        ("codex-astra", "review-agent"),
        ("cursor", "review-bugbot"),
        ("claude", "bug-hunter-review"),
    ] {
        let task = fixture.default_v2();
        let plan = build(&task, host, None);
        let hunter = assignment(&plan, "code_reviewer_bug_hunter");
        assert!(hunter["role"].is_text("code_reviewer"));
        assert!(hunter["code_review_assignment"].py_eq(&Value::object([
            ("focus", "bug_hunting".into()),
            ("required_skill", skill.into())
        ])));
        assert!(hunter["model"].py_eq(&assignment(&plan, "code_reviewer")["model"]));
        assert!(hunter["reasoning_effort"].is_text(if host == "codex" { "xhigh" } else { "high" }));
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
    }
}
pub fn invalid_panel() {
    let fixture = TaskFixture::new();
    for marker in [
        Value::Null,
        true.into(),
        0_i64.into(),
        2_i64.into(),
        "1".into(),
        Value::List(vec![]),
    ] {
        assert_eq!(
            execution::reconstruct_execution_plan(
                "RUN-1",
                &fixture.default_task(),
                &Value::object([("code_review_panel_version", marker)]),
                None
            )
            .unwrap_err(),
            "CODE_REVIEW_PANEL_VERSION_INVALID"
        );
    }
}
pub fn retired_new_selections() {
    let fixture = TaskFixture::new();
    for host in hosts::HOSTS {
        for model in ["gpt-5.6-sol", "gpt-5.6-luna", "gpt-5.6-terra"] {
            for revision in [None, Some(hosts::current_revision(host).unwrap())] {
                assert_eq!(
                    hosts::selected_dispatch_model(host, model, "max", revision).unwrap_err(),
                    "MODEL_RETIRED"
                );
            }
            for key in [
                "tech_lead",
                "impact_mapper",
                "design_research_validation",
                "supervisor_recommendation",
                "publication_assignment",
                "simplification_assignment",
            ] {
                let mut task = fixture.default_v2();
                overrides(&mut task, &[key], model, "max");
                assert_eq!(
                    execution::build_execution_plan("RUN-1", &task, None, host).unwrap_err(),
                    "MODEL_RETIRED"
                );
            }
        }
    }
}
pub fn recorded_revision_digests() {
    let fixture = TaskFixture::new();
    for (host, revision, digest) in [
        (
            "codex",
            7,
            "72366eef1929dba0e3b467888c0d3d3a716e82725909ea46ab94afd02f745732",
        ),
        (
            "codex-astra",
            7,
            "7af80b3eb9bcf15fa5f8da734629f2e10210790272027657aafd9fbb986733f8",
        ),
        (
            "cursor",
            3,
            "6192d520850fc5d31af8ef58d7712ffc6417a213cfffb56c6da5b486313361c5",
        ),
    ] {
        let task = fixture.default_v2();
        let plan = rebuild(&task, host, Some(revision), None);
        assert!(plan["plan_digest"].is_text(digest));
        assert!(plan["model_options"].get("gpt-5.6-sol").is_some());
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
        assert!(!build(&task, host, None)["plan_digest"].is_text(digest));
    }
}
pub fn retired_historical_override() {
    let fixture = TaskFixture::new();
    for (host, revision) in [("codex", 7), ("codex-astra", 7), ("cursor", 3)] {
        let mut task = fixture.default_v2();
        overrides(&mut task, &["tech_lead"], "gpt-5.6-sol", "medium");
        let plan = rebuild(&task, host, Some(revision), None);
        assert!(assignment(&plan, "tech_lead")["model"].is_text("gpt-5.6-sol"));
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
        assert_eq!(
            execution::build_execution_plan("RUN-1", &task, None, host).unwrap_err(),
            "MODEL_RETIRED"
        );
    }
}
pub fn current_recommendations() {
    let fixture = TaskFixture::new();
    for (host, core, scout) in [
        ("codex-astra", "gpt-6-astra", "gpt-6.1-sol"),
        ("codex", "gpt-6.1-sol", "gpt-6.1-sol"),
        ("claude", "claude-opus-5-5", "claude-sonnet-5-5"),
        ("cursor", "grok-4.7", "gemini-3.8-flash"),
    ] {
        let task = fixture.default_v2();
        let plan = build(&task, host, None);
        assert_eq!(
            plan["catalog_revision"].as_i64(),
            Some(hosts::current_revision(host).unwrap())
        );
        for node in ["tech_lead", "senior_engineer", "test_engineer"] {
            let model = if host == "codex" && node == "test_engineer" {
                hosts::MUSE_MODEL
            } else {
                core
            };
            let effort = if host == "codex" { "xhigh" } else { "medium" };
            assert!(assignment(&plan, node)["model"].is_text(model));
            assert!(assignment(&plan, node)["reasoning_effort"].is_text(effort));
        }
        assert!(plan["helper_recommendation"].py_eq(&pair(scout, "low")));
        for node in [
            "impact_mapper",
            "design_research_architecture",
            "design_research_validation",
        ] {
            let model = if host == "codex" {
                hosts::MUSE_MODEL
            } else {
                scout
            };
            let effort = if host == "codex" { "xhigh" } else { "low" };
            assert!(assignment(&plan, node)["model"].is_text(model));
            assert!(assignment(&plan, node)["reasoning_effort"].is_text(effort));
        }
        assert!(plan["publication_assignment"]["reasoning_effort"].is_text("low"));
        if ["codex", "codex-astra"].contains(&host) {
            assert!(plan["economy_fanout_option"].py_eq(&pair("gpt-6.1-sol", "low")));
            assert!(plan["external_economy_option"].py_eq(&Value::object([
                ("model", hosts::MUSE_MODEL.into()),
                ("reasoning_effort", "xhigh".into()),
                ("dispatch_runtime", "opencode-cli".into())
            ])));
            assert!(plan["model_options"][hosts::MUSE_MODEL]
                .py_eq(&Value::list(hosts::MUSE_EFFORTS.iter().map(|&e| e.into()))));
        } else {
            assert!(plan.get("economy_fanout_option").is_none());
            assert!(plan.get("external_economy_option").is_none());
            assert!(plan["model_options"].get(hosts::MUSE_MODEL).is_none());
        }
        assert!(plan["model_options"].get(core).is_some());
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
    }
    for host in hosts::HOSTS {
        let plan = build(&fixture.default_v2(), host, None);
        assert!(!plan["model_options"]
            .as_object()
            .unwrap()
            .iter()
            .any(|(model, _)| model.starts_with("gpt-5.6-")));
        assert!(!plan["assignments"]
            .as_list()
            .unwrap()
            .iter()
            .any(|row| row["model"].as_string().unwrap().starts_with("gpt-5.6-")));
    }
    assert!(
        build(&fixture.default_v2(), hosts::DEFAULT_HOST, None)["model_options"]
            .get("gpt-6-sol")
            .is_some()
    );
    assert!(
        build(&fixture.default_v2(), hosts::DEFAULT_HOST, None)["model_options"]
            .get("gpt-6.1-sol")
            .is_some()
    );
}
pub fn muse_rejections() {
    let fixture = TaskFixture::new();
    for (host, node, effort, error) in [
        ("codex", "impact_mapper", "max", "MODEL_ASSIGNMENT_INVALID"),
        ("claude", "impact_mapper", "low", "MODEL_ASSIGNMENT_INVALID"),
        ("cursor", "impact_mapper", "low", "MODEL_ASSIGNMENT_INVALID"),
        (
            "codex-astra",
            "supervisor_recommendation",
            "low",
            "SUPERVISOR_MODEL_RUNTIME_UNSUPPORTED",
        ),
    ] {
        let mut task = fixture.default_v2();
        overrides(&mut task, &[node], hosts::MUSE_MODEL, effort);
        assert_eq!(
            execution::build_execution_plan("RUN-1", &task, None, host).unwrap_err(),
            error
        );
    }
}

pub const CASES: &[(&str, fn())] = &[
    (
        "test_historical_delegation_plan_reconstructs_without_catalog_upgrade",
        invariants::historical_delegation,
    ),
    (
        "test_historical_retired_review_selection_reconstructs_but_new_plan_rejects_it",
        invariants::retired_delegated_review,
    ),
    (
        "test_astra_medium_delegation_weight_is_model_specific",
        invariants::medium_delegation_weights,
    ),
    (
        "test_astra_assignments_reject_unapproved_efforts_and_catalogs",
        invariants::astra_assignment_rejections,
    ),
    (
        "test_new_writer_validation_rejects_economy_even_with_valid_effort",
        invariants::new_writer_validation,
    ),
    (
        "test_model_assignment_invariant_fails_closed",
        invariants::assignment_invariants,
    ),
    (
        "test_ids_are_stable_across_ordering",
        invariants::stable_ids,
    ),
    (
        "test_every_host_can_expand_the_class_matrix",
        invariants::every_host_class_matrix,
    ),
    (
        "test_impact_mapper_always_uses_economy_class",
        invariants::impact_mapper_class,
    ),
    (
        "test_luna_and_design_model_invariants_hold_at_every_size",
        invariants::legacy_model_invariants,
    ),
    (
        "test_size_assignment_matrix_is_exact",
        invariants::exact_size_matrix,
    ),
    (
        "test_fanout_conflicts_require_transitive_ordering",
        invariants::transitive_fanout,
    ),
    (
        "test_fanout_rejects_cycles_and_over_capacity_antichains",
        invariants::fanout_capacity_and_cycles,
    ),
    (
        "test_fanout_case_policy_is_explicit",
        invariants::fanout_case_policy,
    ),
    (
        "test_v2_full_delivery_uses_structured_size_policy",
        contracts::structured_size_policy,
    ),
    (
        "test_v2_override_cannot_drop_below_safety_floor",
        contracts::safety_floor,
    ),
    (
        "test_sol_and_muse_are_the_default_for_every_size",
        contracts::default_codex,
    ),
    (
        "test_new_writers_are_reasoning_and_legacy_writers_remain_loadable",
        contracts::historical_writers,
    ),
    (
        "test_execution_plan_prefers_node_assignment_then_role_fallback",
        contracts::role_assignment_fallback,
    ),
    (
        "test_native_profile_defaults_match_or_require_external_dispatch",
        contracts::profile_defaults,
    ),
    (
        "test_delivery_only_plan_uses_route_aware_dispatch_metadata",
        contracts::delivery_only_metadata,
    ),
    (
        "test_revision_nine_baseline_digests_and_muse_routes_remain_exact",
        history::revision_nine_baselines,
    ),
    (
        "test_codex_revision_six_retains_approved_plan_digests",
        history::revision_six,
    ),
    (
        "test_sol_six_one_selections_reconstruct_and_reject_unsupported_efforts",
        history::sol_six_one,
    ),
    (
        "test_codex_revision_three_reconstructs_original_recommendations_and_options",
        history::revision_three,
    ),
    (
        "test_codex_revision_four_retains_approved_plan_digests",
        history::revision_four,
    ),
    (
        "test_codex_revision_five_retains_approved_plan_digests",
        history::revision_five,
    ),
    (
        "test_codex_economy_fanout_selection_is_exact_and_reconstructs",
        history::economy_fanout,
    ),
    (
        "test_alternative_models_are_not_rejected_as_nondefault",
        history::alternative_models,
    ),
    (
        "test_selected_plan_cannot_be_reinterpreted_as_historical",
        history::selected_history_rejected,
    ),
    (
        "test_v1_plan_shape_and_digest_remain_frozen",
        history::version_one_shape,
    ),
    (
        "test_host_and_supervisor_mappings_are_unchanged_for_v2",
        history::legacy_host_mappings,
    ),
    (
        "test_claude_catalog_recommends_exact_models_and_efforts",
        history::claude_current,
    ),
    (
        "test_claude_55_options_preserve_previous_catalog_digests",
        history::claude_previous_options,
    ),
    (
        "test_historical_missing_host_keeps_codex_catalog",
        history::missing_host,
    ),
    (
        "test_astra_catalog_revision_two_core_assignments_for_both_task_versions",
        history::astra_revision_two,
    ),
    (
        "test_frozen_catalog_digests_remain_exact",
        history::frozen_catalog_digests,
    ),
    (
        "test_catalog_revision_markers_fail_closed",
        history::catalog_markers,
    ),
    (
        "test_terra_selections_cover_native_assignments_without_delegation_weights",
        terra_selections,
    ),
    (
        "test_terra_rejects_invalid_efforts_names_and_hosts",
        terra_rejections,
    ),
    (
        "test_projection_version_is_approval_bound_and_unknown_versions_fail_closed",
        projection_version,
    ),
    (
        "test_role_projection_retains_exact_approved_files_under_directory_ceilings",
        role_projection,
    ),
    (
        "test_simplification_assignment_is_bound_separately_from_delivery_panel",
        simplification_separate,
    ),
    (
        "test_simplification_override_changes_digest_and_preserves_runtime",
        simplification_override,
    ),
    (
        "test_simplification_does_not_upgrade_historical_or_nondelivery_plans",
        simplification_history,
    ),
    (
        "test_invalid_simplification_version_fails_closed",
        invalid_simplification,
    ),
    (
        "test_bug_hunter_binds_host_skill_and_review_model",
        bug_hunter_skill,
    ),
    ("test_invalid_panel_version_fails_closed", invalid_panel),
    (
        "test_retired_models_cannot_enter_new_plans_or_legacy_selection_fallback",
        retired_new_selections,
    ),
    (
        "test_revision_seven_and_cursor_three_keep_recorded_digests",
        recorded_revision_digests,
    ),
    (
        "test_historical_retired_override_is_a_record_not_a_new_selection",
        retired_historical_override,
    ),
    (
        "test_current_recommendations_and_available_alternatives",
        current_recommendations,
    ),
    (
        "test_muse_rejects_unavailable_effort_host_and_supervisor",
        muse_rejections,
    ),
];
