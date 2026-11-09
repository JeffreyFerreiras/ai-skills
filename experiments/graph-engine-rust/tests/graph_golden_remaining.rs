#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[allow(dead_code)]
#[path = "support/recovery_case.rs"]
mod recovery_case;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_arguments::shared_parser,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        join_validation::{canonical_collection_members, join_members},
        persistent_store::StateStore,
    },
    compat::{
        self,
        argparse::{ArgumentValue, ParserOutcome},
        PyString, Value,
    },
    domain::{contracts::ContractError, evidence::canonical_ledger_artifact, ids},
};
use hardening_case::{write_json, HardeningCase};
use recovery_case::{block, block_evidence, state_error};
use std::{collections::BTreeSet, process::Command};

fn initialize(case: &HardeningCase, tags: &[&str]) -> Value {
    let task = case.fixture.graph.task("delivery", "full_delivery", tags);
    case.fixture.initialize(&task, true).unwrap()
}

fn contract_error(result: EngineResult<Value>, expected: &str) -> ContractError {
    match result.unwrap_err() {
        EngineError::Contract(error) if error.code == expected => error,
        error => panic!("expected {expected}, got {error:?}"),
    }
}

fn finding(id: &str, disposition: &str) -> Value {
    Value::object([
        ("finding_id", id.into()),
        ("disposition", disposition.into()),
    ])
}

fn fail(case: &mut HardeningCase, branch: &Value, label: &str, code: &str) {
    let evidence = case.repo_artifact("failure", label);
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
            ("failure_code", code.into()),
            ("evidence", Value::list([evidence])),
        ]),
        "result",
    )
    .unwrap();
}

fn retry(case: &HardeningCase, branch: &Value, op: &str) -> EngineResult<Value> {
    case.run(&[
        "record",
        "retry",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch["branch_id"].text().unwrap(),
        "--reason-code",
        "RETRY",
        "--op-id",
        op,
    ])
}

fn start_design_reviews(case: &mut HardeningCase, tags: &[&str]) {
    initialize(case, tags);
    let mapper = case.claim();
    let evidence = case.repo_artifact("finding", &format!("impact-evidence-{}", case.counter()));
    case.record_as(
        &mapper,
        Value::object([
            ("schema_version", 1_i64.into()),
            ("task_id", "TASK-1".into()),
            ("route_label", "full_delivery".into()),
            (
                "impact_tags",
                Value::list(tags.iter().copied().map(Value::from)),
            ),
            ("evidence_refs", Value::list([evidence["ref"].clone()])),
        ]),
        "result",
    )
    .unwrap();
    let tech = case.claim();
    assert!(tech["node_key"].is_text("tech_lead"));
    case.success(&tech);
    case.advance("design_inputs");
}

fn start_research() -> HardeningCase {
    let mut case = HardeningCase::golden();
    initialize(&case, &[]);
    case.impact("full_delivery");
    let status = case.status().unwrap();
    let action = &status["next_action"];
    assert!(action["kind"].is_text("record_fanout_assessment"));
    assert!(action["stage"].is_text("research"));
    let fanout = status["fanouts"]
        .as_list()
        .unwrap()
        .iter()
        .find(|item| item["stage"].is_text("research"))
        .unwrap();
    case.assess(&fanout["fanout_id"].text().unwrap());
    case
}

