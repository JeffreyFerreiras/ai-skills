use graph_engine::compat::{self, datetime, SerializationProfile, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn outcome(operation: &str, input: &Value) -> Value {
    if operation == "timestamp_microseconds" {
        return match datetime::strict_utc_microseconds(input) {
            Some(value) => Value::object([("kind", "integer".into()), ("value", value.into())]),
            None => Value::object([
                ("kind", "exception".into()),
                ("class", "StateError".into()),
                ("message", "TIMESTAMP_INVALID".into()),
            ]),
        };
    }
    let parsed = if operation == "receipt_datetime" {
        datetime::receipt_datetime(input)
    } else {
        datetime::fromisoformat(input)
    };
    match parsed {
        Ok(value) => Value::object([
            ("kind", "datetime".into()),
            (
                "fields",
                Value::list(
                    [
                        i64::from(value.year),
                        i64::from(value.month),
                        i64::from(value.day),
                        i64::from(value.hour),
                        i64::from(value.minute),
                        i64::from(value.second),
                        i64::from(value.microsecond),
                    ]
                    .into_iter()
                    .map(Value::from),
                ),
            ),
            ("isoformat", value.isoformat().into()),
            ("aware", value.offset_microseconds.is_some().into()),
            (
                "offset_microseconds",
                value
                    .offset_microseconds
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            ),
            ("fold", 0i64.into()),
        ]),
        Err(error) => Value::object([
            ("kind", "exception".into()),
            ("class", error.kind.into()),
            ("message", Value::String(error.message)),
        ]),
    }
}

#[test]
fn retained_datetime_reference_preserves_parsing_fields_errors_and_exact_serialization() {
    let artifact_root =
        PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").expect("ART required"));
    let source = std::fs::read(artifact_root.join("parity/datetime-01/report.json")).unwrap();
    let reference = compat::loads_value(&Value::Bytes(source.clone())).unwrap();
    let mut differences = Vec::new();
    for (index, record) in reference["records"].as_list().unwrap().iter().enumerate() {
        let actual = outcome(&record["operation"].text().unwrap(), &record["input"]);
        if compat::dumps(&actual, SerializationProfile::CANONICAL).unwrap()
            != compat::dumps(&record["expected"], SerializationProfile::CANONICAL).unwrap()
        {
            differences.push(Value::object([
                ("index", (index as i64).into()),
                ("record", record.clone()),
                ("actual", actual),
            ]));
        }
    }
    let report_name = std::env::var("GRAPH_RUST_DATETIME_REPORT").unwrap_or_else(|_| {
        format!(
            "datetime-native-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    });
    assert!(report_name.starts_with("datetime-native-") && !report_name.contains(['/', '\\']));
    let output = artifact_root.join("parity").join(report_name);
    std::fs::create_dir(&output).unwrap();
    let executable = std::env::current_exe().unwrap();
    let report = Value::object([
        ("requests", reference["calls"].clone()),
        (
            "reference_sha256",
            format!("{:x}", Sha256::digest(source)).into(),
        ),
        (
            "binary_sha256",
            format!("{:x}", Sha256::digest(std::fs::read(executable).unwrap())).into(),
        ),
        ("differences", Value::List(differences)),
    ]);
    std::fs::write(
        output.join("report.json"),
        compat::dumps(&report, SerializationProfile::STORED).unwrap(),
    )
    .unwrap();
    assert_eq!(
        report["differences"].as_list().unwrap().len(),
        0,
        "see {}",
        output.display()
    );
}

#[test]
fn receipt_datetime_comparison_preserves_naive_aware_type_error_and_offset_order() {
    let naive = datetime::fromisoformat(&"2024-01-01T00:00:00".into()).unwrap();
    let utc = datetime::fromisoformat(&"2024-01-01T00:00:00Z".into()).unwrap();
    let equivalent = datetime::fromisoformat(&"2024-01-01T01:00:00+01:00".into()).unwrap();
    let error = naive.compare(&utc).unwrap_err();
    assert_eq!(error.kind, "TypeError");
    assert!(error
        .message
        .equals("can't compare offset-naive and offset-aware datetimes"));
    assert_eq!(utc.compare(&equivalent).unwrap(), std::cmp::Ordering::Equal);
}
