use super::contract_values::{
    fail, in_strings, list, object, sort_by_fields, strings, unique_strings, values,
};
use super::contracts::{
    self, bounded_string, digest, opaque, require_keys, validate_ref, ProjectionError,
};
use super::errors::{
    attribute_error, iterable_set, iterable_values, set_contains, string_item,
    LocatedValidationFailure, ValidationFailure, ValidationResult,
};
use crate::compat::{unicode, PyString, Value};
use num_bigint::BigInt;
use std::collections::BTreeSet;

pub const ROUTES: &[&str] = &[
    "advisory",
    "design_only",
    "delivery_only",
    "full_delivery",
    "fast_path",
];
pub const RISK_TAGS: &[&str] = &[
    "production_behavior",
    "cross_layer",
    "dependency",
    "persistence",
    "security_privacy",
    "release_operations",
    "audio_realtime_translation",
    "ios_webkit_native",
];
pub const EFFECTS: &[&str] = &[
    "filesystem_read",
    "filesystem_write",
    "command",
    "external_read",
    "external_write",
    "destructive",
    "publish",
    "deploy",
];

fn routes_for_mode(mode: &Value) -> ValidationResult<&'static [&'static str]> {
    super::errors::require_hashable(mode)?;
    if mode.is_text("advisory") {
        Ok(&["advisory"])
    } else if mode.is_text("design_only") {
        Ok(&["design_only"])
    } else if mode.is_text("delivery") {
        Ok(&["fast_path", "delivery_only", "full_delivery"])
    } else {
        Ok(&[])
    }
}

fn includes_tag(tags: &[PyString], tag: &str) -> bool {
    tags.iter().any(|item| item.equals(tag))
}
fn risky(tags: &[PyString]) -> bool {
    RISK_TAGS.iter().any(|tag| includes_tag(tags, tag))
}