#[test]
fn test_help_exposes_portable_state_root_and_default_sol_muse_catalog() {
    let case = HardeningCase::golden();
    let parser = shared_parser();
    let help = |args: &[&str]| {
        let parsed = parser.parse(
            &args.iter().copied().map(PyString::from).collect::<Vec<_>>(),
            80,
        );
        let ParserOutcome::Exit { code, stdout, .. } = parsed else {
            panic!("help must exit")
        };
        assert_eq!(code, 0);
        stdout.utf8().unwrap()
    };
    assert!(help(&["--help"]).contains("--state-root"));
    let init = help(&["init", "--help"])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for expected in [
        "codex (default Sol/Muse recommendations)",
        "codex-astra",
        "claude",
    ] {
        assert!(init.contains(expected), "{init}");
    }
    let repo = case.fixture.graph.repository.to_str().unwrap();
    let args = [
        "--repo",
        repo,
        "init",
        "--run-id",
        "R",
        "--task-brief",
        "T",
        "--op-id",
        "O",
    ]
    .map(PyString::from);
    let ParserOutcome::Arguments(namespace) = parser.parse(&args, 80) else {
        panic!("init parsing failed")
    };
    assert_eq!(
        namespace.get("host"),
        Some(&ArgumentValue::String("codex".into()))
    );
}

#[test]
fn test_normative_golden_trace_fixture_is_complete() {
    let _case = HardeningCase::golden();
    let fixture = compat::loads(include_bytes!(
        "../../../skills/software-engineering-graph/tests/fixtures/result-manifests/golden-traces.json"
    )).unwrap();
    let names = fixture["traces"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, _)| key.utf8().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names,
        BTreeSet::from(
            [
                "BLOCK",
                "REVISE",
                "REPAIR",
                "REDESIGN",
                "FAST_PATH_REDESIGN"
            ]
            .map(str::to_owned)
        )
    );
}

#[test]
fn test_v2_size_below_safety_floor_is_stable_and_leaves_no_run_state() {
    let case = HardeningCase::golden();
    let mut task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    task.set("schema_version", 2_i64.into());
    task.set("risk_level", "high".into());
    task.set(
        "model_sizing",
        Value::object([
            ("scope_extent", "bounded".into()),
            ("uncertainty", "low".into()),
        ]),
    );
    let graph = &case.fixture.graph;
    let path = graph.repository.join("docs/task.json");
    write_json(&path, &task);
    let output = Command::new(env!("CARGO_BIN_EXE_graphctl"))
        .args([
            "--repo",
            graph.repository.to_str().unwrap(),
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
            "init",
            "--run-id",
            "RUN-SIZE-FLOOR",
            "--task-brief",
            path.to_str().unwrap(),
            "--size",
            "medium",
            "--op-id",
            "size-floor",
        ])
        .env("SOFTWARE_ENGINEERING_GRAPH_HOME", &graph.runtime_root)
        .current_dir(&graph.repository)
        .output()
        .unwrap();
    let result = compat::loads(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(4));
    assert!(result["ok"].py_eq(&false.into()));
    assert!(result["code"].is_text("EXECUTION_SIZE_BELOW_SAFETY_FLOOR"));
    assert!(result["field"].is_text("size"));
    assert!(!graph
        .store
        .run_root("albanian-live-translate", "RUN-SIZE-FLOOR")
        .exists());
    assert!(!graph
        .store
        .inbox_root("albanian-live-translate", "RUN-SIZE-FLOOR")
        .exists());
}

#[test]
fn test_block_is_durable_replayable_and_only_abort_can_follow() {
    let mut case = HardeningCase::golden();
    let initialized = initialize(&case, &[]);
    let args = [
        "next",
        "--run-id",
        "RUN-1",
        "--claim",
        "--op-id",
        "claim-before-block",
    ];
    let claimed = case.run(&args).unwrap();
    let manifest = block_evidence(&mut case, "STOP");
    let path = case.inbox(&manifest);
    assert!(block(&case, "STOP", &path, "block-1")["status"].is_text("blocked"));
    let replay = case.run(&args).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&claimed["state_revision"]));
    state_error(
        retry(&case, &initialized["branch"], "late-retry"),
        "GRAPH_BLOCKED",
    );
    let abort = |reason: &str, op: &str| {
        case.run(&[
            "abort",
            "--run-id",
            "RUN-1",
            "--reason-code",
            reason,
            "--authority-ref",
            "authority:test",
            "--op-id",
            op,
        ])
    };
    assert!(abort("STOP", "abort-1").unwrap()["status"].is_text("aborted"));
    assert!(abort("STOP", "abort-1").unwrap()["code"].is_text("REPLAYED"));
    state_error(abort("OTHER", "abort-2"), "TERMINAL_RUN");
}

