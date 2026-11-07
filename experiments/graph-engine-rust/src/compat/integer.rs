use super::{unicode, PyString};
use num_bigint::BigInt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecimalError {
    InvalidLiteral,
    DigitLimit,
}

fn integer_space(point: u32) -> bool {
    matches!(point, 9..=13 | 32) || (point > 127 && unicode::whitespace(point))
}

fn parse_number(
    mut points: &[u32],
    decimal: impl Fn(u32) -> Option<u32>,
    space: impl Fn(u32) -> bool,
) -> Result<BigInt, DecimalError> {
    while points.first().is_some_and(|&point| space(point)) {
        points = &points[1..];
    }
    while points.last().is_some_and(|&point| space(point)) {
        points = &points[..points.len() - 1];
    }
    let mut ascii = Vec::new();
    if points
        .first()
        .is_some_and(|&point| point == 43 || point == 45)
    {
        ascii.push(points[0] as u8);
        points = &points[1..];
    }
    let mut digits = 0;
    let mut digit_required = true;
    for &point in points {
        if point == 95 {
            if digit_required {
                return Err(DecimalError::InvalidLiteral);
            }
            digit_required = true;
        } else {
            let digit = decimal(point).ok_or(DecimalError::InvalidLiteral)?;
            digits += 1;
            if digits <= 4300 {
                ascii.push(b'0' + digit as u8);
            }
            digit_required = false;
        }
    }
    if digit_required {
        return Err(DecimalError::InvalidLiteral);
    }
    if digits > 4300 {
        return Err(DecimalError::DigitLimit);
    }
    BigInt::parse_bytes(&ascii, 10).ok_or(DecimalError::InvalidLiteral)
}

pub fn parse_decimal(text: &PyString) -> Result<BigInt, DecimalError> {
    parse_number(&text.0, unicode::decimal, integer_space)
}
pub fn parse_bytes(bytes: &[u8]) -> Result<BigInt, DecimalError> {
    let points: Vec<_> = bytes.iter().map(|&byte| u32::from(byte)).collect();
    parse_number(
        &points,
        |point| {
            if (48..=57).contains(&point) {
                Some(point - 48)
            } else {
                None
            }
        },
        |point| matches!(point, 9..=13 | 32),
    )
}
#[derive(Debug)]
pub struct PythonIntegerError {
    pub kind: &'static str,
    pub message: PyString,
}
fn literal_repr(text: &PyString, bytes: bool) -> PyString {
    let quote = if text.0.contains(&39) && !text.0.contains(&34) {
        34
    } else {
        39
    };
    let mut output = if bytes { vec![98, quote] } else { vec![quote] };
    for &point in &text.0 {
        let escape = match point {
            9 => Some("\\t".to_string()),
            10 => Some("\\n".to_string()),
            13 => Some("\\r".to_string()),
            92 => Some("\\\\".to_string()),
            value if value == quote => Some(format!("\\{}", char::from_u32(value).unwrap())),
            value
                if (bytes && !(32..=126).contains(&value))
                    || (!bytes && !unicode::property(value, 7)) =>
            {
                Some(if value < 256 {
                    format!("\\x{value:02x}")
                } else if value <= 0xffff {
                    format!("\\u{value:04x}")
                } else {
                    format!("\\U{value:08x}")
                })
            }
            _ => None,
        };
        if let Some(escape) = escape {
            output.extend(escape.chars().map(u32::from));
        } else {
            output.push(point);
        }
    }
    output.push(quote);
    output.truncate(200);
    PyString(output)
}
pub fn coerce(value: &super::Value) -> Result<BigInt, PythonIntegerError> {
    use super::Value;
    use num_traits::FromPrimitive;
    let failure = |kind, message: &str| PythonIntegerError {
        kind,
        message: message.into(),
    };
    let (parsed, text, bytes) = match value {
        Value::Integer(value) => return Ok(value.clone()),
        Value::Bool(value) => return Ok(u8::from(*value).into()),
        Value::Float(value) if value.is_finite() => {
            return Ok(BigInt::from_f64(value.trunc()).unwrap())
        }
        Value::Float(value) if value.is_nan() => {
            return Err(failure("ValueError", "cannot convert float NaN to integer"))
        }
        Value::ConstantNaN => {
            return Err(failure("ValueError", "cannot convert float NaN to integer"))
        }
        Value::Float(_) => {
            return Err(failure(
                "OverflowError",
                "cannot convert float infinity to integer",
            ))
        }
        Value::String(text) => (parse_decimal(text), text.clone(), false),
        Value::Bytes(bytes) => (
            parse_bytes(bytes),
            PyString(bytes.iter().map(|&byte| u32::from(byte)).collect()),
            true,
        ),
        _ => {
            return Err(failure(
                "TypeError",
                &format!(
                "int() argument must be a string, a bytes-like object or a real number, not '{}'",
                value.python_type()
            ),
            ))
        }
    };
    match parsed {
        Ok(value) => Ok(value),
        Err(DecimalError::InvalidLiteral) => {
            let mut message = PyString::from("invalid literal for int() with base 10: ");
            message.0.extend(literal_repr(&text, bytes).0);
            Err(PythonIntegerError {
                kind: "ValueError",
                message,
            })
        }
        Err(DecimalError::DigitLimit) => {
            let digits = text
                .0
                .iter()
                .filter(|&&point| {
                    if bytes {
                        (48..=57).contains(&point)
                    } else {
                        unicode::decimal(point).is_some()
                    }
                })
                .count();
            Err(failure("ValueError",&format!("Exceeds the limit (4300 digits) for integer string conversion: value has {digits} digits; use sys.set_int_max_str_digits() to increase the limit")))
        }
    }
}
