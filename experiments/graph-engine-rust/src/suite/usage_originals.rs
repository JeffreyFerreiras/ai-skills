#[path = "../../tests/support/usage_graph.rs"]
mod usage_graph;

use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_cli::ExecuteOutcome,
        usage_source,
    },
    compat::{self, SerializationProfile, Value},
    domain::{
        ids,
        usage::{Limits, PHASES},
    },
    infrastructure::usage_files::NativeUsageSource,
};
use std::fs;
use usage_graph::UsageGraph;

pub const CASES: &[(&str, fn())] = &[
    (
        "test_checkpoint_before_policy_or_run_and_historical_bind",
        checkpoint_before_policy_or_run_and_historical_bind,
    ),
    (
        "test_default_baseline_deduplicates_collects_and_replay_after_append",
        default_baseline_deduplicates_collects_and_replay_after_append,
    ),
    (
        "test_source_start_subsets_cache_write_and_unknown_observed_context",
        source_start_subsets_cache_write_and_unknown_observed_context,
    ),
    (
        "test_phase_boundaries_all_five_and_effort_pairs",
        phase_boundaries_all_five_and_effort_pairs,
    ),
    (
        "test_overlap_rejected_and_cross_boundary_increment_not_double_counted",
        overlap_rejected_and_cross_boundary_increment_not_double_counted,
    ),
    (
        "test_ambiguous_prerun_bridge_excluded_and_reset_keeps_prior_usage",
        ambiguous_prerun_bridge_excluded_and_reset_keeps_prior_usage,
    ),
    (
        "test_branch_attempt_association_and_resumed_source",
        branch_attempt_association_and_resumed_source,
    ),
    (
        "test_missing_metadata_old_schema_and_late_aborted_run",
        missing_metadata_old_schema_and_late_aborted_run,
    ),
    (
        "test_checkpoint_mismatch_and_arguments_leave_revision_unchanged",
        checkpoint_mismatch_and_arguments_leave_revision_unchanged,
    ),
    (
        "test_rewritten_or_truncated_prefix_preserves_observed_usage",
        rewritten_or_truncated_prefix_preserves_observed_usage,
    ),
    (
        "test_corrupt_optional_event_does_not_block_graph_status",
        corrupt_optional_event_does_not_block_graph_status,
    ),
    (
        "test_invalid_required_counters_never_become_zero_or_complete",
        invalid_required_counters_never_become_zero_or_complete,
    ),
    (
        "test_optional_discontinuity_does_not_discard_required_totals",
        optional_discontinuity_does_not_discard_required_totals,
    ),
    (
        "test_multiple_contexts_are_unattributed_without_guessing_planned_model",
        multiple_contexts_are_unattributed_without_guessing_planned_model,
    ),
    (
        "test_unfinished_record_is_collected_once_after_newline",
        unfinished_record_is_collected_once_after_newline,
    ),
    (
        "test_checkpoint_replacement_same_offset_requires_identity_and_prefix",
        checkpoint_replacement_same_offset_requires_identity_and_prefix,
    ),
    (
        "test_missing_resumed_session_remains_a_gap_after_other_interval_closes",
        missing_resumed_session_remains_a_gap_after_other_interval_closes,
    ),
    (
        "test_observed_cache_write_spelling_and_alias_precedence",
        observed_cache_write_spelling_and_alias_precedence,
    ),
    (
        "test_supervisor_group_keeps_missing_primary_phase_visible",
        supervisor_group_keeps_missing_primary_phase_visible,
    ),
];

fn eq(actual: &Value, expected: impl Into<Value>) {
    let expected = expected.into();
    assert!(
        actual.container_eq(&expected),
        "actual={actual:?}, expected={expected:?}"
    );
}
fn contains(values: &Value, text: &str) {
    assert!(values
        .as_list()
        .unwrap()
        .iter()
        .any(|value| value.is_text(text)));
}
fn json(value: &Value) -> String {
    compat::dumps_text(value, SerializationProfile::STORED)
        .unwrap()
        .utf8()
        .unwrap()
}
fn collect(fixture: &UsageGraph, binding: &Value, operation: &str, close: bool) -> Value {
    fixture
        .collect(binding, if close { "close" } else { "collect" }, operation)
        .unwrap()
}
fn checkpoint(fixture: &UsageGraph) -> Value {
    usage_source::checkpoint(&NativeUsageSource, &fixture.log).unwrap()
}
fn expect_state(result: EngineResult<Value>, code: Option<&str>) {
    match result {
        Err(EngineError::State(error)) => {
            if let Some(code) = code {
                assert_eq!(error.code, code);
            }
        }
        result => panic!("expected source StateError, got {result:?}"),
    }
}

