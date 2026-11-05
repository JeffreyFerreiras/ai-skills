use super::{
    contract_values::get,
    errors::{attribute_error, iterable_values, string_item, ValidationFailure, ValidationResult},
    python_operations::{self, sort_by_less},
    review_contracts::normalize_location,
};
use crate::compat::{unicode, PyString, Value};
use std::collections::BTreeMap;

struct Variant {
    normalized: PyString,
    text: Value,
    sources: Vec<Value>,
}
struct Group {
    issue_key: PyString,
    acceptance: PyString,
    location: PyString,
    defect: PyString,
    canonical: Value,
    provenance: Vec<Value>,
    variants: BTreeMap<PyString, Variant>,
}

fn joined_issue(parts: [&Value; 3]) -> ValidationResult<PyString> {
    let mut result = PyString::default();
    for (index, part) in parts.into_iter().enumerate() {
        let text = part.as_string().ok_or_else(|| {
            ValidationFailure::python(
                "TypeError",
                format!(
                    "sequence item {index}: expected str instance, {} found",
                    part.python_type()
                ),
            )
        })?;
        if index != 0 {
            result.0.push(58);
        }
        result.0.extend_from_slice(&text.0);
    }
    Ok(result)
}

fn normalized_fix(value: &Value) -> ValidationResult<PyString> {
    let text = value
        .as_string()
        .ok_or_else(|| attribute_error(value, "casefold"))?;
    let folded = unicode::casefold(text);
    let mut output = Vec::new();
    for word in folded
        .0
        .split(|point| unicode::whitespace(*point))
        .filter(|word| !word.is_empty())
    {
        if !output.is_empty() {
            output.push(32);
        }
        output.extend_from_slice(word);
    }
    Ok(PyString(output))
}

fn sort_provenance(values: &mut [Value]) -> ValidationResult<()> {
    let mut decorated = Vec::new();
    for (index, value) in values.iter().enumerate() {
        decorated.push((
            Value::List(vec![
                string_item(value, "branch_id")?.clone(),
                string_item(value, "finding_id")?.clone(),
            ]),
            index,
        ));
    }
    sort_by_less(&mut decorated, |left, right| {
        python_operations::less(&left.0, &right.0)
    })?;
    let ordered = decorated
        .into_iter()
        .map(|(_, index)| values[index].clone())
        .collect::<Vec<_>>();
    values.clone_from_slice(&ordered);
    Ok(())
}

pub fn consolidate_findings(sources: &Value) -> ValidationResult<Value> {
    let mut sources = iterable_values(sources)?
        .into_iter()
        .map(|source| Ok((string_item(&source, "branch_id")?.clone(), source)))
        .collect::<ValidationResult<Vec<_>>>()?;
    sort_by_less(&mut sources, |left, right| {
        python_operations::less(&left.0, &right.0)
    })?;
    let mut groups: BTreeMap<(PyString, PyString, PyString), Group> = BTreeMap::new();
    for (_, source) in sources {
        let mut provenance = Value::Object(vec![]);
        for key in [
            "branch_id",
            "role",
            "model",
            "assignment_id",
            "ordinal",
            "review_lens",
        ] {
            provenance.set(key, string_item(&source, key)?.clone());
        }
        if let Some(slot) = source.get("request_slot_id") {
            provenance.set("request_slot_id", slot.clone());
        }
        let empty = Value::List(vec![]);
        get(&source, "findings")?;
        for finding in iterable_values(source.get("findings").unwrap_or(&empty))? {
            let acceptance = string_item(&finding, "acceptance_id")?;
            let location = Value::String(normalize_location(string_item(&finding, "location")?)?);
            let defect = string_item(&finding, "defect_id")?;
            let issue_key = joined_issue([acceptance, &location, defect])?;
            let key = (
                acceptance.as_string().unwrap().clone(),
                location.as_string().unwrap().clone(),
                defect.as_string().unwrap().clone(),
            );
            let branch = string_item(&source, "branch_id")?;
            let group = groups.entry(key.clone()).or_insert_with(|| Group {
                issue_key,
                acceptance: key.0,
                location: key.1,
                defect: key.2,
                canonical: branch.clone(),
                provenance: vec![],
                variants: BTreeMap::new(),
            });
            if python_operations::less(branch, &group.canonical)? {
                group.canonical = branch.clone();
            }
            let mut origin = provenance.clone();
            origin.set("finding_id", string_item(&finding, "finding_id")?.clone());
            group.provenance.push(origin.clone());
            let fix = string_item(&finding, "fix_variant")?;
            let normalized = normalized_fix(fix)?;
            let variant = group
                .variants
                .entry(normalized.clone())
                .or_insert_with(|| Variant {
                    normalized,
                    text: fix.clone(),
                    sources: vec![],
                });
            variant.sources.push(origin);
        }
    }
    let mut result = Vec::new();
    for (_, mut group) in groups {
        sort_provenance(&mut group.provenance)?;
        let mut variants = Vec::new();
        for (_, mut variant) in group.variants {
            sort_provenance(&mut variant.sources)?;
            variants.push(Value::object([
                ("normalized", Value::String(variant.normalized)),
                ("text", variant.text),
                ("sources", Value::List(variant.sources)),
            ]));
        }
        let conflict = variants.len() > 1;
        result.push(Value::object([
            ("issue_key", Value::String(group.issue_key)),
            ("acceptance_id", Value::String(group.acceptance)),
            ("location", Value::String(group.location)),
            ("defect_id", Value::String(group.defect)),
            ("canonical_branch_id", group.canonical),
            ("provenance", Value::List(group.provenance)),
            ("fix_variants", Value::List(variants)),
            ("conflict", conflict.into()),
        ]));
    }
    Ok(Value::List(result))
}
