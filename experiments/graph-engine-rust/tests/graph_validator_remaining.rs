#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[allow(dead_code)]
#[path = "support/planner_case.rs"]
mod planner_case;

use graph_engine::{
    application::{
        error::EngineError,
        execution_validation::validate_execution_plan,
        graph_fanout,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        semantic_state::{verify_resume, verify_semantic_state},
        state::GraphSession,
    },
    compat::{self, SerializationProfile, Value},
    domain::{execution::reconstruct_execution_plan, ids, policy::capability},
};
use hardening_case::HardeningCase;
use planner_case::{
    approve_plan, contract_failure, initialize, initialize_historical, resume, task_v2,
    HistoricalPlan,
};
use std::fs;

#[test]
fn test_new_codex_and_cursor_writer_assignment_survives_resume_and_claim() {
    for host in ["codex", "cursor"] {
        let mut case = HardeningCase::stock();
        let task = case.fixture.graph.task("delivery", "fast_path", &[]);
        let initialized = initialize(&case, &task, Some(host), Some("small"), true).unwrap();
        resume(&case).unwrap();
        case.impact("fast_path");
        let writer = case.claim();
        assert!(writer["node_key"].is_text("senior_engineer"));
        assert!(writer["reasoning_effort"].is_text("medium"));
        assert!(writer["simplification_review_version"].py_eq(&1_i64.into()));
        assert!(["gpt-6.1-sol", "grok-4.7"]
            .iter()
            .any(|model| writer["model"].is_text(model)));
        let status = case.status().unwrap();
        assert!(
            status["execution_plan"]["plan_digest"].py_eq(&initialized["execution_plan_digest"])
        );
        assert!(status["execution_plan"]["status"].is_text("approved"));
    }
}

fn stored_plan_row(case: &HardeningCase) -> Value {
    case.fixture
        .with_session(|session, _| {
            Ok(session
                .first(GraphRead::ExecutionPlan, &["RUN-1".into()])?
                .unwrap())
        })
        .unwrap()
}

#[test]
fn test_codex_and_cursor_legacy_writer_approval_survives_resume_and_claim() {
    for host in ["codex", "cursor"] {
        let mut case = HardeningCase::stock();
        let task = case.fixture.graph.task("delivery", "fast_path", &[]);
        let hint = Value::object([("host", host.into())]);
        let legacy = reconstruct_execution_plan("RUN-1", &task, &hint, Some("small")).unwrap();
        initialize_historical(
            &case,
            &task,
            host,
            Some("small"),
            HistoricalPlan::LegacyCatalog,
        );
        approve_plan(&case, &legacy["plan_digest"], "legacy-approval").unwrap();
        let before = stored_plan_row(&case);
        resume(&case).unwrap();
        case.impact("fast_path");
        let writer = case.claim();
        let recorded = legacy["assignments"]
            .as_list()
            .unwrap()
            .iter()
            .find(|item| item["node_key"].is_text("senior_engineer"))
            .unwrap();
        assert!(writer["node_key"].is_text("senior_engineer"));
        assert!(recorded["intelligence_class"].is_text("economy"));
        assert!(writer.get("simplification_review_version").is_none());
        assert!(writer["model"].py_eq(&recorded["model"]));
        assert!(writer["reasoning_effort"].py_eq(&recorded["reasoning_effort"]));
        assert!(stored_plan_row(&case).py_eq(&before));
    }
}

#[test]
fn test_legacy_astra_pending_and_approved_plans_keep_original_digest() {
    let case = HardeningCase::stock();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    let hint = Value::object([("host", "codex-astra".into())]);
    let legacy = reconstruct_execution_plan("RUN-1", &task, &hint, Some("small")).unwrap();
    let initialized = initialize_historical(
        &case,
        &task,
        "codex-astra",
        Some("small"),
        HistoricalPlan::LegacyCatalog,
    );
    let verify = || {
        case.fixture
            .with_session(|session, _| {
                let run = session.run("RUN-1")?.unwrap().fields;
                assert!(validate_execution_plan(session, &run, &task)?.py_eq(&legacy));
                Ok(())
            })
            .unwrap()
    };
    verify();
    assert!(initialized["execution_plan_digest"].py_eq(&legacy["plan_digest"]));
    approve_plan(&case, &legacy["plan_digest"], "legacy-approval").unwrap();
    verify();
}

