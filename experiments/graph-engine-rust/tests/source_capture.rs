#[path = "support/store.rs"]
mod store_fixture;

use graph_engine::{
    application::{
        error::EngineError,
        source_capture::{self, SourceFilePort, StatusSourceCache},
    },
    compat::{self, PyString, SerializationProfile, Value},
    domain::source_paths,
    infrastructure::{
        policy_files::NativePolicyFiles, process::NativeProcess, source_files::NativeSourceFiles,
    },
};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
#[cfg(windows)]
fn retained_windows_lstat_reference_matches_native_source_file_metadata() {
    let artifact =
        PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").expect("ART required"));
    let reference = compat::loads_value(&Value::Bytes(
        std::fs::read(artifact.join("parity/source-stat-01/report.json")).unwrap(),
    ))
    .unwrap();
    let fixture = store_fixture::Fixture::new("source-stat-native");
    let policy = Value::object([("denied_patterns", Value::list([]))]);
    let files = NativePolicyFiles::default();
    let source = NativeSourceFiles::new(&fixture.repository, &policy, &files);
    for record in reference["records"].as_list().unwrap() {
        let name = record["name"].text().unwrap();
        let path = fixture.repository.join(&name);
        if name == "directory" {
            std::fs::create_dir(&path).unwrap();
        } else if name != "missing" {
            let hex = record["input_hex"].text().unwrap();
            let bytes = hex
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            std::fs::write(&path, bytes).unwrap();
            if record["readonly"].py_eq(&true.into()) {
                let mut permissions = std::fs::metadata(&path).unwrap().permissions();
                permissions.set_readonly(true);
                std::fs::set_permissions(&path, permissions).unwrap();
            }
        }
        let actual = source.metadata(&PyString::from(name.as_str())).unwrap();
        if record["expected"]["kind"].is_text("exception") {
            assert!(record["expected"]["class"].is_text("FileNotFoundError"));
            assert!(actual.is_none());
        } else {
            let actual = actual.unwrap();
            assert_eq!(
                Some(i64::from(actual.mode)),
                record["expected"]["mode"].as_i64(),
                "{name}"
            );
            assert!(
                Value::from(actual.regular).py_eq(&record["expected"]["regular"]),
                "{name}"
            );
            assert!(
                Value::from(actual.symlink).py_eq(&record["expected"]["symlink"]),
                "{name}"
            );
            assert_eq!(
                Some(actual.size as i64),
                record["expected"]["size"].as_i64(),
                "{name}"
            );
        }
        if record["readonly"].py_eq(&true.into()) {
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
    }
}

fn git(repository: &Path, fixture_root: &Path, arguments: &[&str], observations: &mut Vec<Value>) {
    let artifact = PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let repository = repository.canonicalize().unwrap();
    assert!(
        repository.starts_with(artifact.join("fixtures"))
            && repository.starts_with(fixture_root.canonicalize().unwrap())
    );
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        assert!(
            std::env::var_os(key).is_none(),
            "fixture requires absent {key}"
        );
    }
    let result = Command::new("git")
        .args(arguments)
        .current_dir(&repository)
        .output()
        .unwrap();
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    observations.push(Value::object([
        (
            "argv",
            Value::list(
                std::iter::once("git")
                    .chain(arguments.iter().copied())
                    .map(Value::from),
            ),
        ),
        ("cwd", repository.to_string_lossy().into_owned().into()),
        ("exit", i64::from(result.status.code().unwrap()).into()),
        ("stdout_hex", hex(&result.stdout).into()),
        ("stderr_hex", hex(&result.stderr).into()),
    ]));
    assert!(
        result.status.success(),
        "fixture git command failed: {arguments:?}"
    );
    assert!(repository
        .join(".git")
        .canonicalize()
        .unwrap()
        .starts_with(&repository));
}

