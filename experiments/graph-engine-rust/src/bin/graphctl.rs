use graph_engine::{
    application::{
        claim_runtime::ClaimServices,
        error::{EngineError, EngineResult, GraphctlTraceSite},
        graph_cli::{self, CliServices, ExecuteOutcome},
        graph_response::json_result,
        policy::PolicyFilePort,
        state::FaultHook,
    },
    compat::{self, PyString, SerializationProfile, Value},
    infrastructure::{
        check_runtime::NativeCheckRuntime, claim_runtime::NativeClaimRuntime, command_line,
        graph_traceback::graphctl_traceback, policy_files::NativePolicyFiles,
        process::NativeProcess, source_files::NativeSourceFileFactory,
        state_database::NativeStateDatabase, state_platform::NativeStatePlatform,
        text_stream::Stream, usage_files::NativeUsageSource,
    },
};
use std::{
    io,
    path::{Path, PathBuf},
};

struct NoFault;

impl FaultHook for NoFault {
    fn observe(&self, _point: &str) -> EngineResult<()> {
        Ok(())
    }
}

fn installation_root() -> PathBuf {
    graph_engine::infrastructure::installation::root()
}

fn execute(installation: &Path) -> EngineResult<ExecuteOutcome> {
    let request =
        match graph_cli::parse(&command_line::arguments(), command_line::terminal_columns()) {
            Ok(request) => request,
            Err(exit) => return Ok(exit),
        };
    let files = NativePolicyFiles::default();
    let repository = request
        .repository()
        .map(|argument| files.from_string(argument))
        .transpose()?;
    graph_cli::execute_parsed(
        request,
        None,
        &CliServices {
            files: &files,
            snapshots: &files,
            provenance: &NativeCheckRuntime {
                repository: repository.as_deref().unwrap_or_else(|| Path::new("")),
            },
            skill: installation,
            installation,
            platform: &NativeStatePlatform,
            database: &NativeStateDatabase,
            fault: &NoFault,
            claims: ClaimServices {
                runtime: &NativeClaimRuntime,
                source_files: &NativeSourceFileFactory,
                process: &NativeProcess,
            },
            usage_files: &NativeUsageSource,
        },
    )
}

fn failure(code: &str) -> Value {
    json_result(false, code, Value::Null, Value::Null)
}

fn state_exit(code: &str) -> i32 {
    match code {
        "OPERATION_CONFLICT"
        | "TRANSACTION_CONFLICT"
        | "CLAIM_CONFLICT"
        | "INITIALIZATION_CONFLICT"
        | "RUN_ALREADY_EXISTS"
        | "INCOMPLETE_INIT_CONFLICT"
        | "FANOUT_ASSESSMENT_EXISTS"
        | "FANOUT_IMMUTABLE"
        | "FANOUT_DEPENDENCY_IMMUTABLE" => 5,
        "GRAPH_BLOCKED" | "RETRY_LIMIT" | "BUDGET_LIMIT" => 3,
        "NOT_READY"
        | "RETRY_REQUIRED"
        | "EXECUTION_PLAN_APPROVAL_REQUIRED"
        | "FANOUT_ASSESSMENT_REQUIRED" => 2,
        _ => 4,
    }
}

fn diagnostic(error: EngineError, installation: &Path) -> PyString {
    let (site, kind, message) = match error {
        EngineError::TracedGraphctl {
            site,
            kind,
            message,
        } => (Some(site), kind, message),
        EngineError::TracedGraphctlJson { site, error } => (
            Some(GraphctlTraceSite::SnapshotJson(site)),
            error.kind,
            error.message.into(),
        ),
        EngineError::Python { kind, message } => (None, kind, message),
        EngineError::Json(error) => (None, error.kind, error.message.into()),
        EngineError::Helper(error) => (None, "HelperRegisterError", error.code.into()),
        _ => (
            None,
            "RuntimeError",
            "graph engine execution interrupted".into(),
        ),
    };
    if let Some(site) = site {
        let resolved = NativePolicyFiles::default().resolve(installation, true);
        return graphctl_traceback(
            resolved.as_deref().unwrap_or(installation),
            site,
            kind,
            &message,
        );
    }
    let mut output = PyString::from(format!("{kind}: "));
    output.0.extend(message.0);
    output.0.push(10);
    output
}

fn write_outcome(outcome: EngineResult<ExecuteOutcome>, installation: &Path) -> io::Result<i32> {
    let (result, code) = match outcome {
        Ok(ExecuteOutcome::Response { result, exit_code }) => (result, exit_code),
        Ok(ExecuteOutcome::ParserExit {
            exit_code,
            stdout,
            stderr,
        }) => {
            command_line::write_text(&mut io::stdout().lock(), &stdout, Stream::Stdout)?;
            command_line::write_text(&mut io::stderr().lock(), &stderr, Stream::Stderr)?;
            return Ok(exit_code);
        }
        Err(EngineError::Contract(error)) => {
            let mut result = failure(&error.code);
            result.set("field", Value::String(error.field));
            (result, 4)
        }
        Err(EngineError::State(error)) => (failure(&error.code), state_exit(&error.code)),
        Err(EngineError::Io(_) | EngineError::Storage { .. }) => {
            (failure("IO_OR_TRANSACTION_FAILURE"), 5)
        }
        Err(error) => {
            command_line::write_text(
                &mut io::stderr().lock(),
                &diagnostic(error, installation),
                Stream::Stderr,
            )?;
            return Ok(1);
        }
    };
    let mut output = compat::dumps(&result, SerializationProfile::CLI).map_err(io::Error::other)?;
    output.push(b'\n');
    command_line::write_bytes(&mut io::stdout().lock(), &output)?;
    Ok(code)
}

fn main() {
    let installation = installation_root();
    let code = write_outcome(execute(&installation), &installation).unwrap_or_else(|error| {
        eprintln!("{error}");
        1
    });
    std::process::exit(code);
}
