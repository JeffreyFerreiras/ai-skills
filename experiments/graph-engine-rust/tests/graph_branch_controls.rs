#[path = "support/evidence_schema.rs"]
mod evidence_schema;
#[path = "support/hardening_case.rs"]
mod hardening_case;

use evidence_schema::assert_repair_schema;
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
use hardening_case::{write_json, HardeningCase};
use std::path::{Path, PathBuf};

fn initialize(case: &HardeningCase, tags: &[&str]) -> Value {
    let task = case.fixture.graph.task("delivery", "full_delivery", tags);
    case.fixture.initialize(&task, true).unwrap()
}

fn control_manifest(case: &mut HardeningCase, kind: &str, reason: &str, branch: &Value) -> Value {
    let evidence = case.repo_artifact(kind, &format!("{kind}-evidence-{}", case.counter()));
    let mut manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", kind.into()),
        ("run_id", "RUN-1".into()),
        ("reason_code", reason.into()),
        ("evidence", Value::list([evidence])),
    ]);
    if !branch.is_null() {
        manifest.set("branch_id", branch["branch_id"].clone());
    }
    if kind == "timeout" && branch["claim_token"].truthy() {
        manifest.set("attempt_id", branch["attempt_id"].clone());
        manifest.set("claim_digest", claim_digest(branch));
    }
    manifest
}

fn claim_digest(branch: &Value) -> Value {
    ids::sha256_bytes(branch["claim_token"].text().unwrap().as_bytes()).into()
}

fn timeout_args(branch: &Value, path: &Path, reason: &str, operation: &str) -> Vec<String> {
    [
        "record",
        "timeout",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch["branch_id"].text().unwrap(),
        "--attempt-id",
        &branch["attempt_id"].text().unwrap(),
        "--claim-token",
        &branch["claim_token"].text().unwrap(),
        "--reason-code",
        reason,
        "--evidence-manifest",
        path.to_str().unwrap(),
        "--op-id",
        operation,
    ]
    .map(str::to_owned)
    .to_vec()
}

fn run(case: &HardeningCase, args: &[String]) -> EngineResult<Value> {
    case.run(&args.iter().map(String::as_str).collect::<Vec<_>>())
}

fn retry(
    case: &HardeningCase,
    branch: &Value,
    operation: &str,
    path: Option<&Path>,
) -> EngineResult<Value> {
    let id = branch["branch_id"].text().unwrap();
    let mut args = vec![
        "record",
        "retry",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &id,
        "--reason-code",
        "RETRY",
        "--op-id",
        operation,
    ];
    if let Some(path) = path {
        args.extend(["--repair-manifest", path.to_str().unwrap()]);
    }
    case.run(&args)
}

fn failure_manifest(case: &mut HardeningCase, branch: &Value, label: &str, code: &str) -> Value {
    let evidence = case.repo_artifact("failure", label);
    Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", branch["branch_id"].clone()),
        ("status", "failed".into()),
        (
            "output_kind",
            branch["output_contract"]["artifact_kind"].clone(),
        ),
        ("failure_code", code.into()),
        ("evidence", Value::list([evidence])),
    ])
}

fn fail(case: &mut HardeningCase, branch: &Value, label: &str, code: &str) {
    let manifest = failure_manifest(case, branch, label, code);
    case.record_as(branch, manifest, "result").unwrap();
}

fn state_error(result: EngineResult<Value>, code: &str) {
    let error = result.unwrap_err();
    assert!(
        matches!(&error, EngineError::State(error) if error.code == code),
        "{error:?}"
    );
}

fn contract_error(result: EngineResult<Value>, code: &str) {
    let error = result.unwrap_err();
    assert!(
        matches!(&error, EngineError::Contract(error) if error.code == code),
        "{error:?}"
    );
}

