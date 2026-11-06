use graph_engine::{
    application::{
        claim_runtime::ClaimServices,
        error::EngineResult,
        graph_cli::{self, CliServices, ExecuteOutcome},
        persistent_store::StateStore,
        state::FaultHook,
    },
    compat::{PyString, Value},
    infrastructure::{
        check_runtime::NativeCheckRuntime, claim_runtime::NativeClaimRuntime,
        policy_files::NativePolicyFiles, process::NativeProcess,
        source_files::NativeSourceFileFactory, state_database::NativeStateDatabase,
        state_platform::NativeStatePlatform, usage_files::NativeUsageSource,
    },
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct ThreadGraph {
    pub repository: PathBuf,
    pub installation: PathBuf,
    pub state_root: PathBuf,
}

impl ThreadGraph {
    pub fn execute(&self, operation: &str, fault: &dyn FaultHook) -> EngineResult<Value> {
        let argv = vec![
            "--repo".into(),
            self.repository.to_str().unwrap().into(),
            "next".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--claim".into(),
            "--op-id".into(),
            operation.into(),
        ];
        self.execute_args(&argv, &self.state_root, fault)
    }

    pub fn execute_args(
        &self,
        argv: &[PyString],
        policy_runtime: &Path,
        fault: &dyn FaultHook,
    ) -> EngineResult<Value> {
        // Each view has the source store's root and stateless native adapters; no constructor or IO is repeated here.
        let store = StateStore {
            state_root: self.state_root.clone(),
            platform: &NativeStatePlatform,
            database: &NativeStateDatabase,
            fault,
        };
        let files = NativePolicyFiles {
            environment_override: Some(BTreeMap::from([(
                "SOFTWARE_ENGINEERING_GRAPH_HOME".to_owned(),
                policy_runtime.to_str().unwrap().into(),
            )])),
        };
        let provenance = NativeCheckRuntime {
            repository: &self.repository,
        };
        let skill =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/software-engineering-graph");
        match graph_cli::execute(
            argv,
            80,
            Some(&store),
            &CliServices {
                files: &files,
                snapshots: &files,
                provenance: &provenance,
                skill: &skill,
                installation: &self.installation,
                platform: store.platform,
                database: store.database,
                fault,
                claims: ClaimServices {
                    runtime: &NativeClaimRuntime,
                    source_files: &NativeSourceFileFactory,
                    process: &NativeProcess,
                },
                usage_files: &NativeUsageSource,
            },
        )? {
            ExecuteOutcome::Response { result, .. } => Ok(result),
            outcome => {
                panic!("unexpected parser outcome in original concurrent fixture: {outcome:?}")
            }
        }
    }
}
