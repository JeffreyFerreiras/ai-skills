use graph_engine::{
    application::{
        error::EngineError,
        policy::{self, PolicyFilePort},
    },
    compat::{PyString, Value},
    domain::{contracts, policy_validation},
    infrastructure::policy_files::NativePolicyFiles,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    repo: PathBuf,
    skill: PathBuf,
    home: PathBuf,
    default_home: PathBuf,
    files: NativePolicyFiles,
}
impl Fixture {
    fn new() -> Self {
        let art =
            PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").expect("ART required"));
        let base = art.join("tests/policy").join(format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = base.join("repo");
        let skill = base.join("skills/software-engineering-graph");
        let home = base.join("selected-home");
        let default_home = base.join("default-home");
        for path in [
            repo.join("docs"),
            repo.join("src"),
            repo.join(".codex"),
            skill.clone(),
        ] {
            std::fs::create_dir_all(path).unwrap();
        }
        for name in ["AGENTS.md", "top.rs", "credentials.txt"] {
            std::fs::write(repo.join(name), b"fixture\n").unwrap();
        }
        let mut files = NativePolicyFiles::default();
        files.environment_override = Some(BTreeMap::from([(
            "SOFTWARE_ENGINEERING_GRAPH_HOME".into(),
            graph_engine::infrastructure::filesystem::os_string(default_home.as_os_str()),
        )]));
        Self {
            repo,
            skill,
            home,
            default_home,
            files,
        }
    }
}
fn assert_contract(error: EngineError, field: &str, code: &str) {
    match error {
        EngineError::Contract(error) => {
            assert!(error.field.equals(field));
            assert_eq!(error.code, code);
        }
        other => panic!("expected {field}:{code}, got {other:?}"),
    }
}
#[test]
fn policy_creation_and_each_load_recheck_current_bytes() {
    let fixture = Fixture::new();
    let (value, snapshot) = policy::load_policy(
        &fixture.files,
        &fixture.repo,
        &fixture.skill,
        Some(&fixture.home),
    )
    .unwrap();
    assert!(value["implementation_roots"].py_eq(&Value::list([
        "docs/".into(),
        "src/".into(),
        "top.rs".into()
    ])));
    assert_eq!(snapshot.data, std::fs::read(&snapshot.path).unwrap());
    assert!(snapshot.parsed.as_ref().unwrap().py_eq(&value));
    let mut changed = value;
    let mut limits = changed["limits"].clone();
    limits.set("design_revisions", 3_i64.into());
    changed.set("limits", limits);
    std::fs::write(
        &snapshot.path,
        graph_engine::domain::ids::canonical_bytes(&changed).unwrap(),
    )
    .unwrap();
    assert_contract(
        policy::load_policy(
            &fixture.files,
            &fixture.repo,
            &fixture.skill,
            Some(&fixture.home),
        )
        .unwrap_err(),
        "limits.design_revisions",
        "LIMIT_MAY_NOT_INCREASE",
    );
}
#[test]
fn selected_home_seeds_exact_default_bytes_once() {
    let fixture = Fixture::new();
    let original =
        policy::ensure_profile_policy(&fixture.files, &fixture.repo, &fixture.default_home, None)
            .unwrap();
    let mut value = graph_engine::compat::loads(&std::fs::read(&original).unwrap()).unwrap();
    let mut limits = value["limits"].clone();
    limits.set("design_revisions", 1_i64.into());
    value.set("limits", limits);
    let seed = graph_engine::domain::ids::canonical_bytes(&value).unwrap();
    std::fs::write(&original, &seed).unwrap();
    let (_, snapshot) = policy::load_policy(
        &fixture.files,
        &fixture.repo,
        &fixture.skill,
        Some(&fixture.home),
    )
    .unwrap();
    assert_eq!(snapshot.data, seed);
    assert_eq!(std::fs::read(&snapshot.path).unwrap(), seed);
    std::fs::write(original, b"{}\n").unwrap();
    let (_, again) = policy::load_policy(
        &fixture.files,
        &fixture.repo,
        &fixture.skill,
        Some(&fixture.home),
    )
    .unwrap();
    assert_eq!(again.data, seed);
}
#[test]
fn legacy_policy_is_selected_for_existing_run_home() {
    let fixture = Fixture::new();
    let legacy =
        policy::ensure_profile_policy(&fixture.files, &fixture.repo, &fixture.skill, None).unwrap();
    std::fs::create_dir_all(fixture.home.join("graph-runs")).unwrap();
    let (_, snapshot) = policy::load_policy(
        &fixture.files,
        &fixture.repo,
        &fixture.skill,
        Some(&fixture.home),
    )
    .unwrap();
    assert!(fixture.files.same_path(&snapshot.path, &legacy));
    let fresh = policy::profile_policy_path(&fixture.files, &fixture.repo, &fixture.home).unwrap();
    assert!(!fresh.exists());
}
#[test]
fn state_root_precedence_and_explicit_invalid_paths() {
    let mut fixture = Fixture::new();
    fixture
        .files
        .environment_override
        .as_mut()
        .unwrap()
        .insert("SOFTWARE_ENGINEERING_GRAPH_HOME".into(), PyString::from(""));
    assert!(fixture.files.same_path(
        &policy::resolve_state_root(&fixture.files, Some(&fixture.home), &fixture.skill).unwrap(),
        &fixture.home
    ));
    match policy::resolve_state_root(&fixture.files, None, &fixture.skill).unwrap_err() {
        EngineError::State(error) => assert_eq!(error.code, "STATE_ROOT_INVALID"),
        other => panic!("{other:?}"),
    }
    match policy::resolve_state_root(
        &fixture.files,
        Some(std::path::Path::new("relative")),
        &fixture.skill,
    )
    .unwrap_err()
    {
        EngineError::State(error) => assert_eq!(error.code, "STATE_ROOT_MUST_BE_ABSOLUTE"),
        other => panic!("{other:?}"),
    }
}
#[test]
fn generic_key_errors_and_runtime_short_circuit_match_python() {
    let error = contracts::require_python_keys(&Value::Null, &1_i64.into(), &false.into(), "f")
        .unwrap_err();
    match error {
        graph_engine::domain::errors::ValidationFailure::Python { kind, message } => {
            assert_eq!(kind, "TypeError");
            assert!(message.equals("'int' object is not iterable"));
        }
        other => panic!("{other:?}"),
    }
    let target = Value::from("runtime:artifacts/id/a");
    assert!(policy_validation::runtime_target(
        &target,
        &["artifacts/".into(), Value::Null],
        cfg!(windows)
    )
    .is_ok());
    assert!(policy_validation::runtime_target(
        &target,
        &[Value::Null, "artifacts/".into()],
        cfg!(windows)
    )
    .is_err());
}

struct DistinctCaseFiles {
    native: NativePolicyFiles,
    identity_checks: std::cell::Cell<usize>,
}
impl graph_engine::application::snapshots::FileSnapshotPort for DistinctCaseFiles {
    fn snapshot(
        &self,
        path: &std::path::Path,
        roots: &[PathBuf],
        maximum: usize,
    ) -> Result<
        graph_engine::application::snapshots::Snapshot,
        graph_engine::application::snapshots::SnapshotFailure,
    > {
        graph_engine::application::snapshots::FileSnapshotPort::snapshot(
            &self.native,
            path,
            roots,
            maximum,
        )
    }
}
impl PolicyFilePort for DistinctCaseFiles {
    fn absolute(&self, path: &std::path::Path) -> Result<PathBuf, EngineError> {
        self.native.absolute(path)
    }
    fn resolve(&self, path: &std::path::Path, strict: bool) -> Result<PathBuf, EngineError> {
        self.native.resolve(path, strict)
    }
    fn from_string(&self, text: &PyString) -> Result<PathBuf, EngineError> {
        self.native.from_string(text)
    }
    fn spelling(&self, path: &std::path::Path) -> PyString {
        self.native.spelling(path)
    }
    fn same_path(&self, left: &std::path::Path, right: &std::path::Path) -> bool {
        self.native.same_path(left, right)
    }
    fn under(&self, path: &std::path::Path, root: &std::path::Path) -> bool {
        self.native.under(path, root)
    }
    fn exists(&self, path: &std::path::Path, follow: bool) -> Result<bool, EngineError> {
        self.native.exists(path, follow)
    }
    fn is_directory(&self, path: &std::path::Path) -> Result<bool, EngineError> {
        self.native.is_directory(path)
    }
    fn is_file(&self, path: &std::path::Path) -> Result<bool, EngineError> {
        self.native.is_file(path)
    }
    fn is_symlink(&self, path: &std::path::Path) -> Result<bool, EngineError> {
        self.native.is_symlink(path)
    }
    fn is_reparse(&self, path: &std::path::Path) -> Result<bool, EngineError> {
        self.native.is_reparse(path)
    }
    fn ensure_safe(
        &self,
        path: &std::path::Path,
        stop: Option<&std::path::Path>,
    ) -> Result<(), EngineError> {
        self.native.ensure_safe(path, stop)
    }
    fn entries(&self, path: &std::path::Path) -> Result<Vec<PathBuf>, EngineError> {
        self.native.entries(path)
    }
    fn make_directory(
        &self,
        path: &std::path::Path,
        parents: bool,
        mode: u32,
    ) -> Result<(), EngineError> {
        self.native.make_directory(path, parents, mode)
    }
    fn chmod(&self, path: &std::path::Path, mode: u32) -> Result<(), EngineError> {
        self.native.chmod(path, mode)
    }
    fn write_new_sync(
        &self,
        path: &std::path::Path,
        bytes: &[u8],
        mode: u32,
    ) -> Result<(), EngineError> {
        self.native.write_new_sync(path, bytes, mode)
    }
    fn same_file(
        &self,
        _left: &std::path::Path,
        _right: &std::path::Path,
    ) -> Result<bool, EngineError> {
        self.identity_checks.set(self.identity_checks.get() + 1);
        Ok(false)
    }
    fn environment(&self, key: &str) -> Option<PyString> {
        self.native.environment(key)
    }
    fn windows(&self) -> bool {
        self.native.windows()
    }
}

#[test]
fn every_projected_role_pair_requires_its_own_case_identity() {
    let fixture = Fixture::new();
    std::fs::write(fixture.repo.join("src/one.py"), b"fixture\n").unwrap();
    let files = DistinctCaseFiles {
        native: fixture.files,
        identity_checks: std::cell::Cell::new(0),
    };
    let grant = Value::object([
        ("effect", "filesystem_read".into()),
        ("action", "read".into()),
        ("target_ref", "repo:src/one.py".into()),
    ]);
    let mut sibling = grant.clone();
    sibling.set("target_ref", "repo:SRC/ONE.py".into());
    let task = Value::object([(
        "authority",
        Value::object([("capabilities", Value::list([grant.clone()]))]),
    )]);
    let record = Value::object([
        ("denied_patterns", Value::List(vec![])),
        (
            "role_capabilities",
            Value::object([
                ("first", Value::list([grant])),
                ("second", Value::list([sibling])),
            ]),
        ),
    ]);
    let version = 1_i64.into();
    if cfg!(windows) {
        assert_contract(
            policy::validate_task_capability_paths(
                &files,
                &fixture.repo,
                &task,
                &record,
                Some(&fixture.home),
                Some(&version),
            )
            .unwrap_err(),
            "capability.target_ref",
            "CAPABILITY_CASE_IDENTITY_UNPROVEN",
        );
        assert_eq!(files.identity_checks.get(), 1);
    } else {
        policy::validate_task_capability_paths(
            &files,
            &fixture.repo,
            &task,
            &record,
            Some(&fixture.home),
            Some(&version),
        )
        .unwrap();
        assert_eq!(files.identity_checks.get(), 0);
    }
}