#[test]
fn test_timeout_is_frozen_into_collection_and_forces_block() {
    let mut case = HardeningCase::golden();
    initialize(&case, &[]);
    case.impact("full_delivery");
    let tech = case.claim();
    assert!(tech["node_key"].is_text("tech_lead"));
    case.success(&tech);
    case.advance("design_inputs");
    let architect = case.claim();
    let manifest = control_manifest(&mut case, "timeout", "DEADLINE", &architect);
    let path = case.inbox(&manifest);
    run(
        &case,
        &timeout_args(&architect, &path, "DEADLINE", "timeout-1"),
    )
    .unwrap();
    assert!(case.advance("design_collection")["code"].is_text("JOIN_ADVANCED"));
    case.fixture
        .with_session(|session, _| {
            let json: String = session
                .connection()
                .query_row(
                    "SELECT result_json FROM joins WHERE join_key='design_collection'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let frozen = compat::loads(json.as_bytes())?;
            assert!(frozen.as_list().unwrap()[0]["status"].is_text("timed_out"));
            Ok(())
        })
        .unwrap();
    case.consolidate("design", "BLOCK", 0, vec![]);
    assert!(case.advance("design_consolidation")["outcome"].is_text("BLOCK"));
    assert!(case.status().unwrap()["status"].is_text("blocked"));
}

#[test]
fn test_retry_boundary_blocks_mandatory_branch() {
    let mut case = HardeningCase::golden();
    initialize(&case, &[]);
    case.impact("full_delivery");
    let branch = case.claim();
    fail(&mut case, &branch, "failure-one", "TOOL_FAILURE");
    retry(&case, &branch, "retry-one", None).unwrap();
    let branch = case.claim();
    fail(&mut case, &branch, "failure-two", "TOOL_FAILURE");
    assert!(retry(&case, &branch, "retry-two", None).unwrap()["code"].is_text("GRAPH_BLOCKED"));
    assert!(case.status().unwrap()["status"].is_text("blocked"));
}

#[test]
fn test_failed_result_wrapper_cannot_satisfy_retry_output_contract() {
    let mut case = HardeningCase::golden();
    initialize(&case, &[]);
    case.impact("full_delivery");
    let engineer = case.claim();
    fail(
        &mut case,
        &engineer,
        "failed-design-wrapper",
        "TOOL_FAILURE",
    );
    let wrapper = case
        .fixture
        .with_session(|session, _| {
            let node = session
                .first(
                    GraphRead::Node,
                    std::slice::from_ref(&engineer["branch_id"]),
                )?
                .unwrap();
            let packet = compat::loads_value(&node["envelope_json"])?;
            let wrapper = &packet["artifact_ref"];
            let artifact = session
                .first(GraphRead::Artifact, std::slice::from_ref(&wrapper["ref"]))?
                .unwrap();
            assert!(wrapper["kind"].is_text("failure"));
            assert!(artifact["kind"].is_text("failure"));
            Ok(wrapper.clone())
        })
        .unwrap();
    retry(&case, &engineer, "wrapper-retry", None).unwrap();
    let retried = case.claim();
    let adversarial = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", retried["branch_id"].clone()),
        ("status", "succeeded".into()),
        ("output_kind", "technical_design".into()),
        (
            "artifact_ref",
            Value::object([
                ("kind", "technical_design".into()),
                ("ref", wrapper["ref"].clone()),
                ("sha256", wrapper["sha256"].clone()),
            ]),
        ),
        ("evidence", Value::List(vec![])),
        ("findings", Value::List(vec![])),
    ]);
    contract_error(
        case.record_as(&retried, adversarial, "result"),
        "LEDGER_ARTIFACT_NOT_FOUND",
    );
}

fn record_result_with_attempt(
    case: &HardeningCase,
    branch: &Value,
    attempt: &Value,
    path: &Path,
    operation: &str,
) -> EngineResult<Value> {
    case.run(&[
        "record",
        "branch-result",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch["branch_id"].text().unwrap(),
        "--attempt-id",
        &attempt["attempt_id"].text().unwrap(),
        "--claim-token",
        &attempt["claim_token"].text().unwrap(),
        "--result-manifest",
        path.to_str().unwrap(),
        "--op-id",
        operation,
    ])
}

