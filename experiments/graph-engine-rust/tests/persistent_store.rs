use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        persistent_store::{StatePlatform, StateStore},
    },
    compat::{PyString, Value},
    infrastructure::{
        sqlite::SqliteSession, state_database::NativeStateDatabase,
        state_platform::NativeStatePlatform,
    },
};
use std::cell::RefCell;
#[path = "support/store.rs"]
mod store_fixture;
use store_fixture::Fixture;
fn code(error: EngineError) -> String {
    if let EngineError::State(error) = error {
        error.code
    } else {
        panic!("unexpected error {error:?}")
    }
}
#[test]
fn initialize_reopen_and_identical_operation_replay_persist_authoritative_identity() {
    let fixture = Fixture::new("success");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let points = RefCell::new(Vec::new());
    let hook = |point: &str| {
        points.borrow_mut().push(point.to_owned());
        Ok(())
    };
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
    let first = store.initialize(&fixture.request()).unwrap();
    assert!(first["code"].is_text("INITIALIZED"));
    let connection = store.open_run("fixture-repository", "RUN-1").unwrap();
    let run = connection.first_run().unwrap().unwrap();
    let info = platform
        .info(&store.db_path("fixture-repository", "RUN-1"), true)
        .unwrap();
    assert!(run["authoritative"].py_eq(&1_i64.into()));
    assert!(run["database_device"].py_eq(&Value::Integer(info.device.into())));
    assert!(run["database_inode"].py_eq(&Value::Integer(info.inode.into())));
    assert!(run["sqlite_version"].is_text("3.49.1"));
    assert!(run["local_filesystem"].is_text(&platform.locality(&store.state_root).unwrap()));
    drop(connection);
    let replay = store.initialize(&fixture.request()).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&first["state_revision"]));
    let expected = [
        "directory_sync",
        "locking_probe",
        "database_file_flush",
        "directory_sync",
        "replace",
        "post_replace",
        "directory_sync",
        "reopen",
        "identity_verification",
        "integrity_check",
    ];
    assert_eq!(&*points.borrow(), &expected);
    assert!(!store
        .run_root("fixture-repository", "RUN-1")
        .join(".init.lock")
        .exists());
}
#[test]
fn every_initializer_fault_removes_authoritative_final_and_retains_recovery_marker() {
    for selected in [
        "locking_probe",
        "database_file_flush",
        "directory_sync",
        "replace",
        "post_replace",
        "reopen",
        "identity_verification",
        "integrity_check",
    ] {
        let fixture = Fixture::new(selected);
        let platform = NativeStatePlatform;
        let database = NativeStateDatabase;
        let hook = |point: &str| -> EngineResult<()> {
            if point == selected {
                Err(EngineError::Injected(point.into()))
            } else {
                Ok(())
            }
        };
        let store =
            StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
        assert!(
            matches!(
                store.initialize(&fixture.request()),
                Err(EngineError::Injected(_))
            ),
            "{selected}"
        );
        assert!(
            !store.db_path("fixture-repository", "RUN-1").exists(),
            "{selected}"
        );
        let entries = platform
            .entries(&store.run_root("fixture-repository", "RUN-1"))
            .unwrap();
        assert!(
            entries.iter().any(|path| platform
                .spelling(path)
                .0
                .ends_with(&PyString::from(".incomplete.sqlite3.marker").0)),
            "{selected}"
        );
        assert!(!entries
            .iter()
            .any(|path| path.file_name().unwrap() == ".init.lock"));
    }
}
#[test]
fn post_replace_exception_conflicts_then_recovers_identical_request() {
    let fixture = Fixture::new("recover-exception");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let hook = |point: &str| -> EngineResult<()> {
        if point == "post_replace" {
            Err(EngineError::Injected(point.into()))
        } else {
            Ok(())
        }
    };
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
    assert!(store.initialize(&fixture.request()).is_err());
    assert!(!store.db_path("fixture-repository", "RUN-1").exists());
    let no_fault = |_: &str| Ok(());
    let recovered =
        StateStore::at_root(store.state_root.clone(), &platform, &database, &no_fault).unwrap();
    let mut different = fixture.request();
    different.request_digest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    assert_eq!(
        code(recovered.initialize(&different).unwrap_err()),
        "INCOMPLETE_INIT_CONFLICT"
    );
    assert!(recovered.initialize(&fixture.request()).unwrap()["code"].is_text("INITIALIZED"));
    assert!(recovered.initialize(&fixture.request()).unwrap()["code"].is_text("REPLAYED"));
}
#[test]
fn process_death_leaves_non_authoritative_database_and_ignores_truncated_marker_on_recovery() {
    let fixture = Fixture::new("recover-death");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let hook = |point: &str| -> EngineResult<()> {
        if point == "post_replace" {
            Err(EngineError::ProcessDeath(point.into()))
        } else {
            Ok(())
        }
    };
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
    assert!(matches!(
        store.initialize(&fixture.request()),
        Err(EngineError::ProcessDeath(_))
    ));
    assert!(store.db_path("fixture-repository", "RUN-1").exists());
    let no_fault = |_: &str| Ok(());
    let recovered =
        StateStore::at_root(store.state_root.clone(), &platform, &database, &no_fault).unwrap();
    assert_eq!(
        code(
            recovered
                .open_run("fixture-repository", "RUN-1")
                .err()
                .unwrap()
        ),
        "NON_AUTHORITATIVE_STATE"
    );
    for entry in platform
        .entries(&store.run_root("fixture-repository", "RUN-1"))
        .unwrap()
    {
        if entry.extension().is_some_and(|value| value == "marker") {
            std::fs::write(entry, b"{").unwrap();
        }
    }
    assert!(recovered.initialize(&fixture.request()).unwrap()["code"].is_text("INITIALIZED"));
}
#[test]
fn different_request_cannot_replace_non_authoritative_database_without_marker() {
    let fixture = Fixture::new("death-no-marker");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let hook = |point: &str| -> EngineResult<()> {
        if point == "post_replace" {
            Err(EngineError::ProcessDeath(point.into()))
        } else {
            Ok(())
        }
    };
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
    let _ = store.initialize(&fixture.request());
    for entry in platform
        .entries(&store.run_root("fixture-repository", "RUN-1"))
        .unwrap()
    {
        if entry.extension().is_some_and(|value| value == "marker") {
            std::fs::remove_file(entry).unwrap();
        }
    }
    let no_fault = |_: &str| Ok(());
    let recovered =
        StateStore::at_root(store.state_root.clone(), &platform, &database, &no_fault).unwrap();
    let mut different = fixture.request();
    different.request_digest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    assert_eq!(
        code(recovered.initialize(&different).unwrap_err()),
        "INCOMPLETE_INIT_CONFLICT"
    );
    assert!(store.db_path("fixture-repository", "RUN-1").exists());
}
#[test]
fn open_run_checks_current_inbox_identity_durability_and_permissions() {
    for (label, sql, expected) in [("identity", "UPDATE runs SET database_inode=0", "DATABASE_IDENTITY_FAILED"), ("locality", "UPDATE runs SET local_filesystem='changed'", "FILESYSTEM_IDENTITY_CHANGED"), ("durability", "UPDATE runs SET durability='unknown'", "DURABILITY_STATE_INVALID"), ("permission", "UPDATE runs SET degraded_permissions_ack=0,permission_verification='DEGRADED_PERMISSION_VERIFICATION'", "PERMISSION_VERIFICATION_FAILED")] {
        let fixture = Fixture::new(label); let platform = NativeStatePlatform; let database = NativeStateDatabase; let hook = |_: &str| Ok(());
        let store = StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap(); store.initialize(&fixture.request()).unwrap();
        let raw = SqliteSession::connect_existing(&store.db_path("fixture-repository", "RUN-1")).unwrap(); raw.execute_values(sql, &[]).unwrap(); drop(raw);
        assert_eq!(code(store.open_run("fixture-repository", "RUN-1").err().unwrap()), expected);
    }
    let fixture = Fixture::new("inbox");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let hook = |_: &str| Ok(());
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
    store.initialize(&fixture.request()).unwrap();
    std::fs::create_dir(
        store
            .inbox_root("fixture-repository", "RUN-1")
            .join("unexpected"),
    )
    .unwrap();
    assert_eq!(
        code(store.open_run("fixture-repository", "RUN-1").err().unwrap()),
        "INBOX_ENTRY_INVALID"
    );
}
#[test]
fn windows_permission_ack_precedes_database_creation_and_directory_sync_ack() {
    if !cfg!(windows) {
        return;
    }
    let fixture = Fixture::new("acks");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let hook = |_: &str| Ok(());
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
    let mut request = fixture.request();
    request.acknowledge_permissions = false;
    request.acknowledge_durability = false;
    assert_eq!(
        code(store.initialize(&request).unwrap_err()),
        "DEGRADED_PERMISSION_ACK_REQUIRED"
    );
    assert!(!store.db_path("fixture-repository", "RUN-1").exists());
    request.acknowledge_permissions = true;
    assert_eq!(
        code(store.initialize(&request).unwrap_err()),
        "DEGRADED_DURABILITY_ACK_REQUIRED"
    );
}
#[test]
fn concurrent_initialization_has_one_authoritative_winner() {
    let fixture = Fixture::new("concurrent");
    let (sender, receiver) = std::sync::mpsc::channel();
    let first_fixture = fixture.clone();
    let first = std::thread::spawn(move || {
        let platform = NativeStatePlatform;
        let database = NativeStateDatabase;
        let hook = |point: &str| {
            if point == "database_file_flush" {
                sender.send(()).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Ok(())
        };
        let store = StateStore::at_root(
            first_fixture.root.join("state"),
            &platform,
            &database,
            &hook,
        )
        .unwrap();
        store
            .initialize(&first_fixture.request())
            .map(|response| response["code"].text().unwrap())
            .unwrap_or_else(code)
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    let second_fixture = fixture.clone();
    let second = std::thread::spawn(move || {
        let platform = NativeStatePlatform;
        let database = NativeStateDatabase;
        let hook = |_: &str| Ok(());
        let store = StateStore::at_root(
            second_fixture.root.join("state"),
            &platform,
            &database,
            &hook,
        )
        .unwrap();
        store
            .initialize(&second_fixture.request())
            .map(|response| response["code"].text().unwrap())
            .unwrap_or_else(code)
    });
    let outcomes = [first.join().unwrap(), second.join().unwrap()];
    assert!(outcomes.iter().any(|value| value == "INITIALIZED"));
    assert!(outcomes.iter().all(|value| matches!(
        value.as_str(),
        "INITIALIZED" | "INITIALIZATION_CONFLICT" | "REPLAYED"
    )));
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let hook = |_: &str| Ok(());
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
    store.open_run("fixture-repository", "RUN-1").unwrap();
    let raw =
        SqliteSession::connect_existing(&store.db_path("fixture-repository", "RUN-1")).unwrap();
    assert!(raw
        .query_values("SELECT COUNT(*) AS count FROM runs", &[])
        .unwrap()[0]["count"]
        .py_eq(&1_i64.into()));
}
#[test]
fn second_directory_sync_failure_never_becomes_authoritative() {
    let fixture = Fixture::new("second-sync");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let calls = RefCell::new(0);
    let hook = |point: &str| {
        if point == "directory_sync" {
            *calls.borrow_mut() += 1;
            if *calls.borrow() == 2 {
                return Err(EngineError::Injected("post-rename-sync".into()));
            }
        }
        Ok(())
    };
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
    assert!(matches!(
        store.initialize(&fixture.request()),
        Err(EngineError::Injected(_))
    ));
    assert!(!store.db_path("fixture-repository", "RUN-1").exists());
}
