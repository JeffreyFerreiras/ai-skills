#[allow(dead_code)]
#[path = "../../src/suite/task_fixture.rs"]
mod task_fixture;

use graph_engine::{
    application::{
        error::EngineResult,
        persistent_store::StateStore,
        policy::{profile_policy_path, PolicyFilePort},
        state::FaultHook,
    },
    compat::Value,
    domain::ids,
    infrastructure::{
        policy_files::NativePolicyFiles, state_database::NativeStateDatabase,
        state_platform::NativeStatePlatform,
    },
};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

struct NoFault;
impl FaultHook for NoFault {
    fn observe(&self, _: &str) -> EngineResult<()> {
        Ok(())
    }
}

static PLATFORM: NativeStatePlatform = NativeStatePlatform;
static DATABASE: NativeStateDatabase = NativeStateDatabase;
static FAULT: NoFault = NoFault;

pub struct GraphFixture {
    temporary: Option<TempDir>,
    fixture_parent: PathBuf,
    pub root: PathBuf,
    pub repository: PathBuf,
    pub skill_root: PathBuf,
    pub runtime_root: PathBuf,
    pub policy_bytes: Vec<u8>,
    pub store: StateStore<'static>,
    pub counter: usize,
}

impl GraphFixture {
    pub fn new() -> Self {
        let parent =
            PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").expect("ART required"))
                .join("fixtures");
        assert!(
            parent.is_absolute(),
            "Artifact fixtures require an absolute root"
        );
        fs::create_dir_all(&parent).unwrap();
        let files = NativePolicyFiles::default();
        let parent = files.resolve(&parent, true).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("tmp")
            .tempdir_in(&parent)
            .unwrap();
        let root = files.resolve(temporary.path(), true).unwrap();
        let repository = root.join("repo");
        fs::create_dir_all(repository.join(".codex")).unwrap();
        fs::create_dir_all(repository.join("docs/artifacts")).unwrap();
        fs::write(
            repository.join("docs/engineering-graph.md"),
            if cfg!(windows) {
                b"# Test graph evidence\r\n".as_slice()
            } else {
                b"# Test graph evidence\n".as_slice()
            },
        )
        .unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../skills/software-engineering-graph/tests/fixtures/engineering-graph.json");
        let policy = repository.join(".codex/engineering-graph.json");
        // copyfile preserves data, while destination permissions follow ordinary creation.
        let mut input = fs::File::open(source).unwrap();
        let mut copied = fs::File::create(&policy).unwrap();
        std::io::copy(&mut input, &mut copied).unwrap();
        drop(copied);
        drop(input);
        let skill_root = root.join("installed-skill");
        fs::create_dir(&skill_root).unwrap();
        let runtime_root = root.join("codex");
        let installed_policy = profile_policy_path(&files, &repository, &runtime_root).unwrap();
        fs::create_dir_all(installed_policy.parent().unwrap()).unwrap();
        fs::hard_link(&policy, installed_policy).unwrap();
        let policy_bytes = fs::read(policy).unwrap();
        let store =
            StateStore::at_root(runtime_root.clone(), &PLATFORM, &DATABASE, &FAULT).unwrap();
        Self {
            temporary: Some(temporary),
            fixture_parent: parent,
            root,
            repository,
            skill_root,
            runtime_root,
            policy_bytes,
            store,
            counter: 0,
        }
    }

    pub fn task(&self, mode: &str, route: &str, tags: &[&str]) -> Value {
        task_fixture::TaskFixture {
            policy: Value::Null,
            policy_digest: ids::sha256_bytes(&self.policy_bytes),
            oracle: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../skills/software-engineering-graph"),
        }
        .task(mode, route, tags)
    }

    pub fn tear_down(mut self) {
        self.cleanup();
    }

    fn cleanup(&mut self) {
        let Some(temporary) = self.temporary.take() else {
            return;
        };
        if !temporary.path().exists() {
            let _ = temporary.keep();
            return;
        }
        let verified = fs::canonicalize(temporary.path()).and_then(|actual| {
            fs::canonicalize(&self.fixture_parent)
                .map(|parent| actual.parent() == Some(parent.as_path()))
        });
        if !matches!(verified, Ok(true)) {
            let _ = temporary.keep();
            panic!("Preserved a fixture whose resolved path escaped its owned parent");
        }
        temporary.close().unwrap();
    }
}

impl Drop for GraphFixture {
    fn drop(&mut self) {
        self.cleanup();
    }
}
