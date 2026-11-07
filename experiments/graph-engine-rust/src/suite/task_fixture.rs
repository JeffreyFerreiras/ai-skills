use graph_engine::{
    compat::{self, Value},
    domain::ids,
};
use std::path::PathBuf;

pub struct TaskFixture {
    pub policy: Value,
    pub policy_digest: String,
    pub oracle: PathBuf,
}
impl TaskFixture {
    pub fn new() -> Self {
        let oracle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../skills/software-engineering-graph");
        let bytes = std::fs::read(oracle.join("tests/fixtures/engineering-graph.json")).unwrap();
        let policy = compat::loads(&bytes).unwrap();
        let policy_digest = ids::sha256_bytes(&bytes);
        Self {
            policy,
            policy_digest,
            oracle,
        }
    }
    pub fn task(&self, mode: &str, route: &str, tags: &[&str]) -> Value {
        Value::object([
            ("schema_version", 1_i64.into()),
            ("task_id", "TASK-1".into()),
            ("objective", "Exercise the graph".into()),
            ("user_outcome", "Deterministic evidence".into()),
            ("request_mode", mode.into()),
            ("minimum_route", route.into()),
            (
                "mandatory_impact_tags",
                Value::list(tags.iter().map(|&tag| tag.into())),
            ),
            (
                "scope",
                Value::object([
                    ("included", Value::list(["graph".into()])),
                    ("excluded", Value::list(["application".into()])),
                ]),
            ),
            ("constraints", Value::list(["preserve runtime".into()])),
            (
                "acceptance_criteria",
                Value::list([Value::object([
                    ("id", "AC-001".into()),
                    ("text", "Graph closes".into()),
                ])]),
            ),
            (
                "risk_level",
                if tags.is_empty() { "low" } else { "high" }.into(),
            ),
            (
                "authority",
                Value::object([(
                    "capabilities",
                    Value::list([Value::object([
                        ("effect", "filesystem_read".into()),
                        ("action", "read".into()),
                        ("target_ref", "repo:docs/".into()),
                    ])]),
                )]),
            ),
            (
                "policy_approval",
                Value::object([
                    ("sha256", self.policy_digest.clone().into()),
                    ("authority_ref", "authority:test".into()),
                ]),
            ),
            (
                "evidence_paths",
                Value::list(["repo:docs/engineering-graph.md".into()]),
            ),
            (
                "inspection_budget",
                Value::object([
                    ("file_reads", 12_i64.into()),
                    ("discovery_commands", 8_i64.into()),
                ]),
            ),
            ("required_check_ids", Value::list(["repo-check".into()])),
            ("required_human_decisions", Value::List(vec![])),
        ])
    }
    pub fn task_v2(
        &self,
        mode: &str,
        route: &str,
        tags: &[&str],
        risk: &str,
        scope: &str,
        uncertainty: &str,
    ) -> Value {
        let mut task = self.task(mode, route, tags);
        task.set("schema_version", 2_i64.into());
        task.set("risk_level", risk.into());
        task.set(
            "model_sizing",
            Value::object([
                ("scope_extent", scope.into()),
                ("uncertainty", uncertainty.into()),
            ]),
        );
        task
    }
    pub fn default_task(&self) -> Value {
        self.task("delivery", "full_delivery", &[])
    }
    pub fn default_v2(&self) -> Value {
        self.task_v2("delivery", "full_delivery", &[], "low", "bounded", "low")
    }
    pub fn delivery_only(&self) -> Value {
        let mut task = self.task_v2("delivery", "delivery_only", &[], "low", "bounded", "low");
        task.set(
            "delivery_readiness",
            Value::object([
                ("requirements_source", "JIRA-123".into()),
                ("requirements_complete", true.into()),
                ("acceptance_criteria_complete", true.into()),
                ("implementation_decisions_resolved", true.into()),
                ("architecture_decisions_resolved", true.into()),
                ("unresolved_items", Value::List(vec![])),
            ]),
        );
        task
    }
}
