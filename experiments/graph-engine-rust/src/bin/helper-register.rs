use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        helper::HelperRegister,
        helper_repository::HelperRegisterRepository,
        policy::PolicyFilePort,
        snapshots::{self, SnapshotFailure},
        state::FaultHook,
    },
    compat::{
        self,
        argparse::{Arguments, ParseOutcome},
        PyString, SerializationProfile, Value,
    },
    domain::helper_register::MAX_RECORD_BYTES,
    infrastructure::{
        command_line,
        helper_files::NativeHelperFileTransactions,
        helper_traceback::{input_json_traceback, HelperInputSite},
        policy_files::NativePolicyFiles,
        state_platform::NativeStatePlatform,
        text_stream::Stream,
    },
};
use std::path::PathBuf;

struct CliFailure {
    error: EngineError,
    input_trace: Option<(HelperInputSite, PathBuf)>,
}

impl From<EngineError> for CliFailure {
    fn from(error: EngineError) -> Self {
        Self {
            error,
            input_trace: None,
        }
    }
}

fn traced_input(
    files: &dyn PolicyFilePort,
    argument: &PyString,
    installation: &std::path::Path,
    site: HelperInputSite,
) -> Result<Value, CliFailure> {
    input(files, argument).map_err(|error| {
        let input_trace = matches!(&error, EngineError::Json(error) if matches!(error.kind, "ValueError" | "RecursionError"))
            .then(|| (site, installation.to_path_buf()));
        CliFailure { error, input_trace }
    })
}

struct NoFault;
impl FaultHook for NoFault {
    fn observe(&self, _point: &str) -> EngineResult<()> {
        Ok(())
    }
}
fn input(files: &dyn PolicyFilePort, argument: &PyString) -> EngineResult<Value> {
    let candidate = files.resolve(&files.from_string(argument)?, true)?;
    let snapshot = snapshots::safe_json_snapshot(
        files,
        &candidate,
        &[candidate.parent().unwrap().to_path_buf()],
        MAX_RECORD_BYTES,
    )
    .map_err(|error| match error {
        SnapshotFailure::Contract(error) => EngineError::from(error),
        SnapshotFailure::Io(error) => error.into(),
        SnapshotFailure::Json(error) => error.into(),
    })?;
    Ok(snapshot.parsed.unwrap())
}
fn execute(arguments: &Arguments) -> Result<Value, CliFailure> {
    let files = NativePolicyFiles::default();
    let platform = NativeStatePlatform;
    let transactions = NativeHelperFileTransactions;
    let installation = files.resolve(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/software-engineering-graph"),
        false,
    )?;
    let registry = HelperRegister {
        repository: HelperRegisterRepository {
            files: &files,
            platform: &platform,
            transactions: &transactions,
            installation: &installation,
            fault: &NoFault,
        },
    };
    let path = |field| files.from_string(arguments.get(field));
    if arguments.command == "initialize" {
        return registry
            .initialize(
                &path("state-root")?,
                &path("repo")?,
                &Value::String(arguments.get("run-id").clone()),
                &path("plan")?,
                &path("allowance")?,
                &path("host-observation")?,
            )
            .map_err(Into::into);
    }
    let context = traced_input(
        &files,
        arguments.get("context"),
        &installation,
        HelperInputSite::Context,
    )?;
    let register = path("register")?;
    match arguments.command {
        "preflight" => registry
            .preflight(
                &register,
                &context,
                &traced_input(
                    &files,
                    arguments.get("request"),
                    &installation,
                    HelperInputSite::PreflightRequest,
                )?,
            )
            .map_err(Into::into),
        "reserve" => registry
            .reserve(
                &register,
                &context,
                &traced_input(
                    &files,
                    arguments.get("request"),
                    &installation,
                    HelperInputSite::ReserveRequest,
                )?,
            )
            .map_err(Into::into),
        "settle" => registry
            .settle(
                &register,
                &context,
                &traced_input(
                    &files,
                    arguments.get("settlement"),
                    &installation,
                    HelperInputSite::SettleSettlement,
                )?,
            )
            .map_err(Into::into),
        _ => registry.status(&register, &context).map_err(Into::into),
    }
}
fn failure(code: &str) -> Value {
    Value::object([("ok", false.into()), ("code", code.into())])
}
fn main() {
    let arguments = match compat::argparse::helper_arguments(
        &command_line::arguments(),
        command_line::terminal_columns(),
    ) {
        ParseOutcome::Arguments(arguments) => arguments,
        ParseOutcome::Exit {
            code,
            stdout,
            stderr,
        } => {
            if !stdout.0.is_empty() {
                command_line::write_text(&mut std::io::stdout().lock(), &stdout, Stream::Stdout)
                    .unwrap();
            }
            if !stderr.0.is_empty() {
                command_line::write_text(&mut std::io::stderr().lock(), &stderr, Stream::Stderr)
                    .unwrap();
            }
            std::process::exit(code);
        }
    };
    let (result, code) = match execute(&arguments) {
        Ok(result) => {
            let code = if result.get("ok").is_some_and(Value::truthy) {
                0
            } else {
                3
            };
            (result, code)
        }
        Err(CliFailure {
            error: EngineError::Contract(error),
            ..
        }) => (failure(&error.code), 4),
        Err(CliFailure {
            error: EngineError::Helper(error),
            ..
        }) => (failure(&error.code), 4),
        Err(CliFailure {
            error: EngineError::State(error),
            ..
        }) => (failure(&error.code), 4),
        Err(CliFailure {
            error: EngineError::Io(_),
            ..
        }) => (failure("IO_FAILURE"), 5),
        Err(error) => {
            let message = match error.error {
                EngineError::Python { kind, message } => {
                    let mut text = PyString::from(format!("{kind}: "));
                    text.0.extend(message.0);
                    text.0.push(10);
                    text
                }
                EngineError::Json(json) => match error.input_trace {
                    Some((site, installation)) => {
                        input_json_traceback(&installation, site, json.kind, &json.message.into())
                    }
                    None => format!("{}: {}\n", json.kind, json.message).into(),
                },
                other => format!("{other:?}\n").into(),
            };
            command_line::write_text(&mut std::io::stderr().lock(), &message, Stream::Stderr)
                .unwrap();
            std::process::exit(1);
        }
    };
    let mut output = compat::dumps(&result, SerializationProfile::CLI).unwrap();
    output.push(b'\n');
    command_line::write_bytes(&mut std::io::stdout().lock(), &output).unwrap();
    std::process::exit(code);
}
