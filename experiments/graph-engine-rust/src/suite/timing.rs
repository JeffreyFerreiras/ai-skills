use graph_engine::{application::timing, compat::Value, infrastructure::sqlite::SqliteSession};

fn fixture() -> SqliteSession {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("
        CREATE TABLE nodes(branch_id TEXT PRIMARY KEY,run_id TEXT,node_key TEXT,role TEXT,stage TEXT,
          generation INTEGER,status TEXT,retry_count INTEGER,max_retries INTEGER,envelope_json TEXT,
          started_at TEXT,finished_at TEXT);
        CREATE TABLE branch_attempts(run_id TEXT,branch_id TEXT,attempt_number INTEGER,attempt_id TEXT,
          claim_digest TEXT,started_at TEXT,finished_at TEXT,outcome TEXT);
        CREATE TABLE operations(run_id TEXT,resulting_revision INTEGER,response_json TEXT);
        CREATE TABLE joins(join_id TEXT PRIMARY KEY,run_id TEXT);
        CREATE TABLE join_members(join_id TEXT,branch_id TEXT,mandatory INTEGER);
        CREATE TABLE fanouts(fanout_id TEXT PRIMARY KEY,run_id TEXT);
        CREATE TABLE fanout_dependencies(fanout_id TEXT,before_branch_id TEXT,after_branch_id TEXT,reason TEXT);
    ").unwrap();
    // Python's first INSERT begins a deferred transaction, retained until close.
    connection.execute_batch("BEGIN ").unwrap();
    SqliteSession::from_connection(connection)
}

fn node(connection: &SqliteSession, branch_id: &str, started: &str, finished: &str) {
    connection
        .execute_values(
            "INSERT INTO nodes VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
            &[
                branch_id.into(),
                "RUN".into(),
                branch_id.into(),
                "role".into(),
                "delivery".into(),
                0i64.into(),
                "succeeded".into(),
                0i64.into(),
                1i64.into(),
                "{}".into(),
                started.into(),
                finished.into(),
            ],
        )
        .unwrap();
}

fn attempt(
    connection: &SqliteSession,
    branch_id: &str,
    number: i64,
    started: &str,
    finished: &str,
) {
    connection
        .execute_values(
            "INSERT INTO branch_attempts VALUES(?,?,?,?,?,?,?,?)",
            &[
                "RUN".into(),
                branch_id.into(),
                number.into(),
                format!("{branch_id}{number}").into(),
                "a".repeat(64).into(),
                started.into(),
                finished.into(),
                "succeeded".into(),
            ],
        )
        .unwrap();
}

fn run(finish: Value, status: &str) -> Value {
    Value::object([
        ("run_id", "RUN".into()),
        ("status", status.into()),
        ("started_at", "2026-01-01T00:00:00Z".into()),
        ("finished_at", finish),
    ])
}

pub fn exact_overlap_slowest_and_critical_path_metrics() {
    let connection = fixture();
    for (branch, start, finish) in [("A", 1, 5), ("B", 2, 7), ("C", 7, 9)] {
        let start = format!("2026-01-01T00:00:0{start}Z");
        let finish = format!("2026-01-01T00:00:0{finish}Z");
        node(&connection, branch, &start, &finish);
        attempt(&connection, branch, 1, &start, &finish);
    }
    connection
        .execute_values("INSERT INTO fanouts VALUES('F','RUN')", &[])
        .unwrap();
    connection
        .execute_values(
            "INSERT INTO fanout_dependencies VALUES('F','B','C','ordered')",
            &[],
        )
        .unwrap();
    let timing =
        timing::compute_timing(&connection, &run("2026-01-01T00:00:10Z".into(), "complete"))
            .unwrap();
    let stage = &timing["stages"].as_list().unwrap()[0];
    assert_eq!(timing["run"]["wall_time_ms"].as_i64(), Some(10000));
    assert_eq!(
        (
            stage["wall_time_ms"].as_i64(),
            stage["overlap_time_ms"].as_i64()
        ),
        (Some(8000), Some(3000))
    );
    assert!(stage["slowest_branch"].py_eq(&Value::object([
        ("branch_id", "B".into()),
        ("active_duration_ms", 5000i64.into())
    ])));
    assert!(stage["critical_path"].py_eq(&Value::object([
        ("branch_ids", Value::List(vec!["B".into(), "C".into()])),
        ("active_duration_ms", 7000i64.into())
    ])));
}

pub fn retry_lifecycle_and_microsecond_flooring() {
    let connection = fixture();
    node(
        &connection,
        "R",
        "2026-01-01T00:00:01Z",
        "2026-01-01T00:00:06Z",
    );
    attempt(
        &connection,
        "R",
        1,
        "2026-01-01T00:00:01Z",
        "2026-01-01T00:00:02Z",
    );
    attempt(
        &connection,
        "R",
        2,
        "2026-01-01T00:00:04Z",
        "2026-01-01T00:00:06Z",
    );
    let timing = timing::compute_timing(
        &connection,
        &run("2026-01-01T00:00:01.001500Z".into(), "complete"),
    )
    .unwrap();
    assert_eq!(timing["branches"]["R"]["wall_time_ms"].as_i64(), Some(5000));
    assert_eq!(
        timing["branches"]["R"]["active_duration_ms"].as_i64(),
        Some(3000)
    );
    assert_eq!(timing["run"]["wall_time_ms"].as_i64(), Some(1001));
}

pub fn incomplete_metrics_are_null_and_equal_ties_are_lexical() {
    let connection = fixture();
    for branch in ["B", "A"] {
        node(
            &connection,
            branch,
            "2026-01-01T00:00:01Z",
            "2026-01-01T00:00:03Z",
        );
        attempt(
            &connection,
            branch,
            1,
            "2026-01-01T00:00:01Z",
            "2026-01-01T00:00:03Z",
        );
    }
    let tied = timing::compute_timing(&connection, &run("2026-01-01T00:00:04Z".into(), "complete"))
        .unwrap();
    let stage = &tied["stages"].as_list().unwrap()[0];
    assert!(stage["slowest_branch"]["branch_id"].is_text("A"));
    assert!(stage["critical_path"]["branch_ids"].py_eq(&Value::List(vec!["A".into()])));
    connection
        .execute_values("DELETE FROM branch_attempts", &[])
        .unwrap();
    connection.execute_values("DELETE FROM nodes", &[]).unwrap();
    connection
        .execute_values(
            "INSERT INTO nodes VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
            &[
                "RUNNING".into(),
                "RUN".into(),
                "running".into(),
                "role".into(),
                "delivery".into(),
                0i64.into(),
                "running".into(),
                0i64.into(),
                1i64.into(),
                "{}".into(),
                "2026-01-01T00:00:01Z".into(),
                Value::Null,
            ],
        )
        .unwrap();
    connection
        .execute_values(
            "INSERT INTO branch_attempts VALUES(?,?,?,?,?,?,?,?)",
            &[
                "RUN".into(),
                "RUNNING".into(),
                1i64.into(),
                "attempt".into(),
                "a".repeat(64).into(),
                "2026-01-01T00:00:01Z".into(),
                Value::Null,
                Value::Null,
            ],
        )
        .unwrap();
    let incomplete = timing::compute_timing(&connection, &run(Value::Null, "active")).unwrap();
    let branch = &incomplete["branches"]["RUNNING"];
    assert!(!branch["timing_complete"].truthy());
    assert!(branch["wall_time_ms"].is_null());
    assert!(branch["active_duration_ms"].is_null());
    assert!(incomplete["overall"]["overlap_time_ms"].is_null());
    assert!(incomplete["overall"]["slowest_branch"].is_null());
    assert!(incomplete["overall"]["critical_path"].is_null());
}

pub const CASES: &[(&str, fn())] = &[
    (
        "test_exact_overlap_slowest_and_critical_path_metrics",
        exact_overlap_slowest_and_critical_path_metrics,
    ),
    (
        "test_retry_lifecycle_and_microsecond_flooring",
        retry_lifecycle_and_microsecond_flooring,
    ),
    (
        "test_incomplete_metrics_are_null_and_equal_ties_are_lexical",
        incomplete_metrics_are_null_and_equal_ties_are_lexical,
    ),
];
