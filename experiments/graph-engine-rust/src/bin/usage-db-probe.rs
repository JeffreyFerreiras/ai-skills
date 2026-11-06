use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        state::{GraphSession, RunRecord},
        usage_record::{self, PreparedUsage, UsageArguments},
        usage_report, usage_source,
    },
    compat::{self, SerializationProfile, Value},
    domain::ids,
    infrastructure::{sqlite::SqliteSession, usage_files::NativeUsageSource},
};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn io_error(error: impl std::fmt::Debug) -> io::Error {
    io::Error::other(format!("{error:?}"))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn bytes(value: &Value) -> Vec<u8> {
    value
        .text()
        .expect("generated hex")
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII hex"), 16)
                .expect("generated pair")
        })
        .collect()
}
fn contained(path: &Path, artifact: &Path) -> io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    if !canonical.starts_with(fs::canonicalize(artifact)?) {
        return Err(io::Error::other("usage database probe escaped ART"));
    }
    Ok(path.to_path_buf())
}
fn outcome(result: EngineResult<Value>) -> Value {
    match result {
        Ok(value) => {
            let stored = compat::dumps(&value, SerializationProfile::STORED)
                .expect("valid generated result");
            Value::object([
                ("ok", true.into()),
                ("value", value),
                ("stored_hex", hex(&stored).into()),
            ])
        }
        Err(error) => {
            let (kind, message) = match error {
                EngineError::Contract(error) => ("ContractError", error.to_string().into()),
                EngineError::State(error) => ("StateError", error.code.into()),
                EngineError::Json(error) => (error.kind, error.message.into()),
                EngineError::Python { kind, message } => (kind, message),
                EngineError::Storage { kind, message, .. } => (kind, message.into()),
                other => panic!("unexpected native harness failure: {other:?}"),
            };
            Value::object([
                ("ok", false.into()),
                ("type", kind.into()),
                ("message", Value::String(message)),
            ])
        }
    }
}
fn sqlite_bytes(row: &Value) -> Value {
    Value::Object(
        row.as_object()
            .expect("owned SQL row")
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    match value {
                        Value::Bytes(bytes) => {
                            Value::object([("sqlite_bytes_hex", hex(bytes).into())])
                        }
                        value => value.clone(),
                    },
                )
            })
            .collect(),
    )
}
fn snapshot(connection: &SqliteSession) -> EngineResult<Value> {
    Ok(Value::object([
        ("run", connection.query_values("SELECT state_revision,status FROM runs WHERE run_id='RUN-1'", &[])?.into_iter().next().expect("seeded run")),
        ("operations", Value::list(connection.query_values("SELECT operation_id,request_digest,CAST(response_json AS BLOB) AS response_bytes,typeof(response_json) AS response_type,resulting_revision FROM operations WHERE run_id='RUN-1' ORDER BY operation_id", &[])?.iter().map(sqlite_bytes))),
        ("events", Value::list(connection.query_values("SELECT event_id,run_id,revision,event_type,source_id,CAST(detail_json AS BLOB) AS detail_bytes,typeof(detail_json) AS detail_type FROM events WHERE run_id='RUN-1' ORDER BY event_id", &[])?.iter().map(sqlite_bytes))),
    ]))
}
fn prepare(
    connection: &SqliteSession,
    log: &Path,
    step: &Value,
    labels: &BTreeMap<String, Value>,
) -> EngineResult<PreparedUsage> {
    let operation = step["operation"].clone();
    let action = &step["action"];
    let null = Value::Null;
    let checkpoint = step["checkpoint_from"]
        .text()
        .and_then(|label| labels.get(&label));
    let binding = step["binding_from"]
        .text()
        .and_then(|label| labels.get(&label))
        .map_or(&null, |value| &value["binding_id"]);
    let phase = step
        .get("phase")
        .cloned()
        .unwrap_or_else(|| "scoping".into());
    let generation = 0_i64.into();
    let run = connection.run("RUN-1")?.expect("seeded run");
    usage_record::prepare(
        UsageArguments {
            operation_id: &operation,
            action,
            session_log: log,
            start_offset: checkpoint.map_or(&null, |value| &value["offset"]),
            source_id: checkpoint.map_or(&null, |value| &value["source_id"]),
            prefix_sha256: checkpoint.map_or(&null, |value| &value["prefix_sha256"]),
            binding_id: binding,
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
        connection,
        "RUN-1",
        &run,
        &NativeUsageSource,
    )
}
fn call(
    connection: &SqliteSession,
    log: &Path,
    step: &Value,
    labels: &BTreeMap<String, Value>,
) -> EngineResult<Value> {
    prepare(connection, log, step, labels)?.execute(
        connection,
        |_, _: &RunRecord| Ok(()),
        &|_: &str| Ok(()),
    )
}
fn clock(connection: &SqliteSession) -> EngineResult<Value> {
    let columns = connection
        .query_values("PRAGMA table_info(events)", &[])?
        .iter()
        .filter_map(|row| row["name"].text())
        .filter(|name| name.ends_with("_at") || name.contains("timestamp"))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Ok(Value::List(Vec::new()));
    }
    let query = format!(
        "SELECT event_id,{} FROM events ORDER BY event_id",
        columns
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(Value::list(connection.query_values(&query, &[])?))
}
fn main() -> io::Result<()> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        return Err(io::Error::other(
            "expected seed, reference directory and output directory",
        ));
    }
    let artifact_path = PathBuf::from(
        std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT")
            .ok_or_else(|| io::Error::other("ART required"))?,
    );
    let artifact = fs::canonicalize(&artifact_path)?;
    let seed = contained(Path::new(&arguments[0]), &artifact)?;
    let reference = contained(Path::new(&arguments[1]), &artifact)?;
    let output = PathBuf::from(&arguments[2]);
    contained(
        output
            .parent()
            .ok_or_else(|| io::Error::other("output parent required"))?,
        &artifact,
    )?;
    fs::create_dir(&output)?;
    let seed_bytes = fs::read(&seed)?;
    let actions_bytes = fs::read(reference.join("actions.json"))?;
    let actions = compat::loads_value(&Value::Bytes(actions_bytes.clone())).map_err(io_error)?;
    let fixtures = contained(&artifact_path.join("fixtures"), &artifact)?;
    let mut results = Vec::new();
    for case in actions.as_list().expect("generated scenarios") {
        let root = loop {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(io_error)?
                .as_nanos();
            let candidate =
                fixtures.join(format!("usage-db-native-{}-{nonce}", std::process::id()));
            match fs::create_dir(&candidate) {
                Ok(()) => break contained(&candidate, &fixtures)?,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let database = root.join("usage.sqlite3");
        fs::write(&database, &seed_bytes)?;
        let log = root.join("synthetic-session.jsonl");
        fs::write(&log, bytes(&case["initial_hex"]))?;
        let connection = SqliteSession::connect_existing(&database).map_err(io_error)?;
        let mut labels = BTreeMap::new();
        let mut original_json = None;
        let mut rows = Vec::new();
        for step in case["steps"].as_list().expect("generated steps") {
            let mut result = Value::Null;
            match step["kind"].text().as_deref() {
                Some("append") => fs::OpenOptions::new()
                    .append(true)
                    .open(&log)?
                    .write_all(&bytes(&step["data_hex"]))?,
                Some("unlink") => fs::remove_file(&log)?,
                Some("replace") => {
                    let old = step["old"]
                        .text()
                        .expect("generated ASCII replacement")
                        .into_bytes();
                    let new = step["new"]
                        .text()
                        .expect("generated ASCII replacement")
                        .into_bytes();
                    let data = fs::read(&log)?;
                    let mut replaced = Vec::new();
                    let mut position = 0;
                    while position < data.len() {
                        if data[position..].starts_with(&old) {
                            replaced.extend(&new);
                            position += old.len();
                        } else {
                            replaced.push(data[position]);
                            position += 1;
                        }
                    }
                    fs::write(&log, replaced)?;
                }
                Some("truncate") => fs::OpenOptions::new()
                    .write(true)
                    .open(&log)?
                    .set_len(step["size"].as_i64().expect("generated nonnegative size") as u64)?,
                Some("checkpoint") => {
                    result = outcome(usage_source::checkpoint(&NativeUsageSource, &log))
                }
                Some("report") => {
                    result = outcome(usage_report::report(
                        &connection,
                        &connection
                            .run("RUN-1")
                            .map_err(io_error)?
                            .expect("seeded run")
                            .fields,
                    ))
                }
                Some("abort_row") => {
                    connection
                        .execute_values(
                            "UPDATE runs SET status='aborted' WHERE run_id='RUN-1'",
                            &[],
                        )
                        .map_err(io_error)?;
                }
                Some("corrupt_json") => {
                    original_json = Some(connection.query_values("SELECT detail_json FROM events WHERE event_type='usage.observation' ORDER BY event_id", &[]).map_err(io_error)?[0]["detail_json"].clone());
                    connection.execute_values("UPDATE events SET detail_json='{bad}' WHERE event_type='usage.observation'", &[]).map_err(io_error)?;
                }
                Some("append_bad_text") => {
                    connection.execute_values("INSERT INTO events(run_id,revision,event_type,source_id,detail_json) SELECT run_id,revision,event_type,source_id,CAST(X'ff' AS TEXT) FROM events WHERE event_type='usage.observation' LIMIT 1", &[]).map_err(io_error)?;
                }
                Some("restore_json") => {
                    connection.execute_values("UPDATE events SET detail_json=? WHERE event_id=(SELECT MIN(event_id) FROM events WHERE event_type='usage.observation')", &[original_json.clone().expect("saved original observation")]).map_err(io_error)?;
                }
                Some("call") => {
                    if let Some(winner) = step["interleave"].text() {
                        let prepared = prepare(&connection, &log, step, &labels);
                        let mut interleaved = Vec::new();
                        result = match prepared {
                            Ok(prepared) => {
                                let mut winner_step = step.clone();
                                winner_step.set("operation", winner.into());
                                interleaved.push(outcome(call(
                                    &connection,
                                    &log,
                                    &winner_step,
                                    &labels,
                                )));
                                outcome(
                                    prepared.execute(&connection, |_, _| Ok(()), &|_: &str| Ok(())),
                                )
                            }
                            Err(error) => outcome(Err(error)),
                        };
                        result.set("interleaved", Value::list(interleaved));
                    } else {
                        result = outcome(call(&connection, &log, step, &labels));
                    }
                }
                _ => return Err(io::Error::other("unregistered generated step")),
            }
            if result["ok"].is_true() {
                if let Some(label) = step["label"].text() {
                    labels.insert(label, result["value"].clone());
                }
            }
            rows.push(Value::object([
                ("step", step.clone()),
                ("outcome", result),
                ("snapshot", snapshot(&connection).map_err(io_error)?),
            ]));
        }
        results.push(Value::object([
            ("name", case["name"].clone()),
            ("fixture", root.to_string_lossy().as_ref().into()),
            ("rows", Value::list(rows)),
            (
                "clock_diagnostics_excluded_from_matching",
                clock(&connection).map_err(io_error)?,
            ),
        ]));
    }
    let report = Value::object([
        ("scenarios", Value::Integer(results.len().into())),
        ("records", Value::list(results)),
        ("seed_sha256", ids::sha256_bytes(&seed_bytes).into()),
        ("actions_sha256", ids::sha256_bytes(&actions_bytes).into()),
        ("native_binary_sha256", ids::sha256_bytes(&fs::read(std::env::current_exe()?)?).into()),
        ("proof_limit", "component SQL/API proof; private no-op semantic callback; no full GraphCase/CLI/core-semantic or whole DB file byte claim".into()),
    ]);
    fs::write(
        output.join("report.json"),
        compat::dumps(&report, SerializationProfile::SORTED_STORED).map_err(io_error)?,
    )?;
    println!(
        "{{\"scenarios\":{}}}",
        report["scenarios"].as_i64().unwrap()
    );
    Ok(())
}
