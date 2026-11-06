use super::{
    contract_values::get,
    errors::{iterable_values, string_item, ValidationResult},
    python_dictionary::{DictionaryValue, PythonDictionary},
};
use crate::compat::Value;

pub fn freeze_terminal_member(
    member: &Value,
    envelope: &Value,
    result: &Value,
    result_artifact: &Value,
    attempts: &Value,
) -> ValidationResult<PythonDictionary> {
    let empty = Value::List(vec![]);
    let findings = if string_item(member, "status")?.is_text("succeeded") {
        get(result, "findings")?;
        iterable_values(result.get("findings").unwrap_or(&empty))?
    } else {
        vec![]
    };
    let mut frozen = PythonDictionary::default();
    for key in ["branch_id", "assignment_id", "ordinal", "role", "status"] {
        frozen.set(key, string_item(member, key)?.clone());
    }
    frozen.set("findings", Value::List(findings));
    let mut terminal = PythonDictionary::default();
    terminal.set("result", PythonDictionary::coerce(result)?);
    terminal.set(
        "result_artifact",
        PythonDictionary::coerce(result_artifact)?,
    );
    terminal.set("artifact_ref", get(envelope, "artifact_ref")?.clone());
    terminal.set(
        "evidence",
        Value::List(iterable_values(envelope.get("evidence").unwrap_or(&empty))?),
    );
    terminal.set("decision", get(envelope, "decision")?.clone());
    for key in [
        "reason_code",
        "failure_code",
        "result_digest",
        "retry_count",
        "max_retries",
        "started_at",
        "finished_at",
    ] {
        terminal.set(key, string_item(member, key)?.clone());
    }
    let mut frozen_attempts = Vec::new();
    for attempt in iterable_values(attempts)? {
        frozen_attempts.push(DictionaryValue::Dictionary(PythonDictionary::coerce(
            &attempt,
        )?));
    }
    terminal.set("attempts", DictionaryValue::List(frozen_attempts));
    frozen.set("terminal", terminal);
    Ok(frozen)
}