pub fn checkpoint_before_policy_or_run_and_historical_bind() {
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
    let ExecuteOutcome::Response {
        result: before,
        exit_code,
    } = outcome
    else {
        panic!("parser exit")
    };
    assert_eq!(exit_code, 0);
    eq(&before["checkpoint_schema_version"], 1_i64);
    assert!(!json(&before).contains("synthetic-session"));
    fixture.tokens(160, 30, Some((60, 10)));
    fixture.initialize();
    let result = fixture
        .bind_with("scoping", "bind-usage", Some(&before), None)
        .unwrap();
    eq(&result["usage"]["observed_totals"]["total_tokens"], 70_i64);
    eq(
        &result["usage"]["model_efforts"]["gpt-6-astra/medium"]["observed_totals"]["total_tokens"],
        70_i64,
    );
    assert!(result["usage"]["running"].truthy());
    assert!(result["usage"]["complete_totals"].is_null());
    fixture.tear_down();
}

pub fn default_baseline_deduplicates_collects_and_replay_after_append() {
    let fixture = UsageGraph::new();
    fixture.tokens(100, 20, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    let optional = [
        ("cached_input_tokens", 30_i64.into()),
        ("reasoning_output_tokens", 3_i64.into()),
    ];
    fixture.tokens_with(120, 25, Some((20, 5)), &optional);
    eq(
        &collect(&fixture, &binding, "collect-usage", false)["usage"]["observed_totals"]
            ["total_tokens"],
        25_i64,
    );
    fixture.tokens_with(120, 25, Some((20, 5)), &optional);
    eq(
        &collect(&fixture, &binding, "collect-again", false)["usage"]["observed_totals"]
            ["total_tokens"],
        25_i64,
    );
    fixture.tokens(140, 30, Some((20, 5)));
    let replay = collect(&fixture, &binding, "collect-usage", false);
    eq(&replay["code"], "REPLAYED");
    eq(&replay["usage"]["observed_totals"]["total_tokens"], 25_i64);
    expect_state(
        fixture.collect(&binding, "close", "collect-usage"),
        Some("OPERATION_CONFLICT"),
    );
    let closed = collect(&fixture, &binding, "close", true);
    eq(&closed["usage"]["observed_totals"]["total_tokens"], 50_i64);
    eq(&closed["usage"]["open_bindings"], Value::List(vec![]));
    fixture.tear_down();
}

pub fn source_start_subsets_cache_write_and_unknown_observed_context() {
    let fixture = UsageGraph::new();
    fixture.tokens_with(
        100,
        20,
        None,
        &[
            ("cached_input_tokens", 40_i64.into()),
            ("reasoning_output_tokens", 10_i64.into()),
            ("cache_write_tokens", 12_i64.into()),
        ],
    );
    fixture.initialize();
    let initial = Value::object([
        ("offset", 0_i64.into()),
        ("source_id", ids::sha256_bytes(b"synthetic-session").into()),
        ("prefix_sha256", ids::sha256_bytes(b"").into()),
    ]);
    let binding = fixture
        .bind_with("scoping", "bind-usage", Some(&initial), None)
        .unwrap()["binding_id"]
        .clone();
    let result = collect(&fixture, &binding, "collect-usage", true)["usage"].clone();
    eq(
        &result["observed_totals"],
        Value::object([
            ("input_tokens", 100_i64.into()),
            ("output_tokens", 20_i64.into()),
            ("total_tokens", 120_i64.into()),
            ("cached_input_tokens", 40_i64.into()),
            ("reasoning_output_tokens", 10_i64.into()),
            ("cache_write_tokens", 12_i64.into()),
        ]),
    );
    eq(&result["phases"]["scoping"]["coverage"], "complete");
    assert!(result["phases"]["implementation"]["observed_totals"].is_null());
    fixture.context("UNCONTROLLED_MODEL_SECRET", "UNCONTROLLED_EFFORT_SECRET");
    let binding = fixture
        .bind_with("implementation", "bind-next", None, None)
        .unwrap()["binding_id"]
        .clone();
    fixture.tokens(120, 30, Some((20, 10)));
    let result = collect(&fixture, &binding, "close-next", true)["usage"].clone();
    assert!(result["model_efforts"].get("unknown/unknown").is_some());
    assert!(!json(&result).contains("UNCONTROLLED"));
    fixture.tear_down();
}

pub fn phase_boundaries_all_five_and_effort_pairs() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let mut count = 10;
    let mut result = Value::Null;
    for (index, phase) in PHASES.into_iter().enumerate() {
        fixture.context(
            "gpt-6-astra",
            if index % 2 != 0 { "high" } else { "medium" },
        );
        let binding = fixture
            .bind_with(phase, &format!("bind-{phase}"), None, None)
            .unwrap()["binding_id"]
            .clone();
        count += 10;
        fixture.tokens(count, index as i64 + 2, Some((10, 1)));
        result = collect(&fixture, &binding, &format!("close-{phase}"), true)["usage"].clone();
        eq(
            &result["phases"][phase]["observed_totals"]["total_tokens"],
            11_i64,
        );
    }
    eq(&result["observed_totals"]["total_tokens"], 55_i64);
    eq(
        &result["model_efforts"]["gpt-6-astra/medium"]["observed_totals"]["total_tokens"],
        33_i64,
    );
    eq(
        &result["model_efforts"]["gpt-6-astra/high"]["observed_totals"]["total_tokens"],
        22_i64,
    );
    fixture.tear_down();
}

