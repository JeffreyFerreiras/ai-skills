#[path = "support/capability_contract_cases.rs"]
mod capability_contract_cases;
#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[allow(dead_code)]
#[path = "support/planner_policy_files.rs"]
mod planner_policy_files;
#[path = "support/policy_runtime_cases.rs"]
mod policy_runtime_cases;
#[path = "support/task_contract_cases.rs"]
mod task_contract_cases;

use graph_engine::{
    application::{error::EngineError, policy, snapshots::Snapshot},
    compat::{PyString, Value},
    domain::{
        contracts::ContractError,
        ids,
        planner::{self, NodeSpec},
        policy_validation, task_contract,
    },
    infrastructure::policy_files::NativePolicyFiles,
};
use hardening_case::HardeningCase;
use std::{fs, path::Path};

struct PolicyCase {
    graph: HardeningCase,
    policy: Value,
    snapshot: Snapshot,
}

impl PolicyCase {
    fn new() -> Self {
        let graph = HardeningCase::stock();
        let (policy, snapshot) = Self::load_graph(&graph).unwrap();
        Self {
            graph,
            policy,
            snapshot,
        }
    }

    fn load_graph(case: &HardeningCase) -> Result<(Value, Snapshot), EngineError> {
        let graph = &case.fixture.graph;
        policy::load_policy(
            &NativePolicyFiles::default(),
            &graph.repository,
            &graph.skill_root,
            Some(&graph.runtime_root),
        )
    }

    fn load(&self) -> Result<(Value, Snapshot), EngineError> {
        Self::load_graph(&self.graph)
    }

    fn repo(&self) -> &Path {
        &self.graph.fixture.graph.repository
    }

    fn root(&self) -> &Path {
        &self.graph.fixture.graph.root
    }

    fn write(&self, policy: &Value) {
        hardening_case::write_json(&self.repo().join(".codex/engineering-graph.json"), policy);
    }

    fn task(&self) -> Value {
        self.graph
            .fixture
            .graph
            .task("delivery", "full_delivery", &[])
    }

    fn v2(&self) -> Value {
        let mut value = self.policy.clone();
        value.set("schema_version", 2_i64.into());
        set(
            &mut value,
            &["artifact_roots", "repo"],
            strings(&["artifacts/"]),
        );
        value.set(
            "implementation_roots",
            strings(&["specs/", "app/", "tools/", "pyproject.toml"]),
        );
        value.set(
            "required_checks",
            Value::object([(
                "focused",
                Value::object([
                    ("command_id", "python-focused".into()),
                    ("mandatory", true.into()),
                    (
                        "argv",
                        strings(&["python", "-m", "unittest", "tests.test_contracts"]),
                    ),
                    ("timeout_seconds", 600_i64.into()),
                ]),
            )]),
        );
        let replacements = [
            ("repo:docs/", "repo:specs/"),
            ("repo:src/", "repo:app/"),
            ("repo:scripts/", "repo:tools/"),
            ("repo:docs/technical-designs/", "repo:artifacts/design/"),
            ("npm-run-check", "python-focused"),
        ];
        edit(&mut value, &["role_capabilities"], |roles| {
            for (role, entries) in roles.as_object().unwrap().to_vec() {
                let rows = entries.as_list().unwrap().iter().cloned().map(|mut row| {
                    if let Some((_, replacement)) = replacements
                        .iter()
                        .find(|(old, _)| row["target_ref"].is_text(old))
                    {
                        row.set("target_ref", (*replacement).into());
                    }
                    row
                });
                roles.set(&role.utf8().unwrap(), Value::list(rows));
            }
        });
        for path in ["artifacts/design", "specs", "app", "tools"] {
            fs::create_dir_all(self.repo().join(path)).unwrap();
        }
        let contents = if cfg!(windows) {
            "[tool.test]\r\n"
        } else {
            "[tool.test]\n"
        };
        fs::write(self.repo().join("pyproject.toml"), contents).unwrap();
        role_grant(
            &mut value,
            "senior_engineer",
            grant("filesystem_read", "read", "repo:pyproject.toml"),
        );
        role_grant(
            &mut value,
            "senior_engineer",
            grant("filesystem_write", "edit", "repo:pyproject.toml"),
        );
        value
    }
}

fn strings(values: &[&str]) -> Value {
    Value::list(values.iter().copied().map(Value::from))
}

