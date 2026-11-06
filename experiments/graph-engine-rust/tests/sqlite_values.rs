use graph_engine::{
    application::error::EngineError,
    compat::{self, SerializationProfile, Value},
    infrastructure::sqlite::SqliteSession,
};

#[test]
fn blob_is_owned_bytes_and_json_rejects_it() {
    let session = SqliteSession::in_memory().unwrap();
    let rows = session
        .query_values("SELECT x'0001ff' AS value", &[])
        .unwrap();
    assert!(matches!(&rows[0]["value"],Value::Bytes(bytes) if bytes==&[0,1,255]));
    let error = compat::dumps(&rows[0], SerializationProfile::CLI).unwrap_err();
    assert_eq!(error.kind, "TypeError");
    assert_eq!(
        error.message,
        "Object of type bytes is not JSON serializable"
    );
}
#[test]
fn row_dictionary_uses_first_ascii_case_insensitive_lookup() {
    let session = SqliteSession::in_memory().unwrap();
    let rows = session.query_values("SELECT 1 AS a,2 AS A", &[]).unwrap();
    assert_eq!(rows[0]["a"].as_i64(), Some(1));
    assert_eq!(rows[0]["A"].as_i64(), Some(1));
    let rows = session
        .query_values("SELECT 1 AS \"Ä\",2 AS \"ä\"", &[])
        .unwrap();
    assert_eq!(rows[0]["Ä"].as_i64(), Some(1));
    assert_eq!(rows[0]["ä"].as_i64(), Some(2));
}
#[test]
fn every_selected_cell_is_decoded_before_dictionary_duplicate_elision() {
    let session = SqliteSession::in_memory().unwrap();
    let error = session
        .query_values("SELECT 1 AS same,CAST(x'ff' AS TEXT) AS same", &[])
        .unwrap_err();
    match error {
        EngineError::Storage { kind, message, .. } => {
            assert_eq!(kind, "OperationalError");
            assert_eq!(
                message,
                "Could not decode to UTF-8 column 'same' with text '�'"
            );
        }
        other => panic!("{other:?}"),
    }
}
#[test]
fn nested_bytes_clone_equality_drop_and_serialization_error_cleanup_are_iterative() {
    let mut value = Value::Bytes(vec![0, 255]);
    for _ in 0..2997 {
        value = Value::list([value]);
    }
    let cloned = value.clone();
    assert!(value.py_eq(&cloned));
    let error = compat::dumps(&value, SerializationProfile::CANONICAL).unwrap_err();
    assert_eq!(error.kind, "RecursionError");
    assert_eq!(error.message, "maximum recursion depth exceeded");
    drop(value);
    drop(cloned);
}

#[test]
fn stored_byte_json_preserves_the_actual_accepted_depth() {
    let depth = 2997;
    let text = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
    let parsed = compat::loads_value(&Value::Bytes(text.as_bytes().to_vec())).unwrap();
    let cloned = parsed.clone();
    assert!(parsed.py_eq(&cloned));
    assert_eq!(
        compat::dumps(&parsed, SerializationProfile::CLI).unwrap(),
        text.as_bytes()
    );
    drop(cloned);
    drop(parsed);
    let rejected = format!("{}0{}", "[".repeat(depth + 1), "]".repeat(depth + 1));
    assert_eq!(
        compat::loads_value(&Value::Bytes(rejected.into_bytes()))
            .unwrap_err()
            .kind,
        "RecursionError"
    );
}

#[test]
fn replay_accepts_blob_json_and_ignores_unused_ledger_column_types() {
    use graph_engine::application::state;
    let session = SqliteSession::in_memory().unwrap();
    session
        .connection()
        .execute_batch("PRAGMA foreign_keys=OFF")
        .unwrap();
    let request = Value::object([("command", "complete".into())]);
    let digest = graph_engine::domain::ids::sha256_bytes(
        &graph_engine::domain::ids::canonical_bytes(&request).unwrap(),
    );
    let mut bytes = vec![0xff, 0xfe];
    for unit in "{\"ok\":true,\"status\":\"complete\"}".encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    session
        .execute_values(
            "INSERT INTO operations VALUES(?,?,?,?,?)",
            &[
                "RUN-BLOB".into(),
                "OP-BLOB".into(),
                digest.into(),
                Value::Bytes(bytes),
                Value::Bytes(vec![1]),
            ],
        )
        .unwrap();
    let result = state::mutate(
        &session,
        "RUN-BLOB",
        "OP-BLOB",
        &request,
        |_, _, _| panic!("replay called action"),
        |_, _| panic!("replay called semantic validation"),
        &|_: &str| panic!("replay called fault hook"),
    )
    .unwrap();
    assert!(result["code"].is_text("REPLAYED"));
    assert!(result["status"].is_text("complete"));
}
