#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_cli::ExecuteOutcome,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        semantic_state::verify_semantic_state,
        state::{FaultHook, GraphSession},
    },
    compat::{self, PyString, Value},
    domain::ids,
};
use initialized_graph::InitializedGraph;
use std::{cell::Cell, fs, path::PathBuf};

fn initialize(approve: bool) -> InitializedGraph {
    let fixture = InitializedGraph::new();
    fixture
        .initialize(
            &fixture.graph.task("delivery", "full_delivery", &[]),
            approve,
        )
        .unwrap();
    fixture
}

fn claim(fixture: &InitializedGraph, operation: &str) -> EngineResult<Value> {
    fixture.graphctl(&[
        "next".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--claim".into(),
        "--op-id".into(),
        operation.into(),
    ])
}

fn result_arguments(
    fixture: &InitializedGraph,
    branch: &Value,
    manifest: &Value,
    operation: &str,
) -> Vec<PyString> {
    let path = fixture
        .graph
        .store
        .inbox_root("albanian-live-translate", "RUN-1")
        .join("mapper-result.json");
    fs::write(&path, ids::canonical_bytes(manifest).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    vec![
        "record".into(),
        "branch-result".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--branch-id".into(),
        branch["branch_id"].as_string().unwrap().clone(),
        "--attempt-id".into(),
        branch["attempt_id"].as_string().unwrap().clone(),
        "--claim-token".into(),
        branch["claim_token"].as_string().unwrap().clone(),
        "--result-manifest".into(),
        path.to_str().unwrap().into(),
        "--op-id".into(),
        operation.into(),
    ]
}

fn impact_manifest(branch: &Value, route: &str, references: Vec<Value>) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("task_id", "TASK-1".into()),
        ("route_label", route.into()),
        ("impact_tags", Value::List(vec![])),
        ("evidence_refs", Value::List(references)),
        ("attempt_id", branch["attempt_id"].clone()),
        ("claim_digest", branch["claim_digest"].clone()),
    ])
}

#[test]
fn mapper_results_commit_route_and_original_first_successors() {
    for (mode, route, successor) in [
        ("delivery", "fast_path", "senior_engineer"),
        ("delivery", "delivery_only", "senior_engineer"),
        ("advisory", "advisory", "advisory_reviewer"),
    ] {
        let fixture = InitializedGraph::new();
        let mut task = fixture.graph.task(mode, route, &[]);
        if route == "delivery_only" {
            task.set("schema_version", 2_i64.into());
            task.set("model_sizing", Value::object([
                ("scope_extent", "bounded".into()), ("uncertainty", "low".into()),
            ]));
            task.set("delivery_readiness", Value::object([
                ("requirements_source", "JIRA-123".into()),
                ("requirements_complete", true.into()),
                ("acceptance_criteria_complete", true.into()),
                ("implementation_decisions_resolved", true.into()),
                ("architecture_decisions_resolved", true.into()),
                ("unresolved_items", Value::List(vec![])),
            ]));
        }
        fixture.initialize(&task, true).unwrap();
        let mapper = claim(&fixture, "claim-mapper").unwrap();
        let branch = &mapper["branch"];
        assert!(branch["node_key"].is_text("impact_mapper"));
        let first = b"{}";
        let second = br#"{"finding_id":"F1"}"#;
        fs::write(fixture.graph.repository.join("docs/z.json"), first).unwrap();
        fs::write(fixture.graph.repository.join("docs/a.json"), second).unwrap();
        let first_ref = format!("repo:docs/z.json#sha256={}", ids::sha256_bytes(first));
        let second_ref = format!("repo:docs/a.json#sha256={}", ids::sha256_bytes(second));
        let manifest = impact_manifest(branch, route, vec![first_ref.as_str().into(), "repo:docs/a.json".into()]);
        let args = result_arguments(&fixture, branch, &manifest, "record-mapper");
        let result = fixture.graphctl(&args).unwrap();
        assert!(result["code"].is_text("BRANCH_RESULT_RECORDED"));
        assert!(result["branch_status"].is_text("succeeded"));
        assert!(result["state_revision"].py_eq(&4_i64.into()));
        assert!(result["created_join_ids"].as_list().unwrap().is_empty());
        assert!(result["created_fanout_ids"].as_list().unwrap().is_empty());
        assert!(result["promoted_branch_ids"].as_list().unwrap().is_empty());
        fixture.with_session(|session, context| {
            let run = session.run("RUN-1")?.unwrap().fields;
            assert!(run["selected_route"].is_text(route));
            assert!(run["selected_tags_json"].is_text("[]"));
            let node = session.first(GraphRead::Node, &[branch["branch_id"].clone()])?.unwrap();
            let envelope = compat::loads_value(&node["envelope_json"])?;
            let normalized = compat::loads_value(&node["result_json"])?;
            assert!(normalized["evidence_refs"].py_eq(&Value::List(vec![second_ref.as_str().into(), first_ref.as_str().into()])));
            assert_eq!(node["result_digest"].text().unwrap(), ids::sha256_bytes(&ids::canonical_bytes(&normalized)?));
            let attempt = session.first(GraphRead::FenceAttempt, &[
                "RUN-1".into(), branch["branch_id"].clone(), branch["attempt_id"].clone(),
            ])?.unwrap();
            assert!(attempt["outcome"].is_text("succeeded"));
            assert!(attempt["finished_at"].py_eq(&node["finished_at"]));
            assert!(envelope.get("claim_token").is_none());
            assert!(envelope["artifact_ref"]["kind"].is_text("impact_map"));
            let event = session.query_values("SELECT detail_json FROM events WHERE source_id='record-mapper'", &[])?;
            let detail = compat::loads_value(&event[0]["detail_json"])?;
            assert!(detail.get("claim_token").is_none());
            assert!(detail["claim_digest"].py_eq(&branch["claim_digest"]));
            verify_semantic_state(session, &run, context)
        }).unwrap();
        let replay = fixture.graphctl(&args).unwrap();
        assert!(replay["code"].is_text("REPLAYED"));
        assert!(replay["state_revision"].py_eq(&result["state_revision"]));
        let next = claim(&fixture, "claim-successor").unwrap();
        assert!(next["branch"]["node_key"].is_text(successor));
        assert!(result["successor_branch_ids"].py_eq(&Value::List(vec![next["branch"]["branch_id"].clone()])));
        let inputs = next["branch"]["inputs"].as_list().unwrap();
        assert_eq!(inputs.len(), 1);
        assert!(inputs[0]["kind"].is_text("task_brief"));
        fixture.tear_down();
    }
}

