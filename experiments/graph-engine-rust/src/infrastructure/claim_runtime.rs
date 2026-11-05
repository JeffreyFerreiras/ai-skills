use super::state_platform::NativeStatePlatform;
use crate::application::{
    check_fences::CheckClockPort,
    claim_runtime::ClaimRuntimePort,
    error::{EngineError, EngineResult},
    persistent_store::StatePlatform,
};
use crate::{
    compat::Value,
    domain::python_datetime::{self, Seconds},
};
use num_bigint::BigInt;
use std::io;

pub struct NativeClaimRuntime;

impl CheckClockPort for NativeClaimRuntime {
    fn utc_now(&self) -> EngineResult<String> {
        NativeStatePlatform.utc_now()
    }
}

impl ClaimRuntimePort for NativeClaimRuntime {
    fn utc_now_microseconds(&self) -> EngineResult<i128> {
        Ok(time::OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .div_euclid(1_000))
    }
    fn random_bytes(&self, count: usize) -> EngineResult<Vec<u8>> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(count)
            .map_err(|_| EngineError::python("MemoryError", ""))?;
        bytes.resize(count, 0);
        getrandom::getrandom(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
        Ok(bytes)
    }

    fn utc_after(&self, seconds: &BigInt) -> EngineResult<String> {
        Ok(python_datetime::utc_after_at(
            time::OffsetDateTime::now_utc(),
            Seconds::Value(&Value::Integer(seconds.clone())),
        )?)
    }
}
