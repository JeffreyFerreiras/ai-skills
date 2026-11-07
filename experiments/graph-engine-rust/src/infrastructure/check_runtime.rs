use super::{
    process::NativeProcess,
    state_platform::{self, NativeStatePlatform},
};
use crate::{
    application::{
        check_execution::CheckRuntimePort,
        check_receipts::CheckProvenancePort,
        error::{EngineError, EngineResult},
        persistent_store::StatePlatform,
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
    },
    compat::{PyString, Value},
    domain::ids,
};
use std::{path::Path, time::Duration};

pub struct NativeCheckRuntime<'a> {
    pub repository: &'a Path,
}

pub fn repository_worktree_digest(
    repository: &Path,
    process: &dyn ProcessPort,
) -> EngineResult<String> {
    let mut values = Value::object([("git", true.into())]);
    for (name, arguments) in [
        ("head", vec!["git", "rev-parse", "HEAD"]),
        ("status", vec!["git", "status", "--porcelain=v1"]),
        ("diff", vec!["git", "diff", "--binary", "HEAD", "--"]),
    ] {
        let arguments = arguments
            .into_iter()
            .map(PyString::from)
            .collect::<Vec<_>>();
        let result = process.capture(&ProcessRequest {
            arguments: &arguments,
            directory: repository,
            timeout: Duration::from_secs(10),
            stdout_limit: None,
            capture_stderr: true,
        });
        match result {
            Ok(ProcessOutcome::Completed {
                exit_code: 0,
                stdout,
                ..
            }) => {
                values.set(
                    name,
                    if name == "diff" {
                        ids::sha256_bytes(&stdout).into()
                    } else {
                        String::from_utf8_lossy(&stdout).into_owned().into()
                    },
                );
            }
            Err(EngineError::Io(_))
            | Ok(ProcessOutcome::Completed { .. } | ProcessOutcome::TimedOut { .. }) => {
                values.set("git", false.into());
                values.set(name, Value::Null);
            }
            Err(error) => return Err(error),
            Ok(ProcessOutcome::OutputLimit) => {
                return Err(EngineError::state("SOURCE_UNVERIFIABLE"))
            }
        }
    }
    Ok(ids::sha256_bytes(&ids::canonical_bytes(&values)?))
}

impl CheckProvenancePort for NativeCheckRuntime<'_> {
    fn actor(&self) -> EngineResult<PyString> {
        state_platform::current_actor()
    }
    fn host_identity(&self) -> EngineResult<String> {
        NativeStatePlatform.host_identity()
    }
    fn repository_worktree_digest(&self) -> EngineResult<String> {
        repository_worktree_digest(self.repository, &NativeProcess)
    }
}
impl CheckRuntimePort for NativeCheckRuntime<'_> {
    fn utc_now(&self) -> EngineResult<String> {
        NativeStatePlatform.utc_now()
    }
}
impl crate::application::check_fences::CheckClockPort for NativeCheckRuntime<'_> {
    fn utc_now(&self) -> EngineResult<String> {
        NativeStatePlatform.utc_now()
    }
}
