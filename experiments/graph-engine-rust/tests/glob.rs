use graph_engine::compat::{
    glob::{posix_path_match, GlobError},
    PyString,
};

#[test]
fn observed_posix_root_and_empty_path_matching() {
    for (text, pattern, expected) in [
        ("/", "*", false),
        ("//", "*", false),
        ("/", "?", false),
        ("//", "??", false),
        ("//", "[!a-z]", true),
        ("//", "[!]]", true),
        ("/", "[!z-a]", false),
        ("/", "*?/", false),
        ("//", "*?/", false),
        ("", "**", true),
        (".", "**", true),
        ("", "*", false),
        (".", "*", false),
    ] {
        assert_eq!(
            posix_path_match(&PyString::from(text), &PyString::from(pattern)),
            Ok(expected),
            "text={text:?}, pattern={pattern:?}",
        );
    }
    for pattern in ["", "."] {
        assert_eq!(
            posix_path_match(&PyString::from("src/a.py"), &PyString::from(pattern)),
            Err(GlobError::EmptyPattern),
        );
    }
}
