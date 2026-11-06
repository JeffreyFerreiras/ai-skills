#[path = "graph_case.rs"]
mod graph_case;

use graph_case::GraphFixture;
use graph_engine::{
    application::{
        claim_runtime::ClaimServices,
        error::EngineResult,
        graph_cli::{self, CliServices, ExecuteOutcome},
        persistent_store::StateStore,
        policy::{self, PolicyFilePort},
        process::ProcessPort,
        semantic_state::SemanticContext,
    },
    compat::{self, PyString, SerializationProfile, Value},
    infrastructure::{
        check_runtime::NativeCheckRuntime, claim_runtime::NativeClaimRuntime,
        policy_files::NativePolicyFiles, process::NativeProcess,
        source_files::NativeSourceFileFactory, sqlite::SqliteSession,
        usage_files::NativeUsageSource,
    },
};
use std::{collections::BTreeMap, fs, path::Path};

pub struct InitializedGraph {
    pub graph: GraphFixture,
}

impl InitializedGraph {
    pub fn new() -> Self {
        Self {
            graph: GraphFixture::new(),
        }
    }

    pub fn with_context<T>(
        &self,
        action: impl FnOnce(&SemanticContext<'_>) -> EngineResult<T>,
    ) -> EngineResult<T> {
        // GraphCase patches config.resolve_state_root to this owned runtime home.
        let files = NativePolicyFiles {
            environment_override: Some(BTreeMap::from([(
                "SOFTWARE_ENGINEERING_GRAPH_HOME".to_owned(),
                self.graph.runtime_root.to_str().unwrap().into(),
            )])),
        };
        let (policy, snapshot) = policy::load_policy(
            &files,
            &self.graph.repository,
            &self.graph.skill_root,
            Some(&self.graph.runtime_root),
        )?;
        let source_skill = files.resolve(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/software-engineering-graph"),
            true,
        )?;
        let provenance = NativeCheckRuntime {
            repository: &self.graph.repository,
        };
        action(&SemanticContext {
            store: &self.graph.store,
            files: &files,
            snapshots: &files,
            provenance: &provenance,
            repository: &self.graph.repository,
            skill: &source_skill,
            installation: &self.graph.skill_root,
            runtime: Some(&self.graph.runtime_root),
            policy: &policy,
            current_policy_digest: &snapshot.digest,
            case_sensitive: !cfg!(windows),
            evidence_enable: false,
        })
    }

    pub fn initialize(&self, task: &Value, approve: bool) -> EngineResult<Value> {
        let task_path = self.graph.repository.join("docs/task.json");
        fs::write(
            &task_path,
            compat::dumps(task, SerializationProfile::STORED)?,
        )?;
        let files = NativePolicyFiles::default();
        let result = self.graphctl(&[
            "--ack-degraded-permissions".into(),
            "--ack-degraded-durability".into(),
            "init".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--task-brief".into(),
            files.spelling(&task_path),
            "--op-id".into(),
            "init-1".into(),
        ])?;
        if approve {
            self.graphctl(&[
                "record".into(),
                "plan-approval".into(),
                "--run-id".into(),
                "RUN-1".into(),
                "--plan-digest".into(),
                result["execution_plan_digest"].as_string().unwrap().clone(),
                "--decision".into(),
                "APPROVE".into(),
                "--authority-ref".into(),
                "authority:test".into(),
                "--op-id".into(),
                "plan-approval-1".into(),
            ])?;
        }
        Ok(result)
    }

    pub fn execute(&self, arguments: &[PyString]) -> EngineResult<ExecuteOutcome> {
        self.execute_with_store(arguments, &self.graph.store)
    }

    pub fn execute_with_store(
        &self,
        arguments: &[PyString],
        store: &StateStore<'_>,
    ) -> EngineResult<ExecuteOutcome> {
        self.execute_with_usage_source(arguments, store, &NativeUsageSource)
    }

    pub fn execute_with_usage_source(
        &self,
        arguments: &[PyString],
        store: &StateStore<'_>,
        usage_files: &dyn graph_engine::application::usage_source::UsageSourcePort,
    ) -> EngineResult<ExecuteOutcome> {
        self.execute_with_ports(arguments, store, usage_files, &NativeProcess)
    }

    pub fn execute_with_process(
        &self,
        arguments: &[PyString],
        store: &StateStore<'_>,
        process: &dyn ProcessPort,
    ) -> EngineResult<ExecuteOutcome> {
        self.execute_with_ports(arguments, store, &NativeUsageSource, process)
    }

    fn execute_with_ports(
        &self,
        arguments: &[PyString],
        store: &StateStore<'_>,
        usage_files: &dyn graph_engine::application::usage_source::UsageSourcePort,
        process: &dyn ProcessPort,
    ) -> EngineResult<ExecuteOutcome> {
        let files = NativePolicyFiles {
            environment_override: Some(BTreeMap::from([(
                "SOFTWARE_ENGINEERING_GRAPH_HOME".to_owned(),
                self.graph.runtime_root.to_str().unwrap().into(),
            )])),
        };
        let provenance = NativeCheckRuntime {
            repository: &self.graph.repository,
        };
        let skill =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/software-engineering-graph");
        graph_cli::execute(
            arguments,
            80,
            Some(store),
            &CliServices {
                files: &files,
                snapshots: &files,
                provenance: &provenance,
                skill: &skill,
                installation: &self.graph.skill_root,
                platform: store.platform,
                database: store.database,
                fault: store.fault,
                claims: ClaimServices {
                    runtime: &NativeClaimRuntime,
                    source_files: &NativeSourceFileFactory,
                    process,
                },
                usage_files,
            },
        )
    }

    pub fn graphctl(&self, arguments: &[PyString]) -> EngineResult<Value> {
        let files = NativePolicyFiles::default();
        let mut argv = vec!["--repo".into(), files.spelling(&self.graph.repository)];
        argv.extend_from_slice(arguments);
        match self.execute(&argv)? {
            ExecuteOutcome::Response { result, .. } => Ok(result),
            ExecuteOutcome::ParserExit {
                exit_code,
                stdout,
                stderr,
            } => {
                panic!("fixture parser exited {exit_code}: {stdout:?} {stderr:?}")
            }
        }
    }

    pub fn with_session<T>(
        &self,
        action: impl FnOnce(&SqliteSession, &SemanticContext<'_>) -> EngineResult<T>,
    ) -> EngineResult<T> {
        self.with_context(|context| {
            let repository = context.policy["repository_id"]
                .text()
                .expect("validated fixture repository ID");
            let database = self.graph.store.db_path(&repository, "RUN-1");
            let session = SqliteSession::connect_existing(&database)?;
            action(&session, context)
        })
    }

    pub fn tear_down(self) {
        self.graph.tear_down();
    }
}
