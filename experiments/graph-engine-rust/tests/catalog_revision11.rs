#[allow(dead_code)]
#[path = "../src/suite/task_fixture.rs"]
mod task_fixture;

use graph_engine::{
    compat::Value,
    domain::{execution, hosts},
};
use task_fixture::TaskFixture;

fn selection(model: &str, effort: &str) -> Value {
    Value::object([("model", model.into()), ("reasoning_effort", effort.into())])
}

#[test]
fn default_muse_nodes_allow_explicit_native_overrides() {
    let fixture = TaskFixture::new();
    let mut task = fixture.default_v2();
    let default =
        execution::build_execution_plan("RUN-1", &task, None, hosts::DEFAULT_HOST).unwrap();
    let nodes = [
        "impact_mapper",
        "design_research_architecture",
        "design_research_validation",
        "code_reviewer_naming",
        "test_engineer",
    ];
    task.set(
        "model_overrides",
        Value::Object(
            nodes
                .iter()
                .map(|node| ((*node).into(), selection("gpt-6.1-sol", "high")))
                .collect(),
        ),
    );
    let plan = execution::build_execution_plan("RUN-1", &task, None, hosts::DEFAULT_HOST).unwrap();
    assert!(!plan["plan_digest"].py_eq(&default["plan_digest"]));
    for node in nodes {
        let row = execution::assignment_for(&plan, node).unwrap();
        assert!(row["model"].is_text("gpt-6.1-sol"));
        assert!(row["reasoning_effort"].is_text("high"));
        assert!(row.get("dispatch_runtime").is_none());
    }
    assert!(
        execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
            .unwrap()
            .py_eq(&plan)
    );
}

#[test]
fn revision_ten_digests_and_muse_routes_remain_exact() {
    let fixture = TaskFixture::new();
    for (host, expected) in [
        (
            "codex",
            [
                "c063367b4e25d447e9765ab38d2f7617958a699e0ac294477ad98e94f080e850",
                "4b412e8e2fca633552981a4bc611197622bc46013a068e45a7ece579f2fb5b7c",
                "d5db33f276b7526accecb3a2b017f96f7b8130ff2f37dda6d96c259f06d2c6b2",
            ],
        ),
        (
            "codex-astra",
            [
                "f452eb6cb83fb1c212c04266475da557f0f9411984ef32475db5f8bf9cb732f0",
                "781923412237c46bd3984be3cc3393ce6c85d562bdc265c0f876091d4664bf4c",
                "8e51c886a598eacbf8550e0d689b9eded5f3e729ec0824d85e58b0fd0e5281e9",
            ],
        ),
    ] {
        for (index, digest) in expected.into_iter().enumerate() {
            let mut task = if index == 0 {
                fixture.default_task()
            } else {
                fixture.default_v2()
            };
            let nodes = [
                "tech_lead",
                "publication_assignment",
                "simplification_assignment",
            ];
            if index == 2 {
                task.set(
                    "model_overrides",
                    Value::Object(
                        nodes
                            .iter()
                            .map(|node| ((*node).into(), selection(hosts::MUSE_MODEL, "low")))
                            .collect(),
                    ),
                );
            }
            let stored = Value::object([
                ("host", host.into()),
                ("catalog_revision", 10_i64.into()),
                ("code_review_panel_version", 1_i64.into()),
                ("simplification_review_version", 1_i64.into()),
                ("capability_projection_version", 1_i64.into()),
            ]);
            let plan =
                execution::reconstruct_execution_plan("RUN-1", &task, &stored, None).unwrap();
            assert!(
                plan["plan_digest"].is_text(digest),
                "{host} fixture {index}"
            );
            assert!(
                execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                    .unwrap()
                    .py_eq(&plan)
            );
            if index == 2 {
                for node in nodes {
                    let row = if node == "tech_lead" {
                        execution::assignment_for(&plan, node).unwrap()
                    } else {
                        &plan[node]
                    };
                    assert!(row["dispatch_runtime"].is_text("opencode-cli"));
                }
            }
        }
    }
}

#[test]
fn revision_eleven_is_codex_only_and_preserves_fanout_weights() {
    let task = TaskFixture::new().default_v2();
    let plan = execution::build_execution_plan("RUN-1", &task, None, "codex").unwrap();
    assert_eq!(plan["catalog_revision"].as_i64(), Some(11));
    assert!(plan["model_options"]["gpt-6.1-terra"]
        .as_list()
        .unwrap()
        .iter()
        .any(|effort| effort.is_text("ultra")));
    assert!(plan["economy_fanout_option"].py_eq(&selection("gpt-6.1-sol", "low")));
    assert!(plan.get("conditional_review_assignments").is_none());
    assert_eq!(
        hosts::dispatch_weight_for("gpt-6.1-sol", "xhigh", false),
        Some(4)
    );
    for model in [hosts::MUSE_MODEL, "gpt-6.1-terra"] {
        assert_eq!(hosts::dispatch_weight_for(model, "xhigh", false), None);
    }
    let invalid = Value::object([
        ("host", "codex-astra".into()),
        ("catalog_revision", 11_i64.into()),
    ]);
    assert_eq!(
        execution::reconstruct_execution_plan("RUN-1", &task, &invalid, None).unwrap_err(),
        "CATALOG_REVISION_INVALID"
    );
}
