use graph_engine::{
    application::persistent_store::InitializeRequest,
    compat::{self, Value},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone)]
pub struct Fixture {
    pub root: PathBuf,
    pub repository: PathBuf,
    pub policy: Value,
    pub task: Value,
    pub node: Value,
    pub plan: Value,
    pub task_ref: Value,
}
fn json(text: &str) -> Value {
    compat::loads_text(&text.into()).unwrap()
}
impl Fixture {
    pub fn new(label: &str) -> Self {
        let fixtures =
            PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").expect("ART required"))
                .join("fixtures");
        std::fs::create_dir_all(&fixtures).unwrap();
        let root = loop {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let candidate = fixtures.join(format!(
                "persistent-{label}-{}-{nonce}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create fresh persistent fixture: {error}"),
            }
        };
        let repository = root.join("repo");
        std::fs::create_dir(&repository).unwrap();
        Self {
            root,
            repository,
            policy: json(
                r#"{"repository_id":"fixture-repository","limits":{"design_revisions":2,"delivery_repairs":3}}"#,
            ),
            task: json(
                r#"{"request_mode":"delivery","minimum_route":"full_delivery","inspection_budget":{"file_reads":12,"discovery_commands":8}}"#,
            ),
            node: json(
                r#"{"branch_id":"bootstrap-branch","node_instance_id":"bootstrap-instance","node_key":"bootstrap","role":"tech_lead","stage":"bootstrap","generation":1,"mandatory":true,"status":"ready","retry_count":0,"max_retries":0,"envelope_json":"{}"}"#,
            ),
            plan: Value::object([
                ("size", "small".into()),
                ("plan_digest", "f".repeat(64).into()),
            ]),
            task_ref: "task-ref".into(),
        }
    }
    pub fn request(&self) -> InitializeRequest<'_> {
        InitializeRequest {
            repository: &self.repository,
            policy: &self.policy,
            policy_digest: "policy-digest",
            task: &self.task,
            task_path: &self.repository,
            task_digest: "task-digest",
            run_id: "RUN-1",
            operation_id: "init-1",
            request_digest: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            acknowledge_permissions: true,
            acknowledge_durability: true,
            bootstrap: &self.node,
            execution_plan: &self.plan,
            task_ref: &self.task_ref,
            initial_artifacts: &[],
        }
    }
}
