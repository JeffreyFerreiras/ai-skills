use super::errors::{ValidationFailure, ValidationResult};
use crate::compat::Value;
use num_bigint::{BigInt, Sign};
use num_traits::{FromPrimitive, ToPrimitive};

pub enum Seconds<'a> {
    Value(&'a Value),
    Complex { real: f64, imaginary: f64 },
}

impl Seconds<'_> {
    pub fn python_type(&self) -> &'static str {
        match self {
            Self::Value(value) => value.python_type(),
            Self::Complex { .. } => "complex",
        }
    }
}

fn seconds_microseconds(seconds: Seconds<'_>) -> ValidationResult<BigInt> {
    let unsupported = || {
        ValidationFailure::python(
            "TypeError",
            format!(
                "unsupported type for timedelta seconds component: {}",
                seconds.python_type()
            ),
        )
    };
    let value = match &seconds {
        Seconds::Value(value) => *value,
        Seconds::Complex { .. } => return Err(unsupported()),
    };
    match value {
        Value::Integer(value) => return Ok(value * 1_000_000),
        Value::Bool(value) => return Ok(BigInt::from(u8::from(*value)) * 1_000_000),
        _ => (),
    }
    let value = match value {
        Value::Float(value) => *value,
        Value::ConstantNaN => f64::NAN,
        _ => return Err(unsupported()),
    };
    if value.is_nan() {
        return Err(ValidationFailure::python(
            "ValueError",
            "cannot convert float NaN to integer",
        ));
    }
    if value.is_infinite() {
        return Err(ValidationFailure::python(
            "OverflowError",
            "cannot convert float infinity to integer",
        ));
    }
    let integral = value.trunc();
    let fraction = value - integral;
    let mut microseconds: BigInt = BigInt::from_f64(integral)
        .expect("finite integral float converts to BigInt")
        * 1_000_000_i64;
    if fraction != 0.0 {
        let fractional_microseconds = fraction * 1_000_000.0;
        let integral = fractional_microseconds.trunc();
        let leftover = fractional_microseconds - integral;
        microseconds += BigInt::from_f64(integral).expect("fractional microseconds are finite");
        if leftover != 0.0 {
            let mut rounded = leftover.round();
            if (rounded - leftover).abs() == 0.5 {
                let odd = if (&microseconds & BigInt::from(1_u8)) == BigInt::from(1_u8) {
                    1.0
                } else {
                    0.0
                };
                rounded = 2.0 * ((leftover + odd) * 0.5).round() - odd;
            }
            microseconds += BigInt::from(rounded as i32);
        }
    }
    Ok(microseconds)
}

fn floor_divmod(value: &BigInt, divisor: i64) -> (BigInt, BigInt) {
    let divisor = BigInt::from(divisor);
    let mut quotient = value / &divisor;
    let mut remainder = value % &divisor;
    if remainder.sign() == Sign::Minus {
        quotient -= 1_u8;
        remainder += divisor;
    }
    (quotient, remainder)
}

pub fn format_utc(now: time::OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

pub fn utc_after_at(now: time::OffsetDateTime, seconds: Seconds<'_>) -> ValidationResult<String> {
    let microseconds = seconds_microseconds(seconds)?;
    let (seconds, microseconds) = floor_divmod(&microseconds, 1_000_000);
    let (days, seconds) = floor_divmod(&seconds, 86_400);
    let days = days.to_i32().ok_or_else(|| {
        ValidationFailure::python("OverflowError", "Python int too large to convert to C int")
    })?;
    if !(-999_999_999..=999_999_999).contains(&days) {
        return Err(ValidationFailure::python(
            "OverflowError",
            format!("days={days}; must have magnitude <= 999999999"),
        ));
    }
    let seconds = i64::from(days) * 86_400
        + seconds
            .to_i64()
            .expect("normalized seconds are below one day");
    let microseconds = microseconds
        .to_i64()
        .expect("normalized microseconds are below one second");
    let duration = time::Duration::seconds(seconds) + time::Duration::microseconds(microseconds);
    let now = now
        .replace_nanosecond(now.nanosecond() / 1_000 * 1_000)
        .expect("truncating nanoseconds preserves the component range");
    let later = now
        .checked_add(duration)
        .filter(|value| value.year() >= 1)
        .ok_or_else(|| ValidationFailure::python("OverflowError", "date value out of range"))?;
    Ok(format_utc(later))
}