fn edit(value: &mut Value, path: &[&str], change: impl FnOnce(&mut Value)) {
    if let Some((key, rest)) = path.split_first() {
        let mut child = value[*key].clone();
        edit(&mut child, rest, change);
        value.set(key, child);
    } else {
        change(value);
    }
}

fn set(value: &mut Value, path: &[&str], replacement: Value) {
    edit(value, path, |field| *field = replacement);
}

fn append(value: &mut Value, path: &[&str], item: Value) {
    edit(value, path, |field| {
        let mut entries = field.as_list().unwrap().to_vec();
        entries.push(item);
        *field = Value::List(entries);
    });
}

fn grant(effect: &str, action: &str, target: &str) -> Value {
    Value::object([
        ("effect", effect.into()),
        ("action", action.into()),
        ("target_ref", target.into()),
    ])
}

fn role_grant(policy: &mut Value, role: &str, grant: Value) {
    append(policy, &["role_capabilities", role], grant);
}

fn contract<T>(result: Result<T, EngineError>, code: Option<&str>) -> ContractError {
    match result {
        Err(EngineError::Contract(error)) => {
            assert!(
                code.is_none_or(|expected| error.code == expected),
                "{error}"
            );
            error
        }
        Err(error) => panic!("expected contract error {code:?}, got {error:?}"),
        Ok(_) => panic!("expected contract error {code:?}, got success"),
    }
}

fn validate_task(
    task: &Value,
    digest: &str,
    policy: &Value,
    windows: bool,
) -> Result<Value, EngineError> {
    Ok(task_contract::validate_task_brief(
        task,
        &PyString::from(digest),
        policy,
        Some(&1_i64.into()),
        windows,
    )?)
}

fn check_policy(case: &PolicyCase, value: &Value, code: Option<&str>) {
    case.write(value);
    contract(case.load(), code);
}

fn directory_link(link: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    return std::os::windows::fs::symlink_dir(target, link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)
}

#[test]
fn test_policy_cannot_raise_loop_limit() {
    let case = PolicyCase::new();
    let mut value = case.policy.clone();
    set(&mut value, &["limits", "design_revisions"], 3_i64.into());
    check_policy(&case, &value, Some("LIMIT_MAY_NOT_INCREASE"));
}

#[test]
fn test_policy_cannot_add_engine_authority() {
    let case = PolicyCase::new();
    let mut value = case.policy.clone();
    role_grant(
        &mut value,
        "senior_engineer",
        grant("external_write", "deploy", "production"),
    );
    check_policy(&case, &value, Some("ENGINE_AUTHORITY_EXCEEDED"));
}

#[test]
fn test_policy_v2_allows_bounded_repo_paths_and_exact_configured_commands() {
    let case = PolicyCase::new();
    case.write(&case.v2());
    let (loaded, snapshot) = case.load().unwrap();
    assert_eq!(loaded["schema_version"].as_i64(), Some(2));
    assert!(loaded["required_checks"]["focused"]["command_id"].is_text("python-focused"));
    let mut task = case.task();
    set(
        &mut task,
        &["policy_approval", "sha256"],
        snapshot.digest.clone().into(),
    );
    set(
        &mut task,
        &["authority", "capabilities"],
        Value::list([
            grant("filesystem_read", "read", "repo:pyproject.toml"),
            grant("filesystem_write", "edit", "repo:pyproject.toml"),
            grant("filesystem_write", "edit", "repo:app/"),
            grant("command", "run", "python-focused"),
        ]),
    );
    task.set("required_check_ids", strings(&["focused"]));
    let validated = validate_task(&task, &snapshot.digest, &loaded, cfg!(windows)).unwrap();
    let branch = planner::envelope(
        "RUN-1",
        &snapshot.digest,
        &loaded,
        &validated,
        &NodeSpec {
            key: "senior_engineer".into(),
            role: "senior_engineer".into(),
            stage: "implementation".into(),
            generation: 0,
            mandatory: true,
            specialist_tag: None,
        },
        "ready",
        &[],
        0,
        None,
        cfg!(windows),
    )
    .unwrap();
    assert!(branch["effect_capabilities"].py_eq(&validated["authority"]["capabilities"]));
}

