#[path = "support/daily_review.rs"]
mod daily_review;
#[path = "support/evidence_schema.rs"]
mod evidence_schema;
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[path = "support/recovery_case.rs"]
mod recovery_case;

use evidence_schema::assert_repair_schema;
use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
        state::FaultHook,
    },
    compat::{self, PyString, Value},
    domain::{ids, merge_readiness::assess_merge_readiness},
    infrastructure::process::NativeProcess,
};
use hardening_case::HardeningCase;
use recovery_case::{
    artifact_bytes, authorize, block, block_evidence, fail_review, ledger_snapshot,
    recovery_manifest, state_error, BLOCK_REASON,
};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    process::Command,
};

const REVIEW_PRODUCERS: [&str; 5] = [
    "code_reviewer",
    "code_reviewer_architecture",
    "code_reviewer_naming",
    "code_reviewer_bug_hunter",
    "test_engineer",
];

fn fast_path(case: &mut HardeningCase) -> Value {
    let task = case.fixture.graph.task("delivery", "fast_path", &[]);
    case.fixture.initialize(&task, true).unwrap();
    case.impact("fast_path");
    case.claim()
}

fn approved_design(case: &mut HardeningCase) -> Value {
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    case.fixture.initialize(&task, true).unwrap();
    case.impact("full_delivery");
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
    case.claim()
}

fn writer_manifest(writer: &Value, decision: &str) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", writer["branch_id"].clone()),
        ("status", "succeeded".into()),
        ("output_kind", "implementation_handoff".into()),
        ("evidence", Value::List(vec![])),
        ("decision", decision.into()),
        (
            "findings",
            Value::List(vec![Value::object([
                ("finding_id", "REV-900".into()),
                ("disposition", "redesign".into()),
            ])]),
        ),
    ])
}

#[test]
fn test_fast_path_has_writer_then_independent_delivery_gates() {
    let mut case = HardeningCase::golden();
    let writer = fast_path(&mut case);
    assert!(writer["role"].is_text("senior_engineer"));
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let status = case.status().unwrap();
    let fanout = &status["fanouts"].as_list().unwrap()[0];
    case.assess(&fanout["fanout_id"].text().unwrap());
    let ready = case.run(&["ready", "--run-id", "RUN-1"]).unwrap();
    let branches = ready["branches"].as_list().unwrap();
    let roles: BTreeSet<_> = branches
        .iter()
        .map(|branch| branch["role"].text().unwrap())
        .collect();
    assert_eq!(
        roles,
        ["code_reviewer".to_owned(), "test_engineer".to_owned()]
            .into_iter()
            .collect()
    );
    let reviewers: Vec<_> = branches
        .iter()
        .filter(|branch| branch["role"].is_text("code_reviewer"))
        .collect();
    assert_eq!(reviewers.len(), 4);
    let focuses: BTreeSet<_> = reviewers
        .iter()
        .map(|branch| branch["code_review_assignment"]["focus"].text().unwrap())
        .collect();
    assert_eq!(
        focuses,
        ["clean_code", "clean_architecture", "naming", "bug_hunting"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
}

#[test]
fn test_artifact_free_evidenced_redesign_packet_is_accepted() {
    let mut case = HardeningCase::golden();
    let writer = approved_design(&mut case);
    let mut manifest = writer_manifest(&writer, "REDESIGN_REQUIRED");
    let error = case
        .record_as(&writer, manifest.clone(), "result")
        .unwrap_err();
    assert!(
        matches!(&error, EngineError::Contract(error) if error.code == "REDESIGN_EVIDENCE_REQUIRED"),
        "{error:?}"
    );
    manifest.set(
        "evidence",
        Value::List(vec![case.repo_artifact("finding", "redesign-rationale")]),
    );
    case.record_as(&writer, manifest, "result").unwrap();
    let join = case.open_join("delivery_collection");
    assert_eq!(join["generation"].as_i64(), Some(0));
}

#[test]
fn test_implemented_requires_artifact_and_consistent_findings() {
    let mut case = HardeningCase::golden();
    let writer = approved_design(&mut case);
    let mut manifest = writer_manifest(&writer, "IMPLEMENTED");
    let error = case
        .record_as(&writer, manifest.clone(), "result")
        .unwrap_err();
    assert!(
        matches!(&error, EngineError::Contract(error) if error.code == "ARTIFACT_REQUIRED"),
        "{error:?}"
    );
    manifest.set(
        "artifact_ref",
        case.repo_artifact("implementation_handoff", "bad-implemented"),
    );
    let error = case.record_as(&writer, manifest, "result").unwrap_err();
    assert!(
        matches!(&error, EngineError::Contract(error) if error.code == "DECISION_FINDING_MISMATCH"),
        "{error:?}"
    );
}

#[test]
fn test_successor_context_carries_verified_task_design_and_implementation_refs() {
    let mut case = HardeningCase::golden();
    let writer = approved_design(&mut case);
    let has_input = |branch: &Value, kind: &str| {
        branch["inputs"]
            .as_list()
            .unwrap()
            .iter()
            .any(|input| input["kind"].is_text(kind))
    };
    assert!(has_input(&writer, "task_brief"));
    assert!(has_input(&writer, "technical_design"));
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let reviewer = case.claim();
    assert!(["task_brief", "technical_design", "implementation_handoff"]
        .iter()
        .all(|kind| has_input(&reviewer, kind)));
}

struct ReviewBoundary {
    case: HardeningCase,
    join_id: String,
    covered: PathBuf,
}

fn fixture_git(case: &HardeningCase, tracked: &[&str]) {
    let graph = &case.fixture.graph;
    let config = graph.root.join("git-config-empty");
    fs::write(&config, b"").unwrap();
    let template = graph.root.join("git-template-empty");
    fs::create_dir(&template).unwrap();
    let mut add = vec!["add"];
    add.extend_from_slice(tracked);
    for arguments in [
        &["init"][..],
        &add,
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
}

fn enable_coverage(case: &mut HardeningCase, relevant_input: &str) {
    enable_checks(case, &["repo-check"], relevant_input);
}

fn enable_checks(case: &mut HardeningCase, checks: &[&str], relevant_input: &str) {
    enable_check_paths(case, checks, &[relevant_input]);
}

fn enable_check_paths(case: &mut HardeningCase, checks: &[&str], relevant_inputs: &[&str]) {
    let coverage = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "check_coverage".into()),
        (
            "checks",
            Value::list(checks.iter().map(|check| {
                Value::object([
                    ("check_id", (*check).into()),
                    (
                        "relevant_inputs",
                        Value::list(relevant_inputs.iter().map(|input| (*input).into())),
                    ),
                    ("complete", true.into()),
                ])
            })),
        ),
    ]);
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

impl ReviewBoundary {
    fn new() -> Self {
        let mut case = HardeningCase::new();
        let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
        let mut authority = task["authority"].clone();
        let mut capabilities = authority["capabilities"].as_list().unwrap().to_vec();
        capabilities.push(Value::object([
            ("effect", "command".into()),
            ("action", "run".into()),
            ("target_ref", "npm-run-check".into()),
        ]));
        capabilities.push(Value::object([
            ("effect", "filesystem_write".into()),
            ("action", "edit".into()),
            ("target_ref", "repo:docs/".into()),
        ]));
        authority.set("capabilities", Value::List(capabilities));
        task.set("authority", authority);
        case.fixture.initialize(&task, true).unwrap();
        case.impact("fast_path");
        let writer = case.claim();
        case.success_with(&writer, Some("IMPLEMENTED"), vec![])
            .unwrap();

        let covered = case.fixture.graph.repository.join("docs/covered.txt");
        fs::write(&covered, b"first").unwrap();
        fixture_git(&case, &["docs/engineering-graph.md", "docs/covered.txt"]);
        let info = case.fixture.graph.repository.join(".git/info");
        fs::create_dir_all(&info).unwrap();
        fs::write(
            info.join("exclude"),
            b"/docs/artifacts/fanout-evidence-*.json\n",
        )
        .unwrap();
        enable_coverage(&mut case, "docs/covered.txt");
        let join_id = case.open_join("implementation")["join_id"].text().unwrap();
        Self {
            case,
            join_id,
            covered,
        }
    }

    fn advance_args(&mut self, plan: Option<&Value>) -> Vec<PyString> {
        let mut args = vec![
            "join",
            "advance",
            "--run-id",
            "RUN-1",
            "--join-id",
            &self.join_id,
            "--op-id",
            "review-boundary",
        ]
        .into_iter()
        .map(PyString::from)
        .collect::<Vec<_>>();
        if let Some(plan) = plan {
            let path = self.case.inbox(plan);
            args.extend([
                "--generated-output-plan".into(),
                path.to_str().unwrap().into(),
            ]);
        }
        args
    }
}

fn report(path: &str, producer: &str) -> Value {
    Value::object([
        ("path", path.into()),
        ("purpose", "review_report".into()),
        ("producer_node_key", producer.into()),
        ("artifact_kind", "delivery_review".into()),
    ])
}

fn output_plan(outputs: Vec<Value>) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "generated_output_plan".into()),
        ("outputs", Value::List(outputs)),
    ])
}

fn reviewer_reports() -> Vec<Value> {
    REVIEW_PRODUCERS
        .iter()
        .map(|producer| report(&format!("docs/artifacts/{producer}.json"), producer))
        .collect()
}

#[test]
fn format7_freezes_output_plan_and_rejects_unregistered_reports_and_source_edits() {
    let mut boundary = ReviewBoundary::new();
    let status = boundary.case.status().unwrap();
    let report_path = "docs/artifacts/review.json";
    for (plan, code) in [
        (
            output_plan(vec![report(report_path, "unknown")]),
            "PRODUCER_MISMATCH",
        ),
        (
            output_plan(vec![report("docs/covered.txt", "code_reviewer")]),
            "SOURCE_UNVERIFIABLE",
        ),
        (
            output_plan(vec![
                report(report_path, "code_reviewer"),
                report(report_path, "test_engineer"),
            ]),
            "PRODUCER_MISMATCH",
        ),
    ] {
        let argv = boundary.advance_args(Some(&plan));
        let error = boundary.case.fixture.graphctl(&argv).unwrap_err();
        assert!(
            matches!(&error, EngineError::Contract(error) if error.code == code)
                || matches!(&error, EngineError::State(error) if error.code == code),
            "{error:?}"
        );
        assert!(status["state_revision"].py_eq(&boundary.case.status().unwrap()["state_revision"]));
    }
    let plan = output_plan(vec![report(report_path, "code_reviewer")]);
    let argv = boundary.advance_args(Some(&plan));
    let advanced = boundary.case.fixture.graphctl(&argv).unwrap();
    assert!(advanced["code"].is_text("JOIN_ADVANCED"));
    let replay = boundary.case.fixture.graphctl(&argv).unwrap();
    assert!(advanced["successor_branch_ids"].py_eq(&replay["successor_branch_ids"]));
    assert!(replay["code"].is_text("REPLAYED"));
    let changed = output_plan(vec![]);
    let argv = boundary.advance_args(Some(&changed));
    let error = boundary.case.fixture.graphctl(&argv).unwrap_err();
    assert!(
        matches!(&error, EngineError::State(error) if error.code == "OPERATION_CONFLICT"),
        "{error:?}"
    );
    let fanout = &advanced["created_fanout_ids"].as_list().unwrap()[0];
    boundary.case.assess(&fanout.text().unwrap());

    let generated = boundary.case.fixture.graph.repository.join(report_path);
    fs::write(&generated, b"{}").unwrap();
    let error = boundary.case.claim_raw().unwrap_err();
    assert!(
        matches!(&error, EngineError::State(error) if error.code == "GENERATED_OUTPUT_UNREGISTERED"),
        "{error:?}"
    );
    fs::remove_file(generated).unwrap();
    fs::write(&boundary.covered, b"changed").unwrap();
    let error = boundary.case.claim_raw().unwrap_err();
    assert!(
        matches!(&error, EngineError::State(error) if error.code == "REVIEW_SOURCE_CHANGED"),
        "{error:?}"
    );
}

