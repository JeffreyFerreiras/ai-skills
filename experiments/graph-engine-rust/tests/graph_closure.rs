#[path = "support/hardening_case.rs"]
mod hardening_case;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        state::FaultHook,
    },
    compat::{self, PyString, Value},
};
use hardening_case::{write_json, HardeningCase};
use std::{collections::BTreeSet, fs, process::Command};

fn design_closure(case: &mut HardeningCase) {
    let task = case.fixture.graph.task("design_only", "design_only", &[]);
    case.fixture.initialize(&task, true).unwrap();
    case.impact("design_only");
    let tech = case.claim();
    assert!(tech["node_key"].is_text("tech_lead"));
    case.success(&tech);
    case.advance("design_inputs");
    let architect = case.claim();
    assert!(architect["node_key"].is_text("architect"));
    case.success_with(&architect, Some("APPROVE"), vec![])
        .unwrap();
    case.advance("design_collection");
    case.consolidate("design", "APPROVE", 0, vec![]);
    case.advance("design_consolidation");
}

fn start_advisory(case: &mut HardeningCase, approvals: &[&str]) -> Value {
    let mut task = case.fixture.graph.task("advisory", "advisory", &[]);
    task.set(
        "required_human_decisions",
        Value::list(approvals.iter().map(|id| (*id).into())),
    );
    case.fixture.initialize(&task, true).unwrap();
    case.impact("advisory");
    case.claim()
}

fn complete(case: &HardeningCase, op: &str) -> EngineResult<Value> {
    case.run(&["complete", "--run-id", "RUN-1", "--op-id", op])
}

fn check(case: &HardeningCase) -> Value {
    case.run(&[
        "check",
        "run",
        "--run-id",
        "RUN-1",
        "--check-id",
        "repo-check",
        "--op-id",
        "check-1",
    ])
    .unwrap()
}

fn record_acceptance(
    case: &HardeningCase,
    id: &str,
    artifact: &Value,
    op: &str,
) -> EngineResult<Value> {
    case.run(&[
        "record",
        "acceptance-evidence",
        "--run-id",
        "RUN-1",
        "--criterion-id",
        id,
        "--artifact-ref",
        &artifact["ref"].text().unwrap(),
        "--artifact-sha256",
        &artifact["sha256"].text().unwrap(),
        "--op-id",
        op,
    ])
}

fn approval(case: &HardeningCase, decision: &str, artifact: &Value) -> Value {
    case.run(&[
        "record",
        "approval",
        "--run-id",
        "RUN-1",
        "--approval-id",
        "DEC-001",
        "--scope-ref",
        &artifact["ref"].text().unwrap(),
        "--decision",
        decision,
        "--authority-ref",
        "authority:test",
        "--artifact-sha256",
        &artifact["sha256"].text().unwrap(),
        "--op-id",
        "approval-1",
    ])
    .unwrap()
}

fn expect_state(result: EngineResult<Value>, code: &str) {
    let error = result.unwrap_err();
    assert!(
        matches!(&error, EngineError::State(error) if error.code == code),
        "{error:?}"
    );
}

#[test]
fn test_advisory_route_has_only_read_only_entry_and_closure() {
    let mut case = HardeningCase::golden();
    let reviewer = start_advisory(&mut case, &[]);
    assert!(reviewer["node_key"].is_text("advisory_reviewer"));
    assert!(reviewer["role"].is_text("code_reviewer"));
    case.success(&reviewer);
    let status = case.status().unwrap();
    let roles: BTreeSet<_> = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .map(|branch| branch["role"].text().unwrap())
        .collect();
    assert_eq!(
        roles,
        ["impact_mapper", "code_reviewer"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    let keys: Vec<_> = status["joins"]
        .as_list()
        .unwrap()
        .iter()
        .map(|join| join["join_key"].text().unwrap())
        .collect();
    assert_eq!(keys, vec!["closure"]);
}

#[test]
fn test_completion_requires_acceptance_and_passing_check() {
    let mut case = HardeningCase::golden();
    design_closure(&mut case);
    expect_state(
        complete(&case, "complete-too-early"),
        "ACCEPTANCE_EVIDENCE_INCOMPLETE",
    );
    let evidence = case.repo_artifact("acceptance_evidence", "acceptance");
    record_acceptance(&case, "AC-001", &evidence, "acceptance-1").unwrap();
    check(&case);
    let result = complete(&case, "complete-1").unwrap();
    assert!(result["status"].is_text("complete"));
    let replay = complete(&case, "complete-1").unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&result["state_revision"]));
    expect_state(complete(&case, "complete-late"), "TERMINAL_RUN");
}

