use super::contract_values::{
    fail, in_strings, list, object, sort_by_fields, strings, unique_strings,
};
use super::contracts::{
    bounded_string, digest, lexical_relative, opaque, require_keys, validate_ref,
};
use super::errors::ValidationResult;
use crate::compat::{PyString, Value};
use num_bigint::BigInt;
use std::collections::BTreeSet;

pub fn validate_fanout_assessment(
    value: &Value,
    run_id: &Value,
    fanout_id: &Value,
    expected_members: &[Value],
) -> ValidationResult<Value> {
    object(value, "assessment_manifest", "FANOUT_ASSESSMENT_INVALID")?;
    const KEYS: &[&str] = &[
        "schema_version",
        "kind",
        "run_id",
        "fanout_id",
        "members",
        "dependencies",
        "evidence",
    ];
    require_keys(value, KEYS, KEYS, "assessment_manifest")?;
    if !value["schema_version"].py_eq(&1i64.into())
        || !value["kind"].is_text("fanout_assessment")
        || !value["run_id"].py_eq(run_id)
        || !value["fanout_id"].py_eq(fanout_id)
    {
        return Err(fail("assessment_manifest", "FANOUT_ASSESSMENT_INVALID"));
    }
    let mut members = Vec::new();
    for (index, member) in list(&value["members"], "members", "FANOUT_MEMBER_INVALID")?
        .iter()
        .enumerate()
    {
        let field = format!("members[{index}]");
        object(member, &field, "FANOUT_MEMBER_INVALID")?;
        require_keys(
            member,
            &["branch_id", "resources"],
            &["branch_id", "resources"],
            &field,
        )?;
        let branch = opaque(&member["branch_id"], &format!("{field}.branch_id"))?;
        let resources = &member["resources"];
        object(
            resources,
            &format!("{field}.resources"),
            "FANOUT_RESOURCE_INVALID",
        )?;
        const RESOURCE_KEYS: &[&str] = &[
            "writable_paths",
            "mutable_state_refs",
            "exclusive_device_refs",
            "services",
        ];
        require_keys(
            resources,
            RESOURCE_KEYS,
            RESOURCE_KEYS,
            &format!("{field}.resources"),
        )?;
        let mut paths = Vec::new();
        let mut path_keys = BTreeSet::new();
        for (path_index, item) in list(
            &resources["writable_paths"],
            &format!("{field}.writable_paths"),
            "FANOUT_RESOURCE_INVALID",
        )?
        .iter()
        .enumerate()
        {
            let path_field = format!("{field}.writable_paths[{path_index}]");
            object(item, &path_field, "FANOUT_RESOURCE_INVALID")?;
            require_keys(item, &["path", "scope"], &["path", "scope"], &path_field)?;
            if !in_strings(&item["scope"], &["exact", "subtree"])? {
                return Err(fail(
                    &format!("{path_field}.scope"),
                    "FANOUT_RESOURCE_INVALID",
                ));
            }
            let mut path = lexical_relative(&item["path"], &format!("{path_field}.path"))?;
            while path.0.last() == Some(&47) {
                path.0.pop();
            }
            path_keys.insert((path.clone(), item["scope"].as_string().unwrap().clone()));
            paths.push(Value::object([
                ("path", Value::String(path)),
                ("scope", item["scope"].clone()),
            ]));
        }
        if path_keys.len() != paths.len() {
            return Err(fail(
                &format!("{field}.writable_paths"),
                "FANOUT_RESOURCE_INVALID",
            ));
        }
        let mutable = unique_strings(
            &resources["mutable_state_refs"],
            &format!("{field}.mutable_state_refs"),
            None,
        )?;
        let devices = unique_strings(
            &resources["exclusive_device_refs"],
            &format!("{field}.exclusive_device_refs"),
            None,
        )?;
        let mut services = Vec::new();
        let mut service_refs = BTreeSet::new();
        for (service_index, service) in list(
            &resources["services"],
            &format!("{field}.services"),
            "FANOUT_RESOURCE_INVALID",
        )?
        .iter()
        .enumerate()
        {
            let service_field = format!("{field}.services[{service_index}]");
            object(service, &service_field, "FANOUT_RESOURCE_INVALID")?;
            require_keys(
                service,
                &["ref", "units", "capacity"],
                &["ref", "units", "capacity"],
                &service_field,
            )?;
            let reference =
                bounded_string(&service["ref"], &format!("{service_field}.ref"), 1024)?.clone();
            let valid = service["units"]
                .strict_integer()
                .zip(service["capacity"].strict_integer())
                .is_some_and(|(units, capacity)| {
                    units > &BigInt::from(0) && capacity > &BigInt::from(0) && units <= capacity
                });
            if !valid {
                return Err(fail(&service_field, "FANOUT_CAPACITY_INVALID"));
            }
            service_refs.insert(reference.clone());
            services.push(Value::object([
                ("ref", Value::String(reference)),
                ("units", service["units"].clone()),
                ("capacity", service["capacity"].clone()),
            ]));
        }
        if service_refs.len() != services.len() {
            return Err(fail(
                &format!("{field}.services"),
                "FANOUT_RESOURCE_INVALID",
            ));
        }
        sort_by_fields(&mut paths, &["path", "scope"]);
        sort_by_fields(&mut services, &["ref"]);
        members.push(Value::object([
            ("branch_id", branch.into()),
            (
                "resources",
                Value::object([
                    ("writable_paths", Value::List(paths)),
                    ("mutable_state_refs", strings(mutable)),
                    ("exclusive_device_refs", strings(devices)),
                    ("services", Value::List(services)),
                ]),
            ),
        ]));
    }
    let actual = members
        .iter()
        .map(|m| m["branch_id"].as_string().unwrap().clone())
        .collect::<Vec<_>>();
    let actual_set = actual.iter().cloned().collect::<BTreeSet<PyString>>();
    for member in expected_members {
        super::errors::require_hashable(member)?;
    }
    if actual_set.len() != actual.len()
        || expected_members
            .iter()
            .any(|m| !actual_set.iter().any(|a| m.as_string() == Some(a)))
        || actual_set
            .iter()
            .any(|a| !expected_members.iter().any(|m| m.as_string() == Some(a)))
    {
        return Err(fail("members", "FANOUT_MEMBER_INVALID"));
    }
    let mut dependencies = Vec::new();
    for (index, item) in list(
        &value["dependencies"],
        "dependencies",
        "FANOUT_DEPENDENCY_INVALID",
    )?
    .iter()
    .enumerate()
    {
        let field = format!("dependencies[{index}]");
        object(item, &field, "FANOUT_DEPENDENCY_INVALID")?;
        const DEPENDENCY_KEYS: &[&str] = &["before_branch_id", "after_branch_id", "reason"];
        require_keys(item, DEPENDENCY_KEYS, DEPENDENCY_KEYS, &field)?;
        dependencies.push(Value::object([
            (
                "before_branch_id",
                opaque(
                    &item["before_branch_id"],
                    &format!("{field}.before_branch_id"),
                )?
                .into(),
            ),
            (
                "after_branch_id",
                opaque(
                    &item["after_branch_id"],
                    &format!("{field}.after_branch_id"),
                )?
                .into(),
            ),
            (
                "reason",
                Value::String(
                    bounded_string(&item["reason"], &format!("{field}.reason"), 1024)?.clone(),
                ),
            ),
        ]));
    }
    let evidence_items = list(&value["evidence"], "evidence", "FANOUT_EVIDENCE_REQUIRED")?;
    if evidence_items.is_empty() {
        return Err(fail("evidence", "FANOUT_EVIDENCE_REQUIRED"));
    }
    let mut evidence = Vec::new();
    let mut references = BTreeSet::new();
    for (index, item) in evidence_items.iter().enumerate() {
        let field = format!("evidence[{index}]");
        object(item, &field, "FANOUT_EVIDENCE_REQUIRED")?;
        require_keys(
            item,
            &["kind", "ref", "sha256"],
            &["kind", "ref", "sha256"],
            &field,
        )?;
        let kind = opaque(&item["kind"], &format!("{field}.kind"))?;
        let reference = validate_ref(&item["ref"], &format!("{field}.ref"), true)?;
        let hash = digest(&item["sha256"], &format!("{field}.sha256"))?;
        references.insert(reference.clone());
        evidence.push(Value::object([
            ("kind", kind.into()),
            ("ref", Value::String(reference)),
            ("sha256", hash.into()),
        ]));
    }
    if references.len() != evidence.len() {
        return Err(fail("evidence", "FANOUT_EVIDENCE_REQUIRED"));
    }
    sort_by_fields(&mut members, &["branch_id"]);
    sort_by_fields(
        &mut dependencies,
        &["before_branch_id", "after_branch_id", "reason"],
    );
    sort_by_fields(&mut evidence, &["kind", "ref"]);
    Ok(Value::object([
        ("schema_version", 1i64.into()),
        ("kind", "fanout_assessment".into()),
        ("run_id", run_id.clone()),
        ("fanout_id", fanout_id.clone()),
        ("members", Value::List(members)),
        ("dependencies", Value::List(dependencies)),
        ("evidence", Value::List(evidence)),
    ]))
}
