use graph_engine::{
    compat::{self, repr, SerializationProfile, Value},
    domain::{
        errors::{self, ValidationFailure},
        python_dictionary::{DictionaryValue, PythonDictionary},
        python_operations, task_contract,
    },
};

fn assert_python(error: ValidationFailure, expected: &str) {
    let ValidationFailure::Python { kind, message } = error else {
        panic!("expected Python failure")
    };
    assert_eq!(kind, "TypeError");
    assert!(message.equals(expected), "{message:?}");
}

#[test]
fn tuple_equality_keeps_sequence_type_numeric_equality_and_container_identity() {
    assert!(Value::tuple([true.into()]).py_eq(&Value::tuple([1_i64.into()])));
    assert!(!Value::tuple([1_i64.into()]).py_eq(&Value::list([1_i64.into()])));
    assert!(Value::tuple([Value::ConstantNaN]).py_eq(&Value::tuple([Value::ConstantNaN])));
    assert!(!Value::tuple([Value::Float(f64::NAN)]).py_eq(&Value::tuple([Value::Float(f64::NAN)])));
    let value = Value::tuple([Value::Float(f64::NAN)]);
    assert!(value.py_eq(&value));
    let nan = Value::Float(f64::NAN);
    assert!(!nan.py_eq(&nan));
    assert!(nan.container_eq(&nan));
}

#[test]
fn tuple_hashability_reports_the_first_unhashable_member() {
    let value = Value::tuple([Value::tuple([Value::list([])]), Value::object([])]);
    assert!(!value.hashable());
    assert_python(
        errors::require_hashable(&value).unwrap_err(),
        "unhashable type: 'list'",
    );
    assert!(Value::tuple([Value::Bytes(vec![255]), Value::tuple([1_i64.into()])]).hashable());
}

#[test]
fn tuple_outer_pairs_and_tuple_keys_keep_first_key_and_last_value() {
    let input = Value::tuple([
        Value::tuple([Value::tuple([true.into()]), "first".into()]),
        Value::list([Value::tuple([1_i64.into()]), "last".into()]),
    ]);
    let dictionary = PythonDictionary::coerce(&input).unwrap();
    assert_eq!(dictionary.entries.len(), 1);
    assert!(
        matches!(&dictionary.entries[0].0, Value::Tuple(items) if matches!(items[0], Value::Bool(true)))
    );
    let DictionaryValue::Value(value) = &dictionary.entries[0].1 else {
        panic!("scalar value expected")
    };
    assert!(value.is_text("last"));
}

#[test]
fn tuple_order_is_lexicographic_and_keeps_mixed_type_error_direction() {
    assert!(python_operations::less(
        &Value::tuple([Value::ConstantNaN, 1_i64.into()]),
        &Value::tuple([Value::ConstantNaN, 2_i64.into()]),
    )
    .unwrap());
    assert!(!python_operations::less(
        &Value::tuple([Value::Float(f64::NAN), 1_i64.into()]),
        &Value::tuple([Value::Float(f64::NAN), 2_i64.into()]),
    )
    .unwrap());
    assert_python(
        python_operations::less(&Value::tuple([]), &Value::list([])).unwrap_err(),
        "'<' not supported between instances of 'tuple' and 'list'",
    );
    assert!(python_operations::less(
        &Value::tuple([1_i64.into()]),
        &Value::tuple([1_i64.into(), 0_i64.into()])
    )
    .unwrap());
}

#[test]
fn tuple_repr_and_json_use_their_distinct_python_formats() {
    for (value, expected_repr, expected_json) in [
        (Value::tuple([]), "()", "[]"),
        (Value::tuple([1_i64.into()]), "(1,)", "[1]"),
        (
            Value::tuple([1_i64.into(), "x".into()]),
            "(1, 'x')",
            "[1,\"x\"]",
        ),
    ] {
        assert!(repr::value_repr(&value).unwrap().equals(expected_repr));
        assert_eq!(
            compat::dumps(&value, SerializationProfile::CLI).unwrap(),
            expected_json.as_bytes()
        );
    }
    let error = compat::loads_value(&Value::tuple([])).unwrap_err();
    assert_eq!(
        error.message,
        "the JSON object must be str, bytes or bytearray, not tuple"
    );
}

#[test]
fn tuple_dictionary_keys_are_sorted_before_the_json_key_type_failure() {
    let mut dictionary = PythonDictionary::default();
    dictionary
        .insert(Value::tuple([1_i64.into()]), Value::Null.into())
        .unwrap();
    let error = dictionary
        .serialize(SerializationProfile::STORED)
        .unwrap_err();
    assert_eq!(
        error.message,
        "keys must be str, int, float, bool or None, not tuple"
    );
    dictionary.insert("x".into(), Value::Null.into()).unwrap();
    let error = dictionary.serialize(SerializationProfile::CLI).unwrap_err();
    assert_eq!(
        error.message,
        "'<' not supported between instances of 'str' and 'tuple'"
    );
}

#[test]
fn private_authoritative_subset_preserves_reachable_tuple_key_pairs() {
    let task = Value::object([
        ("schema_version", 1_i64.into()),
        ("task_id", "t".into()),
        ("request_mode", "delivery".into()),
        ("minimum_route", "fast_path".into()),
        ("mandatory_impact_tags", Value::tuple([])),
        ("risk_level", "low".into()),
        ("acceptance_criteria", Value::tuple([])),
        ("authority", Value::Null),
        ("policy_approval", Value::Null),
        ("evidence_paths", Value::tuple([])),
        (
            "inspection_budget",
            Value::tuple([Value::tuple([
                Value::tuple(["limit".into(), 1_i64.into()]),
                3_i64.into(),
            ])]),
        ),
        ("required_check_ids", Value::tuple([])),
        ("required_human_decisions", Value::tuple([])),
    ]);
    let subset = task_contract::authoritative_task_subset(&task).unwrap();
    let Some(DictionaryValue::Dictionary(budget)) = subset.get("inspection_budget") else {
        panic!("copied dictionary expected")
    };
    assert!(matches!(&budget.entries[0].0, Value::Tuple(items) if items.len()==2));
    assert!(subset.string_keyed_value().is_none());
}

#[test]
fn tuple_addition_and_list_addition_preserve_their_type_specific_errors() {
    let result =
        python_operations::add(&Value::tuple([1_i64.into()]), &Value::tuple([2_i64.into()]))
            .unwrap();
    assert!(result.py_eq(&Value::tuple([1_i64.into(), 2_i64.into()])));
    assert_python(
        python_operations::add(&Value::tuple([]), &Value::list([])).unwrap_err(),
        "can only concatenate tuple (not \"list\") to tuple",
    );
    assert_python(
        python_operations::add(&Value::list([]), &Value::tuple([])).unwrap_err(),
        "can only concatenate list (not \"tuple\") to list",
    );
}

#[test]
fn deeply_nested_tuple_clone_equality_repr_and_drop_use_explicit_stacks() {
    let mut value = Value::Null;
    for _ in 0..4096 {
        value = Value::tuple([value]);
    }
    let cloned = value.clone();
    assert!(value.py_eq(&cloned));
    assert_eq!(repr::value_repr(&value).unwrap().len(), 4096 * 3 + 4);
}
