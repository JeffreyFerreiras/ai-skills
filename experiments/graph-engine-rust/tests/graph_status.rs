#[path = "support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{
        error::EngineError, graph_status::blocked_recovery_guidance, state::GraphSession,
    },
    compat::Value,
};
use initialized_graph::InitializedGraph;
use std::fs;

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
fn status(fixture: &InitializedGraph) -> Value {
    fixture
        .graphctl(&["status".into(), "--run-id".into(), "RUN-1".into()])
        .unwrap()
}

#[test]
fn pending_plan_status_projects_usage_timing_and_approval_without_mutation() {
    let fixture = initialize(false);
    let result = status(&fixture);
    assert!(result["code"].is_text("STATUS"));
    assert!(result["ok"].truthy());
    assert!(result["sensitive"].truthy());
    assert!(result["state_revision"].py_eq(&1_i64.into()));
    assert!(result["execution_plan"]["status"].is_text("pending"));
    assert!(result["next_action"]["kind"].is_text("await_execution_plan_approval"));
    assert!(result["next_action"]["plan_digest"].py_eq(&result["execution_plan"]["plan_digest"]));
    assert!(result["usage"].as_object().is_some());
    assert!(result["timing"].as_object().is_some());
    assert!(result.get("evidence_lifecycle").is_none());
    assert!(!result["branches"].as_list().unwrap().is_empty());
    assert!(result["branches"]
        .as_list()
        .unwrap()
        .iter()
        .all(|branch| branch.get("model").is_some()
            && branch.get("attempt_id").is_some()
            && branch.get("lease_expires_at").is_some()));
    fixture.tear_down();
}

#[test]
fn approved_status_routes_to_first_ready_then_reports_the_claimed_lease() {
    let fixture = initialize(true);
    let before = status(&fixture);
    assert!(before["next_action"]["kind"].is_text("claim"));
    let claim = fixture
        .graphctl(&[
            "next".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--claim".into(),
            "--op-id".into(),
            "status-claim".into(),
        ])
        .unwrap();
    let after = status(&fixture);
    assert!(after["state_revision"].py_eq(&3_i64.into()));
    assert!(after["next_action"]["kind"].is_text("await_result"));
    let branch = after["branches"]
        .as_list()
        .unwrap()
        .iter()
        .find(|branch| branch["branch_id"].py_eq(&before["next_action"]["branch_id"]))
        .unwrap();
    assert!(branch["status"].is_text("running"));
    assert!(branch["attempt_id"].py_eq(&claim["branch"]["attempt_id"]));
    assert!(branch["lease_expires_at"].py_eq(&claim["branch"]["lease_expires_at"]));
    assert!(branch.get("claim_token").is_none());
    assert_eq!(after["next_action"]["leases"].as_list().unwrap().len(), 1);
    fixture.tear_down();
}

#[test]
fn terminal_status_still_revalidates_live_task_bytes() {
    let fixture = initialize(true);
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
            "status-abort".into(),
        ])
        .unwrap();
    assert!(status(&fixture)["next_action"]["kind"].is_text("aborted"));
    let path = fixture.graph.repository.join("docs/task.json");
    let bytes = fs::read(&path).unwrap();
    let mut changed = bytes.clone();
    changed.push(b' ');
    fs::write(&path, changed).unwrap();
    let result = fixture.graphctl(&["status".into(), "--run-id".into(), "RUN-1".into()]);
    fs::write(&path, bytes).unwrap();
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "INPUT_DIGEST_MISMATCH")
    );
    fixture.tear_down();
}

#[test]
fn blocked_guidance_uses_equivalent_review_authority_only_for_the_exact_reason() {
    let fixture = initialize(true);
    fixture
        .with_session(|session, _| {
            let mut run = session.run("RUN-1")?.unwrap().fields;
            assert!(matches!(
                blocked_recovery_guidance(session, &run)?,
                Value::Null
            ));
            run.set("status", "blocked".into());
            run.set("blocked_reason", "MISSING_REQUIRED_REVIEW_SKILL".into());
            let equivalent = blocked_recovery_guidance(session, &run)?;
            assert!(equivalent["next_step"].is_text("inspect_equivalent_review_authorization"));
            assert_eq!(equivalent["requirements"].as_list().unwrap().len(), 2);
            run.set("blocked_reason", "OTHER".into());
            let ordinary = blocked_recovery_guidance(session, &run)?;
            assert!(ordinary["next_step"].is_text("new_plan_required"));
            assert_eq!(ordinary["requirements"].as_list().unwrap().len(), 4);
            assert!(ordinary["origins"].as_list().unwrap().is_empty());
            Ok(())
        })
        .unwrap();
    fixture.tear_down();
}