#[test]
fn test_budget_consumption_is_atomic_replayable_and_unique_per_source() {
    let mut case = HardeningCase::golden();
    initialize(&case, &[]);
    case.impact("full_delivery");
    let status = case.status().unwrap();
    let mapper = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .find(|branch| branch["node_key"].is_text("impact_mapper"))
        .unwrap()["branch_id"]
        .text()
        .unwrap();
    let record = |op: &str| {
        case.run(&[
            "record",
            "budget-use",
            "--run-id",
            "RUN-1",
            "--budget-id",
            "file_reads",
            "--amount",
            "1",
            "--source-branch-id",
            &mapper,
            "--op-id",
            op,
        ])
    };
    let first = record("budget-1").unwrap();
    let replay = record("budget-1").unwrap();
    assert!(first["code"].is_text("BUDGET_USE_RECORDED"));
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&first["state_revision"]));
    state_error(record("budget-2"), "BUDGET_CONSUMPTION_CONFLICT");
}

#[test]
fn test_recovery_does_not_advertise_retry_for_blocked_run() {
    assert_blocked_recovery(HardeningCase::golden());
}

#[test]
fn test_blocked_failure_keeps_no_retry_contract() {
    assert_blocked_recovery(HardeningCase::stock());
}

fn assert_blocked_recovery(mut case: HardeningCase) {
    initialize(&case, &[]);
    case.impact("full_delivery");
    let branch = case.claim();
    fail(
        &mut case,
        &branch,
        "blocked-recovery-failure",
        "TOOL_FAILURE",
    );
    let manifest = block_evidence(&mut case, "STOP");
    let path = case.inbox(&manifest);
    block(&case, "STOP", &path, "block-after-failure");
    let id = branch["branch_id"].text().unwrap();
    let recovery = case
        .run(&["recovery", "show", "--run-id", "RUN-1", "--branch-id", &id])
        .unwrap();
    let packet = &recovery["packets"].as_list().unwrap()[0];
    assert!(packet["next_permitted_action"].is_text("supervisor_assessment"));
    assert!(case.status().unwrap()["status"].is_text("blocked"));
}

#[test]
fn test_out_of_scope_block_is_rejected_instead_of_silently_blocking() {
    let mut case = HardeningCase::golden();
    start_design_reviews(&mut case, &[]);
    let architect = case.claim();
    let mut report = finding("REL-002", "block");
    report.set("scope", "out_of_scope".into());
    contract_error(
        case.success_with(&architect, Some("BLOCK"), vec![report]),
        "OUT_OF_SCOPE_ACTIONABLE",
    );
}

#[test]
fn test_partial_collection_reports_exact_inflight_groups() {
    let mut case = HardeningCase::golden();
    start_design_reviews(&mut case, &["security_privacy"]);
    let first = case.claim();
    case.success_with(&first, Some("APPROVE"), vec![]).unwrap();
    let join = case.open_join("design_collection")["join_id"]
        .text()
        .unwrap();
    let args = ["join", "validate", "--run-id", "RUN-1", "--join-id", &join];
    let pending = case.run(&args).unwrap();
    assert!(pending["code"].is_text("NOT_READY"));
    assert_eq!(pending["groups"]["ready"].as_list().unwrap().len(), 1);
    let second = case.claim();
    case.success_with(&second, Some("APPROVE"), vec![]).unwrap();
    assert!(case.run(&args).unwrap()["code"].is_text("READY"));
}

