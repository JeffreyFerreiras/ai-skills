use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    graph_response::json_result,
    join_validation::{canonical_collection_members, join_members},
};
use crate::{
    compat::{self, Value},
    domain::{
        consolidation::{
            compute_delivery_outcome, compute_design_outcome, delegated_issue_sources,
            delegated_terminal_precedence, delivery_outcome_without_delegated_echoes,
            source_finding_dispositions,
        },
        ids,
        python_dictionary::DictionaryValue,
        python_operations,
    },
};

pub(crate) fn collection_document(join_id: &Value, members: Value) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "collection".into()),
        ("join_id", join_id.clone()),
        ("members", members),
    ])
}

pub(super) fn delivery_nested_collections(
    ledger: &dyn GraphLedgerReadPort,
    members: &[Value],
    generation: &Value,
) -> EngineResult<Vec<Value>> {
    let mut nested = Vec::new();
    for member in members {
        nested.extend(ledger.all(
            GraphRead::DeliveryNestedCollections,
            &[member["branch_id"].clone(), generation.clone()],
        )?);
    }
    Ok(nested)
}

pub fn draft(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    join_id: &Value,
) -> EngineResult<Value> {
    let join = ledger
        .first(
            GraphRead::JoinForRun,
            &[join_id.clone(), run["run_id"].clone()],
        )?
        .filter(|join| {
            join["kind"].is_text("collection")
                && (join["stage"].is_text("design") || join["stage"].is_text("delivery"))
                && join["status"].is_text("sealed")
        })
        .ok_or_else(|| EngineError::state("SOURCE_COLLECTION_NOT_SEALED"))?;
    let members = join_members(ledger, &join["join_id"])?;
    let design = join["stage"].is_text("design");
    let nested = if design {
        vec![]
    } else {
        delivery_nested_collections(ledger, &members, &Value::Null)?
            .iter()
            .map(|row| compat::loads_value(&row["content_json"]))
            .collect::<Result<Vec<_>, _>>()?
    };
    let (mut outcome, _) = if design {
        compute_design_outcome(&members)?
    } else if nested.is_empty() {
        compute_delivery_outcome(&members)?
    } else {
        delivery_outcome_without_delegated_echoes(&members, &nested)?
    };
    let mut dispositions = source_finding_dispositions(&members)?.entries;
    dispositions.sort_by(|(a, _), (b, _)| a.as_string().cmp(&b.as_string()));
    let dispositions = Value::list(dispositions.into_iter().map(|(key, value)| {
        let DictionaryValue::Value(value) = value else {
            unreachable!("source findings have scalar dispositions");
        };
        Value::object([("finding_id", key), ("disposition", value)])
    }));
    let mut sources = members
        .iter()
        .map(|member| member["branch_id"].clone())
        .collect::<Vec<_>>();
    python_operations::sort_comparable_values(&mut sources)?;
    let mut manifest = Value::object([
        ("schema_version", 1_i64.into()),
        (
            "kind",
            if design {
                "design_consolidation"
            } else {
                "delivery_consolidation"
            }
            .into(),
        ),
        ("run_id", run["run_id"].clone()),
        ("join_id", join["join_id"].clone()),
        ("generation", join["generation"].clone()),
        ("source_branch_ids", Value::List(sources)),
        ("finding_dispositions", dispositions),
    ]);
    let unresolved = if nested.is_empty() {
        Value::object([])
    } else {
        delegated_issue_sources(&nested)?
            .string_keyed_value()
            .expect("validated issue keys are strings")
    };
    if !unresolved.truthy() {
        if !nested.is_empty() {
            let outcomes = ["ACCEPT", "REPAIR", "REDESIGN", "BLOCK"];
            let precedence = outcomes
                .iter()
                .position(|label| outcome.is_text(label))
                .expect("known delivery outcome");
            outcome = outcomes[precedence.max(delegated_terminal_precedence(&nested)?)].into();
        }
        manifest.set("outcome", outcome);
    }
    let frozen = canonical_collection_members(ledger, &members)?;
    let collection = collection_document(&join["join_id"], frozen);
    let digest = ids::sha256_bytes(&ids::canonical_bytes(&collection)?);
    let mut result = json_result(
        true,
        "CONSOLIDATION_DRAFT",
        run["run_id"].clone(),
        run["state_revision"].clone(),
    );
    result.set("collection_digest", digest.into());
    result.set("manifest", manifest);
    result.set("unresolved_issue_identities", unresolved);
    Ok(result)
}
