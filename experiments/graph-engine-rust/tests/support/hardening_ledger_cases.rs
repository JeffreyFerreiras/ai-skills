use super::{
    compat, database, initialize, lookup, row, state_failure, stored_json, EngineError,
    HardeningCase, Value, RUN,
};
use graph_engine::domain::{ids, task_contract};

fn branch_id(branch: &Value) -> String {
    branch["branch_id"].text().unwrap()
}

fn claim(case: &mut HardeningCase) -> Value {
    case.claim()
}

fn mapped(case: &mut HardeningCase, tags: &[&str]) {
    initialize(case, tags);
    let mapper = claim(case);
    case.record_mapper(&mapper, tags);
}

fn tech_result(case: &mut HardeningCase) {
    let tech = claim(case);
    case.success(&tech);
}

fn revise_operation(
    case: &HardeningCase,
    select: impl Fn(&Value) -> bool,
    edit: impl FnOnce(&mut Value),
) {
    let session = database(case);
    let operations = session
        .query_values("SELECT operation_id,response_json FROM operations", &[])
        .unwrap();
    let (operation, mut response) = operations
        .iter()
        .find_map(|row| {
            let response = compat::loads_value(&row["response_json"]).unwrap();
            select(&response).then(|| (row["operation_id"].clone(), response))
        })
        .unwrap();
    edit(&mut response);
    session
        .execute_values(
            "UPDATE operations SET response_json=? WHERE operation_id=?",
            &[stored_json(&response), operation],
        )
        .unwrap();
}

fn revision(case: &HardeningCase) -> Value {
    row(&database(case), "SELECT state_revision FROM runs", &[])["state_revision"].clone()
}

fn rejected_claim(case: &HardeningCase, errors: &[&str], operation: &str) {
    let before = revision(case);
    state_failure(
        case.run(&["next", "--run-id", RUN, "--claim", "--op-id", operation]),
        errors,
    );
    assert!(revision(case).py_eq(&before));
    assert!(database(case)
        .query_values(
            "SELECT 1 FROM operations WHERE operation_id=?",
            &[operation.into()],
        )
        .unwrap()
        .is_empty());
}

fn plan_approval(
    case: &HardeningCase,
    initialized: &Value,
    decision: &str,
    operation: &str,
) -> Value {
    case.run(&[
        "record",
        "plan-approval",
        "--run-id",
        RUN,
        "--plan-digest",
        &initialized["execution_plan_digest"].text().unwrap(),
        "--decision",
        decision,
        "--authority-ref",
        "authority:test",
        "--op-id",
        operation,
    ])
    .unwrap()
}

#[test]
fn test_heartbeat_and_expired_lease_are_visible() {
    let mut case = HardeningCase::new();
    initialize(&case, &[]);
    let branch = claim(&mut case);
    let heartbeat = case
        .run(&[
            "record",
            "heartbeat",
            "--run-id",
            RUN,
            "--branch-id",
            &branch_id(&branch),
            "--attempt-id",
            &branch["attempt_id"].text().unwrap(),
            "--claim-token",
            &branch["claim_token"].text().unwrap(),
            "--op-id",
            "heartbeat-1",
        ])
        .unwrap();
    assert!(heartbeat["code"].is_text("HEARTBEAT_RECORDED"));
    {
        let session = database(&case);
        let mut envelope = compat::loads_value(
            &row(
                &session,
                "SELECT envelope_json FROM nodes WHERE branch_id=?",
                &[branch["branch_id"].clone()],
            )["envelope_json"],
        )
        .unwrap();
        envelope.set("lease_expires_at", "2000-01-01T00:00:00Z".into());
        session
            .execute_values(
                "UPDATE nodes SET envelope_json=? WHERE branch_id=?",
                &[stored_json(&envelope), branch["branch_id"].clone()],
            )
            .unwrap();
    }
    assert!(case.status().unwrap()["next_action"]["kind"].is_text("timeout_expired"));
}

