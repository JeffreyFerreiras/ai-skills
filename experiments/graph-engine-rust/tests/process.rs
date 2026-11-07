#[path = "support/store.rs"]
mod store_fixture;

use graph_engine::{
    application::{
        error::EngineError,
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
    },
    compat::{self, PyString, Value},
    infrastructure::{command_line, process::NativeProcess},
};
use std::{path::PathBuf, time::Duration};

fn arguments(mode: &str) -> Vec<PyString> {
    vec![
        command_line::python_string(
            PathBuf::from(env!("CARGO_BIN_EXE_process-fixture")).as_os_str(),
        ),
        mode.into(),
    ]
}

#[test]
fn native_process_captures_raw_streams_exit_and_argument_codepoints() {
    let fixture = store_fixture::Fixture::new("process-streams");
    let args = arguments("emit");
    let outcome = NativeProcess
        .capture(&ProcessRequest {
            arguments: &args,
            directory: &fixture.repository,
            timeout: Duration::from_secs(5),
            stdout_limit: None,
            capture_stderr: true,
        })
        .unwrap();
    let ProcessOutcome::Completed {
        exit_code,
        stdout,
        stderr,
    } = outcome
    else {
        panic!("unexpected {outcome:?}")
    };
    assert_eq!(exit_code, 7);
    assert_eq!(stdout, b"out\0\xff");
    assert_eq!(stderr, b"err\0\xfe");
    let mut args = arguments("arguments");
    let values = [
        PyString::from("two words"),
        PyString::from("quote\"slash\\"),
        PyString(vec![0x3a3, 0x1f600]),
        PyString(vec![0xd800]),
    ];
    args.extend(values.iter().cloned());
    let outcome = NativeProcess.capture(&ProcessRequest {
        arguments: &args,
        directory: &fixture.repository,
        timeout: Duration::from_secs(5),
        stdout_limit: None,
        capture_stderr: true,
    });
    #[cfg(windows)]
    {
        let ProcessOutcome::Completed {
            exit_code: 0,
            stdout,
            stderr,
        } = outcome.unwrap()
        else {
            panic!("argument process failed")
        };
        assert!(stderr.is_empty());
        let result = compat::loads_value(&Value::Bytes(stdout)).unwrap();
        assert!(result.container_eq(&Value::list(values.into_iter().map(Value::String))));
    }
    #[cfg(unix)]
    assert!(matches!(outcome, Err(EngineError::Json(_))));
}

#[test]
fn native_process_timeout_and_output_ceiling_terminate_and_reap_children() {
    let fixture = store_fixture::Fixture::new("process-limits");
    let args = arguments("delay");
    let started = std::time::Instant::now();
    let outcome = NativeProcess
        .capture(&ProcessRequest {
            arguments: &args,
            directory: &fixture.repository,
            timeout: Duration::from_millis(250),
            stdout_limit: None,
            capture_stderr: true,
        })
        .unwrap();
    let ProcessOutcome::TimedOut { stdout, stderr } = outcome else {
        panic!("unexpected {outcome:?}")
    };
    let stdout = String::from_utf8(stdout).unwrap();
    let child_id: i64 = stdout.strip_prefix("ready:").unwrap().parse().unwrap();
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0},
            System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        };
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, child_id.try_into().unwrap()) };
        if process.is_null() {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(ERROR_INVALID_PARAMETER as i32)
            );
        } else {
            // An exited process remains openable while any handle refers to it.
            let status = unsafe { WaitForSingleObject(process, 0) };
            unsafe { CloseHandle(process) };
            assert_eq!(status, WAIT_OBJECT_0);
        }
    }
    #[cfg(unix)]
    {
        use graph_engine::application::persistent_store::StatePlatform;
        assert!(
            !graph_engine::infrastructure::state_platform::NativeStatePlatform
                .process_alive(&child_id.into())
                .unwrap()
        );
    }
    assert_eq!(stderr, b"waiting");
    assert!(started.elapsed() < Duration::from_secs(5));
    let args = arguments("flood");
    let outcome = NativeProcess
        .capture(&ProcessRequest {
            arguments: &args,
            directory: &fixture.repository,
            timeout: Duration::from_secs(5),
            stdout_limit: Some(1024),
            capture_stderr: false,
        })
        .unwrap();
    assert!(matches!(outcome, ProcessOutcome::OutputLimit));
}

#[test]
fn native_process_preserves_windows_unsigned_exit_code() {
    let fixture = store_fixture::Fixture::new("process-exit-code");
    let args = arguments("exit-high");
    let outcome = NativeProcess
        .capture(&ProcessRequest {
            arguments: &args,
            directory: &fixture.repository,
            timeout: Duration::from_secs(5),
            stdout_limit: None,
            capture_stderr: true,
        })
        .unwrap();
    let ProcessOutcome::Completed { exit_code, .. } = outcome else {
        panic!("unexpected {outcome:?}")
    };
    #[cfg(windows)]
    assert_eq!(exit_code, 4294967295);
    #[cfg(unix)]
    assert_eq!(exit_code, 255);
}

#[test]
fn native_process_distinguishes_spawn_os_failure_from_embedded_null_value_error() {
    let fixture = store_fixture::Fixture::new("process-failures");
    let args = vec![command_line::python_string(
        fixture.root.join("absent-executable.exe").as_os_str(),
    )];
    let outcome = NativeProcess.capture(&ProcessRequest {
        arguments: &args,
        directory: &fixture.repository,
        timeout: Duration::from_secs(5),
        stdout_limit: None,
        capture_stderr: true,
    });
    assert!(matches!(outcome, Err(EngineError::Io(_))));
    let args = vec![PyString::from("embedded\0null")];
    let outcome = NativeProcess.capture(&ProcessRequest {
        arguments: &args,
        directory: &fixture.repository,
        timeout: Duration::from_secs(5),
        stdout_limit: None,
        capture_stderr: true,
    });
    assert!(
        matches!(outcome, Err(EngineError::Python { kind: "ValueError", message }) if message.equals("embedded null character"))
    );
}
