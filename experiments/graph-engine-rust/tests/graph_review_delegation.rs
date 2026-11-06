#[allow(dead_code)]
#[path = "support/evidence_schema.rs"]
mod evidence_schema;
#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[allow(dead_code)]
#[path = "support/recovery_case.rs"]
mod recovery_case;
#[path = "support/review_continuation.rs"]
mod review_continuation;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        state::FaultHook,
    },
    compat::{self, PyString, Value},
    domain::ids,
};
use hardening_case::HardeningCase;
use recovery_case::{ledger_snapshot, run, state_error};
use std::path::{Path, PathBuf};

fn delegation_limits(rounds: i64, cost: i64) -> Value {
    Value::object([
        ("max_depth", 1_i64.into()),
        ("max_children_per_request", 3_i64.into()),
        ("max_children_per_run", 6_i64.into()),
        ("max_request_rounds", rounds.into()),
        ("max_weighted_dispatch_cost", cost.into()),
    ])
}

fn delegation_config(policy: bool, cost: i64) -> Value {
    let mut assignment = Value::object([
        ("assignment_id", "correctness".into()),
        ("allowed_reason_codes", Value::list(["CROSS_FILE".into()])),
        (
            "allowed_evidence_kinds",
            Value::list(["finding".into(), "implementation_handoff".into()]),
        ),
        ("scope_refs", Value::list(["repo:docs/".into()])),
        ("max_instances", 2_i64.into()),
    ]);
    if policy {
        assignment.set("role", "security_reviewer".into());
        assignment.set("model", "gpt-6.1-sol".into());
        assignment.set("reasoning_effort", "high".into());
        assignment.set(
            "review_lens",
            "Cross-file correctness and concurrency".into(),
        );
        assignment.set(
            "prompt_template",
            "Review only the approved evidence and scope.".into(),
        );
        assignment.set("dispatch_weight", 3_i64.into());
    } else {
        assignment.set("allowed_acceptance_ids", Value::list(["AC-001".into()]));
    }
    Value::object([
        (
            "limits",
            delegation_limits(if policy { 2 } else { 1 }, cost),
        ),
        ("assignments", Value::list([assignment])),
    ])
}

fn to_delivery_review(primary: &str, cost: i64) -> (HardeningCase, Value) {
    to_delivery_review_with(primary, cost, |_| {}, |_, _| {})
}