#[test]
fn mapper_research_routes_open_pending_pair_collection_and_awaiting_fanout() {
    for (mode, route) in [("delivery", "full_delivery"), ("design_only", "design_only")] {
        let fixture = InitializedGraph::new();
        fixture.initialize(&fixture.graph.task(mode, route, &[]), true).unwrap();
        let mapper = claim(&fixture, "claim-mapper").unwrap();
        let branch = &mapper["branch"];
        let evidence = fixture.graph.repository.join("docs/impact-evidence.json");
        fs::write(evidence, b"{}").unwrap();
        let manifest = impact_manifest(branch, route, vec!["repo:docs/impact-evidence.json".into()]);
        let args = result_arguments(&fixture, branch, &manifest, "record-mapper");
        let result = fixture.graphctl(&args).unwrap();
        assert!(result["branch_status"].is_text("succeeded"));
        let branches = result["successor_branch_ids"].as_list().unwrap();
        assert_eq!(branches.len(), 2);
        assert!(branches[0].as_string() < branches[1].as_string());
        let joins = result["created_join_ids"].as_list().unwrap();
        let fanouts = result["created_fanout_ids"].as_list().unwrap();
        assert_eq!(joins.len(), 1);
        assert_eq!(fanouts.len(), 1);
        assert!(result["promoted_branch_ids"].as_list().unwrap().is_empty());

        let status = fixture.graphctl(&["status".into(), "--run-id".into(), "RUN-1".into()]).unwrap();
        assert!(status["route"].is_text(route));
        assert!(status["next_action"]["kind"].is_text("record_fanout_assessment"));
        assert!(status["next_action"]["fanout_id"].py_eq(&fanouts[0]));
        let join = &status["joins"].as_list().unwrap()[0];
        assert!(join["join_id"].py_eq(&joins[0]));
        assert!(join["join_key"].is_text("research_collection"));
        assert!(join["status"].is_text("open"));
        let fanout = &status["fanouts"].as_list().unwrap()[0];
        assert!(fanout["stage"].is_text("research"));
        assert!(fanout["status"].is_text("awaiting"));
        assert!(fanout["generation"].py_eq(&0_i64.into()));
        assert!(fanout["member_branch_ids"].py_eq(&result["successor_branch_ids"]));
        let argv = [
            "--repo".into(), fixture.graph.repository.to_str().unwrap().into(),
            "join".into(), "validate".into(), "--run-id".into(), "RUN-1".into(),
            "--join-id".into(), joins[0].as_string().unwrap().clone(),
        ];
        match fixture.execute(&argv).unwrap() {
            ExecuteOutcome::Response { result, exit_code } => {
                assert_eq!(exit_code, 2);
                assert!(result["code"].is_text("NOT_READY"));
                assert!(result["groups"]["pending"].py_eq(&Value::List(branches.to_vec())));
            }
            other => panic!("unexpected parser result: {other:?}"),
        }
        fixture.with_session(|session, context| {
            let members = session.all(GraphRead::JoinMembers, &joins[0..1])?;
            assert!(members.iter().all(|node| node["join_mandatory"].truthy()));
            let mut keys = members.iter().map(|node| node["node_key"].text().unwrap()).collect::<Vec<_>>();
            keys.sort();
            assert_eq!(keys, ["design_research_architecture", "design_research_validation"]);
            let run = session.run("RUN-1")?.unwrap().fields;
            verify_semantic_state(session, &run, context)
        }).unwrap();
        let replay = fixture.graphctl(&args).unwrap();
        assert!(replay["code"].is_text("REPLAYED"));
        assert!(replay["created_fanout_ids"].py_eq(&result["created_fanout_ids"]));
        assert!(replay["state_revision"].py_eq(&result["state_revision"]));
        fixture.tear_down();
    }
}