#[test]
fn test_research_result_decisions_and_findings_are_atomic_and_forbidden() {
    let mut case = start_research();
    let branch = case.claim_raw().unwrap();
    let output = case.repo_artifact("evidence_manifest", "research-output");
    let evidence = case.repo_artifact("finding", "research-evidence");
    let id = branch["branch_id"].text().unwrap();
    let base = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", branch["branch_id"].clone()),
        ("status", "succeeded".into()),
        ("output_kind", "evidence_manifest".into()),
        ("artifact_ref", output),
        ("evidence", Value::list([evidence])),
        ("findings", Value::List(vec![])),
    ]);
    let observe = |case: &HardeningCase| {
        case.fixture.with_session(|session, _| {
        let connection = session.connection();
        let row = connection.query_row(
            "SELECT nodes.status,runs.state_revision,nodes.result_json FROM nodes JOIN runs ON runs.run_id=nodes.run_id WHERE nodes.branch_id=?",
            [&id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<String>>(2)?)),
        ).unwrap();
        let count: i64 = connection.query_row("SELECT COUNT(*) FROM artifacts WHERE run_id=?", ["RUN-1"], |row| row.get(0)).unwrap();
        Ok((row, count))
    }).unwrap()
    };
    for (field, value, code) in [
        ("decision", "APPROVE".into(), "DECISION_FORBIDDEN"),
        (
            "findings",
            Value::list([finding("ARCH-001", "approve")]),
            "FINDINGS_FORBIDDEN",
        ),
    ] {
        let mut manifest = base.clone();
        manifest.set(field, value);
        let ((_, revision, _), artifacts) = observe(&case);
        contract_error(case.record_as(&branch, manifest, "result"), code);
        assert_eq!(
            observe(&case),
            (("running".to_owned(), revision, None), artifacts)
        );
    }
    case.success(&branch);
}

#[test]
fn test_persisted_research_evidence_is_reverified_before_status() {
    let mut case = start_research();
    let branch = case.claim_raw().unwrap();
    case.success(&branch);
    case.fixture
        .with_session(|session, _| {
            let row = session
                .first(GraphRead::Node, std::slice::from_ref(&branch["branch_id"]))?
                .unwrap();
            let result = compat::loads_value(&row["result_json"])?;
            let reference = result["evidence"].as_list().unwrap()[0]["ref"]
                .text()
                .unwrap();
            session
                .connection()
                .execute("DELETE FROM artifacts WHERE ref=?", [&reference])
                .unwrap();
            Ok(())
        })
        .unwrap();
    state_error(case.status(), "TERMINAL_RESULT_INVALID");
}

#[test]
fn test_exhausted_mandatory_research_blocks_without_creating_tech_lead() {
    let mut case = start_research();
    let first = case.claim_raw().unwrap();
    let second = case.claim_raw().unwrap();
    fail(
        &mut case,
        &first,
        "research-failure-one",
        "RESEARCH_FAILURE",
    );
    fail(
        &mut case,
        &second,
        "research-failure-two",
        "RESEARCH_FAILURE",
    );
    for (branch, ordinal) in [(&first, "one"), (&second, "two")] {
        retry(
            &case,
            branch,
            &format!("research-retry-exhausted-{ordinal}"),
        )
        .unwrap();
        let replacement = case.claim_raw().unwrap();
        fail(
            &mut case,
            &replacement,
            &format!("research-failure-{ordinal}-exhausted"),
            "RESEARCH_FAILURE",
        );
    }
    let blocked = case.advance("research_collection");
    assert!(blocked["code"].is_text("GRAPH_BLOCKED"));
    assert!(blocked["reason"].is_text("MANDATORY_RESEARCH_FAILED"));
    let status = case.status().unwrap();
    assert!(status["status"].is_text("blocked"));
    assert!(!status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .any(|branch| branch["node_key"].is_text("tech_lead")));
}

