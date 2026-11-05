use graph_engine::compat::{
    integer::{parse_decimal, DecimalError},
    unicode, PyString,
};

#[test]
fn decimal_values_use_frozen_unicode_properties() {
    for (point, expected) in [
        (0x30, 0),
        (0x39, 9),
        (0x0665, 5),
        (0xff19, 9),
        (0x1d7ce, 0),
        (0x1d7d7, 9),
        (0x1d7d8, 0),
        (0x1d7ff, 9),
    ] {
        assert_eq!(unicode::decimal(point), Some(expected));
    }
    for point in [0xb2, 0x2165, 0xd800, 0x110000] {
        assert_eq!(unicode::decimal(point), None);
    }
}

#[test]
fn python_decimal_grammar_and_limit() {
    for (text, expected) in [
        (" +١_２۳ ", "123"),
        ("\u{85}-০০৭\u{3000}", "-7"),
        ("-0", "0"),
        ("1_000", "1000"),
    ] {
        assert_eq!(
            parse_decimal(&PyString::from(text)).unwrap().to_string(),
            expected
        );
    }
    for text in ["", "+", "_1", "1_", "1__0", "1.0", "²", "\u{1c}1"] {
        assert_eq!(
            parse_decimal(&PyString::from(text)),
            Err(DecimalError::InvalidLiteral)
        );
    }
    assert!(parse_decimal(&PyString::from("9".repeat(4300))).is_ok());
    assert_eq!(
        parse_decimal(&PyString::from("9".repeat(4301))),
        Err(DecimalError::DigitLimit)
    );
}