#[test]
fn research_fanout_assessment_persists_ordered_dependencies_and_rolls_back_atomically() {
    let fixture = initialize(true);
    let mapper = claim(&fixture, "claim-mapper").unwrap();
    let branch = &mapper["branch"];
    let impact_path = fixture.graph.repository.join("docs/impact.json");
    fs::write(impact_path, b"{}").unwrap();
    let impact = impact_manifest(branch, "full_delivery", vec!["repo:docs/impact.json".into()]);
    let mapped = fixture
        .graphctl(&result_arguments(&fixture, branch, &impact, "record-mapper"))
        .unwrap();
    let fanout_id = mapped["created_fanout_ids"].as_list().unwrap()[0].clone();
    let members = mapped["successor_branch_ids"].as_list().unwrap();
    let [first, second] = members else { panic!("expected two research branches") };
    let resources = Value::object([
        ("writable_paths", Value::List(vec![Value::object([
            ("path", "docs/shared.md".into()),
            ("scope", "exact".into()),
        ])])),
        ("mutable_state_refs", Value::List(vec![])),
        ("exclusive_device_refs", Value::List(vec![])),
        ("services", Value::List(vec![])),
    ]);
    let submitted_members = Value::list(members.iter().rev().map(|member| {
        Value::object([("branch_id", member.clone()), ("resources", resources.clone())])
    }));
    let finding = b"{}";
    fs::write(fixture.graph.repository.join("docs/assessment.json"), finding).unwrap();
    let sha = ids::sha256_bytes(finding);
    let evidence = Value::object([
        ("kind", "finding".into()),
        ("ref", format!("repo:docs/assessment.json#sha256={sha}").into()),
        ("sha256", sha.into()),
    ]);
    let mut manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", "RUN-1".into()),
        ("fanout_id", fanout_id.clone()),
        ("members", submitted_members),
        ("dependencies", Value::List(vec![])),
        ("evidence", Value::List(vec![evidence])),
    ]);
    let command = |manifest: &Value, authority: &str, operation: &str| {
        let path = fixture.graph.store.inbox_root("albanian-live-translate", "RUN-1").join("assessment.json");
        fs::write(&path, ids::canonical_bytes(manifest).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        vec![
            "record".into(), "fanout-assessment".into(), "--run-id".into(), "RUN-1".into(),
            "--fanout-id".into(), fanout_id.as_string().unwrap().clone(),
            "--assessment-manifest".into(), path.to_str().unwrap().into(),
            "--authority-ref".into(), authority.into(), "--op-id".into(), operation.into(),
        ]
    };
    let ledger = || fixture.with_session(|session, _| {
        let run = Value::from("RUN-1");
        let fanout = session.first(GraphRead::FanoutForRun, &[run.clone(), fanout_id.clone()])?.unwrap();
        Ok(Value::object([
            ("revision", session.run("RUN-1")?.unwrap().fields["state_revision"].clone()),
            ("fanout", fanout),
            ("nodes", Value::List(session.all(GraphRead::RootBranches, &[run.clone()])?)),
            ("dependencies", Value::List(session.all(GraphRead::FanoutDependencies, &[fanout_id.clone()])?)),
            ("operations", Value::List(session.all(GraphRead::OperationResponses, &[run.clone()])?)),
            ("artifacts", Value::List(session.all(GraphRead::ArtifactsForValidation, &[run.clone()])?)),
            ("events", Value::List(session.all(GraphRead::ReservationEvents, &[run])?)),
        ]))
    }).unwrap();
    let before = ledger();
    let mut missing = manifest.clone();
    missing.set("members", Value::List(vec![]));
    let invalid = fixture.graphctl(&command(&missing, "authority:test", "invalid-members"));
    assert!(matches!(invalid, Err(EngineError::Contract(error)) if error.code == "FANOUT_MEMBER_INVALID"));
    let unordered = fixture.graphctl(&command(&manifest, "authority:test", "unordered"));
    assert!(matches!(unordered, Err(EngineError::State(error)) if error.code == "FANOUT_UNORDERED_CONFLICT"));
    assert!(ledger().py_eq(&before));
    let dependency = |before: &Value, after: &Value| Value::object([
        ("before_branch_id", before.clone()),
        ("after_branch_id", after.clone()),
        ("reason", "One writer at a time".into()),
    ]);
    let edge = dependency(first, second);
    manifest.set("dependencies", Value::List(vec![edge.clone(), dependency(second, first)]));
    let cycle = fixture.graphctl(&command(&manifest, "authority:test", "cycle"));
    assert!(matches!(cycle, Err(EngineError::State(error)) if error.code == "FANOUT_CYCLE"));
    assert!(ledger().py_eq(&before));
    manifest.set("dependencies", Value::List(vec![edge.clone()]));

    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("assessment fault".into()))
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
    let args = command(&manifest, "authority:test", "assess");
    let mut argv = vec!["--repo".into(), fixture.graph.repository.to_str().unwrap().into()];
    argv.extend_from_slice(&args);
    assert!(matches!(
        fixture.execute_with_store(&argv, &store),
        Err(EngineError::Injected(message)) if message == "assessment fault"
    ));
    assert!(ledger().py_eq(&before));

    let assessment = fixture.graphctl(&args).unwrap();
    assert!(assessment["code"].is_text("FANOUT_ASSESSMENT_RECORDED"));
    assert!(assessment["ready_branch_ids"].py_eq(&Value::List(vec![first.clone()])));
    let committed = ledger();
    assert!(committed["fanout"]["status"].is_text("assessed"));
    assert!(committed["fanout"]["assessment_ref"].py_eq(&assessment["assessment_ref"]));
    assert!(committed["dependencies"].py_eq(&Value::List(vec![edge])));
    fixture.with_session(|session, context| {
        let artifact = session.first(GraphRead::Artifact, &[assessment["assessment_ref"].clone()])?.unwrap();
        let payload = compat::loads_value(&artifact["content_json"])?;
        for key in ["authority_ref", "actor", "host_identity", "assessed_at"] {
            assert!(payload[key].py_eq(&committed["fanout"][key]));
        }
        assert!(payload["authority_ref"].is_text("authority:test"));
        assert_eq!(artifact["sha256"].text().unwrap(), ids::sha256_bytes(&ids::canonical_bytes(&payload)?));
        let normalized_members = payload["members"].as_list().unwrap();
        assert!(normalized_members[0]["branch_id"].py_eq(first));
        assert!(normalized_members[1]["branch_id"].py_eq(second));
        for (member, status) in [(first, "ready"), (second, "pending")] {
            let row = session.first(GraphRead::Node, &[member.clone()])?.unwrap();
            assert!(row["status"].is_text(status));
            let envelope = compat::loads_value(&row["envelope_json"])?;
            let input = &envelope["inputs"].as_list().unwrap()[0];
            assert!(input["ref"].py_eq(&artifact["ref"]));
            assert!(input["sha256"].py_eq(&artifact["sha256"]));
            assert!(input["size_bytes"].py_eq(&artifact["size_bytes"]));
        }
        let run = session.run("RUN-1")?.unwrap().fields;
        verify_semantic_state(session, &run, context)
    }).unwrap();
    assert!(fixture.graphctl(&args).unwrap()["code"].is_text("REPLAYED"));
    let changed = fixture.graphctl(&command(&manifest, "authority:changed", "assess"));
    assert!(matches!(changed, Err(EngineError::State(error)) if error.code == "OPERATION_CONFLICT"));
    let duplicate = fixture.graphctl(&command(&manifest, "authority:test", "reassess"));
    assert!(matches!(duplicate, Err(EngineError::State(error)) if error.code == "FANOUT_ASSESSMENT_EXISTS"));
    assert!(ledger().py_eq(&committed));
    let ready = claim(&fixture, "claim-research").unwrap();
    assert!(ready["branch"]["branch_id"].py_eq(first));
    fixture.tear_down();
}

