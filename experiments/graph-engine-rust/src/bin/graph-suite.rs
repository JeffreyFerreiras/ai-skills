#[path = "../suite/helper_fixture.rs"]
mod helper_fixture;
#[path = "../suite/helper_originals.rs"]
mod helper_originals;
#[path = "../suite/merge_readiness.rs"]
mod merge_cases;
#[path = "../suite/planning.rs"]
mod planning_cases;
#[path = "../suite/state_originals.rs"]
mod state_cases;
#[path = "../suite/task_fixture.rs"]
mod task_fixture;
#[path = "../suite/timing.rs"]
mod timing_cases;
#[path = "../suite/usage_originals.rs"]
mod usage_cases;
#[path = "../suite/validator_originals.rs"]
mod validator_cases;
#[path = "../suite/workload.rs"]
mod workload;

fn registry() -> Vec<(String, fn())> {
    let merges:Vec<(&str,fn())>=vec![
        ("tests.test_daily_workflow.MergeReadinessTests.test_head_change_invalidates_review_and_ci_even_with_identical_tree",merge_cases::changed_heads),
        ("tests.test_daily_workflow.MergeReadinessTests.test_matching_snapshot_is_dry_run_only_and_does_not_mutate_evidence",merge_cases::matching_snapshot),
        ("tests.test_daily_workflow.MergeReadinessTests.test_required_ci_must_exist_and_pass",merge_cases::required_checks),
        ("tests.test_daily_workflow.MergeReadinessTests.test_review_blocks_and_incomplete_graph_prevent_readiness",merge_cases::review_and_graph_blockers),
    ];
    let mut cases: Vec<(String, fn())> = merges
        .into_iter()
        .map(|(id, run)| (String::from(id), run))
        .collect();
    cases.extend(planning_cases::CASES.iter().map(|(name, run)| {
        (
            format!("tests.test_planner_unit.PlannerUnitTests.{name}"),
            *run,
        )
    }));
    cases.extend(helper_originals::CASES.iter().map(|(name, run)| {
        (
            format!("tests.test_graph_hardening.HelperRegisterTests.{name}"),
            *run,
        )
    }));
    cases.extend(timing_cases::CASES.iter().map(|(name, run)| {
        (
            format!("tests.test_graph_hardening.TimingMetricTests.{name}"),
            *run,
        )
    }));
    cases.extend(
        usage_cases::CASES
            .iter()
            .map(|(name, run)| (format!("tests.test_cli.UsageTests.{name}"), *run)),
    );
    cases.extend(
        validator_cases::CASES
            .iter()
            .map(|(name, run)| (format!("tests.test_validator.ValidatorTests.{name}"), *run)),
    );
    cases.extend(
        state_cases::CASES
            .iter()
            .map(|(name, run)| (format!("tests.test_state.StateTests.{name}"), *run)),
    );
    cases.sort_by(|(a, _), (b, _)| a.cmp(b));
    cases
}
fn main() {
    let result = workload::run(registry(), &std::env::args().skip(1).collect::<Vec<_>>());
    match result {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