pub fn overlap_rejected_and_cross_boundary_increment_not_double_counted() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    expect_state(
        fixture.bind_with("implementation", "overlap", None, None),
        Some("USAGE_INTERVAL_OVERLAP"),
    );
    fixture.tokens(20, 2, Some((10, 1)));
    collect(&fixture, &binding, "close", true);
    let binding = fixture
        .bind_with("implementation", "next", None, None)
        .unwrap()["binding_id"]
        .clone();
    fixture.tokens(40, 4, Some((30, 3)));
    let result = collect(&fixture, &binding, "close-next", true)["usage"].clone();
    eq(&result["observed_totals"]["total_tokens"], 33_i64);
    eq(
        &result["unattributed"]["observed_totals"]["total_tokens"],
        22_i64,
    );
    eq(&result["phases"]["implementation"]["coverage"], "partial");
    fixture.tear_down();
}

pub fn ambiguous_prerun_bridge_excluded_and_reset_keeps_prior_usage() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    fixture.tokens(30, 3, Some((30, 3)));
    let result = collect(&fixture, &binding, "collect-usage", false)["usage"].clone();
    eq(&result["observed_totals"]["total_tokens"], 0_i64);
    contains(&result["diagnostics"], "prefix_gap");
    fixture.tokens(40, 4, Some((10, 1)));
    fixture.tokens(5, 1, None);
    fixture.tokens(15, 2, Some((10, 1)));
    let result = collect(&fixture, &binding, "after-reset", true)["usage"].clone();
    eq(&result["observed_totals"]["total_tokens"], 22_i64);
    contains(&result["diagnostics"], "counter_reset");
    fixture.tear_down();
}

pub fn branch_attempt_association_and_resumed_source() {
    let mut fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let branch = fixture.claim_raw();
    let binding = fixture
        .bind_with("scoping", "bind-usage", None, Some(&branch))
        .unwrap()["binding_id"]
        .clone();
    fixture.tokens(20, 2, Some((10, 1)));
    let result = collect(&fixture, &binding, "collect-usage", true)["usage"].clone();
    let attempt = branch["attempt_id"].text().unwrap();
    let agent = branch["branch_id"].text().unwrap();
    eq(
        &result["attempts"][attempt.as_str()]["observed_totals"]["total_tokens"],
        11_i64,
    );
    eq(
        &result["agents"][agent.as_str()]["observed_totals"]["total_tokens"],
        11_i64,
    );
    fixture.log = fixture.graph.graph.root.join("resumed.jsonl");
    fixture.append(
        "session_meta",
        Value::object([("id", "resumed-session".into())]),
    );
    fixture.context("gpt-6-astra", "medium");
    fixture.tokens(0, 0, None);
    let binding = fixture
        .bind_with("scoping", "resume-bind", None, Some(&branch))
        .unwrap()["binding_id"]
        .clone();
    fixture.tokens(10, 1, None);
    let result = collect(&fixture, &binding, "resume-close", true)["usage"].clone();
    eq(
        &result["attempts"][attempt.as_str()]["observed_totals"]["total_tokens"],
        22_i64,
    );
    eq(&result["missing_executed_attempts"], Value::List(vec![]));
    fixture.tear_down();
}

