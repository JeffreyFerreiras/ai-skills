#[path = "initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::error::EngineResult,
    compat::{self, PyString, SerializationProfile, Value},
    domain::ids,
};
use initialized_graph::InitializedGraph;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct HardeningCase {
    pub(super) fixture: InitializedGraph,
    counter: usize,
}

pub(super) fn write_json(path: &Path, value: &Value) -> Vec<u8> {
    let bytes = compat::dumps(value, SerializationProfile::STORED).unwrap();
    fs::write(path, &bytes).unwrap();
    bytes
}

impl HardeningCase {
    pub(super) fn new() -> Self {
        Self::configured(true)
    }

    pub(super) fn golden() -> Self {
        Self::configured(false)
    }

    pub(super) fn stock() -> Self {
        Self {
            fixture: InitializedGraph::new(),
            counter: 0,
        }
    }

    fn configured(hardening: bool) -> Self {
        let mut case = Self::stock();
        case.edit_policy(|policy| {
            if hardening {
                let mut limits = policy["limits"].clone();
                limits.set("branch_lease_seconds", 30_i64.into());
                policy.set("limits", limits);
            }
            let python =
                std::env::var("GRAPH_RUST_ORACLE_PYTHON").expect("fixture interpreter required");
            let mut checks = policy["required_checks"].clone();
            let mut repo_check = checks["repo-check"].clone();
            repo_check.set(
                "argv",
                Value::List(vec![
                    python.into(),
                    "-c".into(),
                    "import sys; sys.exit(0)".into(),
                ]),
            );
            checks.set("repo-check", repo_check);
            policy.set("required_checks", checks);
        });
        case
    }

    pub(super) fn edit_policy(&mut self, edit: impl FnOnce(&mut Value)) {
        let mut policy = compat::loads(&self.fixture.graph.policy_bytes).unwrap();
        edit(&mut policy);
        let path = self
            .fixture
            .graph
            .repository
            .join(".codex/engineering-graph.json");
        self.fixture.graph.policy_bytes = write_json(&path, &policy);
    }

    pub(super) fn run(&self, arguments: &[&str]) -> EngineResult<Value> {
        let argv = arguments
            .iter()
            .map(|arg| PyString::from(*arg))
            .collect::<Vec<_>>();
        self.fixture.graphctl(&argv)
    }

    pub(super) fn status(&self) -> EngineResult<Value> {
        self.run(&["status", "--run-id", "RUN-1"])
    }

    pub(super) fn evidence(&self, name: &str, payload: &Value) -> (String, String) {
        let relative = format!("docs/artifacts/{name}.json");
        let path = self.fixture.graph.repository.join(&relative);
        let bytes = write_json(&path, payload);
        let digest = ids::sha256_bytes(&bytes);
        (format!("repo:{relative}#sha256={digest}"), digest)
    }

    pub(super) fn repo_artifact(&mut self, kind: &str, label: &str) -> Value {
        self.counter += 1;
        let (reference, digest) = self.evidence(
            label,
            &Value::object([
                ("schema_version", 1_i64.into()),
                ("kind", kind.into()),
                ("label", label.into()),
            ]),
        );
        Value::object([
            ("kind", kind.into()),
            ("ref", reference.into()),
            ("sha256", digest.into()),
        ])
    }

    pub(super) fn inbox(&mut self, manifest: &Value) -> PathBuf {
        self.counter += 1;
        let path = self
            .fixture
            .graph
            .store
            .inbox_root("albanian-live-translate", "RUN-1")
            .join(format!("manifest-{}.json", self.counter));
        write_json(&path, manifest);
        self.fixture
            .graph
            .store
            .platform
            .set_mode(&path, 0o600)
            .unwrap();
        path
    }

    pub(super) fn claim_raw(&mut self) -> EngineResult<Value> {
        let status = self.status()?;
        let action = &status["next_action"];
        if action["kind"].is_text("record_fanout_assessment") {
            self.assess(&action["fanout_id"].text().unwrap());
        }
        self.counter += 1;
        let op = format!("claim-{}", self.counter);
        Ok(self.run(&["next", "--run-id", "RUN-1", "--claim", "--op-id", &op])?["branch"].clone())
    }

    fn record(&mut self, branch: &Value, manifest: Value) {
        self.record_as(branch, manifest, "result").unwrap();
    }

