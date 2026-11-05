use graph_engine::{
    application::snapshots::{safe_json_snapshot, FileSnapshotPort, SnapshotFailure},
    infrastructure::filesystem::{
        snapshot_integer_final_identity_fault, snapshot_observed, NativeFileSnapshots,
        SnapshotStage,
    },
};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static INDEX: AtomicUsize = AtomicUsize::new(0);
fn fixture() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT")
            .expect("ART is required for native fixtures"),
    )
    .join("fixtures/snapshots")
    .join(format!(
        "{}-{}",
        std::process::id(),
        INDEX.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
fn code(error: SnapshotFailure) -> String {
    match error {
        SnapshotFailure::Contract(e) => e.code,
        other => panic!("unexpected snapshot error: {other:?}"),
    }
}

#[test]
fn snapshot_rejects_identity_replacement_before_open_and_final_path() {
    for stage in [SnapshotStage::BeforeOpen, SnapshotStage::BeforeFinalPath] {
        let root = fixture();
        let path = root.join("file.txt");
        fs::write(&path, b"original").unwrap();
        // Windows CRT sharing blocks physical replacement while the descriptor is open.
        let result = if cfg!(windows) && stage == SnapshotStage::BeforeFinalPath {
            snapshot_integer_final_identity_fault(&path, &[root.clone()], &4096_i64.into())
        } else {
            snapshot_observed(&path, &[root.clone()], 4096, &|actual| {
                if actual == stage {
                    fs::rename(&path, root.join("held-original.txt")).unwrap();
                    fs::write(&path, b"original").unwrap();
                }
            })
        };
        assert_eq!(code(result.unwrap_err()), "FILE_CHANGED");
    }
}
#[test]
#[cfg(windows)]
fn windows_descriptor_blocks_delete_through_final_check_then_closes() {
    let root = fixture();
    let path = root.join("file.txt");
    let held = root.join("held.txt");
    fs::write(&path, b"original").unwrap();
    let error_code = std::cell::Cell::new(None);
    let snapshot = snapshot_observed(&path, &[root.clone()], 4096, &|stage| {
        if stage == SnapshotStage::BeforeFinalPath {
            error_code.set(fs::rename(&path, &held).unwrap_err().raw_os_error());
        }
    })
    .unwrap();
    assert_eq!(snapshot.data, b"original");
    assert_eq!(error_code.get(), Some(32));
    fs::rename(&path, &held).unwrap();
}
#[test]
fn snapshot_rejects_changes_on_open_handle_and_after_read() {
    for stage in [SnapshotStage::AfterOpen, SnapshotStage::AfterRead] {
        let root = fixture();
        let path = root.join("file.txt");
        fs::write(&path, b"original").unwrap();
        let result = snapshot_observed(&path, &[root.clone()], 4096, &|actual| {
            if actual == stage {
                let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
                file.write_all(b"changed").unwrap();
                file.sync_all().unwrap();
            }
        });
        assert_eq!(code(result.unwrap_err()), "FILE_CHANGED");
    }
}
#[test]
fn snapshot_preserves_open_size_and_read_size_error_order() {
    let root = fixture();
    let path = root.join("file.txt");
    fs::write(&path, b"ok").unwrap();
    assert_eq!(
        code(
            snapshot_observed(&path, &[root.clone()], 3, &|stage| if stage
                == SnapshotStage::BeforeOpen
            {
                fs::write(&path, b"too big").unwrap();
            })
            .unwrap_err()
        ),
        "NOT_REGULAR_FILE"
    );
    fs::write(&path, b"ok").unwrap();
    assert_eq!(
        code(
            snapshot_observed(&path, &[root.clone()], 3, &|stage| if stage
                == SnapshotStage::AfterOpen
            {
                fs::write(&path, b"too big").unwrap();
            })
            .unwrap_err()
        ),
        "FILE_TOO_LARGE"
    );
}
#[test]
fn snapshot_checks_scope_sensitive_components_and_json_before_returning() {
    let root = fixture();
    let path = root.join("file.json");
    fs::write(&path, b"{} ").unwrap();
    assert!(safe_json_snapshot(&NativeFileSnapshots, &path, &[root.clone()], 3).is_ok());
    assert_eq!(
        code(
            NativeFileSnapshots
                .snapshot(&path, &[root.join("other")], 3)
                .unwrap_err()
        ),
        "OUTSIDE_ALLOWED_ROOT"
    );
    fs::create_dir(root.join("secret")).unwrap();
    let denied = root.join("secret/a.json");
    fs::write(&denied, b"{}").unwrap();
    assert_eq!(
        code(
            NativeFileSnapshots
                .snapshot(&denied, &[root.clone()], 4096)
                .unwrap_err()
        ),
        "SENSITIVE_PATH"
    );
    fs::write(&path, b"\xff").unwrap();
    assert_eq!(
        code(safe_json_snapshot(&NativeFileSnapshots, &path, &[root.clone()], 3).unwrap_err()),
        "INVALID_JSON"
    );
    fs::write(&path, b"{\"token\":\"hidden\"}").unwrap();
    assert_eq!(
        code(safe_json_snapshot(&NativeFileSnapshots, &path, &[root], 4096).unwrap_err()),
        "SECRET_FIELD"
    );
}
