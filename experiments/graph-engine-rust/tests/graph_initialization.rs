#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{
        error::EngineError,
        graph_arguments,
        graph_cli::ExecuteOutcome,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        semantic_state::verify_semantic_state,
        state::GraphSession,
    },
    compat::{self, argparse::ParserOutcome, PyString, SerializationProfile, Value},
    domain::ids,
};
use initialized_graph::InitializedGraph;
use std::{fs, process::Command};

fn native_cli(fixture: &InitializedGraph) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_graphctl"));
    command
        .arg("--repo")
        .arg(&fixture.graph.repository)
        .arg("--state-root")
        .arg(&fixture.graph.runtime_root);
    command
}

fn legacy_check_fixture(script: &str, extra: Option<&str>) -> InitializedGraph {
    let mut fixture = InitializedGraph::new();
    let mut policy = compat::loads(&fixture.graph.policy_bytes).unwrap();
    let mut limits = policy["limits"].clone();
    limits.set("branch_lease_seconds", 30_i64.into());
    policy.set("limits", limits);
    let mut checks = policy["required_checks"].clone();
    let mut repo_check = checks["repo-check"].clone();
    let python = std::env::var("GRAPH_RUST_ORACLE_PYTHON").expect("fixture interpreter required");
    let mut argv = vec![python.into(), "-c".into(), script.into()];
    argv.extend(extra.map(Value::from));
    repo_check.set("argv", Value::List(argv));
    checks.set("repo-check", repo_check);
    policy.set("required_checks", checks);
    fixture.graph.policy_bytes = compat::dumps(&policy, SerializationProfile::STORED).unwrap();
    fs::write(
        fixture.graph.repository.join(".codex/engineering-graph.json"),
        &fixture.graph.policy_bytes,
    )
    .unwrap();
    fixture
}

#[test]
fn test_only_local_check_runner_receipts_can_satisfy_checks() {
    let fixture = legacy_check_fixture("import sys; sys.exit(0)", None);
    fixture.initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true).unwrap();
    let command = |argv: &[&str]| {
        let argv = argv.iter().map(|arg| PyString::from(*arg)).collect::<Vec<_>>();
        fixture.graphctl(&argv)
    };
    command(&["status", "--run-id", "RUN-1"]).unwrap();
    command(&["status", "--run-id", "RUN-1"]).unwrap();
    let mapper = command(&["next", "--run-id", "RUN-1", "--claim", "--op-id", "claim-1"]).unwrap();
    let mapper = &mapper["branch"];
    let repo_artifact = |kind: &str, label: &str| {
        let body = Value::object([
            ("schema_version", 1_i64.into()), ("kind", kind.into()), ("label", label.into()),
        ]);
        let data = compat::dumps(&body, SerializationProfile::STORED).unwrap();
        let relative = format!("docs/artifacts/{label}.json");
        fs::write(fixture.graph.repository.join(&relative), &data).unwrap();
        let digest = ids::sha256_bytes(&data);
        (format!("repo:{relative}#sha256={digest}"), digest)
    };
    let (finding, _) = repo_artifact("finding", "impact-evidence");
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("task_id", "TASK-1".into()),
        ("route_label", "full_delivery".into()),
        ("impact_tags", Value::List(vec![])),
        ("evidence_refs", Value::List(vec![finding.into()])),
        ("attempt_id", mapper["attempt_id"].clone()),
        ("claim_digest", mapper["claim_digest"].clone()),
    ]);
    let path = fixture.graph.store.inbox_root("albanian-live-translate", "RUN-1").join("manifest-4.json");
    fs::write(&path, compat::dumps(&manifest, SerializationProfile::STORED).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    command(&[
        "record", "branch-result", "--run-id", "RUN-1",
        "--branch-id", &mapper["branch_id"].text().unwrap(),
        "--attempt-id", &mapper["attempt_id"].text().unwrap(),
        "--claim-token", &mapper["claim_token"].text().unwrap(),
        "--result-manifest", path.to_str().unwrap(), "--op-id", "mapper-result-4",
    ]).unwrap();
    let receipt = command(&["check", "run", "--run-id", "RUN-1", "--check-id", "repo-check", "--op-id", "check-1"]).unwrap();
    assert!(receipt["code"].is_text("CHECK_RECORDED"));
    fixture.with_session(|session, _| {
        let selected = session.first(GraphRead::CheckEvidenceForRun, &["RUN-1".into(), "repo-check".into()])?.unwrap();
        let artifact = session.first(GraphRead::Artifact, &[selected["artifact_ref"].clone()])?.unwrap();
        assert!(artifact["source_type"].is_text("ledger"));
        assert!(compat::loads_value(&artifact["content_json"])?["kind"].is_text("check_receipt"));
        Ok(())
    }).unwrap();
    let (forged, digest) = repo_artifact("check_evidence", "forged-check");
    let rejected = command(&[
        "record", "check-evidence", "--run-id", "RUN-1", "--check-id", "repo-check",
        "--outcome", "PASS", "--artifact-ref", &forged, "--artifact-sha256", &digest,
        "--op-id", "forged-check-1",
    ]);
    assert!(matches!(rejected, Err(EngineError::State(error)) if error.code == "CHECK_RECEIPT_REQUIRED"));
    fixture.tear_down();
}

