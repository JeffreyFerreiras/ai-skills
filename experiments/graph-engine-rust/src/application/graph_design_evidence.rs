use super::{
    error::{EngineError, EngineResult},
    graph_consolidation::collection_document,
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    graph_payload::branch_result_input,
};
use crate::{
    compat::{self, PyString, Value},
    domain::{
        evidence::{canonical_ledger_artifact, VerifiedArtifact, DESIGN_EVIDENCE_PAGE_BYTES},
        ids,
        result_contract::is_actionable_finding,
    },
};
use std::collections::BTreeSet;

fn delegated_evidence(
    ledger: &dyn GraphLedgerReadPort,
    member: &Value,
    generation: &Value,
    dispositions: &[Value],
) -> EngineResult<(BTreeSet<PyString>, Vec<Value>)> {
    let mut echoes = BTreeSet::new();
    let mut projected = Vec::new();
    for row in ledger.all(GraphRead::DispatchCollections, &[member["branch_id"].clone(), generation.clone()])? {
        let collection = compat::loads_value(&row["content_json"])?;
        let artifact = ledger.first(GraphRead::Artifact, std::slice::from_ref(&row["preliminary_ref"]))?
            .ok_or_else(|| EngineError::state("DESIGN_EVIDENCE_SOURCE_INVALID"))?;
        let preliminary = compat::loads_value(&artifact["content_json"])?;
        for issue in collection["issues"].as_list().unwrap_or(&[]) {
            let provenance = issue["provenance"].as_list().unwrap_or(&[]);
            echoes.extend(provenance.iter().filter_map(|source| source["finding_id"].as_string().cloned()));
            let Some(disposition) = dispositions.iter().find(|item| {
                item["issue_key"].py_eq(&issue["issue_key"]) && !item["disposition"].is_text("accept")
            }) else {
                continue;
            };
            let mut cited = BTreeSet::new();
            let mut summaries = Vec::new();
            for source in provenance {
                let findings = if source["branch_id"].py_eq(&collection["parent_branch_id"]) {
                    &preliminary["findings"]
                } else {
                    &collection["members"].as_list().unwrap_or(&[]).iter()
                        .find(|child| source["branch_id"].py_eq(&child["branch_id"]))
                        .ok_or_else(|| EngineError::state("DESIGN_EVIDENCE_SOURCE_INVALID"))?["findings"]
                };
                let finding = findings.as_list().unwrap_or(&[]).iter()
                    .find(|finding| source["finding_id"].py_eq(&finding["finding_id"]))
                    .ok_or_else(|| EngineError::state("DESIGN_EVIDENCE_SOURCE_INVALID"))?;
                cited.extend(finding["evidence_ids"].as_list().unwrap_or(&[])
                    .iter().filter_map(|id| id.as_string().cloned()));
                if !summaries.iter().any(|value: &Value| value.py_eq(&finding["summary"])) {
                    summaries.push(finding["summary"].clone());
                }
            }
            let evidence = preliminary["evidence"].as_list().unwrap_or(&[]).iter()
                .filter(|item| item["evidence_id"].as_string().is_some_and(|id| cited.contains(id)))
                .map(|item| Value::object(["kind", "ref", "sha256"].map(|key| (key, item[key].clone()))));
            let variants = issue["fix_variants"].as_list().unwrap_or(&[])
                .iter().map(|variant| variant["text"].clone());
            projected.push(Value::object([
                ("issue_key", issue["issue_key"].clone()),
                ("acceptance_id", issue["acceptance_id"].clone()),
                ("location", issue["location"].clone()),
                ("defect_id", issue["defect_id"].clone()),
                ("summaries", Value::List(summaries)),
                ("fix_variants", Value::list(variants)),
                ("evidence", Value::list(evidence)),
                ("disposition", disposition["disposition"].clone()),
            ]));
        }
    }
    Ok((echoes, projected))
}

fn artifact_pages(mut document: Value, lead_id: &Value) -> EngineResult<Vec<(VerifiedArtifact, Value)>> {
    let fields = ["evidence_refs", "reports", "finding_dispositions", "delegated_issues"];
    let mut arrays = Vec::new();
    for key in fields {
        if let Some(Value::List(items)) = document.get(key) {
            arrays.push((key, items.clone()));
            document.set(key, Value::list([]));
        }
    }
    let lead_id = lead_id.text().expect("stable branch identifier");
    let mut pages = Vec::new();
    let mut page_number = 1_i64;
    let page = |page_number: i64| -> EngineResult<(Value, usize)> {
        let mut page = document.clone();
        page.set("page", page_number.into());
        let size = ids::canonical_bytes(&page)?.len();
        Ok((page, size))
    };
    let seal = |page_number: i64, page: Value| -> EngineResult<(VerifiedArtifact, Value)> {
        let identifier = format!("design-evidence-{lead_id}-{page_number:06}");
        let artifact = canonical_ledger_artifact(&identifier.into(), &"design_evidence".into(), &page)?;
        Ok((artifact, page))
    };
    let (mut current, mut size) = page(page_number)?;
    for (key, items) in arrays {
        let mut values = Vec::new();
        for item in items {
            let item_size = ids::canonical_bytes(&item)?.len();
            let mut growth = item_size + usize::from(!values.is_empty());
            if size + growth > DESIGN_EVIDENCE_PAGE_BYTES {
                current.set(key, Value::List(std::mem::take(&mut values)));
                pages.push(seal(page_number, current)?);
                page_number += 1;
                (current, size) = page(page_number)?;
                growth = item_size;
            }
            if size + growth > DESIGN_EVIDENCE_PAGE_BYTES {
                return Err(EngineError::state("DESIGN_EVIDENCE_ITEM_TOO_LARGE"));
            }
            values.push(item);
            size += growth;
        }
        current.set(key, Value::List(values));
    }
    pages.push(seal(page_number, current)?);
    Ok(pages)
}

