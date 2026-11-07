#[path = "support/hardening_case.rs"]
mod hardening_case;
#[path = "support/recovery_case.rs"]
mod recovery_case;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        graph_recovery::validate_recovery_manifest,
        persistent_store::StateStore,
        state::FaultHook,
    },
    compat::{self, PyString, SerializationProfile, Value},
    domain::ids,
};
use hardening_case::{write_json, HardeningCase};
use recovery_case::{
    artifact_bytes, authorize, block, block_evidence, fail_review, ledger_snapshot, recover_args,
    recovery_manifest, run, state_error, BLOCK_REASON, RECOVERY_REASON,
};
use std::fs;

fn blocked_equivalence_fixture(mut case: HardeningCase) -> (HardeningCase, Value) {
    let task = case.fixture.graph.task("delivery", "fast_path", &[]);
    case.fixture.initialize(&task, true).unwrap();
    case.impact("fast_path");
    let engineer = case.claim();
    case.success_with(&engineer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let failed = loop {
        let reviewer = case.claim();
        if reviewer["node_key"].is_text("code_reviewer_bug_hunter") {
            let evidence = case.repo_artifact("failure", "missing-review-agent");
            fail_review(&mut case, &reviewer, evidence);
            break reviewer;
        }
        case.success_with(&reviewer, Some("APPROVE"), vec![])
            .unwrap();
    };
    let manifest = block_evidence(&mut case, BLOCK_REASON);
    let path = case.inbox(&manifest);
    block(&case, BLOCK_REASON, &path, "block-missing-review-agent");
    (case, failed)
}

fn find_architecture(status: &Value) -> &Value {
    status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .find(|branch| branch["node_key"].is_text("code_reviewer_architecture"))
        .unwrap()
}

fn ready_or_pending(branch: &Value) -> bool {
    branch["status"].is_text("ready") || branch["status"].is_text("pending")
}

#[test]
fn test_audited_recovery_reopens_equivalent_bug_review_and_preserves_architecture_gate() {
    assert_equivalent_recovery(HardeningCase::golden());
}

#[test]
fn test_equivalent_review_recovery_preserves_architecture_gate() {
    assert_equivalent_recovery(HardeningCase::stock());
}

fn assert_equivalent_recovery(case: HardeningCase) {
    let (mut case, failed) = blocked_equivalence_fixture(case);
    let before = case.status().unwrap();
    assert!(ready_or_pending(find_architecture(&before)));
    let equivalent = case.repo_artifact("delivery_review", "equivalent-bug-hunter-review");
    let authorization = recovery_manifest(&case, &failed, equivalent);
    let path = case
        .fixture
        .graph
        .store
        .inbox_root("albanian-live-translate", "RUN-1")
        .join("recover-equivalent-bug-hunter.json");
    write_json(&path, &authorization);
    case.fixture
        .graph
        .store
        .platform
        .set_mode(&path, 0o600)
        .unwrap();
    let args = recover_args(
        &failed,
        &authorization["reason_code"],
        &authorization["authority_ref"],
        &path,
        "recover-equivalent-bug-hunter",
    );
    let resumed = run(&case, &args).unwrap();
    assert!(resumed["code"].is_text("GRAPH_RECOVERED"));
    assert!(resumed["status"].is_text("active"));
    let after = case.status().unwrap();
    let recovered = after["branches"]
        .as_list()
        .unwrap()
        .iter()
        .find(|branch| branch["branch_id"].py_eq(&failed["branch_id"]))
        .unwrap();
    assert!(recovered["status"].is_text("ready"));
    assert_eq!(recovered["retry_count"].as_i64(), Some(1));
    let architecture = find_architecture(&after);
    assert!(ready_or_pending(architecture));
    assert!(!architecture["status"].is_text("succeeded"));

    let mut seen_architecture = false;
    loop {
        let branch = case.claim();
        if branch["node_key"].is_text("code_reviewer_bug_hunter") {
            let assignment = &branch["code_review_assignment"];
            assert!(assignment["required_skill"].is_text("bug-hunter-review"));
            assert!(assignment["skill_override"]["to"].is_text("bug-hunter-review"));
            let artifact = case.repo_artifact("delivery_review", "equivalent-bug-hunter-result");
            let manifest = Value::object([
                ("schema_version", 1_i64.into()),
                ("run_id", "RUN-1".into()),
                ("branch_id", branch["branch_id"].clone()),
                ("status", "succeeded".into()),
                ("output_kind", "delivery_review".into()),
                ("artifact_ref", artifact),
                ("evidence", Value::List(vec![])),
                ("decision", "APPROVE".into()),
                ("findings", Value::List(vec![])),
            ]);
            case.record_as(&branch, manifest, "result").unwrap();
            break;
        }
        seen_architecture |= branch["node_key"].is_text("code_reviewer_architecture");
        case.success_with(&branch, Some("APPROVE"), vec![]).unwrap();
    }
    assert!(seen_architecture || ready_or_pending(architecture));
    let remaining = case.status().unwrap();
    assert_eq!(
        find_architecture(&remaining)["status"].is_text("succeeded"),
        seen_architecture
    );
}

fn changed(value: &Value, key: &str, item: Value) -> Value {
    let mut changed = value.clone();
    changed.set(key, item);
    changed
}

fn stored(value: &Value) -> Value {
    Value::String(compat::dumps_text(value, SerializationProfile::STORED).unwrap())
}

#[test]
fn test_equivalent_review_authorization_rejects_other_failures_and_exhausted_retries() {
    let evidence = Value::object([
        ("kind", "finding".into()),
        (
            "ref",
            format!("repo:docs/proof.json#sha256={}", "a".repeat(64)).into(),
        ),
        ("sha256", "a".repeat(64).into()),
    ]);
    let env = Value::object([
        (
            "code_review_assignment",
            Value::object([("required_skill", "review-agent".into())]),
        ),
        ("inputs", Value::List(vec![])),
        ("effect_capabilities", Value::List(vec![])),
        ("evidence", Value::list([evidence.clone()])),
    ]);
    let branch = Value::object([
        ("branch_id", "failed-review".into()),
        ("node_key", "code_reviewer_bug_hunter".into()),
        ("status", "failed".into()),
        ("failure_code", "MISSING_REQUIRED_SKILL".into()),
        ("retry_count", 0_i64.into()),
        ("max_retries", 1_i64.into()),
        ("envelope_json", stored(&env)),
        ("result_digest", "b".repeat(64).into()),
    ]);
    let plan = Value::object([
        ("status", "approved".into()),
        ("authority_ref", "authority:test".into()),
        ("plan_digest", "c".repeat(64).into()),
    ]);
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "recovery_authorization".into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", branch["branch_id"].clone()),
        ("blocked_reason", BLOCK_REASON.into()),
        ("reason_code", "EQUIVALENT".into()),
        ("authority_ref", plan["authority_ref"].clone()),
        ("task_digest", "d".repeat(64).into()),
        ("plan_digest", plan["plan_digest"].clone()),
        (
            "failure_result_ref",
            format!("ledger:failed-review#sha256={}", "b".repeat(64)).into(),
        ),
        ("required_skill", "review-agent".into()),
        ("replacement_skill", "bug-hunter-review".into()),
        ("decision", "APPROVE".into()),
        (
            "equivalence_basis",
            "user-authorized-equivalent-review".into(),
        ),
        ("prior_input_refs", Value::List(vec![])),
        (
            "effect_capabilities_digest",
            ids::sha256_bytes(b"[]").into(),
        ),
        ("failure_evidence", Value::list([evidence.clone()])),
        ("evidence", Value::list([evidence])),
    ]);
    for version in [6_i64, 7] {
        let run = Value::object([
            ("run_id", "RUN-1".into()),
            ("state_schema_version", version.into()),
            ("task_digest", manifest["task_digest"].clone()),
            ("blocked_reason", BLOCK_REASON.into()),
        ]);
        let validate = |run: &Value, branch: &Value, plan: &Value| {
            validate_recovery_manifest(
                &manifest,
                run,
                branch,
                plan,
                &"EQUIVALENT".into(),
                &"authority:test".into(),
            )
        };
        assert!(validate(&run, &branch, &plan).unwrap().py_eq(&manifest));
        let forbidden = [
            ("node_key", "code_reviewer_architecture".into()),
            ("status", "succeeded".into()),
            ("failure_code", "POLICY_DENIED".into()),
            ("failure_code", "TOOL_FAILURE".into()),
            ("retry_count", 1_i64.into()),
            ("max_retries", 0_i64.into()),
            (
                "envelope_json",
                stored(&changed(
                    &env,
                    "code_review_assignment",
                    Value::object([("required_skill", "another-review".into())]),
                )),
            ),
        ];
        let mut cases = forbidden
            .into_iter()
            .map(|(key, value)| (run.clone(), changed(&branch, key, value), plan.clone()))
            .collect::<Vec<_>>();
        cases.extend([
            (
                run.clone(),
                branch.clone(),
                changed(&plan, "status", "pending".into()),
            ),
            (
                run.clone(),
                branch.clone(),
                changed(&plan, "authority_ref", "authority:other".into()),
            ),
        ]);
        cases.extend(
            [
                "SECURITY_DENIED",
                "POLICY_DENIED",
                "CONSOLIDATION_BLOCK",
                "BUDGET_LIMIT",
            ]
            .map(|reason| {
                (
                    changed(&run, "blocked_reason", reason.into()),
                    branch.clone(),
                    plan.clone(),
                )
            }),
        );
        for (run, branch, plan) in cases {
            let snapshot = || {
                ids::canonical_bytes(&Value::list([
                    manifest.clone(),
                    run.clone(),
                    branch.clone(),
                    plan.clone(),
                ]))
                .unwrap()
            };
            let before = snapshot();
            assert!(matches!(
                validate(&run, &branch, &plan),
                Err(EngineError::State(_))
            ));
            assert_eq!(before, snapshot());
        }
    }
}

