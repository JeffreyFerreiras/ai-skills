use graph_engine::{
    compat::{self, SerializationProfile, Value},
    domain::{
        errors::ValidationFailure,
        ids,
        python_datetime::{utc_after_at, Seconds},
    },
};
use num_bigint::BigInt;
use std::{fs, io::Write, path::PathBuf};

enum Argument {
    Value(Value),
    Complex { real: f64, imaginary: f64 },
}
impl Argument {
    fn seconds(&self) -> Seconds<'_> {
        match self {
            Self::Value(value) => Seconds::Value(value),
            Self::Complex { real, imaginary } => Seconds::Complex {
                real: *real,
                imaginary: *imaginary,
            },
        }
    }
    fn descriptor(&self) -> Value {
        let descriptor = match self {
            Self::Complex { real, imaginary } => Value::object([
                ("real_bits", format!("{:016x}", real.to_bits()).into()),
                (
                    "imaginary_bits",
                    format!("{:016x}", imaginary.to_bits()).into(),
                ),
            ]),
            Self::Value(Value::Integer(value)) => value.to_string().into(),
            Self::Value(Value::Float(value)) => format!("{:016x}", value.to_bits()).into(),
            Self::Value(value) => value.clone(),
        };
        Value::object([
            ("type", self.seconds().python_type().into()),
            ("value", descriptor),
        ])
    }
}

fn value(value: impl Into<Value>) -> Argument {
    Argument::Value(value.into())
}

fn fixed() -> time::OffsetDateTime {
    time::Date::from_calendar_date(2026, time::Month::November, 3)
        .unwrap()
        .with_hms_nano(11, 0, 0, 123_456_000)
        .unwrap()
        .assume_utc()
}

fn cases() -> Vec<(&'static str, Argument)> {
    let upper = time::Date::from_calendar_date(9999, time::Month::December, 31)
        .unwrap()
        .with_hms_nano(23, 59, 59, 999_999_000)
        .unwrap()
        .assume_utc();
    let lower = time::Date::from_calendar_date(1, time::Month::January, 1)
        .unwrap()
        .with_hms(0, 0, 0)
        .unwrap()
        .assume_utc();
    let upper_seconds = (upper - fixed()).whole_microseconds().div_euclid(1_000_000);
    let lower_seconds = (lower - fixed()).whole_microseconds().div_euclid(1_000_000);
    vec![
        ("zero", value(0_i64)),
        ("one", value(1_i64)),
        ("negative_one", value(-1_i64)),
        ("policy_min", value(30_i64)),
        ("policy_max", value(3600_i64)),
        ("false", value(false)),
        ("true", value(true)),
        ("float_fraction", value(Value::Float(0.5))),
        ("float_negative_fraction", value(Value::Float(-0.5))),
        ("float_tiny_positive", value(Value::Float(0.0000005))),
        ("float_tiny_negative", value(Value::Float(-0.0000005))),
        (
            "datetime_upper_last",
            value(Value::Integer(upper_seconds.into())),
        ),
        (
            "datetime_upper_over",
            value(Value::Integer((upper_seconds + 1).into())),
        ),
        (
            "datetime_lower_over",
            value(Value::Integer(lower_seconds.into())),
        ),
        (
            "datetime_lower_first",
            value(Value::Integer((lower_seconds + 1).into())),
        ),
        ("timedelta_max_days", value(999_999_999_i64 * 86400)),
        ("timedelta_days_over", value(1_000_000_000_i64 * 86400)),
        ("timedelta_min_days", value(-999_999_999_i64 * 86400)),
        ("timedelta_days_under", value(-1_000_000_000_i64 * 86400)),
        (
            "int64_positive",
            value(Value::Integer(BigInt::from(1_u8) << 63_usize)),
        ),
        (
            "int64_negative",
            value(Value::Integer(-(BigInt::from(1_u8) << 63_usize))),
        ),
        (
            "huge_positive",
            value(Value::Integer(BigInt::from(10_u8).pow(100))),
        ),
        (
            "huge_negative",
            value(Value::Integer(-BigInt::from(10_u8).pow(100))),
        ),
        ("float_huge_positive", value(Value::Float(1e300))),
        ("float_huge_negative", value(Value::Float(-1e300))),
        ("float_infinity", value(Value::Float(f64::INFINITY))),
        (
            "float_negative_infinity",
            value(Value::Float(f64::NEG_INFINITY)),
        ),
        ("float_nan", value(Value::Float(f64::NAN))),
        ("none", value(Value::Null)),
        ("string", value("1")),
        ("list", value(Value::List(vec![]))),
        ("mapping", value(Value::object([]))),
        ("tuple", value(Value::Tuple(vec![]))),
        (
            "complex",
            Argument::Complex {
                real: 1.0,
                imaginary: 0.0,
            },
        ),
    ]
}