pub(super) fn design_evidence_artifacts(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    lead_id: &Value,
    source: &Value,
) -> EngineResult<Vec<(VerifiedArtifact, Value)>> {
    let result = compat::loads_value(&source["result_json"])?;
    let source_input = branch_result_input(ledger, source, &result)?;
    let mut document = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "design_evidence".into()),
        ("run_id", run["run_id"].clone()),
        ("branch_id", lead_id.clone()),
        ("source_ref", source_input["ref"].clone()),
    ]);
    if source["node_key"].is_text("impact_mapper") {
        for key in ["route_label", "impact_tags", "evidence_refs"] {
            document.set(key, result[key].clone());
        }
    } else {
        let collection = ledger.first(GraphRead::JoinForRun, &[result["join_id"].clone(), run["run_id"].clone()])?
            .filter(|join| join["kind"].is_text("collection") && join["status"].is_text("sealed"))
            .ok_or_else(|| EngineError::state("SOURCE_COLLECTION_NOT_SEALED"))?;
        let frozen = compat::loads_value(&collection["result_json"])?;
        let members = frozen.as_list().expect("validated sealed collection");
        let mut reports = Vec::new();
        let mut finding_ids = Vec::new();
        let mut issues = Vec::new();
        let delegated = result["delegated_finding_dispositions"].as_list().unwrap_or(&[]);
        for member in members.iter().filter(|row| row["status"].is_text("succeeded")) {
            let echoes = if delegated.is_empty() {
                BTreeSet::new()
            } else {
                let (echoes, details) = delegated_evidence(ledger, member, &collection["generation"], delegated)?;
                issues.extend(details);
                echoes
            };
            let report = &member["result"];
            let mut findings = Vec::new();
            for item in report["findings"].as_list().unwrap_or(&[]) {
                if is_actionable_finding(item)?
                    && !["approve", "accept"].iter().any(|value| item["disposition"].is_text(value))
                    && !item["finding_id"].as_string().is_some_and(|id| echoes.contains(id))
                {
                    finding_ids.push(item["finding_id"].clone());
                    findings.push(item.clone());
                }
            }
            if !findings.is_empty() {
                reports.push(Value::object([
                    ("branch_id", member["branch_id"].clone()),
                    ("findings", Value::List(findings)),
                    ("evidence", report.get("evidence").cloned().unwrap_or_else(|| Value::list([]))),
                    ("artifact_ref", report["artifact_ref"].clone()),
                ]));
            }
        }
        let dispositions = result["finding_dispositions"].as_list().unwrap_or(&[])
            .iter().filter(|item| finding_ids.iter().any(|id| item["finding_id"].py_eq(id)));
        let collection_id = collection["join_id"].as_string().expect("validated join identifier");
        let collection_artifact = canonical_ledger_artifact(
            collection_id, &"collection".into(), &collection_document(&collection["join_id"], frozen.clone()),
        )?;
        document.set("collection_ref", collection_artifact.reference);
        document.set("outcome", result["outcome"].clone());
        document.set("reports", Value::List(reports));
        document.set("finding_dispositions", Value::list(dispositions.cloned()));
        if !issues.is_empty() {
            document.set("delegated_issues", Value::List(issues));
        }
    }
    artifact_pages(document, lead_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_evidence_pages_keep_order_and_exact_byte_bound() {
        let reports = ["a", "b", "c"].map(|part| {
            Value::object([("finding", part.repeat(DESIGN_EVIDENCE_PAGE_BYTES / 2).into())])
        });
        let pages = artifact_pages(Value::object([
            ("kind", "design_evidence".into()),
            ("source_ref", "source".into()),
            ("reports", Value::list(reports.iter().cloned())),
        ]), &"lead".into()).unwrap();
        assert_eq!(pages.len(), 3);
        for (index, ((artifact, page), report)) in pages.iter().zip(reports).enumerate() {
            assert_eq!(page["page"].as_i64(), Some(index as i64 + 1));
            assert!(page["source_ref"].is_text("source"));
            assert!(page["reports"].py_eq(&Value::list([report])));
            assert!(artifact.size_bytes.as_i64().unwrap() <= DESIGN_EVIDENCE_PAGE_BYTES as i64);
            assert!(artifact.sha256.is_text(&ids::sha256_bytes(&ids::canonical_bytes(page).unwrap())));
        }
    }
}