#[test]
fn test_equivalent_recovery_cannot_reopen_aborted_or_security_policy_blocked_runs() {
    for version in [6, 7] {
        for reason in ["SECURITY_DENIED", "POLICY_DENIED", "stop"] {
            let mut case = HardeningCase::stock();
            let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
            case.fixture.initialize(&task, true).unwrap();
            if version == 7 {
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
            if reason == "stop" {
                case.run(&[
                    "abort",
                    "--run-id",
                    "RUN-1",
                    "--reason-code",
                    reason,
                    "--authority-ref",
                    "authority:test",
                    "--op-id",
                    "abort",
                ])
                .unwrap();
            } else {
                let evidence = block_evidence(&mut case, reason);
                let path = case.inbox(&evidence);
                block(&case, reason, &path, "block");
            }
            let before = case.status().unwrap();
            let ledger = ledger_snapshot(&case);
            let run = case
                .fixture
                .with_session(|session, _| {
                    Ok(session.first(GraphRead::Run, &["RUN-1".into()])?.unwrap())
                })
                .unwrap();
            let branch = &before["branches"].as_list().unwrap()[0];
            let manifest = Value::object([
                ("schema_version", 1_i64.into()),
                ("kind", "recovery_authorization".into()),
                ("run_id", "RUN-1".into()),
                ("branch_id", branch["branch_id"].clone()),
                ("blocked_reason", BLOCK_REASON.into()),
                ("reason_code", "EQUIVALENT".into()),
                ("authority_ref", "authority:test".into()),
                ("task_digest", run["task_digest"].clone()),
                (
                    "plan_digest",
                    before["execution_plan"]["plan_digest"].clone(),
                ),
                (
                    "failure_result_ref",
                    format!("ledger:failed#sha256={}", "a".repeat(64)).into(),
                ),
                ("required_skill", "review-agent".into()),
                ("replacement_skill", "bug-hunter-review".into()),
                ("decision", "APPROVE".into()),
                (
                    "equivalence_basis",
                    "user-authorized-equivalent-review".into(),
                ),
                ("prior_input_refs", Value::List(vec![])),
                ("effect_capabilities_digest", "b".repeat(64).into()),
                ("failure_evidence", Value::List(vec![])),
                ("evidence", Value::List(vec![])),
            ]);
            state_error(
                authorize(
                    &mut case,
                    branch,
                    &manifest,
                    &manifest,
                    "forbidden-recovery",
                ),
                if reason == "stop" {
                    "TERMINAL_RUN"
                } else {
                    "RECOVERY_NOT_PERMITTED"
                },
            );
            assert!(before.py_eq(&case.status().unwrap()));
            assert_eq!(ledger, ledger_snapshot(&case));
        }
    }
}

struct RejectRecoveryCommit;

impl FaultHook for RejectRecoveryCommit {
    fn observe(&self, point: &str) -> EngineResult<()> {
        if point == "before_commit" {
            return Err(EngineError::Injected("recover-before-commit".into()));
        }
        Ok(())
    }
}

#[test]
fn recovery_rollback_retains_failure_and_authorized_operation_replays() {
    let (mut case, failed) = blocked_equivalence_fixture(HardeningCase::golden());
    let evidence = case.repo_artifact("delivery_review", "equivalent-bug-hunter-review");
    let manifest = recovery_manifest(&case, &failed, evidence);
    let path = case.inbox(&manifest);
    let args = recover_args(
        &failed,
        &RECOVERY_REASON.into(),
        &manifest["authority_ref"],
        &path,
        "recover",
    );
    let ledger = ledger_snapshot(&case);
    let fixture = &case.fixture;
    let original = &fixture.graph.store;
    let store = StateStore::at_root(
        original.state_root.clone(),
        original.platform,
        original.database,
        &RejectRecoveryCommit,
    )
    .unwrap();
    let mut argv = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
    ];
    argv.extend(args.iter().map(|arg| PyString::from(arg.as_str())));
    let error = fixture.execute_with_store(&argv, &store).unwrap_err();
    assert!(matches!(error, EngineError::Injected(reason) if reason == "recover-before-commit"));
    assert_eq!(ledger, ledger_snapshot(&case));
    let recovered = run(&case, &args).unwrap();
    let replay = run(&case, &args).unwrap();
    assert!(recovered["code"].is_text("GRAPH_RECOVERED"));
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(recovered["state_revision"].py_eq(&replay["state_revision"]));
    let packet =
        compat::loads(artifact_bytes(&case, &recovered["repair_packet_ref"]).as_bytes()).unwrap();
    assert!(packet["failure_result_ref"].py_eq(&manifest["failure_result_ref"]));
    assert!(packet["recovery_authorization_ref"].py_eq(&recovered["recovery_authorization_ref"]));
    assert!(packet["next_permitted_action"].is_text("retry"));
    assert!(recovered["required_return_gates"].py_eq(&Value::list([
        "code_reviewer_bug_hunter".into(),
        "code_reviewer_architecture".into()
    ])));
}

#[test]
fn test_resume_preserves_running_work_and_rejects_changed_policy() {
    let mut case = HardeningCase::golden();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    case.fixture.initialize(&task, true).unwrap();
    let claimed = case.claim();
    let args = [
        "resume",
        "--run-id",
        "RUN-1",
        "--ack-degraded-permissions",
        "--ack-degraded-durability",
    ];
    let resumed = case.run(&args).unwrap();
    assert!(resumed["unresolved_running"].py_eq(&Value::list([claimed["branch_id"].clone()])));
    let policy = case
        .fixture
        .graph
        .repository
        .join(".codex/engineering-graph.json");
    let mut bytes = fs::read(&policy).unwrap();
    bytes.push(b' ');
    fs::write(policy, bytes).unwrap();
    state_error(case.run(&args), "CONFIG_CHANGED");
}
