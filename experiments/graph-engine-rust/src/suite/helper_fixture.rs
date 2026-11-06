use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        helper::HelperRegister,
        helper_repository::HelperRegisterRepository,
        persistent_store::StatePlatform,
        policy::PolicyFilePort,
        state::FaultHook,
    },
    compat::{self, Value},
    domain::ids,
    infrastructure::{
        helper_files::NativeHelperFileTransactions, policy_files::NativePolicyFiles,
        state_platform::NativeStatePlatform,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
pub struct NoFault;
impl FaultHook for NoFault {
    fn observe(&self, _point: &str) -> EngineResult<()> {
        Ok(())
    }
}
pub static FILES: std::sync::LazyLock<NativePolicyFiles> =
    std::sync::LazyLock::new(NativePolicyFiles::default);
pub static PLATFORM: NativeStatePlatform = NativeStatePlatform;
pub static TRANSACTIONS: NativeHelperFileTransactions = NativeHelperFileTransactions;
pub static NO_FAULT: NoFault = NoFault;

#[derive(Clone)]
pub struct Settings {
    pub unsupported: bool,
    pub test_mode: bool,
    pub fresh_selection: bool,
    pub model: &'static str,
    pub effort: &'static str,
    pub observed_model: Option<&'static str>,
    pub observed_effort: Option<&'static str>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            unsupported: false,
            test_mode: false,
            fresh_selection: true,
            model: "gpt-6.1-sol",
            effort: "low",
            observed_model: None,
            observed_effort: None,
        }
    }
}
pub struct Fixture {
    pub root: PathBuf,
    pub repository: PathBuf,
    pub installation: PathBuf,
    pub command: Value,
    pub allowance: Value,
    pub observation: Value,
    pub plan: Value,
}
pub fn json(text: &str) -> Value {
    compat::loads_text(&text.into()).unwrap()
}
pub fn write(path: &Path, value: &Value) {
    std::fs::write(path, ids::canonical_bytes(value).unwrap()).unwrap();
}
pub fn read(path: &Path) -> Value {
    compat::loads(&std::fs::read(path).unwrap()).unwrap()
}
pub fn set_nested(value: &mut Value, keys: &[&str], replacement: Value) {
    if keys.len() == 1 {
        value.set(keys[0], replacement);
        return;
    }
    let mut nested = value[keys[0]].clone();
    set_nested(&mut nested, &keys[1..], replacement);
    value.set(keys[0], nested);
}
pub fn remove(value: &mut Value, key: &str) {
    if let Value::Object(fields) = value {
        fields.retain(|(name, _)| !name.equals(key));
    }
}
pub fn sign_plan(plan: &mut Value) {
    remove(plan, "plan_digest");
    plan.set(
        "plan_digest",
        ids::sha256_bytes(&ids::canonical_bytes(plan).unwrap()).into(),
    );
}
pub fn assert_code(result: EngineResult<Value>, expected: &str) {
    let error = result.expect_err(expected);
    let actual = match error {
        EngineError::Helper(error) => error.code,
        EngineError::Contract(error) => error.code,
        other => panic!("expected {expected}, got {other:?}"),
    };
    assert_eq!(actual, expected);
}
pub fn assignment(
    identifier: &str,
    parent: &str,
    role: &str,
    commands: Value,
    settings: &Settings,
) -> Value {
    let mut required = vec![
        "fresh_model_effort_selection".into(),
        "filesystem_confinement".into(),
        "tool_confinement".into(),
    ];
    if role == "validation_executor" {
        required.push("command_confinement".into());
    }
    let mut capabilities = vec![json(
        r#"{"effect":"filesystem_read","action":"read","target_ref":"repo:docs/"}"#,
    )];
    for command in commands.as_list().unwrap() {
        capabilities.push(Value::object([
            ("effect", "command".into()),
            ("action", "run".into()),
            ("target_ref", command["command_id"].clone()),
        ]));
    }
    Value::object([
        ("assignment_id", identifier.into()),
        ("parent_role", parent.into()),
        ("helper_role", role.into()),
        ("contract_revision", 1_i64.into()),
        ("model", settings.model.into()),
        ("reasoning_effort", settings.effort.into()),
        ("parent_capabilities", Value::List(capabilities)),
        ("scope_refs", Value::list(["repo:docs/".into()])),
        ("commands", commands),
        ("checkpoint_policy", "observed_repository_state".into()),
        ("resource_keys", Value::list(["worktree".into()])),
        ("required_host_capabilities", Value::List(required)),
        (
            "limits",
            json(
                r#"{"children":2,"concurrency":1,"commands":2,"time_seconds":240,"output_tokens":4000,"file_reads":12}"#,
            ),
        ),
    ])
}
impl Fixture {
    pub fn new(settings: Settings, revision: Option<i64>) -> Self {
        let mut fixture = Self::prepare(settings);
        let mut task = super::task_fixture::TaskFixture::new().default_v2();
        task.set("schema_version", 3_i64.into());
        task.set("task_id", "TASK-HELPERS".into());
        task.set(
            "helper_allowance",
            Value::object([
                ("ref", "repo:docs/helper-allowance.json".into()),
                (
                    "sha256",
                    ids::sha256_bytes(
                        &std::fs::read(fixture.repository.join("docs/helper-allowance.json"))
                            .unwrap(),
                    )
                    .into(),
                ),
            ]),
        );
        fixture.plan = if let Some(revision) = revision {
            graph_engine::domain::execution::reconstruct_execution_plan(
                "RUN-HELPERS",
                &task,
                &Value::object([
                    ("host", "codex-astra".into()),
                    ("catalog_revision", revision.into()),
                    ("code_review_panel_version", 1_i64.into()),
                    ("simplification_review_version", 1_i64.into()),
                    ("capability_projection_version", 1_i64.into()),
                ]),
                None,
            )
            .unwrap()
        } else {
            graph_engine::domain::execution::build_execution_plan(
                "RUN-HELPERS",
                &task,
                None,
                "codex-astra",
            )
            .unwrap()
        };
        write(
            &fixture.repository.join("docs/helper-plan.json"),
            &fixture.plan,
        );
        fixture
    }
    pub fn prepare(settings: Settings) -> Self {
        let artifact_root =
            PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").expect("ART required"));
        let fixture_parent = artifact_root.join("fixtures");
        std::fs::create_dir_all(&fixture_parent).unwrap();
        let selected_temporary =
            graph_engine::infrastructure::python_temporary::temporary_root().unwrap();
        assert!(
            FILES.same_path(
                &FILES.resolve(&selected_temporary, true).unwrap(),
                &FILES.resolve(&fixture_parent, true).unwrap()
            ),
            "helper fixture TEMP/TMP/TMPDIR must select ART/fixtures"
        );
        let root = (0..1000)
            .find_map(|_| {
                let candidate = fixture_parent.join(format!(
                    "helper-register-{}-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed),
                    PLATFORM.temporary_name().unwrap()
                ));
                let builder = std::fs::DirBuilder::new();
                #[cfg(unix)]
                let mut builder = builder;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                match builder.create(&candidate) {
                    Ok(()) => Some(candidate),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(error) => panic!("helper fixture creation: {error}"),
                }
            })
            .expect("unique helper fixture directory");
        let root = FILES.resolve(&root, true).unwrap();
        let repository = root.join("repo");
        std::fs::create_dir_all(repository.join(".codex")).unwrap();
        std::fs::create_dir_all(repository.join("docs/artifacts")).unwrap();
        let installation = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../skills/software-engineering-graph")
            .canonicalize()
            .unwrap();
        let evidence = if cfg!(windows) {
            b"# Test graph evidence\r\n".as_slice()
        } else {
            b"# Test graph evidence\n".as_slice()
        };
        std::fs::write(repository.join("docs/engineering-graph.md"), evidence).unwrap();
        let policy_source = installation.join("tests/fixtures/engineering-graph.json");
        let repository_policy = repository.join(".codex/engineering-graph.json");
        std::fs::copy(policy_source, &repository_policy).unwrap();
        std::fs::create_dir(root.join("installed-skill")).unwrap();
        let profile_policy = graph_engine::application::policy::profile_policy_path(
            &*FILES,
            &repository,
            &root.join("codex"),
        )
        .unwrap();
        std::fs::create_dir_all(profile_policy.parent().unwrap()).unwrap();
        std::fs::hard_link(repository_policy, profile_policy).unwrap();
        let python = std::env::var("GRAPH_RUST_ORACLE_PYTHON")
            .expect("exact selected oracle sys.executable required for stored command fixture");
        let command = Value::object([
            ("command_id", "focused-tests".into()),
            (
                "argv",
                Value::list([
                    python.into(),
                    "-m".into(),
                    "unittest".into(),
                    "tests.test_contracts".into(),
                ]),
            ),
            ("timeout_seconds", 120_i64.into()),
        ]);
        let mut allowance = Value::object([
            ("schema_version", 1_i64.into()),
            ("allowance_id", "helpers-1".into()),
            ("run_id", "RUN-HELPERS".into()),
            (
                "assignments",
                Value::list([
                    assignment(
                        "evidence",
                        "tech_lead",
                        "evidence_scout",
                        Value::list([]),
                        &settings,
                    ),
                    assignment(
                        "validation",
                        "senior_engineer",
                        "validation_executor",
                        Value::list([command.clone()]),
                        &settings,
                    ),
                ]),
            ),
            (
                "shared_limits",
                json(
                    r#"{"children":2,"concurrency":1,"commands":2,"time_seconds":240,"output_tokens":4000,"file_reads":12}"#,
                ),
            ),
            ("resources", json(r#"[{"key":"worktree","capacity":1}]"#)),
        ]);
        if settings.test_mode {
            allowance.set("schema_version", 2_i64.into());
            allowance.set(
                "test_mode",
                Value::object([
                    (
                        "disposable_repository",
                        Value::String(FILES.spelling(&repository)),
                    ),
                    ("acknowledge_unenforced_isolation", true.into()),
                    ("external_effects", "mocked".into()),
                ]),
            );
        }
        write(&repository.join("docs/helper-allowance.json"), &allowance);
        let mut capabilities = Value::object([]);
        for name in [
            "fresh_model_effort_selection",
            "filesystem_confinement",
            "tool_confinement",
            "command_confinement",
        ] {
            let unavailable = settings.unsupported
                && (name == "filesystem_confinement"
                    || (settings.test_mode && name.ends_with("_confinement")));
            capabilities.set(
                name,
                Value::object([
                    (
                        "status",
                        if unavailable {
                            "unavailable"
                        } else if name == "fresh_model_effort_selection"
                            && !settings.fresh_selection
                        {
                            "unverified"
                        } else {
                            "verified"
                        }
                        .into(),
                    ),
                    ("source", "host_api".into()),
                    (
                        "uncertainty",
                        if unavailable {
                            "not exposed"
                        } else {
                            "none observed"
                        }
                        .into(),
                    ),
                ]),
            );
        }
        let observation = Value::object([
            ("schema_version", 1_i64.into()),
            ("host_id", "local-host".into()),
            ("observed_at", "2026-09-12T00:00:00Z".into()),
            ("source", "host capability probe".into()),
            ("uncertainty", "trusted caller evidence only".into()),
            ("capabilities", capabilities),
            (
                "supported_assignments",
                Value::list([Value::object([
                    (
                        "model",
                        settings.observed_model.unwrap_or(settings.model).into(),
                    ),
                    (
                        "reasoning_effort",
                        settings.observed_effort.unwrap_or(settings.effort).into(),
                    ),
                    ("status", "verified".into()),
                    ("source", "host_api".into()),
                    ("uncertainty", "none observed".into()),
                ])]),
            ),
        ]);
        write(&repository.join("docs/host-observation.json"), &observation);
        Self {
            root,
            repository,
            installation,
            command,
            allowance,
            observation,
            plan: Value::Null,
        }
    }
    pub fn registry<'a>(&'a self, fault: &'a dyn FaultHook) -> HelperRegister<'a> {
        HelperRegister {
            repository: HelperRegisterRepository {
                files: &*FILES,
                platform: &PLATFORM,
                transactions: &TRANSACTIONS,
                installation: &self.installation,
                fault,
            },
        }
    }
    pub fn initialize(&self) -> EngineResult<Value> {
        self.registry(&NO_FAULT).initialize(
            &self.root.join("host-state"),
            &self.repository,
            &"RUN-HELPERS".into(),
            &self.repository.join("docs/helper-plan.json"),
            &self.repository.join("docs/helper-allowance.json"),
            &self.repository.join("docs/host-observation.json"),
        )
    }
    pub fn request(&self, identifier: &str, assignment: &str, command: Option<Value>) -> Value {
        let checkpoint = self
            .repository
            .join("docs")
            .join(format!("checkpoint-{identifier}.json"));
        let content = format!("{{\"request\": \"{identifier}\", \"dirty\": true}}");
        std::fs::write(&checkpoint, content.as_bytes()).unwrap();
        let reference = format!(
            "repo:docs/checkpoint-{identifier}.json#sha256={}",
            ids::sha256_bytes(content.as_bytes())
        );
        Value::object([
            ("schema_version", 1_i64.into()),
            ("request_id", identifier.into()),
            ("assignment_id", assignment.into()),
            (
                "parent_role",
                if assignment == "validation" {
                    "senior_engineer"
                } else {
                    "tech_lead"
                }
                .into(),
            ),
            (
                "helper_role",
                if assignment == "validation" {
                    "validation_executor"
                } else {
                    "evidence_scout"
                }
                .into(),
            ),
            ("contract_revision", 1_i64.into()),
            ("model", "gpt-6.1-sol".into()),
            ("reasoning_effort", "low".into()),
            ("scope_refs", Value::list(["repo:docs/".into()])),
            ("commands", Value::List(command.into_iter().collect())),
            ("checkpoint_ref", reference.into()),
            ("resource_keys", Value::list(["worktree".into()])),
            (
                "budgets",
                json(r#"{"time_seconds":60,"output_tokens":1000,"file_reads":3}"#),
            ),
        ])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let fixtures =
            PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").expect("ART required"))
                .join("fixtures")
                .canonicalize()
                .unwrap();
        assert!(!std::fs::symlink_metadata(&self.root)
            .unwrap()
            .file_type()
            .is_symlink());
        let target = self.root.canonicalize().unwrap();
        assert_eq!(target.parent(), Some(fixtures.as_path()));
        assert!(target
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("helper-register-"));
        std::fs::remove_dir_all(&target).expect("source-equivalent helper fixture teardown");
    }
}
pub fn settlement(request: &Value, terminal: &str) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("request_id", request["request_id"].clone()),
        (
            "request_digest",
            ids::sha256_bytes(&ids::canonical_bytes(request).unwrap()).into(),
        ),
        ("terminal_state", terminal.into()),
        ("evidence_refs", Value::list([])),
        ("uncertainty", "host execution evidence only".into()),
        ("session_identity_sha256", "a".repeat(64).into()),
        ("usage_ref", Value::Null),
    ])
}
pub fn register_path(initialized: &Value) -> PathBuf {
    PathBuf::from(initialized["register_path"].text().unwrap())
}
