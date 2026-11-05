use super::{
    append, contract, grant, planner, policy, role_grant, set, strings, validate_task, PolicyCase,
    Value,
};
use crate::planner_policy_files::CaseIdentity;
use graph_engine::{
    application::policy::PolicyFilePort, domain::contracts::capability_within,
    infrastructure::policy_files::NativePolicyFiles,
};
use std::{fs, io::ErrorKind};

const AUTHORITY: &[&str] = &["authority", "capabilities"];

fn effective(policy: &Value, task: &Value, role: &str, windows: bool) -> Vec<Value> {
    planner::effective_role_capabilities(policy, task, role, Some(&1_i64.into()), windows).unwrap()
}

fn has_capability(values: &[Value], candidate: &Value) -> bool {
    values.iter().any(|value| value.py_eq(candidate))
}

#[test]
fn test_case_only_prefix_requires_existing_filesystem_identity() {
    let case = PolicyCase::new();
    fs::create_dir(case.repo().join("src")).unwrap();
    let mut task = case.task();
    let candidate = grant("filesystem_write", "edit", "repo:SRC/new.py");
    append(&mut task, AUTHORITY, candidate.clone());
    let validated = validate_task(&task, &case.snapshot.digest, &case.policy, true).unwrap();
    let files = CaseIdentity::new(&case.graph);
    let validate = |files: &dyn PolicyFilePort| {
        policy::validate_task_capability_paths(
            files,
            case.repo(),
            &validated,
            &case.policy,
            None,
            Some(&1_i64.into()),
        )
    };
    contract(validate(&files), Some("CAPABILITY_CASE_IDENTITY_UNPROVEN"));
    for error in [ErrorKind::NotFound, ErrorKind::PermissionDenied] {
        files.fail_with(error);
        contract(validate(&files), Some("CAPABILITY_CASE_IDENTITY_UNPROVEN"));
    }
    files.take_calls();
    files.prove(true);
    validate(&files).unwrap();
    let calls = files.take_calls();
    let (left, right) = calls.last().unwrap();
    let native = NativePolicyFiles::default();
    assert_eq!(
        native.spelling(left),
        native.spelling(&case.repo().join("SRC"))
    );
    assert_eq!(
        native.spelling(right),
        native.spelling(&case.repo().join("src"))
    );
    assert!(has_capability(
        &effective(&case.policy, &validated, "senior_engineer", true),
        &candidate
    ));
    if cfg!(windows) {
        assert!(native
            .same_file(&case.repo().join("src"), &case.repo().join("SRC"))
            .unwrap());
        validate(&native).unwrap();
    }
}

#[test]
fn test_exact_new_grants_and_legacy_projection_do_not_need_case_proof() {
    let case = PolicyCase::new();
    let mut task = case.task();
    let mut candidate = grant("filesystem_write", "edit", "repo:src/new/");
    append(&mut task, AUTHORITY, candidate.clone());
    let files = CaseIdentity::new(&case.graph);
    policy::validate_task_capability_paths(
        &files,
        case.repo(),
        &task,
        &case.policy,
        None,
        Some(&1_i64.into()),
    )
    .unwrap();
    candidate.set("target_ref", "repo:SRC/new/".into());
    let mut rows = task["authority"]["capabilities"]
        .as_list()
        .unwrap()
        .to_vec();
    *rows.last_mut().unwrap() = candidate;
    set(&mut task, AUTHORITY, Value::List(rows));
    policy::validate_task_capability_paths(&files, case.repo(), &task, &case.policy, None, None)
        .unwrap();
    assert!(files.take_calls().is_empty());
}

#[test]
fn test_policy_union_match_cannot_hide_an_unproved_role_pair() {
    let case = PolicyCase::new();
    let mut policy = case.policy.clone();
    policy.set("schema_version", 2_i64.into());
    let mut task = case.task();
    let candidate = grant("filesystem_read", "read", "repo:SRC/new.py");
    append(&mut task, AUTHORITY, candidate.clone());
    role_grant(
        &mut policy,
        "supervisor",
        grant("filesystem_read", "read", "repo:SRC/"),
    );
    let validated = validate_task(&task, &case.snapshot.digest, &policy, true).unwrap();
    for role in ["supervisor", "tech_lead"] {
        assert!(has_capability(
            &effective(&policy, &validated, role, true),
            &candidate
        ));
    }
    let files = CaseIdentity::new(&case.graph);
    contract(
        policy::validate_task_capability_paths(
            &files,
            case.repo(),
            &validated,
            &policy,
            None,
            Some(&1_i64.into()),
        ),
        Some("CAPABILITY_CASE_IDENTITY_UNPROVEN"),
    );
}

#[test]
fn test_runtime_case_only_prefix_also_requires_identity() {
    let case = PolicyCase::new();
    let task = Value::object([(
        "authority",
        Value::object([(
            "capabilities",
            Value::list([grant(
                "filesystem_write",
                "edit",
                "runtime:artifacts/OUTPUTS/new.py",
            )]),
        )]),
    )]);
    let policy = Value::object([
        ("denied_patterns", strings(&[])),
        (
            "role_capabilities",
            Value::object([(
                "senior_engineer",
                Value::list([grant(
                    "filesystem_write",
                    "edit",
                    "runtime:artifacts/outputs/",
                )]),
            )]),
        ),
    ]);
    let runtime = Some(case.graph.fixture.graph.runtime_root.as_path());
    let files = CaseIdentity::new(&case.graph);
    for root in [None, runtime] {
        contract(
            policy::validate_task_capability_paths(
                &files,
                case.repo(),
                &task,
                &policy,
                root,
                Some(&1_i64.into()),
            ),
            Some("CAPABILITY_CASE_IDENTITY_UNPROVEN"),
        );
    }
    files.prove(true);
    policy::validate_task_capability_paths(
        &files,
        case.repo(),
        &task,
        &policy,
        runtime,
        Some(&1_i64.into()),
    )
    .unwrap();
}

