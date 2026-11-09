use super::{assignment, build, pair, rebuild, TaskFixture};
use graph_engine::{
    compat::{PyString, Value},
    domain::{execution, hosts},
};

pub fn structured_size_policy() {
    let fixture = TaskFixture::new();
    for (risk, tags, scope, uncertainty, expected, reasons) in [
        (
            "low",
            &[][..],
            "bounded",
            "low",
            "small",
            &["bounded_low_risk_low_uncertainty"][..],
        ),
        (
            "medium",
            &[][..],
            "bounded",
            "low",
            "medium",
            &["risk_medium"][..],
        ),
        (
            "low",
            &[][..],
            "cross_file",
            "low",
            "medium",
            &["scope_cross_file"][..],
        ),
        (
            "low",
            &[][..],
            "bounded",
            "medium",
            "medium",
            &["uncertainty_medium"][..],
        ),
        (
            "low",
            &["release_operations"][..],
            "bounded",
            "low",
            "medium",
            &["mandatory_nonsecurity_impact_tag"][..],
        ),
        (
            "high",
            &[][..],
            "bounded",
            "low",
            "large",
            &["risk_high_or_critical"][..],
        ),
        (
            "critical",
            &["security_privacy"][..],
            "bounded",
            "low",
            "large",
            &["risk_high_or_critical", "security_privacy_required"][..],
        ),
        (
            "low",
            &["security_privacy"][..],
            "bounded",
            "low",
            "large",
            &["security_privacy_required"][..],
        ),
        (
            "low",
            &[][..],
            "bounded",
            "high",
            "large",
            &["uncertainty_high"][..],
        ),
        (
            "low",
            &[][..],
            "broadly_cross_cutting",
            "low",
            "large",
            &["scope_broadly_cross_cutting"][..],
        ),
    ] {
        let task = fixture.task_v2("delivery", "full_delivery", tags, risk, scope, uncertainty);
        let plan = build(&task, hosts::DEFAULT_HOST, None);
        assert!(plan["size"].is_text(expected));
        assert_eq!(plan["size_policy_version"].as_i64(), Some(2));
        assert!(plan["size_recommendation_reason_codes"].py_eq(&Value::list(
            reasons.iter().map(|&reason| Value::from(reason))
        )));
        let mut keys: Vec<_> = plan["size_recommendation_inputs"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, _)| key.clone())
            .collect();
        keys.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            keys,
            ["mandatory_impact_tags", "model_sizing", "risk_level"].map(PyString::from)
        );
        assert!(plan["minimum_route"].is_text("full_delivery"));
    }
}

pub fn safety_floor() {
    let fixture = TaskFixture::new();
    for (task, size) in [
        (
            fixture.task_v2("delivery", "full_delivery", &[], "high", "bounded", "low"),
            "medium",
        ),
        (
            fixture.task_v2("delivery", "full_delivery", &[], "low", "cross_file", "low"),
            "small",
        ),
    ] {
        assert_eq!(
            execution::build_execution_plan("RUN-1", &task, Some(size), hosts::DEFAULT_HOST)
                .unwrap_err(),
            "EXECUTION_SIZE_BELOW_SAFETY_FLOOR"
        );
    }
    assert!(
        build(&fixture.default_v2(), hosts::DEFAULT_HOST, Some("large"))["size"].is_text("large")
    );
}

pub fn default_codex() {
    let fixture = TaskFixture::new();
    let muse_nodes = [
        "impact_mapper",
        "design_research_architecture",
        "design_research_validation",
        "code_reviewer_naming",
        "test_engineer",
    ];
    for size in ["small", "medium", "large"] {
        let task = fixture.default_task();
        let plan = build(&task, hosts::DEFAULT_HOST, Some(size));
        assert!(plan.py_eq(&build(&task, "codex", Some(size))));
        assert!(plan["host"].is_text("codex"));
        assert_eq!(plan["catalog_revision"].as_i64(), Some(11));
        for row in plan["assignments"].as_list().unwrap() {
            if row["role"].is_text("supervisor") {
                assert!(row["model"].is_text("primary-thread"));
                assert!(row["reasoning_effort"].is_text("inherited"));
                continue;
            }
            let external = muse_nodes.iter().any(|node| row["node_key"].is_text(node));
            let model = if external {
                hosts::MUSE_MODEL
            } else {
                "gpt-6.1-sol"
            };
            assert!(row["model"].is_text(model));
            assert!(row["reasoning_effort"].is_text("xhigh"));
            assert!(row["dispatch_model"].is_text(model));
            if external {
                assert!(row["dispatch_runtime"].is_text("opencode-cli"));
            } else {
                assert!(row.get("dispatch_runtime").is_none());
            }
        }
        assert!(plan["supervisor_recommendation"]["model"].is_text("gpt-6.1-sol"));
        assert!(plan["supervisor_recommendation"]["reasoning_effort"].is_text("xhigh"));
        assert!(plan["simplification_assignment"]["reasoning_effort"].is_text("xhigh"));
        assert!(plan["publication_assignment"]["model"].is_text("gpt-6.1-sol"));
        assert!(plan["publication_assignment"]["reasoning_effort"].is_text("low"));
        assert!(plan["helper_recommendation"].py_eq(&pair("gpt-6.1-sol", "low")));
        assert!(plan["economy_fanout_option"].py_eq(&pair("gpt-6.1-sol", "low")));
        assert!(plan.get("conditional_review_assignments").is_none());
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &task, &plan, Some(size))
                .unwrap()
                .py_eq(&plan)
        );
    }
}

