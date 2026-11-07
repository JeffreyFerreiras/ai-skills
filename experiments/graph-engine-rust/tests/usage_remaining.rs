#[allow(dead_code)]
#[path = "support/usage_graph.rs"]
mod usage_graph;

#[cfg(windows)]
use graph_engine::infrastructure::usage_files::requested_open_flags;
use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_cli::ExecuteOutcome,
        semantic_state::verify_semantic_state,
        state::GraphSession,
        usage_record::{self, UsageArguments},
        usage_source::{self, UsageBytes, UsageSourcePort},
    },
    compat::{self, PyString, SerializationProfile, Value},
    domain::{
        errors::ValidationFailure,
        ids,
        usage::{self, Context, Limits},
        usage_contracts::validate_binding,
    },
    infrastructure::usage_files::{
        read_prefix_observed, NativeUsageSource, UsageFault, UsageReadObserver, UsageReadStage,
    },
};
use std::{cell::RefCell, fs, io::Write, path::Path, process::Command};
use usage_graph::UsageGraph;

fn total(summary: &Value, expected: i64) {
    assert!(summary["observed_totals"]["total_tokens"].py_eq(&expected.into()));
}

fn has_code(summary: &Value, code: &str) {
    assert!(summary["diagnostics"]
        .as_list()
        .unwrap()
        .iter()
        .any(|value| value.is_text(code)));
}

fn state_error(result: EngineResult<Value>, expected: &str) {
    let error = result.unwrap_err();
    assert!(
        matches!(&error, EngineError::State(error) if error.code == expected),
        "{error:?}"
    );
}

fn close_or_collect(fixture: &UsageGraph, binding: &Value, op: &str, close: bool) -> Value {
    fixture
        .collect(binding, if close { "close" } else { "collect" }, op)
        .unwrap()["usage"]
        .clone()
}

struct ByteLimit;

impl UsageSourcePort for ByteLimit {
    fn read_prefix(&self, path: &Path, limits: Limits) -> EngineResult<UsageBytes> {
        NativeUsageSource.read_prefix(
            path,
            Limits {
                max_bytes: 1,
                ..limits
            },
        )
    }
}

#[test]
fn test_metadata_limits_malformed_nested_and_unfinished_records_are_private() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&fixture.log)
            .unwrap();
        file.write_all(b"{\"PRIVATE_CREDENTIAL\":\"secret\",broken}\n")
            .unwrap();
        file.write_all(&[b'['; 30]).unwrap();
        file.write_all(b"\"PRIVATE_NESTED_SECRET\"").unwrap();
        file.write_all(&[b']'; 30]).unwrap();
        file.write_all(b"\n").unwrap();
    }
    fixture.context("PRIVATE_MODEL", "PRIVATE_EFFORT");
    fixture.tokens(20, 2, Some((10, 1)));
    fs::OpenOptions::new()
        .append(true)
        .open(&fixture.log)
        .unwrap()
        .write_all(b"{\"type\":\"event_msg\",\"payload\":")
        .unwrap();
    let report = close_or_collect(&fixture, &binding, "collect-usage", false);
    total(&report, 11);
    has_code(&report, "malformed_record");
    let mut saved = fixture.graph.with_session(|session, _| {
        let mut values = Vec::new();
        for query in [
            "SELECT detail_json FROM events WHERE event_type LIKE '%usage%'",
            "SELECT response_json FROM operations WHERE operation_id IN ('bind-usage','collect-usage')",
        ] {
            let mut query = session.connection().prepare(query).unwrap();
            let rows = query.query_map([], |row| row.get::<_, String>(0)).unwrap();
            values.extend(rows.map(Result::unwrap));
        }
        Ok(values.join("\n"))
    }).unwrap();
    saved.push_str(
        &compat::dumps_text(&report, SerializationProfile::CLI)
            .unwrap()
            .utf8()
            .unwrap(),
    );
    for forbidden in [
        "PRIVATE_",
        "synthetic-session",
        fixture.log.to_str().unwrap(),
        "secret",
    ] {
        assert!(!saved.contains(forbidden), "{forbidden}");
    }
    let repo = fixture.graph.graph.repository.to_str().unwrap();
    let binding_id = binding.text().unwrap();
    let result = fixture
        .graph
        .execute_with_usage_source(
            &[
                "--repo",
                repo,
                "record",
                "usage",
                "--run-id",
                "RUN-1",
                "--action",
                "collect",
                "--session-log",
                fixture.log.to_str().unwrap(),
                "--op-id",
                "limited",
                "--binding-id",
                &binding_id,
            ]
            .map(PyString::from),
            &fixture.graph.graph.store,
            &ByteLimit,
        )
        .unwrap();
    let ExecuteOutcome::Response { result, exit_code } = result else {
        panic!("usage parser exit")
    };
    assert_eq!(exit_code, 0);
    has_code(&result["usage"], "source_limit");
    total(&result["usage"], 11);
    for limits in [
        Limits {
            max_records: 2,
            ..Limits::default()
        },
        Limits {
            chunk_bytes: 40,
            ..Limits::default()
        },
    ] {
        let source = usage_source::read_source(&NativeUsageSource, &fixture.log, limits).unwrap();
        assert!(source
            .diagnostics
            .iter()
            .any(|(_, code)| code == "record_limit"));
    }
    fixture.tear_down();
}