#[test]
fn native_legacy_check_receipt_is_bound_to_the_runner_and_fresh_git_state() {
    let fixture = legacy_check_fixture("import sys; sys.exit(0)", None);
    let docs = fixture.graph.repository.join("docs");
    let tracked = docs.join("check-input.txt");
    fs::write(&tracked, b"first source\n").unwrap();
    let forged = b"{}";
    fs::write(docs.join("forged-check.json"), forged).unwrap();
    for args in [
        &["init", "--quiet", "--template="][..],
        &["add", "--", "docs/check-input.txt"],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(&fixture.graph.repository)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    fixture
        .initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true)
        .unwrap();
    let check = |check_id, operation, exit_code| {
        native_json(
            native_cli(&fixture).args([
                "check", "run", "--run-id", "RUN-1", "--check-id", check_id, "--op-id",
                operation,
            ]),
            exit_code,
        )
    };
    let record = |reference, digest, outcome, operation, exit_code| {
        native_json(
            native_cli(&fixture).args([
                "record", "check-evidence", "--run-id", "RUN-1", "--check-id", "repo-check",
                "--artifact-ref", reference, "--artifact-sha256", digest, "--outcome", outcome,
                "--op-id", operation,
            ]),
            exit_code,
        )
    };
    assert!(check("missing", "unknown-check", 4)["code"].is_text("UNKNOWN_CHECK"));
    let forged_digest = ids::sha256_bytes(forged);
    let forged_ref = format!("repo:docs/forged-check.json#sha256={forged_digest}");
    assert!(
        record(&forged_ref, &forged_digest, "PASS", "forged-check", 4)["code"]
            .is_text("CHECK_RECEIPT_REQUIRED")
    );
    let result = check("repo-check", "check-1", 0);
    assert!(result["code"].is_text("CHECK_RECORDED"));
    assert!(result["outcome"].is_text("PASS"));
    let reference = result["artifact_ref"].text().unwrap();
    let digest = result["artifact_sha256"].text().unwrap();
    assert_eq!(
        reference,
        format!("ledger:check-repo-check-check-1#sha256={digest}")
    );
    fixture
        .with_session(|session, context| {
            let selected = session
                .first(GraphRead::CheckEvidenceForRun, &["RUN-1".into(), "repo-check".into()])?
                .unwrap();
            assert!(selected["artifact_ref"].is_text(&reference));
            assert!(selected["artifact_sha256"].is_text(&digest));
            let artifact = session
                .first(GraphRead::Artifact, &[selected["artifact_ref"].clone()])?
                .unwrap();
            assert!(artifact["source_type"].is_text("ledger"));
            let receipt = compat::loads_value(&artifact["content_json"])?;
            assert!(receipt["kind"].is_text("check_receipt"));
            assert!(receipt["argv"].py_eq(&context.policy["required_checks"]["repo-check"]["argv"]));
            assert!(receipt["exit_code"].py_eq(&0_i64.into()));
            assert!(receipt["stdout_sha256"].is_text(&ids::sha256_bytes(b"")));
            assert!(receipt["stderr_sha256"].is_text(&ids::sha256_bytes(b"")));
            assert_eq!(ids::sha256_bytes(&ids::canonical_bytes(&receipt)?), digest);
            Ok(())
        })
        .unwrap();
    assert!(check("repo-check", "duplicate-check", 4)["code"].is_text("EVIDENCE_CONFLICT"));
    assert!(record(&reference, &digest, "PASS", "check-1", 5)["code"].is_text("OPERATION_CONFLICT"));
    assert!(
        record(&reference, &digest, "FAIL", "wrong-outcome", 4)["code"].is_text("EVIDENCE_CONFLICT")
    );
    let recorded = record(&reference, &digest, "PASS", "record-receipt", 0);
    assert!(recorded["code"].is_text("CHECK_EVIDENCE_RECORDED"));
    let status = |exit_code| {
        native_json(native_cli(&fixture).args(["status", "--run-id", "RUN-1"]), exit_code)
    };
    assert!(status(0)["state_revision"].py_eq(&recorded["state_revision"]));
    fs::write(&tracked, b"changed source\n").unwrap();
    assert!(status(4)["code"].is_text("CHECK_EVIDENCE_PROVENANCE_INVALID"));
    let replayed = check("repo-check", "check-1", 0);
    assert!(replayed["code"].is_text("REPLAYED"));
    assert!(replayed["state_revision"].py_eq(&result["state_revision"]));
    assert!(replayed["artifact_ref"].is_text(&reference));
    fixture.tear_down();
}

