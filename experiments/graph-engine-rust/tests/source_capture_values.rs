use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
        source_capture::{capture_source_values, SourceFilePort, SourceMetadata},
    },
    compat::{PyString, Value},
    domain::{contracts::ContractError, ids},
};
use std::{cell::RefCell, collections::VecDeque, path::Path, time::Duration};

struct Files {
    rejected: Option<PyString>,
    normalized: RefCell<Vec<PyString>>,
}

impl Files {
    fn new(rejected: Option<&str>) -> Self {
        Self {
            rejected: rejected.map(PyString::from),
            normalized: RefCell::new(vec![]),
        }
    }
}

impl SourceFilePort for Files {
    fn normalize_target(&self, path: &PyString) -> EngineResult<PyString> {
        self.normalized.borrow_mut().push(path.clone());
        if self.rejected.as_ref() == Some(path) {
            return Err(ContractError::new("target_ref", "DENIED_PATH").into());
        }
        Ok(path.clone())
    }
    fn metadata(&self, _: &PyString) -> EngineResult<Option<SourceMetadata>> {
        Ok(None)
    }
    fn snapshot(&self, _: &PyString, _: usize) -> EngineResult<(usize, String)> {
        panic!("absent-file fixture must not snapshot")
    }
    fn elapsed(&self) -> Duration {
        Duration::ZERO
    }
    fn windows(&self) -> bool {
        cfg!(windows)
    }
}

struct Process {
    outputs: RefCell<VecDeque<Vec<u8>>>,
    calls: RefCell<Vec<Vec<PyString>>>,
}

impl Process {
    fn new(outputs: &[&[u8]]) -> Self {
        Self {
            outputs: RefCell::new(outputs.iter().map(|bytes| bytes.to_vec()).collect()),
            calls: RefCell::new(vec![]),
        }
    }
}

impl ProcessPort for Process {
    fn capture(&self, request: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome> {
        self.calls.borrow_mut().push(request.arguments.to_vec());
        Ok(ProcessOutcome::Completed {
            exit_code: 0,
            stdout: self
                .outputs
                .borrow_mut()
                .pop_front()
                .expect("unexpected process request"),
            stderr: vec![],
        })
    }
}

fn capture(
    roots: &Value,
    outputs: &Value,
    known: &Value,
    files: &Files,
    process: &Process,
) -> EngineResult<Value> {
    capture_source_values(
        Path::new("owned-synthetic-repository"),
        roots,
        outputs,
        known,
        files,
        process,
    )
}

#[test]
fn false_roots_fail_before_normalization_or_process_requests() {
    for roots in [Value::Null, false.into(), Value::List(vec![]), "".into()] {
        let files = Files::new(None);
        let process = Process::new(&[]);
        let result = capture(&roots, &Value::List(vec![]), &Value::Null, &files, &process);
        assert!(
            matches!(result, Err(EngineError::State(error)) if error.code == "SOURCE_UNVERIFIABLE")
        );
        assert!(files.normalized.borrow().is_empty());
        assert!(process.calls.borrow().is_empty());
    }
}

#[test]
fn earlier_root_contract_error_precedes_a_later_root_type_error() {
    let files = Files::new(Some("rejected"));
    let process = Process::new(&[]);
    let result = capture(
        &Value::List(vec!["rejected".into(), 1_i64.into()]),
        &Value::List(vec![]),
        &Value::Null,
        &files,
        &process,
    );
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "SOURCE_UNVERIFIABLE")
    );
    assert_eq!(
        files.normalized.borrow().as_slice(),
        &[PyString::from("rejected")]
    );
    assert!(process.calls.borrow().is_empty());
}

#[test]
fn later_root_type_error_follows_prior_normalization_without_process_requests() {
    let files = Files::new(None);
    let process = Process::new(&[]);
    let result = capture(
        &Value::List(vec!["docs/".into(), 1_i64.into()]),
        &Value::List(vec![]),
        &Value::Null,
        &files,
        &process,
    );
    assert!(
        matches!(result, Err(EngineError::Python { kind: "TypeError", message }) if message.equals("can only concatenate str (not \"int\") to str"))
    );
    assert_eq!(
        files.normalized.borrow().as_slice(),
        &[PyString::from("docs/")]
    );
    assert!(process.calls.borrow().is_empty());
}

#[test]
fn malformed_known_input_fails_after_the_ignored_enumeration_request() {
    let files = Files::new(None);
    let process = Process::new(&[b"abc\n", b"", b"", b""]);
    let result = capture(
        &Value::List(vec!["docs/".into()]),
        &Value::List(vec![]),
        &Value::List(vec![1_i64.into()]),
        &files,
        &process,
    );
    assert!(
        matches!(result, Err(EngineError::Python { kind: "AttributeError", message }) if message.equals("'int' object has no attribute 'endswith'"))
    );
    assert_eq!(process.calls.borrow().len(), 4);
    assert!(process.calls.borrow()[3]
        .iter()
        .any(|argument| argument.equals("--ignored")));
    assert!(process.outputs.borrow().is_empty());
}

#[test]
fn malformed_output_tuple_is_rejected_after_initial_source_enumeration() {
    let files = Files::new(None);
    let process = Process::new(&[b"abc\n", b"", b""]);
    let result = capture(
        &Value::List(vec!["docs/".into()]),
        &Value::Tuple(vec![Value::Tuple(vec![])]),
        &Value::Null,
        &files,
        &process,
    );
    assert!(
        matches!(result, Err(EngineError::Python { kind: "TypeError", message }) if message.equals("tuple indices must be integers or slices, not str"))
    );
    assert_eq!(process.calls.borrow().len(), 3);
    assert!(process.outputs.borrow().is_empty());
}

#[test]
fn head_movement_with_unchanged_paths_is_unverifiable() {
    let files = Files::new(None);
    let process = Process::new(&[b"abc\n", b"", b"", b"", b"", b"def\n"]);
    let result = capture(
        &Value::List(vec!["docs/".into()]),
        &Value::List(vec![]),
        &Value::Null,
        &files,
        &process,
    );
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "SOURCE_UNVERIFIABLE")
    );
    let calls = process.calls.borrow();
    assert_eq!(calls.last(), calls.first());
    assert!(process.outputs.borrow().is_empty());
}

#[test]
fn string_root_iteration_and_tuple_roots_preserve_the_source_capture_contract() {
    for (roots, expected_root, records) in [
        (
            Value::from("a"),
            "a",
            vec![Value::object([
                ("path", "a".into()),
                ("index", Value::List(vec![])),
                ("exists", false.into()),
            ])],
        ),
        (Value::Tuple(vec!["docs/".into()]), "docs/", vec![]),
    ] {
        let files = Files::new(None);
        let process = Process::new(&[b"abc\n", b"", b"", b"", b"", b"abc\n"]);
        let result = capture(
            &roots,
            &Value::Tuple(vec![]),
            &Value::Null,
            &files,
            &process,
        )
        .unwrap();
        assert!(result["roots"].as_list().unwrap()[0].is_text(expected_root));
        assert!(result["path_count"].py_eq(&(records.len() as i64).into()));
        let expected = ids::sha256_bytes(
            &ids::canonical_bytes(&Value::object([
                ("head", "abc".into()),
                ("paths", Value::List(records)),
            ]))
            .unwrap(),
        );
        assert!(result["sha256"].is_text(&expected));
        assert_eq!(process.calls.borrow().len(), 6);
        assert!(process.outputs.borrow().is_empty());
    }
}
