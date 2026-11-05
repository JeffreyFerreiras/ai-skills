use super::{
    database, hardening_source_git as source, initialize, row, state_failure, EngineError,
    HardeningCase, Value, RUN,
};
use graph_engine::{
    application::{
        error::EngineResult,
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
    },
    compat::PyString,
    infrastructure::process::NativeProcess,
};
use std::{cell::Cell, fs};

fn enable(
    case: &HardeningCase,
    coverage: &std::path::Path,
    operation: &str,
) -> EngineResult<Value> {
    case.run(&[
        "evidence",
        "enable",
        "--run-id",
        RUN,
        "--contract-version",
        "2",
        "--coverage-manifest",
        coverage.to_str().unwrap(),
        "--op-id",
        operation,
    ])
}

fn coverage(case: &mut HardeningCase, path: &str) -> std::path::PathBuf {
    case.inbox(&Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "check_coverage".into()),
        (
            "checks",
            Value::list([Value::object([
                ("check_id", "repo-check".into()),
                ("relevant_inputs", Value::list([path.into()])),
                ("complete", true.into()),
            ])]),
        ),
    ]))
}

fn resume(case: &HardeningCase) -> EngineResult<Value> {
    case.run(&[
        "resume",
        "--run-id",
        RUN,
        "--ack-degraded-permissions",
        "--ack-degraded-durability",
    ])
}

#[test]
fn test_stale_v1_receipt_can_enable_and_resume_without_waiving_integrity() {
    let mut case = HardeningCase::new();
    initialize(&case, &[]);
    let repo = &case.fixture.graph.repository;
    let source_path = repo.join("docs").join("source.txt");
    fs::write(&source_path, "A").unwrap();
    source::initial_commit(repo, "docs/source.txt");
    let receipt = case
        .run(&[
            "check",
            "run",
            "--run-id",
            RUN,
            "--check-id",
            "repo-check",
            "--op-id",
            "legacy-check",
        ])
        .unwrap();
    fs::write(&source_path, "B").unwrap();
    state_failure(case.status(), &["CHECK_EVIDENCE_PROVENANCE_INVALID"]);
    let manifest = coverage(&mut case, "docs/source.txt");
    let reference = receipt["artifact_ref"].clone();
    let load = |db: &super::SqliteSession| {
        row(
            db,
            "SELECT content_json FROM artifacts WHERE ref=?",
            std::slice::from_ref(&reference),
        )["content_json"]
            .clone()
    };
    let original = {
        let db = database(&case);
        let original = load(&db);
        db.execute_values(
            "UPDATE artifacts SET content_json='{}' WHERE ref=?",
            std::slice::from_ref(&reference),
        )
        .unwrap();
        original
    };
    assert!(matches!(
        enable(&case, &manifest, "invalid-enable"),
        Err(EngineError::State(_) | EngineError::Contract(_))
    ));
    database(&case)
        .execute_values(
            "UPDATE artifacts SET content_json=? WHERE ref=?",
            &[original.clone(), reference.clone()],
        )
        .unwrap();
    enable(&case, &manifest, "enable").unwrap();
    case.status().unwrap();
    resume(&case).unwrap();
    assert!(original.py_eq(&load(&database(&case))));
}

struct CheckSpy {
    configured: PyString,
    calls: Cell<usize>,
}

