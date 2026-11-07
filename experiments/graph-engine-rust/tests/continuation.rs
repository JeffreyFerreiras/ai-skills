//! Disposable delivery traces with synthetic approvals and reviewer reports.

#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[allow(dead_code)]
#[path = "support/recovery_case.rs"]
mod recovery_case;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
    },
    compat::{self, PyString, Value},
    domain::ids,
    infrastructure::process::NativeProcess,
};
use hardening_case::{write_json, HardeningCase};
use recovery_case::{artifact_bytes, ledger_snapshot, state_error};
use std::{collections::BTreeSet, fs, process::Command};

fn capability(effect: &str, action: &str, target: &str) -> Value {
    Value::object([
        ("effect", effect.into()),
        ("action", action.into()),
        ("target_ref", target.into()),
    ])
}

fn fixture() -> HardeningCase {
    let mut case = HardeningCase::golden();
    fs::write(
        case.fixture.graph.repository.join("docs/source.txt"),
        "pending",
    )
    .unwrap();
    case.edit_policy(|policy| {
        let mut checks = policy["required_checks"].clone();
        let mut check = checks["repo-check"].clone();
        check.set("argv", Value::list([
            std::env::var("GRAPH_RUST_ORACLE_PYTHON").unwrap().into(), "-c".into(),
            "from pathlib import Path; assert Path('docs/source.txt').read_text() == 'delivered'".into(),
        ]));
        checks.set("repo-check", check);
        policy.set("required_checks", checks);
    });
    case
}

fn task(case: &HardeningCase, route: &str) -> Value {
    let mut task = case.fixture.graph.task("delivery", route, &[]);
    let mut authority = task["authority"].clone();
    authority.set(
        "capabilities",
        Value::list([
            capability("filesystem_read", "read", "repo:docs/source.txt"),
            capability("filesystem_write", "edit", "repo:docs/source.txt"),
            capability("command", "run", "npm-run-check"),
        ]),
    );
    task.set("authority", authority);
    task
}

fn preview(case: &HardeningCase, task: &Value) -> EngineResult<Value> {
    let path = case.fixture.graph.repository.join("docs/task.json");
    write_json(&path, task);
    case.run(&[
        "plan",
        "--run-id",
        "RUN-1",
        "--task-brief",
        path.to_str().unwrap(),
        "--host",
        "codex-astra",
    ])
}

fn resume(case: &HardeningCase) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_graphctl"))
        .arg("--repo")
        .arg(&case.fixture.graph.repository)
        .arg("--state-root")
        .arg(&case.fixture.graph.runtime_root)
        .args([
            "resume",
            "--run-id",
            "RUN-1",
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    compat::loads(&output.stdout).unwrap()
}

fn enable_evidence(case: &mut HardeningCase) {
    for args in [
        vec!["init", "--quiet", "--template="],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "synthetic continuation fixture",
        ],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(&case.fixture.graph.repository)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let coverage = case.inbox(&Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "check_coverage".into()),
        (
            "checks",
            Value::list([Value::object([
                ("check_id", "repo-check".into()),
                ("relevant_inputs", Value::list(["docs/source.txt".into()])),
                ("complete", true.into()),
            ])]),
        ),
    ]));
    case.run(&[
        "evidence",
        "enable",
        "--run-id",
        "RUN-1",
        "--contract-version",
        "2",
        "--coverage-manifest",
        coverage.to_str().unwrap(),
        "--op-id",
        "enable-evidence",
    ])
    .unwrap();
}

fn fail(case: &mut HardeningCase, branch: &Value) -> Value {
    let failure = case.repo_artifact(
        "failure",
        &format!("interrupted-{}", branch["attempt_id"].text().unwrap()),
    );
    case.record_as(
        branch,
        Value::object([
            ("schema_version", 1_i64.into()),
            ("run_id", "RUN-1".into()),
            ("branch_id", branch["branch_id"].clone()),
            ("status", "failed".into()),
            (
                "output_kind",
                branch["output_contract"]["artifact_kind"].clone(),
            ),
            ("failure_code", "TOOL_FAILURE".into()),
            ("evidence", Value::list([failure.clone()])),
        ]),
        "failure",
    )
    .unwrap();
    failure
}

