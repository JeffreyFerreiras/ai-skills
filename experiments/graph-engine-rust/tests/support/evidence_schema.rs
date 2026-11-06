use graph_engine::compat::{self, Value};

fn finding(id: &str, disposition: &str) -> Value {
    Value::object([
        ("finding_id", id.into()),
        ("disposition", disposition.into()),
    ])
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SchemaMismatch {
    Constraint,
    UnknownField,
}

// The original fixtures use this schema subset without a JSON Schema dependency.
pub(super) fn validate_schema(
    value: &Value,
    schema: &Value,
    root: &Value,
) -> Result<(), SchemaMismatch> {
    use SchemaMismatch::{Constraint, UnknownField};
    if let Some(reference) = schema["$ref"].text() {
        let Some(path) = reference.strip_prefix("#/") else {
            return Err(Constraint);
        };
        let target = path.split('/').fold(root, |schema, part| &schema[part]);
        return validate_schema(value, target, root);
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(candidates) = schema[keyword].as_list() {
            let mut matched = 0;
            let mut failure = Constraint;
            for candidate in candidates {
                match validate_schema(value, candidate, root) {
                    Ok(()) => matched += 1,
                    Err(UnknownField) => failure = UnknownField,
                    Err(Constraint) => {}
                }
            }
            if matched == 0 {
                return Err(failure);
            }
            if keyword == "oneOf" && matched != 1 {
                return Err(Constraint);
            }
            break;
        }
    }
    for candidate in schema["allOf"].as_list().unwrap_or(&[]) {
        validate_schema(value, candidate, root)?;
    }
    if let Some(condition) = schema.get("if") {
        let branch = if matches_schema(value, condition, root) {
            "then"
        } else {
            "else"
        };
        if let Some(branch) = schema.get(branch) {
            validate_schema(value, branch, root)?;
        }
    }
    if schema
        .get("not")
        .is_some_and(|denied| matches_schema(value, denied, root))
    {
        return Err(Constraint);
    }
    if let Some(expected) = schema.get("const") {
        if !value.py_eq(expected) {
            return Err(Constraint);
        }
    }
    if let Some(values) = schema["enum"].as_list() {
        if !values.iter().any(|item| value.py_eq(item)) {
            return Err(Constraint);
        }
    }
    let type_matches = |kind: &Value| match kind.text().as_deref() {
        Some("object") => value.as_object().is_some(),
        Some("array") => value.as_list().is_some(),
        Some("string") => value.as_string().is_some(),
        Some("integer") => matches!(value, Value::Integer(_)),
        Some("boolean") => value.python_type() == "bool",
        Some("null") => value.is_null(),
        _ => false,
    };
    if let Some(kind) = schema.get("type") {
        if !kind.as_list().map_or_else(
            || type_matches(kind),
            |types| types.iter().any(type_matches),
        ) {
            return Err(Constraint);
        }
    }
    if let Some(fields) = value.as_object() {
        if let Some(required) = schema["required"].as_list() {
            if required
                .iter()
                .any(|key| value.get(&key.text().unwrap()).is_none())
            {
                return Err(Constraint);
            }
        }
        if schema["minProperties"]
            .as_i64()
            .is_some_and(|minimum| fields.len() < minimum as usize)
        {
            return Err(Constraint);
        }
        for (key, value) in fields {
            let name = Value::String(key.clone());
            if let Some(names) = schema.get("propertyNames") {
                validate_schema(&name, names, root)?;
            }
            if let Some(property) = schema["properties"].get(&name.text().unwrap()) {
                validate_schema(value, property, root)?;
            } else if let Some(additional) = schema.get("additionalProperties") {
                if additional.py_eq(&false.into()) {
                    return Err(UnknownField);
                }
                if additional.as_object().is_some() {
                    validate_schema(value, additional, root)?;
                }
            }
        }
    }
    let bounded = |count: usize, min: &str, max: &str| {
        schema[min].as_i64().is_none_or(|min| count >= min as usize)
            && schema[max].as_i64().is_none_or(|max| count <= max as usize)
    };
    if let Some(items) = value.as_list() {
        if !bounded(items.len(), "minItems", "maxItems") {
            return Err(Constraint);
        }
        if schema["uniqueItems"].truthy()
            && items
                .iter()
                .enumerate()
                .any(|(index, item)| items[..index].iter().any(|other| item.py_eq(other)))
        {
            return Err(Constraint);
        }
        if let Some(schema) = schema.get("items") {
            for item in items {
                validate_schema(item, schema, root)?;
            }
        }
        if schema.get("contains").is_some_and(|candidate| {
            !items
                .iter()
                .any(|item| matches_schema(item, candidate, root))
        }) {
            return Err(Constraint);
        }
    }
    if let Some(value) = value.as_string() {
        if !bounded(value.len(), "minLength", "maxLength") {
            return Err(Constraint);
        }
        if let Some(pattern) = schema["pattern"].text() {
            let lowercase_hex = |points: &[u32], length| {
                points.len() == length
                    && points
                        .iter()
                        .all(|point| matches!(point, 48..=57 | 97..=102))
            };
            let alphanumeric = |point| matches!(point, 48..=57 | 65..=90 | 97..=122);
            let opaque = |points: &[u32]| {
                points.split_first().is_some_and(|(&first, tail)| {
                    alphanumeric(first)
                        && tail.len() < 64
                        && tail
                            .iter()
                            .all(|&point| alphanumeric(point) || matches!(point, 45 | 46 | 95))
                })
            };
            let matches = match pattern.as_str() {
                "^[0-9a-f]{64}$" => lowercase_hex(&value.0, 64),
                "^g2-[0-9a-f]{24}$" => value
                    .0
                    .strip_prefix(&[103, 50, 45])
                    .is_some_and(|tail| lowercase_hex(tail, 24)),
                "^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$" => opaque(&value.0),
                "^authority:[A-Za-z0-9][A-Za-z0-9._-]{0,63}$" => value
                    .0
                    .strip_prefix(&[97, 117, 116, 104, 111, 114, 105, 116, 121, 58])
                    .is_some_and(opaque),
                r"^[0-9]+\.[0-9]+\.[0-9]+$" => value.utf8().is_ok_and(|version| {
                    let mut parts = version.split('.');
                    parts
                        .by_ref()
                        .take(3)
                        .filter(|part| {
                            !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
                        })
                        .count()
                        == 3
                        && parts.next().is_none()
                }),
                r"^(repo:|runtime:artifacts/)[A-Za-z0-9._/-]+\.json$" => {
                    value.utf8().is_ok_and(|reference| {
                        reference
                            .strip_prefix("repo:")
                            .or_else(|| reference.strip_prefix("runtime:artifacts/"))
                            .and_then(|path| path.strip_suffix(".json"))
                            .is_some_and(|path| {
                                !path.is_empty()
                                    && path.bytes().all(|byte| {
                                        byte.is_ascii_alphanumeric() || b"._/-".contains(&byte)
                                    })
                            })
                    })
                }
                _ => panic!("unsupported frozen schema pattern {pattern}"),
            };
            if !matches {
                return Err(Constraint);
            }
        }
    }
    if let Some(value) = value.as_i64() {
        if schema["minimum"].as_i64().is_some_and(|min| value < min)
            || schema["maximum"].as_i64().is_some_and(|max| value > max)
        {
            return Err(Constraint);
        }
    }
    Ok(())
}

fn matches_schema(value: &Value, schema: &Value, root: &Value) -> bool {
    validate_schema(value, schema, root).is_ok()
}

#[allow(dead_code)]
pub(super) fn assert_branch_envelope_schema(envelope: &Value) {
    let schema = compat::loads(include_bytes!(
        "../../../../skills/software-engineering-graph/references/branch-envelope.schema.json"
    ))
    .unwrap();
    assert!(matches_schema(envelope, &schema, &schema));
}

fn changed(value: &Value, key: &str, replacement: Value) -> Value {
    let mut result = value.clone();
    result.set(key, replacement);
    result
}

fn without(value: &Value, key: &str) -> Value {
    Value::Object(
        value
            .as_object()
            .unwrap()
            .iter()
            .filter(|(name, _)| !name.equals(key))
            .cloned()
            .collect(),
    )
}

pub(super) fn assert_repair_schema(packet: &Value) {
    let schema = compat::loads(include_bytes!(
        "../../../../skills/software-engineering-graph/references/evidence-lifecycle.schema.json"
    ))
    .unwrap();
    let valid = |value: &Value| matches_schema(value, &schema, &schema);
    assert!(valid(packet));
    assert!(!valid(&changed(packet, "unexpected", true.into())));
    if packet.get("origin_kind").is_none() {
        for field in [
            "origin_kind",
            "origin_collection_id",
            "origin_consolidation_ref",
            "origin_reports",
        ] {
            assert!(!valid(&changed(packet, field, "partial".into())));
        }
        return;
    }
    for field in [
        "origin_kind",
        "origin_collection_id",
        "origin_consolidation_ref",
        "origin_reports",
    ] {
        assert!(!valid(&without(packet, field)));
    }
    for (field, value) in [
        ("origin_kind", "invented".into()),
        ("origin_collection_id", 1_i64.into()),
        (
            "origin_consolidation_ref",
            changed(&packet["origin_consolidation_ref"], "sha256", "bad".into()),
        ),
        ("origin_reports", Value::List(vec![])),
    ] {
        assert!(!valid(&changed(packet, field, value)));
    }
    let report = &packet["origin_reports"].as_list().unwrap()[0];
    let with_report = |report| changed(packet, "origin_reports", Value::list([report]));
    let scoped = Value::list([
        changed(&finding("FIX-001", "repair"), "scope", "in_scope".into()),
        changed(
            &finding("FIX-002", "approve"),
            "scope",
            "out_of_scope".into(),
        ),
    ]);
    assert!(valid(&with_report(changed(report, "findings", scoped))));
    for invalid in [
        changed(
            report,
            "findings",
            Value::list([changed(
                &finding("FIX-001", "repair"),
                "scope",
                "invented".into(),
            )]),
        ),
        without(report, "attempt_id"),
        changed(report, "unexpected", true.into()),
        changed(report, "attempt_id", Value::Null),
        changed(
            report,
            "result_ref",
            changed(&report["result_ref"], "size_bytes", (-1_i64).into()),
        ),
        changed(
            report,
            "artifact_ref",
            changed(&report["artifact_ref"], "unexpected", true.into()),
        ),
        changed(
            report,
            "findings",
            Value::list([finding("FIX-001", "invented")]),
        ),
        changed(
            report,
            "evidence",
            Value::list([Value::object([
                ("kind", "finding".into()),
                ("ref", "repo:missing".into()),
                ("sha256", "bad".into()),
            ])]),
        ),
    ] {
        assert!(!valid(&with_report(invalid)));
    }
}