impl ProcessPort for CheckSpy {
    fn capture(&self, request: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome> {
        if request.arguments.first() == Some(&self.configured) {
            self.calls.set(self.calls.get() + 1);
        }
        NativeProcess.capture(request)
    }
}

#[test]
fn test_declared_ignored_input_with_only_broad_authority_refuses_execution() {
    let mut case = HardeningCase::new();
    let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
    let mut authority = task["authority"].clone();
    let mut capabilities = authority["capabilities"].as_list().unwrap().to_vec();
    capabilities.push(Value::object([
        ("effect", "command".into()),
        ("action", "run".into()),
        ("target_ref", "npm-run-check".into()),
    ]));
    authority.set("capabilities", Value::List(capabilities));
    task.set("authority", authority);
    case.fixture.initialize(&task, true).unwrap();
    case.impact("fast_path");
    let writer = case.claim_raw().unwrap();
    let repo = &case.fixture.graph.repository;
    let ignore = if cfg!(windows) {
        "docs/ignored-input.txt\r\n"
    } else {
        "docs/ignored-input.txt\n"
    };
    fs::write(repo.join(".gitignore"), ignore).unwrap();
    let ignored = repo.join("docs").join("ignored-input.txt");
    fs::write(&ignored, "A").unwrap();
    source::initial_commit(repo, "docs/engineering-graph.md");
    let manifest = coverage(&mut case, "docs/ignored-input.txt");
    enable(&case, &manifest, "enable").unwrap();
    let repo = &case.fixture.graph.repository;
    let policy = case
        .fixture
        .with_context(|context| Ok(context.policy.clone()))
        .unwrap();
    let process = CheckSpy {
        configured: policy["required_checks"]["repo-check"]["argv"]
            .as_list()
            .unwrap()[0]
            .as_string()
            .unwrap()
            .clone(),
        calls: Cell::new(0),
    };
    for value in ["A", "B"] {
        fs::write(&ignored, value).unwrap();
        let command = vec![
            "--repo".into(),
            repo.to_str().unwrap().into(),
            "check".into(),
            "run".into(),
            "--run-id".into(),
            RUN.into(),
            "--check-id".into(),
            "repo-check".into(),
            "--executor-branch-id".into(),
            writer["branch_id"].as_string().unwrap().clone(),
            "--executor-attempt-id".into(),
            writer["attempt_id"].as_string().unwrap().clone(),
            "--executor-claim-token".into(),
            writer["claim_token"].as_string().unwrap().clone(),
            "--source-branch-id".into(),
            writer["branch_id"].as_string().unwrap().clone(),
            "--source-attempt-id".into(),
            writer["attempt_id"].as_string().unwrap().clone(),
            "--source-claim-digest".into(),
            writer["claim_digest"].as_string().unwrap().clone(),
            "--op-id".into(),
            format!("check-{value}").into(),
        ];
        state_failure(
            case.fixture
                .execute_with_process(&command, &case.fixture.graph.store, &process),
            &["SOURCE_UNVERIFIABLE"],
        );
    }
    assert_eq!(process.calls.get(), 0);
    let capture = || {
        source::capture(
            repo,
            &policy,
            "docs/ignored-input.txt",
            &["docs/ignored-input.txt"],
        )
        .unwrap()
    };
    let first = capture();
    fs::write(&ignored, "C").unwrap();
    assert!(!first["sha256"].py_eq(&capture()["sha256"]));
}

#[test]
fn test_source_capture_hashes_untracked_content_and_rejects_missing_git() {
    let case = HardeningCase::new();
    let repo = &case.fixture.graph.repository;
    let policy = case
        .fixture
        .with_context(|context| Ok(context.policy.clone()))
        .unwrap();
    let capture = || source::capture(repo, &policy, "docs/", &[]);
    state_failure(capture(), &["SOURCE_UNVERIFIABLE"]);
    source::initial_commit(repo, "docs/engineering-graph.md");
    let path = repo.join("docs").join("untracked.txt");
    fs::write(&path, "first").unwrap();
    let first = capture().unwrap();
    fs::write(&path, "second").unwrap();
    assert!(!first["sha256"].py_eq(&capture().unwrap()["sha256"]));
}

#[test]
fn test_evidence_enable_preserves_historical_task_and_plan() {
    let mut case = HardeningCase::new();
    let initialized = initialize(&case, &[]);
    let path = case.fixture.graph.repository.join("docs").join("task.json");
    let task = fs::read(&path).unwrap();
    let manifest = coverage(&mut case, "docs/");
    let result = enable(&case, &manifest, "enable").unwrap();
    assert_eq!(result["state_schema_version"].as_i64(), Some(7));
    let status = case.status().unwrap();
    assert!(initialized["execution_plan_digest"].py_eq(&status["execution_plan"]["plan_digest"]));
    assert_eq!(task, fs::read(path).unwrap());
}