struct EditOnSecondCapture {
    path: PathBuf,
    captures: Cell<usize>,
}

impl ProcessPort for EditOnSecondCapture {
    fn capture(&self, request: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome> {
        if request.arguments == ["git", "rev-parse", "HEAD"].map(PyString::from) {
            let captures = self.captures.get() + 1;
            self.captures.set(captures);
            if captures == 2 {
                fs::write(&self.path, b"changed").unwrap();
            }
        }
        NativeProcess.capture(request)
    }
}

#[test]
fn source_changed_during_implementation_advance_rolls_back_binding_and_successors() {
    let mut boundary = ReviewBoundary::new();
    let status = boundary.case.status().unwrap();
    let process = EditOnSecondCapture {
        path: boundary.covered.clone(),
        captures: Cell::new(0),
    };
    let argv = boundary.advance_args(None);
    let fixture = &boundary.case.fixture;
    let mut command = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
    ];
    command.extend_from_slice(&argv);
    let error = fixture
        .execute_with_process(&command, &fixture.graph.store, &process)
        .unwrap_err();
    assert!(
        matches!(&error, EngineError::State(error) if error.code == "REVIEW_SOURCE_CHANGED"),
        "{error:?}"
    );
    assert_eq!(process.captures.get(), 2);
    let after = boundary.case.status().unwrap();
    for key in ["state_revision", "branches", "joins", "fanouts"] {
        assert!(status[key].py_eq(&after[key]), "{key}");
    }
    assert!(boundary.case.fixture.graphctl(&argv).unwrap()["code"].is_text("JOIN_ADVANCED"));
    let reviewer = boundary.case.claim();
    let prefix = format!("ledger:review-source-{}#sha256=", boundary.join_id);
    let bindings = reviewer["inputs"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|input| {
            input["kind"].is_text("evidence_manifest")
                && input["ref"].as_string().unwrap().starts_with(&prefix)
        })
        .count();
    assert_eq!(bindings, 1);
}

fn review_manifest(reviewer: &Value, artifact: Value) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", reviewer["branch_id"].clone()),
        ("status", "succeeded".into()),
        (
            "output_kind",
            reviewer["output_contract"]["artifact_kind"].clone(),
        ),
        ("artifact_ref", artifact),
        ("evidence", Value::List(vec![])),
        ("findings", Value::List(vec![])),
    ])
}

fn tagged_impact(case: &mut HardeningCase, route: &str, tags: &[&str]) {
    let mapper = case.claim();
    let evidence = case.repo_artifact("finding", &format!("impact-evidence-{}", case.counter()));
    let mut tags = tags.to_vec();
    tags.sort_unstable();
    case.record_as(
        &mapper,
        Value::object([
            ("schema_version", 1_i64.into()),
            ("task_id", "TASK-1".into()),
            ("route_label", route.into()),
            (
                "impact_tags",
                Value::list(tags.into_iter().map(Value::from)),
            ),
            ("evidence_refs", Value::List(vec![evidence["ref"].clone()])),
        ]),
        "result",
    )
    .unwrap();
}