fn to_delivery_review_with(
    primary: &str,
    cost: i64,
    edit_policy: impl FnOnce(&mut Value),
    edit_task: impl FnOnce(&mut Value, &Value),
) -> (HardeningCase, Value) {
    let mut case = HardeningCase::stock();
    case.edit_policy(|policy| {
        policy.set("reviewer_delegation", delegation_config(true, 15));
        edit_policy(policy);
    });
    let mut task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    task.set("reviewer_delegation", delegation_config(false, cost));
    edit_task(
        &mut task,
        &compat::loads(&case.fixture.graph.policy_bytes).unwrap(),
    );
    case.fixture.initialize(&task, true).unwrap();
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
    let engineer = case.claim();
    case.success_with(&engineer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let mut parent = None;
    for _ in 0..5 {
        let reviewer = case.claim();
        if reviewer["node_key"].is_text(primary) {
            parent = Some(reviewer);
        } else {
            case.success_with(&reviewer, Some("APPROVE"), vec![])
                .unwrap();
        }
    }
    (case, parent.expect("reviewer present"))
}

fn manifests(parent: &Value, ordinals: &[i64]) -> (Value, Value) {
    let inputs = parent["inputs"].as_list().unwrap();
    let evidence = if parent["review_continuation"].truthy() {
        &inputs[0]["content"]["preliminary"].as_list().unwrap()[0]["evidence"]
            .as_list()
            .unwrap()[0]
    } else {
        inputs
            .iter()
            .find(|item| item["kind"].is_text("implementation_handoff"))
            .unwrap()
    };
    let fence = [
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("parent_branch_id", parent["branch_id"].clone()),
        ("parent_attempt_id", parent["attempt_id"].clone()),
    ];
    let mut preliminary = Value::object(fence.clone());
    preliminary.set("kind", "review_preliminary".into());
    preliminary.set("generation", 0_i64.into());
    preliminary.set("findings", Value::List(vec![]));
    preliminary.set(
        "evidence",
        Value::list([Value::object([
            ("evidence_id", "E-1".into()),
            ("kind", evidence["kind"].clone()),
            ("sha256", evidence["sha256"].clone()),
        ])]),
    );
    let mut request = Value::object(fence);
    request.set("kind", "review_fanout_request".into());
    request.set("round", 1_i64.into());
    request.set(
        "members",
        Value::list(ordinals.iter().map(|ordinal| {
            Value::object([
                ("assignment_id", "correctness".into()),
                ("ordinal", (*ordinal).into()),
                ("reason_code", "CROSS_FILE".into()),
                ("acceptance_ids", Value::list(["AC-001".into()])),
                ("evidence_ids", Value::list(["E-1".into()])),
            ])
        })),
    );
    (preliminary, request)
}

fn write_manifests(
    case: &mut HardeningCase,
    parent: &Value,
    ordinals: &[i64],
) -> (PathBuf, PathBuf) {
    let (preliminary, request) = manifests(parent, ordinals);
    (case.inbox(&preliminary), case.inbox(&request))
}

fn request_args(
    parent: &Value,
    preliminary: &Path,
    request: &Path,
    token: &str,
    authority: &str,
    op: &str,
) -> Vec<String> {
    let branch = parent["branch_id"].text().unwrap();
    let attempt = parent["attempt_id"].text().unwrap();
    [
        "record",
        "review-fanout",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch,
        "--attempt-id",
        &attempt,
        "--claim-token",
        token,
        "--preliminary-manifest",
        preliminary.to_str().unwrap(),
        "--request-manifest",
        request.to_str().unwrap(),
        "--authority-ref",
        authority,
        "--op-id",
        op,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn assessment(case: &mut HardeningCase, request: &Value, label: &str) -> Value {
    let resources = Value::object([
        ("writable_paths", Value::List(vec![])),
        ("mutable_state_refs", Value::List(vec![])),
        ("exclusive_device_refs", Value::List(vec![])),
        ("services", Value::List(vec![])),
    ]);
    Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", "RUN-1".into()),
        ("fanout_id", request["request_slot_id"].clone()),
        (
            "members",
            Value::list(
                request["child_branch_ids"]
                    .as_list()
                    .unwrap()
                    .iter()
                    .map(|child| {
                        Value::object([
                            ("branch_id", child.clone()),
                            ("resources", resources.clone()),
                        ])
                    }),
            ),
        ),
        ("dependencies", Value::List(vec![])),
        (
            "evidence",
            Value::list([case.repo_artifact("finding", label)]),
        ),
    ])
}

fn assessment_args(request: &Value, path: &Path, op: &str) -> Vec<String> {
    let slot = request["request_slot_id"].text().unwrap();
    [
        "record",
        "review-fanout-assessment",
        "--run-id",
        "RUN-1",
        "--request-slot-id",
        &slot,
        "--assessment-manifest",
        path.to_str().unwrap(),
        "--authority-ref",
        "authority:test",
        "--op-id",
        op,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn record_request(case: &mut HardeningCase, parent: &Value, ordinals: &[i64], op: &str) -> Value {
    let (preliminary, request) = write_manifests(case, parent, ordinals);
    run(
        case,
        &request_args(
            parent,
            &preliminary,
            &request,
            &parent["claim_token"].text().unwrap(),
            "authority:test",
            op,
        ),
    )
    .unwrap()
}

fn assert_forbidden_parent(key: &str) {
    let (mut case, parent) = to_delivery_review(key, 15);
    let (preliminary, request) = write_manifests(&mut case, &parent, &[1]);
    let args = request_args(
        &parent,
        &preliminary,
        &request,
        &parent["claim_token"].text().unwrap(),
        "authority:test",
        "round-1-request",
    );
    state_error(run(&case, &args), "DELEGATION_PARENT_FORBIDDEN");
}

#[test]
fn test_bug_hunter_cannot_delegate_its_builtin_review() {
    assert_forbidden_parent("code_reviewer_bug_hunter");
}

#[test]
fn test_naming_review_cannot_request_specialist_reviewers() {
    assert_forbidden_parent("code_reviewer_naming");
}

#[test]
fn test_old_parent_fence_is_rejected_atomically() {
    let (mut case, parent) = to_delivery_review("code_reviewer", 15);
    let (preliminary, request) = write_manifests(&mut case, &parent, &[1]);
    let before = case.status().unwrap()["state_revision"].clone();
    let args = request_args(
        &parent,
        &preliminary,
        &request,
        "wrong",
        "authority:test",
        "old-fence",
    );
    state_error(run(&case, &args), "ATTEMPT_FENCE_MISMATCH");
    assert!(case.status().unwrap()["state_revision"].py_eq(&before));
}

fn dependency(children: &[Value], before: usize, after: usize, reason: &str) -> Value {
    Value::object([
        ("before_branch_id", children[before].clone()),
        ("after_branch_id", children[after].clone()),
        ("reason", reason.into()),
    ])
}

#[test]
fn test_review_assessment_reconstruction_rejects_invalid_and_tampered_state() {
    let (mut case, parent) = to_delivery_review("code_reviewer", 15);
    let recorded = record_request(
        &mut case,
        &parent,
        &[1, 2],
        "assessment-reconstruction-request",
    );
    let children = recorded["child_branch_ids"].as_list().unwrap();
    let base = assessment(&mut case, &recorded, "reconstruction-assessment");
    let mut wrong_kind = base.clone();
    wrong_kind.set("kind", "wrong_kind".into());
    let mut missing_member = base.clone();
    missing_member.set(
        "members",
        Value::list([base["members"].as_list().unwrap()[0].clone()]),
    );
    let mut cycle = base.clone();
    cycle.set(
        "dependencies",
        Value::list([
            dependency(children, 0, 1, "first"),
            dependency(children, 1, 0, "cycle"),
        ]),
    );
    let mut writable = base.clone();
    let path_name = if cfg!(windows) {
        "Docs/Shared.cs"
    } else {
        "docs/shared.cs"
    };
    let mut members = base["members"].as_list().unwrap().to_vec();
    for (member, path) in members.iter_mut().zip(["docs/shared.cs", path_name]) {
        let mut resources = member["resources"].clone();
        resources.set(
            "writable_paths",
            Value::list([Value::object([
                ("path", path.into()),
                ("scope", "exact".into()),
            ])]),
        );
        member.set("resources", resources);
    }
    writable.set("members", Value::List(members));
    let revision = case.status().unwrap()["state_revision"].clone();
    for (index, invalid) in [wrong_kind, missing_member, cycle, writable]
        .into_iter()
        .enumerate()
    {
        let path = case.inbox(&invalid);
        let result = run(
            &case,
            &assessment_args(
                &recorded,
                &path,
                &format!("invalid-review-assessment-{index}"),
            ),
        );
        assert!(
            matches!(
                result,
                Err(EngineError::Contract(_) | EngineError::State(_))
            ),
            "{result:?}"
        );
        assert!(case.status().unwrap()["state_revision"].py_eq(&revision));
    }
    let path = case.inbox(&base);
    run(
        &case,
        &assessment_args(&recorded, &path, "valid-review-assessment"),
    )
    .unwrap();
    let slot = recorded["request_slot_id"].text().unwrap();
    case.fixture
        .with_session(|session, _| {
            session
                .connection()
                .execute(
                    "INSERT INTO review_delegation_dependencies VALUES(?,?,?,?)",
                    [
                        slot.as_str(),
                        &children[0].text().unwrap(),
                        &children[1].text().unwrap(),
                        "tampered",
                    ],
                )
                .unwrap();
            Ok(())
        })
        .unwrap();
    state_error(case.status(), "DELEGATION_ASSESSMENT_INVALID");
    case.fixture.with_session(|session, _| {
        let connection = session.connection();
        connection.execute_batch("DROP TRIGGER review_dependency_delete_guard; DROP TRIGGER review_fanout_update_guard;").unwrap();
        connection.execute("DELETE FROM review_delegation_dependencies WHERE request_slot_id=?", [&slot]).unwrap();
        connection.execute("UPDATE review_delegation_fanouts SET host_identity='tampered-host' WHERE request_slot_id=?", [&slot]).unwrap();
        Ok(())
    }).unwrap();
    state_error(
        case.run(&["resume", "--run-id", "RUN-1"]),
        "DELEGATION_ASSESSMENT_INVALID",
    );
}

struct RejectCommit;

impl FaultHook for RejectCommit {
    fn observe(&self, stage: &str) -> EngineResult<()> {
        if stage == "before_commit" {
            return Err(EngineError::Injected("delegation-before-commit".into()));
        }
        Ok(())
    }
}

fn assert_rollback(case: &HardeningCase, args: &[String]) {
    let original = &case.fixture.graph.store;
    let store = StateStore::at_root(
        original.state_root.clone(),
        original.platform,
        original.database,
        &RejectCommit,
    )
    .unwrap();
    let mut argv = vec![
        "--repo".into(),
        case.fixture.graph.repository.to_str().unwrap().into(),
    ];
    argv.extend(args.iter().map(|arg| PyString::from(arg.as_str())));
    let before = ledger_snapshot(case);
    let error = case.fixture.execute_with_store(&argv, &store).unwrap_err();
    assert!(
        matches!(&error, EngineError::Injected(reason) if reason == "delegation-before-commit"),
        "{error:?}"
    );
    assert_eq!(ledger_snapshot(case), before);
}

#[test]
fn delegation_request_and_assessment_rollback_and_replay_at_cost_boundary() {
    let (mut case, parent) = to_delivery_review("code_reviewer", 6);
    let (preliminary, request) = write_manifests(&mut case, &parent, &[1, 2]);
    let token = parent["claim_token"].text().unwrap();
    let args = request_args(
        &parent,
        &preliminary,
        &request,
        &token,
        "authority:test",
        "delegate",
    );
    assert_rollback(&case, &args);
    let recorded = run(&case, &args).unwrap();
    assert!(recorded["dispatch_cost"].py_eq(&6_i64.into()));
    let replay = run(&case, &args).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&recorded["state_revision"]));
    let status = case.status().unwrap();
    let waiting = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .find(|branch| branch["branch_id"].py_eq(&parent["branch_id"]))
        .unwrap();
    assert!(waiting["status"].is_text("waiting_for_review_children"));
    for key in ["attempt_id", "claim_digest", "lease_expires_at"] {
        assert!(waiting[key].is_null());
    }
    let conflict = request_args(
        &parent,
        &preliminary,
        &request,
        &token,
        "authority:other",
        "fresh",
    );
    state_error(run(&case, &conflict), "DELEGATION_SLOT_CONFLICT");
    let mut manifest = assessment(&mut case, &recorded, "sequenced-assessment");
    let children = recorded["child_branch_ids"].as_list().unwrap();
    manifest.set(
        "dependencies",
        Value::list([dependency(children, 0, 1, "sequenced")]),
    );
    let path = case.inbox(&manifest);
    let args = assessment_args(&recorded, &path, "assess");
    assert_rollback(&case, &args);
    let assessed = run(&case, &args).unwrap();
    assert!(assessed["ready_branch_ids"].py_eq(&Value::list([children[0].clone()])));
    let replay = run(&case, &args).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&assessed["state_revision"]));
    let child = case.claim();
    assert!(child["branch_id"].py_eq(&children[0]));
    assert!(child["parent_branch_id"].py_eq(&parent["branch_id"]));
    assert!(child["depth"].py_eq(&1_i64.into()));
    assert!(child["inputs"].py_eq(&Value::List(vec![])));
    assert!(
        child["effect_capabilities"].py_eq(&Value::list([Value::object([
            ("effect", "filesystem_read".into()),
            ("action", "read".into()),
            ("target_ref", "repo:docs/".into()),
        ])]))
    );
    assert!(!child["output_contract"]["artifact_required"].truthy());
}