#[test]
fn test_mapper_failure_is_evidenced_retryable_and_replayable() {
    let mut case = HardeningCase::golden();
    let initialized = initialize(&case, &[]);
    let mapper = case.claim();
    let mut manifest = failure_manifest(
        &mut case,
        &mapper,
        "mapper-failure",
        "INSUFFICIENT_EVIDENCE",
    );
    let path = case
        .fixture
        .graph
        .store
        .inbox_root("albanian-live-translate", "RUN-1")
        .join("mapper-failure.json");
    write_json(&path, &manifest);
    case.fixture
        .graph
        .store
        .platform
        .set_mode(&path, 0o600)
        .unwrap();
    manifest.set("attempt_id", mapper["attempt_id"].clone());
    manifest.set("claim_digest", claim_digest(&mapper));
    write_json(&path, &manifest);
    let first =
        record_result_with_attempt(&case, &mapper, &mapper, &path, "mapper-failed").unwrap();
    let replay =
        record_result_with_attempt(&case, &mapper, &mapper, &path, "mapper-failed").unwrap();
    assert!(first["branch_status"].is_text("failed"));
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&first["state_revision"]));
    assert!(retry(&case, &mapper, "mapper-retry", None).unwrap()["branch_status"].is_text("ready"));
    let replacement = case.claim();
    assert!(replacement["branch_id"].py_eq(&mapper["branch_id"]));
    assert!(!replacement["attempt_id"].py_eq(&mapper["attempt_id"]));
    assert_eq!(
        replacement["retry_count"].as_i64().unwrap(),
        mapper["retry_count"].as_i64().unwrap() + 1
    );
    let status = case.status().unwrap();
    let plan = &status["execution_plan"];
    assert!(plan["status"].is_text("approved"));
    assert!(plan["plan_digest"].py_eq(&initialized["execution_plan_digest"]));
}

fn timeout_manifest_for_attempt(
    case: &mut HardeningCase,
    branch: &Value,
    digest: Value,
) -> PathBuf {
    let partial = Value::object([("branch_id", branch["branch_id"].clone())]);
    let mut manifest = control_manifest(case, "timeout", "WORKER_TIMEOUT", &partial);
    let path = case.inbox(&manifest);
    manifest.set("attempt_id", branch["attempt_id"].clone());
    manifest.set("claim_digest", digest);
    write_json(&path, &manifest);
    path
}

#[test]
fn test_late_result_from_previous_attempt_is_fenced() {
    let mut case = HardeningCase::new();
    initialize(&case, &[]);
    let first = case.claim();
    let path = timeout_manifest_for_attempt(&mut case, &first, claim_digest(&first));
    run(
        &case,
        &timeout_args(&first, &path, "WORKER_TIMEOUT", "timeout-1"),
    )
    .unwrap();
    retry(&case, &first, "retry-1", None).unwrap();
    let second = case.claim();
    let stale = Value::object([
        ("schema_version", 1_i64.into()),
        ("task_id", "TASK-1".into()),
        ("route_label", "full_delivery".into()),
        ("impact_tags", Value::List(vec![])),
        ("evidence_refs", Value::List(vec![])),
        ("attempt_id", first["attempt_id"].clone()),
        ("claim_digest", claim_digest(&first)),
    ]);
    let path = case.inbox(&stale);
    state_error(
        record_result_with_attempt(&case, &second, &first, &path, "stale-result-1"),
        "ATTEMPT_FENCE_MISMATCH",
    );
}

#[test]
fn test_format7_retry_packet_retains_failure_and_rejects_scope_expansion() {
    assert_format7_retry_scope(HardeningCase::new());
}

#[test]
fn test_scope_change_during_retry_stays_denied() {
    assert_format7_retry_scope(HardeningCase::stock());
}

fn assert_format7_retry_scope(mut case: HardeningCase) {
    initialize(&case, &[]);
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
    let first = case.claim_raw().unwrap();
    let path = timeout_manifest_for_attempt(&mut case, &first, first["claim_digest"].clone());
    run(
        &case,
        &timeout_args(&first, &path, "WORKER_TIMEOUT", "timeout"),
    )
    .unwrap();
    let judgment = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "repair_judgment".into()),
        ("cause", "infrastructure".into()),
        ("failed_criterion_ids", Value::list(["AC-001".into()])),
        ("authorized_scope", Value::List(vec![])),
    ]);
    let mut wrong = judgment.clone();
    wrong.set("failed_criterion_ids", Value::list(["AC-999".into()]));
    let path = case.inbox(&wrong);
    contract_error(
        retry(&case, &first, "bad-criterion", Some(&path)),
        "UNKNOWN_CRITERION",
    );
    let mut wide = judgment.clone();
    wide.set(
        "authorized_scope",
        Value::list([Value::object([
            ("effect", "filesystem_write".into()),
            ("action", "edit".into()),
            ("target_ref", "repo:src/".into()),
        ])]),
    );
    let path = case.inbox(&wide);
    contract_error(
        retry(&case, &first, "bad-scope", Some(&path)),
        "AUTHORITY_EXCEEDED",
    );
    let path = case.inbox(&judgment);
    retry(&case, &first, "retry", Some(&path)).unwrap();
    let second = case.claim_raw().unwrap();
    let packet = case
        .fixture
        .with_session(|session, _| {
            let mut packets = Vec::new();
            for input in second["inputs"]
                .as_list()
                .unwrap()
                .iter()
                .filter(|input| input["kind"].is_text("failure"))
            {
                let artifact = session
                    .first(
                        GraphRead::ArtifactContent,
                        std::slice::from_ref(&input["ref"]),
                    )?
                    .unwrap();
                packets.push(compat::loads_value(&artifact["content_json"])?);
            }
            Ok(packets.pop().unwrap())
        })
        .unwrap();
    assert!(packet["attempt_id"].py_eq(&first["attempt_id"]));
    assert!(packet["failed_criterion_ids"].py_eq(&Value::list(["AC-001".into()])));
    assert!(packet["return_gates"].py_eq(&Value::list(["supervisor".into()])));
    assert!(!first["attempt_id"].py_eq(&second["attempt_id"]));
    assert_repair_schema(&packet);
}