#[test]
fn test_execution_plan_is_visible_and_blocks_claim_until_human_approval() {
    let mut case = HardeningCase::new();
    let graph = &case.fixture.graph;
    let path = graph.repository.join("docs").join("task.json");
    super::hardening_case::write_json(&path, &graph.task("delivery", "full_delivery", &[]));
    let initialized = case
        .run(&[
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
            "init",
            "--run-id",
            RUN,
            "--task-brief",
            path.to_str().unwrap(),
            "--op-id",
            "init-1",
            "--size",
            "small",
        ])
        .unwrap();
    let plan = &initialized["execution_plan"];
    assert!(plan["size"].is_text("small"));
    assert!(plan["approval_required"].truthy());
    let senior = lookup(&plan["assignments"], "node_key", &"senior_engineer".into());
    assert!(senior["model"].is_text("gpt-6-astra"));
    assert!(senior["reasoning_effort"].is_text("medium"));
    assert!(case.run(&["next", "--run-id", RUN]).unwrap()["code"]
        .is_text("EXECUTION_PLAN_APPROVAL_REQUIRED"));
    state_failure(case.claim_raw(), &["EXECUTION_PLAN_APPROVAL_REQUIRED"]);
    plan_approval(&case, &initialized, "APPROVE", "plan-approval-1");
    let branch = claim(&mut case);
    assert!(branch["model"].is_text("gpt-6.1-sol"));
    assert!(branch["reasoning_effort"].is_text("low"));
}

#[test]
fn test_rejected_execution_plan_blocks_the_run() {
    let mut case = HardeningCase::new();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    let initialized = case.fixture.initialize(&task, false).unwrap();
    let result = plan_approval(&case, &initialized, "REJECT", "plan-rejection-1");
    assert!(result["status"].is_text("blocked"));
    assert!(case.status().unwrap()["execution_plan"]["status"].is_text("rejected"));
    state_failure(case.claim_raw(), &["GRAPH_BLOCKED"]);
}

#[test]
fn test_critical_delivery_is_forced_through_security_full_delivery() {
    let case = HardeningCase::new();
    let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
    task.set("risk_level", "critical".into());
    case.fixture.with_context(|context| {
        let validate = |task: &Value| -> super::EngineResult<Value> {
            Ok(task_contract::validate_task_brief(
                task, &context.current_policy_digest.into(), context.policy, Some(&1_i64.into()),
                context.files.windows(),
            )?)
        };
        assert!(matches!(validate(&task), Err(EngineError::Contract(error)) if error.code == "CRITICAL_REQUIRES_FULL_DELIVERY"));
        task.set("minimum_route", "full_delivery".into());
        assert!(matches!(validate(&task), Err(EngineError::Contract(error)) if error.code == "CRITICAL_REQUIRES_SECURITY_REVIEW"));
        task.set("mandatory_impact_tags", Value::list(["security_privacy".into()]));
        validate(&task)?;
        Ok(())
    }).unwrap();
}

#[test]
fn test_approval_records_local_actor_attestation() {
    let mut case = HardeningCase::new();
    let mut task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    task.set("required_human_decisions", Value::list(["human-1".into()]));
    case.fixture.initialize(&task, true).unwrap();
    let mapper = claim(&mut case);
    case.record_mapper(&mapper, &[]);
    let scope = case.repo_artifact("acceptance_evidence", "approval-scope");
    let reference = scope["ref"].text().unwrap();
    let digest = scope["sha256"].text().unwrap();
    case.run(&[
        "record",
        "acceptance-evidence",
        "--run-id",
        RUN,
        "--criterion-id",
        "AC-001",
        "--artifact-ref",
        &reference,
        "--artifact-sha256",
        &digest,
        "--op-id",
        "acceptance-1",
    ])
    .unwrap();
    case.run(&[
        "record",
        "approval",
        "--run-id",
        RUN,
        "--approval-id",
        "human-1",
        "--scope-ref",
        &reference,
        "--decision",
        "APPROVE",
        "--authority-ref",
        "authority:test",
        "--artifact-sha256",
        &digest,
        "--op-id",
        "approval-1",
    ])
    .unwrap();
    {
        let db = database(&case);
        assert!(!db
            .query_values(
                "SELECT 1 FROM approval_attestations WHERE approval_id='human-1'",
                &[]
            )
            .unwrap()
            .is_empty());
        db.execute_values(
            "DELETE FROM approval_attestations WHERE approval_id='human-1'",
            &[],
        )
        .unwrap();
    }
    state_failure(case.status(), &["APPROVAL_ATTESTATION_INVALID"]);
}