#[test]
fn test_cli_exit_codes_are_stable() {
    let mut case = HardeningCase::golden();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    let task_path = case.fixture.graph.repository.join("docs/task.json");
    write_json(&task_path, &task);
    fn invoke(case: &HardeningCase, args: &[&str]) -> (i32, Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_graphctl"))
            .arg("--repo")
            .arg(&case.fixture.graph.repository)
            .arg("--state-root")
            .arg(&case.fixture.graph.runtime_root)
            .args(args)
            .output()
            .unwrap();
        let result =
            compat::loads(&output.stdout).unwrap_or_else(|error| panic!("{error:?}: {output:?}"));
        (output.status.code().unwrap(), result)
    }
    let (code, initialized) = invoke(
        &case,
        &[
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
            "init",
            "--run-id",
            "RUN-1",
            "--task-brief",
            task_path.to_str().unwrap(),
            "--op-id",
            "cli-init",
        ],
    );
    assert_eq!(code, 0);
    assert!(initialized["code"].is_text("INITIALIZED"));
    let (code, _) = invoke(
        &case,
        &[
            "record",
            "plan-approval",
            "--run-id",
            "RUN-1",
            "--plan-digest",
            &initialized["execution_plan_digest"].text().unwrap(),
            "--decision",
            "APPROVE",
            "--authority-ref",
            "authority:test",
            "--op-id",
            "cli-plan-approval",
        ],
    );
    assert_eq!(code, 0);
    let (code, _) = invoke(
        &case,
        &[
            "next",
            "--run-id",
            "RUN-1",
            "--claim",
            "--op-id",
            "cli-claim",
        ],
    );
    assert_eq!(code, 0);
    let (code, waiting) = invoke(&case, &["next", "--run-id", "RUN-1", "--all"]);
    assert_eq!(code, 2);
    assert!(waiting["code"].is_text("NOT_READY"));
    let abort = |op| {
        [
            "abort",
            "--run-id",
            "RUN-1",
            "--reason-code",
            "STOP",
            "--authority-ref",
            "authority:test",
            "--op-id",
            op,
        ]
    };
    let (code, conflict) = invoke(&case, &abort("cli-claim"));
    assert_eq!(code, 5);
    assert!(conflict["code"].is_text("OPERATION_CONFLICT"));
    let evidence = case.repo_artifact("block", "block-evidence-0");
    let manifest = case.inbox(&Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "block".into()),
        ("run_id", "RUN-1".into()),
        ("reason_code", "STOP".into()),
        ("evidence", Value::list([evidence])),
    ]));
    let (code, blocked) = invoke(
        &case,
        &[
            "block",
            "--run-id",
            "RUN-1",
            "--reason-code",
            "STOP",
            "--evidence-manifest",
            manifest.to_str().unwrap(),
            "--op-id",
            "cli-block",
        ],
    );
    assert_eq!(code, 3);
    assert!(blocked["code"].is_text("GRAPH_BLOCKED"));
    let (code, _) = invoke(&case, &abort("cli-abort"));
    assert_eq!(code, 0);
    let (code, invalid) = invoke(
        &case,
        &["complete", "--run-id", "RUN-1", "--op-id", "cli-complete"],
    );
    assert_eq!(code, 4);
    assert!(invalid["code"].is_text("TERMINAL_RUN"));
}

struct RejectCompletion;

impl FaultHook for RejectCompletion {
    fn observe(&self, stage: &str) -> EngineResult<()> {
        if stage == "before_commit" {
            return Err(EngineError::Injected("complete-before-commit".into()));
        }
        Ok(())
    }
}

