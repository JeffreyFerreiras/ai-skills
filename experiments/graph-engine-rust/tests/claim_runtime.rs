use graph_engine::{
    application::{
        check_fences::CheckClockPort,
        claim_runtime::{new_claim_identity, ClaimRuntimePort},
        error::{EngineError, EngineResult},
    },
    domain::ids,
    infrastructure::{claim_runtime::NativeClaimRuntime, state_platform::utc_timestamp},
};
use num_bigint::BigInt;
use std::{cell::RefCell, io};

struct Runtime {
    calls: RefCell<Vec<usize>>,
    fail_token: bool,
}
impl CheckClockPort for Runtime {
    fn utc_now(&self) -> EngineResult<String> {
        panic!("identity generation must not read the clock")
    }
}
impl ClaimRuntimePort for Runtime {
    fn utc_now_microseconds(&self) -> EngineResult<i128> {
        panic!("identity generation must not read the clock")
    }
    fn random_bytes(&self, count: usize) -> EngineResult<Vec<u8>> {
        self.calls.borrow_mut().push(count);
        if self.fail_token && count == 32 {
            return Err(io::Error::other("entropy unavailable").into());
        }
        Ok((0..count).map(|value| value as u8).collect())
    }
    fn utc_after(&self, _: &BigInt) -> EngineResult<String> {
        panic!("identity generation must not read the clock")
    }
}

#[test]
fn identity_uses_the_source_entropy_widths_hex_urlsafe_encoding_and_token_digest() {
    let runtime = Runtime {
        calls: RefCell::new(vec![]),
        fail_token: false,
    };
    let identity = new_claim_identity(&runtime).unwrap();
    assert_eq!(runtime.calls.borrow().as_slice(), &[12, 32]);
    assert_eq!(identity.attempt_id, "attempt-000102030405060708090a0b");
    assert_eq!(
        identity.claim_token,
        "claim-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"
    );
    assert_eq!(
        identity.claim_digest,
        ids::sha256_bytes(identity.claim_token.as_bytes())
    );
}

#[test]
fn entropy_failure_propagates_without_a_token_fallback() {
    let runtime = Runtime {
        calls: RefCell::new(vec![]),
        fail_token: true,
    };
    let result = new_claim_identity(&runtime);
    assert!(
        matches!(result, Err(EngineError::Io(error)) if error.to_string() == "entropy unavailable")
    );
    assert_eq!(runtime.calls.borrow().as_slice(), &[12, 32]);
}

#[test]
fn native_entropy_returns_the_requested_byte_widths() {
    assert_eq!(NativeClaimRuntime.random_bytes(12).unwrap().len(), 12);
    assert_eq!(NativeClaimRuntime.random_bytes(32).unwrap().len(), 32);
}

#[test]
fn utc_format_uses_z_and_discards_fractional_seconds() {
    let instant = time::Date::from_calendar_date(2024, time::Month::February, 29)
        .unwrap()
        .with_hms_nano(23, 59, 58, 123_456_789)
        .unwrap()
        .assume_utc();
    assert_eq!(utc_timestamp(instant), "2024-02-29T23:59:58Z");
}

#[test]
fn configured_lease_deadline_uses_a_fresh_utc_clock_with_second_precision() {
    let before = time::OffsetDateTime::now_utc().unix_timestamp();
    let expiry = NativeClaimRuntime.utc_after(&BigInt::from(900)).unwrap();
    let after = time::OffsetDateTime::now_utc().unix_timestamp();
    let parsed =
        time::OffsetDateTime::parse(&expiry, &time::format_description::well_known::Rfc3339)
            .unwrap();
    assert!(expiry.ends_with('Z'));
    assert_eq!(parsed.nanosecond(), 0);
    assert!(parsed.unix_timestamp() >= before + 900);
    assert!(parsed.unix_timestamp() <= after + 900);
    let now = NativeClaimRuntime.utc_now().unwrap();
    assert!(now.ends_with('Z'));
    assert_eq!(
        time::OffsetDateTime::parse(&now, &time::format_description::well_known::Rfc3339)
            .unwrap()
            .nanosecond(),
        0
    );
}
