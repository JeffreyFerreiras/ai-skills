use super::{
    check_fences::CheckClockPort, error::EngineResult, process::ProcessPort,
    source_capture::SourceFileFactory,
};
use base64::Engine;
use num_bigint::BigInt;

pub struct ClaimIdentity {
    pub attempt_id: String,
    pub claim_token: String,
    pub claim_digest: String,
}

pub fn new_claim_identity(runtime: &dyn ClaimRuntimePort) -> EngineResult<ClaimIdentity> {
    let attempt = runtime.random_bytes(12)?;
    let attempt = attempt
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let token = runtime.random_bytes(32)?;
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token);
    let claim_token = format!("claim-{token}");
    Ok(ClaimIdentity {
        attempt_id: format!("attempt-{attempt}"),
        claim_digest: crate::domain::ids::sha256_bytes(claim_token.as_bytes()),
        claim_token,
    })
}

pub trait ClaimRuntimePort: CheckClockPort {
    fn utc_now_microseconds(&self) -> EngineResult<i128>;
    fn random_bytes(&self, count: usize) -> EngineResult<Vec<u8>>;
    fn utc_after(&self, seconds: &BigInt) -> EngineResult<String>;
}

pub fn lease_expired(
    value: &crate::compat::Value,
    clock: &dyn ClaimRuntimePort,
) -> EngineResult<bool> {
    use crate::domain::{
        errors::ValidationFailure,
        python_isoformat::{prepare_lease_expiry, LeaseExpiry},
    };
    match prepare_lease_expiry(value)? {
        LeaseExpiry::Expired => Ok(true),
        expiry => {
            let now = clock.utc_now_microseconds()?;
            match expiry {
                LeaseExpiry::Instant(expiry) => Ok(now >= expiry),
                LeaseExpiry::Naive => Err(ValidationFailure::python(
                    "TypeError",
                    "can't compare offset-naive and offset-aware datetimes",
                )
                .into()),
                LeaseExpiry::Expired => unreachable!(),
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct ClaimServices<'a> {
    pub runtime: &'a dyn ClaimRuntimePort,
    pub source_files: &'a dyn SourceFileFactory,
    pub process: &'a dyn ProcessPort,
}
