#[path = "support/hardening_case.rs"]
mod hardening_case;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        state::GraphSession,
    },
    compat::{self, PyString, Value},
};
use hardening_case::HardeningCase;

fn initialized(tags: &[&str]) -> HardeningCase {
    let mut case = HardeningCase::new();
    let task = case.fixture.graph.task("delivery", "full_delivery", tags);
    case.fixture.initialize(&task, true).unwrap();
    let mapper = case.claim();
    case.record_mapper(&mapper, tags);
    case
}

fn before_design_advance(tags: &[&str]) -> HardeningCase {
    let mut case = initialized(tags);
    let lead = case.claim();
    case.success(&lead);
    case
}

fn awaiting_design_fanout(tags: &[&str]) -> (HardeningCase, Value) {
    let mut case = before_design_advance(tags);
    case.advance("design_inputs");
    let status = case.status().unwrap();
    let fanout = status["fanouts"]
        .as_list()
        .unwrap()
        .iter()
        .find(|fanout| fanout["stage"].is_text("design"))
        .unwrap()
        .clone();
    (case, fanout)
}

fn writable(path: &str, scope: &str) -> Value {
    Value::object([("path", path.into()), ("scope", scope.into())])
}

fn service(capacity: usize) -> Value {
    Value::object([
        ("ref", "service:review".into()),
        ("units", 1_i64.into()),
        ("capacity", capacity.into()),
    ])
}

fn assessment(
    fanout: &Value,
    members: impl IntoIterator<Item = (Value, Value)>,
    dependencies: Vec<Value>,
    evidence: Value,
) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", "RUN-1".into()),
        ("fanout_id", fanout["fanout_id"].clone()),
        (
            "members",
            Value::list(members.into_iter().map(|(branch, resources)| {
                Value::object([("branch_id", branch), ("resources", resources)])
            })),
        ),
        ("dependencies", Value::List(dependencies)),
        ("evidence", Value::List(vec![evidence])),
    ])
}

fn record_assessment(
    case: &mut HardeningCase,
    fanout: &Value,
    manifest: &Value,
    operation: &str,
) -> EngineResult<Value> {
    let path = case.inbox(manifest);
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
        operation,
    ])
}

fn resource_assessment(
    case: &mut HardeningCase,
    fanout: &Value,
    dependencies: Vec<Value>,
    claims: Option<&[Value]>,
) -> Value {
    let label = format!("fanout-resource-proof-{}", case.counter());
    let evidence = case.repo_artifact("finding", &label);
    let ids = fanout["member_branch_ids"].as_list().unwrap();
    assessment(
        fanout,
        ids.iter().rev().enumerate().map(|(index, id)| {
            let default = Value::object([
                (
                    "writable_paths",
                    Value::List(vec![
                        writable(&format!("src/member-{index}/file.py"), "exact"),
                        writable(&format!("docs/member-{index}"), "subtree"),
                    ]),
                ),
                (
                    "mutable_state_refs",
                    Value::List(vec![format!("state:member-{index}").into()]),
                ),
                (
                    "exclusive_device_refs",
                    Value::List(vec![format!("device:member-{index}").into()]),
                ),
                ("services", Value::List(vec![service(ids.len())])),
            ]);
            (
                id.clone(),
                claims.map_or(default, |claims| claims[ids.len() - index - 1].clone()),
            )
        }),
        dependencies,
        evidence,
    )
}

#[test]
fn test_successful_non_mapper_result_keeps_attempt_provenance() {
    let mut case = initialized(&[]);
    let branch = case.claim();
    let kind = branch["output_contract"]["artifact_kind"].text().unwrap();
    let artifact = case.repo_artifact(&kind, "technical-design");
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", branch["branch_id"].clone()),
        ("status", "succeeded".into()),
        ("output_kind", kind.into()),
        ("artifact_ref", artifact),
        ("evidence", Value::List(vec![])),
        ("findings", Value::List(vec![])),
    ]);
    let result = case.record_as(&branch, manifest, "design-result").unwrap();
    assert!(result["branch_status"].is_text("succeeded"));
    assert!(case.status().unwrap()["status"].is_text("active"));
    case.fixture.tear_down();
}