#[test]
fn test_retryable_research_failure_requires_retry_before_collection_seal() {
    let mut case = HardeningCase::golden();
    initialize(&case, &[]);
    case.impact("full_delivery");
    let status = case.status().unwrap();
    assert!(status["next_action"]["kind"].is_text("record_fanout_assessment"));
    assert!(status["next_action"]["stage"].is_text("research"));
    let fanout = status["fanouts"]
        .as_list()
        .unwrap()
        .iter()
        .find(|fanout| fanout["stage"].is_text("research"))
        .unwrap();
    case.assess(&fanout["fanout_id"].text().unwrap());
    let first = case.claim_raw().unwrap();
    let second = case.claim_raw().unwrap();
    fail(
        &mut case,
        &first,
        "research-retryable-failure",
        "RESEARCH_FAILURE",
    );
    case.success(&second);
    let join = case.open_join("research_collection")["join_id"]
        .text()
        .unwrap();
    let validated = case
        .run(&["join", "validate", "--run-id", "RUN-1", "--join-id", &join])
        .unwrap();
    assert!(validated["code"].is_text("RETRY_REQUIRED"));
    state_error(
        case.run(&[
            "join",
            "advance",
            "--run-id",
            "RUN-1",
            "--join-id",
            &join,
            "--op-id",
            "early",
        ]),
        "RETRY_REQUIRED",
    );
    retry(&case, &first, "research-retry-one", None).unwrap();
    let retried = case.claim_raw().unwrap();
    assert!(retried["branch_id"].py_eq(&first["branch_id"]));
    case.success(&retried);
    assert!(case.advance("research_collection")["outcome"].is_text("RESEARCH_SEALED"));
    let status = case.status().unwrap();
    let tech: Vec<_> = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|node| node["node_key"].is_text("tech_lead"))
        .collect();
    assert_eq!(tech.len(), 1);
    assert_eq!(tech[0]["generation"].as_i64(), Some(0));
    assert!(tech[0]["status"].is_text("ready"));
}

struct RejectControlCommit;

impl FaultHook for RejectControlCommit {
    fn observe(&self, stage: &str) -> EngineResult<()> {
        if stage == "before_commit" {
            return Err(EngineError::Injected("control-before-commit".into()));
        }
        Ok(())
    }
}

#[test]
fn timeout_rolls_back_attempt_and_artifacts_before_successful_replay() {
    let mut case = HardeningCase::golden();
    initialize(&case, &[]);
    let mapper = case.claim();
    let path = timeout_manifest_for_attempt(&mut case, &mapper, claim_digest(&mapper));
    let arguments = timeout_args(&mapper, &path, "WORKER_TIMEOUT", "timeout");
    let before = case.status().unwrap();
    let artifacts = |case: &HardeningCase| {
        case.fixture
            .with_session(|session, _| {
                let count: i64 = session
                    .connection()
                    .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
                    .unwrap();
                Ok(count)
            })
            .unwrap()
    };
    let count = artifacts(&case);
    let fixture = &case.fixture;
    let original = &fixture.graph.store;
    let store = StateStore::at_root(
        original.state_root.clone(),
        original.platform,
        original.database,
        &RejectControlCommit,
    )
    .unwrap();
    let mut argv = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
    ];
    argv.extend(arguments.iter().map(|arg| PyString::from(arg.as_str())));
    let error = fixture.execute_with_store(&argv, &store).unwrap_err();
    assert!(matches!(error, EngineError::Injected(reason) if reason == "control-before-commit"));
    let after = case.status().unwrap();
    for key in ["state_revision", "branches", "joins", "budgets"] {
        assert!(before[key].py_eq(&after[key]), "{key}");
    }
    assert_eq!(artifacts(&case), count);
    let original = run(&case, &arguments).unwrap();
    let replay = run(&case, &arguments).unwrap();
    assert!(original["code"].is_text("TIMEOUT_RECORDED"));
    assert!(original["branch_status"].is_text("timed_out"));
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&original["state_revision"]));
}

