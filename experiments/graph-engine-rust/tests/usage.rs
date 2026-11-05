#[path = "support/store.rs"]
mod store_fixture;

use graph_engine::{
    application::{
        error::EngineError,
        persistent_store::{PersistentSession, StateStore},
        state::RunPatch,
        usage_record::{self, PreparedUsage, UsageArguments},
        usage_report,
    },
    compat::{self, SerializationProfile, Value},
    infrastructure::{
        sqlite::SqliteSession,
        state_database::NativeStateDatabase,
        state_platform::NativeStatePlatform,
        usage_files::{self, NativeUsageSource, UsageFault, UsageReadStage},
    },
};
use std::{io::Write, path::PathBuf};

fn bind_attempt(fixture: &UsageFixture, attempt_id: &str, operation_id: &str) -> Value {
    let operation = Value::from(operation_id);
    let action = Value::from("bind");
    let branch = Value::from("bootstrap-branch");
    let attempt = Value::from(attempt_id);
    let null = Value::Null;
    let run = fixture.session.run("RUN-1").unwrap().unwrap();
    usage_record::prepare(
        UsageArguments {
            operation_id: &operation,
            action: &action,
            session_log: &fixture.log,
            start_offset: &null,
            source_id: &null,
            prefix_sha256: &null,
            binding_id: &null,
            branch_id: &branch,
            attempt_id: &attempt,
            phase: &null,
            generation: &null,
        },
        fixture.session.as_ref(),
        "RUN-1",
        &run,
        &NativeUsageSource,
    )
    .unwrap()
    .execute(fixture.session.as_ref(), |_, _| Ok(()), &|_: &str| Ok(()))
    .unwrap()
}

#[test]
fn usage_component_branch_resumed_attempts_and_unbound_retry_preserve_association_gaps() {
    let mut fixture = UsageFixture::new("usage-association");
    let sql = SqliteSession::connect_existing(&fixture.database).unwrap();
    let insert_attempt = |number: i64, attempt: &str| {
        sql.execute_values("INSERT INTO branch_attempts(run_id,branch_id,attempt_number,attempt_id,claim_digest,started_at) VALUES(?,?,?,?,?,?)", &[
            "RUN-1".into(), "bootstrap-branch".into(), number.into(), attempt.into(),
            "c".repeat(64).into(), "2026-11-01T00:00:00Z".into(),
        ]).unwrap();
    };
    insert_attempt(1, "usage-attempt-1");
    fixture.tokens(10, 1, 10, 1);
    let first = bind_attempt(&fixture, "usage-attempt-1", "bind-first");
    fixture.tokens(20, 2, 10, 1);
    fixture
        .call("close", &first["binding_id"].text().unwrap(), "close-first")
        .unwrap();
    fixture.log = fixture
        ._fixture
        .root
        .join("synthetic-resumed-session.jsonl");
    std::fs::write(&fixture.log, b"").unwrap();
    fixture.append(
        "session_meta",
        Value::object([("id", "synthetic-resumed-session".into())]),
    );
    fixture.append(
        "turn_context",
        Value::object([("model", "gpt-6-astra".into()), ("effort", "medium".into())]),
    );
    fixture.tokens(0, 0, 0, 0);
    let resumed = bind_attempt(&fixture, "usage-attempt-1", "bind-resumed");
    fixture.tokens(10, 1, 10, 1);
    let closed = fixture
        .call(
            "close",
            &resumed["binding_id"].text().unwrap(),
            "close-resumed",
        )
        .unwrap();
    assert_eq!(
        closed["usage"]["attempts"]["usage-attempt-1"]["observed_totals"]["total_tokens"].as_i64(),
        Some(22)
    );
    assert_eq!(
        closed["usage"]["agents"]["bootstrap-branch"]["observed_totals"]["total_tokens"].as_i64(),
        Some(22)
    );
    insert_attempt(2, "usage-attempt-2");
    let report = fixture.report().unwrap();
    for value in [
        &report["agents"]["bootstrap-branch"],
        &report["roles"]["tech_lead"],
        &report["generations"]["1"],
    ] {
        assert!(value["coverage"].is_text("partial"));
        assert_eq!(value["observed_totals"]["total_tokens"].as_i64(), Some(22));
        assert!(value["missing_executed_attempts"]
            .as_list()
            .unwrap()
            .iter()
            .any(|value| value.is_text("usage-attempt-2")));
    }
    assert!(
        report["model_efforts"]["gpt-6-astra/medium"]["association_coverage"].is_text("partial")
    );
    sql.execute_values(
        "UPDATE nodes SET generation=2 WHERE branch_id='bootstrap-branch'",
        &[],
    )
    .unwrap();
    assert!(
        fixture.report().unwrap()["diagnostics"].as_list().unwrap()[0]
            .is_text("invalid_usage_state")
    );
}