#[test]
fn test_filesystem_grants_narrow_directory_ceilings_without_replacing_them() {
    let case = PolicyCase::new();
    let mut task = case.task();
    let grants = [
        grant("filesystem_write", "edit", "repo:src/slots.py"),
        grant("filesystem_write", "edit", "repo:src/test_slots.py"),
    ];
    for grant in &grants {
        append(&mut task, AUTHORITY, grant.clone());
    }
    let validated =
        validate_task(&task, &case.snapshot.digest, &case.policy, cfg!(windows)).unwrap();
    let writes = validated["authority"]["capabilities"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|cap| cap["effect"].is_text("filesystem_write"))
        .cloned()
        .collect::<Vec<_>>();
    assert!(Value::List(writes).py_eq(&Value::list(grants)));
    assert!(case.policy["role_capabilities"]["senior_engineer"]
        .as_list()
        .unwrap()
        .iter()
        .any(|cap| cap["target_ref"].is_text("repo:src/")));
}

#[test]
fn test_filesystem_containment_uses_host_case_policy_and_preserves_spelling() {
    let outer = grant("filesystem_write", "edit", "repo:src/");
    let candidate = grant("filesystem_write", "edit", "repo:SRC/slots.py");
    let case = PolicyCase::new();
    let mut task = case.task();
    append(&mut task, AUTHORITY, candidate.clone());
    for windows in [true, false] {
        assert_eq!(
            capability_within(&candidate, &outer, windows).unwrap(),
            windows
        );
        let exact = grant("filesystem_write", "edit", "repo:src/slots.py");
        assert_eq!(
            capability_within(&candidate, &exact, windows).unwrap(),
            windows
        );
        let validated = validate_task(&task, &case.snapshot.digest, &case.policy, windows);
        if windows {
            let validated = validated.unwrap();
            assert!(has_capability(
                validated["authority"]["capabilities"].as_list().unwrap(),
                &candidate
            ));
            assert!(has_capability(
                &effective(&case.policy, &validated, "senior_engineer", windows),
                &candidate
            ));
        } else {
            contract(validated, Some("POLICY_AUTHORITY_EXCEEDED"));
        }
        assert!(!capability_within(
            &grant("filesystem_write", "edit", "repo:SRC-other/slots.py"),
            &outer,
            windows
        )
        .unwrap());
        assert!(!capability_within(
            &grant("external_read", "inspect", "service"),
            &grant("external_read", "inspect", "Service"),
            windows
        )
        .unwrap());
        contract(
            capability_within(
                &grant("filesystem_write", "edit", "repo:SRC/../secret.py"),
                &outer,
                windows,
            )
            .map_err(Into::into),
            None,
        );
    }
    assert!(candidate["target_ref"].is_text("repo:SRC/slots.py"));
}

#[test]
fn test_narrowing_refuses_parent_sibling_action_scheme_and_nonfilesystem_prefixes() {
    let windows = cfg!(windows);
    let outer = grant("filesystem_write", "edit", "repo:src/feature/");
    for candidate in [
        grant("filesystem_write", "edit", "repo:src/"),
        grant("filesystem_write", "edit", "repo:src/feature2/file.py"),
        grant(
            "filesystem_write",
            "edit",
            "runtime:artifacts/src/feature/file.py",
        ),
        grant("filesystem_write", "delete", "repo:src/feature/"),
    ] {
        assert!(!capability_within(&candidate, &outer, windows).unwrap());
    }
    assert!(!capability_within(
        &grant("filesystem_write", "edit", "repo:src/file.py/child"),
        &grant("filesystem_write", "edit", "repo:src/file.py"),
        windows,
    )
    .unwrap());
    for effect in ["command", "external_read", "external_write"] {
        assert!(!capability_within(
            &grant(effect, "run", "service-child"),
            &grant(effect, "run", "service"),
            windows
        )
        .unwrap());
    }
}

#[test]
fn test_narrowing_rejects_ambiguous_sensitive_hashed_and_custom_denied_paths() {
    let case = PolicyCase::new();
    let mut policy = case.policy.clone();
    append(&mut policy, &["denied_patterns"], "**/private.txt".into());
    let hashed = format!("repo:src/file.py#sha256={}", "a".repeat(64));
    for target in [
        "repo:src/../docs/file.py",
        r"repo:src\file.py",
        "repo:src//file.py",
        "repo:src/*.py",
        "repo:src/[a].py",
        &hashed,
        "repo:src/file.py:stream",
        "repo:src/file.py.",
        "repo:src/.git/config",
        "repo:src/.env",
        "repo:src/private.txt",
    ] {
        let mut task = case.task();
        append(
            &mut task,
            AUTHORITY,
            grant("filesystem_read", "read", target),
        );
        contract(
            validate_task(&task, &case.snapshot.digest, &policy, cfg!(windows)),
            None,
        );
    }
}