#[test]
fn test_fanout_assessment_is_atomic_replayable_and_single_assignment() {
    let (mut case, fanout) = awaiting_design_fanout(&["security_privacy"]);
    let evidence = case.repo_artifact("finding", "assessment-proof");
    let members = fanout["member_branch_ids"].as_list().unwrap();
    let make = |ids: &[Value]| {
        assessment(
            &fanout,
            ids.iter().map(|id| {
                (
                    id.clone(),
                    Value::object([
                        ("writable_paths", Value::List(vec![])),
                        ("mutable_state_refs", Value::List(vec![])),
                        ("exclusive_device_refs", Value::List(vec![])),
                        ("services", Value::List(vec![])),
                    ]),
                )
            }),
            Vec::new(),
            evidence.clone(),
        )
    };
    let invalid = record_assessment(
        &mut case,
        &fanout,
        &make(&members[..members.len() - 1]),
        "assessment-invalid",
    );
    assert!(
        matches!(invalid, Err(EngineError::Contract(error)) if error.code == "FANOUT_MEMBER_INVALID")
    );
    let status = case.status().unwrap();
    let current = status["fanouts"]
        .as_list()
        .unwrap()
        .iter()
        .find(|row| row["fanout_id"].py_eq(&fanout["fanout_id"]))
        .unwrap();
    assert!(current["status"].is_text("awaiting"));
    let path = case.inbox(&make(members));
    let id = fanout["fanout_id"].text().unwrap();
    let assess = |operation: &str| {
        case.run(&[
            "record",
            "fanout-assessment",
            "--run-id",
            "RUN-1",
            "--fanout-id",
            &id,
            "--assessment-manifest",
            path.to_str().unwrap(),
            "--authority-ref",
            "authority:test",
            "--op-id",
            operation,
        ])
    };
    assert!(assess("assessment-valid").unwrap()["code"].is_text("FANOUT_ASSESSMENT_RECORDED"));
    assert!(assess("assessment-valid").unwrap()["code"].is_text("REPLAYED"));
    assert!(
        matches!(assess("assessment-replacement"), Err(EngineError::State(error)) if error.code == "FANOUT_ASSESSMENT_EXISTS")
    );
    case.fixture.tear_down();
}

#[test]
fn test_real_resource_assessment_is_canonical_and_promotes_three_roots() {
    let (mut case, fanout) =
        awaiting_design_fanout(&["audio_realtime_translation", "security_privacy"]);
    let members = fanout["member_branch_ids"].as_list().unwrap();
    assert_eq!(members.len(), 3);
    let manifest = resource_assessment(&mut case, &fanout, vec![], None);
    let result =
        record_assessment(&mut case, &fanout, &manifest, "real-resource-assessment").unwrap();
    assert!(result["ready_branch_ids"].py_eq(&fanout["member_branch_ids"]));
    case.fixture
        .with_session(|session, _| {
            let persisted = session
                .first(
                    GraphRead::FanoutForRun,
                    &["RUN-1".into(), fanout["fanout_id"].clone()],
                )?
                .unwrap();
            let artifact = session
                .first(GraphRead::Artifact, &[persisted["assessment_ref"].clone()])?
                .unwrap();
            let content = compat::loads_value(&artifact["content_json"])?;
            let normalized = content["members"].as_list().unwrap();
            let ids = Value::list(normalized.iter().map(|member| member["branch_id"].clone()));
            let mut sorted = members.to_vec();
            sorted.sort_by(|a, b| a.as_string().cmp(&b.as_string()));
            assert!(ids.py_eq(&Value::List(sorted)));
            for member in normalized {
                for category in [
                    "writable_paths",
                    "mutable_state_refs",
                    "exclusive_device_refs",
                    "services",
                ] {
                    assert!(member["resources"][category].truthy());
                }
            }
            let mut ready = 0;
            for member in members {
                let row = session.first(GraphRead::Node, &[member.clone()])?.unwrap();
                ready += usize::from(row["status"].is_text("ready"));
                let envelope = compat::loads_value(&row["envelope_json"])?;
                let count = envelope["inputs"]
                    .as_list()
                    .unwrap()
                    .iter()
                    .filter(|input| input["ref"].py_eq(&persisted["assessment_ref"]))
                    .count();
                assert_eq!(count, 1);
            }
            assert_eq!(ready, 3);
            Ok(())
        })
        .unwrap();
    case.fixture.tear_down();
}