pub fn missing_metadata_old_schema_and_late_aborted_run() {
    let fixture = UsageGraph::new();
    fixture.initialize();
    let initial = fixture.status();
    eq(&initial["state_schema_version"], 6_i64);
    eq(&initial["usage"]["coverage"], "unavailable");
    assert!(initial["usage"]["observed_totals"].is_null());
    let branch = fixture.claim_raw();
    let result = fixture.status()["usage"].clone();
    contains(
        &result["missing_executed_attempts"],
        &branch["attempt_id"].text().unwrap(),
    );
    fixture.tokens(10, 1, None);
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    fixture
        .graph
        .graphctl(&[
            "abort".into(),
            "--run-id".into(),
            "RUN-1".into(),
            "--reason-code".into(),
            "stop".into(),
            "--authority-ref".into(),
            "authority:test".into(),
            "--op-id".into(),
            "abort".into(),
        ])
        .unwrap();
    fixture.tokens(20, 2, Some((10, 1)));
    let result = collect(&fixture, &binding, "collect-usage", true);
    eq(&result["usage"]["observed_totals"]["total_tokens"], 11_i64);
    assert!(!result["usage"]["running"].truthy());
    eq(&fixture.status()["status"], "aborted");
    fixture.tear_down();
}

pub fn checkpoint_mismatch_and_arguments_leave_revision_unchanged() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    let saved = checkpoint(&fixture);
    fixture.initialize();
    let revision = fixture.status()["state_revision"].clone();
    for (key, value) in [
        ("offset", 1_i64.into()),
        ("source_id", "a".repeat(64).into()),
        ("prefix_sha256", "b".repeat(64).into()),
        (
            "offset",
            Value::Integer(num_bigint::BigInt::from(1_u64) << 63_usize),
        ),
    ] {
        let mut changed = saved.clone();
        changed.set(key, value);
        expect_state(
            fixture.bind_with("scoping", "bind-usage", Some(&changed), None),
            None,
        );
    }
    expect_state(
        fixture.usage(
            "bind",
            "bad",
            &[
                "--phase".into(),
                "scoping".into(),
                "--generation".into(),
                "0".into(),
                "--start-offset".into(),
                "0".into(),
            ],
        ),
        None,
    );
    let mut changed = saved;
    changed.set("prefix_sha256", "PRIVATE_SECRET".into());
    assert!(matches!(
        fixture.bind_with("scoping", "bind-usage", Some(&changed), None),
        Err(EngineError::Contract(_))
    ));
    eq(&fixture.status()["state_revision"], revision);
    fixture.tear_down();
}

pub fn rewritten_or_truncated_prefix_preserves_observed_usage() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    fixture.tokens(20, 2, Some((10, 1)));
    collect(&fixture, &binding, "collect-usage", false);
    let original = fs::read(&fixture.log).unwrap();
    fs::write(
        &fixture.log,
        String::from_utf8(original.clone())
            .unwrap()
            .replace("PRIVATE_SOURCE_PATH", "ALTERED_SOURCEPATHX"),
    )
    .unwrap();
    let result = collect(&fixture, &binding, "rewritten", false)["usage"].clone();
    eq(&result["observed_totals"]["total_tokens"], 11_i64);
    contains(&result["diagnostics"], "prefix_changed");
    fs::write(&fixture.log, original).unwrap();
    fixture.tokens(30, 3, Some((10, 1)));
    let result = collect(&fixture, &binding, "restored", true)["usage"].clone();
    eq(&result["observed_totals"]["total_tokens"], 11_i64);
    fixture.tear_down();
}

pub fn corrupt_optional_event_does_not_block_graph_status() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    fixture.bind().unwrap();
    fixture
        .graph
        .with_session(|session, _| {
            session.execute_values(
                "UPDATE events SET detail_json=? WHERE event_type='usage.observation'",
                &[json(&Value::object([(
                    "PRIVATE_SECRET",
                    "PRIVATE_SOURCE_PATH".into(),
                )]))
                .into()],
            )?;
            Ok(())
        })
        .unwrap();
    let result = fixture.status();
    eq(
        &result["usage"]["diagnostics"],
        Value::List(vec!["invalid_usage_state".into()]),
    );
    assert!(!json(&result["usage"]).contains("PRIVATE"));
    fixture.tear_down();
}