struct UsageFixture {
    _fixture: store_fixture::Fixture,
    session: Box<dyn PersistentSession>,
    database: PathBuf,
    log: PathBuf,
}
impl UsageFixture {
    fn new(label: &str) -> Self {
        let fixture = store_fixture::Fixture::new(label);
        let platform = NativeStatePlatform;
        let database = NativeStateDatabase;
        let fault = |_: &str| Ok(());
        let store =
            StateStore::at_root(fixture.root.join("state"), &platform, &database, &fault).unwrap();
        store.initialize(&fixture.request()).unwrap();
        let path = store.db_path("fixture-repository", "RUN-1");
        let session = store.open_run("fixture-repository", "RUN-1").unwrap();
        let log = fixture.root.join("synthetic-private-session.jsonl");
        std::fs::write(&log, b"").unwrap();
        let result = Self {
            _fixture: fixture,
            session,
            database: path,
            log,
        };
        result.append(
            "session_meta",
            Value::object([
                ("id", "synthetic-session".into()),
                ("cwd", "PRIVATE_SOURCE_PATH".into()),
            ]),
        );
        result.append(
            "turn_context",
            Value::object([("model", "gpt-6-astra".into()), ("effort", "medium".into())]),
        );
        result
    }
    fn append(&self, kind: &str, payload: Value) {
        let value = Value::object([("type", kind.into()), ("payload", payload)]);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.log)
            .unwrap();
        file.write_all(&compat::dumps(&value, SerializationProfile::STORED).unwrap())
            .unwrap();
        file.write_all(b"\r\n").unwrap();
    }
    fn tokens(&self, input: i64, output: i64, last_input: i64, last_output: i64) {
        let counters = |input: i64, output: i64| {
            Value::object([
                ("input_tokens", input.into()),
                ("output_tokens", output.into()),
                ("total_tokens", (input + output).into()),
            ])
        };
        self.append(
            "event_msg",
            Value::object([
                ("type", "token_count".into()),
                (
                    "info",
                    Value::object([
                        ("total_token_usage", counters(input, output)),
                        ("last_token_usage", counters(last_input, last_output)),
                    ]),
                ),
            ]),
        );
    }
    fn prepare(&self, action: &str, binding: &str, operation: &str) -> PreparedUsage {
        self.prepare_at(action, binding, operation, "scoping", None)
    }
    fn prepare_at(
        &self,
        action: &str,
        binding: &str,
        operation: &str,
        selected_phase: &str,
        checkpoint: Option<&Value>,
    ) -> PreparedUsage {
        let action: Value = action.into();
        let binding: Value = if binding.is_empty() {
            Value::Null
        } else {
            binding.into()
        };
        let operation: Value = operation.into();
        let phase: Value = selected_phase.into();
        let generation: Value = 0_i64.into();
        let null = Value::Null;
        let run = self.session.run("RUN-1").unwrap().unwrap();
        usage_record::prepare(
            UsageArguments {
                operation_id: &operation,
                action: &action,
                session_log: &self.log,
                start_offset: checkpoint.map_or(&null, |value| &value["offset"]),
                source_id: checkpoint.map_or(&null, |value| &value["source_id"]),
                prefix_sha256: checkpoint.map_or(&null, |value| &value["prefix_sha256"]),
                binding_id: &binding,
                branch_id: &null,
                attempt_id: &null,
                phase: if action.is_text("bind") {
                    &phase
                } else {
                    &null
                },
                generation: if action.is_text("bind") {
                    &generation
                } else {
                    &null
                },
            },
            self.session.as_ref(),
            "RUN-1",
            &run,
            &NativeUsageSource,
        )
        .unwrap()
    }
    fn call(&self, action: &str, binding: &str, operation: &str) -> Result<Value, EngineError> {
        self.prepare(action, binding, operation).execute(
            self.session.as_ref(),
            |_, _| Ok(()),
            &|_: &str| Ok(()),
        )
    }
    fn report(&self) -> Result<Value, EngineError> {
        usage_report::report(
            self.session.as_ref(),
            &self.session.run("RUN-1").unwrap().unwrap().fields,
        )
    }
}

