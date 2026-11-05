use super::contracts::{self, ContractError};
use super::errors::{attribute_error, require_hashable, ValidationFailure, ValidationResult};
use crate::compat::{PyString, Value};
use std::collections::BTreeSet;

pub fn fail(field: &str, code: &str) -> ValidationFailure {
    ContractError::new(field, code).into()
}

pub fn object(value: &Value, field: &str, code: &str) -> ValidationResult<()> {
    if value.as_object().is_none() {
        return Err(fail(field, code));
    }
    Ok(())
}

pub fn list<'a>(value: &'a Value, field: &str, code: &str) -> ValidationResult<&'a [Value]> {
    value.as_list().ok_or_else(|| fail(field, code))
}

pub fn get<'a>(value: &'a Value, key: &str) -> ValidationResult<&'a Value> {
    if value.as_object().is_none() {
        return Err(attribute_error(value, "get"));
    }
    Ok(&value[key])
}

pub fn values(value: &Value) -> ValidationResult<Vec<&Value>> {
    value
        .as_object()
        .map(|items| items.iter().map(|(_, item)| item).collect())
        .ok_or_else(|| attribute_error(value, "values"))
}

pub fn in_strings(value: &Value, choices: &[&str]) -> ValidationResult<bool> {
    require_hashable(value)?;
    Ok(choices.iter().any(|choice| value.is_text(choice)))
}

pub fn contains(container: &Value, item: &Value) -> ValidationResult<bool> {
    match container {
        Value::List(values) => Ok(values.iter().any(|value| value.container_eq(item))),
        Value::Object(values) => {
            require_hashable(item)?;
            Ok(values.iter().any(|(key, _)| item.as_string() == Some(key)))
        }
        Value::String(text) => {
            let needle = item.as_string().ok_or_else(|| {
                ValidationFailure::python(
                    "TypeError",
                    format!(
                        "'in <string>' requires string as left operand, not {}",
                        item.python_type()
                    ),
                )
            })?;
            Ok(needle.is_empty() || text.0.windows(needle.len()).any(|part| part == needle.0))
        }
        Value::Bytes(bytes) => {
            let number = match item {
                Value::Integer(value) => Some(value.clone()),
                Value::Bool(value) => Some(num_bigint::BigInt::from(u8::from(*value))),
                _ => None,
            };
            if let Some(number) = number {
                use num_traits::ToPrimitive;
                let byte = number.to_u8().ok_or_else(|| {
                    ValidationFailure::python("ValueError", "byte must be in range(0, 256)")
                })?;
                Ok(bytes.contains(&byte))
            } else if let Value::Bytes(needle) = item {
                Ok(needle.is_empty() || bytes.windows(needle.len()).any(|part| part == needle))
            } else {
                Err(ValidationFailure::python(
                    "TypeError",
                    format!(
                        "a bytes-like object is required, not '{}'",
                        item.python_type()
                    ),
                ))
            }
        }
        _ => Err(ValidationFailure::python(
            "TypeError",
            format!(
                "argument of type '{}' is not iterable",
                container.python_type()
            ),
        )),
    }
}

pub fn unique_strings(
    value: &Value,
    field: &str,
    allowed: Option<&[Value]>,
) -> ValidationResult<Vec<PyString>> {
    let mut result = Vec::new();
    for item in list(value, field, "INVALID_LIST")? {
        result.push(contracts::bounded_string(item, field, 1024)?.clone());
    }
    let unique = result.iter().collect::<BTreeSet<_>>();
    if unique.len() != result.len() {
        return Err(fail(field, "DUPLICATE_VALUE"));
    }
    if allowed.is_some_and(|allowed| {
        result
            .iter()
            .any(|item| !allowed.iter().any(|entry| entry.as_string() == Some(item)))
    }) {
        return Err(fail(field, "UNKNOWN_VALUE"));
    }
    result.sort();
    Ok(result)
}

pub fn strings(values: Vec<PyString>) -> Value {
    Value::List(values.into_iter().map(Value::String).collect())
}

pub fn validate_attempt(value: &Value, field: &str) -> ValidationResult<()> {
    contracts::opaque(get(value, "attempt_id")?, &format!("{field}.attempt_id"))?;
    contracts::digest(
        get(value, "claim_digest")?,
        &format!("{field}.claim_digest"),
    )?;
    Ok(())
}

pub fn sort_by_fields(values: &mut [Value], keys: &[&str]) {
    values.sort_by(|left, right| {
        for key in keys {
            let ordering = left[key].as_string().cmp(&right[key].as_string());
            if !ordering.is_eq() {
                return ordering;
            }
        }
        std::cmp::Ordering::Equal
    });
}
