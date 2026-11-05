#[path = "support/hardening_case.rs"]
mod hardening_case;

use graph_engine::{
    application::{
        error::EngineError,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::StateStore,
        state::GraphSession,
    },
    compat::{PyString, Value},
    domain::ids,
};
use hardening_case::HardeningCase;

fn finding(id: &str, disposition: &str, scope: Option<&str>) -> Value {
    let mut result = Value::object([
        ("finding_id", id.into()),
        ("disposition", disposition.into()),
    ]);
    if let Some(scope) = scope {
        result.set("scope", scope.into());
    }
    result
}

fn design_to_reviews(case: &mut HardeningCase, mode: &str, route: &str) {
    let task = case.fixture.graph.task(mode, route, &[]);
    case.fixture.initialize(&task, true).unwrap();
    case.impact(route);
    let tech = case.claim();
    assert!(tech["node_key"].is_text("tech_lead"));
    case.success(&tech);
    case.advance("design_inputs");
}

fn record_design_review(case: &mut HardeningCase, decision: &str, id: Option<&str>) {
    let architect = case.claim();
    assert!(architect["node_key"].is_text("architect"));
    let disposition = if decision == "BLOCK" {
        "block"
    } else {
        "revise"
    };
    let findings = id
        .map(|id| finding(id, disposition, None))
        .into_iter()
        .collect();
    case.success_with(&architect, Some(decision), findings)
        .unwrap();
    case.advance("design_collection");
}

fn used(status: &Value, budget: &str) -> Value {
    status["budgets"]
        .as_list()
        .unwrap()
        .iter()
        .find(|row| row["budget_id"].is_text(budget))
        .unwrap()["used"]
        .clone()
}

