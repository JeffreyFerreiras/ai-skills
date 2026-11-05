use super::{
    artifacts::persist_artifact,
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    persistent_store::PersistentSession,
};
use crate::{
    compat::{PyString, Value},
    domain::{
        errors,
        evidence::{self, VerifiedArtifact},
        ids, python_operations, python_sort,
    },
};

pub(super) fn artifact_reference(artifact: &VerifiedArtifact) -> Value {
    Value::object([
        ("kind", artifact.kind.clone()),
        ("ref", artifact.reference.clone()),
        ("sha256", artifact.sha256.clone()),
    ])
}

pub fn branch_result_input(
    ledger: &dyn GraphLedgerReadPort,
    branch: &Value,
    result: &Value,
) -> EngineResult<Value> {
    let bytes = ids::canonical_bytes(result)?;
    let id = errors::string_item(branch, "branch_id")?;
    let digest = errors::string_item(branch, "result_digest")?;
    let reference = python_operations::add(&"ledger:".into(), id)?;
    let reference = python_operations::add(&reference, &"#sha256=".into())?;
    let reference = python_operations::add(&reference, digest)?;
    let artifact = ledger.first(GraphRead::Artifact, std::slice::from_ref(&reference))?;
    let Some(artifact) = artifact else {
        return Err(EngineError::state("INPUT_ARTIFACT_INVALID"));
    };
    if !errors::string_item(&artifact, "sha256")?.py_eq(digest) {
        return Err(EngineError::state("INPUT_ARTIFACT_INVALID"));
    }
    Ok(Value::object([
        ("kind", errors::string_item(&artifact, "kind")?.clone()),
        ("ref", reference),
        ("sha256", digest.clone()),
        ("size_bytes", bytes.len().into()),
    ]))
}

pub fn persist_payload(
    session: &dyn PersistentSession,
    run: &Value,
    policy: &Value,
    identifier: &PyString,
    kind: &Value,
    value: &Value,
) -> EngineResult<VerifiedArtifact> {
    let artifact = evidence::canonical_ledger_artifact(identifier, kind, value)?;
    evidence::enforce_artifact_size(kind, &artifact.size_bytes, policy)?;
    let run_id = errors::string_item(run, "run_id")?
        .text()
        .expect("validated run ID");
    persist_artifact(session, &run_id, &artifact)?;
    Ok(artifact)
}

pub fn sorted_inputs(inputs: Vec<Value>) -> EngineResult<Value> {
    let mut keyed = Vec::with_capacity(inputs.len());
    for input in inputs {
        keyed.push((
            Value::Tuple(vec![
                errors::string_item(&input, "kind")?.clone(),
                errors::string_item(&input, "ref")?.clone(),
            ]),
            input,
        ));
    }
    let mut order = python_sort::identity_indices(keyed.len())?;
    python_sort::sort_indices(&mut order, |left, right| {
        python_operations::less(&keyed[left].0, &keyed[right].0)
    })?;
    Ok(Value::List(
        order
            .into_iter()
            .map(|index| keyed[index].1.clone())
            .collect(),
    ))
}
