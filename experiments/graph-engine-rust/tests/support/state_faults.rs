use super::{database, state_failure, HardeningCase, REPOSITORY, RUN};
use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_cli::ExecuteOutcome,
        persistent_store::StateStore,
        state::FaultHook,
    },
    compat::{PyString, Value},
};
use std::{cell::RefCell, fs, path::PathBuf, sync::mpsc, time::Duration};

#[allow(dead_code)]
#[path = "concurrent_graph.rs"]
mod concurrent_graph;

pub(super) fn write_task(case: &HardeningCase) -> PathBuf {
    let path = case.fixture.graph.repository.join("docs/task.json");
    let task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    super::hardening_case::write_json(&path, &task);
    path
}

pub(super) fn init_argv(case: &HardeningCase, operation: &str) -> Vec<PyString> {
    let path = case.fixture.graph.repository.join("docs/task.json");
    vec![
        "--repo".into(),
        case.fixture.graph.repository.to_str().unwrap().into(),
        "--ack-degraded-permissions".into(),
        "--ack-degraded-durability".into(),
        "init".into(),
        "--run-id".into(),
        RUN.into(),
        "--task-brief".into(),
        path.to_str().unwrap().into(),
        "--op-id".into(),
        operation.into(),
    ]
}

pub(super) fn init_with(
    case: &HardeningCase,
    store: &StateStore<'_>,
    argv: &[PyString],
) -> EngineResult<Value> {
    match case.fixture.execute_with_store(argv, store)? {
        ExecuteOutcome::Response { result, .. } => Ok(result),
        outcome => panic!("unexpected init parser outcome: {outcome:?}"),
    }
}

pub(super) fn alternate_store<'a>(
    case: &HardeningCase,
    home: &str,
    fault: &'a dyn FaultHook,
) -> StateStore<'a> {
    let graph = &case.fixture.graph;
    StateStore::at_root(
        graph.root.join(home),
        graph.store.platform,
        graph.store.database,
        fault,
    )
    .unwrap()
}

pub(super) fn no_fault(_: &str) -> EngineResult<()> {
    Ok(())
}

fn injected_fault(point: &str, at: &str, message: &str) -> EngineResult<()> {
    if point == at {
        Err(EngineError::Injected(message.into()))
    } else {
        Ok(())
    }
}

fn process_death(point: &str) -> EngineResult<()> {
    if point == "post_replace" {
        Err(EngineError::ProcessDeath("simulated process death".into()))
    } else {
        Ok(())
    }
}

fn assert_injected(result: EngineResult<Value>, expected: &str) {
    assert!(matches!(result, Err(EngineError::Injected(message)) if message == expected));
}

fn assert_process_death(result: EngineResult<Value>) {
    assert!(matches!(result, Err(EngineError::ProcessDeath(_))));
}

#[test]
fn test_initialization_fault_never_leaves_authoritative_database() {
    let case = HardeningCase::stock();
    write_task(&case);
    let fault = |point: &str| injected_fault(point, "database_file_flush", "flush");
    let store = alternate_store(&case, "fault-codex", &fault);
    assert_injected(
        init_with(&case, &store, &init_argv(&case, "init-fault")),
        "flush",
    );
    assert!(!store.db_path(REPOSITORY, RUN).exists());
}

#[test]
fn test_all_initialization_fault_points_fail_closed() {
    let case = HardeningCase::stock();
    write_task(&case);
    for expected in [
        "locking_probe",
        "database_file_flush",
        "directory_sync",
        "replace",
        "post_replace",
        "reopen",
        "identity_verification",
        "integrity_check",
    ] {
        let fault = |point: &str| injected_fault(point, expected, expected);
        let store = alternate_store(&case, &format!("fault-{expected}"), &fault);
        assert_injected(
            init_with(
                &case,
                &store,
                &init_argv(&case, &format!("init-{expected}")),
            ),
            expected,
        );
        assert!(!store.db_path(REPOSITORY, RUN).exists(), "{expected}");
    }
}

#[test]
fn test_post_replace_failure_is_recoverable_only_by_identical_init() {
    let case = HardeningCase::stock();
    write_task(&case);
    let fault = |point: &str| injected_fault(point, "post_replace", "death");
    let faulting = alternate_store(&case, "recover-codex", &fault);
    let argv = init_argv(&case, "recover-op");
    assert_injected(init_with(&case, &faulting, &argv), "death");
    assert!(!faulting.db_path(REPOSITORY, RUN).exists());

    let recovered = alternate_store(&case, "recover-codex", &no_fault);
    let mut conflict = argv.clone();
    *conflict.last_mut().unwrap() = "different-op".into();
    state_failure(
        init_with(&case, &recovered, &conflict),
        &["INCOMPLETE_INIT_CONFLICT"],
    );
    assert!(init_with(&case, &recovered, &argv).unwrap()["code"].is_text("INITIALIZED"));
    let replay = init_with(&case, &recovered, &argv).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert_eq!(replay["state_revision"].as_i64(), Some(1));
}