pub fn historical_writers() {
    let fixture = TaskFixture::new();
    for host in hosts::HOSTS {
        for task in [fixture.default_task(), fixture.default_v2()] {
            for size in ["small", "medium", "large"] {
                let new = rebuild(
                    &task,
                    host,
                    Some(if *host == "claude" { 1 } else { 2 }),
                    Some(size),
                );
                assert!(
                    assignment(&new, "senior_engineer")["intelligence_class"].is_text("reasoning")
                );
                let legacy = rebuild(&task, host, None, Some(size));
                assert!(
                    execution::reconstruct_execution_plan("RUN-1", &task, &legacy, Some(size))
                        .unwrap()
                        .py_eq(&legacy)
                );
                assert!(
                    assignment(&legacy, "senior_engineer")["intelligence_class"].is_text(
                        if size == "small" {
                            "economy"
                        } else {
                            "reasoning"
                        }
                    )
                );
                for node in new["assignments"].as_list().unwrap() {
                    if !node["node_key"].is_text("senior_engineer") && *host != "codex-astra" {
                        let key = node["node_key"].as_string().unwrap().utf8().unwrap();
                        assert!(node.py_eq(assignment(&legacy, &key)));
                    }
                }
            }
        }
    }
}

pub fn role_assignment_fallback() {
    let fixture = TaskFixture::new();
    for (size, advisory_model, advisory_effort) in [
        ("small", "gpt-5.6-luna", "max"),
        ("medium", "gpt-5.6-sol", "medium"),
        ("large", "gpt-5.6-sol", "high"),
    ] {
        let plan = rebuild(&fixture.default_task(), "codex", None, Some(size));
        for (key, model, effort) in [
            ("advisory_reviewer", advisory_model, advisory_effort),
            (
                "supervisor_design_consolidation",
                "primary-thread",
                "inherited",
            ),
            (
                "supervisor_delivery_consolidation",
                "primary-thread",
                "inherited",
            ),
        ] {
            let row = plan["assignments"]
                .as_list()
                .unwrap()
                .iter()
                .find(|row| row["node_key"].is_text(key))
                .unwrap();
            assert!(row["model"].is_text(model));
            assert!(row["reasoning_effort"].is_text(effort));
        }
    }
}

pub fn profile_defaults() {
    let fixture = TaskFixture::new();
    let plan = build(&fixture.default_task(), hosts::DEFAULT_HOST, Some("medium"));
    for (profile, key) in [
        ("impact_mapper", "impact_mapper"),
        ("tech_lead", "tech_lead"),
        ("software_architect", "architect"),
        ("senior_engineer", "senior_engineer"),
        ("code_reviewer", "code_reviewer"),
        ("test_engineer", "test_engineer"),
        ("security_reviewer", "security_reviewer"),
    ] {
        let text = std::fs::read_to_string(
            fixture
                .oracle
                .join("profile-agents")
                .join(format!("{profile}.toml")),
        )
        .unwrap();
        let effort = text
            .lines()
            .find(|line| line.starts_with("model_reasoning_effort = "))
            .unwrap()
            .split('"')
            .nth(1)
            .unwrap();
        let model = text
            .lines()
            .find(|line| line.starts_with("model = "))
            .unwrap()
            .split('"')
            .nth(1)
            .unwrap();
        let row = plan["assignments"]
            .as_list()
            .unwrap()
            .iter()
            .find(|row| row["node_key"].is_text(key))
            .unwrap();
        if row["dispatch_runtime"].is_text("opencode-cli") {
            assert!(text.contains("Codex graph revision 11 uses Muse through OpenCode CLI."));
            assert_eq!(model, "gpt-6.1-sol");
            continue;
        }
        assert!(row["model"].is_text(model), "{profile}");
        assert!(row["reasoning_effort"].is_text(effort), "{profile}");
    }
}

pub fn delivery_only_metadata() {
    let fixture = TaskFixture::new();
    let plan = build(&fixture.delivery_only(), hosts::DEFAULT_HOST, None);
    for key in ["senior_engineer", "code_reviewer", "test_engineer"] {
        let row = plan["assignments"]
            .as_list()
            .unwrap()
            .iter()
            .find(|row| row["node_key"].is_text(key))
            .unwrap();
        assert!(row["dispatch_when"].is_text("delivery_only or full_delivery route"));
    }
}