#[test]
fn test_whole_join_deletion_is_detected_from_operation_history() {
    let mut case = HardeningCase::new();
    mapped(&mut case, &["security_privacy"]);
    tech_result(&mut case);
    let join = case.open_join("design_inputs");
    let db = database(&case);
    let id = std::slice::from_ref(&join["join_id"]);
    db.execute_values("BEGIN", &[]).unwrap();
    db.execute_values("DELETE FROM join_members WHERE join_id=?", id)
        .unwrap();
    db.execute_values("DELETE FROM joins WHERE join_id=?", id)
        .unwrap();
    db.execute_values("COMMIT", &[]).unwrap();
    drop(db);
    rejected_claim(&case, &["TOPOLOGY_JOIN_INVALID"], "deleted-join-claim");
}

#[test]
fn test_whole_successor_deletion_is_detected_from_operation_history() {
    let mut case = HardeningCase::new();
    mapped(&mut case, &[]);
    let status = case.status().unwrap();
    let node = lookup(
        &status["branches"],
        "node_key",
        &"design_research_architecture".into(),
    );
    let db = database(&case);
    db.execute_values("PRAGMA foreign_keys=OFF", &[]).unwrap();
    db.execute_values(
        "DELETE FROM nodes WHERE branch_id=?",
        &[node["branch_id"].clone()],
    )
    .unwrap();
    drop(db);
    rejected_claim(
        &case,
        &["TOPOLOGY_NODE_INVALID", "JOIN_MEMBERSHIP_INVALID"],
        "deleted-successor-claim",
    );
}

fn sealed_research(case: &mut HardeningCase) -> (Value, Value) {
    initialize(case, &[]);
    let mapper = case.claim_raw().unwrap();
    case.record_mapper(&mapper, &[]);
    let status = case.status().unwrap();
    let research = lookup(&status["fanouts"], "stage", &"research".into());
    case.assess(&research["fanout_id"].text().unwrap());
    for _ in 0..2 {
        let branch = case.claim_raw().unwrap();
        case.success(&branch);
    }
    let sealed = case.advance("research_collection");
    let status = case.status().unwrap();
    let tech = lookup(&status["branches"], "node_key", &"tech_lead".into()).clone();
    (sealed, tech)
}

#[test]
fn test_research_collection_witness_requires_exact_tech_lead_successor() {
    let mut case = HardeningCase::new();
    let (sealed, _) = sealed_research(&mut case);
    revise_operation(
        &case,
        |response| response["join_id"].py_eq(&sealed["join_id"]),
        |response| response.set("successor_branch_ids", Value::list([])),
    );
    rejected_claim(
        &case,
        &["TOPOLOGY_HISTORY_INVALID"],
        "missing-research-successor",
    );
}

#[test]
fn test_research_tech_lead_successor_cannot_appear_on_unrelated_operation() {
    let mut case = HardeningCase::new();
    let (_, tech) = sealed_research(&mut case);
    revise_operation(
        &case,
        |response| {
            response["code"].is_text("BRANCH_RESULT_RECORDED") && response["branch_id"].truthy()
        },
        |response| {
            let mut ids = response["successor_branch_ids"].as_list().unwrap().to_vec();
            ids.push(tech["branch_id"].clone());
            response.set("successor_branch_ids", Value::List(ids));
        },
    );
    rejected_claim(
        &case,
        &["TOPOLOGY_HISTORY_INVALID"],
        "unrelated-research-successor",
    );
}

#[test]
fn test_extra_node_and_join_fail_before_mutation() {
    {
        let case = HardeningCase::new();
        initialize(&case, &[]);
        let db = database(&case);
        let source = row(
            &db,
            "SELECT * FROM nodes WHERE node_key='impact_mapper'",
            &[],
        );
        let mut envelope = compat::loads_value(&source["envelope_json"]).unwrap();
        envelope.set("branch_id", "extra-branch".into());
        envelope.set("node_instance_id", "extra-node".into());
        envelope.set("generation", 1_i64.into());
        db.execute_values(
            "INSERT INTO nodes(branch_id,run_id,node_instance_id,node_key,role,stage,generation,mandatory,specialist_tag,status,retry_count,max_retries,envelope_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
            &[
                "extra-branch".into(), RUN.into(), "extra-node".into(), source["node_key"].clone(),
                source["role"].clone(), source["stage"].clone(), 1_i64.into(), source["mandatory"].clone(),
                Value::Null, "ready".into(), 0_i64.into(), source["max_retries"].clone(), stored_json(&envelope),
            ],
        ).unwrap();
        drop(db);
        rejected_claim(
            &case,
            &["STABLE_ID_INVALID", "TOPOLOGY_NODE_INVALID"],
            "extra-node-claim",
        );
    }
    let mut case = HardeningCase::new();
    mapped(&mut case, &[]);
    tech_result(&mut case);
    let db = database(&case);
    let tech = row(&db, "SELECT * FROM nodes WHERE node_key='tech_lead'", &[]);
    db.execute_values("BEGIN", &[]).unwrap();
    db.execute_values(
        "INSERT INTO joins(join_id,run_id,join_key,kind,stage,generation,status,degraded) VALUES(?,?,?,?,?,?,?,0)",
        &["extra-join".into(), RUN.into(), "closure".into(), "closure".into(), "closure".into(), 99_i64.into(), "open".into()],
    ).unwrap();
    db.execute_values(
        "INSERT INTO join_members VALUES(?,?,1)",
        &["extra-join".into(), tech["branch_id"].clone()],
    )
    .unwrap();
    db.execute_values("COMMIT", &[]).unwrap();
    drop(db);
    rejected_claim(
        &case,
        &["STABLE_ID_INVALID", "JOIN_MEMBERSHIP_INVALID"],
        "extra-join-claim",
    );
}