fn delivery_only_task(case: &HardeningCase, tags: &[&str], risk: &str, scope: &str) -> Value {
    let mut task = case.fixture.graph.task("delivery", "delivery_only", tags);
    task.set("schema_version", 2_i64.into());
    task.set("risk_level", risk.into());
    task.set(
        "model_sizing",
        Value::object([
            ("scope_extent", scope.into()),
            ("uncertainty", "low".into()),
        ]),
    );
    task.set("delivery_readiness", compat::loads(br#"{"requirements_source":"JIRA-123","requirements_complete":true,"acceptance_criteria_complete":true,"implementation_decisions_resolved":true,"architecture_decisions_resolved":true,"unresolved_items":[]}"#).unwrap());
    task
}

#[test]
fn test_delivery_only_has_writer_then_independent_delivery_gates() {
    let mut case = HardeningCase::golden();
    let tags = ["production_behavior"];
    let task = delivery_only_task(&case, &tags, "medium", "cross_file");
    case.fixture.initialize(&task, true).unwrap();
    tagged_impact(&mut case, "delivery_only", &tags);
    let writer = case.claim();
    assert!(writer["node_key"].is_text("senior_engineer"));
    assert!(writer["role"].is_text("senior_engineer"));
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let status = case.status().unwrap();
    let fanout = &status["fanouts"].as_list().unwrap()[0];
    case.assess(&fanout["fanout_id"].text().unwrap());
    let ready = case.run(&["ready", "--run-id", "RUN-1"]).unwrap();
    let roles: BTreeSet<_> = ready["branches"]
        .as_list()
        .unwrap()
        .iter()
        .map(|branch| branch["role"].text().unwrap())
        .collect();
    assert_eq!(
        roles,
        ["code_reviewer", "test_engineer"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    assert!(!status["branches"].as_list().unwrap().iter().any(|branch| {
        branch["role"].is_text("tech_lead") || branch["role"].is_text("software_architect")
    }));
}

#[test]
fn test_panel_focus_cannot_be_changed_in_stored_envelope() {
    let mut case = HardeningCase::golden();
    let writer = fast_path(&mut case);
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    case.fixture
        .with_session(|session, _| {
            let connection = session.connection();
            let (id, json): (String, String) = connection.query_row(
            "SELECT branch_id,envelope_json FROM nodes WHERE node_key='code_reviewer_naming'",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
            let mut packet = compat::loads(json.as_bytes()).unwrap();
            packet.set(
                "code_review_assignment",
                Value::object([
                    ("focus", "clean_code".into()),
                    ("required_skill", "clean-code-review".into()),
                ]),
            );
            let updated = compat::dumps(&packet, compat::SerializationProfile::STORED).unwrap();
            connection
                .execute(
                    "UPDATE nodes SET envelope_json=? WHERE branch_id=?",
                    (String::from_utf8(updated).unwrap(), id),
                )
                .unwrap();
            Ok(())
        })
        .unwrap();
    let error = case.status().unwrap_err();
    assert!(
        matches!(&error, EngineError::State(error) if error.code == "ENVELOPE_INVALID"),
        "{error:?}"
    );
}

#[test]
fn test_delivery_rejects_design_revise_disposition() {
    let mut case = HardeningCase::golden();
    let writer = approved_design(&mut case);
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let reviewer = case.claim();
    let error = case
        .success_with(
            &reviewer,
            Some("REVISE"),
            vec![Value::object([
                ("finding_id", "REV-004".into()),
                ("disposition", "revise".into()),
                ("scope", "in_scope".into()),
            ])],
        )
        .unwrap_err();
    assert!(
        matches!(&error, EngineError::Contract(error) if error.code == "PHASE_DISPOSITION_MISMATCH"),
        "{error:?}"
    );
}

#[test]
fn test_delivery_requires_typed_decision_from_every_mandatory_reviewer() {
    let mut case = HardeningCase::golden();
    let tags = [
        "audio_realtime_translation",
        "ios_webkit_native",
        "release_operations",
        "security_privacy",
    ];
    let task = case.fixture.graph.task("delivery", "full_delivery", &tags);
    case.fixture.initialize(&task, true).unwrap();
    tagged_impact(&mut case, "full_delivery", &tags);
    let tech = case.claim();
    case.success(&tech);
    case.advance("design_inputs");
    for _ in 0..5 {
        let reviewer = case.claim();
        case.success_with(&reviewer, Some("APPROVE"), vec![])
            .unwrap();
    }
    case.advance("design_collection");
    case.consolidate("design", "APPROVE", 0, vec![]);
    case.advance("design_consolidation");
    let writer = case.claim();
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let mut seen = BTreeSet::new();
    for _ in 0..9 {
        let reviewer = case.claim();
        seen.insert(reviewer["role"].text().unwrap());
        let artifact = case.repo_artifact(
            &reviewer["output_contract"]["artifact_kind"].text().unwrap(),
            &format!("missing-decision-{}", reviewer["node_key"].text().unwrap()),
        );
        let error = case
            .record_as(&reviewer, review_manifest(&reviewer, artifact), "result")
            .unwrap_err();
        assert!(
            matches!(&error, EngineError::Contract(error) if error.code == "DECISION_CONTRACT_MISMATCH"),
            "{error:?}"
        );
        case.success_with(&reviewer, Some("APPROVE"), vec![])
            .unwrap();
    }
    assert_eq!(
        seen,
        [
            "code_reviewer",
            "test_engineer",
            "audio_realtime_specialist",
            "ios_platform_specialist",
            "release_operations_reviewer",
            "security_reviewer",
        ]
        .map(str::to_owned)
        .into_iter()
        .collect()
    );
    assert!(case.advance("delivery_collection")["code"].is_text("JOIN_ADVANCED"));
}

fn collection_advance_args(join_id: &str) -> Vec<PyString> {
    [
        "join",
        "advance",
        "--run-id",
        "RUN-1",
        "--join-id",
        join_id,
        "--op-id",
        "delivery-collection",
    ]
    .map(PyString::from)
    .to_vec()
}

fn draft(case: &HardeningCase, join_id: &str) -> EngineResult<Value> {
    case.run(&[
        "consolidation",
        "draft",
        "--run-id",
        "RUN-1",
        "--join-id",
        join_id,
    ])
}

fn record_review_args(
    case: &mut HardeningCase,
    reviewer: &Value,
    artifact: Value,
) -> Vec<PyString> {
    let id = reviewer["branch_id"].text().unwrap();
    let attempt = reviewer["attempt_id"].text().unwrap();
    let token = reviewer["claim_token"].text().unwrap();
    let mut manifest = review_manifest(reviewer, artifact);
    manifest.set("decision", "APPROVE".into());
    manifest.set("attempt_id", attempt.clone().into());
    manifest.set("claim_digest", ids::sha256_bytes(token.as_bytes()).into());
    let path = case.inbox(&manifest);
    [
        "record",
        "branch-result",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &id,
        "--attempt-id",
        &attempt,
        "--claim-token",
        &token,
        "--result-manifest",
        path.to_str().unwrap(),
        "--op-id",
        &format!("result-{id}"),
    ]
    .map(PyString::from)
    .to_vec()
}

#[test]
fn format7_settles_registered_reports_and_seals_exact_collection_after_source_rollback() {
    let mut boundary = ReviewBoundary::new();
    let plan = output_plan(reviewer_reports());
    let argv = boundary.advance_args(Some(&plan));
    boundary.case.fixture.graphctl(&argv).unwrap();
    let join_id = boundary.case.open_join("delivery_collection")["join_id"]
        .text()
        .unwrap();
    let advance = collection_advance_args(&join_id);
    let early = draft(&boundary.case, &join_id).unwrap_err();
    assert!(
        matches!(early, EngineError::State(error) if error.code == "SOURCE_COLLECTION_NOT_SEALED")
    );
    let mut results = BTreeSet::new();
    let mut refs = BTreeSet::new();
    for index in 0..5 {
        let reviewer = boundary.case.claim_raw().unwrap();
        let key = reviewer["node_key"].text().unwrap();
        let reference = reviewer["inputs"]
            .as_list()
            .unwrap()
            .iter()
            .find(|input| {
                input["ref"]
                    .as_string()
                    .unwrap()
                    .starts_with("ledger:review-source-")
            })
            .unwrap()["ref"]
            .text()
            .unwrap();
        refs.insert(reference);
        let artifact = boundary.case.repo_artifact("delivery_review", &key);
        let result = record_review_args(&mut boundary.case, &reviewer, artifact);
        if index == 0 {
            let revision = boundary.case.status().unwrap()["state_revision"].clone();
            fs::write(&boundary.covered, b"changed during review").unwrap();
            let error = boundary.case.fixture.graphctl(&result).unwrap_err();
            assert!(
                matches!(&error, EngineError::State(error) if error.code == "REVIEW_SOURCE_CHANGED"),
                "{error:?}"
            );
            let unchanged = boundary.case.status().unwrap();
            assert!(revision.py_eq(&unchanged["state_revision"]));
            let branch = unchanged["branches"]
                .as_list()
                .unwrap()
                .iter()
                .find(|branch| branch["branch_id"].py_eq(&reviewer["branch_id"]))
                .unwrap();
            assert!(branch["status"].is_text("running"));
            fs::write(&boundary.covered, b"first").unwrap();
        }
        let recorded = boundary.case.fixture.graphctl(&result).unwrap();
        assert!(recorded["code"].is_text("BRANCH_RESULT_RECORDED"));
        let replay = boundary.case.fixture.graphctl(&result).unwrap();
        assert!(replay["code"].is_text("REPLAYED"));
        assert!(replay["state_revision"].py_eq(&recorded["state_revision"]));
        results.insert(key);
        if index < 4 {
            let error = boundary.case.fixture.graphctl(&advance).unwrap_err();
            assert!(
                matches!(&error, EngineError::State(error) if error.code == "NOT_READY"),
                "{error:?}"
            );
        }
    }
    assert_eq!(
        results,
        REVIEW_PRODUCERS.map(str::to_owned).into_iter().collect()
    );
    assert_eq!(refs.len(), 1);
    let advanced = boundary.case.fixture.graphctl(&advance).unwrap();
    assert!(advanced["code"].is_text("JOIN_ADVANCED"));
    let replay = boundary.case.fixture.graphctl(&advance).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&advanced["state_revision"]));
    let drafted = draft(&boundary.case, &join_id).unwrap();
    assert!(drafted["code"].is_text("CONSOLIDATION_DRAFT"));
    assert!(drafted["manifest"]["outcome"].is_text("ACCEPT"));
    assert!(!drafted["unresolved_issue_identities"].truthy());
    let supervisor = boundary.case.claim_raw().unwrap();
    assert!(supervisor["node_key"].is_text("supervisor_delivery_consolidation"));
    let inputs = supervisor["inputs"].as_list().unwrap();
    let collection = inputs
        .iter()
        .find(|input| input["kind"].is_text("collection"))
        .unwrap();
    assert!(collection["sha256"].py_eq(&drafted["collection_digest"]));
    assert_eq!(collection["content"]["members"].as_list().unwrap().len(), 5);
    let binding = refs.first().unwrap();
    assert!(inputs.iter().any(|input| input["ref"].is_text(binding)));
}

#[test]
fn test_delivery_collection_waits_for_each_panel_review() {
    let mut case = HardeningCase::golden();
    let writer = fast_path(&mut case);
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let mut seen = BTreeSet::new();
    for index in 0..5 {
        let reviewer = case.claim();
        seen.insert(reviewer["node_key"].text().unwrap());
        case.success_with(&reviewer, Some("APPROVE"), vec![])
            .unwrap();
        if index < 4 {
            let join_id = case.open_join("delivery_collection")["join_id"]
                .text()
                .unwrap();
            let result = case.run(&[
                "join",
                "advance",
                "--run-id",
                "RUN-1",
                "--join-id",
                &join_id,
                "--op-id",
                &format!("early-collection-{index}"),
            ]);
            assert!(matches!(result, Err(EngineError::State(error)) if error.code == "NOT_READY"));
        }
    }
    assert_eq!(
        seen,
        REVIEW_PRODUCERS.map(str::to_owned).into_iter().collect()
    );
    case.advance("delivery_collection");
    case.consolidate("delivery", "ACCEPT", 0, vec![]);
    case.advance("delivery_consolidation");
}

#[test]
fn test_out_of_scope_delivery_observation_does_not_block_acceptance() {
    let mut case = HardeningCase::golden();
    let writer = approved_design(&mut case);
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let disposition = Value::object([
        ("finding_id", "REL-003".into()),
        ("disposition", "approve".into()),
    ]);
    for index in 0..5 {
        let reviewer = case.claim();
        let findings = if index == 0 {
            let mut observation = disposition.clone();
            observation.set("scope", "out_of_scope".into());
            vec![observation]
        } else {
            vec![]
        };
        case.success_with(&reviewer, Some("APPROVE"), findings)
            .unwrap();
    }
    case.advance("delivery_collection");
    case.consolidate("delivery", "ACCEPT", 0, vec![disposition]);
    let result = case.advance("delivery_consolidation");
    assert!(result["outcome"].is_text("ACCEPT"));
    assert!(case.status().unwrap()["status"].is_text("active"));
}

fn settle_declared_reports(case: &mut HardeningCase) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    for _ in REVIEW_PRODUCERS {
        let reviewer = case.claim_raw().unwrap();
        let reference = reviewer["inputs"]
            .as_list()
            .unwrap()
            .iter()
            .find(|input| input["kind"].is_text("evidence_manifest"))
            .unwrap();
        refs.insert(reference["ref"].text().unwrap());
        let artifact = case.repo_artifact("delivery_review", &reviewer["node_key"].text().unwrap());
        let mut manifest = review_manifest(&reviewer, artifact);
        manifest.set("decision", "APPROVE".into());
        case.record_as(&reviewer, manifest, "result").unwrap();
    }
    refs
}

fn assess_with_evidence(case: &mut HardeningCase, evidence: Value) {
    assess_generation_with_evidence(case, evidence, 0, "assess");
}

fn assess_generation_with_evidence(
    case: &mut HardeningCase,
    evidence: Value,
    generation: i64,
    operation: &str,
) {
    let status = case.status().unwrap();
    let fanout = status["fanouts"]
        .as_list()
        .unwrap()
        .iter()
        .find(|fanout| {
            fanout["stage"].is_text("delivery") && fanout["generation"].as_i64() == Some(generation)
        })
        .unwrap();
    let fanout_id = fanout["fanout_id"].text().unwrap();
    let members = Value::list(
        fanout["member_branch_ids"]
            .as_list()
            .unwrap()
            .iter()
            .map(|id| {
                Value::object([
                    ("branch_id", id.clone()),
                    (
                        "resources",
                        Value::object([
                            ("writable_paths", Value::List(vec![])),
                            ("mutable_state_refs", Value::List(vec![])),
                            ("exclusive_device_refs", Value::List(vec![])),
                            ("services", Value::List(vec![])),
                        ]),
                    ),
                ])
            }),
    );
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", "RUN-1".into()),
        ("fanout_id", fanout_id.clone().into()),
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
        &fanout_id,
        "--assessment-manifest",
        path.to_str().unwrap(),
        "--authority-ref",
        "authority:test",
        "--op-id",
        operation,
    ])
    .unwrap();
}

#[test]
fn test_review_binding_allows_declared_reports_but_rejects_later_source() {
    let mut case = HardeningCase::new();
    let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
    let mut authority = task["authority"].clone();
    let mut capabilities = authority["capabilities"].as_list().unwrap().to_vec();
    for (effect, action, target) in [
        ("command", "run", "npm-run-check"),
        ("filesystem_write", "edit", "repo:docs/"),
    ] {
        capabilities.push(Value::object([
            ("effect", effect.into()),
            ("action", action.into()),
            ("target_ref", target.into()),
        ]));
    }
    authority.set("capabilities", Value::List(capabilities));
    task.set("authority", authority);
    case.fixture.initialize(&task, true).unwrap();
    case.impact("fast_path");
    let writer = case.claim_raw().unwrap();
    let evidence = case.repo_artifact("finding", "assessment-before-boundary");
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    fixture_git(&case, &["docs/engineering-graph.md"]);
    enable_coverage(&mut case, "docs/engineering-graph.md");
    let join = case.open_join("implementation")["join_id"].text().unwrap();
    let path = case.inbox(&output_plan(reviewer_reports()));
    case.run(&[
        "join",
        "advance",
        "--run-id",
        "RUN-1",
        "--join-id",
        &join,
        "--generated-output-plan",
        path.to_str().unwrap(),
        "--op-id",
        "review-boundary",
    ])
    .unwrap();
    assess_with_evidence(&mut case, evidence);
    assert_eq!(settle_declared_reports(&mut case).len(), 1);
    let collection = case.advance("delivery_collection");
    let collection_id = collection["join_id"].text().unwrap();
    let drafted = draft(&case, &collection_id).unwrap();
    assert!(drafted["manifest"]["outcome"].is_text("ACCEPT"));
    assert!(drafted["unresolved_issue_identities"]
        .as_object()
        .unwrap()
        .is_empty());
    case.consolidate("delivery", "ACCEPT", 0, vec![]);
    fs::write(
        case.fixture.graph.repository.join("docs/new-source.txt"),
        b"B",
    )
    .unwrap();
    let join = case.open_join("delivery_consolidation")["join_id"]
        .text()
        .unwrap();
    let result = case.run(&[
        "join",
        "advance",
        "--run-id",
        "RUN-1",
        "--join-id",
        &join,
        "--op-id",
        "advance-after-source",
    ]);
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "REVIEW_SOURCE_CHANGED")
    );
}