#[test]
fn mapper_rejects_wrong_fence_and_rolls_back_route_artifacts_and_successor() {
    let fixture = InitializedGraph::new();
    fixture.initialize(&fixture.graph.task("delivery", "fast_path", &[]), true).unwrap();
    let claim = claim(&fixture, "claim-mapper").unwrap();
    let branch = &claim["branch"];
    fs::write(fixture.graph.repository.join("docs/evidence.json"), b"{}").unwrap();
    let manifest = impact_manifest(branch, "fast_path", vec!["repo:docs/evidence.json".into()]);
    let args = result_arguments(&fixture, branch, &manifest, "fault-mapper");
    let mut wrong = args.clone();
    let token_index = wrong.iter().position(|arg| arg.equals("--claim-token")).unwrap() + 1;
    wrong[token_index] = "wrong-token".into();
    assert!(matches!(fixture.graphctl(&wrong), Err(EngineError::State(error)) if error.code == "ATTEMPT_FENCE_MISMATCH"));
    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("mapper fault".into()))
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
    let mut argv = vec!["--repo".into(), fixture.graph.repository.to_str().unwrap().into()];
    argv.extend_from_slice(&args);
    assert!(matches!(fixture.execute_with_store(&argv, &store), Err(EngineError::Injected(message)) if message == "mapper fault"));
    fixture.with_session(|session, context| {
        let run = session.run("RUN-1")?.unwrap().fields;
        assert!(run["state_revision"].py_eq(&3_i64.into()));
        assert!(run["selected_route"].is_null());
        let nodes = session.all(GraphRead::RootBranches, &["RUN-1".into()])?;
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0]["status"].is_text("running"));
        assert!(nodes[0]["result_digest"].is_null());
        let attempt = session.first(GraphRead::FenceAttempt, &[
            "RUN-1".into(), branch["branch_id"].clone(), branch["attempt_id"].clone(),
        ])?.unwrap();
        assert!(attempt["finished_at"].is_null());
        assert!(session.operation("RUN-1", "fault-mapper")?.is_none());
        assert!(session.query_values("SELECT 1 FROM events WHERE source_id='fault-mapper'", &[])?.is_empty());
        assert!(session.all(GraphRead::ArtifactsForValidation, &["RUN-1".into()])?.iter()
            .all(|row| !["finding", "impact_map", "branch_result"].iter().any(|kind| row["kind"].is_text(kind))));
        verify_semantic_state(session, &run, context)
    }).unwrap();
    assert!(fixture.graphctl(&args).unwrap()["code"].is_text("BRANCH_RESULT_RECORDED"));
    let mut modified = manifest;
    modified.set("evidence_refs", Value::List(vec![]));
    let changed = result_arguments(&fixture, branch, &modified, "fault-mapper");
    assert!(matches!(fixture.graphctl(&changed), Err(EngineError::State(error)) if error.code == "OPERATION_CONFLICT"));
    fixture.tear_down();
}

#[test]
fn failed_mapper_keeps_route_unselected_and_recovery_offers_retry() {
    let fixture = initialize(true);
    let claim = claim(&fixture, "claim-failure").unwrap();
    let branch = &claim["branch"];
    let evidence = b"{}";
    fs::write(fixture.graph.repository.join("docs/failure.json"), evidence).unwrap();
    let digest = ids::sha256_bytes(evidence);
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", branch["branch_id"].clone()),
        ("status", "failed".into()),
        ("output_kind", "impact_map".into()),
        ("failure_code", "TOOL_FAILURE".into()),
        ("attempt_id", branch["attempt_id"].clone()),
        ("claim_digest", branch["claim_digest"].clone()),
        ("evidence", Value::List(vec![Value::object([
            ("kind", "failure".into()),
            ("ref", format!("repo:docs/failure.json#sha256={digest}").into()),
            ("sha256", digest.into()),
        ])])),
    ]);
    let result = fixture.graphctl(&result_arguments(&fixture, branch, &manifest, "fail-mapper")).unwrap();
    assert!(result["branch_status"].is_text("failed"));
    assert!(result["successor_branch_ids"].as_list().unwrap().is_empty());
    let recovery = fixture.graphctl(&[
        "recovery".into(), "show".into(), "--run-id".into(), "RUN-1".into(),
    ]).unwrap();
    let packets = recovery["packets"].as_list().unwrap();
    assert_eq!(packets.len(), 1);
    assert!(packets[0]["branch_id"].py_eq(&branch["branch_id"]));
    assert!(packets[0]["next_permitted_action"].is_text("retry"));
    fixture.tear_down();
}

fn research_artifact(fixture: &InitializedGraph, kind: &str, label: &str) -> Value {
    let payload = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", kind.into()),
        ("label", label.into()),
    ]);
    let bytes = compat::dumps(&payload, compat::SerializationProfile::STORED).unwrap();
    let relative = format!("docs/artifacts/{label}.json");
    fs::write(fixture.graph.repository.join(&relative), &bytes).unwrap();
    let sha = ids::sha256_bytes(&bytes);
    Value::object([
        ("kind", kind.into()),
        ("ref", format!("repo:{relative}#sha256={sha}").into()),
        ("sha256", sha.into()),
    ])
}

fn ordered_research() -> (InitializedGraph, Value) {
    begin_ordered_research(initialize(true))
}

fn begin_ordered_research(fixture: InitializedGraph) -> (InitializedGraph, Value) {
    let mapper = claim(&fixture, "claim-mapper").unwrap();
    let branch = &mapper["branch"];
    let finding = research_artifact(&fixture, "finding", "impact-evidence");
    let manifest = impact_manifest(branch, "full_delivery", vec![finding["ref"].clone()]);
    let mapped = fixture.graphctl(&result_arguments(&fixture, branch, &manifest, "record-mapper")).unwrap();
    let members = mapped["successor_branch_ids"].as_list().unwrap();
    let fanout = &mapped["created_fanout_ids"].as_list().unwrap()[0];
    let resources = Value::object([
        ("writable_paths", Value::List(vec![])),
        ("mutable_state_refs", Value::List(vec![])),
        ("exclusive_device_refs", Value::List(vec![])),
        ("services", Value::List(vec![])),
    ]);
    let assessment = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", "RUN-1".into()),
        ("fanout_id", fanout.clone()),
        ("members", Value::list(members.iter().map(|branch| Value::object([
            ("branch_id", branch.clone()), ("resources", resources.clone()),
        ])))),
        ("dependencies", Value::List(vec![Value::object([
            ("before_branch_id", members[0].clone()),
            ("after_branch_id", members[1].clone()),
            ("reason", "Review first analysis before second".into()),
        ])])),
        ("evidence", Value::List(vec![finding])),
    ]);
    let path = fixture.graph.store.inbox_root("albanian-live-translate", "RUN-1").join("research-assessment.json");
    fs::write(&path, ids::canonical_bytes(&assessment).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    fixture.graphctl(&[
        "record".into(), "fanout-assessment".into(), "--run-id".into(), "RUN-1".into(),
        "--fanout-id".into(), fanout.as_string().unwrap().clone(), "--assessment-manifest".into(),
        path.to_str().unwrap().into(), "--authority-ref".into(), "authority:test".into(),
        "--op-id".into(), "assess-research".into(),
    ]).unwrap();
    (fixture, mapped)
}