fn design_fanout(case: &mut HardeningCase, tags: &[&str]) -> Value {
    mapped(case, tags);
    tech_result(case);
    case.advance("design_inputs");
    let status = case.status().unwrap();
    lookup(&status["fanouts"], "stage", &"design".into()).clone()
}

#[test]
fn test_removed_and_added_join_member_edges_fail_closed() {
    let tags = ["audio_realtime_translation", "security_privacy"];
    {
        let mut case = HardeningCase::new();
        let fanout = design_fanout(&mut case, &tags);
        let db = database(&case);
        let collection = row(
            &db,
            "SELECT * FROM joins WHERE join_key='design_collection'",
            &[],
        );
        db.execute_values(
            "DELETE FROM join_members WHERE join_id=? AND branch_id=?",
            &[
                collection["join_id"].clone(),
                fanout["member_branch_ids"].as_list().unwrap()[0].clone(),
            ],
        )
        .unwrap();
        drop(db);
        rejected_claim(&case, &["JOIN_MEMBERSHIP_INVALID"], "removed-member-claim");
    }
    let mut case = HardeningCase::new();
    design_fanout(&mut case, &tags);
    let db = database(&case);
    let collection = row(
        &db,
        "SELECT * FROM joins WHERE join_key='design_collection'",
        &[],
    );
    let producer = row(&db, "SELECT * FROM nodes WHERE node_key='tech_lead'", &[]);
    db.execute_values(
        "INSERT INTO join_members VALUES(?,?,1)",
        &[collection["join_id"].clone(), producer["branch_id"].clone()],
    )
    .unwrap();
    drop(db);
    rejected_claim(&case, &["JOIN_MEMBERSHIP_INVALID"], "added-member-claim");
}

fn dependency(before: &Value, after: &Value, reason: &str) -> Value {
    Value::object([
        ("before_branch_id", before.clone()),
        ("after_branch_id", after.clone()),
        ("reason", reason.into()),
    ])
}