#[test]
fn test_policy_v2_rejects_artifact_implementation_overlap_and_unclassified_writes() {
    let case = PolicyCase::new();
    for (implementation, artifacts) in [
        ("artifacts/", "artifacts/"),
        ("artifacts/", "artifacts/design/"),
        ("artifacts/design/", "artifacts/"),
        ("shared", "shared/"),
    ] {
        let mut policy = case.v2();
        policy.set("implementation_roots", strings(&[implementation]));
        set(
            &mut policy,
            &["artifact_roots", "repo"],
            strings(&[artifacts]),
        );
        check_policy(&case, &policy, Some("ARTIFACT_IMPLEMENTATION_OVERLAP"));
    }
    let mut policy = case.v2();
    fs::create_dir(case.repo().join("unclassified")).unwrap();
    role_grant(
        &mut policy,
        "senior_engineer",
        grant("filesystem_write", "edit", "repo:unclassified/"),
    );
    check_policy(&case, &policy, Some("ENGINE_AUTHORITY_EXCEEDED"));
}

#[test]
fn test_policy_v2_uses_platform_path_identity_for_classification_and_duplicates() {
    let case = PolicyCase::new();
    let windows = cfg!(windows);
    for (implementation, artifacts) in [
        ("Artifacts/", "artifacts/"),
        ("Artifacts/Design/", "artifacts/"),
        ("Artifacts/", "artifacts/design/"),
    ] {
        let mut value = case.v2();
        value.set("implementation_roots", strings(&[implementation]));
        set(
            &mut value,
            &["artifact_roots", "repo"],
            strings(&[artifacts]),
        );
        case.write(&value);
        if windows {
            contract(case.load(), Some("ARTIFACT_IMPLEMENTATION_OVERLAP"));
        }
    }
    for names in [["generated", "generated/"], ["RootFile", "rootfile"]]
        .into_iter()
        .take(if windows { 2 } else { 1 })
    {
        let mut value = case.v2();
        value.set("implementation_roots", strings(&names));
        check_policy(&case, &value, Some("INVALID_OR_DUPLICATE"));
    }
    if windows {
        let mut value = case.v2();
        set(
            &mut value,
            &["artifact_roots", "repo"],
            strings(&["Artifacts/", "artifacts/"]),
        );
        check_policy(&case, &value, Some("INVALID_OR_DUPLICATE"));
    }
    let mut value = case.v2();
    value.set("implementation_roots", strings(&["App/"]));
    let allowed = |policy: &Value, target| {
        policy_validation::role_capability_allowed(
            policy,
            &"senior_engineer".into(),
            &grant("filesystem_write", "edit", target),
            windows,
        )
        .unwrap()
    };
    assert_eq!(allowed(&value, "repo:app/module.py"), windows);
    value.set("implementation_roots", strings(&["package.json"]));
    assert!(allowed(&value, "repo:package.json/"));
    assert!(!allowed(&value, "repo:package.json/child"));
}

#[test]
fn test_policy_v2_accepts_empty_implementation_roots_without_implementation_writes() {
    let case = PolicyCase::new();
    let mut value = case.v2();
    value.set("implementation_roots", strings(&[]));
    edit(
        &mut value,
        &["role_capabilities", "senior_engineer"],
        |rows| {
            *rows = Value::list(
                rows.as_list()
                    .unwrap()
                    .iter()
                    .filter(|capability| {
                        !capability["effect"].is_text("filesystem_write")
                            || capability["target_ref"]
                                .as_string()
                                .unwrap()
                                .starts_with("repo:artifacts/")
                    })
                    .cloned(),
            );
        },
    );
    case.write(&value);
    assert!(case.load().unwrap().0["implementation_roots"].py_eq(&strings(&[])));
}

#[test]
fn test_policy_v1_rejects_v2_implementation_classification() {
    let case = PolicyCase::new();
    let mut value = case.policy.clone();
    value.set("implementation_roots", strings(&[]));
    check_policy(&case, &value, Some("UNKNOWN_FIELD"));
}

#[test]
fn test_policy_v2_rejects_path_command_and_role_escalations() {
    let case = PolicyCase::new();
    for (role, effect, action, target) in [
        (
            "senior_engineer",
            "filesystem_write",
            "edit",
            "repo:.codex/",
        ),
        ("tech_lead", "filesystem_write", "edit", "repo:app/"),
        ("code_reviewer", "filesystem_write", "edit", "repo:app/"),
        ("senior_engineer", "command", "run", "not-configured"),
        ("senior_engineer", "external_write", "deploy", "production"),
    ] {
        let mut value = case.v2();
        role_grant(&mut value, role, grant(effect, action, target));
        check_policy(&case, &value, None);
    }
}

