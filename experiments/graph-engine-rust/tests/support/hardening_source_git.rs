use graph_engine::{
    application::{error::EngineResult, source_capture},
    compat::Value,
    domain::ids,
    infrastructure::{
        policy_files::NativePolicyFiles, process::NativeProcess, source_files::NativeSourceFiles,
    },
};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub(super) fn initial_commit(repository: &Path, staged: &str) {
    let artifacts = PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let repository = repository.canonicalize().unwrap();
    assert!(repository.starts_with(&artifacts) && repository != artifacts);
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
    for arguments in [
        &["init"][..],
        &["add", staged][..],
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ][..],
    ] {
        let outcome = Command::new("git")
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
        println!(
            "fixture git {arguments:?} cwd={repository:?} exit={:?} stdout_sha256={} stderr_sha256={} stdout_hex={} stderr_hex={}",
            outcome.status.code(), ids::sha256_bytes(&outcome.stdout), ids::sha256_bytes(&outcome.stderr),
            hex(&outcome.stdout), hex(&outcome.stderr)
        );
        assert!(outcome.status.success(), "git {arguments:?}");
        assert!(repository
            .join(".git")
            .canonicalize()
            .unwrap()
            .starts_with(&repository));
    }
}

pub(super) fn capture(
    repository: &Path,
    policy: &Value,
    root: &str,
    known_inputs: &[&str],
) -> EngineResult<Value> {
    let files = NativePolicyFiles::default();
    let source = NativeSourceFiles::new(repository, policy, &files);
    source_capture::capture_source(
        repository,
        &[root.into()],
        &[],
        &known_inputs
            .iter()
            .copied()
            .map(Into::into)
            .collect::<Vec<_>>(),
        &source,
        &NativeProcess,
    )
}
