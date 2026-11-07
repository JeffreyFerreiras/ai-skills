#[path = "support/store.rs"]
mod store_fixture;

use graph_engine::{
    application::{
        check_fences::{self, CheckClockPort},
        error::{EngineError, EngineResult},
        persistent_store::StateStore,
        state::GraphSession,
    },
    compat::{self, SerializationProfile, Value},
    infrastructure::{
        sqlite::SqliteSession, state_database::NativeStateDatabase,
        state_platform::NativeStatePlatform,
    },
};
use std::cell::Cell;

struct Clock(Cell<usize>);
impl CheckClockPort for Clock {
    fn utc_now(&self) -> EngineResult<String> {
        self.0.set(self.0.get() + 1);
        Ok("2024-01-01T00:00:00Z".into())
    }
}
fn initialize() -> (store_fixture::Fixture, SqliteSession, Value, Value) {
    let fixture = store_fixture::Fixture::new("check-fences");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let fault = |_: &str| Ok(());
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &fault).unwrap();
    store.initialize(&fixture.request()).unwrap();
    let session =
        SqliteSession::connect_existing(&store.db_path("fixture-repository", "RUN-1")).unwrap();
    let envelope = Value::object([
        ("attempt_id", "attempt-1".into()),
        ("claim_digest", "c".repeat(64).into()),
        ("lease_expires_at", "2024-01-01T00:01:00Z".into()),
    ]);
    session.execute_values("UPDATE nodes SET node_key='senior_engineer',stage='implementation',status='running',envelope_json=? WHERE branch_id='bootstrap-branch'",
        &[String::from_utf8(compat::dumps(&envelope, SerializationProfile::CLI).unwrap()).unwrap().into()]).unwrap();
    session.execute_values("INSERT INTO branch_attempts(run_id,branch_id,attempt_number,attempt_id,claim_digest,started_at) VALUES('RUN-1','bootstrap-branch',1,'attempt-1',?,'2024-01-01T00:00:00Z')", &["c".repeat(64).into()]).unwrap();
    session.execute_values("UPDATE runs SET selected_route='full_delivery',implementation_generation=1 WHERE run_id='RUN-1'", &[]).unwrap();
    let run = session.run("RUN-1").unwrap().unwrap().fields;
    assert_eq!(
        run["implementation_generation"].as_i64(),
        Some(1),
        "implementation branch fixture requires matching run generation"
    );
    let fence = Value::object([
        ("branch_id", "bootstrap-branch".into()),
        ("attempt_id", "attempt-1".into()),
        ("claim_digest", "c".repeat(64).into()),
    ]);
    (fixture, session, run, fence)
}
fn mismatch(error: EngineError) {
    assert!(
        matches!(error, EngineError::State(error) if error.to_string() == "ATTEMPT_FENCE_MISMATCH")
    );
}

#[test]
fn check_fence_reloads_attempts_status_generation_and_route_from_real_sqlite() {
    let (_fixture, session, run, mut fence) = initialize();
    let clock = Clock(Cell::new(0));
    let verified = check_fences::verify_check_fence(&session, &clock, &run, &fence, false).unwrap();
    assert!(verified.branch["node_key"].is_text("senior_engineer"));
    assert!(verified.envelope["attempt_id"].is_text("attempt-1"));
    assert_eq!(clock.0.get(), 1);
    let immutable = session
        .execute_values(
            "UPDATE branch_attempts SET claim_digest=?",
            &["d".repeat(64).into()],
        )
        .unwrap_err();
    assert!(
        matches!(immutable, EngineError::Storage { kind: "IntegrityError", message, .. } if message == "ATTEMPT_IMMUTABLE")
    );
    let mut presented = Value::object([
        ("attempt_id", "attempt-1".into()),
        ("claim_digest", "d".repeat(64).into()),
        ("lease_expires_at", "2024-01-01T00:01:00Z".into()),
    ]);
    session
        .execute_values(
            "UPDATE nodes SET envelope_json=?",
            &[
                String::from_utf8(compat::dumps(&presented, SerializationProfile::CLI).unwrap())
                    .unwrap()
                    .into(),
            ],
        )
        .unwrap();
    fence.set("claim_digest", "d".repeat(64).into());
    mismatch(
        check_fences::verify_check_fence(&session, &clock, &run, &fence, false)
            .err()
            .unwrap(),
    );
    assert_eq!(clock.0.get(), 1);
    presented.set("claim_digest", "c".repeat(64).into());
    fence.set("claim_digest", "c".repeat(64).into());
    session
        .execute_values(
            "UPDATE nodes SET envelope_json=?",
            &[
                String::from_utf8(compat::dumps(&presented, SerializationProfile::CLI).unwrap())
                    .unwrap()
                    .into(),
            ],
        )
        .unwrap();
    session
        .execute_values("UPDATE nodes SET generation=2", &[])
        .unwrap();
    mismatch(
        check_fences::verify_check_fence(&session, &clock, &run, &fence, false)
            .err()
            .unwrap(),
    );
    session
        .execute_values(
            "UPDATE nodes SET generation=1,node_key='code_reviewer'",
            &[],
        )
        .unwrap();
    let error = check_fences::verify_check_fence(&session, &clock, &run, &fence, false)
        .err()
        .unwrap();
    assert!(
        matches!(error, EngineError::State(error) if error.to_string() == "SOURCE_ATTEMPT_INVALID")
    );
    check_fences::verify_check_fence(&session, &clock, &run, &fence, true).unwrap();
}

#[test]
fn check_fence_handles_terminal_attempts_without_clock_and_expired_running_leases() {
    let (_fixture, session, run, fence) = initialize();
    let clock = Clock(Cell::new(0));
    let envelope = Value::object([
        ("attempt_id", "attempt-1".into()),
        ("claim_digest", "c".repeat(64).into()),
        ("lease_expires_at", "2024-01-01T00:00:00Z".into()),
    ]);
    session
        .execute_values(
            "UPDATE nodes SET envelope_json=?",
            &[
                String::from_utf8(compat::dumps(&envelope, SerializationProfile::CLI).unwrap())
                    .unwrap()
                    .into(),
            ],
        )
        .unwrap();
    mismatch(
        check_fences::verify_check_fence(&session, &clock, &run, &fence, true)
            .err()
            .unwrap(),
    );
    assert_eq!(clock.0.get(), 1);
    session
        .execute_values("UPDATE nodes SET status='succeeded'", &[])
        .unwrap();
    check_fences::verify_check_fence(&session, &clock, &run, &fence, true).unwrap();
    assert_eq!(clock.0.get(), 1);
}