#[test]
fn staged_format7_consolidation_matches_the_raw_manifest_and_advances_once() {
    let mut boundary = ReviewBoundary::new();
    let relative = "docs/artifacts/staged-consolidation.json";
    let mut reports = reviewer_reports();
    reports.push(Value::object([
        ("path", relative.into()),
        ("purpose", "consolidation".into()),
        (
            "producer_node_key",
            "supervisor_delivery_consolidation".into(),
        ),
        ("artifact_kind", "delivery_consolidation".into()),
    ]));
    let argv = boundary.advance_args(Some(&output_plan(reports)));
    boundary.case.fixture.graphctl(&argv).unwrap();
    settle_declared_reports(&mut boundary.case);
    let collection = boundary.case.advance("delivery_collection");
    let mut manifest =
        draft(&boundary.case, &collection["join_id"].text().unwrap()).unwrap()["manifest"].clone();
    let supervisor = boundary.case.claim_raw().unwrap();
    let branch = supervisor["branch_id"].text().unwrap();
    let token = supervisor["claim_token"].text().unwrap();
    let attempt = supervisor["attempt_id"].text().unwrap();
    manifest.set("attempt_id", attempt.clone().into());
    manifest.set("claim_digest", ids::sha256_bytes(token.as_bytes()).into());
    let path = boundary.case.inbox(&manifest);
    let record = [
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
        "staged-result",
    ];
    let staged = boundary.case.fixture.graph.repository.join(relative);
    fs::write(&staged, b"{}").unwrap();
    let before = boundary.case.status().unwrap();
    let error = boundary.case.run(&record).unwrap_err();
    assert!(
        matches!(&error, EngineError::State(error) if error.code == "GENERATED_OUTPUT_RESULT_MISMATCH"),
        "{error:?}"
    );
    let after = boundary.case.status().unwrap();
    for key in ["state_revision", "branches", "joins"] {
        assert!(before[key].py_eq(&after[key]));
    }
    let bytes = ids::canonical_bytes(&manifest).unwrap();
    fs::write(&staged, &bytes).unwrap();
    assert!(boundary.case.run(&record).unwrap()["code"].is_text("BRANCH_RESULT_RECORDED"));
    assert!(boundary.case.run(&record).unwrap()["code"].is_text("REPLAYED"));
    boundary
        .case
        .fixture
        .with_session(|session, _| {
            let node = session
                .first(
                    GraphRead::Node,
                    std::slice::from_ref(&supervisor["branch_id"]),
                )?
                .unwrap();
            let result = compat::loads_value(&node["result_json"])?;
            let digest = ids::sha256_bytes(&bytes);
            assert!(
                result["artifact_ref"]["ref"].is_text(&format!("repo:{relative}#sha256={digest}"))
            );
            assert!(result["artifact_ref"]["sha256"].is_text(&digest));
            Ok(())
        })
        .unwrap();
    let join_id = boundary.case.open_join("delivery_consolidation")["join_id"]
        .text()
        .unwrap();
    let advance = [
        "join",
        "advance",
        "--run-id",
        "RUN-1",
        "--join-id",
        &join_id,
        "--op-id",
        "accept-staged",
    ];
    let accepted = boundary.case.run(&advance).unwrap();
    assert!(accepted["outcome"].is_text("ACCEPT"));
    let closure = boundary.case.open_join("closure");
    assert!(closure["join_id"].py_eq(&accepted["created_join_ids"].as_list().unwrap()[0]));
    let replay = boundary.case.run(&advance).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&accepted["state_revision"]));
}

#[test]
fn delivery_block_seals_consolidation_without_creating_closure() {
    let mut case = HardeningCase::golden();
    let writer = fast_path(&mut case);
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    let disposition = Value::object([
        ("finding_id", "REV-001".into()),
        ("disposition", "block".into()),
    ]);
    for index in 0..5 {
        let reviewer = case.claim();
        if index == 0 {
            case.success_with(&reviewer, Some("BLOCK"), vec![disposition.clone()])
                .unwrap();
        } else {
            case.success_with(&reviewer, Some("APPROVE"), vec![])
                .unwrap();
        }
    }
    case.advance("delivery_collection");
    case.consolidate("delivery", "BLOCK", 0, vec![disposition]);
    let join_id = case.open_join("delivery_consolidation")["join_id"]
        .text()
        .unwrap();
    let advance = [
        "join",
        "advance",
        "--run-id",
        "RUN-1",
        "--join-id",
        &join_id,
        "--op-id",
        "block-delivery",
    ];
    let blocked = case.run(&advance).unwrap();
    assert!(blocked["outcome"].is_text("BLOCK"));
    assert!(blocked["created_join_ids"].as_list().unwrap().is_empty());
    let status = case.status().unwrap();
    assert!(status["status"].is_text("blocked"));
    assert!(status["blocked_reason"].is_text("CONSOLIDATION_BLOCK"));
    assert!(case.run(&advance).unwrap()["code"].is_text("REPLAYED"));
}

fn disposition(id: &str, value: &str) -> Value {
    Value::object([("finding_id", id.into()), ("disposition", value.into())])
}

fn budget_used(status: &Value, name: &str) -> i64 {
    status["budgets"]
        .as_list()
        .unwrap()
        .iter()
        .find(|budget| budget["budget_id"].is_text(name))
        .unwrap()["used"]
        .as_i64()
        .unwrap()
}