#[test]
fn legacy_check_rolls_back_durable_evidence_and_replay_skips_process() {
    let fixture = legacy_check_fixture(
        "import pathlib,sys; p=pathlib.Path(sys.argv[1]); p.write_bytes(p.read_bytes()+b'x')",
        Some("../check-invocations"),
    );
    let marker = fixture.graph.root.join("check-invocations");
    fs::write(&marker, b"").unwrap();
    fixture
        .initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true)
        .unwrap();
    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("check fault".into()))
        } else {
            Ok(())
        }
    };
    let store = StateStore {
        state_root: fixture.graph.store.state_root.clone(),
        platform: fixture.graph.store.platform,
        database: fixture.graph.store.database,
        fault: &fault,
    };
    let mut args = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
    ];
    args.extend(
        [
            "check", "run", "--run-id", "RUN-1", "--check-id", "repo-check", "--op-id",
            "check-fault",
        ]
        .into_iter()
        .map(Into::into),
    );
    assert!(matches!(
        fixture.execute_with_store(&args, &store),
        Err(EngineError::Injected(message)) if message == "check fault"
    ));
    assert_eq!(fs::read(&marker).unwrap(), b"x");
    fixture
        .with_session(|session, _| {
            let run = session.run("RUN-1")?.unwrap().fields;
            assert!(run["state_revision"].py_eq(&2_i64.into()));
            assert!(session.operation("RUN-1", "check-fault")?.is_none());
            assert!(session
                .first(GraphRead::CheckEvidenceForRun, &["RUN-1".into(), "repo-check".into()])?
                .is_none());
            assert!(session
                .all(GraphRead::ArtifactsForValidation, &["RUN-1".into()])?
                .iter()
                .all(|artifact| !artifact["kind"].is_text("check_evidence")));
            Ok(())
        })
        .unwrap();
    let result = fixture.graphctl(&args[2..]).unwrap();
    assert!(result["code"].is_text("CHECK_RECORDED"));
    assert_eq!(fs::read(&marker).unwrap(), b"xx");
    let replayed = fixture.graphctl(&args[2..]).unwrap();
    assert!(replayed["code"].is_text("REPLAYED"));
    assert!(replayed["artifact_ref"].py_eq(&result["artifact_ref"]));
    assert_eq!(fs::read(&marker).unwrap(), b"xx");
    fixture.tear_down();
}

