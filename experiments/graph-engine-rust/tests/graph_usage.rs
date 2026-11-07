#[path = "support/usage_graph.rs"]
mod usage_graph;
use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_cli::ExecuteOutcome,
        usage_source::{UsageBytes, UsageSourcePort},
    },
    compat::{self, PyString, SerializationProfile, Value},
    domain::usage::Limits,
};
use std::{cell::Cell, path::Path};
use usage_graph::UsageGraph;

#[test]
fn checkpoint_executes_before_repository_policy_or_run_loading() {
    let fixture = UsageGraph::new();
    fixture.tokens(100, 20, None);
    let outcome = fixture
        .graph
        .execute(&[
            "usage".into(),
            "checkpoint".into(),
            "--session-log".into(),
            fixture.log.to_str().unwrap().into(),
        ])
        .unwrap();
    let ExecuteOutcome::Response { result, exit_code } = outcome else {
        panic!("unexpected parser exit");
    };
    assert_eq!(exit_code, 0);
    assert!(result["checkpoint_schema_version"].py_eq(&1_i64.into()));
    assert!(!compat::dumps_text(&result, SerializationProfile::CLI)
        .unwrap()
        .utf8()
        .unwrap()
        .contains("synthetic-session"));
    fixture.tear_down();
}

#[test]
fn cli_usage_bind_collect_and_close_keep_exact_deltas_and_sanitize_source_metadata() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let bound = fixture.bind().unwrap();
    fixture.tokens(20, 2, Some((10, 1)));
    let collected = fixture
        .collect(&bound["binding_id"], "collect", "collect-usage")
        .unwrap();
    assert!(collected["usage"]["observed_totals"]["total_tokens"].py_eq(&11_i64.into()));
    let closed = fixture
        .collect(&bound["binding_id"], "close", "close-usage")
        .unwrap();
    assert!(closed["usage"]["open_bindings"]
        .as_list()
        .unwrap()
        .is_empty());
    let text = compat::dumps_text(&closed, SerializationProfile::CLI)
        .unwrap()
        .utf8()
        .unwrap();
    for private in [
        "PRIVATE_",
        "synthetic-session",
        fixture.log.to_str().unwrap(),
    ] {
        assert!(!text.contains(private));
    }
    fixture.tear_down();
}

#[test]
fn usage_replay_keeps_the_original_checkpoint_after_new_appends() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let first = fixture.bind().unwrap();
    fixture.tokens(20, 2, Some((10, 1)));
    let mut expected = first.clone();
    expected.set("code", "REPLAYED".into());
    assert!(fixture.bind().unwrap().container_eq(&expected));
    fixture.tear_down();
}

struct NoRead(Cell<usize>);
impl UsageSourcePort for NoRead {
    fn read_prefix(&self, _: &Path, _: Limits) -> EngineResult<UsageBytes> {
        self.0.set(self.0.get() + 1);
        Err(EngineError::Injected("unexpected source read".into()))
    }
}

#[test]
fn usage_prelude_rejects_invalid_operation_and_partial_checkpoint_without_source_reads() {
    let fixture = UsageGraph::new();
    fixture.initialize();
    let files = NoRead(Cell::new(0));
    let mut arguments: Vec<PyString> = vec![
        "--repo".into(),
        fixture.graph.graph.repository.to_str().unwrap().into(),
        "record".into(),
        "usage".into(),
        "--run-id".into(),
        "RUN-1".into(),
        "--action".into(),
        "bind".into(),
        "--session-log".into(),
        fixture.log.to_str().unwrap().into(),
        "--op-id".into(),
        "invalid?".into(),
        "--phase".into(),
        "scoping".into(),
        "--generation".into(),
        "0".into(),
    ];
    assert!(
        matches!(fixture.graph.execute_with_usage_source(&arguments, &fixture.graph.graph.store, &files), Err(EngineError::Contract(error)) if error.field.equals("op_id") && error.code == "INVALID_ID")
    );
    arguments[11] = "partial-usage".into();
    arguments.extend(["--start-offset".into(), "0".into()]);
    assert!(
        matches!(fixture.graph.execute_with_usage_source(&arguments, &fixture.graph.graph.store, &files), Err(EngineError::State(error)) if error.code == "USAGE_CHECKPOINT_INVALID")
    );
    assert_eq!(files.0.get(), 0);
    fixture.tear_down();
}