struct ObservedUsage<F>(F);

impl<F: UsageReadObserver> UsageSourcePort for ObservedUsage<F> {
    fn read_prefix(&self, path: &Path, limits: Limits) -> EngineResult<UsageBytes> {
        read_prefix_observed(path, limits, &self.0)
    }
}

#[test]
fn test_opened_source_identity_change_and_symlink_are_rejected() {
    let fixture = UsageGraph::new();
    let changed = ObservedUsage(|stage, _: &Path, _| {
        Ok(UsageFault {
            opened_identity_changed: stage == UsageReadStage::Opened,
            ..UsageFault::default()
        })
    });
    let limits = Limits::default();
    let source = usage_source::read_source(&changed, &fixture.log, limits).unwrap();
    assert!(source.source_id.is_none());
    assert!(source
        .diagnostics
        .iter()
        .any(|(_, code)| code == "source_changed"));
    let link = fixture.graph.graph.root.join("linked-session");
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&fixture.log, &link);
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&fixture.log, &link);
    let source = if linked.is_ok() {
        usage_source::read_source(&NativeUsageSource, &link, limits).unwrap()
    } else {
        let guarded = ObservedUsage(|stage, _: &Path, _| {
            Ok(UsageFault {
                unsafe_component: stage == UsageReadStage::SafeComponents,
                ..UsageFault::default()
            })
        });
        println!("Usage symlink fixture used the original permission-denied guard fallback");
        usage_source::read_source(&guarded, &fixture.log, limits).unwrap()
    };
    assert!(source
        .diagnostics
        .iter()
        .any(|(_, code)| code == "source_unsafe"));
    fixture.tear_down();
}

#[test]
fn raced_nonregular_source_is_rejected_with_native_flags() {
    let fixture = UsageGraph::new();
    let opens = RefCell::new(Vec::new());
    let observer = ObservedUsage(|stage, _: &Path, flags| {
        if stage == UsageReadStage::BeforeOpen {
            opens.borrow_mut().push(flags);
        }
        Ok(UsageFault {
            opened_nonregular: stage == UsageReadStage::Opened,
            ..UsageFault::default()
        })
    });
    let source = usage_source::read_source(&observer, &fixture.log, Limits::default()).unwrap();
    let opens = opens.into_inner();
    assert_eq!(opens.len(), 1);
    #[cfg(unix)]
    assert_ne!(opens[0] & libc::O_NONBLOCK, 0);
    #[cfg(windows)]
    assert_eq!(opens[0], requested_open_flags(0));
    assert!(source.source_id.is_none());
    assert_eq!(source.diagnostics, [(0, "source_changed".to_owned())]);
    fixture.tear_down();
}