#[test]
fn delegation_rejects_weight_above_approved_budget() {
    let (mut case, parent) = to_delivery_review("code_reviewer", 5);
    let (preliminary, request) = write_manifests(&mut case, &parent, &[1, 2]);
    let before = ledger_snapshot(&case);
    let args = request_args(
        &parent,
        &preliminary,
        &request,
        &parent["claim_token"].text().unwrap(),
        "authority:test",
        "cost-denied",
    );
    state_error(run(&case, &args), "DELEGATION_COST_LIMIT");
    assert_eq!(ledger_snapshot(&case), before);
}

fn child_finding() -> Value {
    Value::object([
        ("finding_id", "REV-101".into()),
        ("acceptance_id", "AC-001".into()),
        ("location", "docs/Cache.cs".into()),
        ("defect_id", "RACE".into()),
        ("summary", "Cross-file race".into()),
        ("fix_variant", "Serialize updates".into()),
        ("evidence_ids", Value::list(["E-1".into()])),
    ])
}

fn child_manifest(child: &Value, status: &str, findings: Vec<Value>) -> Value {
    let mut manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", child["branch_id"].clone()),
        ("status", status.into()),
        (
            "output_kind",
            child["output_contract"]["artifact_kind"].clone(),
        ),
        ("evidence", Value::List(vec![])),
        ("findings", Value::List(findings)),
    ]);
    if status == "succeeded" {
        manifest.set("decision", "REVISE".into());
    } else {
        manifest.set("failure_code", "REVIEW_FAILED".into());
    }
    manifest
}