#[test]
fn test_policy_v2_rejects_link_escape_and_requires_complete_check_specs() {
    let case = PolicyCase::new();
    let outside = case.root().join("outside");
    fs::create_dir(&outside).unwrap();
    if directory_link(&case.repo().join("linked"), &outside).is_ok() {
        let mut value = case.v2();
        role_grant(
            &mut value,
            "senior_engineer",
            grant("filesystem_write", "edit", "repo:linked/"),
        );
        check_policy(&case, &value, Some("LINK_OR_REPARSE_POINT"));
    } else {
        eprintln!("directory symlink unavailable; original conditional link assertion omitted");
    }
    for missing in ["argv", "timeout_seconds"] {
        let mut value = case.v2();
        edit(&mut value, &["required_checks", "focused"], |check| {
            check.remove(missing).unwrap();
        });
        check_policy(&case, &value, Some("MISSING_FIELD"));
    }
    fs::write(case.repo().join("artifact-file"), "not a directory").unwrap();
    let mut value = case.v2();
    set(
        &mut value,
        &["artifact_roots", "repo"],
        strings(&["artifact-file/"]),
    );
    check_policy(&case, &value, Some("DIRECTORY_ROOT_REQUIRED"));
}

#[test]
fn test_policy_v1_bytes_digest_and_exact_target_ceiling_remain_frozen() {
    let case = PolicyCase::new();
    let bytes = &case.graph.fixture.graph.policy_bytes;
    assert_eq!(case.snapshot.digest, ids::sha256_bytes(bytes));
    let mut value = case.policy.clone();
    role_grant(
        &mut value,
        "senior_engineer",
        grant("filesystem_write", "edit", "repo:app/"),
    );
    fs::create_dir(case.repo().join("app")).unwrap();
    check_policy(&case, &value, Some("ENGINE_AUTHORITY_EXCEEDED"));
}

#[test]
fn test_policy_missing_research_template_fails_closed() {
    let case = PolicyCase::new();
    let mut value = case.policy.clone();
    edit(&mut value, &["node_templates"], |templates| {
        templates.remove("design_research_validation").unwrap();
    });
    check_policy(&case, &value, Some("ENGINE_TOPOLOGY_CHANGED"));
}

#[test]
fn test_legacy_four_route_policy_gains_engine_owned_delivery_only_route() {
    let case = PolicyCase::new();
    let mut value = case.policy.clone();
    edit(&mut value, &["routes"], |routes| {
        routes.remove("delivery_only").unwrap();
    });
    case.write(&value);
    assert!(
        case.load().unwrap().0["routes"]["delivery_only"].py_eq(&Value::object([
            ("entry_node", "senior_engineer".into()),
            ("design_gates", false.into()),
            ("delivery_gates", true.into()),
            ("closure", "closure".into()),
        ]))
    );
}

#[test]
fn test_policy_topology_roots_contracts_and_targets_are_engine_bounded() {
    let case = PolicyCase::new();
    let edits = [
        (
            &["routes", "full_delivery", "entry_node"][..],
            "senior_engineer".into(),
        ),
        (&["artifact_roots", "repo"][..], strings(&["src/"])),
        (
            &[
                "node_templates",
                "architect",
                "output_contract",
                "artifact_kind",
            ][..],
            "technical_design".into(),
        ),
        (
            &["specialists", "security_privacy", "mandatory"][..],
            false.into(),
        ),
        (
            &["artifact_kinds", "finding", "extensions"][..],
            strings(&[".exe"]),
        ),
        (&["compatible_engine", "min"][..], "3.0.0".into()),
    ];
    let restore = || {
        fs::write(
            case.repo().join(".codex/engineering-graph.json"),
            &case.graph.fixture.graph.policy_bytes,
        )
        .unwrap()
    };
    for (path, replacement) in edits {
        let mut value = case.policy.clone();
        set(&mut value, path, replacement);
        check_policy(&case, &value, None);
        restore();
    }
    for capability in [
        grant("filesystem_write", "edit", "repo:.codex/"),
        grant("command", "run", "unknown-command"),
    ] {
        let mut value = case.policy.clone();
        role_grant(&mut value, "senior_engineer", capability);
        check_policy(&case, &value, None);
        restore();
    }
}
