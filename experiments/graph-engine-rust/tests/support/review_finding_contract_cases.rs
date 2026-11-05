use super::{
    approved, changed, check_preliminary, contract, first_changed, json, list, preliminary, Value,
};
use graph_engine::{
    compat,
    domain::{
        consolidation::validate_consolidation_manifest, errors::ValidationResult, ids,
        review_contracts::validate_findings, review_findings::consolidate_findings,
    },
};
use std::{collections::BTreeSet, fs, path::Path};

fn text_set(items: &[Value]) -> BTreeSet<String> {
    items.iter().map(|item| item.text().unwrap()).collect()
}

fn finding_scope(
    rows: &[Value],
    evidence: &Value,
    acceptance: &Value,
    scope: &Value,
) -> ValidationResult<Vec<Value>> {
    validate_findings(
        &Value::List(rows.to_vec()),
        evidence,
        "findings",
        Some(acceptance),
        Some(scope),
        true,
    )
}

#[test]
fn test_finding_consolidation_deduplicates_and_preserves_conflicts() {
    let sources = json(
        r#"[
      {
        "branch_id":"g2-b","role":"security_reviewer","model":"gpt-5.6-sol","assignment_id":"security",
        "ordinal":1,"review_lens":"security",
        "findings":[{"finding_id":"SEC-001","acceptance_id":"AC-001","location":"src/cache.cs","defect_id":"RACE","summary":"race","fix_variant":"Use channel","evidence_ids":[]}]
      },
      {
        "branch_id":"g2-a","role":"code_reviewer","model":"gpt-5.6-sol","assignment_id":"correctness",
        "ordinal":1,"review_lens":"correctness",
        "findings":[{"finding_id":"REV-001","acceptance_id":"AC-001","location":"Src\\Cache.cs","defect_id":"RACE","summary":"race","fix_variant":"Use lock","evidence_ids":[]}]
      },
      {
        "branch_id":"g2-c","role":"code_reviewer","model":"gpt-5.6-sol","assignment_id":"correctness",
        "ordinal":2,"review_lens":"correctness",
        "findings":[{"finding_id":"REV-001","acceptance_id":"AC-001","location":"./src/cache.cs","defect_id":"RACE","summary":"race","fix_variant":" use   lock ","evidence_ids":[]}]
      }
    ]"#,
    );
    let groups = consolidate_findings(&sources).unwrap();
    let groups = groups.as_list().unwrap();
    assert_eq!(groups.len(), 1);
    let group = &groups[0];
    assert!(group["canonical_branch_id"].is_text("g2-a"));
    assert!(group["conflict"].truthy());
    let provenance = group["provenance"].as_list().unwrap();
    assert_eq!(provenance.len(), 3);
    let lenses: BTreeSet<_> = provenance
        .iter()
        .map(|item| item["review_lens"].text().unwrap())
        .collect();
    assert_eq!(
        lenses,
        BTreeSet::from(["correctness".to_owned(), "security".to_owned()])
    );
    assert_eq!(group["fix_variants"].as_list().unwrap().len(), 2);
}

#[test]
fn test_preliminary_registry_and_member_findings_fail_closed() {
    let approved = changed(
        &approved("repo:docs/evidence.json", &"a".repeat(64)),
        "size_bytes",
        10_i64.into(),
    );
    let base = preliminary();
    check_preliminary(&base, approved.clone(), false).unwrap();
    for reference in ["repo:docs/other.json", "ledger:private"] {
        let changed = first_changed(&base, "evidence", |evidence| {
            evidence.set(
                "ref",
                format!("{reference}#sha256={}", "b".repeat(64)).into(),
            );
            evidence.set("sha256", "b".repeat(64).into());
        });
        contract(check_preliminary(&changed, approved.clone(), false), None);
    }

    let finding = json(
        r#"{
      "finding_id":"REV-201","acceptance_id":"AC-001","location":"docs/Cache.cs",
      "defect_id":"RACE","summary":"race","fix_variant":"lock","evidence_ids":["E-1"]
    }"#,
    );
    let evidence = list(&["E-1"]);
    let acceptance = list(&["AC-001"]);
    let scope = list(&["repo:docs/"]);
    finding_scope(
        std::slice::from_ref(&finding),
        &evidence,
        &acceptance,
        &scope,
    )
    .unwrap();
    for (field, replacement) in [
        ("acceptance_id", "AC-999".into()),
        ("location", "src/Cache.cs".into()),
        ("evidence_ids", list(&["E-2"])),
        ("finding_id", "invalid".into()),
        ("scope", "out_of_scope".into()),
        ("disposition", "approve".into()),
    ] {
        let changed = changed(&finding, field, replacement);
        contract(
            finding_scope(&[changed], &evidence, &acceptance, &scope),
            None,
        );
    }
    contract(
        finding_scope(&[finding.clone(), finding], &evidence, &acceptance, &scope),
        Some("DUPLICATE_ID"),
    );
}