#[test]
fn test_persisted_v2_task_drives_reconstruction_independently_of_plan_schema() {
    let case = HardeningCase::stock();
    let task = task_v2(&case, "full_delivery");
    let initialized = initialize(&case, &task, None, None, true).unwrap();
    assert!(initialized["execution_plan"]["schema_version"].py_eq(&1_i64.into()));
    assert!(initialized["execution_plan"]["size"].is_text("small"));
    case.fixture
        .with_session(|session, context| {
            let run = session.run("RUN-1")?.unwrap().fields;
            let mut settings = *context;
            settings.case_sensitive = true;
            verify_semantic_state(session, &run, &settings)?;
            verify_resume(session, &run, &settings)
        })
        .unwrap();
}

fn changed(value: &Value, key: &str, replacement: Value) -> Value {
    let mut value = value.clone();
    value.set(key, replacement);
    value
}

#[test]
fn test_astra_plan_tampering_rejects_recomputed_hash_and_retained_approval() {
    let case = HardeningCase::stock();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    let initialized = initialize(&case, &task, Some("codex-astra"), Some("small"), true).unwrap();
    let original = &initialized["execution_plan"];
    let invalid = "EXECUTION_PLAN_STATE_INVALID";
    let invalid_approval = "EXECUTION_PLAN_APPROVAL_INVALID";
    let mut mutations = Vec::new();
    for marker in [
        Value::Null,
        true.into(),
        0_i64.into(),
        2_i64.into(),
        "1".into(),
    ] {
        mutations.push((
            changed(original, "capability_projection_version", marker),
            invalid,
        ));
    }
    let mut unprojected = original.clone();
    unprojected.remove("capability_projection_version");
    mutations.push((unprojected, invalid_approval));
    for (key, value) in [
        ("model", "gpt-5.6-luna"),
        ("reasoning_effort", "high"),
        ("intelligence_class", "economy"),
        ("dispatch_model", "gpt-5.6-luna"),
    ] {
        let mut assignments = original["assignments"].as_list().unwrap().to_vec();
        let lead = assignments
            .iter_mut()
            .find(|item| item["node_key"].is_text("tech_lead"))
            .unwrap();
        lead.set(key, value.into());
        mutations.push((
            changed(original, "assignments", Value::List(assignments)),
            invalid,
        ));
    }
    let next_catalog = original["catalog_revision"].as_i64().unwrap() + 1;
    for marker in [
        Value::Null,
        true.into(),
        compat::loads(b"2.0").unwrap(),
        "2".into(),
        next_catalog.into(),
    ] {
        mutations.push((changed(original, "catalog_revision", marker), invalid));
    }
    let mut downgraded = original.clone();
    downgraded.remove("catalog_revision");
    mutations.push((downgraded, invalid));
    let hint = Value::object([("host", "codex-astra".into())]);
    let legacy = reconstruct_execution_plan("RUN-1", &task, &hint, Some("small")).unwrap();
    mutations.push((legacy, invalid_approval));
    case.fixture
        .with_session(|session, _| {
            let run = session.run("RUN-1")?.unwrap().fields;
            for (mut plan, code) in mutations {
                plan.remove("plan_digest");
                let digest = ids::sha256_bytes(&ids::canonical_bytes(&plan)?);
                plan.set("plan_digest", digest.as_str().into());
                let json = compat::dumps_text(&plan, SerializationProfile::STORED)?
                    .utf8()
                    .unwrap();
                session
                    .connection()
                    .execute(
                        "UPDATE execution_plans SET plan_json=?,plan_digest=? WHERE run_id='RUN-1'",
                        [&json, &digest],
                    )
                    .unwrap();
                let error = validate_execution_plan(session, &run, &task).unwrap_err();
                assert!(
                    matches!(&error, EngineError::State(error) if error.code == code),
                    "{code}: {error:?}"
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_narrow_task_path_link_escape_is_rechecked_on_plan_and_resume() {
    let case = HardeningCase::stock();
    let graph = &case.fixture.graph;
    fs::create_dir(graph.repository.join("src")).unwrap();
    let outside = graph.root.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("file.py"), b"outside").unwrap();
    let mut task = graph.task("delivery", "full_delivery", &[]);
    let mut authority = task["authority"].clone();
    let mut grants = authority["capabilities"].as_list().unwrap().to_vec();
    grants.push(capability(
        "filesystem_read",
        "read",
        "repo:src/linked/file.py",
    ));
    authority.set("capabilities", Value::List(grants));
    task.set("authority", authority);
    initialize(&case, &task, None, None, true).unwrap();
    let link = graph.repository.join("src").join("linked");
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let expected = Some("LINK_OR_REPARSE_POINT");
    contract_failure(case.status(), expected);
    contract_failure(
        case.run(&[
            "resume",
            "--run-id",
            "RUN-1",
            "--ack-degraded-permissions",
            "--ack-degraded-durability",
        ]),
        expected,
    );
    contract_failure(
        case.run(&[
            "plan",
            "--run-id",
            "RUN-NEW",
            "--task-brief",
            graph.repository.join("docs/task.json").to_str().unwrap(),
        ]),
        expected,
    );
}

#[test]
fn test_persisted_fanout_uses_forwarded_case_policy() {
    let mut case = HardeningCase::stock();
    let tags = ["security_privacy"];
    let task = case.fixture.graph.task("delivery", "full_delivery", &tags);
    initialize(&case, &task, None, None, true).unwrap();
    let mapper = case.claim();
    case.record_mapper(&mapper, &tags);
    let lead = case.claim();
    case.success(&lead);
    let advanced = case.advance("design_inputs");
    let fanout_id = &advanced["created_fanout_ids"].as_list().unwrap()[0];
    let status = case.status().unwrap();
    let fanout = status["fanouts"]
        .as_list()
        .unwrap()
        .iter()
        .find(|item| item["fanout_id"].py_eq(fanout_id))
        .unwrap();
    let evidence = case.repo_artifact("finding", "case-policy-evidence");
    let members = fanout["member_branch_ids"]
        .as_list()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, branch)| {
            let path = if index == 0 {
                "src/Feature.py"
            } else {
                "src/feature.py"
            };
            Value::object([
                ("branch_id", branch.clone()),
                (
                    "resources",
                    Value::object([
                        (
                            "writable_paths",
                            Value::list([Value::object([
                                ("path", path.into()),
                                ("scope", "exact".into()),
                            ])]),
                        ),
                        ("mutable_state_refs", Value::List(vec![])),
                        ("exclusive_device_refs", Value::List(vec![])),
                        ("services", Value::List(vec![])),
                    ]),
                ),
            ])
        });
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", "RUN-1".into()),
        ("fanout_id", fanout_id.clone()),
        ("members", Value::list(members)),
        ("dependencies", Value::List(vec![])),
        ("evidence", Value::list([evidence])),
    ]);
    let path = case.inbox(&manifest);
    let args = Value::object([
        ("fanout_id", fanout_id.clone()),
        ("assessment_manifest", path.to_str().unwrap().into()),
        ("authority_ref", "authority:test".into()),
        ("op_id", "case-policy-assessment".into()),
    ]);
    case.fixture.with_session(|session, context| {
        let mut settings = *context;
        settings.case_sensitive = true;
        graph_fanout::record(session, "RUN-1", &args, context.policy, &settings)?;
        let run = session.run("RUN-1")?.unwrap().fields;
        verify_semantic_state(session, &run, &settings)?;
        verify_resume(session, &run, &settings)?;
        settings.case_sensitive = false;
        for result in [verify_semantic_state(session, &run, &settings), verify_resume(session, &run, &settings)] {
            let error = result.unwrap_err();
            assert!(matches!(&error, EngineError::State(error) if error.code == "FANOUT_ASSESSMENT_INVALID"), "{error:?}");
        }
        Ok(())
    }).unwrap();
}