fn retry(case: &HardeningCase, branch: &Value, operation: &str) {
    case.run(&[
        "record",
        "retry",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch["branch_id"].text().unwrap(),
        "--reason-code",
        "CONTINUE",
        "--op-id",
        operation,
    ])
    .unwrap();
}

fn check_args(
    executor: &Value,
    source: &Value,
    operation: &str,
    previous: Option<&Value>,
) -> Vec<String> {
    let mut args = vec![
        "check".into(),
        "run".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--check-id".into(),
        "repo-check".into(),
        "--op-id".into(),
        operation.into(),
    ];
    for (flag, value) in [
        ("--executor-branch-id", &executor["branch_id"]),
        ("--executor-attempt-id", &executor["attempt_id"]),
        ("--executor-claim-token", &executor["claim_token"]),
        ("--source-branch-id", &source["branch_id"]),
        ("--source-attempt-id", &source["attempt_id"]),
        ("--source-claim-digest", &source["claim_digest"]),
    ] {
        args.extend([flag.into(), value.text().unwrap()]);
    }
    if let Some(previous) = previous {
        args.extend(["--replace-ref".into(), previous.text().unwrap()]);
    }
    args
}

fn check(
    case: &HardeningCase,
    executor: &Value,
    source: &Value,
    operation: &str,
    previous: Option<&Value>,
) -> Value {
    let args = check_args(executor, source, operation, previous);
    case.run(&args.iter().map(String::as_str).collect::<Vec<_>>())
        .unwrap()
}

