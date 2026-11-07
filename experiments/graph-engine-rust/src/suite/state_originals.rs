#[path = "../../tests/support/concurrent_graph.rs"]
mod concurrent_graph;
#[allow(dead_code)]
#[path = "../../tests/support/initialized_graph.rs"]
mod initialized_graph;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        state::{mutate_persistent, GraphSession},
    },
    compat::Value,
    infrastructure::sqlite::SqliteSession,
};
use initialized_graph::InitializedGraph;
use std::cell::RefCell;

pub const CASES: &[(&str, fn())] = &[
    (
        "test_fault_before_commit_leaves_claim_ready",
        fault_before_commit_leaves_claim_ready,
    ),
    (
        "test_replay_precedes_state_matrix_for_every_mutation",
        replay_precedes_state_matrix_for_every_mutation,
    ),
    (
        "test_mutation_validates_before_and_after_and_rolls_back_on_failure",
        mutation_validates_before_and_after_and_rolls_back_on_failure,
    ),
    (
        "test_concurrent_claims_have_one_winner_and_one_conflict",
        concurrent_claims_have_one_winner_and_one_conflict,
    ),
];

pub fn fault_before_commit_leaves_claim_ready() {
    use graph_engine::{
        application::persistent_store::StateStore, infrastructure::policy_files::NativePolicyFiles,
    };
    let fixture = InitializedGraph::new();
    fixture
        .initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true)
        .unwrap();
    let fault = |point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected("fault".into()))
        } else {
            Ok(())
        }
    };
    let faulting = StateStore::new(
        Some(&fixture.graph.store.state_root),
        None,
        &fixture.graph.skill_root,
        &NativePolicyFiles::default(),
        fixture.graph.store.platform,
        fixture.graph.store.database,
        &fault,
    )
    .unwrap();
    let arguments = vec![
        "--repo".into(),
        fixture.graph.repository.to_str().unwrap().into(),
        "next".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--claim".into(),
        "--op-id".into(),
        "fault-op".into(),
    ];
    assert!(
        matches!(fixture.execute_with_store(&arguments, &faulting), Err(EngineError::Injected(message)) if message == "fault")
    );
    let status = fixture
        .graphctl(&["status".into(), "--run-id".into(), "RUN-1".into()])
        .unwrap();
    assert!(status["state_revision"].py_eq(&2_i64.into()));
    assert!(status["branches"].as_list().unwrap()[0]["status"].is_text("ready"));
    drop(faulting);
    fixture.tear_down();
}

