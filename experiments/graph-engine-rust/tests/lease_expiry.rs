use graph_engine::{
    application::{
        check_fences::CheckClockPort,
        claim_runtime::{lease_expired, ClaimRuntimePort},
        error::EngineResult,
    },
    compat::{PyString, Value},
    domain::python_isoformat::{prepare_lease_expiry, LeaseExpiry},
};
use num_bigint::BigInt;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Clock {
    now: i128,
    reads: AtomicUsize,
}
impl Clock {
    fn new(now: i128) -> Self {
        Self {
            now,
            reads: AtomicUsize::new(0),
        }
    }
}
impl CheckClockPort for Clock {
    fn utc_now(&self) -> EngineResult<String> {
        panic!("lease comparison needs full microseconds")
    }
}
impl ClaimRuntimePort for Clock {
    fn utc_now_microseconds(&self) -> EngineResult<i128> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.now)
    }
    fn random_bytes(&self, _: usize) -> EngineResult<Vec<u8>> {
        panic!("lease comparison must not request entropy")
    }
    fn utc_after(&self, _: &BigInt) -> EngineResult<String> {
        panic!("lease comparison must not construct a deadline")
    }
}
fn fixed() -> i128 {
    time::Date::from_calendar_date(2026, time::Month::November, 3)
        .unwrap()
        .with_hms_micro(11, 0, 0, 123456)
        .unwrap()
        .assume_utc()
        .unix_timestamp_nanos()
        .div_euclid(1000)
}
fn instant(value: impl Into<Value>) -> i128 {
    match prepare_lease_expiry(&value.into()).unwrap() {
        LeaseExpiry::Instant(value) => value,
        _ => panic!("expected an aware datetime"),
    }
}

#[test]
fn calendar_basic_week_and_unicode_separator_forms_share_the_same_instant() {
    for text in [
        "2026-11-03T11:00:00.123456Z",
        "20261103T110000.123456+0000",
        "2026-W45-2T11:00:00,123456+00:00",
        "2026W452T110000.123456+0000",
        "2026-11-03😀11:00:00.123456+00:00",
    ] {
        assert_eq!(instant(text), fixed(), "{text}");
    }
}

#[test]
fn offset_normalization_fraction_and_truncated_clock_follow_cpython_source() {
    assert_eq!(instant("2026-11-03T11+00"), fixed() - 123456);
    assert_eq!(instant("2026-11-03T12:+00"), fixed() + 3600000000 - 123456);
    assert_eq!(
        instant("2026-11-03T11:00:00.123456+00:00:00.999999"),
        fixed()
    );
    assert_eq!(
        instant("2026-11-03T11:00:00.123456+00:00:01.000001"),
        fixed() - 1000001
    );
    assert_eq!(
        instant("2026-11-03T11:00:00.123456+00:99"),
        fixed() - 5940000000
    );
}

#[test]
fn invalid_and_falsy_values_expire_before_the_clock_is_read() {
    let clock = Clock::new(fixed());
    for value in [
        Value::Null,
        false.into(),
        0_i64.into(),
        "".into(),
        Value::List(vec![]),
        "bad".into(),
        "2026-02-29T11:00Z".into(),
        "2026-11-03T11:00+24:00".into(),
        "2026-11-03T11:00-24:00".into(),
        "2026-11-03T11:00Zjunk".into(),
    ] {
        assert!(lease_expired(&value, &clock).unwrap());
    }
    assert_eq!(clock.reads.load(Ordering::SeqCst), 0);
}

#[test]
fn valid_naive_datetime_reads_clock_before_comparison_type_error() {
    let clock = Clock::new(fixed());
    let error = lease_expired(&"2026-11-03".into(), &clock).unwrap_err();
    assert!(
        matches!(error, graph_engine::application::error::EngineError::Python { kind: "TypeError", message } if message == PyString::from("can't compare offset-naive and offset-aware datetimes"))
    );
    assert_eq!(clock.reads.load(Ordering::SeqCst), 1);
}

#[test]
fn comparison_uses_microseconds_and_includes_exact_expiry() {
    let clock = Clock::new(fixed());
    assert!(lease_expired(&"2026-11-03T11:00:00.123456Z".into(), &clock).unwrap());
    assert!(!lease_expired(&"2026-11-03T11:00:00.123457Z".into(), &clock).unwrap());
    assert_eq!(clock.reads.load(Ordering::SeqCst), 2);
}

#[test]
fn only_separator_surrogates_are_sanitized_and_nonstring_errors_escape() {
    let mut separator = PyString::from("2026-11-03T11:00:00.123456+00:00");
    separator.0[10] = 0xd800;
    assert_eq!(instant(Value::String(separator)), fixed());
    let invalid = Value::String(PyString(vec![0xd800; 15]));
    assert!(matches!(
        prepare_lease_expiry(&invalid).unwrap(),
        LeaseExpiry::Expired
    ));
    for (value, message) in [
        (
            Value::Bytes(vec![1]),
            "a bytes-like object is required, not 'str'",
        ),
        (true.into(), "'bool' object has no attribute 'replace'"),
    ] {
        let error = prepare_lease_expiry(&value).err().unwrap();
        assert!(
            matches!(error, graph_engine::domain::errors::ValidationFailure::Python { message: actual, .. } if actual == PyString::from(message))
        );
    }
}