fn record_assessment(
    case: &mut HardeningCase,
    fanout: &Value,
    dependencies: Vec<Value>,
    operation: &str,
) {
    let evidence = case.repo_artifact(
        "finding",
        &format!("fanout-resource-proof-{}", case.counter()),
    );
    let ids = fanout["member_branch_ids"].as_list().unwrap();
    let members = Value::list(ids.iter().rev().enumerate().map(|(index, branch_id)| {
        let resources = Value::object([
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
                    ("capacity", Value::from(ids.len())),
                ])]),
            ),
        ]);
        Value::object([("branch_id", branch_id.clone()), ("resources", resources)])
    }));
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", RUN.into()),
        ("fanout_id", fanout["fanout_id"].clone()),
        ("members", members),
        ("dependencies", Value::List(dependencies)),
        ("evidence", Value::list([evidence])),
    ]);
    let path = case.inbox(&manifest);
    case.run(&[
        "record",
        "fanout-assessment",
        "--run-id",
        RUN,
        "--fanout-id",
        &fanout["fanout_id"].text().unwrap(),
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
fn test_assessment_envelope_and_dependency_mismatches_fail_closed() {
    {
        let mut case = HardeningCase::new();
        let fanout = design_fanout(&mut case, &["security_privacy"]);
        record_assessment(&mut case, &fanout, vec![], "envelope-assessment");
        let db = database(&case);
        let assessed = row(
            &db,
            "SELECT * FROM fanouts WHERE fanout_id=?",
            std::slice::from_ref(&fanout["fanout_id"]),
        );
        let branch = &fanout["member_branch_ids"].as_list().unwrap()[0];
        let mut envelope = compat::loads_value(
            &row(
                &db,
                "SELECT envelope_json FROM nodes WHERE branch_id=?",
                std::slice::from_ref(branch),
            )["envelope_json"],
        )
        .unwrap();
        envelope.set(
            "inputs",
            Value::list(
                envelope["inputs"]
                    .as_list()
                    .unwrap()
                    .iter()
                    .filter(|item| !item["ref"].py_eq(&assessed["assessment_ref"]))
                    .cloned(),
            ),
        );
        db.execute_values(
            "UPDATE nodes SET envelope_json=? WHERE branch_id=?",
            &[stored_json(&envelope), branch.clone()],
        )
        .unwrap();
        drop(db);
        rejected_claim(
            &case,
            &["FANOUT_ASSESSMENT_INVALID"],
            "envelope-mismatch-claim",
        );
    }
    {
        let mut case = HardeningCase::new();
        let fanout = design_fanout(&mut case, &["security_privacy"]);
        record_assessment(&mut case, &fanout, vec![], "provenance-assessment");
        let db = database(&case);
        db.execute_values("DROP TRIGGER fanout_transition_guard", &[])
            .unwrap();
        db.execute_values(
            "UPDATE fanouts SET actor='tampered-actor' WHERE fanout_id=?",
            std::slice::from_ref(&fanout["fanout_id"]),
        )
        .unwrap();
        drop(db);
        rejected_claim(
            &case,
            &["FANOUT_ASSESSMENT_INVALID"],
            "provenance-mismatch-claim",
        );
    }
    let mut case = HardeningCase::new();
    let fanout = design_fanout(&mut case, &["security_privacy"]);
    let ids = fanout["member_branch_ids"].as_list().unwrap();
    assert_eq!(ids.len(), 2);
    record_assessment(
        &mut case,
        &fanout,
        vec![dependency(&ids[0], &ids[1], "ordered")],
        "dependency-assessment",
    );
    let db = database(&case);
    db.execute_values("DROP TRIGGER fanout_dependency_delete_guard", &[])
        .unwrap();
    db.execute_values(
        "DELETE FROM fanout_dependencies WHERE fanout_id=?",
        std::slice::from_ref(&fanout["fanout_id"]),
    )
    .unwrap();
    drop(db);
    rejected_claim(
        &case,
        &["FANOUT_DEPENDENCY_STATE_INVALID"],
        "dependency-mismatch-claim",
    );
}

#[test]
fn test_fanout_assessment_gates_claim_and_orders_promotion() {
    let mut case = HardeningCase::new();
    mapped(&mut case, &["security_privacy"]);
    tech_result(&mut case);
    case.advance("design_inputs");
    let required = case.run(&["next", "--run-id", RUN]).unwrap();
    assert!(required["code"].is_text("FANOUT_ASSESSMENT_REQUIRED"));
    let status = case.status().unwrap();
    let fanout = lookup(&status["fanouts"], "stage", &"design".into());
    let ids = fanout["member_branch_ids"].as_list().unwrap();
    assert_eq!(ids.len(), 2);
    let (before, after) = (&ids[0], &ids[1]);
    let result = case.assess_ordered(
        &fanout["fanout_id"].text().unwrap(),
        vec![dependency(before, after, "shared review state")],
    );
    assert!(result["ready_branch_ids"].py_eq(&Value::list([before.clone()])));
    let status = case.status().unwrap();
    assert!(lookup(&status["branches"], "branch_id", before)["status"].is_text("ready"));
    assert!(lookup(&status["branches"], "branch_id", after)["status"].is_text("pending"));
    let first = claim(&mut case);
    let token = first["claim_token"].text().unwrap();
    let evidence = case.repo_artifact("timeout", &format!("timeout-evidence-{}", case.counter()));
    let timeout = case.inbox(&Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "timeout".into()),
        ("run_id", RUN.into()),
        ("reason_code", "WORKER_TIMEOUT".into()),
        ("evidence", Value::list([evidence])),
        ("branch_id", first["branch_id"].clone()),
        ("attempt_id", first["attempt_id"].clone()),
        ("claim_digest", ids::sha256_bytes(token.as_bytes()).into()),
    ]));
    case.run(&[
        "record",
        "timeout",
        "--run-id",
        RUN,
        "--branch-id",
        &before.text().unwrap(),
        "--attempt-id",
        &first["attempt_id"].text().unwrap(),
        "--claim-token",
        &token,
        "--reason-code",
        "WORKER_TIMEOUT",
        "--evidence-manifest",
        timeout.to_str().unwrap(),
        "--op-id",
        "ordered-timeout",
    ])
    .unwrap();
    assert!(
        lookup(&case.status().unwrap()["branches"], "branch_id", after)["status"]
            .is_text("pending")
    );
    case.run(&[
        "record",
        "retry",
        "--run-id",
        RUN,
        "--branch-id",
        &before.text().unwrap(),
        "--reason-code",
        "RETRY",
        "--op-id",
        "ordered-retry",
    ])
    .unwrap();
    let branch = claim(&mut case);
    case.success(&branch);
    assert!(
        lookup(&case.status().unwrap()["branches"], "branch_id", after)["status"].is_text("ready")
    );
}