fn child_result_args(
    case: &mut HardeningCase,
    child: &Value,
    mut manifest: Value,
    op: &str,
) -> Vec<String> {
    let branch = child["branch_id"].text().unwrap();
    let attempt = child["attempt_id"].text().unwrap();
    let token = child["claim_token"].text().unwrap();
    manifest.set("attempt_id", child["attempt_id"].clone());
    manifest.set("claim_digest", ids::sha256_bytes(token.as_bytes()).into());
    let path = case.inbox(&manifest);
    [
        "record",
        "branch-result",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch,
        "--attempt-id",
        &attempt,
        "--claim-token",
        &token,
        "--result-manifest",
        path.to_str().unwrap(),
        "--op-id",
        op,
    ]
    .map(str::to_owned)
    .to_vec()
}

fn assessed_pair(case: &mut HardeningCase, parent: &Value) -> Value {
    let recorded = record_request(case, parent, &[1, 2], "request-pair");
    let mut manifest = assessment(case, &recorded, "ordered-pair");
    let children = recorded["child_branch_ids"].as_list().unwrap();
    manifest.set(
        "dependencies",
        Value::list([dependency(children, 0, 1, "sequenced")]),
    );
    let path = case.inbox(&manifest);
    run(case, &assessment_args(&recorded, &path, "assess-pair")).unwrap();
    recorded
}