fn evidence_directory(artifacts: &std::path::Path) -> PathBuf {
    let parent = if let Some(path) = std::env::var_os("GRAPH_RUST_TEST_EVIDENCE_ROOT") {
        let path = PathBuf::from(path);
        assert!(path.is_absolute());
        assert!(path
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(artifacts));
        fs::create_dir_all(&path).unwrap();
        path.canonicalize().unwrap()
    } else {
        artifacts.join("parity").canonicalize().unwrap()
    };
    assert!(parent.starts_with(artifacts));
    // These are immutable comparison records, retained even when an assertion fails.
    tempfile::Builder::new()
        .prefix("utc-after-native-")
        .tempdir_in(parent)
        .unwrap()
        .keep()
}

#[test]
fn retained_fixed_clock_reference_matches_all_34_raw_records() {
    let artifacts = PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let expected_path = artifacts.join("compat/utc-after-reference-01/stdout.bin");
    let expected = fs::read(&expected_path).unwrap();
    let cases = cases();
    assert_eq!(cases.len(), 34);
    assert_eq!(
        expected
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count(),
        34
    );
    let output = evidence_directory(&artifacts);
    let profile = SerializationProfile {
        ensure_ascii: true,
        spaced: false,
        sorted: false,
    };
    let inputs = Value::list(cases.iter().map(|(name, argument)| {
        Value::object([("case", (*name).into()), ("seconds", argument.descriptor())])
    }));
    let inputs = compat::dumps(&inputs, SerializationProfile::SORTED_STORED).unwrap();
    fs::write(output.join("inputs.json"), &inputs).unwrap();
    let mut actual_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("stdout.bin"))
        .unwrap();
    let mut records = Vec::new();
    for (name, argument) in &cases {
        let mut record = Value::object([
            ("case", (*name).into()),
            ("value_type", argument.seconds().python_type().into()),
        ]);
        match utc_after_at(fixed(), argument.seconds()) {
            Ok(result) => {
                record.set("outcome", "ok".into());
                record.set("result", result.into());
            }
            Err(ValidationFailure::Python { kind, message }) => {
                record.set("outcome", "error".into());
                record.set("exception", kind.into());
                record.set("message", Value::String(message));
            }
            Err(error) => {
                record.set("outcome", "unexpected_native_error".into());
                record.set("message", format!("{error:?}").into());
            }
        }
        actual_file
            .write_all(&compat::dumps(&record, profile).unwrap())
            .unwrap();
        actual_file.write_all(b"\n").unwrap();
        records.push(record);
    }
    drop(actual_file);
    let actual = fs::read(output.join("stdout.bin")).unwrap();
    let expected_records = expected
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| compat::loads(line).unwrap())
        .collect::<Vec<_>>();
    let differences = records
        .iter()
        .zip(&expected_records)
        .enumerate()
        .filter(|(_, (native, source))| !native.container_eq(source))
        .map(|(index, (native, source))| {
            Value::object([
                ("index", (index as i64).into()),
                ("native", native.clone()),
                ("source", source.clone()),
            ])
        })
        .collect::<Vec<_>>();
    let report = Value::object([
        ("cases", 34_i64.into()), ("differences", Value::List(differences)),
        ("raw_equal", (actual == expected).into()),
        ("source_stdout_sha256", ids::sha256_bytes(&expected).into()),
        ("native_stdout_sha256", ids::sha256_bytes(&actual).into()),
        ("native_inputs_sha256", ids::sha256_bytes(&inputs).into()),
        ("reference_scope", "One substituted fixed aware UTC clock; not exhaustive UTC clock behavior".into()),
        ("reference_wrapper_limit", "datetime.py was captured afterward only; the reference probe did not guard its wrapper bytes before/after".into()),
        ("input_binding", "Native inputs reconstruct the fixed expressions in the byte-guarded saved probe.py; per-case source seconds were not separately serialized".into()),
        ("reference_invocations", 0_i64.into()),
    ]);
    fs::write(
        output.join("report.json"),
        compat::dumps(&report, SerializationProfile::SORTED_STORED).unwrap(),
    )
    .unwrap();
    assert!(
        report["differences"].as_list().unwrap().is_empty(),
        "see retained native report at {}",
        output.display()
    );
    assert!(
        actual == expected,
        "raw bytes differ; retained evidence at {}",
        output.display()
    );
}

#[test]
fn fractional_microseconds_round_ties_to_even_before_second_truncation() {
    let midnight = time::Date::from_calendar_date(2026, time::Month::November, 3)
        .unwrap()
        .with_hms(0, 0, 0)
        .unwrap()
        .assume_utc();
    for (offset, expected) in [
        (-0.0000005, "2026-11-03T00:00:00Z"),
        (-0.0000015, "2026-11-02T23:59:59Z"),
        (0.9999995, "2026-11-03T00:00:01Z"),
        (0.9999985, "2026-11-03T00:00:00Z"),
    ] {
        assert_eq!(
            utc_after_at(midnight, Seconds::Value(&Value::Float(offset))).unwrap(),
            expected
        );
    }
}
