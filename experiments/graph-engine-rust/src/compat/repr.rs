use super::{unicode, JsonError, PyString, SerializationProfile, Value};

pub fn string_repr(text: &PyString) -> PyString {
    let quote = if text.0.contains(&39) && !text.0.contains(&34) {
        34
    } else {
        39
    };
    let mut result = vec![quote];
    for &point in &text.0 {
        match point {
            92 => result.extend([92, 92]),
            9 => result.extend([92, 116]),
            10 => result.extend([92, 110]),
            13 => result.extend([92, 114]),
            value if value == quote => result.extend([92, quote]),
            value if unicode::property(value, 7) => result.push(value),
            value => {
                let escaped = if value < 0x100 {
                    format!("\\x{value:02x}")
                } else if value < 0x10000 {
                    format!("\\u{value:04x}")
                } else {
                    format!("\\U{value:08x}")
                };
                result.extend(escaped.chars().map(u32::from));
            }
        }
    }
    result.push(quote);
    PyString(result)
}

fn bytes_repr(bytes: &[u8]) -> PyString {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut result = vec![u32::from(b'b'), u32::from(quote)];
    for byte in bytes {
        let escaped = match *byte {
            b'\\' => Some("\\\\".into()),
            b'\t' => Some("\\t".into()),
            b'\n' => Some("\\n".into()),
            b'\r' => Some("\\r".into()),
            byte if byte == quote => Some(format!("\\{}", char::from(byte))),
            32..=126 => None,
            byte => Some(format!("\\x{byte:02x}")),
        };
        if let Some(escaped) = escaped {
            result.extend(escaped.chars().map(u32::from));
        } else {
            result.push(u32::from(*byte));
        }
    }
    result.push(u32::from(quote));
    PyString(result)
}

pub fn value_str(value: &Value) -> Result<PyString, JsonError> {
    if let Value::String(text) = value {
        Ok(text.clone())
    } else {
        value_repr(value)
    }
}

pub fn value_repr(value: &Value) -> Result<PyString, JsonError> {
    enum Task<'a> {
        Value(&'a Value),
        Points(PyString),
        Text(&'static str),
    }
    let mut tasks = vec![Task::Value(value)];
    let mut result = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Points(text) => result.extend(text.0),
            Task::Text(text) => result.extend(text.chars().map(u32::from)),
            Task::Value(value) => match value {
                Value::Null => result.extend("None".chars().map(u32::from)),
                Value::Bool(value) => {
                    result.extend(if *value { "True" } else { "False" }.chars().map(u32::from))
                }
                Value::String(text) => result.extend(string_repr(text).0),
                Value::Bytes(bytes) => result.extend(bytes_repr(bytes).0),
                Value::ConstantNaN => result.extend("nan".chars().map(u32::from)),
                Value::Float(value) if !value.is_finite() => result.extend(
                    if value.is_nan() {
                        "nan"
                    } else if value.is_sign_negative() {
                        "-inf"
                    } else {
                        "inf"
                    }
                    .chars()
                    .map(u32::from),
                ),
                Value::Integer(_) | Value::Float(_) => result.extend(
                    super::dumps(value, SerializationProfile::CLI)?
                        .into_iter()
                        .map(u32::from),
                ),
                Value::List(items) => {
                    tasks.push(Task::Text("]"));
                    for (index, item) in items.iter().enumerate().rev() {
                        tasks.push(Task::Value(item));
                        if index != 0 {
                            tasks.push(Task::Text(", "));
                        }
                    }
                    tasks.push(Task::Text("["));
                }
                Value::Tuple(items) => {
                    tasks.push(Task::Text(")"));
                    if items.len() == 1 {
                        tasks.push(Task::Text(","));
                    }
                    for (index, item) in items.iter().enumerate().rev() {
                        tasks.push(Task::Value(item));
                        if index != 0 {
                            tasks.push(Task::Text(", "));
                        }
                    }
                    tasks.push(Task::Text("("));
                }
                Value::Object(items) => {
                    tasks.push(Task::Text("}"));
                    for (index, (key, item)) in items.iter().enumerate().rev() {
                        tasks.push(Task::Value(item));
                        tasks.push(Task::Text(": "));
                        tasks.push(Task::Points(string_repr(key)));
                        if index != 0 {
                            tasks.push(Task::Text(", "));
                        }
                    }
                    tasks.push(Task::Text("{"));
                }
            },
        }
    }
    Ok(PyString(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_keep_python_repr_quotes_bytes_and_special_numbers() {
        let value = Value::object([
            (
                "a",
                Value::list([Value::Bytes(vec![0, 39, 255]), Value::Null, true.into()]),
            ),
            ("b", Value::Float(-0.0)),
            ("c", Value::Float(f64::INFINITY)),
        ]);
        assert!(value_repr(&value)
            .unwrap()
            .equals("{'a': [b\"\\x00'\\xff\", None, True], 'b': -0.0, 'c': inf}"));
        assert!(
            value_str(&Value::String(PyString(vec![0xd800]))).unwrap() == PyString(vec![0xd800])
        );
        assert!(value_repr(&Value::String(PyString(vec![0xd800])))
            .unwrap()
            .equals("'\\ud800'"));
        assert!(value_repr(&Value::ConstantNaN).unwrap().equals("nan"));
    }

    #[test]
    fn deeply_nested_repr_uses_an_explicit_stack_through_cleanup() {
        let mut value = Value::Null;
        for _ in 0..3000 {
            value = Value::list([value]);
        }
        let text = value_repr(&value).unwrap();
        assert_eq!(text.len(), 6004);
        drop(value);
    }
}