pub fn validate_task_brief(
    value: &Value,
    policy_digest: &PyString,
    policy: &Value,
    projection_version: Option<&Value>,
    windows: bool,
) -> ValidationResult<Value> {
    object(value, "task_brief", "INVALID_OBJECT")?;
    const REQUIRED: &[&str] = &[
        "schema_version",
        "task_id",
        "objective",
        "user_outcome",
        "request_mode",
        "minimum_route",
        "mandatory_impact_tags",
        "scope",
        "constraints",
        "acceptance_criteria",
        "risk_level",
        "authority",
        "policy_approval",
        "evidence_paths",
        "inspection_budget",
        "required_check_ids",
        "required_human_decisions",
    ];
    let schema = &value["schema_version"];
    let modern = schema.py_eq(&2i64.into()) || schema.py_eq(&3i64.into());
    let newest = schema.py_eq(&3i64.into());
    let mut required = REQUIRED.to_vec();
    let mut allowed = REQUIRED.to_vec();
    allowed.push("reviewer_delegation");
    if modern {
        required.push("model_sizing");
        allowed.extend(["model_sizing", "delivery_readiness", "model_overrides"]);
        if newest {
            required.push("helper_allowance");
            allowed.push("helper_allowance");
        }
    } else if !schema.py_eq(&1i64.into()) {
        allowed.push("model_sizing");
    }
    require_keys(value, &required, &allowed, "task_brief")?;
    if !modern && !schema.py_eq(&1i64.into()) {
        return Err(fail("schema_version", "UNSUPPORTED_SCHEMA"));
    }
    if let Some(overrides) = value.get("model_overrides") {
        super::execution::validate_model_overrides(overrides)
            .map_err(|message| fail("model_overrides", &message))?;
    }
    opaque(&value["task_id"], "task_id")?;
    bounded_string(&value["objective"], "objective", 4096)?;
    bounded_string(&value["user_outcome"], "user_outcome", 4096)?;
    let mode = &value["request_mode"];
    let minimum = &value["minimum_route"];
    let mode_routes = routes_for_mode(mode)?;
    if mode_routes.is_empty() || !in_strings(minimum, mode_routes)? {
        return Err(fail("minimum_route", "ROUTE_MODE_MISMATCH"));
    }
    let allowed_tags = iterable_set(string_item(policy, "impact_tags")?)?;
    let tags = unique_strings(
        &value["mandatory_impact_tags"],
        "mandatory_impact_tags",
        Some(&allowed_tags),
    )?;
    if mode.is_text("delivery") && minimum.is_text("fast_path") && risky(&tags) {
        return Err(fail("minimum_route", "FAST_PATH_INVARIANT"));
    }
    if minimum.is_text("delivery_only") && !modern {
        return Err(fail("delivery_readiness", "DELIVERY_READINESS_REQUIRED"));
    }
    let scope = &value["scope"];
    object(scope, "scope", "INVALID_OBJECT")?;
    require_keys(
        scope,
        &["included", "excluded"],
        &["included", "excluded"],
        "scope",
    )?;
    unique_strings(&scope["included"], "scope.included", None)?;
    unique_strings(&scope["excluded"], "scope.excluded", None)?;
    unique_strings(&value["constraints"], "constraints", None)?;
    let criteria = list(
        &value["acceptance_criteria"],
        "acceptance_criteria",
        "INVALID_LIST",
    )?;
    if criteria.is_empty() {
        return Err(fail("acceptance_criteria", "INVALID_LIST"));
    }
    let mut criterion_ids = Vec::new();
    let mut seen = BTreeSet::new();
    for criterion in criteria {
        object(criterion, "acceptance_criteria", "INVALID_OBJECT")?;
        require_keys(
            criterion,
            &["id", "text"],
            &["id", "text"],
            "acceptance_criteria",
        )?;
        let id = opaque(&criterion["id"], "acceptance_criteria.id")?;
        seen.insert(id.clone());
        criterion_ids.push(Value::from(id));
        bounded_string(&criterion["text"], "acceptance_criteria.text", 4096)?;
    }
    if seen.len() != criterion_ids.len() {
        return Err(fail("acceptance_criteria", "DUPLICATE_ID"));
    }
    let reviewer_policy = super::contract_values::get(policy, "reviewer_delegation")?;
    let mut delegation = super::reviewer_delegation::validate_task_config(
        &value["reviewer_delegation"],
        (!reviewer_policy.is_null()).then_some(reviewer_policy),
        &criterion_ids,
    )?;
    if !in_strings(&value["risk_level"], &["low", "medium", "high", "critical"])? {
        return Err(fail("risk_level", "UNKNOWN_VALUE"));
    }
    if modern {
        let sizing = &value["model_sizing"];
        object(sizing, "model_sizing", "INVALID_OBJECT")?;
        require_keys(
            sizing,
            &["scope_extent", "uncertainty"],
            &["scope_extent", "uncertainty"],
            "model_sizing",
        )?;
        if !in_strings(
            &sizing["scope_extent"],
            &["bounded", "cross_file", "broadly_cross_cutting"],
        )? {
            return Err(fail("model_sizing.scope_extent", "UNKNOWN_VALUE"));
        }
        if !in_strings(&sizing["uncertainty"], &["low", "medium", "high"])? {
            return Err(fail("model_sizing.uncertainty", "UNKNOWN_VALUE"));
        }
    }
    let readiness = &value["delivery_readiness"];
    if !readiness.is_null() {
        if !modern {
            return Err(fail("delivery_readiness", "UNSUPPORTED_SCHEMA"));
        }
        object(readiness, "delivery_readiness", "INVALID_OBJECT")?;
        const FIELDS: &[&str] = &[
            "requirements_source",
            "requirements_complete",
            "acceptance_criteria_complete",
            "implementation_decisions_resolved",
            "architecture_decisions_resolved",
            "unresolved_items",
        ];
        require_keys(readiness, FIELDS, FIELDS, "delivery_readiness")?;
        bounded_string(
            &readiness["requirements_source"],
            "delivery_readiness.requirements_source",
            1024,
        )?;
        for key in &FIELDS[1..5] {
            if !readiness[key].is_true() {
                return Err(fail(
                    &format!("delivery_readiness.{key}"),
                    "READINESS_ASSERTION_REQUIRED",
                ));
            }
        }
        if !unique_strings(
            &readiness["unresolved_items"],
            "delivery_readiness.unresolved_items",
            None,
        )?
        .is_empty()
        {
            return Err(fail(
                "delivery_readiness.unresolved_items",
                "UNRESOLVED_ITEMS_PRESENT",
            ));
        }
    }
    if minimum.is_text("delivery_only") {
        if readiness.is_null() {
            return Err(fail("delivery_readiness", "DELIVERY_READINESS_REQUIRED"));
        }
        if in_strings(&value["risk_level"], &["high", "critical"])?
            || includes_tag(&tags, "security_privacy")
            || value["model_sizing"]["scope_extent"].is_text("broadly_cross_cutting")
            || value["model_sizing"]["uncertainty"].is_text("high")
        {
            return Err(fail("minimum_route", "DELIVERY_ONLY_INELIGIBLE"));
        }
    }
    if newest {
        let allowance = &value["helper_allowance"];
        object(allowance, "helper_allowance", "INVALID_OBJECT")?;
        require_keys(
            allowance,
            &["ref", "sha256"],
            &["ref", "sha256"],
            "helper_allowance",
        )?;
        let reference = validate_ref(&allowance["ref"], "helper_allowance.ref", false)?;
        let runtime = string_item(policy, "schema_version")?.py_eq(&3i64.into());
        if !reference.starts_with(if runtime { "runtime:" } else { "repo:" })
            || reference.contains("#")
        {
            return Err(fail(
                "helper_allowance.ref",
                if runtime {
                    "RUNTIME_REF_REQUIRED"
                } else {
                    "REPOSITORY_REF_REQUIRED"
                },
            ));
        }
        if !unicode::lower(&reference).ends_with(".json") {
            return Err(fail("helper_allowance.ref", "JSON_REQUIRED"));
        }
        digest(&allowance["sha256"], "helper_allowance.sha256")?;
    }
    if value["risk_level"].is_text("critical") && mode.is_text("delivery") {
        if !minimum.is_text("full_delivery") {
            return Err(fail("minimum_route", "CRITICAL_REQUIRES_FULL_DELIVERY"));
        }
        if !includes_tag(&tags, "security_privacy") {
            return Err(fail(
                "mandatory_impact_tags",
                "CRITICAL_REQUIRES_SECURITY_REVIEW",
            ));
        }
    }
    let authority = &value["authority"];
    object(authority, "authority", "INVALID_OBJECT")?;
    require_keys(authority, &["capabilities"], &["capabilities"], "authority")?;
    let capabilities = list(
        &authority["capabilities"],
        "authority.capabilities",
        "INVALID_LIST",
    )?;
    let checks = string_item(policy, "required_checks")?;
    let command_ids = values(checks)?
        .into_iter()
        .map(|check| string_item(check, "command_id").cloned())
        .collect::<ValidationResult<Vec<_>>>()?;
    for id in &command_ids {
        super::errors::require_hashable(id)?;
    }
    let mut policy_capabilities = Vec::new();
    let role_capabilities = string_item(policy, "role_capabilities")?;
    for caps in values(role_capabilities)? {
        policy_capabilities.extend(iterable_values(caps)?);
    }
    let mut canonical = Vec::new();
    for cap in capabilities {
        object(cap, "authority.capabilities", "INVALID_OBJECT")?;
        require_keys(
            cap,
            &["effect", "action", "target_ref"],
            &["effect", "action", "target_ref"],
            "capability",
        )?;
        if !in_strings(&cap["effect"], EFFECTS)? {
            return Err(fail("capability.effect", "UNKNOWN_VALUE"));
        }
        let action = opaque(&cap["action"], "capability.action")?;
        let target = if cap["effect"].as_string().unwrap().starts_with("filesystem") {
            let denied = iterable_values(string_item(policy, "denied_patterns")?)?;
            let patterns = denied
                .iter()
                .map(|item| {
                    item.as_string()
                        .cloned()
                        .ok_or_else(|| attribute_error(item, "lower"))
                })
                .collect::<ValidationResult<Vec<_>>>()?;
            contracts::filesystem_capability_target(&cap["target_ref"], &patterns)?
        } else {
            PyString::from(opaque(&cap["target_ref"], "capability.target_ref")?)
        };
        if cap["effect"].is_text("command")
            && !set_contains(&command_ids, &Value::String(target.clone()))
        {
            return Err(fail("capability.target_ref", "UNKNOWN_COMMAND_TARGET"));
        }
        let mut permitted = false;
        for ceiling in &policy_capabilities {
            if contracts::capability_within(cap, ceiling, windows)? {
                permitted = true;
                break;
            }
        }
        if !permitted {
            return Err(fail("authority.capabilities", "POLICY_AUTHORITY_EXCEEDED"));
        }
        if mode.is_text("advisory")
            && !in_strings(&cap["effect"], &["filesystem_read", "external_read"])?
        {
            return Err(fail("authority.capabilities", "ADVISORY_MUST_BE_READ_ONLY"));
        }
        canonical.push(Value::object([
            ("effect", cap["effect"].clone()),
            ("action", action.into()),
            ("target_ref", Value::String(target)),
        ]));
    }
    let keys = canonical
        .iter()
        .map(|cap| {
            (
                cap["effect"].as_string().unwrap(),
                cap["action"].as_string().unwrap(),
                cap["target_ref"].as_string().unwrap(),
            )
        })
        .collect::<BTreeSet<_>>();
    if keys.len() != canonical.len() {
        return Err(fail("authority.capabilities", "DUPLICATE_VALUE"));
    }
    let approval = &value["policy_approval"];
    object(approval, "policy_approval", "INVALID_OBJECT")?;
    require_keys(
        approval,
        &["sha256", "authority_ref"],
        &["sha256", "authority_ref"],
        "policy_approval",
    )?;
    if !policy_digest.equals(&digest(&approval["sha256"], "policy_approval.sha256")?) {
        return Err(fail("policy_approval.sha256", "POLICY_DIGEST_MISMATCH"));
    }
    validate_ref(
        &approval["authority_ref"],
        "policy_approval.authority_ref",
        false,
    )?;
    for reference in iterable_values(&value["evidence_paths"])? {
        validate_ref(&reference, "evidence_paths", false)?;
    }
    let budget = &value["inspection_budget"];
    object(budget, "inspection_budget", "INVALID_OBJECT")?;
    require_keys(
        budget,
        &["file_reads", "discovery_commands"],
        &["file_reads", "discovery_commands"],
        "inspection_budget",
    )?;
    for key in ["file_reads", "discovery_commands"] {
        let amount = &budget[key];
        let valid = if let Some(amount) = amount.strict_integer() {
            if amount < &BigInt::from(0) {
                false
            } else {
                let limit = string_item(
                    string_item(string_item(policy, "limits")?, "inspection")?,
                    key,
                )?;
                integer_within_limit(amount, limit)?
            }
        } else {
            false
        };
        if !valid {
            return Err(fail(&format!("inspection_budget.{key}"), "BUDGET_LIMIT"));
        }
    }
    let check_ids = unique_strings(&value["required_check_ids"], "required_check_ids", None)?;
    let check_entries = checks
        .as_object()
        .ok_or_else(|| attribute_error(checks, "items"))?;
    for (key, cfg) in check_entries {
        if string_item(cfg, "mandatory")?.truthy() && !check_ids.contains(key) {
            return Err(fail("required_check_ids", "MISSING_REQUIRED_CHECK"));
        }
    }
    let human = unique_strings(
        &value["required_human_decisions"],
        "required_human_decisions",
        None,
    )?;
    if minimum.is_text("delivery_only") && !human.is_empty() {
        return Err(fail(
            "required_human_decisions",
            "DELIVERY_ONLY_UNRESOLVED_DECISION",
        ));
    }
    sort_by_fields(&mut canonical, &["effect", "action", "target_ref"]);
    if let Some(config) = delegation.as_mut() {
        let mut assignments = config["assignments"].as_list().unwrap().to_vec();
        for assignment in &mut assignments {
            let scope_refs = iterable_values(&assignment["scope_refs"])?;
            let role = &assignment["role"];
            if role_capabilities.as_object().is_none() {
                return Err(attribute_error(role_capabilities, "get"));
            }
            super::errors::require_hashable(role)?;
            let ceilings = role_capabilities
                .as_object()
                .unwrap()
                .iter()
                .find(|(key, _)| role.as_string() == Some(key))
                .map(|(_, item)| iterable_values(item))
                .transpose()?
                .unwrap_or_default();
            let mut projected = Vec::new();
            for cap in &canonical {
                if !in_strings(&cap["effect"], &["filesystem_read", "external_read"])?
                    || !scope_refs
                        .iter()
                        .any(|reference| reference.container_eq(&cap["target_ref"]))
                {
                    continue;
                }
                for ceiling in &ceilings {
                    let allowed = contracts::projected_capability_within(
                        cap,
                        ceiling,
                        projection_version,
                        windows,
                    )
                    .map_err(|error| match error {
                        ProjectionError::Validation(error) => error,
                        ProjectionError::InvalidVersion => ValidationFailure::python(
                            "ValueError",
                            "CAPABILITY_PROJECTION_VERSION_INVALID",
                        ),
                    })?;
                    if allowed {
                        projected.push(cap.clone());
                        break;
                    }
                }
            }
            assignment.set("effect_capabilities", Value::List(projected));
        }
        config.set("assignments", Value::List(assignments));
    }
    let mut result = value.clone();
    result.set("mandatory_impact_tags", strings(tags));
    result.set(
        "authority",
        Value::object([("capabilities", Value::List(canonical))]),
    );
    if let Some(delegation) = delegation {
        result.set("reviewer_delegation", delegation);
    }
    Ok(result)
}