#[test]
fn usage_component_historical_checkpoint_and_all_phase_contexts_preserve_exact_attribution() {
    let fixture = UsageFixture::new("usage-historical");
    fixture.tokens(100, 20, 100, 20);
    let before =
        graph_engine::application::usage_source::checkpoint(&NativeUsageSource, &fixture.log)
            .unwrap();
    fixture.tokens(160, 30, 60, 10);
    let historical = fixture
        .prepare_at("bind", "", "historical", "scoping", Some(&before))
        .execute(fixture.session.as_ref(), |_, _| Ok(()), &|_: &str| Ok(()))
        .unwrap();
    assert_eq!(
        historical["usage"]["observed_totals"]["total_tokens"].as_i64(),
        Some(70)
    );
    fixture
        .call(
            "close",
            &historical["binding_id"].text().unwrap(),
            "historical-close",
        )
        .unwrap();
    let fixture = UsageFixture::new("usage-phases");
    fixture.tokens(10, 1, 10, 1);
    for (index, phase) in graph_engine::domain::usage::PHASES.iter().enumerate() {
        let effort = if index % 2 == 0 { "medium" } else { "high" };
        fixture.append(
            "turn_context",
            Value::object([("model", "gpt-6-astra".into()), ("effort", effort.into())]),
        );
        let bound = fixture
            .prepare_at("bind", "", &format!("bind-{phase}"), phase, None)
            .execute(fixture.session.as_ref(), |_, _| Ok(()), &|_: &str| Ok(()))
            .unwrap();
        fixture.tokens(20 + index as i64 * 10, 2 + index as i64, 10, 1);
        let closed = fixture
            .call(
                "close",
                &bound["binding_id"].text().unwrap(),
                &format!("close-{phase}"),
            )
            .unwrap();
        assert_eq!(
            closed["usage"]["phases"][phase]["observed_totals"]["total_tokens"].as_i64(),
            Some(11)
        );
    }
    let report = fixture.report().unwrap();
    assert_eq!(report["observed_totals"]["total_tokens"].as_i64(), Some(55));
    assert_eq!(
        report["model_efforts"]["gpt-6-astra/medium"]["observed_totals"]["total_tokens"].as_i64(),
        Some(33)
    );
    assert_eq!(
        report["model_efforts"]["gpt-6-astra/high"]["observed_totals"]["total_tokens"].as_i64(),
        Some(22)
    );
}

#[test]
fn usage_component_supervisor_and_generation_groups_keep_unobserved_scoping_partial() {
    let fixture = UsageFixture::new("usage-missing-primary");
    fixture.tokens(10, 1, 10, 1);
    let bound = fixture
        .prepare_at("bind", "", "bind", "implementation", None)
        .execute(fixture.session.as_ref(), |_, _| Ok(()), &|_: &str| Ok(()))
        .unwrap();
    fixture.tokens(20, 2, 10, 1);
    let closed = fixture
        .call("close", &bound["binding_id"].text().unwrap(), "close")
        .unwrap();
    for value in [
        &closed["usage"]["roles"]["supervisor"],
        &closed["usage"]["generations"]["0"],
    ] {
        assert!(value["coverage"].is_text("partial"));
        assert_eq!(value["observed_totals"]["total_tokens"].as_i64(), Some(11));
        assert!(value["complete_totals"].is_null());
        assert!(value["missing_primary_phases"].as_list().unwrap()[0].is_text("scoping"));
    }
}