fn native_json(command: &mut Command, exit_code: i32) -> Value {
    let output = command.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(exit_code),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    compat::loads(&output.stdout).unwrap()
}

#[test]
fn native_cli_constraint_preview_checks_support_and_leaves_no_run_state() {
    let fixture = InitializedGraph::new();
    let docs = fixture.graph.repository.join("docs");
    let bundle = docs.join("constraint-bundle.json");
    let context = docs.join("constraint-context.json");
    let finding = br#"{"finding_id":"finding-1"}"#;
    let fix = br#"{"finding_id":"finding-1","status":"accepted"}"#;
    fs::write(docs.join("finding.json"), finding).unwrap();
    fs::write(docs.join("fix.json"), fix).unwrap();
    let mut record = compat::loads(
        br#"{
        "id":"constraint-1", "revision":1, "statement":"Keep the check",
        "rationale":"The accepted fix depends on the check", "state":"active",
        "repository_id":"albanian-live-translate", "paths":["docs/"],
        "roles":["tech_lead"], "acceptance_ids":["AC1"], "finding_id":"finding-1",
        "confirmed_finding_ref":null, "accepted_fix_ref":null,
        "authority_ref":"repo:docs/engineering-graph.md",
        "invalidation_reason":null, "supersedes":[]
    }"#,
    )
    .unwrap();
    record.set(
        "confirmed_finding_ref",
        format!("repo:docs/finding.json#sha256={}", ids::sha256_bytes(finding)).into(),
    );
    record.set(
        "accepted_fix_ref",
        format!("repo:docs/fix.json#sha256={}", ids::sha256_bytes(fix)).into(),
    );
    let mut bundle_value = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "planning_constraints".into()),
        ("records", Value::List(vec![record.clone()])),
    ]);
    let write_bundle = |value: &Value| {
        fs::write(
            &bundle,
            compat::dumps(value, SerializationProfile::CLI).unwrap(),
        )
        .unwrap();
    };
    write_bundle(&bundle_value);
    let scope = if cfg!(windows) {
        "DOCS/fix.json"
    } else {
        "docs/fix.json"
    };
    let context_value = Value::object([
        ("repository_id", "albanian-live-translate".into()),
        ("role", "tech_lead".into()),
        ("acceptance_ids", Value::List(vec!["AC1".into()])),
        ("scope_paths", Value::List(vec![scope.into()])),
    ]);
    fs::write(
        &context,
        compat::dumps(&context_value, SerializationProfile::CLI).unwrap(),
    )
    .unwrap();
    let select = |exit_code| {
        native_json(
            native_cli(&fixture)
                .args(["constraints", "select", "--bundle"])
                .arg(&bundle)
                .arg("--context")
                .arg(&context),
            exit_code,
        )
    };
    let result = select(0);
    assert!(result["code"].is_text("CONSTRAINTS_SELECTED"));
    assert!(result["run_id"].is_null());
    assert!(result["state_revision"].is_null());
    assert!(result["active"].as_list().unwrap().is_empty());
    assert!(result["excluded"].as_list().unwrap().is_empty());
    assert!(result["candidates"].py_eq(&Value::List(vec![record.clone()])));

    let unaccepted = br#"{"finding_id":"finding-1","status":"rejected"}"#;
    fs::write(docs.join("fix.json"), unaccepted).unwrap();
    assert!(select(4)["code"].is_text("INPUT_DIGEST_MISMATCH"));
    record.set(
        "accepted_fix_ref",
        format!("repo:docs/fix.json#sha256={}", ids::sha256_bytes(unaccepted)).into(),
    );
    bundle_value.set("records", Value::List(vec![record]));
    write_bundle(&bundle_value);
    let mismatch = select(4);
    assert!(mismatch["code"].is_text("FINDING_FIX_IDENTITY_MISMATCH"));
    assert!(mismatch["field"].is_text("constraint_support"));

    bundle_value.set("kind", "wrong_kind".into());
    write_bundle(&bundle_value);
    fs::write(&context, b"{").unwrap();
    let invalid = select(4);
    assert!(invalid["code"].is_text("INVALID_JSON"));
    assert!(invalid["field"].is_text("path"));
    assert!(!fixture.graph.runtime_root.join("graph-runs").exists());
    fixture.tear_down();
}