pub fn invalid_required_counters_never_become_zero_or_complete() {
    let fixture = UsageGraph::new();
    for counters in [
        Value::object([
            ("input_tokens", true.into()),
            ("output_tokens", 0_i64.into()),
            ("total_tokens", 1_i64.into()),
        ]),
        Value::object([
            ("input_tokens", (-1_i64).into()),
            ("output_tokens", 2_i64.into()),
            ("total_tokens", 1_i64.into()),
        ]),
        Value::object([
            ("input_tokens", 10_i64.into()),
            ("output_tokens", 2_i64.into()),
            ("total_tokens", 13_i64.into()),
        ]),
        Value::object([
            (
                "input_tokens",
                Value::Integer(num_bigint::BigInt::from(1_u64) << 63_usize),
            ),
            ("output_tokens", 0_i64.into()),
            (
                "total_tokens",
                Value::Integer(num_bigint::BigInt::from(1_u64) << 63_usize),
            ),
        ]),
        Value::object([
            ("input_tokens", 10_i64.into()),
            ("total_tokens", 10_i64.into()),
        ]),
    ] {
        fixture.append(
            "event_msg",
            Value::object([
                ("type", "token_count".into()),
                ("info", Value::object([("total_token_usage", counters)])),
            ]),
        );
    }
    assert!(
        usage_source::read_source(&NativeUsageSource, &fixture.log, Limits::default())
            .unwrap()
            .snapshots
            .is_empty()
    );
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    let result = collect(&fixture, &binding, "collect-usage", true)["usage"].clone();
    eq(&result["coverage"], "unavailable");
    assert!(result["observed_totals"].is_null());
    fixture.tear_down();
}

pub fn optional_discontinuity_does_not_discard_required_totals() {
    let fixture = UsageGraph::new();
    fixture.tokens_with(
        100,
        20,
        None,
        &[
            ("cached_input_tokens", 30_i64.into()),
            ("reasoning_output_tokens", 10_i64.into()),
            ("cache_write_tokens", 12_i64.into()),
        ],
    );
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    fixture.tokens_with(
        120,
        25,
        Some((20, 5)),
        &[
            ("cached_input_tokens", 10_i64.into()),
            ("reasoning_output_tokens", 26_i64.into()),
            ("cache_write_tokens", 15_i64.into()),
        ],
    );
    let result = collect(&fixture, &binding, "collect-usage", true)["usage"].clone();
    eq(&result["observed_totals"]["total_tokens"], 25_i64);
    assert!(result["observed_totals"]["cached_input_tokens"].is_null());
    assert!(result["observed_totals"]["reasoning_output_tokens"].is_null());
    eq(&result["observed_totals"]["cache_write_tokens"], 3_i64);
    let mut metrics: Vec<_> = result["metric_partial"]
        .as_list()
        .unwrap()
        .iter()
        .map(|value| value.text().unwrap())
        .collect();
    metrics.sort();
    assert_eq!(metrics, ["cached_input_tokens", "reasoning_output_tokens"]);
    fixture.tear_down();
}

pub fn multiple_contexts_are_unattributed_without_guessing_planned_model() {
    let fixture = UsageGraph::new();
    fixture.tokens(100, 20, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    fixture.context("gpt-6-astra", "medium");
    fixture.context("gpt-6-astra", "high");
    fixture.tokens(120, 25, Some((20, 5)));
    let result = collect(&fixture, &binding, "collect-usage", true)["usage"].clone();
    eq(
        &result["unattributed"]["observed_totals"]["total_tokens"],
        25_i64,
    );
    let keys: Vec<_> = result["models"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, _)| key.utf8().unwrap())
        .collect();
    assert_eq!(keys, ["unknown"]);
    contains(&result["diagnostics"], "ambiguous_increment");
    fixture.tear_down();
}

pub fn unfinished_record_is_collected_once_after_newline() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    fixture.tokens(20, 2, Some((10, 1)));
    let complete = fs::read(&fixture.log).unwrap();
    fs::write(&fixture.log, &complete[..complete.len() - 1]).unwrap();
    eq(
        &collect(&fixture, &binding, "collect-usage", false)["usage"]["observed_totals"]
            ["total_tokens"],
        0_i64,
    );
    fs::write(&fixture.log, complete).unwrap();
    eq(
        &collect(&fixture, &binding, "complete-line", true)["usage"]["observed_totals"]
            ["total_tokens"],
        11_i64,
    );
    fixture.tear_down();
}

