#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_cli::ExecuteOutcome,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
        state::GraphSession,
    },
    compat::{self, PyString, SerializationProfile, Value},
    domain::ids,
    infrastructure::process::NativeProcess,
};
use initialized_graph::InitializedGraph;
use std::{cell::Cell, fs, path::PathBuf, process::Command};

const SCRIPT: &str = concat!(
    "from pathlib import Path; p=Path('docs/check-mode.txt'); mode=p.read_text(); ",
    "p.write_text('0') if mode == 'mutate' else None; raise SystemExit(1 if mode == 'fail' else 0)"
);

fn args(strings: &[&str]) -> Vec<PyString> {
    strings.iter().map(|value| PyString::from(*value)).collect()
}

fn write_inbox(fixture: &InitializedGraph, name: &str, value: &Value) -> PathBuf {
    let path = fixture
        .graph
        .store
        .inbox_root("albanian-live-translate", "RUN-1")
        .join(name);
    fs::write(
        &path,
        compat::dumps(value, SerializationProfile::STORED).unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    path
}

struct CheckFixture {
    graph: InitializedGraph,
    mode: PathBuf,
    writer: Value,
    interpreter: String,
}

impl CheckFixture {
    fn new() -> Self {
        let mut graph = InitializedGraph::new();
        let interpreter =
            std::env::var("GRAPH_RUST_ORACLE_PYTHON").expect("fixture interpreter required");
        let mut policy = compat::loads(&graph.graph.policy_bytes).unwrap();
        let mut limits = policy["limits"].clone();
        limits.set("branch_lease_seconds", 30_i64.into());
        policy.set("limits", limits);
        let mut checks = policy["required_checks"].clone();
        let mut check = checks["repo-check"].clone();
        check.set(
            "argv",
            Value::List(vec![
                interpreter.as_str().into(),
                "-c".into(),
                SCRIPT.into(),
            ]),
        );
        checks.set("repo-check", check);
        policy.set("required_checks", checks);
        graph.graph.policy_bytes = compat::dumps(&policy, SerializationProfile::STORED).unwrap();
        fs::write(
            graph.graph.repository.join(".codex/engineering-graph.json"),
            &graph.graph.policy_bytes,
        )
        .unwrap();
        let mode = graph.graph.repository.join("docs/check-mode.txt");
        fs::write(&mode, b"fail").unwrap();
        let mut task = graph.graph.task("delivery", "fast_path", &[]);
        let mut authority = task["authority"].clone();
        let mut capabilities = authority["capabilities"].as_list().unwrap().to_vec();
        capabilities.push(Value::object([
            ("effect", "command".into()),
            ("action", "run".into()),
            ("target_ref", "npm-run-check".into()),
        ]));
        authority.set("capabilities", Value::List(capabilities));
        task.set("authority", authority);
        graph.initialize(&task, true).unwrap();

        let claim = |operation| {
            graph
                .graphctl(&args(&["status", "--run-id", "RUN-1"]))
                .unwrap();
            graph
                .graphctl(&args(&[
                    "next", "--run-id", "RUN-1", "--claim", "--op-id", operation,
                ]))
                .unwrap()
        };
        graph
            .graphctl(&args(&["status", "--run-id", "RUN-1"]))
            .unwrap();
        let mapper = claim("claim-1");
        let mapper = &mapper["branch"];
        let finding = br#"{"schema_version": 1, "kind": "finding", "label": "impact-evidence-1"}"#;
        fs::write(
            graph
                .graph
                .repository
                .join("docs/artifacts/impact-evidence-1.json"),
            finding,
        )
        .unwrap();
        let finding = format!(
            "repo:docs/artifacts/impact-evidence-1.json#sha256={}",
            ids::sha256_bytes(finding)
        );
        let manifest = Value::object([
            ("schema_version", 1_i64.into()),
            ("task_id", "TASK-1".into()),
            ("route_label", "fast_path".into()),
            ("impact_tags", Value::List(vec![])),
            ("evidence_refs", Value::List(vec![finding.into()])),
            ("attempt_id", mapper["attempt_id"].clone()),
            ("claim_digest", mapper["claim_digest"].clone()),
        ]);
        let path = write_inbox(&graph, "manifest-4.json", &manifest);
        let mut record = args(&["record", "branch-result", "--run-id", "RUN-1"]);
        for (flag, key) in [
            ("--branch-id", "branch_id"),
            ("--attempt-id", "attempt_id"),
            ("--claim-token", "claim_token"),
        ] {
            record.push(flag.into());
            record.push(mapper[key].as_string().unwrap().clone());
        }
        record.extend(args(&[
            "--result-manifest",
            path.to_str().unwrap(),
            "--op-id",
            "result-4",
        ]));
        graph.graphctl(&record).unwrap();
        let writer = claim("claim-5")["branch"].clone();
        assert!(writer["node_key"].is_text("senior_engineer"));

        let config = graph.graph.root.join("git-config-empty");
        fs::write(&config, b"").unwrap();
        let template = graph.graph.root.join("git-template-empty");
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
                .current_dir(&graph.graph.repository)
                .env("GIT_CONFIG_GLOBAL", &config)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_TEMPLATE_DIR", &template)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        let coverage = compat::loads(br#"{"schema_version":1,"kind":"check_coverage","checks":[{"check_id":"repo-check","relevant_inputs":["docs/"],"complete":true}]}"#).unwrap();
        let path = write_inbox(&graph, "manifest-6.json", &coverage);
        graph
            .graphctl(&args(&[
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
            ]))
            .unwrap();
        Self {
            graph,
            mode,
            writer,
            interpreter,
        }
    }

    fn check_args(&self, operation: &str, token: &str, predecessor: Option<&str>) -> Vec<PyString> {
        let branch = self.writer["branch_id"].text().unwrap();
        let attempt = self.writer["attempt_id"].text().unwrap();
        let claim_digest = self.writer["claim_digest"].text().unwrap();
        let mut request = args(&[
            "check",
            "run",
            "--run-id",
            "RUN-1",
            "--check-id",
            "repo-check",
            "--op-id",
            operation,
            "--executor-branch-id",
            &branch,
            "--executor-attempt-id",
            &attempt,
            "--executor-claim-token",
            token,
            "--source-branch-id",
            &branch,
            "--source-attempt-id",
            &attempt,
            "--source-claim-digest",
            &claim_digest,
        ]);
        if let Some(predecessor) = predecessor {
            request.extend(args(&["--replace-ref", predecessor]));
        }
        request
    }

    fn check(
        &self,
        operation: &str,
        token: &str,
        predecessor: Option<&str>,
    ) -> EngineResult<Value> {
        self.graph
            .graphctl(&self.check_args(operation, token, predecessor))
    }

    fn status(&self) -> Value {
        self.graph
            .graphctl(&args(&["status", "--run-id", "RUN-1"]))
            .unwrap()
    }

    fn execute_with(
        &self,
        arguments: &[PyString],
        store: &StateStore<'_>,
        process: &dyn ProcessPort,
    ) -> EngineResult<ExecuteOutcome> {
        let mut command = args(&["--repo", self.graph.graph.repository.to_str().unwrap()]);
        command.extend_from_slice(arguments);
        self.graph.execute_with_process(&command, store, process)
    }

    fn open_reservation(status: &Value) -> String {
        status["evidence_lifecycle"]["reservations"]
            .as_list()
            .unwrap()
            .iter()
            .find(|item| item["state"].is_text("open"))
            .unwrap()["reservation_id"]
            .text()
            .unwrap()
    }

    fn abandon(&self, reservation: &str, operation: &str) -> EngineResult<Value> {
        self.graph.graphctl(&args(&[
            "check",
            "abandon",
            "--run-id",
            "RUN-1",
            "--reservation-id",
            reservation,
            "--reason",
            "Original fixture process stopped",
            "--op-id",
            operation,
        ]))
    }
}

struct InterruptedCheck<'a>(&'a str);

