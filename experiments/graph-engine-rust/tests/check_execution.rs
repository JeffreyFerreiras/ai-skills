use graph_engine::{
    application::{
        check_execution::{self, CheckExecution, CheckRuntimePort},
        check_receipts::CheckProvenancePort,
        error::{EngineError, EngineResult},
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
    },
    compat::{PyString, Value},
    domain::ids,
    infrastructure::check_runtime,
};
use std::{cell::RefCell, collections::VecDeque, path::Path, time::Duration};

struct Harness {
    calls: RefCell<Vec<String>>,
    outcomes: RefCell<VecDeque<EngineResult<ProcessOutcome>>>,
}
impl Harness {
    fn new(outcomes: Vec<EngineResult<ProcessOutcome>>) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            outcomes: RefCell::new(outcomes.into()),
        }
    }
}
impl ProcessPort for Harness {
    fn capture(&self, request: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome> {
        assert!(request.stdout_limit.is_none());
        assert!(request.capture_stderr);
        self.calls.borrow_mut().push(
            request
                .arguments
                .iter()
                .map(|arg| arg.utf8().unwrap())
                .collect::<Vec<_>>()
                .join("|"),
        );
        self.outcomes.borrow_mut().pop_front().unwrap()
    }
}
impl CheckProvenancePort for Harness {
    fn actor(&self) -> EngineResult<PyString> {
        self.calls.borrow_mut().push("actor".into());
        Ok("actor".into())
    }
    fn host_identity(&self) -> EngineResult<String> {
        self.calls.borrow_mut().push("host".into());
        Ok("host".into())
    }
    fn repository_worktree_digest(&self) -> EngineResult<String> {
        self.calls.borrow_mut().push("digest".into());
        Ok("d".repeat(64))
    }
}
impl CheckRuntimePort for Harness {
    fn utc_now(&self) -> EngineResult<String> {
        self.calls.borrow_mut().push("clock".into());
        Ok("2024-01-01T00:00:00Z".into())
    }
}

#[test]
fn local_check_receipt_preserves_execution_error_hashes_and_observation_order() {
    let cases = [
        (
            Ok(ProcessOutcome::Completed {
                exit_code: 0,
                stdout: b"out".to_vec(),
                stderr: b"err".to_vec(),
            }),
            0,
            false,
            b"out".as_slice(),
            b"err".as_slice(),
        ),
        (
            Ok(ProcessOutcome::TimedOut {
                stdout: b"partial".to_vec(),
                stderr: b"waiting".to_vec(),
            }),
            124,
            true,
            b"partial".as_slice(),
            b"waiting".as_slice(),
        ),
        (
            Err(EngineError::Io(std::io::Error::other(
                "synthetic OS failure",
            ))),
            127,
            false,
            b"".as_slice(),
            b"synthetic OS failure".as_slice(),
        ),
    ];
    for (outcome, code, timed_out, stdout, stderr) in cases {
        let harness = Harness::new(vec![outcome]);
        let value = check_execution::run_check(
            &CheckExecution {
                repository: Path::new("synthetic-repository"),
                run_id: &"RUN-1".into(),
                check_id: &"check-1".into(),
                command_id: &"command-1".into(),
                arguments: &["synthetic-command".into(), "argument".into()],
                timeout: Duration::from_secs(3),
                legacy_digest: true,
            },
            &harness,
            &harness,
        )
        .unwrap();
        assert_eq!(value["exit_code"].as_i64(), Some(code));
        assert!(value["timed_out"].py_eq(&Value::from(timed_out)));
        assert!(value["outcome"].is_text(if code == 0 { "PASS" } else { "FAIL" }));
        assert!(value["stdout_sha256"].is_text(&ids::sha256_bytes(stdout)));
        assert!(value["stderr_sha256"].is_text(&ids::sha256_bytes(stderr)));
        assert_eq!(
            *harness.calls.borrow(),
            [
                "clock",
                "synthetic-command|argument",
                "clock",
                "digest",
                "actor",
                "host"
            ]
        );
    }
}

#[test]
fn local_check_validates_identifiers_after_execution_and_can_omit_legacy_git_digest() {
    let harness = Harness::new(vec![Ok(ProcessOutcome::Completed {
        exit_code: 0,
        stdout: vec![],
        stderr: vec![],
    })]);
    let failure = check_execution::run_check(
        &CheckExecution {
            repository: Path::new("synthetic-repository"),
            run_id: &"invalid/run".into(),
            check_id: &"check-1".into(),
            command_id: &"command-1".into(),
            arguments: &["synthetic-command".into()],
            timeout: Duration::from_secs(3),
            legacy_digest: false,
        },
        &harness,
        &harness,
    )
    .unwrap_err();
    assert!(
        matches!(failure, EngineError::Contract(error) if error.field.equals("run_id") && error.code == "INVALID_ID")
    );
    assert_eq!(
        *harness.calls.borrow(),
        ["clock", "synthetic-command", "clock"]
    );
    let harness = Harness::new(vec![Ok(ProcessOutcome::Completed {
        exit_code: 0,
        stdout: vec![],
        stderr: vec![],
    })]);
    let value = check_execution::run_check(
        &CheckExecution {
            repository: Path::new("synthetic-repository"),
            run_id: &"RUN-1".into(),
            check_id: &"check-1".into(),
            command_id: &"command-1".into(),
            arguments: &["synthetic-command".into()],
            timeout: Duration::from_secs(3),
            legacy_digest: false,
        },
        &harness,
        &harness,
    )
    .unwrap();
    assert!(value["repo_worktree_sha256"].is_text(&"0".repeat(64)));
    assert_eq!(
        *harness.calls.borrow(),
        ["clock", "synthetic-command", "clock", "actor", "host"]
    );
}

#[test]
fn legacy_worktree_digest_keeps_exact_git_argv_order_and_independent_failures() {
    let harness = Harness::new(vec![
        Ok(ProcessOutcome::Completed {
            exit_code: 0,
            stdout: b"head\n".to_vec(),
            stderr: vec![],
        }),
        Ok(ProcessOutcome::Completed {
            exit_code: 1,
            stdout: vec![],
            stderr: b"bad".to_vec(),
        }),
        Ok(ProcessOutcome::TimedOut {
            stdout: vec![],
            stderr: vec![],
        }),
    ]);
    let result =
        check_runtime::repository_worktree_digest(Path::new("synthetic-repository"), &harness)
            .unwrap();
    let expected = Value::object([
        ("git", false.into()),
        ("head", "head\n".into()),
        ("status", Value::Null),
        ("diff", Value::Null),
    ]);
    assert_eq!(
        result,
        ids::sha256_bytes(&ids::canonical_bytes(&expected).unwrap())
    );
    assert_eq!(
        *harness.calls.borrow(),
        [
            "git|rev-parse|HEAD",
            "git|status|--porcelain=v1",
            "git|diff|--binary|HEAD|--"
        ]
    );
}