#[test]
fn test_preliminary_accepts_only_approved_runtime_evidence() {
    let digest = "a".repeat(64);
    let reference = "runtime:artifacts/profile-1/review.json";
    let approved = approved(reference, &digest);
    let preliminary = preliminary();
    let normalized = check_preliminary(&preliminary, approved.clone(), false).unwrap();
    assert!(normalized["evidence"].as_list().unwrap()[0]["ref"].py_eq(&approved["ref"]));
    for evidence in [
        changed(&approved, "kind", "implementation_handoff".into()),
        super::approved(reference, &"b".repeat(64)),
        super::approved("runtime:artifacts/../outside.json", &digest),
    ] {
        contract(check_preliminary(&preliminary, evidence, false), None);
    }
    let resolved = first_changed(&preliminary, "evidence", |evidence| {
        evidence.set(
            "ref",
            format!("runtime:artifacts/profile-1/other.json#sha256={digest}").into(),
        );
    });
    contract(
        check_preliminary(&resolved, approved, true),
        Some("DELEGATION_EVIDENCE_UNAPPROVED"),
    );
}

fn terminal_values(finding: &str, status: &str, outcome: &str) -> (Value, Value, Value) {
    let parent = Value::object([
        ("branch_id", "g2-parent".into()), ("status", "succeeded".into()), ("mandatory", true.into()),
        ("result_json", format!(r#"{{"decision":"BLOCK","findings":[{{"finding_id":"{finding}","disposition":"block"}}]}}"#).into()),
    ]);
    let collection = json(&format!(
        r#"{{
      "kind":"review_nested_collection","request_slot_id":"slot-1","parent_branch_id":"g2-parent",
      "members":[{{"status":"{status}","terminal":{{"result":{{}}}}}}],
      "issues":[{{"issue_key":"issue-1","provenance":[{{
        "request_slot_id":"slot-1","branch_id":"g2-parent","finding_id":"{finding}",
        "assignment_id":"primary-reviewer","ordinal":0
      }}]}}]
    }}"#
    ));
    let manifest = json(&format!(
        r#"{{
      "kind":"delivery_consolidation","run_id":"RUN-1","join_id":"join-1","generation":0,
      "source_branch_ids":["g2-parent"],
      "finding_dispositions":[{{"finding_id":"{finding}","disposition":"block"}}],
      "delegated_finding_dispositions":[{{
        "issue_key":"issue-1","source_identities":[["slot-1","g2-parent","{finding}"]],"disposition":"accept"
      }}],"outcome":"{outcome}"
    }}"#
    ));
    (parent, collection, manifest)
}

fn consolidation(manifest: &Value, parent: &Value, collection: &Value) -> ValidationResult<Value> {
    validate_consolidation_manifest(
        manifest,
        &"delivery".into(),
        &"RUN-1".into(),
        &"join-1".into(),
        &0_i64.into(),
        std::slice::from_ref(parent),
        std::slice::from_ref(collection),
    )
}

#[test]
fn test_terminal_failure_or_timeout_cannot_be_accepted_away() {
    for status in ["failed", "timed_out"] {
        let (parent, collection, manifest) = terminal_values("REV-901", status, "BLOCK");
        assert!(consolidation(&manifest, &parent, &collection)
            .unwrap()
            .is_text("BLOCK"));
        contract(
            consolidation(
                &changed(&manifest, "outcome", "ACCEPT".into()),
                &parent,
                &collection,
            ),
            Some("OUTCOME_PRECEDENCE_MISMATCH"),
        );
    }
}

#[test]
fn test_explicitly_skipped_delegated_member_keeps_accept_disposition() {
    let (parent, collection, manifest) = terminal_values("REV-902", "skipped", "ACCEPT");
    assert!(consolidation(&manifest, &parent, &collection)
        .unwrap()
        .is_text("ACCEPT"));
}