#[test]
fn test_concurrent_candidate_cannot_overwrite_newer_checkpoint() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    fixture.tokens(20, 2, Some((10, 1)));
    fixture
        .graph
        .with_session(|session, context| {
            let run = session.run("RUN-1")?.unwrap();
            let absent = &Value::Null;
            let candidate = usage_record::prepare(
                UsageArguments {
                    operation_id: &"loser".into(),
                    action: &"collect".into(),
                    session_log: &fixture.log,
                    start_offset: absent,
                    source_id: absent,
                    prefix_sha256: absent,
                    binding_id: &binding,
                    branch_id: absent,
                    attempt_id: absent,
                    phase: absent,
                    generation: absent,
                },
                session,
                "RUN-1",
                &run,
                &NativeUsageSource,
            )?;
            fixture.collect(&binding, "collect", "winner")?;
            state_error(
                candidate.execute(
                    session,
                    |_, run| verify_semantic_state(session, &run.fields, context),
                    context.store.fault,
                ),
                "USAGE_STALE_CHECKPOINT",
            );
            Ok(())
        })
        .unwrap();
    total(&fixture.status()["usage"], 11);
    fixture.tear_down();
}

fn mapper_failure(fixture: &UsageGraph, branch: &Value, label: &str) {
    let kind = "failure";
    let name = format!("{label}.json");
    let bytes = compat::dumps(
        &Value::object([
            ("schema_version", 1_i64.into()),
            ("kind", kind.into()),
            ("label", label.into()),
        ]),
        SerializationProfile::STORED,
    )
    .unwrap();
    let path = fixture
        .graph
        .graph
        .repository
        .join("docs/artifacts")
        .join(&name);
    fs::write(path, &bytes).unwrap();
    let digest = ids::sha256_bytes(&bytes);
    let token = branch["claim_token"].text().unwrap();
    let branch_id = branch["branch_id"].text().unwrap();
    let attempt = branch["attempt_id"].text().unwrap();
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", branch_id.as_str().into()),
        ("status", "failed".into()),
        ("output_kind", "impact_map".into()),
        ("failure_code", "INSUFFICIENT_EVIDENCE".into()),
        (
            "evidence",
            Value::list([Value::object([
                ("kind", kind.into()),
                (
                    "ref",
                    format!("repo:docs/artifacts/{name}#sha256={digest}").into(),
                ),
                ("sha256", digest.into()),
            ])]),
        ),
        ("attempt_id", attempt.as_str().into()),
        ("claim_digest", ids::sha256_bytes(token.as_bytes()).into()),
    ]);
    let path = fixture
        .graph
        .graph
        .store
        .inbox_root("albanian-live-translate", "RUN-1")
        .join("failure.json");
    fs::write(
        &path,
        compat::dumps(&manifest, SerializationProfile::STORED).unwrap(),
    )
    .unwrap();
    fixture
        .graph
        .graph
        .store
        .platform
        .set_mode(&path, 0o600)
        .unwrap();
    fixture
        .graph
        .graphctl(
            &[
                "record",
                "branch-result",
                "--run-id",
                "RUN-1",
                "--branch-id",
                &branch_id,
                "--attempt-id",
                &attempt,
                "--claim-token",
                &token,
                "--result-manifest",
                path.to_str().unwrap(),
                "--op-id",
                "mapper-failure",
            ]
            .map(PyString::from),
        )
        .unwrap();
}