#[test]
fn legacy_closure_validates_evidence_and_approval_before_atomic_completion() {
    let mut case = HardeningCase::golden();
    let reviewer = start_advisory(&mut case, &["DEC-001"]);
    expect_state(complete(&case, "early"), "CLOSURE_NOT_READY");
    case.success(&reviewer);
    let join = case.open_join("closure")["join_id"].text().unwrap();
    expect_state(
        case.run(&[
            "join",
            "advance",
            "--run-id",
            "RUN-1",
            "--join-id",
            &join,
            "--op-id",
            "wrong-command",
        ]),
        "COMPLETE_COMMAND_REQUIRED",
    );
    let evidence = case.repo_artifact("acceptance_evidence", "accepted-legacy");
    let conflict = case.repo_artifact("acceptance_evidence", "conflicting-legacy");
    expect_state(
        record_acceptance(&case, "AC-404", &evidence, "unknown"),
        "UNKNOWN_CRITERION",
    );
    let first = record_acceptance(&case, "AC-001", &evidence, "acceptance").unwrap();
    let replay = record_acceptance(&case, "AC-001", &evidence, "acceptance").unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(first["state_revision"].py_eq(&replay["state_revision"]));
    expect_state(
        record_acceptance(&case, "AC-001", &conflict, "acceptance"),
        "OPERATION_CONFLICT",
    );
    expect_state(
        record_acceptance(&case, "AC-001", &conflict, "duplicate"),
        "EVIDENCE_CONFLICT",
    );
    expect_state(complete(&case, "no-check"), "REQUIRED_CHECKS_INCOMPLETE");
    assert!(check(&case)["outcome"].is_text("PASS"));
    expect_state(
        complete(&case, "no-approval"),
        "REQUIRED_APPROVALS_INCOMPLETE",
    );
    assert!(approval(&case, "APPROVE", &evidence)["code"].is_text("APPROVAL_RECORDED"));

    let before = case.status().unwrap();
    let fixture = &case.fixture;
    let original = &fixture.graph.store;
    let store = StateStore::at_root(
        original.state_root.clone(),
        original.platform,
        original.database,
        &RejectCompletion,
    )
    .unwrap();
    let argv = [
        "--repo",
        fixture.graph.repository.to_str().unwrap(),
        "complete",
        "--run-id",
        "RUN-1",
        "--op-id",
        "complete",
    ]
    .map(PyString::from);
    let error = fixture.execute_with_store(&argv, &store).unwrap_err();
    assert!(matches!(error, EngineError::Injected(reason) if reason == "complete-before-commit"));
    let after = case.status().unwrap();
    for key in ["state_revision", "status", "joins"] {
        assert!(before[key].py_eq(&after[key]));
    }
    let result = complete(&case, "complete").unwrap();
    assert!(result["code"].is_text("COMPLETE"));
    let replay = record_acceptance(&case, "AC-001", &evidence, "acceptance").unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    case.fixture
        .with_session(|session, _| {
            let closure = session
                .first(GraphRead::JoinForRun, &[join.into(), "RUN-1".into()])?
                .unwrap();
            assert!(closure["status"].is_text("sealed"));
            assert!(compat::loads_value(&closure["result_json"])?["complete"].truthy());
            Ok(())
        })
        .unwrap();
}

#[test]
fn failing_checks_and_rejected_approvals_prevent_legacy_completion() {
    for rejected in [false, true] {
        let mut case = HardeningCase::golden();
        if !rejected {
            case.edit_policy(|policy| {
                let mut checks = policy["required_checks"].clone();
                let mut check = checks["repo-check"].clone();
                let mut argv = check["argv"].as_list().unwrap().to_vec();
                argv[2] = "import sys; sys.exit(1)".into();
                check.set("argv", Value::List(argv));
                checks.set("repo-check", check);
                policy.set("required_checks", checks);
            });
        }
        let reviewer = start_advisory(&mut case, &["DEC-001"]);
        case.success(&reviewer);
        let evidence = case.repo_artifact("acceptance_evidence", "acceptance");
        record_acceptance(&case, "AC-001", &evidence, "acceptance").unwrap();
        assert!(approval(&case, "REJECT", &evidence)["code"].is_text("APPROVAL_RECORDED"));
        assert!(check(&case)["outcome"].is_text(if rejected { "PASS" } else { "FAIL" }));
        expect_state(
            complete(&case, "ineligible"),
            if rejected {
                "APPROVAL_REJECTED"
            } else {
                "REQUIRED_CHECKS_INCOMPLETE"
            },
        );
        assert!(case.open_join("closure")["status"].is_text("open"));
    }
}