#[test]
fn test_design_routes_gate_tech_lead_on_bounded_research_collection() {
    let mut case = HardeningCase::golden();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    case.fixture.initialize(&task, true).unwrap();
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
    let status = case.status().unwrap();
    let branches = status["branches"].as_list().unwrap();
    let mut research = branches
        .iter()
        .filter(|branch| branch["stage"].is_text("research"))
        .map(|branch| {
            (
                branch["node_key"].text().unwrap(),
                branch["role"].text().unwrap(),
                branch["generation"].as_i64().unwrap(),
                branch["status"].text().unwrap(),
                branch["model"].text().unwrap(),
                branch["reasoning_effort"].text().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    research.sort();
    assert_eq!(
        research,
        [
            (
                "design_research_architecture".into(),
                "impact_mapper".into(),
                0,
                "ready".into(),
                "gpt-6.1-sol".into(),
                "low".into()
            ),
            (
                "design_research_validation".into(),
                "impact_mapper".into(),
                0,
                "ready".into(),
                "gpt-6.1-sol".into(),
                "low".into()
            ),
        ]
    );
    assert!(branches
        .iter()
        .all(|branch| !branch["node_key"].is_text("tech_lead")));
    let first = case.claim_raw().unwrap();
    let second = case.claim_raw().unwrap();
    let mut names = [
        first["node_key"].text().unwrap(),
        second["node_key"].text().unwrap(),
    ];
    names.sort();
    assert_eq!(
        names,
        ["design_research_architecture", "design_research_validation"]
    );
    case.success(&first);
    case.success(&second);
    assert!(case.advance("research_collection")["outcome"].is_text("RESEARCH_SEALED"));
    let status = case.status().unwrap();
    let lead = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|branch| branch["node_key"].is_text("tech_lead"))
        .collect::<Vec<_>>();
    assert_eq!(lead.len(), 1);
    assert!(lead[0]["generation"].py_eq(&0_i64.into()) && lead[0]["status"].is_text("ready"));
    let tech = case.claim_raw().unwrap();
    let inputs = tech["inputs"].as_list().unwrap();
    let kinds = inputs
        .iter()
        .map(|input| input["kind"].text().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        kinds,
        [
            "branch_result",
            "collection",
            "evidence_manifest",
            "task_brief"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    let collection = inputs
        .iter()
        .find(|input| input["kind"].is_text("collection"))
        .unwrap();
    let join = status["joins"]
        .as_list()
        .unwrap()
        .iter()
        .find(|join| join["join_key"].is_text("research_collection"))
        .unwrap();
    assert!(collection["content"]["join_id"].py_eq(&join["join_id"]));
    case.fixture.tear_down();
}

#[test]
fn test_design_only_approval_closes_without_senior_engineer() {
    let mut case = HardeningCase::golden();
    design_to_reviews(&mut case, "design_only", "design_only");
    record_design_review(&mut case, "APPROVE", None);
    case.consolidate("design", "APPROVE", 0, vec![]);
    case.advance("design_consolidation");
    let status = case.status().unwrap();
    assert!(status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .all(|branch| !branch["role"].is_text("senior_engineer")));
    assert!(status["next_action"]["kind"].is_text("advance_join"));
    case.fixture.tear_down();
}

#[test]
fn test_golden_block_collection_does_not_block_then_consolidation_does() {
    let mut case = HardeningCase::golden();
    design_to_reviews(&mut case, "delivery", "full_delivery");
    record_design_review(&mut case, "BLOCK", Some("ARCH-001"));
    let mid = case.status().unwrap();
    assert!(mid["status"].is_text("active"));
    assert!(used(&mid, "design_revisions").py_eq(&0_i64.into()));
    case.consolidate(
        "design",
        "BLOCK",
        0,
        vec![finding("ARCH-001", "block", None)],
    );
    let result = case.advance("design_consolidation");
    assert!(result["outcome"].is_text("BLOCK"));
    assert!(case.status().unwrap()["status"].is_text("blocked"));
    case.fixture.tear_down();
}

#[test]
fn test_out_of_scope_observation_does_not_stop_scoped_design_delivery() {
    let mut case = HardeningCase::golden();
    design_to_reviews(&mut case, "delivery", "full_delivery");
    let architect = case.claim();
    case.success_with(
        &architect,
        Some("APPROVE"),
        vec![finding("REL-001", "approve", Some("out_of_scope"))],
    )
    .unwrap();
    case.advance("design_collection");
    case.consolidate(
        "design",
        "APPROVE",
        0,
        vec![finding("REL-001", "approve", None)],
    );
    let result = case.advance("design_consolidation");
    let status = case.status().unwrap();
    assert!(result["outcome"].is_text("APPROVE"));
    let ready = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|branch| branch["status"].is_text("ready"))
        .map(|branch| {
            (
                branch["node_key"].text().unwrap(),
                branch["status"].text().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(ready, [("senior_engineer".into(), "ready".into())]);
    case.fixture.tear_down();
}

#[test]
fn test_in_scope_security_block_still_stops_design_delivery() {
    let mut case = HardeningCase::golden();
    design_to_reviews(&mut case, "delivery", "full_delivery");
    let architect = case.claim();
    case.success_with(
        &architect,
        Some("BLOCK"),
        vec![finding("SEC-001", "block", Some("in_scope"))],
    )
    .unwrap();
    case.advance("design_collection");
    case.consolidate(
        "design",
        "BLOCK",
        0,
        vec![finding("SEC-001", "block", None)],
    );
    let result = case.advance("design_consolidation");
    assert!(result["outcome"].is_text("BLOCK"));
    assert!(case.status().unwrap()["status"].is_text("blocked"));
    case.fixture.tear_down();
}

#[test]
fn test_design_rejects_delivery_repair_disposition() {
    let mut case = HardeningCase::golden();
    design_to_reviews(&mut case, "delivery", "full_delivery");
    let architect = case.claim();
    let result = case.success_with(
        &architect,
        Some("REVISE"),
        vec![finding("REV-003", "repair", Some("in_scope"))],
    );
    assert!(
        matches!(result, Err(EngineError::Contract(error)) if error.code == "PHASE_DISPOSITION_MISMATCH")
    );
    case.fixture.tear_down();
}

#[test]
fn test_golden_revise_activates_only_next_design_generation() {
    let mut case = HardeningCase::golden();
    design_to_reviews(&mut case, "delivery", "full_delivery");
    record_design_review(&mut case, "REVISE", Some("ARCH-001"));
    case.consolidate(
        "design",
        "REVISE",
        0,
        vec![finding("ARCH-001", "revise", None)],
    );
    let result = case.advance("design_consolidation");
    let status = case.status().unwrap();
    assert!(result["outcome"].is_text("REVISE"));
    assert!(status["next_action"]["kind"].is_text("record_fanout_assessment"));
    let mut research = status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|branch| branch["stage"].is_text("research"))
        .map(|branch| {
            (
                branch["generation"].as_i64().unwrap(),
                branch["node_key"].text().unwrap(),
                branch["status"].text().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    research.sort();
    assert_eq!(
        research,
        [
            (0, "design_research_architecture".into(), "succeeded".into()),
            (0, "design_research_validation".into(), "succeeded".into()),
            (1, "design_research_architecture".into(), "pending".into()),
            (1, "design_research_validation".into(), "pending".into()),
        ]
    );
    assert!(!status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .any(|branch| branch["node_key"].is_text("tech_lead")
            && branch["generation"].py_eq(&1_i64.into())));
    assert!(used(&status, "design_revisions").py_eq(&1_i64.into()));
    case.fixture.tear_down();
}

#[test]
fn test_zero_design_and_repair_budgets_block_at_exact_boundary() {
    let mut case = HardeningCase::golden();
    case.edit_policy(|policy| {
        let mut limits = policy["limits"].clone();
        limits.set("design_revisions", 0_i64.into());
        policy.set("limits", limits);
    });
    design_to_reviews(&mut case, "delivery", "full_delivery");
    record_design_review(&mut case, "REVISE", Some("ARCH-001"));
    case.consolidate(
        "design",
        "REVISE",
        0,
        vec![finding("ARCH-001", "revise", None)],
    );
    let result = case.advance("design_consolidation");
    assert!(result["outcome"].is_text("BLOCK"));
    assert!(case.status().unwrap()["status"].is_text("blocked"));
    case.fixture.tear_down();
}

#[test]
fn test_terminal_design_rejection_reports_original_gate_without_reopening() {
    let mut case = HardeningCase::stock();
    design_to_reviews(&mut case, "delivery", "full_delivery");
    record_design_review(&mut case, "BLOCK", Some("SEC-001"));
    case.consolidate(
        "design",
        "BLOCK",
        0,
        vec![finding("SEC-001", "block", None)],
    );
    case.advance("design_consolidation");
    let before = case.status().unwrap();
    let recovery = case
        .run(&["recovery", "show", "--run-id", "RUN-1"])
        .unwrap();
    let guidance = &recovery["blocked_recovery"];
    assert!(recovery["packets"].as_list().unwrap().is_empty());
    assert!(guidance["next_step"].is_text("new_plan_required"));
    assert!(guidance["blocked_reason"].is_text("CONSOLIDATION_BLOCK"));
    let architect = guidance["origins"]
        .as_list()
        .unwrap()
        .iter()
        .find(|origin| origin["gate"].is_text("architect"))
        .unwrap();
    assert!(architect["findings"].py_eq(&Value::List(vec![finding("SEC-001", "block", None)])));
    assert!(architect["result_ref"]["ref"]
        .as_string()
        .unwrap()
        .starts_with("ledger:"));
    assert!(guidance.py_eq(&before["next_action"]["recovery"]));
    let claim = case.run(&[
        "next",
        "--run-id",
        "RUN-1",
        "--claim",
        "--op-id",
        "unsafe-retry",
    ]);
    assert!(matches!(claim, Err(EngineError::State(error)) if error.code == "GRAPH_BLOCKED"));
    let after = case.status().unwrap();
    assert!(before["state_revision"].py_eq(&after["state_revision"]));
    assert!(after["status"].is_text("blocked"));
    assert!(after["branches"]
        .as_list()
        .unwrap()
        .iter()
        .all(|branch| !branch["role"].is_text("senior_engineer")));
    case.fixture.tear_down();
}

#[test]
fn test_design_revision_returns_through_fresh_design_gates() {
    let mut case = HardeningCase::stock();
    design_to_reviews(&mut case, "delivery", "full_delivery");
    record_design_review(&mut case, "REVISE", Some("ARCH-001"));
    case.consolidate(
        "design",
        "REVISE",
        0,
        vec![finding("ARCH-001", "revise", None)],
    );
    case.advance("design_consolidation");
    let lead = case.claim();
    assert!(lead["node_key"].is_text("tech_lead") && lead["generation"].py_eq(&1_i64.into()));
    case.success(&lead);
    case.advance_at("design_inputs", 1);
    let architect = case.claim();
    assert!(
        architect["node_key"].is_text("architect") && architect["generation"].py_eq(&1_i64.into())
    );
    case.success_with(&architect, Some("APPROVE"), vec![])
        .unwrap();
    case.advance_at("design_collection", 1);
    case.consolidate("design", "APPROVE", 1, vec![]);
    case.advance_at("design_consolidation", 1);
    let writer = case.claim();
    assert!(writer["node_key"].is_text("senior_engineer"));
    assert!(used(&case.status().unwrap(), "design_revisions").py_eq(&1_i64.into()));
    assert!(case
        .run(&["recovery", "show", "--run-id", "RUN-1"])
        .unwrap()["blocked_recovery"]
        .is_null());
    case.fixture.tear_down();
}

#[test]
fn design_draft_and_revision_share_collection_and_rollback_budget_on_failure() {
    let mut case = HardeningCase::golden();
    design_to_reviews(&mut case, "delivery", "full_delivery");
    let collection = case.open_join("design_collection");
    let collection_id = collection["join_id"].text().unwrap();
    let draft = |case: &HardeningCase| {
        case.run(&[
            "consolidation",
            "draft",
            "--run-id",
            "RUN-1",
            "--join-id",
            &collection_id,
        ])
    };
    assert!(
        matches!(draft(&case), Err(EngineError::State(error)) if error.code == "SOURCE_COLLECTION_NOT_SEALED")
    );
    record_design_review(&mut case, "REVISE", Some("ARCH-001"));
    let before = case.status().unwrap();
    let draft = draft(&case).unwrap();
    assert!(draft["code"].is_text("CONSOLIDATION_DRAFT"));
    assert!(draft["manifest"]["outcome"].is_text("REVISE"));
    assert!(draft["manifest"]["finding_dispositions"]
        .py_eq(&Value::List(vec![finding("ARCH-001", "revise", None)])));
    assert!(draft["unresolved_issue_identities"]
        .as_object()
        .unwrap()
        .is_empty());
    assert!(before["state_revision"].py_eq(&draft["state_revision"]));
    let supervisor = case.claim();
    let collection_input = supervisor["inputs"]
        .as_list()
        .unwrap()
        .iter()
        .find(|input| input["kind"].is_text("collection"))
        .unwrap();
    let digest = ids::sha256_bytes(&ids::canonical_bytes(&collection_input["content"]).unwrap());
    assert!(draft["collection_digest"].is_text(&digest));
    assert!(collection_input["sha256"].is_text(&digest));
    case.record_as(&supervisor, draft["manifest"].clone(), "result")
        .unwrap();
    let join = case.open_join("design_consolidation");
    let join_id = join["join_id"].text().unwrap();
    let before = case.status().unwrap();
    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("design revision fault".into()))
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
    let args = [
        "join",
        "advance",
        "--run-id",
        "RUN-1",
        "--join-id",
        &join_id,
        "--op-id",
        "revise-fault",
    ];
    let mut argv = vec![
        "--repo".into(),
        case.fixture.graph.repository.to_str().unwrap().into(),
    ];
    argv.extend(args.into_iter().map(PyString::from));
    assert!(
        matches!(case.fixture.execute_with_store(&argv, &store), Err(EngineError::Injected(message)) if message == "design revision fault")
    );
    let after = case.status().unwrap();
    assert!(after["state_revision"].py_eq(&before["state_revision"]));
    assert!(after["branches"].py_eq(&before["branches"]));
    assert!(after["joins"].py_eq(&before["joins"]));
    assert!(after["fanouts"].py_eq(&before["fanouts"]));
    assert!(used(&after, "design_revisions").py_eq(&0_i64.into()));
    case.fixture
        .with_session(|session, _| {
            assert!(session.operation("RUN-1", "revise-fault")?.is_none());
            assert!(!session.event_exists("RUN-1", "revise-fault")?);
            let join = session
                .first(
                    GraphRead::JoinForRun,
                    &[join_id.as_str().into(), "RUN-1".into()],
                )?
                .unwrap();
            assert!(join["status"].is_text("open"));
            assert!(join["result_json"].is_null());
            Ok(())
        })
        .unwrap();
    let advanced = case.run(&args).unwrap();
    assert!(advanced["outcome"].is_text("REVISE"));
    assert!(case.run(&args).unwrap()["code"].is_text("REPLAYED"));
    assert!(used(&case.status().unwrap(), "design_revisions").py_eq(&1_i64.into()));
    case.fixture.tear_down();
}

#[test]
fn design_advance_rejects_an_outcome_that_drops_the_architects_revision() {
    let mut case = HardeningCase::golden();
    design_to_reviews(&mut case, "delivery", "full_delivery");
    record_design_review(&mut case, "REVISE", Some("ARCH-001"));
    case.consolidate(
        "design",
        "APPROVE",
        0,
        vec![finding("ARCH-001", "revise", None)],
    );
    let join = case.open_join("design_consolidation");
    let id = join["join_id"].text().unwrap();
    let before = case.status().unwrap();
    let result = case.run(&[
        "join",
        "advance",
        "--run-id",
        "RUN-1",
        "--join-id",
        &id,
        "--op-id",
        "wrong-precedence",
    ]);
    assert!(
        matches!(result, Err(EngineError::Contract(error)) if error.code == "OUTCOME_PRECEDENCE_MISMATCH")
    );
    let after = case.status().unwrap();
    assert!(after["state_revision"].py_eq(&before["state_revision"]));
    assert!(after["joins"].py_eq(&before["joins"]));
    assert!(after["branches"].py_eq(&before["branches"]));
    case.fixture.tear_down();
}