fn read_child(case: &HardeningCase, id: &Value) -> Value {
    case.fixture
        .with_session(|session, _| {
            Ok(session
                .first(GraphRead::Node, std::slice::from_ref(id))?
                .unwrap())
        })
        .unwrap()
}

fn contract_error(result: EngineResult<Value>, code: &str) {
    let error = result.unwrap_err();
    assert!(
        matches!(&error, EngineError::Contract(failure) if failure.code == code),
        "{error:?}"
    );
}

#[test]
fn delegated_child_result_checks_frozen_assignment_and_promotes_only_after_commit() {
    let (mut case, parent) = to_delivery_review("code_reviewer", 15);
    let request = assessed_pair(&mut case, &parent);
    let children = request["child_branch_ids"].as_list().unwrap();
    let child = case.claim();
    assert!(child["branch_id"].py_eq(&children[0]));
    let finding = child_finding();
    let mut acceptance = finding.clone();
    acceptance.set("acceptance_id", "AC-999".into());
    let mut evidence = finding.clone();
    evidence.set("evidence_ids", Value::list(["E-OTHER".into()]));
    let mut location = finding.clone();
    location.set("location", "src/Cache.cs".into());
    let mut identifier = finding.clone();
    identifier.set("finding_id", "invalid".into());
    let before = ledger_snapshot(&case);
    for (index, (invalid, code)) in [
        (vec![acceptance], "DELEGATION_ACCEPTANCE_FORBIDDEN"),
        (vec![evidence], "FINDING_EVIDENCE_MISSING"),
        (vec![location], "DELEGATION_SCOPE_FORBIDDEN"),
        (vec![finding.clone(), finding.clone()], "DUPLICATE_ID"),
        (vec![identifier], "INVALID_ID"),
    ]
    .into_iter()
    .enumerate()
    {
        let args = child_result_args(
            &mut case,
            &child,
            child_manifest(&child, "succeeded", invalid),
            &format!("invalid-child-{index}"),
        );
        contract_error(run(&case, &args), code);
        assert_eq!(ledger_snapshot(&case), before);
    }
    let mut manifest = child_manifest(&child, "succeeded", vec![finding.clone()]);
    manifest.set(
        "artifact_ref",
        case.repo_artifact("specialist_review", "unapproved-child-artifact"),
    );
    let args = child_result_args(
        &mut case,
        &child,
        manifest.clone(),
        "unapproved-child-artifact",
    );
    contract_error(run(&case, &args), "DELEGATION_ARTIFACT_FORBIDDEN");
    manifest.set("artifact_ref", Value::Null);
    manifest.set(
        "evidence",
        Value::list([case.repo_artifact("finding", "unapproved-child-evidence")]),
    );
    let args = child_result_args(&mut case, &child, manifest, "unapproved-child-evidence");
    contract_error(run(&case, &args), "DELEGATION_MEMBER_MISMATCH");
    assert_eq!(ledger_snapshot(&case), before);
    let args = child_result_args(
        &mut case,
        &child,
        child_manifest(&child, "succeeded", vec![finding.clone()]),
        "record-child",
    );
    assert_rollback(&case, &args);
    assert!(read_child(&case, &children[1])["status"].is_text("pending"));
    let result = run(&case, &args).unwrap();
    assert!(result["branch_status"].is_text("succeeded"));
    assert!(result["promoted_branch_ids"].py_eq(&Value::list([children[1].clone()])));
    let snapshot = ledger_snapshot(&case);
    let replay = run(&case, &args).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&result["state_revision"]));
    assert_eq!(ledger_snapshot(&case), snapshot);
    let row = read_child(&case, &children[0]);
    let stored = compat::loads_value(&row["result_json"]).unwrap();
    let mut normalized = finding;
    normalized.set("location", "docs/cache.cs".into());
    assert!(stored["findings"].py_eq(&Value::list([normalized])));
    let envelope = compat::loads_value(&row["envelope_json"]).unwrap();
    assert!(envelope["artifact_ref"]["kind"].is_text("specialist_review"));
    assert!(envelope["artifact_ref"]["ref"]
        .text()
        .unwrap()
        .contains("-output#sha256="));
}