impl ProcessPort for InterruptedCheck<'_> {
    fn capture(&self, request: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome> {
        if request
            .arguments
            .first()
            .is_some_and(|name| name.equals(self.0))
        {
            return Err(EngineError::python("RuntimeError", "interrupted"));
        }
        NativeProcess.capture(request)
    }
}

fn assert_state(result: EngineResult<Value>, code: &str) {
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == code),
        "expected {code}"
    );
}

#[test]
fn test_format7_check_replacement_token_fence_and_budget() {
    let fixture = CheckFixture::new();
    let token = fixture.writer["claim_token"].text().unwrap();
    let claim_digest = fixture.writer["claim_digest"].text().unwrap();
    assert_state(
        fixture.check("first", &claim_digest, None),
        "ATTEMPT_FENCE_MISMATCH",
    );
    let first = fixture.check("first", &token, None).unwrap();
    assert!(first["outcome"].is_text("FAIL"));
    assert!(fixture.check("first", &token, None).unwrap()["code"].is_text("REPLAYED"));
    assert_state(
        fixture.check("first", "wrong", None),
        "ATTEMPT_FENCE_MISMATCH",
    );

    fs::write(&fixture.mode, b"0").unwrap();
    let first_ref = first["artifact_ref"].text().unwrap();
    let second = fixture.check("second", &token, Some(&first_ref)).unwrap();
    assert!(second["outcome"].is_text("PASS"));
    assert!(!first["artifact_ref"].py_eq(&second["artifact_ref"]));
    let budgets = fixture.status()["budgets"].as_list().unwrap().to_vec();
    let delivery = budgets
        .iter()
        .find(|item| item["budget_id"].is_text("delivery_repairs"))
        .unwrap();
    assert!(delivery["used"].py_eq(&1_i64.into()));

    let second_ref = second["artifact_ref"].text().unwrap();
    let interrupted = fixture.check_args("interrupted", &token, Some(&second_ref));
    let result = fixture.execute_with(
        &interrupted,
        &fixture.graph.graph.store,
        &InterruptedCheck(&fixture.interpreter),
    );
    assert!(
        matches!(result, Err(EngineError::Python {kind: "RuntimeError", message}) if message.equals("interrupted"))
    );
    assert_state(
        fixture.check("interrupted", &token, Some(&second_ref)),
        "CHECK_EXECUTION_UNKNOWN",
    );
    let reservation = CheckFixture::open_reservation(&fixture.status());
    fixture.abandon(&reservation, "abandon").unwrap();

    fs::write(&fixture.mode, b"mutate").unwrap();
    let third = fixture
        .check("after-abandon", &token, Some(&second_ref))
        .unwrap();
    let third_ref = third["artifact_ref"].text().unwrap();
    assert_state(
        fixture.check("exhausted", &token, Some(&third_ref)),
        "BUDGET_LIMIT",
    );
    fixture.graph.with_session(|session, _| {
        let row = session.first(GraphRead::ArtifactContent, &[third["artifact_ref"].clone()])?.unwrap();
        let receipt = compat::loads_value(&row["content_json"])?;
        assert!(receipt["validity"].is_text("source_changed"));
        let events = session.query_values(
            "SELECT e.detail_json,o.response_json FROM events e JOIN operations o ON o.operation_id=e.source_id AND o.run_id=e.run_id WHERE e.run_id='RUN-1' AND e.event_type LIKE 'check.%'",
            &[],
        )?;
        assert!(!events.is_empty());
        assert!(!String::from_utf8(compat::dumps(&Value::List(events), SerializationProfile::STORED)?).unwrap().contains(&token));
        Ok(())
    }).unwrap();
    fixture.graph.tear_down();
}

