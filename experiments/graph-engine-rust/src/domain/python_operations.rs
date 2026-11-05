use super::errors::{ValidationFailure, ValidationResult};
use crate::compat::Value;
use num_bigint::BigInt;
use num_traits::{FromPrimitive, ToPrimitive};
use std::cmp::Ordering;

fn integer(value: &Value) -> Option<BigInt> {
    match value {
        Value::Integer(value) => Some(value.clone()),
        Value::Bool(value) => Some(BigInt::from(u8::from(*value))),
        _ => None,
    }
}

fn float(value: &Value) -> Option<f64> {
    match value {
        Value::Float(value) => Some(*value),
        Value::ConstantNaN => Some(f64::NAN),
        _ => None,
    }
}

fn integer_float_order(integer: &BigInt, float: f64) -> Option<Ordering> {
    if float.is_nan() {
        return None;
    }
    if float == f64::INFINITY {
        return Some(Ordering::Less);
    }
    if float == f64::NEG_INFINITY {
        return Some(Ordering::Greater);
    }
    let integral = BigInt::from_f64(float.trunc()).unwrap();
    let ordering = integer.cmp(&integral);
    Some(if !ordering.is_eq() {
        ordering
    } else if float.fract() > 0.0 {
        Ordering::Less
    } else if float.fract() < 0.0 {
        Ordering::Greater
    } else {
        Ordering::Equal
    })
}

pub fn order(
    mut left: &Value,
    mut right: &Value,
    operator: &str,
) -> ValidationResult<Option<Ordering>> {
    loop {
        if let Some(left_integer) = integer(left) {
            if let Some(right_integer) = integer(right) {
                return Ok(Some(left_integer.cmp(&right_integer)));
            }
            if let Some(right_float) = float(right) {
                return Ok(integer_float_order(&left_integer, right_float));
            }
        }
        if let Some(left_float) = float(left) {
            if let Some(right_integer) = integer(right) {
                return Ok(integer_float_order(&right_integer, left_float).map(Ordering::reverse));
            }
            if let Some(right_float) = float(right) {
                return Ok(left_float.partial_cmp(&right_float));
            }
        }
        match (left, right) {
            (Value::String(left), Value::String(right)) => return Ok(Some(left.cmp(right))),
            (Value::Bytes(left), Value::Bytes(right)) => return Ok(Some(left.cmp(right))),
            (Value::List(left_values), Value::List(right_values))
            | (Value::Tuple(left_values), Value::Tuple(right_values)) => {
                if let Some((next_left, next_right)) = left_values
                    .iter()
                    .zip(right_values)
                    .find(|(left, right)| !left.container_eq(right))
                {
                    left = next_left;
                    right = next_right;
                } else {
                    return Ok(Some(left_values.len().cmp(&right_values.len())));
                }
            }
            _ => {
                return Err(ValidationFailure::python(
                    "TypeError",
                    format!(
                        "'{operator}' not supported between instances of '{}' and '{}'",
                        left.python_type(),
                        right.python_type()
                    ),
                ))
            }
        }
    }
}

pub fn less(left: &Value, right: &Value) -> ValidationResult<bool> {
    Ok(order(left, right, "<")? == Some(Ordering::Less))
}
pub fn greater(left: &Value, right: &Value) -> ValidationResult<bool> {
    Ok(order(left, right, ">")? == Some(Ordering::Greater))
}
pub fn greater_equal(left: &Value, right: &Value) -> ValidationResult<bool> {
    Ok(matches!(
        order(left, right, ">=")?,
        Some(Ordering::Greater | Ordering::Equal)
    ))
}
pub fn less_equal(left: &Value, right: &Value) -> ValidationResult<bool> {
    Ok(matches!(
        order(left, right, "<=")?,
        Some(Ordering::Less | Ordering::Equal)
    ))
}

pub fn length(value: &Value) -> ValidationResult<usize> {
    match value {
        Value::String(value) => Ok(value.len()),
        Value::Bytes(value) => Ok(value.len()),
        Value::List(value) | Value::Tuple(value) => Ok(value.len()),
        Value::Object(value) => Ok(value.len()),
        _ => Err(ValidationFailure::python(
            "TypeError",
            format!("object of type '{}' has no len()", value.python_type()),
        )),
    }
}