fn failed_mapper_with_usage(
    fixture: &UsageGraph,
    failure_label: &str,
    retry_op: &str,
) -> (Value, Value) {
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let first = fixture.claim_raw();
    let binding = fixture
        .bind_with("scoping", "bind-usage", None, Some(&first))
        .unwrap()["binding_id"]
        .clone();
    fixture.tokens(20, 2, Some((10, 1)));
    close_or_collect(fixture, &binding, "collect-usage", true);
    mapper_failure(fixture, &first, failure_label);
    fixture
        .graph
        .graphctl(
            &[
                "record",
                "retry",
                "--run-id",
                "RUN-1",
                "--branch-id",
                &first["branch_id"].text().unwrap(),
                "--reason-code",
                "RETRY",
                "--op-id",
                retry_op,
            ]
            .map(PyString::from),
        )
        .unwrap();
    (first, fixture.claim_raw())
}

#[test]
fn test_retry_has_distinct_usage_attempt_without_recounting_first() {
    let fixture = UsageGraph::new();
    let (first, second) = failed_mapper_with_usage(&fixture, "retry-usage-failure", "retry");
    let binding = fixture
        .bind_with("scoping", "second-bind", None, Some(&second))
        .unwrap()["binding_id"]
        .clone();
    fixture.tokens(30, 3, Some((10, 1)));
    let result = close_or_collect(&fixture, &binding, "second-close", true);
    total(&result, 22);
    for attempt in [first, second] {
        total(
            &result["attempts"][&attempt["attempt_id"].text().unwrap()],
            11,
        );
    }
    fixture.tear_down();
}

#[test]
fn test_unbound_retry_keeps_agent_role_and_generation_totals_partial() {
    let fixture = UsageGraph::new();
    let (first, second) =
        failed_mapper_with_usage(&fixture, "unbound-retry-failure", "unbound-retry");
    let result = &fixture.status()["usage"];
    for (dimension, key) in [
        ("agents", first["branch_id"].text().unwrap()),
        ("roles", first["role"].text().unwrap()),
        ("generations", "0".to_owned()),
    ] {
        let summary = &result[dimension][&key];
        assert!(summary["coverage"].is_text("partial"));
        total(summary, 11);
        assert!(summary["complete_totals"].is_null());
        assert!(summary["missing_executed_attempts"]
            .as_list()
            .unwrap()
            .iter()
            .any(|item| item.py_eq(&second["attempt_id"])));
    }
    let models = result["models"].as_object().unwrap();
    assert_eq!(models.len(), 1);
    assert!(models[0].0.equals("gpt-6-astra"));
    for summary in [
        &result["models"]["gpt-6-astra"],
        &result["efforts"]["medium"],
        &result["model_efforts"]["gpt-6-astra/medium"],
    ] {
        assert!(summary["attribution_scope"].is_text("observed_intervals_only"));
        assert!(summary["association_coverage"].is_text("partial"));
    }
    fixture.tear_down();
}

#[test]
fn test_large_valid_counters_preserve_exact_aggregate_and_structured_cli_output() {
    let fixture = UsageGraph::new();
    fixture.tokens(0, 0, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].text().unwrap();
    fixture.tokens(10, 0, None);
    close_or_collect(
        &fixture,
        &Value::from(binding.as_str()),
        "collect-usage",
        false,
    );
    fixture.tokens(0, 0, None);
    fixture.tokens(i64::MAX, 0, None);
    let repo = &fixture.graph.graph.repository;
    let output = Command::new(env!("CARGO_BIN_EXE_graphctl"))
        .args([
            "--repo",
            repo.to_str().unwrap(),
            "record",
            "usage",
            "--run-id",
            "RUN-1",
            "--action",
            "close",
            "--binding-id",
            &binding,
            "--session-log",
            fixture.log.to_str().unwrap(),
            "--op-id",
            "large-close",
        ])
        .env(
            "SOFTWARE_ENGINEERING_GRAPH_HOME",
            &fixture.graph.graph.runtime_root,
        )
        .current_dir(repo)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let result = compat::loads(&output.stdout).unwrap();
    assert!(result["ok"].is_true());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Traceback"));
    let exact = Value::Integer(num_bigint::BigInt::from(i64::MAX) + 10);
    let usage = &result["usage"];
    for summary in [
        usage,
        &usage["phases"]["scoping"],
        &usage["model_efforts"]["gpt-6-astra/medium"],
    ] {
        assert!(summary["observed_totals"]["input_tokens"].py_eq(&exact));
        assert!(summary["observed_totals"]["output_tokens"].py_eq(&0_i64.into()));
        assert!(summary["observed_totals"]["total_tokens"].py_eq(&exact));
        assert!(summary["coverage"].is_text("partial"));
        assert!(summary["complete_totals"].is_null());
        has_code(summary, "counter_reset");
    }
    assert!(fixture.status()["usage"]["observed_totals"]["total_tokens"].py_eq(&exact));
    fixture.tear_down();
}

