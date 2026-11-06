use graph_engine::{
    application::{
        artifacts::{persist_artifact, ArtifactResolver},
        error::EngineError,
        persistent_store::StateStore,
        snapshots::{IntegerFileSnapshotPort, SnapshotFailure},
    },
    compat::Value,
    domain::{evidence, ids, policy},
    infrastructure::{
        filesystem::NativeFileSnapshots, policy_files::NativePolicyFiles, sqlite::SqliteSession,
        state_database::NativeStateDatabase, state_platform::NativeStatePlatform,
    },
};
use std::path::Path;
#[path = "support/store.rs"]
mod store_fixture;
use store_fixture::Fixture;
fn contract(error: EngineError) -> String {
    if let EngineError::Contract(error) = error {
        error.code
    } else {
        panic!("unexpected error {error:?}")
    }
}
fn resolver<'a>(
    files: &'a NativePolicyFiles,
    fixture: &'a Fixture,
    runtime: &'a Path,
    ledger: Option<&'a SqliteSession>,
) -> ArtifactResolver<'a> {
    ArtifactResolver {
        files,
        snapshots: files,
        repository: &fixture.repository,
        skill: &fixture.root,
        installation: &fixture.root,
        policy: &fixture.policy,
        runtime: Some(runtime),
        ledger: ledger
            .map(|value| value as &dyn graph_engine::application::artifacts::ArtifactLedger),
    }
}
#[test]
fn integer_snapshot_limits_keep_negative_huge_and_path_error_order() {
    let fixture = Fixture::new("integer-snapshot");
    let empty = fixture.repository.join("empty.json");
    std::fs::write(&empty, []).unwrap();
    let roots = [fixture.repository.clone()];
    let negative = num_bigint::BigInt::from(-1);
    assert!(
        matches!(NativeFileSnapshots.snapshot_integer(&empty, &roots, &negative), Err(SnapshotFailure::Contract(error)) if error.code == "FILE_TOO_LARGE")
    );
    assert!(
        matches!(NativeFileSnapshots.snapshot_integer(&fixture.repository.join("missing"), &roots, &negative), Err(SnapshotFailure::Io(error)) if error.kind() == std::io::ErrorKind::NotFound)
    );
    assert!(
        matches!(NativeFileSnapshots.snapshot_integer(&fixture.root.join("outside"), &roots, &negative), Err(SnapshotFailure::Contract(error)) if error.code == "OUTSIDE_ALLOWED_ROOT")
    );
    assert!(
        matches!(NativeFileSnapshots.snapshot_integer(&fixture.repository, &roots, &negative), Err(SnapshotFailure::Contract(error)) if error.code == "NOT_REGULAR_FILE")
    );
    assert_eq!(
        NativeFileSnapshots
            .snapshot_integer(&empty, &roots, &0_i64.into())
            .unwrap()
            .size,
        0
    );
    std::fs::write(&empty, b"123").unwrap();
    let huge = num_bigint::BigInt::from(1) << 1024;
    assert_eq!(
        NativeFileSnapshots
            .snapshot_integer(&empty, &roots, &huge)
            .unwrap()
            .data,
        b"123"
    );
}
#[test]
fn file_resolution_rechecks_bytes_and_reverification_rejects_replaced_identity() {
    let mut fixture = Fixture::new("artifact-files");
    fixture.policy = policy::default_profile_policy("fixture-repository", &["docs/".into()]);
    std::fs::create_dir(fixture.repository.join("docs")).unwrap();
    let path = fixture.repository.join("docs/content.JSON");
    let bytes = b"{\"answer\":1}\n";
    std::fs::write(&path, bytes).unwrap();
    let files = NativePolicyFiles::default();
    let runtime = fixture.root.join("runtime");
    let context = resolver(&files, &fixture, &runtime, None);
    let unhashed = context
        .resolve_unhashed_reference(&"repo:docs/content.JSON".into(), &"collection".into())
        .unwrap();
    assert!(unhashed.sha256.is_text(&ids::sha256_bytes(bytes)));
    assert!(unhashed.as_input()["size_bytes"].py_eq(&Value::Integer(bytes.len().into())));
    context.reverify_artifact(&unhashed.record()).unwrap();
    std::fs::write(&path, b"changed").unwrap();
    assert_eq!(
        contract(context.reverify_artifact(&unhashed.record()).unwrap_err()),
        "INPUT_DIGEST_MISMATCH"
    );
    std::fs::rename(&path, fixture.repository.join("docs/prior.JSON")).unwrap();
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(
        contract(context.reverify_artifact(&unhashed.record()).unwrap_err()),
        "ARTIFACT_IDENTITY_MISMATCH"
    );
    assert_eq!(
        contract(
            context
                .resolve_unhashed_reference(&"repo:other/file.json".into(), &"collection".into())
                .unwrap_err()
        ),
        "OUTSIDE_ALLOWED_ROOT"
    );
}
#[test]
fn immutable_ledger_persistence_replay_conflict_resolution_and_reverification() {
    let mut fixture = Fixture::new("artifact-ledger");
    fixture.policy = policy::default_profile_policy("fixture-repository", &["docs/".into()]);
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let hook = |_: &str| Ok(());
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &hook).unwrap();
    store.initialize(&fixture.request()).unwrap();
    let connection =
        SqliteSession::connect_existing(&store.db_path("fixture-repository", "RUN-1")).unwrap();
    let files = NativePolicyFiles::default();
    let runtime = fixture.root.join("runtime");
    let context = resolver(&files, &fixture, &runtime, Some(&connection));
    let artifact = evidence::canonical_ledger_artifact(
        &"collection-1".into(),
        &"collection".into(),
        &Value::object([("b", 2_i64.into()), ("a", "é".into())]),
    )
    .unwrap();
    persist_artifact(&connection, "RUN-1", &artifact).unwrap();
    persist_artifact(&connection, "RUN-1", &artifact).unwrap();
    assert!(connection
        .query_values("SELECT COUNT(*) AS count FROM artifacts", &[])
        .unwrap()[0]["count"]
        .py_eq(&1_i64.into()));
    let resolved = context
        .resolve_reference(&artifact.reference, &artifact.sha256, &artifact.kind)
        .unwrap();
    assert!(resolved.record().container_eq(&artifact.record()));
    context.reverify_artifact(&resolved.record()).unwrap();
    let mut changed = artifact.clone();
    changed.source_path = "other".into();
    assert_eq!(
        contract(persist_artifact(&connection, "RUN-1", &changed).unwrap_err()),
        "ARTIFACT_CONFLICT"
    );
    for (content, expected) in [
        (Value::Null, "LEDGER_CONTENT_MISSING"),
        ("{".into(), "LEDGER_CONTENT_INVALID"),
        ("{}".into(), "LEDGER_DIGEST_MISMATCH"),
    ] {
        let mut record = artifact.record();
        record.set("content_json", content);
        assert_eq!(
            contract(context.reverify_artifact(&record).unwrap_err()),
            expected
        );
    }
    let no_ledger = resolver(&files, &fixture, &runtime, None);
    assert_eq!(
        contract(
            no_ledger
                .resolve_reference(&artifact.reference, &artifact.sha256, &artifact.kind)
                .unwrap_err()
        ),
        "LEDGER_CONTEXT_REQUIRED"
    );
}