fn research_success(fixture: &InitializedGraph, branch: &Value) -> Value {
    let id = branch["branch_id"].text().unwrap();
    let result = research_artifact(fixture, "evidence_manifest", &id);
    let evidence = research_artifact(fixture, "finding", &format!("{id}-evidence"));
    Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", branch["branch_id"].clone()),
        ("status", "succeeded".into()),
        ("output_kind", "evidence_manifest".into()),
        ("artifact_ref", result),
        ("evidence", Value::List(vec![evidence])),
        ("findings", Value::List(vec![])),
        ("attempt_id", branch["attempt_id"].clone()),
        ("claim_digest", branch["claim_digest"].clone()),
    ])
}

fn research_join_arguments(join: &Value, operation: &str) -> Vec<PyString> {
    vec![
        "join".into(), "advance".into(), "--run-id".into(), "RUN-1".into(),
        "--join-id".into(), join.as_string().unwrap().clone(), "--op-id".into(), operation.into(),
    ]
}

#[test]
fn research_advance_seals_collection_and_creates_exact_tech_lead_inputs() {
    let (fixture, mapped) = ordered_research();
    let join_id = &mapped["created_join_ids"].as_list().unwrap()[0];
    let advance = research_join_arguments(join_id, "seal-research");
    assert!(matches!(
        fixture.graphctl(&advance),
        Err(EngineError::State(error)) if error.code == "NOT_READY"
    ));
    let members = mapped["successor_branch_ids"].as_list().unwrap();
    for (id, claim_id, result_id) in [
        (&members[0], "claim-first", "finish-first"),
        (&members[1], "claim-second", "finish-second"),
    ] {
        let result = claim(&fixture, claim_id).unwrap();
        let branch = &result["branch"];
        assert!(branch["branch_id"].py_eq(id));
        let manifest = research_success(&fixture, branch);
        fixture.graphctl(&result_arguments(&fixture, branch, &manifest, result_id)).unwrap();
    }
    let before = fixture.graphctl(&["status".into(), "--run-id".into(), "RUN-1".into()]).unwrap();
    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("research seal fault".into()))
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
    let mut argv = vec!["--repo".into(), fixture.graph.repository.to_str().unwrap().into()];
    argv.extend_from_slice(&advance);
    assert!(matches!(
        fixture.execute_with_store(&argv, &store),
        Err(EngineError::Injected(message)) if message == "research seal fault"
    ));
    fixture.with_session(|session, context| {
        let run = session.run("RUN-1")?.unwrap().fields;
        let join = session.first(GraphRead::JoinForRun, &[join_id.clone(), "RUN-1".into()])?.unwrap();
        assert!(join["status"].is_text("open"));
        assert!(join["result_json"].is_null());
        assert!(run["state_revision"].py_eq(&before["state_revision"]));
        assert!(session.operation("RUN-1", "seal-research")?.is_none());
        assert!(!session.event_exists("RUN-1", "seal-research")?);
        assert!(session.query_values("SELECT 1 FROM nodes WHERE node_key='tech_lead'", &[])?.is_empty());
        assert!(session.query_values("SELECT 1 FROM artifacts WHERE kind='collection'", &[])?.is_empty());
        verify_semantic_state(session, &run, context)
    }).unwrap();
    let advanced = fixture.graphctl(&advance).unwrap();
    assert!(advanced["code"].is_text("JOIN_ADVANCED"));
    assert!(advanced["outcome"].is_text("RESEARCH_SEALED"));
    assert!(advanced["join_id"].py_eq(join_id));
    assert!(advanced["created_join_ids"].as_list().unwrap().is_empty());
    assert!(advanced["created_fanout_ids"].as_list().unwrap().is_empty());
    let replayed = fixture.graphctl(&advance).unwrap();
    assert!(replayed["code"].is_text("REPLAYED"));
    assert!(replayed["state_revision"].py_eq(&advanced["state_revision"]));
    assert!(matches!(
        fixture.graphctl(&research_join_arguments(&"other-join".into(), "seal-research")),
        Err(EngineError::State(error)) if error.code == "OPERATION_CONFLICT"
    ));
    assert!(matches!(
        fixture.graphctl(&research_join_arguments(join_id, "seal-again")),
        Err(EngineError::State(error)) if error.code == "INVALID_STATE"
    ));

    let claimed = claim(&fixture, "claim-tech-lead").unwrap();
    let lead = &claimed["branch"];
    assert!(lead["node_key"].is_text("tech_lead"));
    assert!(advanced["successor_branch_ids"].py_eq(&Value::List(vec![lead["branch_id"].clone()])));
    let inputs = lead["inputs"].as_list().unwrap();
    assert_eq!(inputs.iter().map(|input| input["kind"].text().unwrap()).collect::<Vec<_>>(),
        ["branch_result", "branch_result", "collection", "evidence_manifest", "evidence_manifest", "task_brief"]);
    let collection = &inputs[2]["content"];
    assert!(collection["kind"].is_text("collection"));
    assert!(collection["join_id"].py_eq(join_id));
    let frozen = collection["members"].as_list().unwrap();
    assert_eq!(frozen.len(), 2);
    let digest = ids::sha256_bytes(&ids::canonical_bytes(collection).unwrap());
    assert!(inputs[2]["sha256"].is_text(&digest));
    fixture.with_session(|session, context| {
        let join = session.first(GraphRead::JoinForRun, &[join_id.clone(), "RUN-1".into()])?.unwrap();
        assert!(join["status"].is_text("sealed"));
        assert!(compat::loads_value(&join["result_json"])?.py_eq(&collection["members"]));
        let artifact = session.first(GraphRead::Artifact, &[inputs[2]["ref"].clone()])?.unwrap();
        assert!(artifact["immutable"].truthy());
        assert!(compat::loads_value(&artifact["content_json"])?.py_eq(collection));
        for (member, id) in frozen.iter().zip(members) {
            let node = session.first(GraphRead::Node, &[id.clone()])?.unwrap();
            assert!(member["branch_id"].py_eq(id));
            assert!(member["result_digest"].py_eq(&node["result_digest"]));
            assert!(member["result"].py_eq(&compat::loads_value(&node["result_json"])?));
        }
        let run = session.run("RUN-1")?.unwrap().fields;
        verify_semantic_state(session, &run, context)
    }).unwrap();
    fixture.tear_down();
}