#[test]
fn usage_component_negative_open_faults_reject_sources_and_close_real_descriptors() {
    let fixture = store_fixture::Fixture::new("usage-open-faults");
    let path = fixture.root.join("synthetic-source.jsonl");
    std::fs::write(&path, b"synthetic\n").unwrap();
    for nonregular in [false, true] {
        let sampled = usage_files::read_prefix_observed(
            &path,
            graph_engine::domain::usage::Limits::default(),
            &|stage: UsageReadStage, _: &std::path::Path, _| {
                Ok(UsageFault {
                    opened_identity_changed: stage == UsageReadStage::Opened && !nonregular,
                    opened_nonregular: stage == UsageReadStage::Opened && nonregular,
                    ..UsageFault::default()
                })
            },
        )
        .unwrap();
        assert_eq!(sampled.diagnostics, ["source_changed"]);
        assert!(sampled.data.is_empty());
        let renamed = path.with_extension("closed");
        std::fs::rename(&path, &renamed).unwrap();
        std::fs::rename(&renamed, &path).unwrap();
    }
    assert_ne!(
        usage_files::requested_open_flags(0x4000_0000) & 0x4000_0000,
        0
    );
    let sampled = usage_files::read_prefix_observed(
        &path,
        graph_engine::domain::usage::Limits::default(),
        &|stage: UsageReadStage, _: &std::path::Path, _| {
            Ok(UsageFault {
                unsafe_component: stage == UsageReadStage::SafeComponents,
                ..UsageFault::default()
            })
        },
    )
    .unwrap();
    assert_eq!(sampled.diagnostics, ["source_unsafe"]);
}

#[test]
fn usage_component_replays_exact_prior_response_after_new_appends_and_rejects_conflicting_close() {
    let fixture = UsageFixture::new("usage-replay");
    fixture.tokens(100, 20, 100, 20);
    let bind = fixture.call("bind", "", "bind").unwrap();
    let binding = bind["binding_id"].text().unwrap();
    fixture.tokens(120, 25, 20, 5);
    let collected = fixture.call("collect", &binding, "collect").unwrap();
    assert_eq!(
        collected["usage"]["observed_totals"]["total_tokens"].as_i64(),
        Some(25)
    );
    fixture.tokens(120, 25, 20, 5);
    assert_eq!(
        fixture.call("collect", &binding, "duplicate").unwrap()["usage"]["observed_totals"]
            ["total_tokens"]
            .as_i64(),
        Some(25)
    );
    fixture.tokens(140, 30, 20, 5);
    let replay = fixture.call("collect", &binding, "collect").unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert_eq!(
        replay["usage"]["observed_totals"]["total_tokens"].as_i64(),
        Some(25)
    );
    assert!(
        matches!(fixture.call("close", &binding, "collect"), Err(EngineError::State(error)) if error.code == "OPERATION_CONFLICT")
    );
    let closed = fixture.call("close", &binding, "close").unwrap();
    assert_eq!(
        closed["usage"]["observed_totals"]["total_tokens"].as_i64(),
        Some(50)
    );
    assert!(closed["usage"]["open_bindings"]
        .as_list()
        .unwrap()
        .is_empty());
}

#[test]
fn usage_component_losing_prepared_checkpoint_cannot_replace_winner_or_advance_revision() {
    let fixture = UsageFixture::new("usage-cas");
    fixture.tokens(10, 1, 10, 1);
    let binding = fixture.call("bind", "", "bind").unwrap()["binding_id"]
        .text()
        .unwrap();
    fixture.tokens(20, 2, 10, 1);
    let loser = fixture.prepare("collect", &binding, "loser");
    fixture.call("collect", &binding, "winner").unwrap();
    let revision = fixture
        .session
        .run("RUN-1")
        .unwrap()
        .unwrap()
        .revision()
        .unwrap();
    assert!(
        matches!(loser.execute(fixture.session.as_ref(), |_, _| Ok(()), &|_: &str| Ok(())),
        Err(EngineError::State(error)) if error.code == "USAGE_STALE_CHECKPOINT")
    );
    assert_eq!(
        fixture
            .session
            .run("RUN-1")
            .unwrap()
            .unwrap()
            .revision()
            .unwrap(),
        revision
    );
    assert!(fixture
        .session
        .operation("RUN-1", "loser")
        .unwrap()
        .is_none());
    assert_eq!(
        fixture.report().unwrap()["observed_totals"]["total_tokens"].as_i64(),
        Some(11)
    );
}