    pub(super) fn record_as(
        &mut self,
        branch: &Value,
        mut manifest: Value,
        prefix: &str,
    ) -> EngineResult<Value> {
        self.counter += 1;
        let token = branch["claim_token"].text().unwrap();
        manifest.set("attempt_id", branch["attempt_id"].clone());
        manifest.set("claim_digest", ids::sha256_bytes(token.as_bytes()).into());
        let path = self.inbox(&manifest);
        let op = format!("{prefix}-{}", self.counter);
        self.run(&[
            "record",
            "branch-result",
            "--run-id",
            "RUN-1",
            "--branch-id",
            &branch["branch_id"].text().unwrap(),
            "--attempt-id",
            &branch["attempt_id"].text().unwrap(),
            "--claim-token",
            &token,
            "--result-manifest",
            path.to_str().unwrap(),
            "--op-id",
            &op,
        ])
    }

    pub(super) fn assess(&mut self, fanout_id: &str) {
        self.assess_ordered(fanout_id, vec![]);
    }

    pub(super) fn assess_ordered(&mut self, fanout_id: &str, dependencies: Vec<Value>) -> Value {
        let status = self.status().unwrap();
        let fanout = status["fanouts"]
            .as_list()
            .unwrap()
            .iter()
            .find(|row| row["fanout_id"].is_text(fanout_id))
            .unwrap();
        let label = format!("fanout-evidence-{}", self.counter);
        let evidence = self.repo_artifact("finding", &label);
        let resources = Value::object([
            ("writable_paths", Value::List(vec![])),
            ("mutable_state_refs", Value::List(vec![])),
            ("exclusive_device_refs", Value::List(vec![])),
            ("services", Value::List(vec![])),
        ]);
        let manifest =
            Value::object([
                ("schema_version", 1_i64.into()),
                ("kind", "fanout_assessment".into()),
                ("run_id", "RUN-1".into()),
                ("fanout_id", fanout_id.into()),
                (
                    "members",
                    Value::list(fanout["member_branch_ids"].as_list().unwrap().iter().map(
                        |member| {
                            Value::object([
                                ("branch_id", member.clone()),
                                ("resources", resources.clone()),
                            ])
                        },
                    )),
                ),
                ("dependencies", Value::List(dependencies)),
                ("evidence", Value::List(vec![evidence])),
            ]);
        self.counter += 1;
        let path = self.inbox(&manifest);
        let op = format!("fanout-assessment-{}", self.counter);
        self.run(&[
            "record",
            "fanout-assessment",
            "--run-id",
            "RUN-1",
            "--fanout-id",
            fanout_id,
            "--assessment-manifest",
            path.to_str().unwrap(),
            "--authority-ref",
            "authority:test",
            "--op-id",
            &op,
        ])
        .unwrap()
    }

    pub(super) fn success(&mut self, branch: &Value) {
        self.success_with(branch, None, vec![]).unwrap();
    }

    pub(super) fn success_with(
        &mut self,
        branch: &Value,
        decision: Option<&str>,
        findings: Vec<Value>,
    ) -> EngineResult<Value> {
        let id = branch["branch_id"].text().unwrap();
        let kind = branch["output_contract"]["artifact_kind"].text().unwrap();
        let artifact = self.repo_artifact(&kind, &id);
        let mut evidence = Vec::new();
        if branch["node_key"].is_text("design_research_architecture")
            || branch["node_key"].is_text("design_research_validation")
        {
            evidence.push(self.repo_artifact("finding", &format!("{id}-evidence")));
        }
        let mut manifest = Value::object([
            ("schema_version", 1_i64.into()),
            ("run_id", "RUN-1".into()),
            ("branch_id", id.into()),
            ("status", "succeeded".into()),
            ("output_kind", kind.into()),
            ("artifact_ref", artifact),
            ("evidence", Value::List(evidence)),
            ("findings", Value::List(findings)),
        ]);
        let decision = decision.or_else(|| {
            branch["output_contract"]["decision_values"]
                .truthy()
                .then_some("APPROVE")
        });
        if let Some(decision) = decision {
            manifest.set("decision", decision.into());
        }
        self.record_as(branch, manifest, "result")
    }

