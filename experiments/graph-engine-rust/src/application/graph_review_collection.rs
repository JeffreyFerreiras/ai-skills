use super::{
    artifacts::persist_artifact,
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    graph_payload::sorted_inputs,
    graph_transitions::branch_settled,
    graph_writes::GraphWrite,
    persistent_store::{PersistentSession, StatePlatform},
};
use crate::{
    compat::{self, SerializationProfile, Value},
    domain::{errors, evidence, python_operations, review_collection::freeze_terminal_member, review_findings::consolidate_findings},
};

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}
fn optional<'a>(value: &'a Value, key: &str, default: &'a Value) -> EngineResult<&'a Value> {
    if value.as_object().is_none() { return Err(errors::attribute_error(value, "get").into()); }
    Ok(value.get(key).unwrap_or(default))
}
fn required_row(ledger: &dyn GraphLedgerReadPort, query: GraphRead, parameters: &[Value]) -> EngineResult<Value> {
    ledger.first(query, parameters)?.ok_or_else(|| EngineError::python("TypeError", "'NoneType' object is not subscriptable"))
}
fn content(ledger: &dyn GraphLedgerReadPort, reference: &Value) -> EngineResult<Value> {
    let row = required_row(ledger, GraphRead::ArtifactContent, std::slice::from_ref(reference))?;
    Ok(compat::loads_value(item(&row, "content_json")?)?)
}

pub fn cumulative_binding(ledger: &dyn GraphLedgerReadPort, parent: &Value, generation: &Value) -> EngineResult<Value> {
    let mut slots = Vec::new();
    let mut children = Vec::new();
    let mut terminal = Vec::new();
    let mut sources = Vec::new();
    for row in ledger.all(GraphRead::SealedReviewCollections, &[parent.clone(), generation.clone()])? {
        let content = compat::loads_value(item(&row, "content_json")?)?;
        let slot = item(&row, "request_slot_id")?;
        slots.push(Value::List(vec![slot.clone(), item(&row, "collection_digest")?.clone()]));
        for member in errors::iterable_values(item(&content, "members")?)? {
            children.push(Value::List(vec![slot.clone(), item(&member, "branch_id")?.clone(), item(&member, "assignment_id")?.clone(), item(&member, "ordinal")?.clone()]));
            if !item(&member, "status")?.is_text("succeeded") {
                let outcome = item(&member, "terminal")?;
                terminal.push(Value::object([
                    ("request_slot_id", slot.clone()), ("branch_id", item(&member, "branch_id")?.clone()),
                    ("status", item(&member, "status")?.clone()), ("reason_code", item(outcome, "reason_code")?.clone()),
                    ("failure_code", item(outcome, "failure_code")?.clone()),
                ]));
            }
        }
        for issue in errors::iterable_values(item(&content, "issues")?)? {
            for provenance in errors::iterable_values(item(&issue, "provenance")?)? {
                let source_slot = optional(&provenance, "request_slot_id", slot)?;
                sources.push(Value::List(vec![source_slot.clone(), item(&issue, "issue_key")?.clone(), item(&provenance, "branch_id")?.clone(), item(&provenance, "finding_id")?.clone(), item(&provenance, "assignment_id")?.clone(), item(&provenance, "ordinal")?.clone()]));
            }
        }
    }
    python_operations::sort_comparable_values(&mut sources)?;
    Ok(Value::object([("review_request_slots", Value::List(slots)), ("child_members", Value::List(children)), ("terminal_non_successes", Value::List(terminal)), ("finding_sources", Value::List(sources))]))
}