#[test]
fn test_retained_schemas_fail_closed_without_mutation() {
    let case = HardeningCase::new();
    initialize(&case, &[]);
    for version in [2_i64, 3, 4] {
        let db = database(&case);
        db.execute_values("UPDATE runs SET state_schema_version=?", &[version.into()])
            .unwrap();
        let revision = row(&db, "SELECT state_revision FROM runs", &[])["state_revision"].clone();
        let count = row(&db, "SELECT COUNT(*) AS count FROM operations", &[])["count"].clone();
        drop(db);
        state_failure(
            case.run(&[
                "next",
                "--run-id",
                RUN,
                "--claim",
                "--op-id",
                &format!("unsupported-schema-{version}"),
            ]),
            &["UNSUPPORTED_STATE_SCHEMA"],
        );
        let db = database(&case);
        let run = row(
            &db,
            "SELECT state_schema_version,state_revision FROM runs",
            &[],
        );
        assert_eq!(run["state_schema_version"].as_i64(), Some(version));
        assert!(run["state_revision"].py_eq(&revision));
        assert!(row(&db, "SELECT COUNT(*) AS count FROM operations", &[])["count"].py_eq(&count));
    }
}

#[test]
fn test_status_returns_incomplete_and_completed_attempt_timing() {
    let mut case = HardeningCase::new();
    initialize(&case, &[]);
    let mapper = claim(&mut case);
    let running = case.status().unwrap();
    let branch = lookup(&running["branches"], "branch_id", &mapper["branch_id"]);
    assert_eq!(branch["attempt_count"].as_i64(), Some(1));
    assert!(!branch["timing_complete"].truthy());
    assert!(branch["wall_time_ms"].is_null());
    assert!(branch["active_duration_ms"].is_null());
    assert!(!running["timing"]["overall"]["timing_complete"].truthy());
    assert!(running["timing"]["overall"]["critical_path"].is_null());
    case.record_mapper(&mapper, &[]);
    let completed = case.status().unwrap();
    let branch = lookup(&completed["branches"], "branch_id", &mapper["branch_id"]);
    assert!(branch["timing_complete"].truthy());
    assert_eq!(branch["attempt_count"].as_i64(), Some(1));
    assert!(!branch["active_duration_ms"].is_null());
    let bootstrap = lookup(&completed["timing"]["stages"], "stage", &"bootstrap".into());
    assert!(bootstrap["timing_complete"].truthy());
}

#[test]
fn test_reversed_attempt_timestamp_fails_semantic_status_without_mutation() {
    let mut case = HardeningCase::new();
    initialize(&case, &[]);
    let mapper = claim(&mut case);
    case.record_mapper(&mapper, &[]);
    let db = database(&case);
    db.execute_values("DROP TRIGGER branch_attempt_identity_guard", &[])
        .unwrap();
    db.execute_values(
        "UPDATE branch_attempts SET finished_at='2000-01-01T00:00:00Z' WHERE branch_id=?",
        std::slice::from_ref(&mapper["branch_id"]),
    )
    .unwrap();
    drop(db);
    let before = revision(&case);
    state_failure(case.status(), &["ATTEMPT_TIMESTAMP_INVALID"]);
    assert!(revision(&case).py_eq(&before));
}