#[test]
fn native_cli_policy_and_plan_preview_precede_initialization_and_status() {
    let fixture = InitializedGraph::new();
    let task = fixture.graph.task("delivery", "full_delivery", &[]);
    let task_path = fixture.graph.repository.join("docs/task.json");
    fs::write(
        &task_path,
        compat::dumps(&task, SerializationProfile::STORED).unwrap(),
    )
    .unwrap();
    let policy = native_json(native_cli(&fixture).arg("policy"), 0);
    assert!(policy["code"].is_text("POLICY_LOADED"));
    assert!(policy["policy_digest"].is_text(&ids::sha256_bytes(&fixture.graph.policy_bytes)));

    let plan = native_json(
        native_cli(&fixture)
            .args(["plan", "--run-id", "RUN-1", "--task-brief"])
            .arg(&task_path),
        0,
    );
    assert!(plan["code"].is_text("EXECUTION_PLAN_PREVIEW"));
    assert!(plan["state_revision"].is_null());
    assert!(plan["execution_plan"].as_object().is_some());
    assert!(!fixture.graph.runtime_root.join("graph-runs").exists());

    let initialized = native_json(
        native_cli(&fixture)
            .args([
                "--ack-degraded-permissions",
                "--ack-degraded-durability",
                "init",
                "--run-id",
                "RUN-1",
                "--task-brief",
            ])
            .arg(&task_path)
            .args(["--op-id", "init-1"]),
        0,
    );
    assert!(initialized["code"].is_text("INITIALIZED"));
    let status = native_json(native_cli(&fixture).args(["status", "--run-id", "RUN-1"]), 0);
    assert!(status["code"].is_text("STATUS"));
    assert!(status["run_id"].is_text("RUN-1"));
    assert!(status["state_revision"].py_eq(&initialized["state_revision"]));
    fixture.tear_down();
}

