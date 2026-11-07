#[path = "../src/suite/planning.rs"]
mod cases;
#[path = "../src/suite/task_fixture.rs"]
mod task_fixture;

#[test]
fn original_planning_assertions() {
    let mut failures = Vec::new();
    for (name, run) in cases::CASES {
        println!("RUN tests.test_planner_unit.PlannerUnitTests.{name}");
        if std::panic::catch_unwind(*run).is_err() {
            failures.push(*name);
        }
    }
    assert!(
        failures.is_empty(),
        "failed original identities: {failures:?}"
    );
}
