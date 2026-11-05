#[path = "../src/suite/timing.rs"]
mod timing_originals;

#[test]
fn exact_overlap_slowest_and_critical_path_metrics() {
    timing_originals::exact_overlap_slowest_and_critical_path_metrics();
}

#[test]
fn retry_lifecycle_and_microsecond_flooring() {
    timing_originals::retry_lifecycle_and_microsecond_flooring();
}

#[test]
fn incomplete_metrics_are_null_and_equal_ties_are_lexical() {
    timing_originals::incomplete_metrics_are_null_and_equal_ties_are_lexical();
}

#[test]
fn critical_path_rejects_cycles_and_keeps_unbounded_weights() {
    use graph_engine::{
        application::{error::EngineError, timing},
        compat::PyString,
    };
    use num_bigint::BigInt;
    use std::collections::{BTreeMap, BTreeSet};
    let a = PyString::from("A");
    let b = PyString::from("B");
    let huge: BigInt = BigInt::from(1u64) << 180usize;
    let weights = BTreeMap::from([(a.clone(), huge.clone()), (b.clone(), huge.clone())]);
    let edge = BTreeSet::from([(a.clone(), b.clone())]);
    let (path, duration) = timing::critical_path(&[b.clone(), a.clone()], &weights, &edge).unwrap();
    assert_eq!(path, vec![a.clone(), b.clone()]);
    assert_eq!(duration, &huge * 2);
    let cycle = BTreeSet::from([(a.clone(), b.clone()), (b.clone(), a.clone())]);
    assert!(
        matches!(timing::critical_path(&[a,b], &weights, &cycle).unwrap_err(), EngineError::State(error) if error.to_string()=="TIMING_GRAPH_INVALID")
    );
}

#[test]
fn negative_microseconds_use_python_floor_division() {
    use graph_engine::{application::timing, compat::Value, infrastructure::sqlite::SqliteSession};
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE nodes(branch_id TEXT,run_id TEXT); CREATE TABLE operations(run_id TEXT,resulting_revision INTEGER,response_json TEXT); CREATE TABLE fanouts(fanout_id TEXT,run_id TEXT); CREATE TABLE fanout_dependencies(fanout_id TEXT,before_branch_id TEXT,after_branch_id TEXT);").unwrap();
    let session = SqliteSession::from_connection(connection);
    let timing = timing::compute_timing(
        &session,
        &Value::object([
            ("run_id", "RUN".into()),
            ("status", "complete".into()),
            ("started_at", "1970-01-01T00:00:00.000001Z".into()),
            ("finished_at", "1970-01-01T00:00:00Z".into()),
        ]),
    )
    .unwrap();
    assert_eq!(timing["run"]["wall_time_ms"].as_i64(), Some(-1));
}