#[test]
fn exhausted_research_failure_blocks_run_and_leaves_collection_open() {
    let mut fixture = InitializedGraph::new();
    let mut policy = compat::loads(&fixture.graph.policy_bytes).unwrap();
    let mut templates = policy["node_templates"].clone();
    for key in ["design_research_architecture", "design_research_validation"] {
        let mut template = templates[key].clone();
        template.set("max_retries", 0_i64.into());
        templates.set(key, template);
    }
    policy.set("node_templates", templates);
    fixture.graph.policy_bytes = compat::dumps(&policy, compat::SerializationProfile::STORED).unwrap();
    fs::write(fixture.graph.repository.join(".codex/engineering-graph.json"), &fixture.graph.policy_bytes).unwrap();
    fixture.initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true).unwrap();
    let (fixture, mapped) = begin_ordered_research(fixture);
    let claimed = claim(&fixture, "claim-first").unwrap();
    let branch = &claimed["branch"];
    assert!(branch["max_retries"].py_eq(&0_i64.into()));
    let failure = research_artifact(&fixture, "failure", "mandatory-research-failed");
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", branch["branch_id"].clone()),
        ("status", "failed".into()),
        ("output_kind", "evidence_manifest".into()),
        ("failure_code", "TOOL_FAILURE".into()),
        ("evidence", Value::List(vec![failure])),
        ("attempt_id", branch["attempt_id"].clone()),
        ("claim_digest", branch["claim_digest"].clone()),
    ]);
    fixture.graphctl(&result_arguments(&fixture, branch, &manifest, "fail-first")).unwrap();
    let claimed = claim(&fixture, "claim-second").unwrap();
    let branch = &claimed["branch"];
    let manifest = research_success(&fixture, branch);
    fixture.graphctl(&result_arguments(&fixture, branch, &manifest, "finish-second")).unwrap();
    let join_id = &mapped["created_join_ids"].as_list().unwrap()[0];
    let arguments = research_join_arguments(join_id, "advance-exhausted");
    let mut argv = vec!["--repo".into(), fixture.graph.repository.to_str().unwrap().into()];
    argv.extend_from_slice(&arguments);
    match fixture.execute(&argv).unwrap() {
        ExecuteOutcome::Response { result, exit_code } => {
            assert_eq!(exit_code, 3);
            assert!(result["code"].is_text("GRAPH_BLOCKED"));
            assert!(result["reason"].is_text("MANDATORY_RESEARCH_FAILED"));
            assert!(result["outcome"].is_text("BLOCK"));
            assert!(result["successor_branch_ids"].as_list().unwrap().is_empty());
        }
        other => panic!("unexpected parser result: {other:?}"),
    }
    fixture.with_session(|session, context| {
        let run = session.run("RUN-1")?.unwrap().fields;
        assert!(run["status"].is_text("blocked"));
        assert!(run["blocked_reason"].is_text("MANDATORY_RESEARCH_FAILED"));
        assert!(run["finished_at"].truthy());
        let join = session.first(GraphRead::JoinForRun, &[join_id.clone(), "RUN-1".into()])?.unwrap();
        assert!(join["status"].is_text("open"));
        assert!(join["result_json"].is_null());
        assert!(session.query_values("SELECT 1 FROM artifacts WHERE kind='collection'", &[])?.is_empty());
        assert!(session.query_values("SELECT 1 FROM nodes WHERE node_key='tech_lead'", &[])?.is_empty());
        verify_semantic_state(session, &run, context)
    }).unwrap();
    assert!(fixture.graphctl(&arguments).unwrap()["code"].is_text("REPLAYED"));
    fixture.tear_down();
}

#[test]
fn research_results_bind_attempts_and_evidence_and_release_the_ordered_successor() {
    let (fixture, mapped) = ordered_research();
    let ids = mapped["successor_branch_ids"].as_list().unwrap();
    let claimed = claim(&fixture, "claim-first-research").unwrap();
    let first = &claimed["branch"];
    assert!(first["branch_id"].py_eq(&ids[0]));
    let manifest = research_success(&fixture, first);
    let mut empty_evidence = manifest.clone();
    empty_evidence.set("evidence", Value::List(vec![]));
    let rejected = fixture.graphctl(&result_arguments(&fixture, first, &empty_evidence, "empty-evidence"));
    assert!(matches!(rejected, Err(EngineError::Contract(error)) if error.code == "EVIDENCE_REQUIRED"));

    let mut invalid_fence = manifest.clone();
    invalid_fence.set("claim_digest", "0".repeat(64).into());
    let rejected = fixture.graphctl(&result_arguments(&fixture, first, &invalid_fence, "bad-fence"));
    assert!(matches!(rejected, Err(EngineError::State(error)) if error.code == "ATTEMPT_FENCE_MISMATCH"));

    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("research result fault".into()))
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
    let record = result_arguments(&fixture, first, &manifest, "first-research-result");
    let mut argv = vec!["--repo".into(), fixture.graph.repository.to_str().unwrap().into()];
    argv.extend_from_slice(&record);
    assert!(matches!(fixture.execute_with_store(&argv, &store), Err(EngineError::Injected(message)) if message == "research result fault"));
    fixture.with_session(|session, context| {
        let run = session.run("RUN-1")?.unwrap().fields;
        assert!(run["state_revision"].py_eq(&claimed["state_revision"]));
        assert!(session.operation("RUN-1", "first-research-result")?.is_none());
        assert!(!session.event_exists("RUN-1", "first-research-result")?);
        assert!(session.first(GraphRead::Artifact, &[manifest["artifact_ref"]["ref"].clone()])?.is_none());
        let evidence = &manifest["evidence"].as_list().unwrap()[0];
        assert!(session.first(GraphRead::Artifact, &[evidence["ref"].clone()])?.is_none());
        for (id, expected) in [(&ids[0], "running"), (&ids[1], "pending")] {
            let node = session.first(GraphRead::Node, &[id.clone()])?.unwrap();
            assert!(node["status"].is_text(expected));
            assert!(node["result_digest"].is_null());
        }
        let attempt = session.first(GraphRead::FenceAttempt, &[
            "RUN-1".into(), first["branch_id"].clone(), first["attempt_id"].clone(),
        ])?.unwrap();
        assert!(attempt["finished_at"].is_null());
        verify_semantic_state(session, &run, context)
    }).unwrap();

    let result = fixture.graphctl(&record).unwrap();
    assert!(result["code"].is_text("BRANCH_RESULT_RECORDED"));
    assert!(result["branch_status"].is_text("succeeded"));
    assert!(result["promoted_branch_ids"].py_eq(&Value::List(vec![ids[1].clone()])));
    let replay = fixture.graphctl(&record).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&result["state_revision"]));
    fixture.with_session(|session, _| {
        let node = session.first(GraphRead::Node, &[ids[0].clone()])?.unwrap();
        let envelope = compat::loads_value(&node["envelope_json"])?;
        let durable = compat::loads_value(&node["result_json"])?;
        assert!(durable.py_eq(&manifest));
        let digest = ids::sha256_bytes(&ids::canonical_bytes(&durable)?);
        let wrapper = format!("ledger:{}#sha256={digest}", ids[0].text().unwrap());
        assert!(node["result_digest"].is_text(&digest));
        assert!(session.first(GraphRead::Artifact, &[wrapper.into()])?.unwrap()["kind"].is_text("branch_result"));
        assert!(envelope["artifact_ref"].py_eq(&manifest["artifact_ref"]));
        assert!(envelope["evidence"].py_eq(&manifest["evidence"]));
        assert!(envelope.get("claim_token").is_none());
        let attempt = session.first(GraphRead::FenceAttempt, &[
            "RUN-1".into(), first["branch_id"].clone(), first["attempt_id"].clone(),
        ])?.unwrap();
        assert!(attempt["outcome"].is_text("succeeded"));
        assert!(attempt["finished_at"].py_eq(&node["finished_at"]));
        Ok(())
    }).unwrap();

    let claimed = claim(&fixture, "claim-second-research").unwrap();
    let second = &claimed["branch"];
    assert!(second["branch_id"].py_eq(&ids[1]));
    let manifest = research_success(&fixture, second);
    fixture.graphctl(&result_arguments(&fixture, second, &manifest, "second-research-result")).unwrap();
    let join = &mapped["created_join_ids"].as_list().unwrap()[0];
    let ready = fixture.graphctl(&[
        "join".into(), "validate".into(), "--run-id".into(), "RUN-1".into(),
        "--join-id".into(), join.as_string().unwrap().clone(),
    ]).unwrap();
    assert!(ready["code"].is_text("READY"));
    assert!(ready["ok"].truthy());
    assert!(ready["groups"]["succeeded"].py_eq(&Value::List(ids.to_vec())));
    fixture.tear_down();
}