fn enable_child_format7(case: &mut HardeningCase, relevant_input: &str) {
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "check_coverage".into()),
        (
            "checks",
            Value::list([Value::object([
                ("check_id", "repo-check".into()),
                ("relevant_inputs", Value::list([relevant_input.into()])),
                ("complete", true.into()),
            ])]),
        ),
    ]);
    let path = case.inbox(&manifest);
    let enabled = case
        .run(&[
            "evidence",
            "enable",
            "--run-id",
            "RUN-1",
            "--contract-version",
            "2",
            "--coverage-manifest",
            path.to_str().unwrap(),
            "--op-id",
            "activate-child-evidence",
        ])
        .unwrap();
    assert!(enabled["state_schema_version"].py_eq(&7_i64.into()));
}

fn terminal_control_args(
    case: &mut HardeningCase,
    child: &Value,
    kind: &str,
    op: &str,
) -> Vec<String> {
    let branch = child["branch_id"].text().unwrap();
    let reason = if kind == "timeout" {
        "LEASE_TIMEOUT"
    } else {
        "SUPERVISOR_SKIP"
    };
    let mut manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", kind.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", child["branch_id"].clone()),
        ("reason_code", reason.into()),
        (
            "evidence",
            Value::list([case.repo_artifact(kind, &format!("{kind}-evidence-{}", case.counter()))]),
        ),
    ]);
    let mut args = [
        "record",
        kind,
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch,
        "--reason-code",
        reason,
        "--op-id",
        op,
    ]
    .map(str::to_owned)
    .to_vec();
    if kind == "timeout" {
        let attempt = child["attempt_id"].text().unwrap();
        let token = child["claim_token"].text().unwrap();
        manifest.set("attempt_id", child["attempt_id"].clone());
        manifest.set("claim_digest", ids::sha256_bytes(token.as_bytes()).into());
        args.extend(["--attempt-id", &attempt, "--claim-token", &token].map(str::to_owned));
    }
    let path = case.inbox(&manifest);
    args.extend([
        "--evidence-manifest".to_owned(),
        path.to_str().unwrap().to_owned(),
    ]);
    args
}