#[test]
fn format7_check_conflicts_finish_rollback_and_fresh_status_use_durable_source() {
    let fixture = CheckFixture::new();
    fs::write(&fixture.mode, b"0").unwrap();
    let token = fixture.writer["claim_token"].text().unwrap();
    let first = fixture.check("first", &token, None).unwrap();
    let first_ref = first["artifact_ref"].text().unwrap();
    assert_state(
        fixture.check("first", &token, Some(&first_ref)),
        "OPERATION_CONFLICT",
    );
    assert_state(
        fixture.check("without-predecessor", &token, None),
        "CHECK_PREDECESSOR_CONFLICT",
    );
    let eligibility = |status: Value| {
        status["evidence_lifecycle"]["checks"].as_list().unwrap()[0]["eligibility"].clone()
    };
    let before = fixture.status();
    assert!(eligibility(before.clone()).is_text("eligible"));
    fs::write(&fixture.mode, b"changed").unwrap();
    assert!(eligibility(fixture.status()).is_text("source_changed"));

    let commits = Cell::new(0);
    let fault = |point: &str| {
        if point == "before_commit" {
            commits.set(commits.get() + 1);
            if commits.get() == 2 {
                return Err(EngineError::Injected("check finish fault".into()));
            }
        }
        Ok(())
    };
    let store = StateStore {
        state_root: fixture.graph.graph.store.state_root.clone(),
        platform: fixture.graph.graph.store.platform,
        database: fixture.graph.graph.store.database,
        fault: &fault,
    };
    let interrupted = fixture.check_args("fault-finish", &token, Some(&first_ref));
    assert!(
        matches!(fixture.execute_with(&interrupted, &store, &NativeProcess), Err(EngineError::Injected(message)) if message == "check finish fault")
    );
    assert_eq!(commits.get(), 2);
    let status = fixture.status();
    let reservation = CheckFixture::open_reservation(&status);
    assert!(
        status["state_revision"].py_eq(&(before["state_revision"].as_i64().unwrap() + 1).into())
    );
    assert!(status["check_evidence"].as_list().unwrap()[0]["artifact_ref"].is_text(&first_ref));
    fixture
        .graph
        .with_session(|session, _| {
            let digest = ids::sha256_bytes(b"fault-finish");
            let start = format!("check-start-{}", &digest[..40]);
            let finish = format!("check-finish-{}", &digest[..40]);
            assert!(session.operation("RUN-1", &start)?.is_some());
            assert!(session.operation("RUN-1", &finish)?.is_none());
            assert!(!session.event_exists("RUN-1", &finish)?);
            let artifact = format!("ledger:receipt-{}#sha256=", &digest[..40]);
            assert!(session
                .all(GraphRead::ArtifactsForValidation, &["RUN-1".into()])?
                .iter()
                .all(|row| !row["ref"].as_string().unwrap().starts_with(&artifact)));
            Ok(())
        })
        .unwrap();
    assert_state(
        fixture.check("fault-finish", &token, Some(&first_ref)),
        "CHECK_EXECUTION_UNKNOWN",
    );
    assert!(
        fixture.abandon(&reservation, "close-fault").unwrap()["code"].is_text("CHECK_ABANDONED")
    );
    assert_state(
        fixture.abandon(&reservation, "already-closed"),
        "CHECK_RESERVATION_CLOSED",
    );
    let fresh = fixture.check("fresh", &token, Some(&first_ref)).unwrap();
    assert!(fresh["validity"].is_text("valid"));
    assert!(eligibility(fixture.status()).is_text("eligible"));
    fixture.graph.tear_down();
}