#[test]
fn retryable_research_failure_keeps_dependents_pending_and_returns_recovery_packet() {
    let (fixture, mapped) = ordered_research();
    let claimed = claim(&fixture, "claim-research").unwrap();
    let branch = &claimed["branch"];
    let failure = research_artifact(&fixture, "failure", "research-tool-failure");
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", branch["branch_id"].clone()),
        ("status", "failed".into()),
        ("output_kind", "evidence_manifest".into()),
        ("failure_code", "TOOL_FAILURE".into()),
        ("evidence", Value::List(vec![failure])),
        ("attempt_id", branch["attempt_id"].clone()),
        ("claim_digest", branch["claim_digest"].clone()),
    ]);
    let result = fixture.graphctl(&result_arguments(&fixture, branch, &manifest, "research-failure")).unwrap();
    assert!(result["branch_status"].is_text("failed"));
    assert!(result["promoted_branch_ids"].as_list().unwrap().is_empty());
    let join = &mapped["created_join_ids"].as_list().unwrap()[0];
    let waiting = fixture.graphctl(&[
        "join".into(), "validate".into(), "--run-id".into(), "RUN-1".into(),
        "--join-id".into(), join.as_string().unwrap().clone(),
    ]).unwrap();
    assert!(waiting["code"].is_text("RETRY_REQUIRED"));
    assert!(waiting["groups"]["failed"].py_eq(&Value::List(vec![branch["branch_id"].clone()])));
    assert_eq!(waiting["groups"]["pending"].as_list().unwrap().len(), 1);
    let recovery = fixture.graphctl(&["recovery".into(), "show".into(), "--run-id".into(), "RUN-1".into()]).unwrap();
    let packet = &recovery["packets"].as_list().unwrap()[0];
    assert!(packet["next_permitted_action"].is_text("retry"));
    assert!(packet["branch_id"].py_eq(&branch["branch_id"]));
    fixture.tear_down();
}

fn assert_unchanged(fixture: &InitializedGraph, operation: &str) {
    fixture
        .with_session(|session, context| {
            let run = session.run("RUN-1")?.unwrap().fields;
            assert!(run["state_revision"].py_eq(&2_i64.into()));
            let attempts = session.query_values(
                "SELECT COUNT(*) AS n FROM branch_attempts WHERE run_id=?",
                &["RUN-1".into()],
            )?;
            assert!(attempts[0]["n"].py_eq(&0_i64.into()));
            assert!(session.operation("RUN-1", operation)?.is_none());
            assert!(session
                .query_values(
                    "SELECT 1 FROM events WHERE source_id=?",
                    &[operation.into()]
                )?
                .is_empty());
            let nodes = session
                .query_values("SELECT status FROM nodes WHERE run_id=?", &["RUN-1".into()])?;
            assert!(nodes.iter().all(|node| node["status"].is_text("ready")));
            verify_semantic_state(session, &run, context)
        })
        .unwrap();
}