#[test]
fn preflight_matches_python_without_initialization_or_plan_digest_changes() {
    let case = fixture();
    let mut task = task(&case, "full_delivery");
    task.set(
        "scope",
        Value::object([
            ("included", Value::list(["product fix".into()])),
            ("excluded", Value::list(["graph redesign".into()])),
        ]),
    );
    task.set("required_human_decisions", Value::list(["release".into()]));
    let native = preview(&case, &task).unwrap();
    let path = case.fixture.graph.repository.join("docs/task.json");
    let before = fs::read(&path).unwrap();
    let output = Command::new(std::env::var("GRAPH_RUST_ORACLE_PYTHON").unwrap())
        .arg("-B")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../skills/software-engineering-graph/scripts/graphctl.py"
        ))
        .arg("--repo")
        .arg(&case.fixture.graph.repository)
        .arg("--state-root")
        .arg(&case.fixture.graph.runtime_root)
        .args([
            "plan",
            "--run-id",
            "RUN-1",
            "--host",
            "codex-astra",
            "--task-brief",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let python = compat::loads(&output.stdout).unwrap();
    assert!(
        native["preflight"].py_eq(&python["preflight"]),
        "native={:?}\npython={:?}",
        native["preflight"],
        python["preflight"]
    );
    assert!(native["execution_plan"].py_eq(&python["execution_plan"]));
    let preflight = &native["preflight"];
    assert!(preflight["task"]["scope"].py_eq(&task["scope"]));
    assert!(preflight["task"]["sha256"].is_text(&ids::sha256_bytes(&before)));
    assert!(
        preflight["policy_digest"].is_text(&ids::sha256_bytes(&case.fixture.graph.policy_bytes))
    );
    assert!(preflight["required_human_decisions"].py_eq(&task["required_human_decisions"]));
    assert!(preflight["inspection_budget"].py_eq(&task["inspection_budget"]));
    let write = capability("filesystem_write", "edit", "repo:docs/source.txt");
    assert!(preflight["role_capabilities"]["senior_engineer"]
        .as_list()
        .unwrap()
        .iter()
        .any(|cap| cap.py_eq(&write)));
    assert!(!preflight["role_capabilities"]["code_reviewer"]
        .as_list()
        .unwrap()
        .iter()
        .any(|cap| cap.py_eq(&write)));
    assert!(!case
        .fixture
        .graph
        .store
        .db_path("albanian-live-translate", "RUN-1")
        .exists());
    assert_eq!(before, fs::read(path).unwrap());
    let initialized = case.fixture.initialize(&task, false).unwrap();
    assert!(preflight["plan_digest"].py_eq(&initialized["execution_plan_digest"]));
}

#[test]
fn unconfigured_required_check_fails_before_plan_or_init() {
    let case = fixture();
    let mut task = task(&case, "full_delivery");
    task.set(
        "required_check_ids",
        Value::list(["repo-check".into(), "missing-check".into()]),
    );
    state_error(preview(&case, &task), "UNKNOWN_CHECK");
    state_error(case.fixture.initialize(&task, false), "UNKNOWN_CHECK");
    assert!(!case
        .fixture
        .graph
        .store
        .db_path("albanian-live-translate", "RUN-1")
        .exists());
}

#[test]
fn required_check_without_command_fails_before_initialization() {
    let mut case = fixture();
    case.edit_policy(|policy| {
        let mut checks = policy["required_checks"].clone();
        let mut fields = checks["repo-check"].as_object().unwrap().to_vec();
        fields.retain(|(key, _)| !key.equals("argv"));
        checks.set("repo-check", Value::Object(fields));
        policy.set("required_checks", checks);
    });
    let task = task(&case, "full_delivery");
    state_error(
        case.fixture.initialize(&task, false),
        "CHECK_COMMAND_NOT_CONFIGURED",
    );
    assert!(!case
        .fixture
        .graph
        .store
        .db_path("albanian-live-translate", "RUN-1")
        .exists());
}

fn interrupted_delivery(evidence: bool) {
    let mut case = fixture();
    let task = task(&case, "full_delivery");
    let initialized = case.fixture.initialize(&task, true).unwrap();
    if evidence {
        enable_evidence(&mut case);
    }
    case.impact("full_delivery");
    let tech = case.claim();
    case.success(&tech);
    case.advance("design_inputs");
    let architect = case.claim();
    case.success_with(&architect, Some("APPROVE"), vec![])
        .unwrap();
    case.advance("design_collection");
    case.consolidate("design", "APPROVE", 0, vec![]);
    case.advance("design_consolidation");
    let writer = case.claim();
    case.run(&[
        "record",
        "budget-use",
        "--run-id",
        "RUN-1",
        "--budget-id",
        "file_reads",
        "--amount",
        "3",
        "--source-branch-id",
        &writer["branch_id"].text().unwrap(),
        "--op-id",
        "reads",
    ])
    .unwrap();
    let failure = fail(&mut case, &writer);
    let before = ledger_snapshot(&case);
    let resumed = resume(&case);
    assert_eq!(before, ledger_snapshot(&case));
    assert!(resumed["next_action"]["kind"].is_text("retry"));
    assert!(resumed["next_action"]["branch_id"].py_eq(&writer["branch_id"]));
    assert!(resumed["continuation"]["remaining_budgets"]["file_reads"].py_eq(&9_i64.into()));
    assert!(resumed["execution_plan"]["plan_digest"].py_eq(&initialized["execution_plan_digest"]));
    let failure_ref = &resumed["next_action"]["repair_packet"]["failure_result_ref"];
    let original_failure = artifact_bytes(&case, failure_ref);
    retry(&case, &writer, "retry-writer");
    let replacement = case.claim();
    assert!(replacement["branch_id"].py_eq(&writer["branch_id"]));
    assert!(!replacement["attempt_id"].py_eq(&writer["attempt_id"]));
    assert!(replacement["effect_capabilities"].py_eq(&writer["effect_capabilities"]));
    fs::write(
        case.fixture.graph.repository.join("docs/source.txt"),
        "delivered",
    )
    .unwrap();
    let mut selected = None;
    if evidence {
        selected = Some(
            check(&case, &replacement, &replacement, "writer-check", None)["artifact_ref"].clone(),
        );
    }
    case.success_with(&replacement, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let mut reviewers = BTreeSet::new();
    for _ in 0..5 {
        let reviewer = case.claim();
        reviewers.insert(reviewer["node_key"].text().unwrap());
        if evidence && reviewer["node_key"].is_text("test_engineer") {
            selected = Some(
                check(
                    &case,
                    &reviewer,
                    &replacement,
                    "tester-check",
                    selected.as_ref(),
                )["artifact_ref"]
                    .clone(),
            );
        }
        case.success_with(&reviewer, Some("APPROVE"), vec![])
            .unwrap();
    }
    assert_eq!(reviewers.len(), 5);
    assert!(reviewers.contains("test_engineer"));
    case.advance("delivery_collection");
    case.consolidate("delivery", "ACCEPT", 0, vec![]);
    case.advance("delivery_consolidation");
    if !evidence {
        let checked = case
            .run(&[
                "check",
                "run",
                "--run-id",
                "RUN-1",
                "--check-id",
                "repo-check",
                "--op-id",
                "check",
            ])
            .unwrap();
        assert!(checked["outcome"].is_text("PASS"));
    }
    let acceptance = case.repo_artifact("acceptance_evidence", "acceptance");
    case.run(&[
        "record",
        "acceptance-evidence",
        "--run-id",
        "RUN-1",
        "--criterion-id",
        "AC-001",
        "--artifact-ref",
        &acceptance["ref"].text().unwrap(),
        "--artifact-sha256",
        &acceptance["sha256"].text().unwrap(),
        "--op-id",
        "acceptance",
    ])
    .unwrap();
    assert!(case
        .run(&["complete", "--run-id", "RUN-1", "--op-id", "complete"])
        .unwrap()["status"]
        .is_text("complete"));
    let finished = resume(&case);
    assert!(finished["next_action"]["kind"].is_text("complete"));
    assert!(finished["budgets"].py_eq(&resumed["budgets"]));
    assert!(finished["execution_plan"].py_eq(&resumed["execution_plan"]));
    assert_eq!(
        finished["branches"]
            .as_list()
            .unwrap()
            .iter()
            .filter(|branch| branch["node_key"].is_text("tech_lead"))
            .count(),
        1
    );
    assert_eq!(original_failure, artifact_bytes(&case, failure_ref));
    let reference = failure["ref"].text().unwrap();
    let path = reference
        .trim_start_matches("repo:")
        .split('#')
        .next()
        .unwrap();
    assert!(failure["sha256"].is_text(&ids::sha256_bytes(
        &fs::read(case.fixture.graph.repository.join(path)).unwrap()
    )));
    if evidence {
        assert!(finished["evidence_lifecycle"]["checks"]
            .as_list()
            .unwrap()
            .iter()
            .all(|check| check["eligibility"].is_text("eligible")));
    }
}

#[test]
fn legacy_interruption_completes_without_repeating_design_or_refunding_budget() {
    interrupted_delivery(false);
}

#[test]
fn format7_interruption_completes_with_source_bound_checks() {
    interrupted_delivery(true);
}

#[test]
fn resume_prioritizes_running_review_over_ready_sibling() {
    let mut case = fixture();
    case.fixture
        .initialize(&task(&case, "fast_path"), true)
        .unwrap();
    case.impact("fast_path");
    let writer = case.claim();
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let reviewer = case.claim();
    assert!(case.status().unwrap()["next_action"]["kind"].is_text("claim"));
    let before = ledger_snapshot(&case);
    let resumed = resume(&case);
    assert!(resumed["next_action"]["kind"].is_text("await_result"));
    assert!(
        resumed["next_action"]["branch_ids"].py_eq(&Value::list([reviewer["branch_id"].clone()]))
    );
    assert_eq!(before, ledger_snapshot(&case));
}

#[test]
fn resume_restores_failed_check_source_before_replacing_receipt() {
    let mut case = fixture();
    case.fixture
        .initialize(&task(&case, "fast_path"), true)
        .unwrap();
    enable_evidence(&mut case);
    case.impact("fast_path");
    let writer = case.claim();
    fs::write(
        case.fixture.graph.repository.join("docs/source.txt"),
        "delivered",
    )
    .unwrap();
    let receipt = check(&case, &writer, &writer, "writer-check", None);
    let original = artifact_bytes(&case, &receipt["artifact_ref"]);
    fail(&mut case, &writer);
    let before = ledger_snapshot(&case);
    let resumed = resume(&case);
    assert!(resumed["next_action"]["kind"].is_text("retry"));
    assert!(resumed["next_action"]["branch_id"].py_eq(&writer["branch_id"]));
    assert_eq!(before, ledger_snapshot(&case));
    assert!(
        !resumed["evidence_lifecycle"]["checks"].as_list().unwrap()[0]["eligibility"]
            .is_text("eligible")
    );
    retry(&case, &writer, "retry-check-source");
    let ready = resume(&case);
    assert!(ready["next_action"]["kind"].is_text("claim"));
    assert!(ready["next_action"]["branch_id"].py_eq(&writer["branch_id"]));
    let replacement = case.claim();
    let checked = check(
        &case,
        &replacement,
        &replacement,
        "replacement-check",
        Some(&receipt["artifact_ref"]),
    );
    assert!(checked["outcome"].is_text("PASS"));
    assert!(!checked["artifact_ref"].py_eq(&receipt["artifact_ref"]));
    assert!(replacement["retry_count"].py_eq(&1_i64.into()));
    assert!(ready["budgets"].py_eq(&resumed["budgets"]));
    assert_eq!(original, artifact_bytes(&case, &receipt["artifact_ref"]));
    fail(&mut case, &replacement);
    let before = ledger_snapshot(&case);
    let exhausted = resume(&case);
    assert!(exhausted["next_action"]["kind"].is_text("supervisor_assessment"));
    assert!(exhausted["next_action"]["repair_packet"]["remaining_retries"].py_eq(&0_i64.into()));
    assert_eq!(before, ledger_snapshot(&case));
}

struct InterruptedCheck;
impl ProcessPort for InterruptedCheck {
    fn capture(&self, request: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome> {
        if request.arguments.first()
            == Some(&PyString::from(
                std::env::var("GRAPH_RUST_ORACLE_PYTHON").unwrap(),
            ))
        {
            return Err(EngineError::state("INTERRUPTED_CHECK"));
        }
        NativeProcess.capture(request)
    }
}

#[test]
fn resume_preserves_unknown_check_reservation_even_after_source_failure() {
    let mut case = fixture();
    case.fixture
        .initialize(&task(&case, "fast_path"), true)
        .unwrap();
    enable_evidence(&mut case);
    case.impact("fast_path");
    let writer = case.claim();
    let mut args = vec![
        PyString::from("--repo"),
        case.fixture.graph.repository.to_str().unwrap().into(),
    ];
    args.extend(
        check_args(&writer, &writer, "interrupted-check", None)
            .iter()
            .map(|arg| PyString::from(arg.as_str())),
    );
    let result =
        case.fixture
            .execute_with_process(&args, &case.fixture.graph.store, &InterruptedCheck);
    assert!(matches!(result, Err(EngineError::State(error)) if error.code == "INTERRUPTED_CHECK"));
    let before = ledger_snapshot(&case);
    let resumed = resume(&case);
    assert!(resumed["next_action"]["kind"].is_text("check_execution_unknown"));
    assert_eq!(
        resumed["next_action"]["reservation_ids"]
            .as_list()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(before, ledger_snapshot(&case));
    fail(&mut case, &writer);
    let before = ledger_snapshot(&case);
    assert!(resume(&case)["next_action"].py_eq(&resumed["next_action"]));
    assert_eq!(before, ledger_snapshot(&case));
}

#[test]
fn resume_reports_stale_check_without_refreshing_review_source() {
    let mut case = fixture();
    case.fixture
        .initialize(&task(&case, "fast_path"), true)
        .unwrap();
    enable_evidence(&mut case);
    case.impact("fast_path");
    let writer = case.claim();
    fs::write(
        case.fixture.graph.repository.join("docs/source.txt"),
        "delivered",
    )
    .unwrap();
    let receipt = check(&case, &writer, &writer, "writer-check", None);
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let fanout = case.status().unwrap()["next_action"]["fanout_id"]
        .text()
        .unwrap();
    case.assess(&fanout);
    fs::write(
        case.fixture.graph.repository.join("docs/source.txt"),
        "changed-after-review-boundary",
    )
    .unwrap();
    let before = ledger_snapshot(&case);
    let resumed = resume(&case);
    assert!(resumed["next_action"]["kind"].is_text("replace_check"));
    assert!(resumed["next_action"]["replace_ref"].py_eq(&receipt["artifact_ref"]));
    assert!(!resumed["next_action"]["eligibility"].is_text("eligible"));
    state_error(
        case.run(&[
            "next",
            "--run-id",
            "RUN-1",
            "--claim",
            "--op-id",
            "stale-source-claim",
        ]),
        "REVIEW_SOURCE_CHANGED",
    );
    assert_eq!(before, ledger_snapshot(&case));
}

#[test]
fn resume_retains_original_task_and_resolved_human_decisions() {
    let case = fixture();
    let mut task = task(&case, "full_delivery");
    task.set(
        "required_human_decisions",
        Value::list(["release".into(), "scope".into()]),
    );
    case.fixture.initialize(&task, true).unwrap();
    let (reference, digest) = case.evidence(
        "scope-approval",
        &Value::object([("decision", "APPROVE".into())]),
    );
    case.run(&[
        "record",
        "approval",
        "--run-id",
        "RUN-1",
        "--approval-id",
        "scope",
        "--scope-ref",
        &reference,
        "--decision",
        "APPROVE",
        "--authority-ref",
        "authority:test",
        "--artifact-sha256",
        &digest,
        "--op-id",
        "scope-approval",
    ])
    .unwrap();
    let before = ledger_snapshot(&case);
    let resumed = resume(&case);
    assert!(
        resumed["continuation"]["pending_human_decisions"].py_eq(&Value::list(["release".into()]))
    );
    let digest =
        ids::sha256_bytes(&fs::read(case.fixture.graph.repository.join("docs/task.json")).unwrap());
    assert!(resumed["continuation"]["task"]["sha256"].is_text(&digest));
    assert!(resumed["continuation"]["task"]["ref"]
        .is_text(&format!("repo:docs/task.json#sha256={digest}")));
    assert_eq!(before, ledger_snapshot(&case));
}

#[test]
fn resume_does_not_reopen_exhausted_run_or_refund_inspection() {
    let mut case = fixture();
    case.fixture
        .initialize(&task(&case, "fast_path"), true)
        .unwrap();
    case.impact("fast_path");
    let mut writer = case.claim();
    case.run(&[
        "record",
        "budget-use",
        "--run-id",
        "RUN-1",
        "--budget-id",
        "file_reads",
        "--amount",
        "12",
        "--source-branch-id",
        &writer["branch_id"].text().unwrap(),
        "--op-id",
        "all-reads",
    ])
    .unwrap();
    for attempt in 0..2 {
        fail(&mut case, &writer);
        retry(&case, &writer, &format!("retry-{attempt}"));
        if attempt == 0 {
            writer = case.claim();
        }
    }
    let before = ledger_snapshot(&case);
    let resumed = resume(&case);
    assert!(resumed["status"].is_text("blocked"));
    assert!(resumed["blocked_reason"].is_text("RETRY_LIMIT"));
    assert!(resumed["next_action"]["kind"].is_text("blocked"));
    assert!(resumed["continuation"]["remaining_budgets"]["file_reads"].py_eq(&0_i64.into()));
    assert_eq!(before, ledger_snapshot(&case));
}
