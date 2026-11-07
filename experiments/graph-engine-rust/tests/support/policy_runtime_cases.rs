use super::{
    append, contract, grant, policy, role_grant, set, strings, EngineError, PolicyCase, Value,
};
use graph_engine::{compat, infrastructure::policy_files::NativePolicyFiles};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn installed_policy(case: &PolicyCase) -> PathBuf {
    policy::profile_policy_path(
        &NativePolicyFiles::default(),
        case.repo(),
        &case.graph.fixture.graph.runtime_root,
    )
    .unwrap()
}

fn text_path(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn installed_cli(case: &PolicyCase, host: &str) -> (PathBuf, PathBuf) {
    let skill = case
        .root()
        .join(host)
        .join("skills")
        .join("software-engineering-graph");
    let scripts = skill.join("scripts");
    fs::create_dir_all(&scripts).unwrap();
    let executable = scripts.join(format!("graphctl{}", std::env::consts::EXE_SUFFIX));
    fs::copy(env!("CARGO_BIN_EXE_graphctl"), &executable).unwrap();
    (skill, executable)
}

fn run_installed(
    case: &PolicyCase,
    executable: &Path,
    arguments: &[&str],
    home: Option<&str>,
) -> Value {
    let mut command = Command::new(executable);
    command
        .args(["--repo", text_path(case.repo())])
        .args(arguments);
    for key in [
        "SOFTWARE_ENGINEERING_GRAPH_HOME",
        "SOFTWARE_ENGINEERING_GRAPH_STATE_HOME",
        "XDG_STATE_HOME",
    ] {
        command.env_remove(key);
    }
    if let Some(home) = home {
        command.env("SOFTWARE_ENGINEERING_GRAPH_HOME", home);
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "installed graphctl failed: {:?} {} {}",
        result.status.code(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    compat::loads(&result.stdout).unwrap()
}

#[test]
fn test_missing_installed_policy_is_created_and_repository_policy_is_ignored() {
    let case = PolicyCase::new();
    let installed = installed_policy(&case);
    fs::remove_file(&installed).unwrap();
    fs::write(case.repo().join(".codex/engineering-graph.json"), "{}").unwrap();
    let (policy, snapshot) = case.load().unwrap();
    assert_eq!(snapshot.path, installed);
    assert_eq!(policy["schema_version"].as_i64(), Some(3));
    assert!(policy["implementation_roots"]
        .as_list()
        .unwrap()
        .iter()
        .any(|root| root.is_text("docs/")));
    assert!(policy["required_checks"]["diff-check"]["argv"]
        .py_eq(&strings(&["git", "diff", "--check"])));
    assert!(case.graph.run(&["policy"]).unwrap()["policy_digest"].is_text(&snapshot.digest));

    let mut task = case.task();
    set(
        &mut task,
        &["policy_approval", "sha256"],
        snapshot.digest.into(),
    );
    task.set("required_check_ids", strings(&["diff-check"]));
    task.set("evidence_paths", strings(&[]));
    let graph = &case.graph.fixture.graph;
    let repository = policy["repository_id"].text().unwrap();
    let artifacts = graph
        .runtime_root
        .join("artifacts")
        .join(&repository)
        .join("profile-run");
    fs::create_dir_all(&artifacts).unwrap();
    let task_path = artifacts.join("task.json");
    super::hardening_case::write_json(&task_path, &task);
    let task_path = text_path(&task_path);
    let preview = case
        .graph
        .run(&["plan", "--run-id", "PROFILE-RUN", "--task-brief", task_path])
        .unwrap();
    assert!(preview["code"].is_text("EXECUTION_PLAN_PREVIEW"));
    let initialized = case
        .graph
        .run(&[
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
            "init",
            "--run-id",
            "PROFILE-RUN",
            "--task-brief",
            task_path,
            "--op-id",
            "profile-init",
        ])
        .unwrap();
    let plan = initialized["execution_plan_digest"].text().unwrap();
    case.graph
        .run(&[
            "record",
            "plan-approval",
            "--run-id",
            "PROFILE-RUN",
            "--plan-digest",
            &plan,
            "--decision",
            "APPROVE",
            "--authority-ref",
            "authority:test",
            "--op-id",
            "profile-approval",
        ])
        .unwrap();
    assert!(case
        .graph
        .run(&["next", "--run-id", "PROFILE-RUN", "--all"])
        .unwrap()["ok"]
        .truthy());
    assert!(case
        .graph
        .run(&[
            "resume",
            "--run-id",
            "PROFILE-RUN",
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
        ])
        .unwrap()["code"]
        .is_text("RESUMED"));
    let run = graph
        .store
        .open_run(&repository, "PROFILE-RUN")
        .unwrap()
        .first_run()
        .unwrap()
        .unwrap();
    assert!(run["task_ref"]
        .as_string()
        .unwrap()
        .starts_with(&format!("runtime:artifacts/{repository}/")));
    fs::write(&installed, "{}").unwrap();
    contract(case.load(), None);
}

#[test]
fn test_explicit_legacy_state_root_reads_installed_policy_without_moving_it() {
    let case = PolicyCase::new();
    let (skill, executable) = installed_cli(&case, ".codex");
    let legacy =
        policy::profile_policy_path(&NativePolicyFiles::default(), case.repo(), &skill).unwrap();
    fs::create_dir(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, &case.graph.fixture.graph.policy_bytes).unwrap();
    let state = skill.join("state");
    let result = run_installed(
        &case,
        &executable,
        &["--state-root", text_path(&state), "policy"],
        None,
    );
    assert_eq!(
        result["policy_path"].text().as_deref(),
        Some(text_path(&legacy))
    );
    assert!(result["policy_source"].is_text("legacy_installed_skill"));
    assert!(!state.exists());
}

#[test]
fn test_explicit_state_root_ignores_invalid_environment_home() {
    let case = PolicyCase::new();
    let (skill, executable) = installed_cli(&case, ".codex");
    let explicit = case.root().join("explicit-home");
    let invalid = "relative/invalid";
    let result = run_installed(
        &case,
        &executable,
        &["--state-root", text_path(&explicit), "policy"],
        Some(invalid),
    );
    assert_eq!(
        PathBuf::from(result["state_root"].text().unwrap()),
        explicit
    );
    assert!(PathBuf::from(result["policy_path"].text().unwrap()).is_file());
    let files = NativePolicyFiles {
        environment_override: Some(BTreeMap::from([(
            "SOFTWARE_ENGINEERING_GRAPH_HOME".to_owned(),
            invalid.into(),
        )])),
    };
    assert!(matches!(
        policy::load_policy(&files, case.repo(), &skill, None),
        Err(EngineError::State(_))
    ));
}

#[test]
fn test_generated_policy_excludes_existing_artifact_and_sensitive_roots() {
    let case = PolicyCase::new();
    let installed = installed_policy(&case);
    fs::remove_file(&installed).unwrap();
    for name in ["engineering-graph-artifacts", "secrets-backup"] {
        fs::create_dir(case.repo().join(name)).unwrap();
    }
    for name in ["credentials.json", "notes.key"] {
        fs::write(case.repo().join(name), "private").unwrap();
    }
    let (policy, _) = case.load().unwrap();
    for root in [
        "engineering-graph-artifacts/",
        "credentials.json",
        "notes.key",
        "secrets-backup/",
    ] {
        assert!(!policy["implementation_roots"]
            .as_list()
            .unwrap()
            .iter()
            .any(|value| value.is_text(root)));
    }
    assert!(policy["artifact_roots"]["repo"].py_eq(&strings(&[])));
    let repository = policy["repository_id"].text().unwrap();
    assert!(
        policy["artifact_roots"]["runtime"].py_eq(&strings(&[&format!("artifacts/{repository}/")]))
    );
    assert!(!policy["role_capabilities"]
        .as_object()
        .unwrap()
        .iter()
        .any(|(_, rows)| {
            rows.as_list()
                .unwrap()
                .iter()
                .any(|capability| capability["target_ref"].is_text("repo:credentials.json"))
        }));
    let write = |policy: &Value| super::hardening_case::write_json(&installed, policy);
    let mut invalid = policy.clone();
    set(
        &mut invalid,
        &["artifact_roots", "repo"],
        strings(&["docs/"]),
    );
    write(&invalid);
    contract(case.load(), Some("RUNTIME_ARTIFACTS_REQUIRED"));
    for root in ["credentials.json", "notes.key", "secrets-backup/"] {
        let mut invalid = policy.clone();
        append(&mut invalid, &["implementation_roots"], root.into());
        write(&invalid);
        contract(case.load(), Some("SENSITIVE_PATH"));
        let mut invalid = policy.clone();
        role_grant(
            &mut invalid,
            "code_reviewer",
            grant("filesystem_read", "read", &format!("repo:{root}")),
        );
        write(&invalid);
        contract(case.load(), Some("SENSITIVE_PATH"));
    }
}

#[test]
#[cfg_attr(not(windows), ignore = "Windows junction behavior")]
fn test_generated_policy_skips_top_level_junction() {
    let case = PolicyCase::new();
    fs::remove_file(installed_policy(&case)).unwrap();
    let outside = case.root().join("external");
    fs::create_dir(&outside).unwrap();
    let link = case.repo().join("linked-project");
    let junction = Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(outside)
        .output()
        .unwrap();
    assert!(
        junction.status.success(),
        "mklink failed: {}",
        String::from_utf8_lossy(&junction.stderr)
    );
    let (policy, _) = case.load().unwrap();
    assert!(!policy["implementation_roots"]
        .as_list()
        .unwrap()
        .iter()
        .any(|root| root.is_text("linked-project/")));
}

#[test]
fn test_policy_and_state_follow_each_install_location() {
    let case = PolicyCase::new();
    for host in [".agents", ".claude", ".cursor", ".codex"] {
        let (_, executable) = installed_cli(&case, host);
        let result = run_installed(&case, &executable, &["policy"], None);
        let runtime = case
            .root()
            .join(format!("{host}.local"))
            .join("software-engineering-graph");
        let installed = PathBuf::from(result["policy_path"].text().unwrap());
        assert_eq!(installed.parent().unwrap(), runtime.join("policies"));
        assert_eq!(PathBuf::from(result["state_root"].text().unwrap()), runtime);
        assert!(result["policy_source"].is_text("runtime_home"));
    }
}