#[test]
fn cli_claim_records_a_fenced_attempt_and_commits_both_semantic_checks() {
    let fixture = initialize(true);
    let result = claim(&fixture, "claim-real").unwrap();
    assert!(result["code"].is_text("CLAIMED"));
    assert!(result["state_revision"].py_eq(&3_i64.into()));
    let branch = &result["branch"];
    assert!(branch["status"].is_text("running"));
    let attempt = branch["attempt_id"].text().unwrap();
    let token = branch["claim_token"].text().unwrap();
    assert!(attempt.starts_with("attempt-"));
    assert_eq!(attempt.len(), 32);
    assert!(token.starts_with("claim-"));
    assert_eq!(token.len(), 49);
    assert!(branch["claim_digest"].is_text(&ids::sha256_bytes(token.as_bytes())));
    fixture
        .with_session(|session, context| {
            let rows = session.query_values(
                "SELECT * FROM branch_attempts WHERE run_id=?",
                &["RUN-1".into()],
            )?;
            assert_eq!(rows.len(), 1);
            assert!(rows[0]["attempt_id"].container_eq(&branch["attempt_id"]));
            assert!(rows[0]["claim_digest"].container_eq(&branch["claim_digest"]));
            assert!(rows[0]["attempt_number"].py_eq(&1_i64.into()));
            let node = session
                .query_values(
                    "SELECT * FROM nodes WHERE branch_id=?",
                    &[branch["branch_id"].clone()],
                )?
                .remove(0);
            let stored = compat::loads_value(&node["envelope_json"])?;
            assert!(stored.get("claim_token").is_none());
            assert!(stored["claim_digest"].container_eq(&branch["claim_digest"]));
            assert!(node["started_at"].container_eq(&branch["started_at"]));
            let run = session.run("RUN-1")?.unwrap().fields;
            verify_semantic_state(session, &run, context)
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn replay_after_abort_preserves_the_original_claim_token_and_revision() {
    let fixture = initialize(true);
    let first = claim(&fixture, "claim-before-abort").unwrap();
    fixture
        .graphctl(&[
            "abort".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--reason-code".into(),
            "STOP".into(),
            "--authority-ref".into(),
            "authority:test".into(),
            "--op-id".into(),
            "abort-after-claim".into(),
        ])
        .unwrap();
    let replay = fixture
        .graphctl(&[
            "next".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--all".into(),
            "--claim".into(),
            "--op-id".into(),
            "claim-before-abort".into(),
        ])
        .unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].container_eq(&first["state_revision"]));
    assert!(replay["branch"].container_eq(&first["branch"]));
    let late = claim(&fixture, "claim-after-abort");
    assert!(matches!(late, Err(EngineError::State(error)) if error.code == "TERMINAL_RUN"));
    fixture
        .with_session(|session, _| {
            assert_eq!(
                session
                    .query_values(
                        "SELECT * FROM branch_attempts WHERE run_id=?",
                        &["RUN-1".into()]
                    )?
                    .len(),
                1
            );
            assert!(session.run("RUN-1")?.unwrap().fields["state_revision"].py_eq(&4_i64.into()));
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn replay_precedes_fresh_task_validation_but_a_later_operation_rechecks() {
    let fixture = initialize(true);
    let first = claim(&fixture, "claim-before-task-edit").unwrap();
    let path = fixture.graph.repository.join("docs/task.json");
    let bytes = fs::read(&path).unwrap();
    let mut changed = bytes.clone();
    changed.push(b' ');
    fs::write(&path, changed).unwrap();
    let replay = claim(&fixture, "claim-before-task-edit").unwrap();
    let late = claim(&fixture, "claim-after-task-edit");
    fs::write(path, bytes).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["branch"].container_eq(&first["branch"]));
    assert!(
        matches!(late, Err(EngineError::State(error)) if error.code == "INPUT_DIGEST_MISMATCH")
    );
    fixture.tear_down();
}

#[test]
fn unapproved_plan_cannot_be_claimed_or_create_an_attempt() {
    let fixture = initialize(false);
    let result = claim(&fixture, "claim-unapproved");
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "EXECUTION_PLAN_APPROVAL_REQUIRED")
    );
    fixture
        .with_session(|session, _| {
            assert!(session
                .query_values(
                    "SELECT 1 FROM branch_attempts WHERE run_id=?",
                    &["RUN-1".into()]
                )?
                .is_empty());
            assert!(session.run("RUN-1")?.unwrap().fields["state_revision"].py_eq(&1_i64.into()));
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}

#[test]
fn another_claim_fails_when_no_ready_branch_remains() {
    let fixture = initialize(true);
    claim(&fixture, "first-claim").unwrap();
    let result = claim(&fixture, "second-claim");
    assert!(matches!(result, Err(EngineError::State(error)) if error.code == "CLAIM_CONFLICT"));
    fixture
        .with_session(|session, _| {
            assert_eq!(
                session
                    .query_values(
                        "SELECT * FROM branch_attempts WHERE run_id=?",
                        &["RUN-1".into()]
                    )?
                    .len(),
                1
            );
            assert!(session.operation("RUN-1", "second-claim")?.is_none());
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}

struct BeforeCommit {
    observed: Cell<bool>,
    tamper: Option<PathBuf>,
}
impl FaultHook for BeforeCommit {
    fn observe(&self, stage: &str) -> EngineResult<()> {
        if stage == "before_commit" {
            self.observed.set(true);
            if let Some(path) = &self.tamper {
                let mut bytes = fs::read(path)?;
                bytes.push(b' ');
                fs::write(path, bytes)?;
            } else {
                return Err(EngineError::Injected("claim-before-metadata".into()));
            }
        }
        Ok(())
    }
}

fn faulted_claim(
    fixture: &InitializedGraph,
    fault: &BeforeCommit,
    operation: &str,
) -> EngineResult<ExecuteOutcome> {
    let original = &fixture.graph.store;
    let store = StateStore::at_root(
        original.state_root.clone(),
        original.platform,
        original.database,
        fault,
    )?;
    let argv: Vec<PyString> = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
        "next".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--claim".into(),
        "--op-id".into(),
        operation.into(),
    ];
    fixture.execute_with_store(&argv, &store)
}

#[test]
fn pre_metadata_hook_failure_rolls_back_attempt_node_operation_and_event() {
    let fixture = initialize(true);
    let fault = BeforeCommit {
        observed: Cell::new(false),
        tamper: None,
    };
    let result = faulted_claim(&fixture, &fault, "claim-hook-failure");
    assert!(
        matches!(result, Err(EngineError::Injected(message)) if message == "claim-before-metadata")
    );
    assert!(fault.observed.get());
    assert_unchanged(&fixture, "claim-hook-failure");
    fixture.tear_down();
}

#[test]
fn post_write_semantic_failure_rolls_back_all_claim_database_writes() {
    let fixture = initialize(true);
    let path = fixture.graph.repository.join("docs/task.json");
    let bytes = fs::read(&path).unwrap();
    let fault = BeforeCommit {
        observed: Cell::new(false),
        tamper: Some(path.clone()),
    };
    let result = faulted_claim(&fixture, &fault, "claim-after-invalid-input");
    fs::write(path, bytes).unwrap();
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "INPUT_DIGEST_MISMATCH")
    );
    assert!(fault.observed.get());
    assert_unchanged(&fixture, "claim-after-invalid-input");
    fixture.tear_down();
}