#[test]
fn native_source_capture_rejects_missing_git_and_rechecks_real_tracked_and_untracked_edits() {
    let parent =
        PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").unwrap()).join("fixtures");
    std::fs::create_dir_all(&parent).unwrap();
    let fixture = tempfile::Builder::new()
        .prefix("src")
        .tempdir_in(parent)
        .unwrap()
        .keep();
    let repository = fixture.join("repo");
    std::fs::create_dir_all(repository.join("docs")).unwrap();
    std::fs::write(
        repository.join("docs/engineering-graph.md"),
        b"# Native source fixture\r\n",
    )
    .unwrap();
    let policy = Value::object([("denied_patterns", Value::list([]))]);
    let files = NativePolicyFiles::default();
    let source = NativeSourceFiles::new(&repository, &policy, &files);
    let capture = || {
        source_capture::capture_source(
            &repository,
            &["docs/".into()],
            &[],
            &[],
            &source,
            &NativeProcess,
        )
    };
    let error = capture().unwrap_err();
    assert!(
        matches!(error, EngineError::State(error) if error.to_string() == "SOURCE_UNVERIFIABLE")
    );
    let mut observations = Vec::new();
    git(&repository, &fixture, &["init"], &mut observations);
    git(
        &repository,
        &fixture,
        &["add", "docs/engineering-graph.md"],
        &mut observations,
    );
    git(
        &repository,
        &fixture,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
        &mut observations,
    );
    let target = repository.join("docs/untracked.txt");
    std::fs::write(&target, b"first").unwrap();
    let first = capture().unwrap();
    std::fs::write(&target, b"second").unwrap();
    let second = capture().unwrap();
    assert!(!first["sha256"].py_eq(&second["sha256"]));
    let same = capture().unwrap();
    assert!(same.container_eq(&second));
    std::fs::write(
        repository.join("docs/engineering-graph.md"),
        b"# Changed tracked source\r\n",
    )
    .unwrap();
    let third = capture().unwrap();
    assert!(!third["sha256"].py_eq(&second["sha256"]));
    let observations = Value::object([
        ("git", Value::List(observations)),
        ("first", first),
        ("second", second),
        ("third", third),
    ]);
    std::fs::write(
        fixture.join("source-capture-observations.json"),
        compat::dumps(&observations, SerializationProfile::STORED).unwrap(),
    )
    .unwrap();
}

#[test]
fn status_source_cache_preserves_ordered_roots_full_descriptors_and_success_only_scope() {
    let mut cache = StatusSourceCache::default();
    let calls = Cell::new(0);
    let snapshot = || {
        calls.set(calls.get() + 1);
        Ok(Value::object([("sha256", "a".repeat(64).into())]))
    };
    let roots = vec![PyString::from("docs/"), PyString::from("src/")];
    let output = Value::object([
        ("path", "docs/artifacts/report.json".into()),
        ("purpose", "review_report".into()),
        ("producer_node_key", "code_reviewer".into()),
        ("artifact_kind", "delivery_review".into()),
    ]);
    cache.capture(&roots, &[output.clone()], snapshot).unwrap();
    cache.capture(&roots, &[output.clone()], snapshot).unwrap();
    assert_eq!(calls.get(), 1);
    let mut reversed = roots.clone();
    reversed.reverse();
    cache
        .capture(&reversed, &[output.clone()], snapshot)
        .unwrap();
    assert_eq!(calls.get(), 2);
    let mut changed = output.clone();
    changed.set("producer_node_key", "test_engineer".into());
    cache.capture(&roots, &[changed], snapshot).unwrap();
    assert_eq!(calls.get(), 3);
    let roots = [PyString::from("uncached/")];
    assert!(cache
        .capture(&roots, &[], || {
            calls.set(calls.get() + 1);
            Err(EngineError::state("SOURCE_UNVERIFIABLE"))
        })
        .is_err());
    cache.capture(&roots, &[], snapshot).unwrap();
    assert_eq!(calls.get(), 5);
    StatusSourceCache::default()
        .capture(&roots, &[], snapshot)
        .unwrap();
    assert_eq!(calls.get(), 6);
}

#[test]
fn source_path_containment_uses_host_case_rules_and_exact_trailing_slash_authority() {
    assert!(source_paths::contains(
        &"Docs/".into(),
        &"docs/file.txt".into(),
        true
    ));
    assert!(!source_paths::contains(
        &"Docs/".into(),
        &"docs/file.txt".into(),
        false
    ));
    assert!(!source_paths::contains(
        &"docs".into(),
        &"docs/file.txt".into(),
        true
    ));
    assert!(!source_paths::contains(
        &"docs\\".into(),
        &"docs/file.txt".into(),
        true
    ));
    assert!(source_paths::contains(
        &"ΟΣ/".into(),
        &"ος/file.txt".into(),
        true
    ));
    assert!(!source_paths::contains(
        &"docs/".into(),
        &"docs-other/file.txt".into(),
        true
    ));
}