#[test]
fn native_cli_readonly_recovery_and_join_lookup_preserve_run_history() {
    let fixture = InitializedGraph::new();
    let task = fixture.graph.task("delivery", "full_delivery", &[]);
    fixture.initialize(&task, true).unwrap();
    let recovery = |branch: Option<&str>, exit_code| {
        let mut command = native_cli(&fixture);
        command.args(["recovery", "show", "--run-id", "RUN-1"]);
        if let Some(branch) = branch {
            command.args(["--branch-id", branch]);
        }
        native_json(&mut command, exit_code)
    };
    let status = || native_json(native_cli(&fixture).args(["status", "--run-id", "RUN-1"]), 0);
    let history = || {
        fixture
            .with_session(|session, _| {
                session.all(GraphRead::OperationResponses, &["RUN-1".into()])
            })
            .unwrap()
    };
    let active = status();
    let active_history = history();
    let ready = native_json(native_cli(&fixture).args(["ready", "--run-id", "RUN-1"]), 0);
    let branch = ready["branches"].as_list().unwrap()[0]["branch_id"]
        .text()
        .unwrap();
    let empty = recovery(None, 0);
    assert!(empty["code"].is_text("RECOVERY"));
    assert!(empty["packets"].as_list().unwrap().is_empty());
    assert!(empty["blocked_recovery"].is_null());
    assert!(empty["state_revision"].py_eq(&active["state_revision"]));
    let selected = recovery(Some(&branch), 0);
    assert!(selected["packets"].as_list().unwrap()[0]["branch_id"].is_text(&branch));
    assert!(recovery(Some("UNKNOWN"), 4)["code"].is_text("BRANCH_NOT_FOUND"));
    let missing_join = native_json(
        native_cli(&fixture).args([
            "join", "validate", "--run-id", "RUN-1", "--join-id", "UNKNOWN",
        ]),
        4,
    );
    assert!(missing_join["code"].is_text("JOIN_NOT_FOUND"));
    assert!(status()["state_revision"].py_eq(&active["state_revision"]));
    assert!(Value::List(history()).py_eq(&Value::List(active_history)));

    let inbox = fixture.graph.store.inbox_root("albanian-live-translate", "RUN-1");
    let manifest = inbox.join("block-recovery.json");
    fs::write(
        &manifest,
        br#"{"schema_version":1,"kind":"block","run_id":"RUN-1","reason_code":"STOP","evidence":[]}"#,
    )
    .unwrap();
    let blocked = native_json(
        native_cli(&fixture)
            .args([
                "block",
                "--run-id",
                "RUN-1",
                "--reason-code",
                "STOP",
                "--evidence-manifest",
            ])
            .arg(&manifest)
            .args(["--op-id", "block-recovery"]),
        3,
    );
    let blocked_history = history();
    let report = recovery(Some(&branch), 0);
    assert!(report["blocked_recovery"]["next_step"].is_text("new_plan_required"));
    assert!(report["blocked_recovery"]["blocked_reason"].is_text("STOP"));
    assert!(report["packets"].as_list().unwrap()[0]["next_permitted_action"]
        .is_text("supervisor_assessment"));
    assert!(report["state_revision"].py_eq(&blocked["state_revision"]));
    assert!(status()["status"].is_text("blocked"));
    assert!(Value::List(history()).py_eq(&Value::List(blocked_history)));
    fixture.tear_down();
}

#[test]
fn production_parser_is_reused_with_per_call_argv_and_width() {
    let parser = graph_arguments::shared_parser();
    assert!(std::ptr::eq(parser, graph_arguments::shared_parser()));
    let narrow = parser.parse(&["--help".into()], 20);
    let wide = graph_arguments::shared_parser().parse(&["--help".into()], 120);
    match (narrow, wide) {
        (
            ParserOutcome::Exit {
                code: 0,
                stdout: narrow,
                stderr: narrow_error,
            },
            ParserOutcome::Exit {
                code: 0,
                stdout: wide,
                stderr: wide_error,
            },
        ) => {
            assert_ne!(narrow, wide);
            assert!(narrow_error.is_empty());
            assert!(wide_error.is_empty());
        }
        other => panic!("unexpected help outcomes: {other:?}"),
    }
    assert!(matches!(
        parser.parse(&["policy".into()], 80),
        ParserOutcome::Arguments(_)
    ));
    assert!(std::ptr::eq(parser, graph_arguments::shared_parser()));
}

#[test]
fn execute_preserves_parser_exit_and_missing_repository_before_policy_loading() {
    let fixture = InitializedGraph::new();
    assert!(matches!(
        fixture.execute(&["--help".into()]).unwrap(),
        ExecuteOutcome::ParserExit { exit_code: 0, .. }
    ));
    let missing = fixture.execute(&["policy".into()]);
    assert!(
        matches!(missing, Err(EngineError::Contract(error)) if error.field.equals("repo") && error.code == "MISSING_FIELD")
    );
    assert!(!fixture.graph.runtime_root.join("graph-runs").exists());
    fixture.tear_down();
}