#[test]
fn test_selected_model_options_remain_recognizable_in_usage_context() {
    let known = usage::known_models();
    for model in [
        "gpt-5.6-terra",
        "grok-4.7",
        "gemini-3.8-flash",
        "claude-fable-5-1",
        "cursor-grok-4.6",
        "gpt-6-astra",
        "claude-sonnet-5-5",
        "claude-opus-5-5",
    ] {
        let sample = Value::object([("model", model.into()), ("effort", "medium".into())]);
        assert!(Context::from_value(&sample, known)
            .unwrap()
            .value()
            .py_eq(&sample));
    }
    let sample = Value::object([
        ("model", "untrusted-arbitrary-model".into()),
        ("effort", "medium".into()),
    ]);
    assert_eq!(
        Context::from_value(&sample, known).unwrap().model,
        "unknown"
    );
    let totals = Value::object([
        ("input_tokens", 0_i64.into()),
        ("output_tokens", 0_i64.into()),
        ("total_tokens", 0_i64.into()),
        ("cached_input_tokens", 0_i64.into()),
        ("reasoning_output_tokens", 0_i64.into()),
        ("cache_write_tokens", 0_i64.into()),
    ]);
    for effort in ["low", "medium", "high", "xhigh", "max", "ultra"] {
        let sample = Value::object([
            ("model", "gpt-6.1-terra".into()),
            ("reasoning_effort", effort.into()),
        ]);
        let context = Context::from_value(&sample, known).unwrap().value();
        assert!(context.py_eq(&Value::object([
            ("model", "gpt-6.1-terra".into()),
            ("effort", effort.into())
        ])));
        let mut segment = context;
        segment.set("phase", "implementation".into());
        segment.set("totals", totals.clone());
        let mut binding = Value::object([
            ("usage_schema_version", 1_i64.into()),
            ("binding_id", format!("g2-{}", "a".repeat(24)).into()),
            ("source_id", "a".repeat(64).into()),
            ("adapter", "codex_jsonl_v1".into()),
            ("branch_id", Value::Null),
            ("attempt_id", Value::Null),
            ("role", "senior_engineer".into()),
            ("phase", "implementation".into()),
            ("generation", 0_i64.into()),
            ("start_offset", 0_i64.into()),
            ("offset", 0_i64.into()),
            ("prefix_sha256", "a".repeat(64).into()),
            ("previous", Value::Null),
            ("observed", false.into()),
            ("closed", false.into()),
            ("broken", false.into()),
            ("diagnostics", Value::List(vec![])),
            ("metric_partial", Value::List(vec![])),
            ("boundary_kind", "baseline".into()),
            ("segments", Value::list([segment.clone()])),
        ]);
        validate_binding(&binding).unwrap();
        segment.set("model", "untrusted-arbitrary-model".into());
        binding.set("segments", Value::list([segment]));
        assert!(matches!(
            validate_binding(&binding),
            Err(ValidationFailure::Python {
                kind: "ValueError",
                ..
            })
        ));
    }
}
