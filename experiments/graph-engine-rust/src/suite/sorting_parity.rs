//! Typed, data-only verification ingress for ordering and private dictionary JSON.
use graph_engine::{
    compat::{JsonError, SerializationProfile, Value},
    domain::{
        errors::{self, ValidationFailure},
        python_dictionary::{DictionaryValue, PythonDictionary},
        python_operations, task_contract,
    },
};

fn scalar(spec: &Value) -> Value {
    match spec["t"].text().unwrap().as_str() {
        "null" => Value::Null,
        "bool" => Value::Bool(spec["v"].is_true()),
        "int" => Value::Integer(spec["v"].text().unwrap().parse().unwrap()),
        "float" => Value::Float(f64::from_bits(
            u64::from_str_radix(&spec["v"].text().unwrap(), 16).unwrap(),
        )),
        "constant_nan" => Value::ConstantNaN,
        "str" => spec["v"].clone(),
        "bytes" => {
            let hex = spec["v"].text().unwrap();
            Value::Bytes(
                (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect(),
            )
        }
        "list" => Value::list(spec["v"].as_list().unwrap().iter().map(scalar)),
        "tuple" => Value::tuple(spec["v"].as_list().unwrap().iter().map(scalar)),
        "object" => Value::Object(
            spec["v"]
                .as_object()
                .unwrap_or(&[])
                .iter()
                .map(|(key, value)| (key.clone(), scalar(value)))
                .collect(),
        ),
        kind => panic!("unrecognized bounded verification scalar: {kind}"),
    }
}

fn descriptor(kind: &str, value: Value) -> Value {
    Value::object([("t", kind.into()), ("v", value)])
}

fn scalar_descriptor(value: &Value) -> Value {
    match value {
        Value::Null => Value::object([("t", "null".into())]),
        Value::Bool(_) => descriptor("bool", value.clone()),
        Value::Integer(number) => descriptor("int", number.to_string().into()),
        Value::Float(number) => descriptor("float", format!("{:016x}", number.to_bits()).into()),
        Value::ConstantNaN => Value::object([("t", "constant_nan".into())]),
        Value::String(_) => descriptor("str", value.clone()),
        Value::Bytes(bytes) => descriptor(
            "bytes",
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
                .into(),
        ),
        Value::List(items) | Value::Tuple(items) => descriptor(
            value.python_type(),
            Value::list(items.iter().map(scalar_descriptor)),
        ),
        Value::Object(items) => descriptor(
            "dict",
            Value::list(items.iter().map(|(key, value)| {
                Value::list([
                    scalar_descriptor(&Value::String(key.clone())),
                    scalar_descriptor(value),
                ])
            })),
        ),
    }
}

fn dictionary_descriptor(dictionary: &PythonDictionary) -> Value {
    descriptor(
        "dict",
        Value::list(dictionary.entries.iter().map(|(key, value)| {
            Value::list([scalar_descriptor(key), dictionary_value_descriptor(value)])
        })),
    )
}

fn dictionary_value_descriptor(value: &DictionaryValue) -> Value {
    match value {
        DictionaryValue::Value(value) => scalar_descriptor(value),
        DictionaryValue::Dictionary(value) => dictionary_descriptor(value),
        DictionaryValue::List(values) => descriptor(
            "list",
            Value::list(values.iter().map(dictionary_value_descriptor)),
        ),
    }
}

fn python_error(error: ValidationFailure) -> Value {
    match error {
        ValidationFailure::Python { kind, message } => Value::object([
            ("kind", kind.into()),
            ("message", Value::String(message)),
            ("position", 0_i64.into()),
        ]),
        ValidationFailure::Contract(error) => Value::object([
            ("kind", "ValueError".into()),
            ("message", error.to_string().into()),
            ("position", 0_i64.into()),
        ]),
    }
}

fn json_error(error: JsonError) -> Value {
    Value::object([
        ("kind", error.kind.into()),
        ("message", error.message.into()),
        ("position", error.position.into()),
    ])
}

fn dictionary_value(spec: &Value) -> Result<DictionaryValue, Value> {
    if spec["t"].is_text("dict") {
        let mut dictionary = PythonDictionary::default();
        for pair in spec["v"].as_list().unwrap() {
            let pair = pair.as_list().unwrap();
            dictionary
                .insert(scalar(&pair[0]), dictionary_value(&pair[1])?)
                .map_err(python_error)?;
        }
        Ok(DictionaryValue::Dictionary(dictionary))
    } else if spec["t"].is_text("list") {
        Ok(DictionaryValue::List(
            spec["v"]
                .as_list()
                .unwrap()
                .iter()
                .map(dictionary_value)
                .collect::<Result<_, _>>()?,
        ))
    } else {
        Ok(DictionaryValue::Value(scalar(spec)))
    }
}

pub fn dispatch(request: &Value) -> Result<Value, Value> {
    if request["op"].is_text("private_subset") {
        return task_contract::authoritative_task_subset(&scalar(&request["task"]))
            .map(|value| dictionary_descriptor(&value))
            .map_err(python_error);
    }
    if request["op"].is_text("tuple_operation") {
        let left = scalar(&request["left"]);
        let right = request.get("right").map(scalar).unwrap_or(Value::Null);
        return match request["operation"].text().unwrap().as_str() {
            "equal" => Ok(Value::Bool(left.py_eq(&right))),
            "less" => python_operations::less(&left, &right)
                .map(Value::Bool)
                .map_err(python_error),
            "add" => python_operations::add(&left, &right)
                .map(|value| scalar_descriptor(&value))
                .map_err(python_error),
            "length" => python_operations::length(&left)
                .map(Value::from)
                .map_err(python_error),
            "repr" => graph_engine::compat::repr::value_repr(&left)
                .map(Value::String)
                .map_err(json_error),
            "hashable" => errors::require_hashable(&left)
                .map(|()| Value::Bool(true))
                .map_err(python_error),
            "coerce" => PythonDictionary::coerce(&left)
                .map(|value| dictionary_descriptor(&value))
                .map_err(python_error),
            operation => panic!("unknown bounded tuple operation: {operation}"),
        };
    }
    if request["op"].is_text("sort_trace") {
        let keys = request["keys"]
            .as_list()
            .unwrap()
            .iter()
            .map(scalar)
            .collect::<Vec<_>>();
        let mut permutation = (0..keys.len()).collect::<Vec<_>>();
        let fail_at = request["fail_at"].as_i64().unwrap_or(0) as usize;
        let mut trace = Vec::new();
        let result = python_operations::sort_by_less(&mut permutation, |&left, &right| {
            trace.push(Value::list([Value::from(left), Value::from(right)]));
            if trace.len() == fail_at {
                return Err(ValidationFailure::python(
                    "TypeError",
                    "injected comparison failure",
                ));
            }
            if trace.len() > 100_000 {
                return Err(ValidationFailure::python(
                    "RuntimeError",
                    "verification comparison bound exceeded",
                ));
            }
            python_operations::less(&keys[left], &keys[right])
        });
        return Ok(Value::object([
            (
                "permutation",
                Value::list(permutation.into_iter().map(Value::from)),
            ),
            ("trace", Value::List(trace)),
            ("error", result.err().map_or(Value::Null, python_error)),
        ]));
    }
    let profile = match request["profile"].text().unwrap().as_str() {
        "canonical" => SerializationProfile::CANONICAL,
        "cli" => SerializationProfile::CLI,
        "stored" => SerializationProfile::STORED,
        "sorted_stored" => SerializationProfile::SORTED_STORED,
        profile => panic!("unrecognized bounded verification profile: {profile}"),
    };
    let DictionaryValue::Dictionary(dictionary) = dictionary_value(&request["dictionary"])? else {
        panic!("bounded dictionary request requires dictionary root")
    };
    dictionary
        .serialize(profile)
        .map(|bytes| {
            Value::from(
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            )
        })
        .map_err(json_error)
}