fn assert_failed_assessment(
    case: &mut HardeningCase,
    fanout: &Value,
    manifest: &Value,
    code: &str,
    operation: &str,
) {
    let snapshot = |case: &HardeningCase| {
        case.fixture
            .with_session(|session, _| {
                let revision = session.run("RUN-1")?.unwrap().fields["state_revision"].clone();
                let envelopes = session.query_values(
                    "SELECT branch_id,envelope_json,status FROM nodes ORDER BY branch_id",
                    &[],
                )?;
                let prefix = format!("ledger:{}#%", fanout["fanout_id"].text().unwrap());
                let count = session.query_values(
                    "SELECT COUNT(*) AS n FROM artifacts WHERE ref LIKE ?",
                    &[prefix.into()],
                )?;
                Ok(Value::object([
                    ("revision", revision),
                    ("envelopes", Value::List(envelopes)),
                    ("artifacts", count[0]["n"].clone()),
                ]))
            })
            .unwrap()
    };
    let before = snapshot(case);
    let failure = record_assessment(case, fanout, manifest, operation);
    let matched = match failure {
        Err(EngineError::State(error)) => error.code == code,
        Err(EngineError::Contract(error)) => error.code == code,
        _ => false,
    };
    assert!(matched, "expected {code}");
    assert!(snapshot(case).py_eq(&before));
    case.fixture
        .with_session(|session, _| {
            let row = session
                .first(
                    GraphRead::FanoutForRun,
                    &["RUN-1".into(), fanout["fanout_id"].clone()],
                )?
                .unwrap();
            assert!(row["status"].is_text("awaiting"));
            assert!(session
                .all(
                    GraphRead::FanoutDependencies,
                    &[fanout["fanout_id"].clone()]
                )?
                .is_empty());
            assert!(session.operation("RUN-1", operation)?.is_none());
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_invalid_resource_assessments_are_atomic() {
    let (mut case, fanout) =
        awaiting_design_fanout(&["audio_realtime_translation", "security_privacy"]);
    let ids = fanout["member_branch_ids"].as_list().unwrap();
    let unique = || {
        ids.iter()
            .enumerate()
            .map(|(index, _)| {
                Value::object([
                    (
                        "writable_paths",
                        Value::List(vec![writable(&format!("src/{index}.py"), "exact")]),
                    ),
                    (
                        "mutable_state_refs",
                        Value::List(vec![format!("state:{index}").into()]),
                    ),
                    (
                        "exclusive_device_refs",
                        Value::List(vec![format!("device:{index}").into()]),
                    ),
                    ("services", Value::List(vec![service(3)])),
                ])
            })
            .collect::<Vec<_>>()
    };
    for category in ["file", "state", "device"] {
        let mut claims = unique();
        match category {
            "file" => {
                claims[0].set(
                    "writable_paths",
                    Value::List(vec![writable("src/shared", "subtree")]),
                );
                claims[1].set(
                    "writable_paths",
                    Value::List(vec![writable("src/shared/file.py", "exact")]),
                );
            }
            "state" => {
                for member in &mut claims[..2] {
                    member.set(
                        "mutable_state_refs",
                        Value::List(vec!["state:shared".into()]),
                    );
                }
            }
            _ => {
                for member in &mut claims[..2] {
                    member.set(
                        "exclusive_device_refs",
                        Value::List(vec!["device:shared".into()]),
                    );
                }
            }
        }
        let manifest = resource_assessment(&mut case, &fanout, vec![], Some(&claims));
        assert_failed_assessment(
            &mut case,
            &fanout,
            &manifest,
            "FANOUT_UNORDERED_CONFLICT",
            &format!("unordered-{category}"),
        );
    }
    let mut inconsistent = unique();
    inconsistent[0].set("services", Value::List(vec![service(2)]));
    let manifest = resource_assessment(&mut case, &fanout, vec![], Some(&inconsistent));
    assert_failed_assessment(
        &mut case,
        &fanout,
        &manifest,
        "FANOUT_CAPACITY_INVALID",
        "inconsistent-capacity",
    );
    let mut over = unique();
    for claim in &mut over {
        claim.set("services", Value::List(vec![service(2)]));
    }
    let manifest = resource_assessment(&mut case, &fanout, vec![], Some(&over));
    assert_failed_assessment(
        &mut case,
        &fanout,
        &manifest,
        "FANOUT_CAPACITY_EXCEEDED",
        "over-capacity",
    );
    let dependency = |before: &Value, after: &Value, reason: &str| {
        Value::object([
            ("before_branch_id", before.clone()),
            ("after_branch_id", after.clone()),
            ("reason", reason.into()),
        ])
    };
    let cycle = vec![
        dependency(&ids[0], &ids[1], "first"),
        dependency(&ids[1], &ids[0], "cycle"),
    ];
    let manifest = resource_assessment(&mut case, &fanout, cycle, None);
    assert_failed_assessment(
        &mut case,
        &fanout,
        &manifest,
        "FANOUT_CYCLE",
        "cycle-assessment",
    );
    case.fixture.tear_down();
}

#[test]
fn design_inputs_advancement_is_atomic_and_singleton_architect_is_ready() {
    let mut case = before_design_advance(&[]);
    let join = case.open_join("design_inputs");
    let id = join["join_id"].text().unwrap();
    let before = case.status().unwrap();
    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("design inputs fault".into()))
        } else {
            Ok(())
        }
    };
    let store = StateStore {
        state_root: case.fixture.graph.store.state_root.clone(),
        platform: case.fixture.graph.store.platform,
        database: case.fixture.graph.store.database,
        fault: &fault,
    };
    let argv = [
        "--repo",
        case.fixture.graph.repository.to_str().unwrap(),
        "join",
        "advance",
        "--run-id",
        "RUN-1",
        "--join-id",
        &id,
        "--op-id",
        "seal-design",
    ]
    .into_iter()
    .map(PyString::from)
    .collect::<Vec<_>>();
    assert!(matches!(
        case.fixture.execute_with_store(&argv, &store),
        Err(EngineError::Injected(message)) if message == "design inputs fault"
    ));
    case.fixture
        .with_session(|session, _| {
            let run = session.run("RUN-1")?.unwrap().fields;
            assert!(run["state_revision"].py_eq(&before["state_revision"]));
            assert!(session.operation("RUN-1", "seal-design")?.is_none());
            assert!(!session.event_exists("RUN-1", "seal-design")?);
            let row = session
                .first(GraphRead::JoinForRun, &[id.as_str().into(), "RUN-1".into()])?
                .unwrap();
            assert!(row["status"].is_text("open"));
            assert!(row["result_json"].is_null());
            assert!(session
                .query_values("SELECT 1 FROM nodes WHERE node_key='architect'", &[])?
                .is_empty());
            assert!(session
                .query_values(
                    "SELECT 1 FROM joins WHERE join_key='design_collection'",
                    &[]
                )?
                .is_empty());
            Ok(())
        })
        .unwrap();
    let seal = |join: &str, op: &str| {
        case.run(&[
            "join",
            "advance",
            "--run-id",
            "RUN-1",
            "--join-id",
            join,
            "--op-id",
            op,
        ])
    };
    let result = seal(&id, "seal-design").unwrap();
    assert!(result["code"].is_text("JOIN_ADVANCED"));
    assert!(result["outcome"].is_null());
    assert_eq!(result["created_join_ids"].as_list().unwrap().len(), 1);
    assert!(result["created_fanout_ids"].as_list().unwrap().is_empty());
    assert!(seal(&id, "seal-design").unwrap()["code"].is_text("REPLAYED"));
    assert!(
        matches!(seal("different", "seal-design"), Err(EngineError::State(error)) if error.code == "OPERATION_CONFLICT")
    );
    let architect = case.claim();
    assert!(architect["node_key"].is_text("architect"));
    assert!(
        result["successor_branch_ids"].py_eq(&Value::List(vec![architect["branch_id"].clone()]))
    );
    let kinds = architect["inputs"]
        .as_list()
        .unwrap()
        .iter()
        .map(|input| input["kind"].text().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["branch_result", "task_brief", "technical_design"]);
    case.fixture.tear_down();
}
