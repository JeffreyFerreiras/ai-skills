use graph_engine::{
    compat::{PyString, SerializationProfile, Value},
    domain::python_dictionary::{DictionaryValue, PythonDictionary},
};

fn profile(ascii: bool) -> SerializationProfile {
    SerializationProfile {
        ensure_ascii: ascii,
        spaced: false,
        sorted: true,
    }
}
fn dictionary(entries: impl IntoIterator<Item = (Value, Value)>) -> PythonDictionary {
    let mut result = PythonDictionary::default();
    for (key, value) in entries {
        result.insert(key, DictionaryValue::Value(value)).unwrap();
    }
    result
}

#[test]
fn sorting_uses_original_numeric_keys_before_json_key_conversion() {
    let value = dictionary([
        (10_i64.into(), "ten".into()),
        (2_i64.into(), "two".into()),
        (1_i64.into(), "one".into()),
    ]);
    assert_eq!(
        value.serialize(profile(false)).unwrap(),
        br#"{"1":"one","2":"two","10":"ten"}"#
    );
}

#[test]
fn equal_bool_and_integer_keys_keep_the_first_key_representative() {
    let value = dictionary([
        (true.into(), "first".into()),
        (2_i64.into(), "two".into()),
        (1_i64.into(), "updated".into()),
    ]);
    assert_eq!(
        value.serialize(profile(false)).unwrap(),
        br#"{"true":"updated","2":"two"}"#
    );
}

#[test]
fn mixed_key_comparison_errors_precede_json_key_encoding() {
    let value = dictionary([(Value::Null, 0_i64.into()), (1_i64.into(), 1_i64.into())]);
    let error = value.serialize(profile(false)).unwrap_err();
    assert_eq!(error.kind, "TypeError");
    assert_eq!(
        error.message,
        "'<' not supported between instances of 'int' and 'NoneType'"
    );
    let value = dictionary([
        (Value::Bytes(b"a".to_vec()), 0_i64.into()),
        ("b".into(), 1_i64.into()),
    ]);
    let error = value.serialize(profile(false)).unwrap_err();
    assert_eq!(error.kind, "TypeError");
    assert_eq!(
        error.message,
        "'<' not supported between instances of 'str' and 'bytes'"
    );
}

#[test]
fn one_unsupported_key_reaches_the_json_key_type_error() {
    let value = dictionary([(Value::Bytes(b"a".to_vec()), 0_i64.into())]);
    let error = value.serialize(profile(false)).unwrap_err();
    assert_eq!(error.kind, "TypeError");
    assert_eq!(
        error.message,
        "keys must be str, int, float, bool or None, not bytes"
    );
}

#[test]
fn sorted_profile_applies_recursively_to_private_dictionaries() {
    let mut nested = dictionary([(10_i64.into(), "ten".into()), (2_i64.into(), "two".into())]);
    nested.set("nested", Value::Null);
    // A mixed string/integer nested key still fails before outer serialization can finish.
    let mut outer = PythonDictionary::default();
    outer.set("value", nested);
    assert_eq!(
        outer.serialize(profile(false)).unwrap_err().kind,
        "TypeError"
    );
    let nested = dictionary([(10_i64.into(), "ten".into()), (2_i64.into(), "two".into())]);
    let mut outer = PythonDictionary::default();
    outer.set("z", Value::from(0_i64));
    outer.set("a", nested);
    assert_eq!(
        outer.serialize(profile(false)).unwrap(),
        br#"{"a":{"2":"two","10":"ten"},"z":0}"#
    );
}

#[test]
fn lone_surrogate_keys_are_escaped_or_raise_at_utf8_encoding() {
    let value = dictionary([(Value::String(PyString(vec![0xd800])), 1_i64.into())]);
    assert_eq!(value.serialize(profile(true)).unwrap(), br#"{"\ud800":1}"#);
    assert_eq!(
        value.serialize(profile(false)).unwrap_err().kind,
        "UnicodeEncodeError"
    );
}

#[test]
fn long_leading_nan_key_keeps_powersort_trim_order_and_float_spelling() {
    let mut entries = vec![(Value::ConstantNaN, Value::from("nan"))];
    entries.extend(
        (1..130)
            .rev()
            .map(|key| (Value::Float(key as f64), Value::from(key as i64))),
    );
    let value = dictionary(entries);
    let actual = value.serialize(profile(false)).unwrap();
    let expected = format!(
        "{{\"NaN\":\"nan\",{}}}",
        (1..130)
            .map(|key| format!("\"{key}.0\":{key}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(actual, expected.as_bytes());
}