#[test]
fn test_csharp_fixture_is_executable_consolidation_evidence() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../skills/software-engineering-graph/tests/fixtures/reviewer-delegation-csharp");
    let expected =
        compat::loads(&fs::read(fixture.join("expected-findings.json")).unwrap()).unwrap();
    for path in ["Cache.cs", "Worker.cs", "WorkerTests.cs"] {
        assert!(!fs::read_to_string(fixture.join(path))
            .unwrap()
            .trim()
            .is_empty());
    }
    let scope = list(&["repo:tests/fixtures/reviewer-delegation-csharp/"]);
    let supported = expected["supported_findings"].as_list().unwrap();
    let mut sources = Vec::new();
    for expected in supported {
        let acceptance = expected["acceptance_id"].text().unwrap();
        let evidence = list(&[&format!("E-{acceptance}")]);
        let variants = variants(expected);
        let lenses = expected["lenses"].as_list().unwrap();
        let duplicates = expected["duplicate_source_ids"].as_list().unwrap();
        for (index, finding_id) in duplicates.iter().enumerate() {
            let lens = &lenses[index.min(lenses.len() - 1)];
            let finding = Value::object([
                ("finding_id", finding_id.clone()),
                ("acceptance_id", expected["acceptance_id"].clone()),
                ("location", expected["location"].clone()),
                ("defect_id", expected["defect_id"].clone()),
                ("summary", "fixture-supported defect".into()),
                (
                    "fix_variant",
                    variants[index.min(variants.len() - 1)].clone(),
                ),
                ("evidence_ids", evidence.clone()),
            ]);
            let normalized =
                finding_scope(&[finding], &evidence, &list(&[&acceptance]), &scope).unwrap();
            let branch = format!("g2-{}", char::from(b'a' + sources.len() as u8));
            sources.push(Value::object([
                ("branch_id", branch.into()),
                (
                    "role",
                    if lens.is_text("security") {
                        "security_reviewer"
                    } else {
                        "code_reviewer"
                    }
                    .into(),
                ),
                ("model", "gpt-5.6-sol".into()),
                ("assignment_id", lens.clone()),
                ("ordinal", Value::from(index + 1)),
                ("review_lens", lens.clone()),
                ("findings", Value::List(normalized)),
            ]));
        }
    }
    let sources = Value::List(sources);
    let groups = consolidate_findings(&sources).unwrap();
    let rows = groups.as_list().unwrap();
    assert_eq!(rows.len(), supported.len());
    for expected in supported {
        let group = rows
            .iter()
            .find(|group| group["acceptance_id"].py_eq(&expected["acceptance_id"]))
            .unwrap();
        let provenance = group["provenance"].as_list().unwrap();
        for (field, expected_field) in [
            ("finding_id", "duplicate_source_ids"),
            ("review_lens", "lenses"),
        ] {
            let actual: BTreeSet<_> = provenance
                .iter()
                .map(|row| row[field].text().unwrap())
                .collect();
            assert_eq!(
                actual,
                text_set(expected[expected_field].as_list().unwrap())
            );
        }
        assert_eq!(
            group["conflict"].truthy(),
            expected["incompatible_fix_variants"].truthy()
        );
        let actual: BTreeSet<_> = group["fix_variants"]
            .as_list()
            .unwrap()
            .iter()
            .map(|variant| variant["text"].text().unwrap())
            .collect();
        assert_eq!(actual, text_set(&variants(expected)));
    }
    let baseline = ids::canonical_bytes(&groups).unwrap();
    let supported = &supported[1];
    let acceptance = supported["acceptance_id"].text().unwrap();
    let evidence = list(&[&format!("E-{acceptance}")]);
    let base = Value::object([
        ("finding_id", "REV-999".into()),
        ("acceptance_id", supported["acceptance_id"].clone()),
        ("location", supported["location"].clone()),
        ("defect_id", supported["defect_id"].clone()),
        ("summary", "unsupported amplification".into()),
        ("fix_variant", "none".into()),
        ("evidence_ids", evidence.clone()),
    ]);
    for (field, value) in [
        ("acceptance_id", "AC-UNSUPPORTED".into()),
        ("evidence_ids", list(&["E-UNSUPPORTED"])),
        ("location", "src/Outside.cs".into()),
    ] {
        contract(
            finding_scope(
                &[changed(&base, field, value)],
                &evidence,
                &list(&[&acceptance]),
                &scope,
            ),
            None,
        );
        assert_eq!(
            ids::canonical_bytes(&consolidate_findings(&sources).unwrap()).unwrap(),
            baseline
        );
    }
}

fn variants(expected: &Value) -> Vec<Value> {
    let variants = expected["incompatible_fix_variants"].as_list().unwrap();
    if variants.is_empty() {
        vec!["add the missing assertion".into()]
    } else {
        variants.to_vec()
    }
}