pub fn checkpoint_replacement_same_offset_requires_identity_and_prefix() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    let saved = checkpoint(&fixture);
    let original = fs::read(&fixture.log).unwrap();
    fixture.initialize();
    fs::write(
        &fixture.log,
        String::from_utf8(original.clone())
            .unwrap()
            .replace("synthetic-session", "different-session"),
    )
    .unwrap();
    expect_state(
        fixture.bind_with("scoping", "bind-usage", Some(&saved), None),
        Some("USAGE_CHECKPOINT_MISMATCH"),
    );
    fs::write(
        &fixture.log,
        String::from_utf8(original.clone())
            .unwrap()
            .replace("PRIVATE_SOURCE_PATH", "CHANGED_SOURCE_PATH"),
    )
    .unwrap();
    expect_state(
        fixture.bind_with("scoping", "bind-usage", Some(&saved), None),
        Some("USAGE_CHECKPOINT_MISMATCH"),
    );
    fs::write(&fixture.log, &original).unwrap();
    let binding = fixture
        .bind_with("scoping", "bind-usage", Some(&saved), None)
        .unwrap()["binding_id"]
        .clone();
    fs::write(&fixture.log, &original[..60]).unwrap();
    let result = collect(&fixture, &binding, "collect-usage", true)["usage"].clone();
    eq(&result["coverage"], "partial");
    assert!(result["complete_totals"].is_null());
    fixture.tear_down();
}

pub fn missing_resumed_session_remains_a_gap_after_other_interval_closes() {
    let mut fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    fixture.tokens(20, 2, Some((10, 1)));
    collect(&fixture, &binding, "collect-usage", true);
    fixture.log = fixture.graph.graph.root.join("PRIVATE_MISSING_SESSION");
    let missing = fixture
        .bind_with("scoping", "missing-resume", None, None)
        .unwrap();
    eq(&missing["code"], "USAGE_UNAVAILABLE");
    assert!(missing["checkpoint"].is_null());
    let result = collect(&fixture, &missing["binding_id"], "missing-close", true)["usage"].clone();
    eq(&result["phases"]["scoping"]["coverage"], "partial");
    eq(&result["observed_totals"]["total_tokens"], 11_i64);
    contains(&result["diagnostics"], "source_unavailable");
    assert!(!json(&result).contains("PRIVATE"));
    fixture.tear_down();
}

pub fn observed_cache_write_spelling_and_alias_precedence() {
    let fixture = UsageGraph::new();
    fixture.tokens_with(
        100,
        20,
        None,
        &[
            ("cache_write_input_tokens", 12_i64.into()),
            ("cache_write_tokens", 99_i64.into()),
        ],
    );
    fixture.initialize();
    let binding = fixture.bind().unwrap()["binding_id"].clone();
    fixture.tokens_with(
        120,
        25,
        Some((20, 5)),
        &[
            ("cache_write_input_tokens", 15_i64.into()),
            ("cache_write_tokens", 199_i64.into()),
        ],
    );
    eq(
        &collect(&fixture, &binding, "collect-usage", false)["usage"]["observed_totals"]
            ["cache_write_tokens"],
        3_i64,
    );
    fixture.tokens_with(
        140,
        30,
        Some((20, 5)),
        &[
            ("cache_write_input_tokens", "invalid".into()),
            ("cache_write_tokens", 200_i64.into()),
        ],
    );
    assert!(
        usage_source::read_source(&NativeUsageSource, &fixture.log, Limits::default())
            .unwrap()
            .snapshots
            .last()
            .unwrap()
            .counters
            .optional[2]
            .is_none()
    );
    let result = collect(&fixture, &binding, "invalid-preferred", true)["usage"].clone();
    eq(&result["observed_totals"]["total_tokens"], 50_i64);
    assert!(result["observed_totals"]["cache_write_tokens"].is_null());
    contains(&result["metric_partial"], "cache_write_tokens");
    fixture.tear_down();
}

pub fn supervisor_group_keeps_missing_primary_phase_visible() {
    let fixture = UsageGraph::new();
    fixture.tokens(10, 1, None);
    fixture.initialize();
    let binding = fixture
        .bind_with("implementation", "bind-usage", None, None)
        .unwrap()["binding_id"]
        .clone();
    fixture.tokens(20, 2, Some((10, 1)));
    let result = collect(&fixture, &binding, "collect-usage", true)["usage"].clone();
    for summary in [&result["roles"]["supervisor"], &result["generations"]["0"]] {
        eq(&summary["coverage"], "partial");
        eq(&summary["observed_totals"]["total_tokens"], 11_i64);
        assert!(summary["complete_totals"].is_null());
        eq(
            &summary["missing_primary_phases"],
            Value::List(vec!["scoping".into()]),
        );
    }
    fixture.tear_down();
}