fn numeric_float(value: &Value) -> ValidationResult<f64> {
    if let Some(float) = float(value) {
        return Ok(float);
    }
    integer(value)
        .unwrap()
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            ValidationFailure::python("OverflowError", "int too large to convert to float")
        })
}

pub fn subtract(left: &Value, right: &Value) -> ValidationResult<Value> {
    if let Some(left_integer) = integer(left) {
        if let Some(right_integer) = integer(right) {
            return Ok(Value::Integer(left_integer - right_integer));
        }
    }
    if (integer(left).is_some() || float(left).is_some())
        && (integer(right).is_some() || float(right).is_some())
    {
        return Ok(Value::Float(numeric_float(left)? - numeric_float(right)?));
    }
    Err(ValidationFailure::python(
        "TypeError",
        format!(
            "unsupported operand type(s) for -: '{}' and '{}'",
            left.python_type(),
            right.python_type()
        ),
    ))
}

pub fn add(left: &Value, right: &Value) -> ValidationResult<Value> {
    if let Some(left_integer) = integer(left) {
        if let Some(right_integer) = integer(right) {
            return Ok(Value::Integer(left_integer + right_integer));
        }
    }
    if (integer(left).is_some() || float(left).is_some())
        && (integer(right).is_some() || float(right).is_some())
    {
        return Ok(Value::Float(numeric_float(left)? + numeric_float(right)?));
    }
    match (left, right) {
        (Value::String(left), Value::String(right)) => {
            let mut joined = left.clone();
            joined.0.extend_from_slice(&right.0);
            Ok(Value::String(joined))
        }
        (Value::List(left), Value::List(right)) => {
            let mut joined = left.clone();
            joined.extend_from_slice(right);
            Ok(Value::List(joined))
        }
        (Value::Tuple(left), Value::Tuple(right)) => {
            let mut joined = left.clone();
            joined.extend_from_slice(right);
            Ok(Value::Tuple(joined))
        }
        (Value::Bytes(left), Value::Bytes(right)) => {
            let mut joined = left.clone();
            joined.extend_from_slice(right);
            Ok(Value::Bytes(joined))
        }
        (Value::String(_), _) => Err(ValidationFailure::python(
            "TypeError",
            format!(
                "can only concatenate str (not \"{}\") to str",
                right.python_type()
            ),
        )),
        (Value::Tuple(_), _) => Err(ValidationFailure::python(
            "TypeError",
            format!(
                "can only concatenate {} (not \"{}\") to {}",
                left.python_type(),
                right.python_type(),
                left.python_type()
            ),
        )),
        (Value::List(_), _) => Err(ValidationFailure::python(
            "TypeError",
            format!(
                "can only concatenate list (not \"{}\") to list",
                right.python_type()
            ),
        )),
        (Value::Bytes(_), _) => Err(ValidationFailure::python(
            "TypeError",
            format!("can't concat {} to bytes", right.python_type()),
        )),
        _ => Err(ValidationFailure::python(
            "TypeError",
            format!(
                "unsupported operand type(s) for +: '{}' and '{}'",
                left.python_type(),
                right.python_type()
            ),
        )),
    }
}

pub fn sort_comparable_values(values: &mut [Value]) -> ValidationResult<()> {
    sort_by_less(values, less)
}

pub fn sort_by_less<T>(
    values: &mut [T],
    mut less: impl FnMut(&T, &T) -> ValidationResult<bool>,
) -> ValidationResult<()> {
    if values.len() < 2 {
        return Ok(());
    }
    let mut indices = super::python_sort::identity_indices(values.len())?;
    let result = super::python_sort::sort_indices(&mut indices, |left, right| {
        less(&values[left], &values[right])
    });
    // Restore the permutation even after a comparison error, as list.sort does.
    for start in 0..indices.len() {
        let mut current = start;
        while indices[current] != start {
            let next = indices[current];
            values.swap(current, next);
            indices[current] = current;
            current = next;
        }
        indices[current] = current;
    }
    result
}
