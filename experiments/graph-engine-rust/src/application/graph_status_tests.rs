#[allow(dead_code)]
#[path = "../../tests/support/hardening_case.rs"]
mod hardening_case;

use super::{evidence_status, status_with_evidence, ClaimServices, Value};
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
