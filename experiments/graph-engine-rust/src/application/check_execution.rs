use super::{
    check_receipts::CheckProvenancePort,
    error::{EngineError, EngineResult},
    process::{ProcessOutcome, ProcessPort, ProcessRequest},
};
use crate::{
    compat::{PyString, Value},
    domain::{contracts, ids},
};
use std::{path::Path, time::Duration};

pub trait CheckRuntimePort: CheckProvenancePort {
    fn utc_now(&self) -> EngineResult<String>;
}

pub struct CheckExecution<'a> {
    pub repository: &'a Path,
    pub run_id: &'a Value,
    pub check_id: &'a Value,
    pub command_id: &'a Value,
    pub arguments: &'a [PyString],
    pub timeout: Duration,
    pub legacy_digest: bool,
}

pub fn run_check(
    request: &CheckExecution<'_>,
    process: &dyn ProcessPort,
    runtime: &dyn CheckRuntimePort,
) -> EngineResult<Value> {
    let started = runtime.utc_now()?;
    let (exit_code, timed_out, stdout, stderr) = match process.capture(&ProcessRequest {
        arguments: request.arguments,
        directory: request.repository,
        timeout: request.timeout,
        stdout_limit: None,
        capture_stderr: true,
    }) {
        Ok(ProcessOutcome::Completed {
            exit_code,
            stdout,
            stderr,
        }) => (exit_code, false, stdout, stderr),
        Ok(ProcessOutcome::TimedOut { stdout, stderr }) => (124, true, stdout, stderr),
        Err(EngineError::Io(error)) => (127, false, Vec::new(), error.to_string().into_bytes()),
        Err(error) => return Err(error),
        Ok(ProcessOutcome::OutputLimit) => return Err(EngineError::state("SOURCE_UNVERIFIABLE")),
    };
    let finished = runtime.utc_now()?;
    let run_id = contracts::opaque(request.run_id, "run_id")?;
    let check_id = contracts::opaque(request.check_id, "check_id")?;
    let command_id = contracts::opaque(request.command_id, "command_id")?;
    let argv = request
        .arguments
        .iter()
        .map(|argument| {
            let value = Value::String(argument.clone());
            contracts::bounded_string(&value, "argv", 1024)?;
            Ok(value)
        })
        .collect::<EngineResult<Vec<_>>>()?;
    Ok(Value::object([
        ("schema_version", 1i64.into()),
        ("kind", "check_receipt".into()),
        ("run_id", run_id.into()),
        ("check_id", check_id.into()),
        ("command_id", command_id.into()),
        ("argv", Value::List(argv)),
        (
            "outcome",
            if exit_code == 0 { "PASS" } else { "FAIL" }.into(),
        ),
        ("exit_code", exit_code.into()),
        ("timed_out", timed_out.into()),
        ("stdout_sha256", ids::sha256_bytes(&stdout).into()),
        ("stderr_sha256", ids::sha256_bytes(&stderr).into()),
        (
            "repo_worktree_sha256",
            if request.legacy_digest {
                runtime.repository_worktree_digest()?
            } else {
                "0".repeat(64)
            }
            .into(),
        ),
        ("started_at", started.into()),
        ("finished_at", finished.into()),
        ("producer_actor", Value::String(runtime.actor()?)),
        ("producer_host_identity", runtime.host_identity()?.into()),
    ]))
}