fn enable_coverage(case: &mut HardeningCase) {
    let graph = &case.fixture.graph;
    let config = graph.root.join("git-config-empty");
    fs::write(&config, b"").unwrap();
    let template = graph.root.join("git-template-empty");
    fs::create_dir(&template).unwrap();
    for arguments in [
        &["init"][..],
        &["add", "docs/engineering-graph.md"],
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
    ] {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&graph.repository)
            .env("GIT_CONFIG_GLOBAL", &config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TEMPLATE_DIR", &template)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let coverage = compat::loads(br#"{"schema_version":1,"kind":"check_coverage","checks":[{"check_id":"repo-check","relevant_inputs":["docs/engineering-graph.md"],"complete":true}]}"#).unwrap();
    let path = case.inbox(&coverage);
    case.run(&[
        "evidence",
        "enable",
        "--run-id",
        "RUN-1",
        "--contract-version",
        "2",
        "--coverage-manifest",
        path.to_str().unwrap(),
        "--op-id",
        "enable",
    ])
    .unwrap();
}

fn run_bound_check(
    case: &HardeningCase,
    executor: &Value,
    source: &Value,
    op: &str,
) -> EngineResult<Value> {
    case.run(&[
        "check",
        "run",
        "--run-id",
        "RUN-1",
        "--check-id",
        "repo-check",
        "--executor-branch-id",
        &executor["branch_id"].text().unwrap(),
        "--executor-attempt-id",
        &executor["attempt_id"].text().unwrap(),
        "--executor-claim-token",
        &executor["claim_token"].text().unwrap(),
        "--source-branch-id",
        &source["branch_id"].text().unwrap(),
        "--source-attempt-id",
        &source["attempt_id"].text().unwrap(),
        "--source-claim-digest",
        &source["claim_digest"].text().unwrap(),
        "--op-id",
        op,
    ])
}

#[test]
fn format7_design_requires_an_eligible_check_without_expanding_role_authority() {
    let mut case = HardeningCase::golden();
    let command = Value::object([
        ("effect", "command".into()),
        ("action", "run".into()),
        ("target_ref", "npm-run-check".into()),
    ]);
    let mut task = case.fixture.graph.task("design_only", "design_only", &[]);
    let mut authority = task["authority"].clone();
    let mut capabilities = authority["capabilities"].as_list().unwrap().to_vec();
    capabilities.push(command);
    authority.set("capabilities", Value::List(capabilities));
    task.set("authority", authority);
    case.fixture.initialize(&task, true).unwrap();
    case.impact("design_only");
    enable_coverage(&mut case);
    let tech = case.claim();
    assert!(tech["node_key"].is_text("tech_lead"));
    case.success(&tech);
    case.advance("design_inputs");
    let architect = case.claim();
    case.success_with(&architect, Some("APPROVE"), vec![])
        .unwrap();
    case.advance("design_collection");
    case.consolidate("design", "APPROVE", 0, vec![]);
    let evidence = case.repo_artifact("acceptance_evidence", "accepted-design");
    expect_state(
        record_acceptance(&case, "AC-001", &evidence, "accept"),
        "REQUIRED_CHECKS_INCOMPLETE",
    );
    expect_state(
        run_bound_check(&case, &tech, &tech, "bound-check"),
        "CHECK_AUTHORITY_REQUIRED",
    );
    case.advance("design_consolidation");
    expect_state(complete(&case, "complete"), "REQUIRED_CHECKS_INCOMPLETE");
    assert!(case.open_join("closure")["status"].is_text("open"));
}

#[test]
fn format7_advisory_keeps_read_only_authority_and_requires_an_eligible_check() {
    let mut case = HardeningCase::golden();
    let advisory = start_advisory(&mut case, &[]);
    enable_coverage(&mut case);
    case.success(&advisory);
    let evidence = case.repo_artifact("acceptance_evidence", "advisory-evidence");
    assert!(advisory["effect_capabilities"]
        .as_list()
        .unwrap()
        .iter()
        .all(|cap| {
            cap["effect"].is_text("filesystem_read") || cap["effect"].is_text("external_read")
        }));
    expect_state(
        record_acceptance(&case, "AC-001", &evidence, "accept"),
        "REQUIRED_CHECKS_INCOMPLETE",
    );
    expect_state(complete(&case, "complete"), "REQUIRED_CHECKS_INCOMPLETE");
    assert!(case.open_join("closure")["status"].is_text("open"));
}