fn timeout(case: &mut HardeningCase, branch: &Value) {
    let reason = "DEADLINE";
    let evidence = case.repo_artifact("timeout", &format!("timeout-evidence-{}", case.counter()));
    let token = branch["claim_token"].text().unwrap();
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "timeout".into()),
        ("run_id", "RUN-1".into()),
        ("reason_code", reason.into()),
        ("evidence", Value::list([evidence])),
        ("branch_id", branch["branch_id"].clone()),
        ("attempt_id", branch["attempt_id"].clone()),
        ("claim_digest", ids::sha256_bytes(token.as_bytes()).into()),
    ]);
    let path = case.inbox(&manifest);
    case.run(&[
        "record",
        "timeout",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch["branch_id"].text().unwrap(),
        "--attempt-id",
        &branch["attempt_id"].text().unwrap(),
        "--claim-token",
        &token,
        "--reason-code",
        reason,
        "--evidence-manifest",
        path.to_str().unwrap(),
        "--op-id",
        "restart-timeout",
    ])
    .unwrap();
}

#[test]
fn test_consolidation_reconstructs_all_results_after_restart_from_envelope_only() {
    let mut case = HardeningCase::golden();
    start_design_reviews(
        &mut case,
        &["audio_realtime_translation", "security_privacy"],
    );
    let architect = case.claim();
    let findings = Value::list([finding("ARCH-808", "revise")]);
    case.success_with(
        &architect,
        Some("REVISE"),
        findings.as_list().unwrap().to_vec(),
    )
    .unwrap();
    let failed = case.claim();
    fail(&mut case, &failed, "specialist-failure", "TOOL_FAILURE");
    let timed_out = case.claim();
    timeout(&mut case, &timed_out);
    case.advance("design_collection");
    let store = &case.fixture.graph.store;
    case.fixture.graph.store = StateStore::at_root(
        store.state_root.clone(),
        store.platform,
        store.database,
        store.fault,
    )
    .unwrap();
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
    let consolidation = case.claim();
    let collection = consolidation["inputs"]
        .as_list()
        .unwrap()
        .iter()
        .find(|item| item["kind"].is_text("collection"))
        .unwrap();
    let members = collection["content"]["members"].as_list().unwrap();
    for (key, expected) in [
        ("status", ["succeeded", "failed", "timed_out"]),
        ("result_kind", ["branch_result", "failure", "timeout"]),
    ] {
        let actual = members
            .iter()
            .map(|member| member[key].text().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, BTreeSet::from(expected.map(str::to_owned)));
    }
    assert!(members
        .iter()
        .all(|member| ["result", "failure_code", "reason_code"]
            .iter()
            .all(|key| member.get(key).is_some())));
    let result = &members
        .iter()
        .find(|member| member["branch_id"].py_eq(&architect["branch_id"]))
        .unwrap()["result"];
    assert!(result["decision"].is_text("REVISE"));
    assert!(result["findings"].py_eq(&findings));
    case.record_as(
        &consolidation,
        Value::object([
            ("schema_version", 1_i64.into()),
            ("kind", "design_consolidation".into()),
            ("run_id", "RUN-1".into()),
            ("join_id", collection["content"]["join_id"].clone()),
            ("generation", 0_i64.into()),
            (
                "source_branch_ids",
                Value::list(members.iter().map(|member| member["branch_id"].clone())),
            ),
            ("finding_dispositions", findings),
            ("outcome", "BLOCK".into()),
        ]),
        "result",
    )
    .unwrap();
}

