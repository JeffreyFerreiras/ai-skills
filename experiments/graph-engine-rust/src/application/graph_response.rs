use crate::compat::Value;

pub fn json_result(ok: bool, code: &str, run_id: Value, revision: Value) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("ok", ok.into()),
        ("code", code.into()),
        ("run_id", run_id),
        ("state_revision", revision),
    ])
}