fn design_fanout(case: &mut HardeningCase, tags: &[&str]) -> Value {
    initialize(case, tags);
    let mapper = case.claim();
    case.record_mapper(&mapper, tags);
    let tech = case.claim();
    case.success(&tech);
    case.advance("design_inputs");
    case.status().unwrap()["fanouts"]
        .as_list()
        .unwrap()
        .iter()
        .find(|fanout| fanout["stage"].is_text("design"))
        .unwrap()
        .clone()
}

fn assess_resources(case: &mut HardeningCase, fanout: &Value) {
    let evidence = case.repo_artifact(
        "finding",
        &format!("fanout-resource-proof-{}", case.counter()),
    );
    let ids = fanout["member_branch_ids"].as_list().unwrap();
    let members = Value::list(ids.iter().rev().enumerate().map(|(index, id)| {
        Value::object([
            ("branch_id", id.clone()),
            (
                "resources",
                Value::object([
                    (
                        "writable_paths",
                        Value::list([
                            Value::object([
                                ("path", format!("src/member-{index}/file.py").into()),
                                ("scope", "exact".into()),
                            ]),
                            Value::object([
                                ("path", format!("docs/member-{index}").into()),
                                ("scope", "subtree".into()),
                            ]),
                        ]),
                    ),
                    (
                        "mutable_state_refs",
                        Value::list([format!("state:member-{index}").into()]),
                    ),
                    (
                        "exclusive_device_refs",
                        Value::list([format!("device:member-{index}").into()]),
                    ),
                    (
                        "services",
                        Value::list([Value::object([
                            ("ref", "service:review".into()),
                            ("units", 1_i64.into()),
                            ("capacity", ids.len().into()),
                        ])]),
                    ),
                ]),
            ),
        ])
    }));
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", "RUN-1".into()),
        ("fanout_id", fanout["fanout_id"].clone()),
        ("members", members),
        ("dependencies", Value::List(vec![])),
        ("evidence", Value::list([evidence])),
    ]);
    let path = case.inbox(&manifest);
    case.run(&[
        "record",
        "fanout-assessment",
        "--run-id",
        "RUN-1",
        "--fanout-id",
        &fanout["fanout_id"].text().unwrap(),
        "--assessment-manifest",
        path.to_str().unwrap(),
        "--authority-ref",
        "authority:test",
        "--op-id",
        "skip-assessment",
    ])
    .unwrap();
}

fn skip(case: &HardeningCase, branch: &Value, path: &Path, operation: &str) -> EngineResult<Value> {
    case.run(&[
        "record",
        "skip",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch["branch_id"].text().unwrap(),
        "--reason-code",
        "NOT_APPLICABLE",
        "--evidence-manifest",
        path.to_str().unwrap(),
        "--op-id",
        operation,
    ])
}

#[test]
fn test_mandatory_ready_branch_cannot_be_skipped() {
    let mut case = HardeningCase::new();
    let fanout = design_fanout(&mut case, &["security_privacy"]);
    assess_resources(&mut case, &fanout);
    let id = &fanout["member_branch_ids"].as_list().unwrap()[0];
    let status = case.status().unwrap();
    let branch = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .find(|branch| branch["branch_id"].py_eq(id))
        .unwrap();
    assert!(branch["status"].is_text("ready"));
    let manifest = control_manifest(&mut case, "skip", "NOT_APPLICABLE", branch);
    let path = case.inbox(&manifest);
    state_error(
        skip(&case, branch, &path, "mandatory-skip"),
        "INVALID_BRANCH_TRANSITION",
    );
    assert!(case.status().unwrap()["state_revision"].py_eq(&status["state_revision"]));
}