fn collection_boundary(maximum: Option<i64>) -> (HardeningCase, Value, i64, Vec<i64>, Vec<String>) {
    let mut case = HardeningCase::golden();
    if let Some(maximum) = maximum {
        case.edit_policy(|policy| {
            let mut kinds = policy["artifact_kinds"].clone();
            let mut collection = kinds["collection"].clone();
            collection.set("max_bytes", maximum.into());
            kinds.set("collection", collection);
            policy.set("artifact_kinds", kinds);
        });
    }
    start_design_reviews(&mut case, &["security_privacy"]);
    let mut findings = Vec::new();
    for index in 0..2 {
        let reviewer = case.claim();
        let id = format!(
            "REV-{:03}{}",
            index + 100,
            (index + 1).to_string().repeat(512)
        );
        case.success_with(&reviewer, Some("APPROVE"), vec![finding(&id, "approve")])
            .unwrap();
        findings.push(id);
    }
    let join = case.open_join("design_collection")["join_id"].clone();
    let (size, wrappers) = case
        .fixture
        .with_session(|session, _| {
            let members = join_members(session, &join)?;
            let frozen = canonical_collection_members(session, &members)?;
            let mut wrappers = Vec::new();
            for member in members {
                let reference = format!(
                    "ledger:{}#sha256={}",
                    member["branch_id"].text().unwrap(),
                    member["result_digest"].text().unwrap()
                );
                let artifact = session
                    .first(GraphRead::Artifact, &[reference.into()])?
                    .unwrap();
                wrappers.push(artifact["size_bytes"].as_i64().unwrap());
            }
            let manifest = Value::object([
                ("schema_version", 1_i64.into()),
                ("kind", "collection".into()),
                ("join_id", join.clone()),
                ("members", frozen),
            ]);
            let artifact = canonical_ledger_artifact(
                join.as_string().unwrap(),
                &"collection".into(),
                &manifest,
            )?;
            Ok((artifact.size_bytes.as_i64().unwrap(), wrappers))
        })
        .unwrap();
    (case, join, size, wrappers, findings)
}

#[test]
fn test_collection_size_boundary_is_atomic_and_non_echoing() {
    let (baseline, _, limit, wrappers, _) = collection_boundary(None);
    assert_eq!(wrappers.len(), 2);
    assert!(wrappers.iter().all(|size| *size <= 256 * 1024));
    drop(baseline);
    let (mut at_limit, join, size, _, _) = collection_boundary(Some(limit));
    assert_eq!(size, limit);
    at_limit.advance("design_collection");
    let pattern = format!("ledger:{}#%", join.text().unwrap());
    let stored: i64 = at_limit
        .fixture
        .with_session(|session, _| {
            Ok(session
                .connection()
                .query_row(
                    "SELECT size_bytes FROM artifacts WHERE kind='collection' AND ref LIKE ?",
                    [&pattern],
                    |row| row.get(0),
                )
                .unwrap())
        })
        .unwrap();
    assert_eq!(stored, limit);
    drop(at_limit);
    let (over, join, size, _, findings) = collection_boundary(Some(limit - 1));
    assert_eq!(size, limit);
    let revision = over.status().unwrap()["state_revision"].clone();
    let join = join.text().unwrap();
    let error = contract_error(
        over.run(&[
            "join",
            "advance",
            "--run-id",
            "RUN-1",
            "--join-id",
            &join,
            "--op-id",
            "collection-over-limit",
        ]),
        "FILE_TOO_LARGE",
    );
    assert!(error.field.equals("artifact_ref"));
    assert_eq!(error.to_string(), "artifact_ref:FILE_TOO_LARGE");
    assert!(!error.to_string().contains(&findings[0]));
    let status = over.status().unwrap();
    assert!(status["status"].is_text("active"));
    assert!(status["state_revision"].py_eq(&revision));
    over.fixture
        .with_session(|session, _| {
            let connection = session.connection();
            let pattern = format!("ledger:{join}#%");
            let artifacts: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM artifacts WHERE kind='collection' AND ref LIKE ?",
                    [&pattern],
                    |row| row.get(0),
                )
                .unwrap();
            let stored: (String, Option<String>) = connection
                .query_row(
                    "SELECT status,result_json FROM joins WHERE join_key='design_collection'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(artifacts, 0);
            assert_eq!(stored, ("open".into(), None));
            Ok(())
        })
        .unwrap();
}