pub fn concurrent_claims_have_one_winner_and_one_conflict() {
    use concurrent_graph::ThreadGraph;
    use graph_engine::{
        application::{persistent_store::StateStore, state::FaultHook},
        infrastructure::policy_files::NativePolicyFiles,
    };
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    struct Hook {
        entered: Arc<(Mutex<bool>, Condvar)>,
    }
    impl FaultHook for Hook {
        fn observe(&self, point: &str) -> EngineResult<()> {
            if point == "before_commit" {
                let (entered, signal) = &*self.entered;
                let mut value = entered.lock().unwrap();
                if !*value {
                    *value = true;
                    signal.notify_all();
                    drop(value);
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
            Ok(())
        }
    }
    struct NoFault;
    impl FaultHook for NoFault {
        fn observe(&self, _: &str) -> EngineResult<()> {
            Ok(())
        }
    }

    let fixture = InitializedGraph::new();
    fixture
        .initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true)
        .unwrap();
    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let hook = Arc::new(Hook {
        entered: entered.clone(),
    });
    let first_store = StateStore::new(
        Some(&fixture.graph.store.state_root),
        None,
        &fixture.graph.skill_root,
        &NativePolicyFiles::default(),
        fixture.graph.store.platform,
        fixture.graph.store.database,
        &*hook,
    )
    .unwrap();
    let first = ThreadGraph {
        repository: fixture.graph.repository.clone(),
        installation: fixture.graph.skill_root.clone(),
        state_root: first_store.state_root.clone(),
    };
    let second = ThreadGraph {
        state_root: fixture.graph.store.state_root.clone(),
        ..first.clone()
    };
    let outcomes = Arc::new(Mutex::new(Vec::new()));
    let worker = |graph: ThreadGraph,
                  operation: &str,
                  fault: &dyn FaultHook,
                  outcomes: &Mutex<Vec<String>>| {
        let code = match graph.execute(operation, fault) {
            Ok(result) => result["code"].text().unwrap(),
            Err(EngineError::State(error)) => error.code,
            Err(error) => panic!("unexpected concurrent claim failure: {error:?}"),
        };
        outcomes.lock().unwrap().push(code);
    };
    std::thread::scope(|scope| {
        let one_outcomes = outcomes.clone();
        let one_hook = hook.clone();
        let one = scope.spawn(move || worker(first, "claim-one", &*one_hook, &one_outcomes));
        let (value, signal) = &*entered;
        let (guard, _) = signal
            .wait_timeout_while(value.lock().unwrap(), Duration::from_secs(2), |entered| {
                !*entered
            })
            .unwrap();
        drop(guard);
        let two_outcomes = outcomes.clone();
        let two = scope.spawn(move || worker(second, "claim-two", &NoFault, &two_outcomes));
        one.join().unwrap();
        two.join().unwrap();
    });
    let mut actual = outcomes.lock().unwrap().clone();
    actual.sort();
    assert_eq!(actual, vec!["CLAIMED", "CLAIM_CONFLICT"]);
    drop(first_store);
    fixture.tear_down();
}

fn replay_precedes_state_matrix_for_every_mutation() {
    let fixture = InitializedGraph::new();
    fixture
        .initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true)
        .unwrap();
    let database = fixture
        .graph
        .store
        .db_path("albanian-live-translate", "RUN-1");
    let session = SqliteSession::connect_existing(&database).unwrap();
    session
        .execute_values("UPDATE runs SET status='active' WHERE run_id='RUN-1'", &[])
        .unwrap();
    let commands = [
        "next.claim",
        "record.branch-result",
        "record.timeout",
        "record.skip",
        "record.retry",
        "record.approval",
        "record.budget-use",
        "record.acceptance-evidence",
        "record.check-evidence",
        "join.advance",
        "complete",
        "block",
        "abort",
    ];
    for (index, command) in commands.iter().enumerate() {
        let operation = format!("matrix-{}", index + 1);
        let request = Value::object([("command", (*command).into())]);
        // The original directly tests StateStore.mutate with a test-only no-op validator.
        let first = mutate_persistent(
            &session,
            "RUN-1",
            &operation,
            &request,
            |_, _, _| {
                Ok(Value::object([
                    ("code", "RECORDED".into()),
                    ("command", (*command).into()),
                ]))
            },
            |_, _| Ok(()),
            fixture.graph.store.fault,
        )
        .unwrap();
        session
            .execute_values(
                "UPDATE runs SET status='complete' WHERE run_id='RUN-1'",
                &[],
            )
            .unwrap();
        let replay = mutate_persistent(
            &session,
            "RUN-1",
            &operation,
            &request,
            |_, _, _| panic!("replay action executed"),
            |_, _| panic!("replay validator executed"),
            fixture.graph.store.fault,
        )
        .unwrap();
        assert!(replay["code"].is_text("REPLAYED"));
        assert!(replay["state_revision"].py_eq(&first["state_revision"]));
        session
            .execute_values("UPDATE runs SET status='active' WHERE run_id='RUN-1'", &[])
            .unwrap();
    }
    drop(session);
    fixture.tear_down();
}

fn mutation_validates_before_and_after_and_rolls_back_on_failure() {
    let fixture = InitializedGraph::new();
    fixture
        .initialize(&fixture.graph.task("delivery", "full_delivery", &[]), true)
        .unwrap();
    let database = fixture
        .graph
        .store
        .db_path("albanian-live-translate", "RUN-1");
    let session = SqliteSession::connect_existing(&database).unwrap();
    let original = session.run("RUN-1").unwrap().unwrap().fields["state_revision"].clone();
    let observations = RefCell::new(Vec::new());
    let result = mutate_persistent(
        &session,
        "RUN-1",
        "rollback-validation",
        &Value::object([("command", "next.claim".into())]),
        |_, _, _| {
            session.execute_values(
                "UPDATE runs SET blocked_reason='temporary' WHERE run_id='RUN-1'",
                &[],
            )?;
            Ok(Value::object([("code", "RECORDED".into())]))
        },
        |_, run| -> EngineResult<()> {
            let mut observations = observations.borrow_mut();
            observations.push((
                run.fields["state_revision"].clone(),
                run.fields["blocked_reason"].clone(),
            ));
            if observations.len() == 2 {
                return Err(EngineError::state("POST_ACTION_INVALID"));
            }
            Ok(())
        },
        fixture.graph.store.fault,
    );
    assert!(matches!(result,Err(EngineError::State(error)) if error.code=="POST_ACTION_INVALID"));
    let observations = observations.into_inner();
    assert_eq!(observations.len(), 2);
    assert!(observations[0].0.py_eq(&original));
    assert!(observations[0].1.is_null());
    assert!(observations[1]
        .0
        .py_eq(&(original.as_i64().unwrap() + 1).into()));
    assert!(observations[1].1.is_text("temporary"));
    let run = session.run("RUN-1").unwrap().unwrap().fields;
    assert!(run["state_revision"].py_eq(&original));
    assert!(run["blocked_reason"].is_null());
    assert!(session
        .operation("RUN-1", "rollback-validation")
        .unwrap()
        .is_none());
    assert!(session
        .query_values(
            "SELECT 1 FROM events WHERE source_id='rollback-validation'",
            &[]
        )
        .unwrap()
        .is_empty());
    drop(session);
    fixture.tear_down();
}