#[test]
fn test_process_death_after_replace_is_superseded_by_identical_init() {
    let case = HardeningCase::stock();
    write_task(&case);
    let dying = alternate_store(&case, "death-codex", &process_death);
    let argv = init_argv(&case, "death-op");
    assert_process_death(init_with(&case, &dying, &argv));
    assert!(dying.db_path(REPOSITORY, RUN).exists());
    let recovered = alternate_store(&case, "death-codex", &no_fault);
    assert!(init_with(&case, &recovered, &argv).unwrap()["code"].is_text("INITIALIZED"));
}

fn incomplete_markers(store: &StateStore<'_>) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(store.run_root(REPOSITORY, RUN))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".incomplete.sqlite3.marker")
        })
}

#[test]
fn test_identical_init_recovers_from_truncated_marker_using_database_digest() {
    let case = HardeningCase::stock();
    write_task(&case);
    let home = "truncated-marker-codex";
    let dying = alternate_store(&case, home, &process_death);
    let argv = init_argv(&case, "truncated-op");
    assert_process_death(init_with(&case, &dying, &argv));
    let marker = incomplete_markers(&dying).next().unwrap();
    fs::write(&marker, b"{").unwrap();
    if !cfg!(windows) {
        dying.platform.set_mode(&marker, 0o600).unwrap();
    }
    let recovered = alternate_store(&case, home, &no_fault);
    assert!(init_with(&case, &recovered, &argv).unwrap()["code"].is_text("INITIALIZED"));
}

#[test]
fn test_conflicting_init_with_absent_marker_fails_closed_from_database_digest() {
    let case = HardeningCase::stock();
    write_task(&case);
    let home = "absent-marker-codex";
    let dying = alternate_store(&case, home, &process_death);
    let mut argv = init_argv(&case, "original-op");
    assert_process_death(init_with(&case, &dying, &argv));
    for marker in incomplete_markers(&dying) {
        fs::remove_file(marker).unwrap();
    }
    *argv.last_mut().unwrap() = "conflicting-op".into();
    let restarted = alternate_store(&case, home, &no_fault);
    state_failure(
        init_with(&case, &restarted, &argv),
        &["INCOMPLETE_INIT_CONFLICT"],
    );
    assert!(dying.db_path(REPOSITORY, RUN).exists());
}

#[test]
fn test_sqlite_cross_connection_lock_probe_is_required_and_succeeds_locally() {
    let case = HardeningCase::stock();
    write_task(&case);
    let seen = RefCell::new(Vec::new());
    let probe = |point: &str| {
        seen.borrow_mut().push(point.to_owned());
        Ok(())
    };
    let probing = alternate_store(&case, "probe-codex", &probe);
    let result = init_with(&case, &probing, &init_argv(&case, "probe-op")).unwrap();
    assert!(result["code"].is_text("INITIALIZED"));
    assert!(seen.borrow().iter().any(|point| point == "locking_probe"));
}

#[test]
fn test_concurrent_initialization_has_one_authoritative_winner() {
    let case = HardeningCase::stock();
    write_task(&case);
    let store = alternate_store(&case, "concurrent-init", &no_fault);
    let graph = concurrent_graph::ThreadGraph {
        repository: case.fixture.graph.repository.clone(),
        installation: case.fixture.graph.skill_root.clone(),
        state_root: store.state_root.clone(),
    };
    let argv = init_argv(&case, "concurrent-op");
    let (entered, receiver) = mpsc::channel();
    let hook = |point: &str| {
        if point == "database_file_flush" {
            entered.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(200));
        }
        Ok(())
    };
    let policy_runtime = case.fixture.graph.runtime_root.clone();
    let execute = |fault: &dyn FaultHook| match graph.execute_args(&argv, &policy_runtime, fault) {
        Ok(result) => result["code"].text().unwrap(),
        Err(EngineError::State(error)) => error.code,
        Err(error) => panic!("unexpected concurrent initialization error: {error:?}"),
    };
    let outcomes = std::thread::scope(|scope| {
        let first = scope.spawn(|| execute(&hook));
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let second = scope.spawn(|| execute(&no_fault));
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert!(outcomes.iter().any(|code| code == "INITIALIZED"));
    assert!(outcomes.iter().all(|code| matches!(
        code.as_str(),
        "INITIALIZED" | "INITIALIZATION_CONFLICT" | "REPLAYED"
    )));
    let authoritative = store.open_run(REPOSITORY, RUN).unwrap();
    assert!(authoritative.first_run().unwrap().is_some());
    assert_eq!(
        database(&store)
            .query_values("SELECT COUNT(*) AS count FROM runs", &[])
            .unwrap()[0]["count"]
            .as_i64(),
        Some(1),
    );
}

#[test]
fn test_second_parent_sync_failure_never_becomes_authoritative() {
    let case = HardeningCase::stock();
    write_task(&case);
    let calls = RefCell::new(0);
    let hook = |point: &str| {
        if point == "directory_sync" {
            let mut calls = calls.borrow_mut();
            *calls += 1;
            if *calls == 2 {
                return Err(EngineError::Injected("post-rename-sync".into()));
            }
        }
        Ok(())
    };
    let store = alternate_store(&case, "sync-codex", &hook);
    assert_injected(
        init_with(&case, &store, &init_argv(&case, "sync-op")),
        "post-rename-sync",
    );
    assert!(!store.db_path(REPOSITORY, RUN).exists());
}
