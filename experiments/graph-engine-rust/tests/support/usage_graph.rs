#[path = "initialized_graph.rs"]
mod initialized_graph;
use graph_engine::{
    application::error::EngineResult,
    compat::{self, PyString, SerializationProfile, Value},
};
use initialized_graph::InitializedGraph;
use std::{cell::Cell, fs, io::Write, path::PathBuf};

pub struct UsageGraph {
    pub graph: InitializedGraph,
    pub log: PathBuf,
    counter: Cell<usize>,
}
impl UsageGraph {
    pub fn new() -> Self {
        let graph = InitializedGraph::new();
        let log = graph.graph.root.join("private-session.jsonl");
        fs::write(&log, b"").unwrap();
        let fixture = Self {
            graph,
            log,
            counter: Cell::new(0),
        };
        fixture.append(
            "session_meta",
            Value::object([
                ("id", "synthetic-session".into()),
                ("cwd", "PRIVATE_SOURCE_PATH".into()),
            ]),
        );
        fixture.context("gpt-6-astra", "medium");
        fixture
    }
    pub fn append(&self, kind: &str, payload: Value) {
        let mut stream = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.log)
            .unwrap();
        stream
            .write_all(
                &compat::dumps(
                    &Value::object([("type", kind.into()), ("payload", payload)]),
                    SerializationProfile::STORED,
                )
                .unwrap(),
            )
            .unwrap();
        stream
            .write_all(if cfg!(windows) { b"\r\n" } else { b"\n" })
            .unwrap();
    }
    pub fn context(&self, model: &str, effort: &str) {
        self.append(
            "turn_context",
            Value::object([("model", model.into()), ("effort", effort.into())]),
        );
    }
    pub fn tokens(&self, input: i64, output: i64, last: Option<(i64, i64)>) {
        self.tokens_with(input, output, last, &[]);
    }
    pub fn tokens_with(
        &self,
        input: i64,
        output: i64,
        last: Option<(i64, i64)>,
        optional: &[(&str, Value)],
    ) {
        let counters = |input: i64, output: i64| {
            Value::object([
                ("input_tokens", input.into()),
                ("output_tokens", output.into()),
                ("total_tokens", (input + output).into()),
            ])
        };
        let (last_input, last_output) = last.unwrap_or((input, output));
        let mut total = counters(input, output);
        for (key, value) in optional {
            total.set(key, value.clone());
        }
        let last = if last.is_none() {
            total.clone()
        } else {
            counters(last_input, last_output)
        };
        self.append(
            "event_msg",
            Value::object([
                ("type", "token_count".into()),
                (
                    "info",
                    Value::object([("total_token_usage", total), ("last_token_usage", last)]),
                ),
            ]),
        );
    }
    pub fn initialize(&self) {
        self.graph
            .initialize(
                &self.graph.graph.task("delivery", "full_delivery", &[]),
                true,
            )
            .unwrap();
    }
    pub fn usage(&self, action: &str, operation: &str, extra: &[PyString]) -> EngineResult<Value> {
        let mut arguments = vec![
            "record".into(),
            "usage".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--action".into(),
            action.into(),
            "--session-log".into(),
            self.log.to_str().unwrap().into(),
            "--op-id".into(),
            operation.into(),
        ];
        arguments.extend_from_slice(extra);
        self.graph.graphctl(&arguments)
    }
    pub fn bind(&self) -> EngineResult<Value> {
        self.bind_with("scoping", "bind-usage", None, None)
    }
    pub fn bind_with(
        &self,
        phase: &str,
        operation: &str,
        checkpoint: Option<&Value>,
        branch: Option<&Value>,
    ) -> EngineResult<Value> {
        let mut arguments = if let Some(branch) = branch {
            vec![
                "--branch-id".into(),
                branch["branch_id"].as_string().unwrap().clone(),
                "--attempt-id".into(),
                branch["attempt_id"].as_string().unwrap().clone(),
            ]
        } else {
            vec![
                "--phase".into(),
                phase.into(),
                "--generation".into(),
                "0".into(),
            ]
        };
        if let Some(checkpoint) = checkpoint {
            arguments.extend([
                "--start-offset".into(),
                checkpoint["offset"]
                    .strict_integer()
                    .unwrap()
                    .to_string()
                    .into(),
                "--source-id".into(),
                checkpoint["source_id"].as_string().unwrap().clone(),
                "--prefix-sha256".into(),
                checkpoint["prefix_sha256"].as_string().unwrap().clone(),
            ]);
        }
        self.usage("bind", operation, &arguments)
    }
    pub fn status(&self) -> Value {
        self.graph
            .graphctl(&["status".into(), "--run-id".into(), "RUN-1".into()])
            .unwrap()
    }
    pub fn claim_raw(&self) -> Value {
        let status = self.status();
        assert!(!status["next_action"]["kind"].is_text("record_fanout_assessment"));
        self.counter.set(self.counter.get() + 1);
        self.graph
            .graphctl(&[
                "next".into(),
                "--run-id".into(),
                "RUN-1".into(),
                "--claim".into(),
                "--op-id".into(),
                format!("claim-{}", self.counter.get()).into(),
            ])
            .unwrap()["branch"]
            .clone()
    }
    pub fn collect(&self, binding: &Value, action: &str, operation: &str) -> EngineResult<Value> {
        self.usage(
            action,
            operation,
            &["--binding-id".into(), binding.as_string().unwrap().clone()],
        )
    }
    pub fn tear_down(self) {
        self.graph.tear_down();
    }
}