fn integer_within_limit(amount: &BigInt, limit: &Value) -> ValidationResult<bool> {
    match limit {
        Value::Integer(limit) => Ok(amount <= limit),
        Value::Bool(limit) => Ok(amount <= &BigInt::from(u8::from(*limit))),
        Value::Float(limit) if limit.is_nan() => Ok(true),
        Value::ConstantNaN => Ok(true),
        Value::Float(limit) if *limit == f64::INFINITY => Ok(true),
        Value::Float(limit) if *limit == f64::NEG_INFINITY => Ok(false),
        Value::Float(limit) => {
            use num_traits::FromPrimitive;
            Ok(amount <= &BigInt::from_f64(limit.floor()).unwrap())
        }
        _ => Err(ValidationFailure::python(
            "TypeError",
            format!(
                "'>' not supported between instances of 'int' and '{}'",
                limit.python_type()
            ),
        )),
    }
}

pub fn validate_impact_map(value: &Value, task: &Value, policy: &Value) -> ValidationResult<Value> {
    validate_impact_map_with_site(value, task, policy).map_err(|failure| failure.error)
}

#[derive(Clone, Copy)]
pub(crate) enum ImpactMapValidationSite {
    Route,
    EvidenceReferences,
}

pub(crate) fn validate_impact_map_with_site(
    value: &Value,
    task: &Value,
    policy: &Value,
) -> Result<Value, LocatedValidationFailure<ImpactMapValidationSite>> {
    object(value, "impact_map", "INVALID_OBJECT")?;
    const KEYS: &[&str] = &[
        "schema_version",
        "task_id",
        "route_label",
        "impact_tags",
        "evidence_refs",
        "attempt_id",
        "claim_digest",
    ];
    require_keys(value, KEYS, KEYS, "impact_map")?;
    super::contract_values::validate_attempt(value, "impact_map")?;
    if !value["schema_version"].py_eq(&1i64.into())
        || !value["task_id"].py_eq(string_item(task, "task_id")?)
    {
        return Err(fail("impact_map", "TASK_OR_SCHEMA_MISMATCH").into());
    }
    let route = &value["route_label"];
    let known_route = in_strings(route, ROUTES)
        .map_err(|error| LocatedValidationFailure::at(error, ImpactMapValidationSite::Route))?;
    if !known_route
        || !super::errors::iterable_values(string_item(policy, "routes")?)?
            .iter()
            .any(|item| item.container_eq(route))
    {
        return Err(fail("route_label", "UNKNOWN_ROUTE").into());
    }
    let mode = string_item(task, "request_mode")?;
    let mode_routes = routes_for_mode(mode)?;
    if mode_routes.is_empty() {
        let representation = crate::compat::repr::value_repr(mode)
            .map_err(|error| ValidationFailure::python("ValueError", error.to_string()))?;
        return Err(ValidationFailure::python("KeyError", representation).into());
    }
    if !in_strings(route, mode_routes)? {
        return Err(fail("route_label", "ROUTE_MODE_MISMATCH").into());
    }
    let minimum = string_item(task, "minimum_route")?;
    if mode.is_text("delivery") {
        if (minimum.is_text("full_delivery") && !route.is_text("full_delivery"))
            || (minimum.is_text("delivery_only")
                && !in_strings(route, &["delivery_only", "full_delivery"])?)
        {
            return Err(fail("route_label", "ROUTE_DOWNGRADE").into());
        }
        if minimum.is_text("fast_path") && route.is_text("delivery_only") {
            return Err(fail("route_label", "DELIVERY_READINESS_REQUIRED").into());
        }
    }
    let allowed = iterable_set(string_item(policy, "impact_tags")?)?;
    let tags = unique_strings(&value["impact_tags"], "impact_tags", Some(&allowed))?;
    for tag in iterable_set(string_item(task, "mandatory_impact_tags")?)? {
        if !tags.iter().any(|item| tag.as_string() == Some(item)) {
            return Err(fail("impact_tags", "MANDATORY_TAG_REMOVED").into());
        }
    }
    if mode.is_text("delivery") && string_item(task, "risk_level")?.is_text("critical") {
        if !route.is_text("full_delivery") {
            return Err(fail("route_label", "CRITICAL_REQUIRES_FULL_DELIVERY").into());
        }
        if !includes_tag(&tags, "security_privacy") {
            return Err(fail("impact_tags", "CRITICAL_REQUIRES_SECURITY_REVIEW").into());
        }
    }
    if mode.is_text("delivery") && route.is_text("fast_path") && risky(&tags) {
        return Err(fail("route_label", "FAST_PATH_INVARIANT").into());
    }
    if mode.is_text("delivery")
        && route.is_text("delivery_only")
        && includes_tag(&tags, "security_privacy")
    {
        return Err(fail("route_label", "DELIVERY_ONLY_INELIGIBLE").into());
    }
    let references = iterable_values(&value["evidence_refs"]).map_err(|error| {
        LocatedValidationFailure::at(error, ImpactMapValidationSite::EvidenceReferences)
    })?;
    for reference in references {
        validate_ref(&reference, "evidence_refs", false)?;
    }
    let mut result = value.clone();
    result.set("impact_tags", strings(tags));
    Ok(result)
}

