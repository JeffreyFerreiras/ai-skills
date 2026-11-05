use graph_engine::{
    compat::{self, PyString, SerializationProfile, Value},
    domain::{contracts, ids},
};

#[test]
fn serialization_profiles_preserve_python_bytes() {
    let value =
        compat::loads(br#"{"z":-0.0,"a":"\u00e9\ud83d\ude00","b":123456789012345678901234567890}"#)
            .unwrap();
    assert_eq!(
        String::from_utf8(ids::canonical_bytes(&value).unwrap()).unwrap(),
        "{\"a\":\"é😀\",\"b\":123456789012345678901234567890,\"z\":-0.0}"
    );
    assert_eq!(
        String::from_utf8(compat::dumps(&value, SerializationProfile::CLI).unwrap()).unwrap(),
        "{\"a\":\"\\u00e9\\ud83d\\ude00\",\"b\":123456789012345678901234567890,\"z\":-0.0}"
    );
    assert_eq!(
        String::from_utf8(compat::dumps(&value, SerializationProfile::STORED).unwrap()).unwrap(),
        "{\"z\": -0.0, \"a\": \"\\u00e9\\ud83d\\ude00\", \"b\": 123456789012345678901234567890}"
    );
}
#[test]
fn lone_surrogates_remain_values_until_utf8_output() {
    let value = compat::loads(br#""\ud800""#).unwrap();
    assert_eq!(value.as_string(), Some(&PyString(vec![0xd800])));
    assert_eq!(
        compat::dumps(&value, SerializationProfile::CLI).unwrap(),
        br#""\ud800""#
    );
    assert_eq!(
        ids::canonical_bytes(&value).unwrap_err().kind,
        "UnicodeEncodeError"
    );
    assert_eq!(
        contracts::lexical_relative(&compat::loads(br#""src/\ud800.txt""#).unwrap(), "ref")
            .unwrap()
            .0,
        vec![115, 114, 99, 47, 0xd800, 46, 116, 120, 116]
    );
}
#[test]
fn python_numeric_equality_retains_arbitrary_precision() {
    let integer = compat::loads(b"9007199254740993").unwrap();
    let rounded = compat::loads(b"9007199254740993.0").unwrap();
    assert!(!integer.py_eq(&rounded));
    assert!(Value::Bool(true).py_eq(&Value::from(1_i64)));
    assert!(Value::Bool(false).py_eq(&Value::Float(-0.0)));
    assert!(!Value::Float(f64::NAN).py_eq(&Value::Float(f64::NAN)));
    assert!(!Value::ConstantNaN.py_eq(&Value::ConstantNaN));
    assert!(Value::list([Value::ConstantNaN]).py_eq(&Value::list([Value::ConstantNaN])));
}
#[test]
fn float_presentation_uses_python_thresholds() {
    for (number, expected) in [
        (1e-7, "1e-07"),
        (1e-5, "1e-05"),
        (1e-4, "0.0001"),
        (1e15, "1000000000000000.0"),
        (1e16, "1e+16"),
        (-0.0, "-0.0"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
    ] {
        assert_eq!(compat::json::float_repr(number), expected);
    }
    assert_eq!(compat::json::float_repr(f64::NAN), "NaN");
}
#[test]
fn contextual_lower_and_casefold_use_frozen_oracle_properties() {
    for (text, lower, folded) in [
        ("ΟΣ", "ος", "οσ"),
        ("ΟΣΑ", "οσα", "οσα"),
        ("AΣ\u{345}", "aς\u{345}", "aσι"),
        ("AΣ\u{345}A", "aσ\u{345}a", "aσιa"),
        ("İΣ", "i\u{307}ς", "i\u{307}σ"),
        ("Σ", "σ", "σ"),
        ("Straße", "straße", "strasse"),
    ] {
        assert_eq!(compat::unicode::lower(&text.into()).utf8().unwrap(), lower);
        assert_eq!(
            compat::unicode::casefold(&text.into()).utf8().unwrap(),
            folded
        );
    }
    assert_eq!(
        compat::unicode::normcase(&"A/ΟΣ".into(), true)
            .utf8()
            .unwrap(),
        "a\\ος"
    );
    assert_eq!(
        compat::unicode::normcase(&"A/ΟΣ".into(), false)
            .utf8()
            .unwrap(),
        "A/ΟΣ"
    );
}
#[test]
fn duplicate_keys_keep_first_position_and_last_value() {
    let value = compat::loads(br#"{"b":1,"a":2,"b":3}"#).unwrap();
    assert_eq!(
        compat::dumps(&value, SerializationProfile::STORED).unwrap(),
        br#"{"b": 3, "a": 2}"#
    );
}
#[test]
fn credentials_and_sensitive_fields_keep_error_precedence() {
    for text in [
        "-----BEGIN FOO PRIVATE KEY-----",
        "Bearer abcDEF123._~+/=-",
        "bearer İ",
        "bearer ı",
        "bearer ſ",
        "bearer K",
        "bearer\u{1c}abc",
    ] {
        assert_eq!(
            contracts::bounded_string(&text.into(), "f", 4096)
                .unwrap_err()
                .code,
            if text.contains('\u{1c}') {
                "CONTROL_CHARACTER"
            } else {
                "CREDENTIAL_VALUE"
            }
        );
    }
    for text in ["bearer ß", "bearer ﬀ"] {
        contracts::bounded_string(&text.into(), "f", 4096).unwrap();
    }
    let value = compat::loads(br#"{"Access-Key":"hidden"}"#).unwrap();
    let error = contracts::reject_sensitive(&value, "root", 0).unwrap_err();
    assert_eq!(error.to_string(), "root.Access-Key:SECRET_FIELD");
    assert!(!error.to_string().contains("hidden"));
}
#[test]
fn reference_validation_keeps_audit_and_content_boundaries() {
    assert_eq!(
        contracts::validate_ref(&"authority:a#b".into(), "f", false)
            .unwrap_err()
            .code,
        "INVALID_ID"
    );
    assert_eq!(
        contracts::validate_ref(&"thread:t1".into(), "f", true)
            .unwrap_err()
            .code,
        "AUDIT_REF_NOT_CONTENT"
    );
    assert_eq!(
        contracts::validate_ref(&"FILE:local".into(), "f", false)
            .unwrap_err()
            .code,
        "URL_FORBIDDEN"
    );
    assert_eq!(
        contracts::validate_ref(
            &"profile:software-engineering-graph/references/a.json".into(),
            "f",
            false
        )
        .unwrap()
        .utf8()
        .unwrap(),
        "profile:software-engineering-graph/references/a.json"
    );
    for text in [
        "/abs",
        "C:/win",
        "a/../b",
        "a/.git/b",
        "key.pem",
        ".env.local",
    ] {
        assert!(contracts::lexical_relative(&text.into(), "ref").is_err());
    }
}

#[test]
fn deep_values_parse_clone_compare_serialize_and_drop_without_native_recursion() {
    for depth in [900, 1000, 2000, 2997] {
        for object in [false, true] {
            let text = if object {
                format!("{}0{}", "{\"a\":".repeat(depth), "}".repeat(depth))
            } else {
                format!("{}0{}", "[".repeat(depth), "]".repeat(depth))
            };
            let value = compat::loads(text.as_bytes()).unwrap();
            let copied = value.clone();
            assert!(value.py_eq(&copied));
            assert_eq!(
                compat::dumps(&copied, SerializationProfile::CLI).unwrap(),
                text.as_bytes()
            );
            drop(value);
            drop(copied);
        }
    }
}
#[test]
fn deep_error_cleanup_keeps_recursion_and_json_failures_distinct() {
    for object in [false, true] {
        let text = if object {
            format!("{}0{}", "{\"a\":".repeat(2998), "}".repeat(2998))
        } else {
            format!("{}0{}", "[".repeat(2998), "]".repeat(2998))
        };
        let error = compat::loads(text.as_bytes()).unwrap_err();
        assert_eq!(error.kind, "RecursionError");
        assert!(error
            .message
            .contains(if object { "JSON object" } else { "JSON array" }));
    }
    let malformed = format!("{}0", "[".repeat(2997));
    let error = compat::loads(malformed.as_bytes()).unwrap_err();
    assert_eq!(error.kind, "JSONDecodeError");
    assert_eq!(error.message, "Expecting ',' delimiter");
    let mut deep = Value::from(0_i64);
    for _ in 0..2998 {
        deep = Value::List(vec![deep]);
    }
    let mixed = Value::List(vec![Value::String(PyString(vec![0xd800])), deep]);
    assert_eq!(
        compat::dumps(&mixed, SerializationProfile::CANONICAL)
            .unwrap_err()
            .kind,
        "RecursionError"
    );
    drop(mixed);
}