    fn drain_research(&mut self) {
        for _ in 0..6 {
            let status = self.status().unwrap();
            let action = &status["next_action"];
            if action["kind"].is_text("record_fanout_assessment")
                && action["stage"].is_text("research")
            {
                self.assess(&action["fanout_id"].text().unwrap());
                continue;
            }
            let research = status["branches"]
                .as_list()
                .unwrap()
                .iter()
                .filter(|branch| branch["stage"].is_text("research"))
                .collect::<Vec<_>>();
            if research
                .iter()
                .any(|branch| branch["status"].is_text("ready"))
            {
                let branch = self.claim_raw().unwrap();
                self.success(&branch);
                continue;
            }
            let terminal = |branch: &&Value| {
                ["succeeded", "failed", "timed_out", "skipped"]
                    .iter()
                    .any(|status| branch["status"].is_text(status))
            };
            if !research.is_empty() && research.iter().all(terminal) {
                if let Some(join) = status["joins"].as_list().unwrap().iter().find(|join| {
                    join["join_key"].is_text("research_collection")
                        && join["status"].is_text("open")
                }) {
                    self.counter += 1;
                    let status = self.status().unwrap();
                    let current = status["joins"]
                        .as_list()
                        .unwrap()
                        .iter()
                        .find(|current| current["join_id"].py_eq(&join["join_id"]))
                        .unwrap();
                    self.run(&[
                        "join",
                        "advance",
                        "--run-id",
                        "RUN-1",
                        "--join-id",
                        &current["join_id"].text().unwrap(),
                        "--op-id",
                        &format!("join-{}", self.counter),
                    ])
                    .unwrap();
                }
            }
            return;
        }
        panic!("research fixture did not settle");
    }

    pub(super) fn claim(&mut self) -> Value {
        self.drain_research();
        self.claim_raw().unwrap()
    }

    pub(super) fn impact(&mut self, route: &str) {
        self.impact_with_tags(route, &[]);
    }

    pub(super) fn impact_with_tags(&mut self, route: &str, tags: &[&str]) {
        let mapper = self.claim();
        let label = format!("impact-evidence-{}", self.counter);
        let evidence = self.repo_artifact("finding", &label);
        let mut tags = tags.to_vec();
        tags.sort();
        self.record(
            &mapper,
            Value::object([
                ("schema_version", 1_i64.into()),
                ("task_id", "TASK-1".into()),
                ("route_label", route.into()),
                (
                    "impact_tags",
                    Value::list(tags.into_iter().map(Value::from)),
                ),
                ("evidence_refs", Value::List(vec![evidence["ref"].clone()])),
            ]),
        );
    }

    pub(super) fn record_mapper(&mut self, branch: &Value, tags: &[&str]) {
        let evidence = self.repo_artifact("finding", "impact-evidence");
        let mut tags = tags.to_vec();
        tags.sort();
        self.record_as(
            branch,
            Value::object([
                ("schema_version", 1_i64.into()),
                ("task_id", "TASK-1".into()),
                ("route_label", "full_delivery".into()),
                (
                    "impact_tags",
                    Value::list(tags.into_iter().map(Value::from)),
                ),
                ("evidence_refs", Value::List(vec![evidence["ref"].clone()])),
            ]),
            "mapper-result",
        )
        .unwrap();
    }

    pub(super) fn open_join(&self, key: &str) -> Value {
        self.open_join_at(key, 0)
    }

    pub(super) fn open_join_at(&self, key: &str, generation: i64) -> Value {
        let status = self.status().unwrap();
        status["joins"]
            .as_list()
            .unwrap()
            .iter()
            .find(|join| {
                join["join_key"].is_text(key)
                    && join["status"].is_text("open")
                    && join["generation"].py_eq(&generation.into())
            })
            .unwrap()
            .clone()
    }

    pub(super) fn advance(&mut self, key: &str) -> Value {
        self.advance_at(key, 0)
    }

    pub(super) fn advance_at(&mut self, key: &str, generation: i64) -> Value {
        self.counter += 1;
        let join = self.open_join_at(key, generation);
        self.run(&[
            "join",
            "advance",
            "--run-id",
            "RUN-1",
            "--join-id",
            &join["join_id"].text().unwrap(),
            "--op-id",
            &format!("join-{}", self.counter),
        ])
        .unwrap()
    }

    pub(super) fn consolidate(
        &mut self,
        stage: &str,
        outcome: &str,
        generation: i64,
        dispositions: Vec<Value>,
    ) {
        let branch = self.claim();
        assert!(branch["node_key"].is_text(&format!("supervisor_{stage}_consolidation")));
        let collection_input = branch["inputs"]
            .as_list()
            .unwrap()
            .iter()
            .find(|input| input["kind"].is_text("collection"))
            .unwrap();
        let collection = &collection_input["content"];
        let sources = Value::list(
            collection["members"]
                .as_list()
                .unwrap()
                .iter()
                .map(|member| member["branch_id"].clone()),
        );
        self.record(
            &branch,
            Value::object([
                ("schema_version", 1_i64.into()),
                ("kind", format!("{stage}_consolidation").into()),
                ("run_id", "RUN-1".into()),
                ("join_id", collection["join_id"].clone()),
                ("generation", generation.into()),
                ("source_branch_ids", sources),
                ("finding_dispositions", Value::List(dispositions)),
                ("outcome", outcome.into()),
            ]),
        );
    }

    pub(super) fn counter(&self) -> usize {
        self.counter
    }
}