pub fn settle(
    session: &dyn PersistentSession,
    run: &Value,
    slot: &Value,
    clock: &dyn StatePlatform,
) -> EngineResult<Value> {
    if !slot.truthy() { return Ok(Value::Null); }
    let request = session.first(GraphRead::ReviewRequest, std::slice::from_ref(slot))?;
    let Some(request) = request else { return Ok(Value::Null); };
    if item(&request, "status")?.is_text("sealed") { return Ok(item(&request, "preliminary_ref")?.clone()); }
    let members = session.all(GraphRead::ReviewSettlementMembers, std::slice::from_ref(slot))?;
    if members.is_empty() { return Ok(Value::Null); }
    for member in &members {
        if !branch_settled(member)? { return Ok(Value::Null); }
    }
    let preliminary = content(session, item(&request, "preliminary_ref")?)?;
    let parent_id = item(&request, "parent_branch_id")?;
    let parent = required_row(session, GraphRead::Node, std::slice::from_ref(parent_id))?;
    let parent_envelope = compat::loads_value(item(&parent, "envelope_json")?)?;
    let mut sources = Vec::new();
    let mut frozen = Vec::new();
    if item(&preliminary, "findings")?.truthy() {
        sources.push(Value::object([
            ("request_slot_id", slot.clone()), ("branch_id", parent_id.clone()), ("role", "code_reviewer".into()),
            ("model", item(&parent_envelope, "model")?.clone()), ("assignment_id", "primary-reviewer".into()),
            ("ordinal", 0_i64.into()), ("review_lens", "preliminary".into()), ("findings", item(&preliminary, "findings")?.clone()),
        ]));
    }
    for member in members {
        let result = if item(&member, "result_json")?.truthy() { compat::loads_value(item(&member, "result_json")?)? } else { Value::object([]) };
        let findings = if item(&member, "status")?.is_text("succeeded") { optional(&result, "findings", &Value::List(vec![]))?.clone() } else { Value::List(vec![]) };
        let envelope = compat::loads_value(item(&member, "envelope_json")?)?;
        sources.push(Value::object([
            ("request_slot_id", slot.clone()), ("branch_id", item(&member, "branch_id")?.clone()),
            ("role", item(&member, "role")?.clone()), ("model", item(&envelope, "model")?.clone()),
            ("assignment_id", item(&member, "assignment_id")?.clone()), ("ordinal", item(&member, "ordinal")?.clone()),
            ("review_lens", item(item(&envelope, "review_assignment")?, "review_lens")?.clone()), ("findings", findings),
        ]));
        let mut reference = compat::PyString::from("ledger:");
        reference.0.extend(compat::repr::value_str(item(&member, "branch_id")?)?.0);
        reference.0.extend(compat::PyString::from("#sha256=").0);
        reference.0.extend(compat::repr::value_str(item(&member, "result_digest")?)?.0);
        let result_artifact = session.first(GraphRead::ReviewSettlementArtifact, &[Value::String(reference)])?;
        let attempts = session.all(GraphRead::ReviewSettlementAttempts, &[item(&member, "branch_id")?.clone()])?;
        let result_artifact = result_artifact.ok_or_else(|| EngineError::state("TERMINAL_RESULT_MISSING"))?;
        let value = freeze_terminal_member(&member, &envelope, &result, &result_artifact, &Value::List(attempts))?.string_keyed_value().expect("persisted review collection has string keys");
        frozen.push(value);
    }
    let manifest = Value::object([
        ("schema_version", 1_i64.into()), ("kind", "review_nested_collection".into()), ("run_id", item(run, "run_id")?.clone()),
        ("request_slot_id", slot.clone()), ("parent_branch_id", parent_id.clone()),
        ("preliminary_findings", item(&preliminary, "findings")?.clone()), ("members", Value::List(frozen)),
        ("issues", consolidate_findings(&Value::List(sources))?),
    ]);
    let identifier = python_operations::add(slot, &"-collection".into())?;
    let artifact = evidence::canonical_ledger_artifact(identifier.as_string().expect("validated slot is text"), &"collection".into(), &manifest)?;
    persist_artifact(session, &item(run, "run_id")?.text().expect("validated run ID"), &artifact)?;
    let sealed = Value::from(clock.utc_now()?);
    session.write(GraphWrite::SealReviewCollection, &[artifact.reference.clone(), artifact.sha256.clone(), artifact.content_json.clone(), sealed.clone(), slot.clone()])?;
    session.write(GraphWrite::SealReviewRequest, &[sealed, slot.clone()])?;
    let parent = session.first(GraphRead::Node, std::slice::from_ref(parent_id))?;
    let parent = parent.filter(|value| value["status"].is_text("waiting_for_review_children")).ok_or_else(|| EngineError::state("DELEGATION_PARENT_STATE_INVALID"))?;
    let mut envelope = compat::loads_value(item(&parent, "envelope_json")?)?;
    let preliminary_row = required_row(session, GraphRead::Artifact, &[item(&request, "preliminary_ref")?.clone()])?;
    let mut inputs = errors::iterable_values(item(&envelope, "inputs")?)?;
    let preliminary_input = Value::object([("kind", item(&preliminary_row, "kind")?.clone()), ("ref", item(&preliminary_row, "ref")?.clone()), ("sha256", item(&preliminary_row, "sha256")?.clone()), ("size_bytes", item(&preliminary_row, "size_bytes")?.clone())]);
    let mut collection_input = artifact.as_input();
    collection_input.set("content", manifest);
    inputs.extend([preliminary_input, collection_input]);
    let mut unique: Vec<Value> = Vec::new();
    for input in inputs {
        let reference = item(&input, "ref")?;
        errors::require_hashable(reference)?;
        if let Some(index) = unique.iter().position(|value| value["ref"].py_eq(reference)) { unique[index] = input; } else { unique.push(input); }
    }
    envelope.set("status", "ready".into());
    for key in ["attempt_id", "claim_digest", "lease_expires_at", "finished_at"] { envelope.set(key, Value::Null); }
    envelope.set("inputs", sorted_inputs(unique)?);
    envelope.set("review_continuation", cumulative_binding(session, parent_id, item(&request, "generation")?)?);
    session.write(GraphWrite::ResumeReviewParent, &[Value::String(compat::dumps_text(&envelope, SerializationProfile::SORTED_STORED)?), item(&parent, "branch_id")?.clone()])?;
    Ok(artifact.reference)
}