#[test]
fn original_initialization_and_approval_fixture_uses_real_policy_ports() {
    let fixture = InitializedGraph::new();
    let task = fixture.graph.task("delivery", "full_delivery", &[]);
    let result = fixture.initialize(&task, true).unwrap();
    assert!(result["code"].is_text("INITIALIZED"));
    assert!(result["state_revision"].py_eq(&1_i64.into()));
    fixture
        .with_session(|session, context| {
            let run = session.run("RUN-1")?.unwrap().fields;
            assert!(run["status"].is_text("active"));
            assert!(run["state_revision"].py_eq(&2_i64.into()));
            assert!(run["task_path"].text().unwrap().ends_with("task.json"));
            assert!(run["task_ref"]
                .text()
                .unwrap()
                .starts_with("repo:docs/task.json#sha256="));
            verify_semantic_state(session, &run, context)
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn schema_three_allowance_initialization_is_validated_and_persisted() {
    let fixture = InitializedGraph::new();
    let allowance=compat::loads(br#"{"schema_version":1,"allowance_id":"helpers-1","run_id":"RUN-1","assignments":[{"assignment_id":"tech-evidence","parent_role":"tech_lead","helper_role":"evidence_scout","contract_revision":1,"model":"gpt-6.1-sol","reasoning_effort":"low","parent_capabilities":[{"effect":"filesystem_read","action":"read","target_ref":"repo:docs/"}],"scope_refs":["repo:docs/"],"commands":[],"checkpoint_policy":"observed_repository_state","resource_keys":["worktree"],"required_host_capabilities":["fresh_model_effort_selection","filesystem_confinement","tool_confinement"],"limits":{"children":1,"concurrency":1,"commands":0,"time_seconds":60,"output_tokens":1000,"file_reads":3}}],"shared_limits":{"children":1,"concurrency":1,"commands":0,"time_seconds":60,"output_tokens":1000,"file_reads":3},"resources":[{"key":"worktree","capacity":1}]}"#).unwrap();
    let path = fixture.graph.repository.join("docs/helper-allowance.json");
    let bytes = ids::canonical_bytes(&allowance).unwrap();
    fs::write(&path, &bytes).unwrap();
    let mut task = fixture.graph.task("delivery", "full_delivery", &[]);
    task.set("schema_version", 3_i64.into());
    task.set(
        "model_sizing",
        Value::object([
            ("scope_extent", "bounded".into()),
            ("uncertainty", "low".into()),
        ]),
    );
    task.set(
        "helper_allowance",
        Value::object([
            ("ref", "repo:docs/helper-allowance.json".into()),
            ("sha256", ids::sha256_bytes(&bytes).into()),
        ]),
    );
    let result = fixture.initialize(&task, true).unwrap();
    assert!(result["execution_plan"]["schema_version"].py_eq(&3_i64.into()));
    assert!(result["execution_plan"]["helper_allowance"].py_eq(&task["helper_allowance"]));
    fixture
        .with_session(|session, context| {
            let run = session.run("RUN-1")?.unwrap().fields;
            let plan = session
                .first(GraphRead::ExecutionPlan, &["RUN-1".into()])?
                .unwrap();
            assert!(plan["status"].is_text("approved"));
            verify_semantic_state(session, &run, context)
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn task_policy_approval_mismatch_fails_before_any_database_is_initialized() {
    let fixture = InitializedGraph::new();
    let mut task = fixture.graph.task("delivery", "full_delivery", &[]);
    let mut approval = task["policy_approval"].clone();
    approval.set("sha256", "0".repeat(64).into());
    task.set("policy_approval", approval);
    let result = fixture.initialize(&task, false);
    assert!(
        matches!(result,Err(EngineError::Contract(error)) if error.field.equals("policy_approval.sha256") && error.code=="POLICY_DIGEST_MISMATCH")
    );
    assert!(!fixture
        .graph
        .store
        .db_path("albanian-live-translate", "RUN-1")
        .exists());
    fixture.tear_down();
}