fn delivery_to_collection(case: &mut HardeningCase, writer: &Value, key: &str, finding: Value) {
    assert!(writer["node_key"].is_text("senior_engineer"));
    case.success_with(writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance("implementation");
    for _ in REVIEW_PRODUCERS {
        let reviewer = case.claim();
        if reviewer["node_key"].is_text(key) {
            case.success_with(&reviewer, Some("REVISE"), vec![finding.clone()])
                .unwrap();
        } else {
            case.success_with(&reviewer, Some("APPROVE"), vec![])
                .unwrap();
        }
    }
    case.advance("delivery_collection");
}

#[test]
fn test_golden_repair_activates_fresh_implementation_generation() {
    let mut case = HardeningCase::golden();
    let writer = approved_design(&mut case);
    let disposition = disposition("REV-001", "repair");
    let mut scoped = disposition.clone();
    scoped.set("scope", "in_scope".into());
    delivery_to_collection(&mut case, &writer, "code_reviewer", scoped);
    let before = case.status().unwrap();
    assert_eq!(budget_used(&before, "delivery_repairs"), 0);
    case.consolidate("delivery", "REPAIR", 0, vec![disposition]);
    case.advance("delivery_consolidation");
    let status = case.status().unwrap();
    let ready: Vec<_> = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|branch| branch["status"].is_text("ready"))
        .map(|branch| {
            (
                branch["node_key"].text().unwrap(),
                branch["generation"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(ready, vec![("senior_engineer".to_owned(), 1)]);
    assert_eq!(budget_used(&status, "delivery_repairs"), 1);
}

#[test]
fn test_golden_redesign_activates_fresh_design_generation() {
    let mut case = HardeningCase::golden();
    let writer = approved_design(&mut case);
    let disposition = disposition("TEST-001", "redesign");
    delivery_to_collection(&mut case, &writer, "test_engineer", disposition.clone());
    case.consolidate("delivery", "REDESIGN", 0, vec![disposition]);
    case.advance("delivery_consolidation");
    let status = case.status().unwrap();
    assert!(status["next_action"]["kind"].is_text("record_fanout_assessment"));
    let branches = status["branches"].as_list().unwrap();
    let research = branches
        .iter()
        .filter(|branch| branch["stage"].is_text("research"));
    let generations: BTreeSet<_> = research
        .clone()
        .map(|branch| branch["generation"].as_i64().unwrap())
        .collect();
    assert_eq!(generations, [0, 1].into_iter().collect());
    for (generation, phase) in [(0, "succeeded"), (1, "pending")] {
        let actual: BTreeSet<_> = research
            .clone()
            .filter(|branch| branch["generation"].as_i64() == Some(generation))
            .map(|branch| {
                (
                    branch["node_key"].text().unwrap(),
                    branch["status"].text().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            actual,
            ["design_research_architecture", "design_research_validation"]
                .map(|key| (key.to_owned(), phase.to_owned()))
                .into_iter()
                .collect()
        );
    }
    assert!(!branches
        .iter()
        .any(|branch| branch["node_key"].is_text("tech_lead")
            && branch["generation"].as_i64() == Some(1)));
    assert_eq!(budget_used(&status, "design_revisions"), 1);
}

#[test]
fn test_zero_delivery_repair_budget_blocks_at_exact_boundary() {
    let mut case = HardeningCase::golden();
    case.edit_policy(|policy| {
        let mut limits = policy["limits"].clone();
        limits.set("delivery_repairs", 0_i64.into());
        policy.set("limits", limits);
    });
    let writer = approved_design(&mut case);
    let disposition = disposition("REV-001", "repair");
    delivery_to_collection(&mut case, &writer, "code_reviewer", disposition.clone());
    case.consolidate("delivery", "REPAIR", 0, vec![disposition]);
    let result = case.advance("delivery_consolidation");
    assert!(result["outcome"].is_text("BLOCK"));
    assert!(case.status().unwrap()["status"].is_text("blocked"));
}

fn request_redesign(case: &mut HardeningCase, writer: &Value, label: &str, id: &str) {
    let rationale = case.repo_artifact("finding", label);
    let finding = disposition(id, "redesign");
    let mut result = writer_manifest(writer, "REDESIGN_REQUIRED");
    result.set("evidence", Value::list([rationale]));
    result.set("findings", Value::list([finding.clone()]));
    case.record_as(writer, result, "result").unwrap();
    case.advance("delivery_collection");
    case.consolidate("delivery", "REDESIGN", 0, vec![finding]);
    case.advance("delivery_consolidation");
}

#[test]
fn test_delivery_only_redesign_activates_fresh_design_gates() {
    let mut case = HardeningCase::golden();
    let task = delivery_only_task(&case, &[], "low", "bounded");
    case.fixture.initialize(&task, true).unwrap();
    case.impact("delivery_only");
    let writer = case.claim();
    request_redesign(
        &mut case,
        &writer,
        "delivery-only-redesign-rationale",
        "REV-778",
    );
    let status = case.status().unwrap();
    assert!(status["route"].is_text("delivery_only"));
    let research: BTreeSet<_> = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|branch| branch["stage"].is_text("research"))
        .map(|branch| branch["node_key"].text().unwrap())
        .collect();
    assert_eq!(
        research,
        ["design_research_architecture", "design_research_validation"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
}

#[test]
fn test_fast_path_redesign_runs_fresh_design_implementation_and_delivery_to_closure() {
    let mut case = HardeningCase::golden();
    let writer = fast_path(&mut case);
    request_redesign(
        &mut case,
        &writer,
        "fast-path-redesign-rationale",
        "REV-777",
    );
    let tech = case.claim();
    assert!(tech["node_key"].is_text("tech_lead"));
    assert_eq!(tech["generation"].as_i64(), Some(1));
    case.success(&tech);
    case.advance_at("design_inputs", 1);
    let architect = case.claim();
    assert!(architect["node_key"].is_text("architect"));
    assert_eq!(architect["generation"].as_i64(), Some(1));
    case.success_with(&architect, Some("APPROVE"), vec![])
        .unwrap();
    case.advance_at("design_collection", 1);
    case.consolidate("design", "APPROVE", 1, vec![]);
    case.advance_at("design_consolidation", 1);

    let replacement = case.claim();
    assert!(replacement["node_key"].is_text("senior_engineer"));
    assert_eq!(replacement["generation"].as_i64(), Some(1));
    case.success_with(&replacement, Some("IMPLEMENTED"), vec![])
        .unwrap();
    case.advance_at("implementation", 1);
    for _ in REVIEW_PRODUCERS {
        let reviewer = case.claim();
        assert_eq!(reviewer["generation"].as_i64(), Some(1));
        case.success_with(&reviewer, Some("APPROVE"), vec![])
            .unwrap();
    }
    case.advance_at("delivery_collection", 1);
    case.consolidate("delivery", "ACCEPT", 1, vec![]);
    case.advance_at("delivery_consolidation", 1);
    let status = case.status().unwrap();
    assert!(status["route"].is_text("fast_path"));
    assert!(status["next_action"]["kind"].is_text("advance_join"));
    let used: Vec<_> = status["budgets"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|budget| budget["budget_id"].is_text("design_revisions"))
        .map(|budget| budget["used"].as_i64().unwrap())
        .collect();
    assert_eq!(used, vec![1]);
}

struct RejectRepairCommit;

impl FaultHook for RejectRepairCommit {
    fn observe(&self, stage: &str) -> EngineResult<()> {
        if stage == "before_commit" {
            return Err(EngineError::Injected("repair-before-commit".into()));
        }
        Ok(())
    }
}

#[test]
fn format7_repair_rolls_back_then_binds_origin_and_the_next_source_generation() {
    let mut boundary = ReviewBoundary::new();
    let argv = boundary.advance_args(Some(&output_plan(reviewer_reports())));
    boundary.case.fixture.graphctl(&argv).unwrap();
    let repair = disposition("REV-001", "repair");
    let observation = disposition("REL-001", "approve");
    let mut observations = observation.clone();
    observations.set("scope", "out_of_scope".into());
    let mut origin = Value::Null;
    for _ in REVIEW_PRODUCERS {
        let reviewer = boundary.case.claim_raw().unwrap();
        let key = reviewer["node_key"].text().unwrap();
        let artifact = boundary.case.repo_artifact("delivery_review", &key);
        let mut manifest = review_manifest(&reviewer, artifact);
        if key == "code_reviewer" {
            manifest.set("decision", "REVISE".into());
            manifest.set(
                "findings",
                Value::list([repair.clone(), observations.clone()]),
            );
            origin = reviewer["branch_id"].clone();
        } else {
            manifest.set("decision", "APPROVE".into());
        }
        boundary
            .case
            .record_as(&reviewer, manifest, "result")
            .unwrap();
    }
    let collection = boundary.case.advance("delivery_collection");
    boundary
        .case
        .consolidate("delivery", "REPAIR", 0, vec![repair.clone(), observation]);
    let join = boundary.case.open_join("delivery_consolidation")["join_id"]
        .text()
        .unwrap();
    let advance = [
        "join",
        "advance",
        "--run-id",
        "RUN-1",
        "--join-id",
        &join,
        "--op-id",
        "advance-repair",
    ];
    let before = boundary.case.status().unwrap();
    let fixture = &boundary.case.fixture;
    let original = &fixture.graph.store;
    let store = StateStore::at_root(
        original.state_root.clone(),
        original.platform,
        original.database,
        &RejectRepairCommit,
    )
    .unwrap();
    let mut faulted_argv = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
    ];
    faulted_argv.extend(advance.map(PyString::from));
    let error = fixture
        .execute_with_store(&faulted_argv, &store)
        .unwrap_err();
    assert!(
        matches!(&error, EngineError::Injected(reason) if reason == "repair-before-commit"),
        "{error:?}"
    );
    let after = boundary.case.status().unwrap();
    for key in ["state_revision", "branches", "joins", "budgets"] {
        assert!(before[key].py_eq(&after[key]), "{key}");
    }
    let advanced = boundary.case.run(&advance).unwrap();
    assert!(advanced["outcome"].is_text("REPAIR"));
    assert!(boundary.case.run(&advance).unwrap()["code"].is_text("REPLAYED"));
    assert_eq!(
        budget_used(&boundary.case.status().unwrap(), "delivery_repairs"),
        1
    );
    let writer = boundary.case.claim_raw().unwrap();
    assert!(writer["node_key"].is_text("senior_engineer"));
    assert_eq!(writer["generation"].as_i64(), Some(1));
    let input = writer["inputs"]
        .as_list()
        .unwrap()
        .iter()
        .find(|input| {
            input["ref"]
                .as_string()
                .unwrap()
                .starts_with("ledger:repair-claim-")
        })
        .unwrap();
    let packet = boundary
        .case
        .fixture
        .with_session(|session, _| {
            let row = session
                .first(
                    GraphRead::ArtifactContent,
                    std::slice::from_ref(&input["ref"]),
                )?
                .unwrap();
            let raw = row["content_json"].text().unwrap();
            assert_eq!(input["size_bytes"].as_i64(), Some(raw.len() as i64));
            Ok(compat::loads(raw.as_bytes())?)
        })
        .unwrap();
    assert!(packet["origin_collection_id"].py_eq(&collection["join_id"]));
    assert!(packet["origin_kind"].is_text("delivery_review"));
    assert!(packet["origin_gate"].is_text("supervisor_delivery_consolidation"));
    assert!(packet["next_permitted_action"].is_text("implement_repair"));
    assert!(packet["state_revision"].py_eq(&boundary.case.status().unwrap()["state_revision"]));
    let origins = packet["origin_reports"].as_list().unwrap();
    assert_eq!(origins.len(), 5);
    let origin = origins
        .iter()
        .find(|item| item["branch_id"].py_eq(&origin))
        .unwrap();
    assert!(origin["findings"].py_eq(&Value::list([repair])));
    assert!(origin["observations"].py_eq(&Value::list([observations])));
    let mut keys = REVIEW_PRODUCERS;
    keys.sort_unstable();
    let expected = std::iter::once("senior_engineer")
        .chain(keys)
        .chain(std::iter::once("supervisor_delivery_consolidation"));
    assert!(packet["return_gates"].py_eq(&Value::list(expected.map(Value::from))));

    boundary
        .case
        .success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    boundary.join_id = boundary.case.open_join_at("implementation", 1)["join_id"]
        .text()
        .unwrap();
    let mut argv = boundary.advance_args(Some(&output_plan(vec![report(
        "docs/artifacts/code-review-g1.json",
        "code_reviewer",
    )])));
    argv[7] = "review-boundary-generation-1".into();
    let second = boundary.case.fixture.graphctl(&argv).unwrap();
    let reviewer = boundary.case.claim_raw().unwrap();
    assert_eq!(reviewer["generation"].as_i64(), Some(1));
    let prefix = format!(
        "ledger:review-source-{}#sha256=",
        second["join_id"].text().unwrap()
    );
    let binding = reviewer["inputs"]
        .as_list()
        .unwrap()
        .iter()
        .find(|input| input["ref"].as_string().unwrap().starts_with(&prefix))
        .unwrap();
    boundary
        .case
        .fixture
        .with_session(|session, _| {
            let row = session
                .first(
                    GraphRead::ArtifactContent,
                    std::slice::from_ref(&binding["ref"]),
                )?
                .unwrap();
            let source = compat::loads_value(&row["content_json"])?;
            assert!(source["source"]["branch_id"].py_eq(&writer["branch_id"]));
            assert_eq!(source["source_generation"].as_i64(), Some(1));
            assert!(source["coverage"]["outputs"].as_list().unwrap()[0]["path"]
                .is_text("docs/artifacts/code-review-g1.json"));
            Ok(())
        })
        .unwrap();
}

fn bound_check(
    case: &HardeningCase,
    check: &str,
    executor: &Value,
    source: &Value,
    operation: &str,
    previous: Option<&str>,
) -> EngineResult<Value> {
    let executor_id = executor["branch_id"].text().unwrap();
    let executor_attempt = executor["attempt_id"].text().unwrap();
    let token = executor["claim_token"].text().unwrap();
    let source_id = source["branch_id"].text().unwrap();
    let source_attempt = source["attempt_id"].text().unwrap();
    let source_claim = source["claim_digest"].text().unwrap();
    let mut args = vec![
        "check",
        "run",
        "--run-id",
        "RUN-1",
        "--check-id",
        check,
        "--executor-branch-id",
        &executor_id,
        "--executor-attempt-id",
        &executor_attempt,
        "--executor-claim-token",
        &token,
        "--source-branch-id",
        &source_id,
        "--source-attempt-id",
        &source_attempt,
        "--source-claim-digest",
        &source_claim,
        "--op-id",
        operation,
    ];
    if let Some(previous) = previous {
        args.extend(["--replace-ref", previous]);
    }
    case.run(&args)
}

fn acceptance_output(label: &str) -> Value {
    Value::object([
        ("path", format!("docs/artifacts/{label}.json").into()),
        ("purpose", "acceptance_wrapper".into()),
        (
            "producer_node_key",
            "supervisor_delivery_consolidation".into(),
        ),
        ("artifact_kind", "acceptance_evidence".into()),
    ])
}

fn accept_bound(
    case: &HardeningCase,
    artifact: &Value,
    operation: &str,
    previous: Option<&str>,
) -> EngineResult<Value> {
    let reference = artifact["ref"].text().unwrap();
    let digest = artifact["sha256"].text().unwrap();
    let mut args = vec![
        "record",
        "acceptance-evidence",
        "--run-id",
        "RUN-1",
        "--criterion-id",
        "AC-001",
        "--artifact-ref",
        &reference,
        "--artifact-sha256",
        &digest,
        "--op-id",
        operation,
    ];
    if let Some(previous) = previous {
        args.extend(["--replace-ref", previous]);
    }
    case.run(&args)
}

fn complete_bound(case: &HardeningCase, operation: &str) -> EngineResult<Value> {
    case.run(&["complete", "--run-id", "RUN-1", "--op-id", operation])
}

#[test]
fn format7_delivery_replaces_acceptance_and_checks_source_again_on_complete() {
    let mut boundary = ReviewBoundary::new();
    let writer = boundary
        .case
        .fixture
        .with_session(|session, _| {
            let row = session
                .first(
                    GraphRead::LatestSucceededContextNode,
                    &["RUN-1".into(), "senior_engineer".into()],
                )?
                .unwrap();
            Ok(compat::loads_value(&row["envelope_json"])?)
        })
        .unwrap();
    let mut outputs = reviewer_reports();
    outputs.push(acceptance_output("acceptance-a"));
    let argv = boundary.advance_args(Some(&output_plan(outputs)));
    boundary.case.fixture.graphctl(&argv).unwrap();
    let mut tester = Value::Null;
    for _ in REVIEW_PRODUCERS {
        let reviewer = boundary.case.claim_raw().unwrap();
        let artifact = boundary
            .case
            .repo_artifact("delivery_review", &reviewer["node_key"].text().unwrap());
        let mut manifest = review_manifest(&reviewer, artifact);
        manifest.set("decision", "APPROVE".into());
        boundary
            .case
            .record_as(&reviewer, manifest, "result")
            .unwrap();
        if reviewer["node_key"].is_text("test_engineer") {
            tester = reviewer;
        }
    }
    let receipt = bound_check(
        &boundary.case,
        "repo-check",
        &tester,
        &writer,
        "check",
        None,
    )
    .unwrap();
    assert!(receipt["outcome"].is_text("PASS"));
    let collection = boundary.case.advance("delivery_collection");
    let collection_id = collection["join_id"].text().unwrap();
    let drafted = draft(&boundary.case, &collection_id).unwrap();
    let collection_ref = format!(
        "ledger:{collection_id}#sha256={}",
        drafted["collection_digest"].text().unwrap()
    );
    boundary.case.consolidate("delivery", "ACCEPT", 0, vec![]);
    boundary.case.advance("delivery_consolidation");
    let first = boundary
        .case
        .repo_artifact("acceptance_evidence", "acceptance-a");
    let recorded = accept_bound(&boundary.case, &first, "first-acceptance", None).unwrap();
    let previous = recorded["artifact_ref"].text().unwrap();
    assert!(previous.starts_with("ledger:acceptance-first-acceptance#sha256="));
    let revision = boundary.case.status().unwrap()["state_revision"].clone();
    let conflict = accept_bound(&boundary.case, &first, "second-acceptance", None).unwrap_err();
    assert!(
        matches!(conflict, EngineError::State(error) if error.code == "ACCEPTANCE_PREDECESSOR_CONFLICT")
    );
    assert!(revision.py_eq(&boundary.case.status().unwrap()["state_revision"]));
    let current =
        accept_bound(&boundary.case, &first, "second-acceptance", Some(&previous)).unwrap();
    let current_ref = &current["artifact_ref"];
    boundary
        .case
        .fixture
        .with_session(|session, _| {
            let old = session
                .first(GraphRead::ArtifactContent, &[previous.clone().into()])?
                .unwrap();
            assert!(
                compat::loads_value(&old["content_json"])?["underlying_evidence"]["ref"]
                    .py_eq(&first["ref"])
            );
            let row = session
                .first(
                    GraphRead::ArtifactContent,
                    std::slice::from_ref(current_ref),
                )?
                .unwrap();
            let wrapper = compat::loads_value(&row["content_json"])?;
            assert!(wrapper["kind"].is_text("acceptance_binding"));
            assert!(wrapper["checks"]["repo-check"].py_eq(&receipt["artifact_ref"]));
            assert!(wrapper["predecessor_ref"].is_text(&previous));
            assert!(wrapper["underlying_evidence"]["ref"].py_eq(&first["ref"]));
            assert!(wrapper["source"]["branch_id"].py_eq(&writer["branch_id"]));
            assert!(wrapper["collection_ref"].is_text(&collection_ref));
            assert!(wrapper["review_source_binding_ref"]
                .as_string()
                .unwrap()
                .starts_with("ledger:review-source-"));
            Ok(())
        })
        .unwrap();
    let before = boundary.case.status().unwrap()["state_revision"].clone();
    fs::write(&boundary.covered, b"late edit").unwrap();
    let stale = complete_bound(&boundary.case, "complete").unwrap_err();
    assert!(
        matches!(stale, EngineError::State(error) if error.code == "REQUIRED_CHECKS_INCOMPLETE")
    );
    assert!(before.py_eq(&boundary.case.status().unwrap()["state_revision"]));
    fs::write(&boundary.covered, b"first").unwrap();
    assert!(complete_bound(&boundary.case, "complete").unwrap()["status"].is_text("complete"));
    assert!(complete_bound(&boundary.case, "complete").unwrap()["code"].is_text("REPLAYED"));
    assert!(
        accept_bound(&boundary.case, &first, "first-acceptance", None).unwrap()["code"]
            .is_text("REPLAYED")
    );
}

fn exercise_repair_checks(mut case: HardeningCase, stale_reviews: bool) -> HardeningCase {
    let interpreter = std::env::var("GRAPH_RUST_ORACLE_PYTHON").unwrap();
    let ids = ["integration", "repo-check", "unit"];
    case.edit_policy(|policy| {
        policy.set("schema_version", 2_i64.into());
        policy.set(
            "implementation_roots",
            Value::list(["src/".into(), "scripts/".into()]),
        );
        let mut checks = policy["required_checks"].clone();
        let mut roles = policy["role_capabilities"].clone();
        for check in ["unit", "integration"] {
            let command = format!("fixture-{check}");
            checks.set(
                check,
                Value::object([
                    ("command_id", command.clone().into()),
                    ("mandatory", true.into()),
                    (
                        "argv",
                        Value::list([interpreter.clone().into(), "-c".into(), "pass".into()]),
                    ),
                    ("timeout_seconds", 30_i64.into()),
                ]),
            );
            for role in ["senior_engineer", "test_engineer"] {
                let mut capabilities = roles[role].as_list().unwrap().to_vec();
                capabilities.push(Value::object([
                    ("effect", "command".into()),
                    ("action", "run".into()),
                    ("target_ref", command.clone().into()),
                ]));
                roles.set(role, Value::List(capabilities));
            }
        }
        policy.set("required_checks", checks);
        policy.set("role_capabilities", roles);
    });
    let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
    task.set("required_check_ids", Value::list(ids.map(Value::from)));
    let mut authority = task["authority"].clone();
    let mut capabilities = authority["capabilities"].as_list().unwrap().to_vec();
    capabilities.push(Value::object([
        ("effect", "filesystem_write".into()),
        ("action", "edit".into()),
        ("target_ref", "repo:docs/".into()),
    ]));
    for command in ["npm-run-check", "fixture-unit", "fixture-integration"] {
        capabilities.push(Value::object([
            ("effect", "command".into()),
            ("action", "run".into()),
            ("target_ref", command.into()),
        ]));
    }
    authority.set("capabilities", Value::List(capabilities));
    task.set("authority", authority);
    case.fixture.initialize(&task, true).unwrap();
    case.impact("fast_path");
    let assessment = case.repo_artifact("finding", "assessment-before-checks");
    fixture_git(&case, &["docs/engineering-graph.md"]);
    enable_checks(&mut case, &ids, "docs/engineering-graph.md");
    let mut selected = BTreeMap::<&str, String>::new();
    let mut checks = |case: &HardeningCase, executor: &Value, writer: &Value, generation: i64| {
        for check in ids {
            let operation = format!(
                "check-{generation}-{}-{check}",
                executor["node_key"].text().unwrap()
            );
            let previous = selected.get(check).map(String::as_str);
            let result = bound_check(case, check, executor, writer, &operation, previous).unwrap();
            assert!(result["outcome"].is_text("PASS"));
            selected.insert(check, result["artifact_ref"].text().unwrap());
        }
    };
    let finding_ids = ["FIX-001", "FIX-002", "FIX-003", "FIX-004", "FIX-005"];
    let mut observation = disposition("FIX-901", "approve");
    observation.set("scope", "out_of_scope".into());
    for generation in 0..2 {
        let writer = case.claim_raw().unwrap();
        assert!(writer["node_key"].is_text("senior_engineer"));
        if generation == 1 {
            case.fixture
                .with_session(|session, _| {
                    let inputs = writer["inputs"].as_list().unwrap();
                    let packets = inputs
                        .iter()
                        .filter(|item| item["kind"].is_text("failure"))
                        .map(|item| {
                            let row = session
                                .first(
                                    GraphRead::ArtifactContent,
                                    std::slice::from_ref(&item["ref"]),
                                )?
                                .unwrap();
                            Ok(compat::loads_value(&row["content_json"])?)
                        })
                        .collect::<EngineResult<Vec<_>>>()?;
                    assert_eq!(packets.len(), 1);
                    let packet = &packets[0];
                    assert_repair_schema(packet);
                    assert!(packet["origin_gate"].is_text("supervisor_delivery_consolidation"));
                    assert_eq!(
                        packet["remaining_allowances"]["delivery_repairs"].as_i64(),
                        Some(2)
                    );
                    assert!(packet["cause"].is_text("unknown"));
                    assert!(packet["failed_criterion_ids"].as_list().unwrap().is_empty());
                    let reports = packet["origin_reports"].as_list().unwrap();
                    assert_eq!(reports.len(), 5);
                    assert!(reports.iter().any(|item| item["findings"]
                        .as_list()
                        .unwrap()
                        .iter()
                        .any(|finding| finding["finding_id"].is_text("FIX-001"))));
                    let code = reports
                        .iter()
                        .find(|item| {
                            item["observations"].py_eq(&Value::list([observation.clone()]))
                        })
                        .unwrap();
                    assert!(
                        code["findings"].py_eq(&Value::list([disposition("FIX-001", "repair")]))
                    );
                    assert!(code["observations"].py_eq(&Value::list([observation.clone()])));
                    assert!(!reports
                        .iter()
                        .flat_map(|item| item["findings"].as_list().unwrap())
                        .any(|finding| finding["finding_id"].is_text("FIX-901")));
                    let scope = Value::list(
                        writer["effect_capabilities"]
                            .as_list()
                            .unwrap()
                            .iter()
                            .filter(|cap| {
                                cap["effect"].as_string().unwrap().starts_with("filesystem")
                            })
                            .cloned(),
                    );
                    assert!(scope.py_eq(&packet["authorized_scope"]));
                    Ok(())
                })
                .unwrap();
        }
        checks(&case, &writer, &writer, generation);
        case.success_with(&writer, Some("IMPLEMENTED"), vec![])
            .unwrap();
        let mut outputs = REVIEW_PRODUCERS
            .iter()
            .map(|key| report(&format!("docs/artifacts/{key}-{generation}.json"), key))
            .collect::<Vec<_>>();
        outputs.push(acceptance_output(&format!("acceptance-{generation}")));
        let plan = case.inbox(&output_plan(outputs));
        let join = case.open_join_at("implementation", generation)["join_id"]
            .text()
            .unwrap();
        case.run(&[
            "join",
            "advance",
            "--run-id",
            "RUN-1",
            "--join-id",
            &join,
            "--generated-output-plan",
            plan.to_str().unwrap(),
            "--op-id",
            &format!("boundary-{generation}"),
        ])
        .unwrap();
        assess_generation_with_evidence(
            &mut case,
            assessment.clone(),
            generation,
            &format!("assess-{generation}"),
        );
        let reviewers = REVIEW_PRODUCERS.map(|_| case.claim_raw().unwrap());
        if stale_reviews && generation == 0 {
            fs::write(
                case.fixture.graph.repository.join("docs/new-source.txt"),
                b"changed during reviews",
            )
            .unwrap();
        }
        let mut tester = Value::Null;
        for reviewer in reviewers {
            let key = reviewer["node_key"].text().unwrap();
            let artifact = case.repo_artifact("delivery_review", &format!("{key}-{generation}"));
            let mut manifest = review_manifest(&reviewer, artifact);
            let repair = generation == 0 && (key == "code_reviewer" || stale_reviews);
            let mut findings = Vec::new();
            if repair {
                let index = REVIEW_PRODUCERS
                    .iter()
                    .position(|producer| *producer == key)
                    .unwrap();
                findings.push(disposition(finding_ids[index], "repair"));
            }
            if generation == 0 && key == "code_reviewer" {
                findings.push(observation.clone());
            }
            manifest.set(
                "decision",
                if repair || (stale_reviews && generation == 0) {
                    "REVISE"
                } else {
                    "APPROVE"
                }
                .into(),
            );
            manifest.set("findings", Value::List(findings));
            case.record_as(&reviewer, manifest, "result").unwrap();
            if key == "test_engineer" {
                tester = reviewer;
            }
        }
        if !stale_reviews || generation != 0 {
            checks(&case, &tester, &writer, generation);
        }
        if generation == 0 {
            case.advance("delivery_collection");
            let repair_ids = if stale_reviews {
                &finding_ids[..]
            } else {
                &finding_ids[..1]
            };
            let mut dispositions = repair_ids
                .iter()
                .map(|id| disposition(id, "repair"))
                .collect::<Vec<_>>();
            dispositions.push(disposition("FIX-901", "approve"));
            case.consolidate("delivery", "REPAIR", 0, dispositions);
            case.advance("delivery_consolidation");
            let recovery = case
                .run(&["recovery", "show", "--run-id", "RUN-1"])
                .unwrap();
            assert!(recovery["packets"]
                .as_list()
                .unwrap()
                .iter()
                .any(|packet| packet["origin_kind"].is_text("delivery_review")));
        }
    }
    let status = case.status().unwrap();
    assert_eq!(budget_used(&status, "delivery_repairs"), 1);
    let reservations = status["evidence_lifecycle"]["reservations"]
        .as_list()
        .unwrap();
    assert_eq!(reservations.len(), if stale_reviews { 9 } else { 12 });
    assert!(reservations
        .iter()
        .all(|reservation| reservation["repeat_cost"].as_i64() == Some(0)));
    case.advance_at("delivery_collection", 1);
    case.consolidate("delivery", "ACCEPT", 1, vec![]);
    case.advance_at("delivery_consolidation", 1);
    let evidence = case.repo_artifact("acceptance_evidence", "acceptance-1");
    accept_bound(&case, &evidence, "acceptance", None).unwrap();
    assert!(complete_bound(&case, "complete").unwrap()["status"].is_text("complete"));
    case
}

#[test]
fn test_three_checks_across_repair_keep_independent_initial_slots() {
    exercise_repair_checks(HardeningCase::new(), false);
}

#[test]
fn test_stale_reviews_can_repair_and_deliver_packet_to_next_writer() {
    exercise_repair_checks(HardeningCase::new(), true);
}

#[test]
fn test_disposable_daily_repair_closes_with_checks_and_exact_head_review() {
    let mut case = HardeningCase::stock();
    let interpreter = std::env::var("GRAPH_RUST_ORACLE_PYTHON").unwrap();
    case.edit_policy(|policy| {
        let mut checks = policy["required_checks"].clone();
        let mut check = checks["repo-check"].clone();
        check.set(
            "argv",
            Value::list([interpreter.into(), "-c".into(), "pass".into()]),
        );
        checks.set("repo-check", check);
        policy.set("required_checks", checks);
    });
    let case = exercise_repair_checks(case, true);
    let status = case.status().unwrap();
    assert!(status["status"].is_text("complete"));
    assert!(status["state_schema_version"].py_eq(&7_i64.into()));
    let ids = status["check_evidence"]
        .as_list()
        .unwrap()
        .iter()
        .map(|item| item["check_id"].text().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids,
        BTreeSet::from(["repo-check", "unit", "integration"].map(str::to_owned))
    );
    assert!(status["evidence_lifecycle"]["checks"]
        .as_list()
        .unwrap()
        .iter()
        .all(|item| item["eligibility"].is_text("eligible")));
    let (binding, task) = daily_review::verify(&case, "RUN-1").unwrap();
    assert!(!binding.is_null());
    let reviewed_head = daily_review::git(&case, &["rev-parse", "HEAD"]);
    let mut merge =
        daily_review::merge_snapshot(&status, &reviewed_head, &task["required_check_ids"]);
    let readiness = assess_merge_readiness(&merge);
    assert!(readiness["decision"].is_text("DRY_RUN_READY"));
    assert!(!readiness["merge_authorized"].truthy());
    assert!(!readiness["deploy_authorized"].truthy());
    daily_review::git(
        &case,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "simulate successor PR head",
        ],
    );
    assert!(
        matches!(daily_review::verify(&case, "RUN-1"), Err(EngineError::State(error)) if error.code == "REVIEW_SOURCE_CHANGED")
    );
    merge.set(
        "head_sha",
        daily_review::git(&case, &["rev-parse", "HEAD"]).into(),
    );
    assert!(
        assess_merge_readiness(&merge)["blockers"].py_eq(&Value::list([
            "REVIEW_HEAD_CHANGED".into(),
            "CI_HEAD_CHANGED".into(),
        ]))
    );
}

#[test]
fn test_format7_equivalent_review_recovery_across_delivery_repair() {
    let mut case = HardeningCase::new();
    let interpreter = std::env::var("GRAPH_RUST_ORACLE_PYTHON").unwrap();
    case.edit_policy(|policy| {
        let mut checks = policy["required_checks"].clone();
        let mut repo = checks["repo-check"].clone();
        let code = concat!(
            "from pathlib import Path; a=Path('docs/source-a.txt').read_text(); ",
            "b=Path('docs/source-b.txt').read_text(); assert a == b and a in {'initial', 'repaired'}",
        );
        repo.set("argv", Value::list([interpreter.into(), "-c".into(), code.into()]));
        checks.set("repo-check", repo);
        policy.set("required_checks", checks);
    });
    let paths = ["docs/source-a.txt", "docs/source-b.txt"];
    let repository = case.fixture.graph.repository.clone();
    for path in paths {
        fs::write(repository.join(path), b"initial").unwrap();
    }
    let refs = paths.map(|path| Value::from(format!("repo:{path}")));
    let capability = |effect: &str, action: &str, target: Value| {
        Value::object([
            ("effect", effect.into()),
            ("action", action.into()),
            ("target_ref", target),
        ])
    };
    let writes = refs
        .clone()
        .map(|reference| capability("filesystem_write", "edit", reference));
    let capabilities = refs
        .clone()
        .into_iter()
        .map(|reference| capability("filesystem_read", "read", reference))
        .chain(writes.clone())
        .chain(std::iter::once(capability(
            "command",
            "run",
            "npm-run-check".into(),
        )));
    let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
    task.set(
        "scope",
        Value::object([
            ("included", Value::list(refs)),
            ("excluded", Value::List(vec![])),
        ]),
    );
    task.set(
        "authority",
        Value::object([("capabilities", Value::list(capabilities))]),
    );
    case.fixture.initialize(&task, true).unwrap();
    case.impact("fast_path");
    let failure = case.repo_artifact("failure", "synthetic-missing-review-skill");
    let approval = case.repo_artifact("delivery_review", "synthetic-equivalence-approval");
    let manifest = block_evidence(&mut case, BLOCK_REASON);
    let block_path = case.inbox(&manifest);
    fixture_git(&case, &["."]);
    enable_check_paths(&mut case, &["repo-check"], &paths);
    let mut selected: Option<String> = None;
    let mut preserved = Vec::new();
    let mut authorizations: Vec<Value> = Vec::new();
    let mut writers: Vec<Value> = Vec::new();
    let mut last_failed = Value::Null;
    for generation in 0..2 {
        let writer = case.claim_raw().unwrap();
        assert!(writer["node_key"].is_text("senior_engineer"));
        assert_eq!(writer["generation"].as_i64(), Some(generation));
        let granted = Value::list(
            writer["effect_capabilities"]
                .as_list()
                .unwrap()
                .iter()
                .filter(|capability| capability["effect"].is_text("filesystem_write"))
                .cloned(),
        );
        assert!(granted.py_eq(&Value::list(writes.clone())));
        if generation != 0 {
            for path in paths {
                fs::write(repository.join(path), b"repaired").unwrap();
            }
            let before = ledger_snapshot(&case);
            state_error(
                bound_check(
                    &case,
                    "repo-check",
                    &writers[0],
                    &writers[0],
                    "stale-writer",
                    selected.as_deref(),
                ),
                "ATTEMPT_FENCE_MISMATCH",
            );
            assert_eq!(before, ledger_snapshot(&case));
        }
        writers.push(writer.clone());
        let receipt = bound_check(
            &case,
            "repo-check",
            &writer,
            &writer,
            &format!("writer-check-{generation}"),
            selected.as_deref(),
        )
        .unwrap();
        assert!(receipt["outcome"].is_text("PASS"));
        selected = receipt["artifact_ref"].text();
        preserved.push((
            receipt["artifact_ref"].clone(),
            artifact_bytes(&case, &receipt["artifact_ref"]),
        ));
        case.success_with(&writer, Some("IMPLEMENTED"), vec![])
            .unwrap();
        case.advance_at("implementation", generation);
        assess_generation_with_evidence(
            &mut case,
            approval.clone(),
            generation,
            &format!("assess-{generation}"),
        );
        let mut reviewers = REVIEW_PRODUCERS.map(|_| case.claim_raw().unwrap()).to_vec();
        let failed_index = reviewers
            .iter()
            .position(|item| item["node_key"].is_text("code_reviewer_bug_hunter"))
            .unwrap();
        let failed = reviewers.remove(failed_index);
        assert!(failed["code_review_assignment"]["required_skill"].is_text("review-agent"));
        assert!(failed["code_review_assignment"]
            .get("skill_override")
            .is_none());
        fail_review(&mut case, &failed, failure.clone());
        block(
            &case,
            BLOCK_REASON,
            &block_path,
            &format!("block-{generation}"),
        );
        let manifest = recovery_manifest(&case, &failed, approval.clone());
        preserved.push((
            manifest["failure_result_ref"].clone(),
            artifact_bytes(&case, &manifest["failure_result_ref"]),
        ));
        let blocked = case.status().unwrap();
        assert!(blocked["next_action"]["recovery"]["next_step"]
            .is_text("inspect_equivalent_review_authorization"));
        let recovery = case
            .run(&["recovery", "show", "--run-id", "RUN-1"])
            .unwrap();
        assert!(recovery["blocked_recovery"]["next_step"]
            .is_text("inspect_equivalent_review_authorization"));
        if generation != 0 {
            let before = ledger_snapshot(&case);
            let error = authorize(
                &mut case,
                &failed,
                &authorizations[0],
                &manifest,
                "old-authorization",
            )
            .unwrap_err();
            assert!(
                matches!(error, EngineError::Contract(error) if error.code == "AUTHORIZATION_MISMATCH")
            );
            assert_eq!(before, ledger_snapshot(&case));
        } else {
            for (field, value) in [
                ("run_id", "WRONG-RUN".into()),
                ("task_digest", "a".repeat(64).into()),
                ("plan_digest", "b".repeat(64).into()),
                ("authority_ref", "authority:other".into()),
                ("branch_id", writer["branch_id"].clone()),
                (
                    "failure_result_ref",
                    format!("ledger:wrong#sha256={}", "a".repeat(64)).into(),
                ),
                ("prior_input_refs", Value::List(vec![])),
                ("effect_capabilities_digest", "c".repeat(64).into()),
                ("failure_evidence", Value::List(vec![])),
                ("evidence", Value::List(vec![])),
                ("replacement_skill", "other-review".into()),
                ("required_skill", "other-review".into()),
                ("decision", "REJECT".into()),
                ("equivalence_basis", "automatic-fallback".into()),
            ] {
                let before = ledger_snapshot(&case);
                let mut invalid = manifest.clone();
                invalid.set(field, value);
                let result = authorize(
                    &mut case,
                    &failed,
                    &invalid,
                    &manifest,
                    &format!("invalid-{field}"),
                );
                assert!(
                    matches!(
                        result,
                        Err(EngineError::State(_) | EngineError::Contract(_))
                    ),
                    "{field}: {result:?}"
                );
                assert_eq!(before, ledger_snapshot(&case));
                assert!(blocked["state_revision"].py_eq(&case.status().unwrap()["state_revision"]));
            }
            let source = repository.join(paths[0]);
            let original = fs::read(&source).unwrap();
            fs::write(&source, b"stale").unwrap();
            let before = ledger_snapshot(&case);
            state_error(
                authorize(&mut case, &failed, &manifest, &manifest, "stale-source"),
                "REVIEW_SOURCE_CHANGED",
            );
            assert_eq!(before, ledger_snapshot(&case));
            assert!(blocked["state_revision"].py_eq(&case.status().unwrap()["state_revision"]));
            fs::write(source, original).unwrap();
        }
        let op = format!("recover-{generation}");
        let result = authorize(&mut case, &failed, &manifest, &manifest, &op).unwrap();
        assert!(result["code"].is_text("GRAPH_RECOVERED"));
        let before = ledger_snapshot(&case);
        assert!(
            authorize(&mut case, &failed, &manifest, &manifest, &op).unwrap()["code"]
                .is_text("REPLAYED")
        );
        let mut rejected = manifest.clone();
        rejected.set("decision", "REJECT".into());
        state_error(
            authorize(&mut case, &failed, &rejected, &manifest, &op),
            "OPERATION_CONFLICT",
        );
        assert_eq!(before, ledger_snapshot(&case));
        authorizations.push(manifest);
        case.run(&[
            "resume",
            "--run-id",
            "RUN-1",
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
        ])
        .unwrap();
        let status = case.status().unwrap();
        let architecture = status["branches"]
            .as_list()
            .unwrap()
            .iter()
            .find(|branch| {
                branch["node_key"].is_text("code_reviewer_architecture")
                    && branch["generation"].as_i64() == Some(generation)
            })
            .unwrap();
        assert!(!architecture["status"].is_text("succeeded"));
        let retried = case.claim_raw().unwrap();
        assert!(retried["branch_id"].py_eq(&failed["branch_id"]));
        assert!(!retried["attempt_id"].py_eq(&failed["attempt_id"]));
        assert!(retried["code_review_assignment"]["required_skill"].is_text("bug-hunter-review"));
        assert!(retried["effect_capabilities"].py_eq(&failed["effect_capabilities"]));
        let before = ledger_snapshot(&case);
        let stale = Value::object([
            ("schema_version", 1_i64.into()),
            ("run_id", "RUN-1".into()),
            ("branch_id", failed["branch_id"].clone()),
            ("status", "failed".into()),
            ("output_kind", "delivery_review".into()),
            ("failure_code", "MISSING_REQUIRED_SKILL".into()),
            ("evidence", Value::list([failure.clone()])),
        ]);
        state_error(
            case.record_as(&failed, stale, "result"),
            "ATTEMPT_FENCE_MISMATCH",
        );
        assert_eq!(before, ledger_snapshot(&case));
        last_failed = failed;
        reviewers.push(retried);
        for reviewer in reviewers {
            let key = reviewer["node_key"].text().unwrap();
            let artifact = case.repo_artifact("delivery_review", &format!("{key}-{generation}"));
            let repair = generation == 0 && key == "code_reviewer";
            if key == "test_engineer" {
                let before = ledger_snapshot(&case);
                state_error(
                    bound_check(
                        &case,
                        "repo-check",
                        &reviewer,
                        &writer,
                        &format!("missing-predecessor-{generation}"),
                        None,
                    ),
                    "CHECK_PREDECESSOR_CONFLICT",
                );
                let first = preserved[0].0.text().unwrap();
                if selected.as_deref() != Some(first.as_str()) {
                    assert!(matches!(
                        bound_check(
                            &case,
                            "repo-check",
                            &reviewer,
                            &writer,
                            "stale-predecessor",
                            Some(&first)
                        ),
                        Err(EngineError::State(_))
                    ));
                }
                assert_eq!(before, ledger_snapshot(&case));
                let check = bound_check(
                    &case,
                    "repo-check",
                    &reviewer,
                    &writer,
                    &format!("tester-check-{generation}"),
                    selected.as_deref(),
                )
                .unwrap();
                assert!(check["outcome"].is_text("PASS"));
                selected = check["artifact_ref"].text();
                preserved.push((
                    check["artifact_ref"].clone(),
                    artifact_bytes(&case, &check["artifact_ref"]),
                ));
            }
            let mut manifest = review_manifest(&reviewer, artifact);
            manifest.set("decision", if repair { "REVISE" } else { "APPROVE" }.into());
            if repair {
                let mut finding = disposition("FIX-001", "repair");
                finding.set("scope", "in_scope".into());
                manifest.set("findings", Value::list([finding]));
            }
            case.record_as(&reviewer, manifest, "result").unwrap();
        }
        case.advance_at("delivery_collection", generation);
        let (outcome, findings) = if generation == 0 {
            ("REPAIR", vec![disposition("FIX-001", "repair")])
        } else {
            ("ACCEPT", vec![])
        };
        case.consolidate("delivery", outcome, generation, findings);
        case.advance_at("delivery_consolidation", generation);
    }
    let acceptance = case.repo_artifact("acceptance_evidence", "acceptance-1");
    accept_bound(&case, &acceptance, "acceptance", None).unwrap();
    assert!(complete_bound(&case, "complete").unwrap()["status"].is_text("complete"));
    let status = case.status().unwrap();
    let before = ledger_snapshot(&case);
    let manifest = authorizations.last().unwrap();
    state_error(
        authorize(
            &mut case,
            &last_failed,
            manifest,
            manifest,
            "forbidden-complete-recovery",
        ),
        "TERMINAL_RUN",
    );
    assert_eq!(before, ledger_snapshot(&case));
    assert!(status.py_eq(&case.status().unwrap()));
    assert_eq!(budget_used(&status, "delivery_repairs"), 1);
    let retries: Vec<_> = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|branch| branch["node_key"].is_text("code_reviewer_bug_hunter"))
        .map(|branch| branch["retry_count"].as_i64().unwrap())
        .collect();
    assert_eq!(retries, [1, 1]);
    for (reference, original) in &preserved {
        assert_eq!(*original, artifact_bytes(&case, reference));
    }
    assert_eq!(
        paths.map(|path| fs::read_to_string(repository.join(path)).unwrap()),
        ["repaired", "repaired"]
    );
    let diff = Command::new("git")
        .args(["diff", "--name-only"])
        .current_dir(&repository)
        .output()
        .unwrap();
    assert!(diff.status.success());
    let changed = String::from_utf8(diff.stdout).unwrap();
    assert_eq!(changed.lines().collect::<Vec<_>>(), paths);
    let preserved = Value::Object(
        preserved
            .into_iter()
            .map(|(reference, bytes)| {
                (
                    reference.as_string().unwrap().clone(),
                    ids::sha256_bytes(bytes.as_bytes()).into(),
                )
            })
            .collect(),
    );
    let source_sha256 = Value::Object(
        paths
            .into_iter()
            .map(|path| {
                (
                    path.into(),
                    ids::sha256_bytes(&fs::read(repository.join(path)).unwrap()).into(),
                )
            })
            .collect(),
    );
    let trace = Value::object([
        ("synthetic_role_reports", true.into()),
        ("status", status["status"].clone()),
        ("generations", 2_i64.into()),
        ("delivery_repairs_used", 1_i64.into()),
        (
            "review_retry_counts",
            Value::list(retries.into_iter().map(Value::from)),
        ),
        ("exact_source_paths", Value::list(paths.map(Value::from))),
        ("source_sha256", source_sha256),
        ("preserved_artifacts", preserved),
        (
            "check_outcomes",
            Value::list(
                status["check_evidence"]
                    .as_list()
                    .unwrap()
                    .iter()
                    .map(|item| item["outcome"].clone()),
            ),
        ),
    ]);
    println!(
        "FORMAT7_RECOVERY_TRACE {}",
        String::from_utf8(ids::canonical_bytes(&trace).unwrap()).unwrap()
    );
}