#[test]
fn usage_component_fault_rolls_back_observation_operation_and_revision_together() {
    let fixture = UsageFixture::new("usage-fault");
    fixture.tokens(10, 1, 10, 1);
    let binding = fixture.call("bind", "", "bind").unwrap()["binding_id"]
        .text()
        .unwrap();
    fixture.tokens(20, 2, 10, 1);
    let prepared = fixture.prepare("collect", &binding, "faulted");
    let revision = fixture
        .session
        .run("RUN-1")
        .unwrap()
        .unwrap()
        .revision()
        .unwrap();
    let error = prepared.execute(fixture.session.as_ref(), |_, _| Ok(()), &|point: &str| {
        if point == "before_commit" {
            Err(EngineError::Injected(point.into()))
        } else {
            Ok(())
        }
    });
    assert!(matches!(error, Err(EngineError::Injected(_))));
    assert_eq!(
        fixture
            .session
            .run("RUN-1")
            .unwrap()
            .unwrap()
            .revision()
            .unwrap(),
        revision
    );
    assert!(fixture
        .session
        .operation("RUN-1", "faulted")
        .unwrap()
        .is_none());
    assert_eq!(
        fixture.report().unwrap()["observed_totals"]["total_tokens"].as_i64(),
        Some(0)
    );
    assert_eq!(
        fixture.call("collect", &binding, "faulted").unwrap()["usage"]["observed_totals"]
            ["total_tokens"]
            .as_i64(),
        Some(11)
    );
}

#[test]
fn usage_component_json_corruption_preempts_later_bad_sqlite_text_but_storage_failure_still_propagates(
) {
    let fixture = UsageFixture::new("usage-lazy-rows");
    fixture.tokens(10, 1, 10, 1);
    fixture.call("bind", "", "bind").unwrap();
    let sql = SqliteSession::connect_existing(&fixture.database).unwrap();
    let original = sql
        .query_values(
            "SELECT detail_json FROM events WHERE event_type='usage.observation' ORDER BY event_id",
            &[],
        )
        .unwrap()[0]["detail_json"]
        .clone();
    sql.execute_values(
        "UPDATE events SET detail_json='{bad}' WHERE event_type='usage.observation'",
        &[],
    )
    .unwrap();
    sql.execute_values("INSERT INTO events(run_id,revision,event_type,source_id,detail_json) SELECT run_id,revision,event_type,source_id,CAST(X'ff' AS TEXT) FROM events WHERE event_type='usage.observation' LIMIT 1", &[]).unwrap();
    let unavailable = fixture.report().unwrap();
    assert!(unavailable["coverage"].is_text("unavailable"));
    assert!(unavailable["diagnostics"].as_list().unwrap()[0].is_text("invalid_usage_state"));
    sql.execute_values("UPDATE events SET detail_json=? WHERE event_id=(SELECT MIN(event_id) FROM events WHERE event_type='usage.observation')", &[original]).unwrap();
    assert!(matches!(
        fixture.report(),
        Err(EngineError::Storage {
            kind: "OperationalError",
            ..
        })
    ));
}

#[test]
fn usage_component_large_totals_remain_exact_and_optional_missing_source_keeps_prior_coverage_partial(
) {
    let fixture = UsageFixture::new("usage-large");
    fixture.tokens(0, 0, 0, 0);
    let binding = fixture.call("bind", "", "bind").unwrap()["binding_id"]
        .text()
        .unwrap();
    fixture.tokens(10, 0, 10, 0);
    fixture.call("collect", &binding, "collect").unwrap();
    fixture.tokens(0, 0, 0, 0);
    fixture.tokens(i64::MAX, 0, i64::MAX, 0);
    fixture
        .session
        .patch_run("RUN-1", &RunPatch::Status("aborted".into()))
        .unwrap();
    let closed = fixture.call("close", &binding, "close").unwrap();
    let expected = num_bigint::BigInt::from(i64::MAX) + num_bigint::BigInt::from(10);
    assert_eq!(
        closed["usage"]["observed_totals"]["total_tokens"].strict_integer(),
        Some(&expected)
    );
    assert!(closed["usage"]["complete_totals"].is_null());
    assert!(!closed["usage"]["running"].is_true());
    std::fs::remove_file(&fixture.log).unwrap();
    let missing = fixture.call("bind", "", "missing-bind").unwrap();
    assert!(missing["code"].is_text("USAGE_UNAVAILABLE"));
    assert!(missing["checkpoint"].is_null());
    let closed = fixture
        .call(
            "close",
            &missing["binding_id"].text().unwrap(),
            "missing-close",
        )
        .unwrap();
    assert_eq!(
        closed["usage"]["observed_totals"]["total_tokens"].strict_integer(),
        Some(&expected)
    );
    assert!(closed["usage"]["phases"]["scoping"]["coverage"].is_text("partial"));
    let encoded = compat::dumps(&closed, SerializationProfile::CLI).unwrap();
    assert!(!String::from_utf8(encoded).unwrap().contains("PRIVATE_"));
}
