use super::hardening_case::HardeningCase;
use graph_engine::{
    application::{
        claim_runtime::ClaimServices,
        error::EngineResult,
        graph_cli::{self, CliServices, ExecuteOutcome},
        policy::PolicyFilePort,
        snapshots::{FileSnapshotPort, Snapshot, SnapshotFailure},
    },
    compat::{PyString, Value},
    infrastructure::{
        check_runtime::NativeCheckRuntime, claim_runtime::NativeClaimRuntime,
        policy_files::NativePolicyFiles, process::NativeProcess,
        source_files::NativeSourceFileFactory, usage_files::NativeUsageSource,
    },
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(super) struct CaseIdentity {
    native: NativePolicyFiles,
    same: Cell<Result<bool, std::io::ErrorKind>>,
    calls: RefCell<Vec<(PathBuf, PathBuf)>>,
}

impl CaseIdentity {
    pub(super) fn new(case: &HardeningCase) -> Self {
        Self {
            native: NativePolicyFiles {
                environment_override: Some(BTreeMap::from([(
                    "SOFTWARE_ENGINEERING_GRAPH_HOME".to_owned(),
                    case.fixture.graph.runtime_root.to_str().unwrap().into(),
                )])),
            },
            same: Cell::new(Ok(false)),
            calls: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn prove(&self, same: bool) {
        self.same.set(Ok(same));
    }

    #[allow(dead_code)]
    pub(super) fn fail_with(&self, error: std::io::ErrorKind) {
        self.same.set(Err(error));
    }

    #[allow(dead_code)]
    pub(super) fn take_calls(&self) -> Vec<(PathBuf, PathBuf)> {
        self.calls.take()
    }

    pub(super) fn run(&self, case: &HardeningCase, args: &[&str]) -> EngineResult<Value> {
        let graph = &case.fixture.graph;
        let mut argv = vec!["--repo".into(), self.spelling(&graph.repository)];
        argv.extend(args.iter().copied().map(PyString::from));
        let source_skill =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/software-engineering-graph");
        let store = &graph.store;
        let result = graph_cli::execute(
            &argv,
            80,
            Some(store),
            &CliServices {
                files: self,
                snapshots: &self.native,
                provenance: &NativeCheckRuntime {
                    repository: &graph.repository,
                },
                skill: &source_skill,
                installation: &graph.skill_root,
                platform: store.platform,
                database: store.database,
                fault: store.fault,
                claims: ClaimServices {
                    runtime: &NativeClaimRuntime,
                    source_files: &NativeSourceFileFactory,
                    process: &NativeProcess,
                },
                usage_files: &NativeUsageSource,
            },
        )?;
        let ExecuteOutcome::Response { result, .. } = result else {
            panic!("unexpected parser exit")
        };
        Ok(result)
    }
}

impl FileSnapshotPort for CaseIdentity {
    fn snapshot(
        &self,
        path: &Path,
        roots: &[PathBuf],
        maximum: usize,
    ) -> Result<Snapshot, SnapshotFailure> {
        self.native.snapshot(path, roots, maximum)
    }
}

impl PolicyFilePort for CaseIdentity {
    fn absolute(&self, path: &Path) -> EngineResult<PathBuf> {
        self.native.absolute(path)
    }
    fn resolve(&self, path: &Path, strict: bool) -> EngineResult<PathBuf> {
        self.native.resolve(path, strict)
    }
    fn from_string(&self, text: &PyString) -> EngineResult<PathBuf> {
        self.native.from_string(text)
    }
    fn spelling(&self, path: &Path) -> PyString {
        self.native.spelling(path)
    }
    fn same_path(&self, left: &Path, right: &Path) -> bool {
        self.native.same_path(left, right)
    }
    fn under(&self, candidate: &Path, root: &Path) -> bool {
        self.native.under(candidate, root)
    }
    fn exists(&self, path: &Path, follow_links: bool) -> EngineResult<bool> {
        self.native.exists(path, follow_links)
    }
    fn is_directory(&self, path: &Path) -> EngineResult<bool> {
        self.native.is_directory(path)
    }
    fn is_file(&self, path: &Path) -> EngineResult<bool> {
        self.native.is_file(path)
    }
    fn is_symlink(&self, path: &Path) -> EngineResult<bool> {
        self.native.is_symlink(path)
    }
    fn is_reparse(&self, path: &Path) -> EngineResult<bool> {
        self.native.is_reparse(path)
    }
    fn ensure_safe(&self, path: &Path, stop: Option<&Path>) -> EngineResult<()> {
        self.native.ensure_safe(path, stop)
    }
    fn entries(&self, directory: &Path) -> EngineResult<Vec<PathBuf>> {
        self.native.entries(directory)
    }
    fn make_directory(&self, path: &Path, parents: bool, mode: u32) -> EngineResult<()> {
        self.native.make_directory(path, parents, mode)
    }
    fn chmod(&self, path: &Path, mode: u32) -> EngineResult<()> {
        self.native.chmod(path, mode)
    }
    fn write_new_sync(&self, path: &Path, bytes: &[u8], mode: u32) -> EngineResult<()> {
        self.native.write_new_sync(path, bytes, mode)
    }
    fn same_file(&self, candidate: &Path, ceiling: &Path) -> EngineResult<bool> {
        self.calls
            .borrow_mut()
            .push((candidate.into(), ceiling.into()));
        self.same
            .get()
            .map_err(|error| std::io::Error::from(error).into())
    }
    fn environment(&self, key: &str) -> Option<PyString> {
        self.native.environment(key)
    }
    fn windows(&self) -> bool {
        true
    }
}