pub fn authoritative_task_subset(
    value: &Value,
) -> ValidationResult<super::python_dictionary::PythonDictionary> {
    let schema = string_item(value, "schema_version")?;
    let mut result = super::python_dictionary::PythonDictionary::default();
    for key in ["schema_version", "task_id", "request_mode", "minimum_route"] {
        result.set(key, string_item(value, key)?.clone());
    }
    result.set(
        "mandatory_impact_tags",
        Value::List(iterable_values(string_item(
            value,
            "mandatory_impact_tags",
        )?)?),
    );
    result.set("risk_level", string_item(value, "risk_level")?.clone());
    let mut acceptance = Vec::new();
    for criterion in iterable_values(string_item(value, "acceptance_criteria")?)? {
        acceptance.push(string_item(&criterion, "id")?.clone());
    }
    result.set("acceptance_ids", Value::List(acceptance));
    result.set("authority", string_item(value, "authority")?.clone());
    result.set(
        "policy_approval",
        string_item(value, "policy_approval")?.clone(),
    );
    result.set(
        "evidence_paths",
        Value::List(iterable_values(string_item(value, "evidence_paths")?)?),
    );
    result.set(
        "inspection_budget",
        copy_dictionary(string_item(value, "inspection_budget")?)?,
    );
    for key in ["required_check_ids", "required_human_decisions"] {
        result.set(key, Value::List(iterable_values(string_item(value, key)?)?));
    }
    if in_strings_numeric_schema(schema)? {
        result.set(
            "model_sizing",
            copy_dictionary(string_item(value, "model_sizing")?)?,
        );
    }
    if !super::contract_values::get(value, "delivery_readiness")?.is_null() {
        result.set(
            "delivery_readiness",
            copy_dictionary(&value["delivery_readiness"])?,
        );
    }
    if schema.py_eq(&3i64.into()) {
        result.set(
            "helper_allowance",
            copy_dictionary(string_item(value, "helper_allowance")?)?,
        );
    }
    if !super::contract_values::get(value, "reviewer_delegation")?.is_null() {
        result.set("reviewer_delegation", value["reviewer_delegation"].clone());
    }
    if let Some(overrides) = value.get("model_overrides") {
        let entries = overrides
            .as_object()
            .ok_or_else(|| attribute_error(overrides, "items"))?;
        let mut normalized = super::python_dictionary::PythonDictionary::default();
        for (key, pair) in entries {
            normalized.insert(Value::String(key.clone()), copy_dictionary(pair)?.into())?;
        }
        result.set("model_overrides", normalized);
    }
    Ok(result)
}

fn in_strings_numeric_schema(schema: &Value) -> ValidationResult<bool> {
    super::errors::require_hashable(schema)?;
    Ok(schema.py_eq(&2i64.into()) || schema.py_eq(&3i64.into()))
}

fn copy_dictionary(value: &Value) -> ValidationResult<super::python_dictionary::PythonDictionary> {
    super::python_dictionary::PythonDictionary::coerce(value)
}
