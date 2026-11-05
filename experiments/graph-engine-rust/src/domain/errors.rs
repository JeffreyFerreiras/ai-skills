use super::contracts::ContractError;
use crate::compat::{PyString, Value};

#[derive(Debug)]
pub enum ValidationFailure {
    Contract(ContractError),
    Python {
        kind: &'static str,
        message: PyString,
    },
}
impl From<ContractError> for ValidationFailure {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

pub(crate) struct LocatedValidationFailure<Site> {
    pub(crate) error: ValidationFailure,
    pub(crate) site: Option<Site>,
}

impl<Site> LocatedValidationFailure<Site> {
    pub(crate) fn at(error: ValidationFailure, site: Site) -> Self {
        Self {
            error,
            site: Some(site),
        }
    }
}

impl<Site> From<ValidationFailure> for LocatedValidationFailure<Site> {
    fn from(error: ValidationFailure) -> Self {
        Self { error, site: None }
    }
}

impl<Site> From<ContractError> for LocatedValidationFailure<Site> {
    fn from(error: ContractError) -> Self {
        ValidationFailure::from(error).into()
    }
}

impl ValidationFailure {
    pub fn python(kind: &'static str, message: impl Into<PyString>) -> Self {
        Self::Python {
            kind,
            message: message.into(),
        }
    }
    pub fn contract(field: &str, code: &str) -> Self {
        ContractError::new(field, code).into()
    }
}
pub type ValidationResult<T> = Result<T, ValidationFailure>;
impl From<crate::compat::integer::PythonIntegerError> for ValidationFailure {
    fn from(error: crate::compat::integer::PythonIntegerError) -> Self {
        Self::Python {
            kind: error.kind,
            message: error.message,
        }
    }
}

pub fn python_type(value: &Value) -> &'static str {
    value.python_type()
}

pub fn attribute_error(value: &Value, attribute: &str) -> ValidationFailure {
    ValidationFailure::python(
        "AttributeError",
        format!(
            "'{}' object has no attribute '{attribute}'",
            python_type(value)
        ),
    )
}

pub fn require_hashable(value: &Value) -> ValidationResult<()> {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::List(_) | Value::Object(_) => {
                return Err(ValidationFailure::python(
                    "TypeError",
                    format!("unhashable type: '{}'", python_type(value)),
                ));
            }
            Value::Tuple(items) => pending.extend(items.iter().rev()),
            _ => (),
        }
    }
    Ok(())
}

pub fn set_values(values: &[Value]) -> ValidationResult<Vec<Value>> {
    let mut unique = Vec::new();
    for value in values {
        require_hashable(value)?;
        if !unique.iter().any(|previous: &Value| {
            previous.py_eq(value)
                || matches!((previous, value), (Value::ConstantNaN, Value::ConstantNaN))
        }) {
            unique.push(value.clone());
        }
    }
    Ok(unique)
}

pub fn iterable_set(value: &Value) -> ValidationResult<Vec<Value>> {
    match value {
        Value::List(values) | Value::Tuple(values) => set_values(values),
        Value::Object(items) => Ok(items
            .iter()
            .map(|(key, _)| Value::String(key.clone()))
            .collect()),
        Value::String(text) => set_values(
            &text
                .0
                .iter()
                .map(|&point| Value::String(PyString(vec![point])))
                .collect::<Vec<_>>(),
        ),
        Value::Bytes(bytes) => set_values(
            &bytes
                .iter()
                .map(|&byte| Value::from(i64::from(byte)))
                .collect::<Vec<_>>(),
        ),
        _ => Err(ValidationFailure::python(
            "TypeError",
            format!("'{}' object is not iterable", python_type(value)),
        )),
    }
}

pub fn iterable_values(value: &Value) -> ValidationResult<Vec<Value>> {
    match value {
        Value::List(values) | Value::Tuple(values) => Ok(values.clone()),
        Value::Object(items) => Ok(items
            .iter()
            .map(|(key, _)| Value::String(key.clone()))
            .collect()),
        Value::String(text) => Ok(text
            .0
            .iter()
            .map(|&point| Value::String(PyString(vec![point])))
            .collect()),
        Value::Bytes(bytes) => Ok(bytes
            .iter()
            .map(|&byte| Value::from(i64::from(byte)))
            .collect()),
        _ => Err(ValidationFailure::python(
            "TypeError",
            format!("'{}' object is not iterable", python_type(value)),
        )),
    }
}

pub fn container_equal(left: &Value, right: &Value) -> bool {
    left.container_eq(right)
}

pub fn dictionary_items(value: &Value) -> ValidationResult<Vec<(Value, Value)>> {
    if let Value::Object(items) = value {
        return Ok(items
            .iter()
            .map(|(key, value)| (Value::String(key.clone()), value.clone()))
            .collect());
    }
    let mut items: Vec<(Value, Value)> = Vec::new();
    for (index, item) in iterable_values(value)?.into_iter().enumerate() {
        let pair = iterable_values(&item).map_err(|_| {
            ValidationFailure::python(
                "TypeError",
                format!("cannot convert dictionary update sequence element #{index} to a sequence"),
            )
        })?;
        if pair.len() != 2 {
            return Err(ValidationFailure::python(
                "ValueError",
                format!(
                    "dictionary update sequence element #{index} has length {}; 2 is required",
                    pair.len()
                ),
            ));
        }
        require_hashable(&pair[0])?;
        if let Some((_, value)) = items
            .iter_mut()
            .find(|(key, _)| container_equal(key, &pair[0]))
        {
            *value = pair[1].clone();
        } else {
            items.push((pair[0].clone(), pair[1].clone()));
        }
    }
    Ok(items)
}

pub fn dictionaries_equal(left: &Value, right: &Value) -> ValidationResult<bool> {
    let left = dictionary_items(left)?;
    let right = dictionary_items(right)?;
    Ok(left.len() == right.len()
        && left.iter().all(|(key, value)| {
            right
                .iter()
                .find(|(other, _)| container_equal(key, other))
                .is_some_and(|(_, other)| container_equal(value, other))
        }))
}

pub fn set_contains(values: &[Value], value: &Value) -> bool {
    values.iter().any(|item| {
        item.py_eq(value) || matches!((item, value), (Value::ConstantNaN, Value::ConstantNaN))
    })
}

pub fn string_item<'a>(value: &'a Value, key: &str) -> ValidationResult<&'a Value> {
    if let Value::Object(items) = value {
        return items
            .iter()
            .find(|(name, _)| name.equals(key))
            .map(|(_, item)| item)
            .ok_or_else(|| ValidationFailure::python("KeyError", format!("'{key}'")));
    }
    let message = match value {
        Value::List(_) => "list indices must be integers or slices, not str".to_string(),
        Value::Tuple(_) => "tuple indices must be integers or slices, not str".to_string(),
        Value::String(_) => "string indices must be integers, not 'str'".to_string(),
        Value::Bytes(_) => "byte indices must be integers or slices, not str".to_string(),
        _ => format!("'{}' object is not subscriptable", python_type(value)),
    };
    Err(ValidationFailure::python("TypeError", message))
}