#[test]
fn format7_child_retry_fences_old_attempt_and_timeout_promotes_at_limit() {
    let (mut case, parent) = to_delivery_review("code_reviewer", 15);
    enable_child_format7(&mut case, "docs/engineering-graph.md");
    let request = assessed_pair(&mut case, &parent);
    let children = request["child_branch_ids"].as_list().unwrap();
    let first = case.claim();
    let failed_manifest = child_manifest(&first, "failed", vec![]);
    let args = child_result_args(
        &mut case,
        &first,
        failed_manifest.clone(),
        "first-child-failure",
    );
    let failed = run(&case, &args).unwrap();
    assert!(failed["branch_status"].is_text("failed"));
    assert!(failed["promoted_branch_ids"].py_eq(&Value::List(vec![])));
    let branch = children[0].text().unwrap();
    let retry = case
        .run(&[
            "record",
            "retry",
            "--run-id",
            "RUN-1",
            "--branch-id",
            &branch,
            "--reason-code",
            "RETRY",
            "--op-id",
            "child-retry",
        ])
        .unwrap();
    assert!(retry["code"].is_text("RETRY_RECORDED"));
    let replacement = case.claim();
    assert!(replacement["branch_id"].py_eq(&first["branch_id"]));
    assert!(!replacement["attempt_id"].py_eq(&first["attempt_id"]));
    assert!(replacement["inputs"].py_eq(&Value::List(vec![])));
    let stale = child_result_args(&mut case, &first, failed_manifest, "stale-child-result");
    let before = ledger_snapshot(&case);
    state_error(run(&case, &stale), "ATTEMPT_FENCE_MISMATCH");
    assert_eq!(ledger_snapshot(&case), before);
    let timeout = terminal_control_args(&mut case, &replacement, "timeout", "child-timeout");
    assert_rollback(&case, &timeout);
    let timed_out = run(&case, &timeout).unwrap();
    assert!(timed_out["branch_status"].is_text("timed_out"));
    assert!(timed_out["promoted_branch_ids"].py_eq(&Value::list([children[1].clone()])));
    let replay = run(&case, &timeout).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&timed_out["state_revision"]));
    case.fixture
        .with_session(|session, _| {
            let attempts = session.all(GraphRead::ReviewSettlementAttempts, &children[..1])?;
            assert_eq!(attempts.len(), 2);
            assert!(attempts[0]["outcome"].is_text("failed"));
            assert!(attempts[1]["outcome"].is_text("timed_out"));
            Ok(())
        })
        .unwrap();
}

#[test]
fn delegated_mandatory_child_can_be_skipped_before_claim() {
    let (mut case, parent) = to_delivery_review("code_reviewer", 15);
    let request = assessed_pair(&mut case, &parent);
    let children = request["child_branch_ids"].as_list().unwrap();
    let child = read_child(&case, &children[0]);
    assert!(child["mandatory"].truthy());
    let args = terminal_control_args(&mut case, &child, "skip", "skip-child");
    let result = run(&case, &args).unwrap();
    assert!(result["branch_status"].is_text("skipped"));
    assert!(result["promoted_branch_ids"].py_eq(&Value::list([children[1].clone()])));
    case.fixture
        .with_session(|session, _| {
            assert!(session
                .all(GraphRead::ReviewSettlementAttempts, &children[..1])?
                .is_empty());
            Ok(())
        })
        .unwrap();
}

#[test]
fn valid_resume_checks_degraded_acknowledgments_without_writing() {
    let case = HardeningCase::stock();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    case.fixture.initialize(&task, true).unwrap();
    let status = case.status().unwrap();
    let before = ledger_snapshot(&case);
    let base = ["resume", "--run-id", "RUN-1"];
    let permissions = status["permission_verification"].is_text("DEGRADED_PERMISSION_VERIFICATION");
    let durability = status["durability"].is_text("degraded");
    if permissions {
        state_error(case.run(&base), "DEGRADED_PERMISSION_ACK_REQUIRED");
    }
    if durability {
        let with_permissions = ["resume", "--run-id", "RUN-1", "--ack-degraded-permissions"];
        state_error(
            case.run(&with_permissions),
            "DEGRADED_DURABILITY_ACK_REQUIRED",
        );
    }
    let resumed = case
        .run(&[
            "resume",
            "--run-id",
            "RUN-1",
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
        ])
        .unwrap();
    assert!(resumed["code"].is_text("RESUMED"));
    assert!(resumed["acknowledgments"]["degraded_permissions"].truthy());
    assert!(resumed["acknowledgments"]["degraded_durability"].truthy());
    assert!(resumed["state_revision"].py_eq(&status["state_revision"]));
    assert_eq!(ledger_snapshot(&case), before);
}
