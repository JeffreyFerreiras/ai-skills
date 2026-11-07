#[allow(dead_code)]
#[path = "../../tests/support/hardening_case.rs"]
mod hardening_case;

use super::{evidence_status, resume_next_action, status_with_evidence, ClaimServices, Value};
use crate::application::graph_ledger::{GraphLedgerReadPort, GraphRead};
use crate::infrastructure::{
    claim_runtime::NativeClaimRuntime, process::NativeProcess,
    source_files::NativeSourceFileFactory,
};
use hardening_case::HardeningCase;

fn report_identity(report: &Value) -> usize {
    report.as_object().unwrap().as_ptr() as usize
}

fn status(case: &HardeningCase, reports: &mut Vec<usize>) -> Value {
    case.fixture
        .with_context(|context| {
            let session = context.store.open_run("albanian-live-translate", "RUN-1")?;
            let run = session.first_run()?.unwrap();
            status_with_evidence(
                &*session,
                &run,
                context,
                &ClaimServices {
                    runtime: &NativeClaimRuntime,
                    source_files: &NativeSourceFileFactory,
                    process: &NativeProcess,
                },
                |port, run| {
                    let report = evidence_status(port, run)?;
                    reports.push(report_identity(&report));
                    Ok(report)
                },
            )
        })
        .unwrap()
}

#[test]
fn test_command_status_shares_single_evidence_report_with_next_action() {
    let mut case = HardeningCase::stock();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    case.fixture.initialize(&task, true).unwrap();
    let coverage = case.inbox(&Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "check_coverage".into()),
        (
            "checks",
            Value::list([Value::object([
                ("check_id", "repo-check".into()),
                ("relevant_inputs", Value::list(["docs/".into()])),
                ("complete", true.into()),
            ])]),
        ),
    ]));
    case.run(&[
        "evidence",
        "enable",
        "--run-id",
        "RUN-1",
        "--contract-version",
        "2",
        "--coverage-manifest",
        coverage.to_str().unwrap(),
        "--op-id",
        "enable",
    ])
    .unwrap();
    let mut reports = Vec::new();
    let first = status(&case, &mut reports);
    assert_eq!(reports, [report_identity(&first["evidence_lifecycle"])]);
    case.claim_raw().unwrap();
    let second = status(&case, &mut reports);
    assert_eq!(
        reports,
        [
            report_identity(&first["evidence_lifecycle"]),
            report_identity(&second["evidence_lifecycle"])
        ],
    );
    assert_ne!(reports[0], reports[1]);
    assert!(!first["next_action"].py_eq(&second["next_action"]));
}

#[test]
fn resume_reports_expired_attempt_without_mutating_ledger() {
    let mut case = HardeningCase::stock();
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    case.fixture.initialize(&task, true).unwrap();
    let branch = case.claim_raw().unwrap();
    let mut status = case.status().unwrap();
    let mut branches = status["branches"].as_list().unwrap().to_vec();
    let projected = branches
        .iter_mut()
        .find(|row| row["branch_id"].py_eq(&branch["branch_id"]))
        .unwrap();
    projected.set("lease_expires_at", "2000-01-01T00:00:00Z".into());
    status.set("branches", Value::List(branches));
    case.fixture
        .with_session(|session, _| {
            let run = session.first(GraphRead::Run, &["RUN-1".into()])?.unwrap();
            let before = session
                .first(GraphRead::Node, &[branch["branch_id"].clone()])?
                .unwrap();
            let action =
                resume_next_action(session, &run, &status, &NativeClaimRuntime, cfg!(windows))?;
            assert!(action["kind"].is_text("timeout_expired"));
            assert!(action["branch_id"].py_eq(&branch["branch_id"]));
            assert!(action["attempt_id"].py_eq(&branch["attempt_id"]));
            assert!(before.py_eq(
                &session
                    .first(GraphRead::Node, &[branch["branch_id"].clone()])?
                    .unwrap()
            ));
            Ok(())
        })
        .unwrap();
}
