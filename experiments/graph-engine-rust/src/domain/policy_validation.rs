use super::{
    contracts::{self, ContractError},
    errors::{attribute_error, require_hashable, string_item, ValidationFailure, ValidationResult},
    policy,
};
use crate::compat::{glob, integer, unicode, PyString, Value};
use num_bigint::BigInt;

pub fn version_tuple(value: &Value) -> Result<[BigInt; 3], ContractError> {
    let invalid = || ContractError::new("compatible_engine", "INVALID_VERSION");
    let text = value.as_string().ok_or_else(invalid)?;
    let parts = text
        .0
        .split(|&point| point == 46)
        .map(|part| integer::parse_decimal(&PyString(part.to_vec())).map_err(|_| invalid()))
        .collect::<Result<Vec<_>, _>>()?;
    parts.try_into().map_err(|_| invalid())
}

pub fn engine_version_compatible(value: &Value, configured: &Value) -> ValidationResult<bool> {
    let compatible = string_item(configured, "compatible_engine")?;
    let candidate = version_tuple(value)?;
    Ok(version_tuple(string_item(compatible, "min")?)? <= candidate
        && candidate < version_tuple(string_item(compatible, "max_exclusive")?)?)
}

pub fn validate_output_contract(node: &str, value: &Value) -> ValidationResult<()> {
    let (_, _, _, _, _, decisions, _) = policy::NODE_SPECS
        .iter()
        .find(|(key, ..)| *key == node)
        .ok_or_else(|| ValidationFailure::python("KeyError", format!("'{node}'")))?;
    let mut keys = vec!["artifact_kind", "schema_version", "artifact_required"];
    if !decisions.is_empty() {
        keys.push("decision_values");
    }
    if node == "senior_engineer" {
        keys.extend([
            "artifact_required_for_decisions",
            "evidence_required_for_decisions",
        ]);
    }
    if policy::RESEARCH.iter().any(|(key, _)| *key == node) {
        keys.extend([
            "evidence_required",
            "decision_forbidden",
            "findings_forbidden",
        ]);
    }
    let field = format!("node_templates.{node}.output_contract");
    let keys = Value::list(keys.into_iter().map(Value::from));
    contracts::require_python_keys(value, &keys, &keys, &field)?;
    if !value.py_eq(&policy::node_templates()[node]["output_contract"]) {
        return Err(ContractError::new(&field, "ENGINE_CONTRACT_CHANGED").into());
    }
    Ok(())
}

pub fn validate_capability_shape(
    value: &Value,
    role: &str,
    command_ids: &[String],
) -> ValidationResult<(Value, Value, Value)> {
    let field = format!("role_capabilities.{role}");
    if value.as_object().is_none() {
        return Err(ContractError::new(&field, "INVALID_OBJECT").into());
    }
    contracts::require_keys(
        value,
        &["effect", "action", "target_ref"],
        &["effect", "action", "target_ref"],
        &field,
    )?;
    let effect = &value["effect"];
    require_hashable(effect)?;
    let effects = [
        "filesystem_read",
        "filesystem_write",
        "command",
        "external_read",
        "external_write",
        "destructive",
        "publish",
        "deploy",
    ];
    if !effects.iter().any(|name| effect.is_text(name)) {
        return Err(ContractError::new(&field, "ENGINE_AUTHORITY_EXCEEDED").into());
    }
    let target = &value["target_ref"];
    if effect.as_string().unwrap().starts_with("filesystem") {
        let target = contracts::validate_ref(target, "capability.target_ref", false)?;
        if !["repo:", "runtime:", "profile:software-engineering-graph/"]
            .iter()
            .any(|prefix| target.starts_with(prefix))
        {
            return Err(
                ContractError::new("capability.target_ref", "FILESYSTEM_REF_REQUIRED").into(),
            );
        }
    } else {
        let identity = contracts::opaque(target, "capability.target_ref")?;
        if effect.is_text("command") && !command_ids.contains(&identity) {
            return Err(
                ContractError::new("capability.target_ref", "UNKNOWN_COMMAND_TARGET").into(),
            );
        }
    }
    Ok((effect.clone(), value["action"].clone(), target.clone()))
}

pub fn repository_target_spelling(value: &Value, denied: &[Value]) -> ValidationResult<PyString> {
    let target = value
        .as_string()
        .ok_or_else(|| attribute_error(value, "startswith"))?;
    if !target.starts_with("repo:") {
        return Err(ContractError::new("capability.target_ref", "REPOSITORY_REF_REQUIRED").into());
    }
    let relative = contracts::lexical_relative(
        &Value::String(PyString(target.0[5..].to_vec())),
        "capability.target_ref",
    )?;
    if relative
        .0
        .iter()
        .any(|&point| matches!(point, 42 | 63 | 91 | 93 | 123 | 125))
    {
        return Err(ContractError::new("capability.target_ref", "WILDCARD_FORBIDDEN").into());
    }
    let normalized = unicode::lower(&relative);
    let mut prefixed = PyString::from("/");
    prefixed.0.extend(&normalized.0);
    for pattern in denied {
        let pattern = pattern
            .as_string()
            .ok_or_else(|| attribute_error(pattern, "lower"))?;
        let lowered = unicode::lower(pattern);
        if glob::fnmatchcase(&normalized, &lowered)
            || glob::fnmatchcase(&prefixed, &lowered)
            || glob::posix_path_match(&normalized, &lowered)
                .map_err(|_| ValidationFailure::python("ValueError", "empty pattern"))?
        {
            return Err(ContractError::new("capability.target_ref", "SENSITIVE_PATH").into());
        }
    }
    if [".codex/", ".agents/", ".claude/", ".cursor/"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
    {
        return Err(ContractError::new(
            "capability.target_ref",
            "PROFILE_OR_POLICY_ROOT_FORBIDDEN",
        )
        .into());
    }
    let mut result = PyString::from("repo:");
    result.0.extend(relative.0);
    Ok(result)
}

pub fn runtime_target(value: &Value, roots: &[Value], windows: bool) -> ValidationResult<PyString> {
    let target = value
        .as_string()
        .ok_or_else(|| attribute_error(value, "startswith"))?;
    if !target.starts_with("runtime:") {
        return Err(ContractError::new("capability.target_ref", "RUNTIME_REF_REQUIRED").into());
    }
    let relative = contracts::lexical_relative(
        &Value::String(PyString(target.0[8..].to_vec())),
        "capability.target_ref",
    )?;
    if relative
        .0
        .iter()
        .any(|&point| matches!(point, 42 | 63 | 91 | 93 | 123 | 125))
    {
        return Err(ContractError::new("capability.target_ref", "WILDCARD_FORBIDDEN").into());
    }
    let mut normalized = PyString::from("runtime:");
    normalized.0.extend(relative.0);
    let mut allowed = false;
    for root in roots {
        if root.as_string().is_none() {
            return Err(attribute_error(root, "rstrip"));
        }
        if policy::under_configured_root(&normalized, std::slice::from_ref(root), windows) {
            allowed = true;
            break;
        }
    }
    if !allowed {
        return Err(ContractError::new("capability.target_ref", "OUTSIDE_ALLOWED_ROOT").into());
    }
    Ok(normalized)
}

fn integer_in(value: &Value, minimum: i64, maximum: i64, allow_bool: bool) -> bool {
    let integer = match value {
        Value::Bool(value) if allow_bool => Some(i64::from(*value)),
        _ => value.as_i64(),
    };
    integer.is_some_and(|number| minimum <= number && number <= maximum)
}
fn object_names(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|items| {
        items.len() == expected.len() && expected.iter().all(|name| value.get(name).is_some())
    })
}
fn set_matches(values: &[Value], expected: &[&str]) -> ValidationResult<bool> {
    let unique = super::errors::set_values(values)?;
    Ok(unique.len() == expected.len()
        && expected
            .iter()
            .all(|name| unique.iter().any(|item| item.is_text(name))))
}
pub fn validate_header(value: &Value) -> ValidationResult<i64> {
    if value.as_object().is_none() {
        return Err(ContractError::new("policy", "INVALID_OBJECT").into());
    }
    let base = [
        "schema_version",
        "repository_id",
        "compatible_engine",
        "artifact_roots",
        "artifact_kinds",
        "impact_tags",
        "routes",
        "node_templates",
        "specialists",
        "limits",
        "required_checks",
        "role_capabilities",
        "denied_patterns",
    ];
    let version = value["schema_version"].as_i64();
    if !version.is_some_and(|version| (1..=3).contains(&version)) {
        let mut allowed = base.to_vec();
        allowed.extend(["reviewer_delegation", "implementation_roots"]);
        contracts::require_keys(value, &base, &allowed, "policy")?;
        return Err(ContractError::new("policy.schema_version", "UNSUPPORTED_SCHEMA").into());
    }
    let version = version.unwrap();
    let mut required = base.to_vec();
    if version >= 2 {
        required.push("implementation_roots");
    }
    let mut allowed = required.clone();
    allowed.push("reviewer_delegation");
    contracts::require_keys(value, &required, &allowed, "policy")?;
    contracts::opaque(&value["repository_id"], "repository_id")?;
    let compatible = &value["compatible_engine"];
    if compatible.as_object().is_none() {
        return Err(ContractError::new("compatible_engine", "INVALID_OBJECT").into());
    }
    contracts::require_keys(
        compatible,
        &["min", "max_exclusive"],
        &["min", "max_exclusive"],
        "compatible_engine",
    )?;
    if !engine_version_compatible(&Value::from(crate::ENGINE_VERSION), value)? {
        return Err(ContractError::new("compatible_engine", "ENGINE_INCOMPATIBLE").into());
    }
    Ok(version)
}
pub fn validate_topology(value: &mut Value) -> ValidationResult<()> {
    let tags = value["impact_tags"]
        .as_list()
        .ok_or_else(|| ContractError::new("impact_tags", "ENGINE_INVARIANT_CHANGED"))?;
    if !set_matches(tags, policy::IMPACT_TAGS)? || tags.len() != policy::IMPACT_TAGS.len() {
        return Err(ContractError::new("impact_tags", "ENGINE_INVARIANT_CHANGED").into());
    }
    let current_routes = policy::routes();
    let legacy_routes = Value::Object(
        current_routes
            .as_object()
            .unwrap()
            .iter()
            .filter(|(key, _)| !key.equals("delivery_only"))
            .cloned()
            .collect(),
    );
    if value["routes"].py_eq(&legacy_routes) {
        value.set("routes", current_routes);
    } else if !value["routes"].py_eq(&current_routes) {
        return Err(ContractError::new("routes", "ENGINE_TOPOLOGY_CHANGED").into());
    }
    let template_keys: Vec<_> = policy::NODE_SPECS.iter().map(|(key, ..)| *key).collect();
    let templates = &value["node_templates"];
    if !object_names(templates, &template_keys) {
        return Err(ContractError::new("node_templates", "ENGINE_TOPOLOGY_CHANGED").into());
    }
    for (key, role, stages, _, _, _, retry_ceiling) in policy::NODE_SPECS {
        let field = format!("node_templates.{key}");
        let template = &templates[key];
        if template.as_object().is_none() {
            return Err(ContractError::new(&field, "INVALID_OBJECT").into());
        }
        let keys = [
            "role",
            "stages",
            "mandatory",
            "max_retries",
            "output_contract",
        ];
        contracts::require_keys(template, &keys, &keys, &field)?;
        if !template["role"].is_text(role) {
            return Err(ContractError::new(&field, "ENGINE_NODE_BINDING_CHANGED").into());
        }
        let configured_stages = super::errors::iterable_set(&template["stages"])?;
        if !set_matches(&configured_stages, stages)?
            || !matches!(template["mandatory"], Value::Bool(true))
        {
            return Err(ContractError::new(&field, "ENGINE_NODE_BINDING_CHANGED").into());
        }
        if !integer_in(&template["max_retries"], 0, *retry_ceiling, false) {
            return Err(ContractError::new(
                &format!("{field}.max_retries"),
                "RETRY_CEILING_EXCEEDED",
            )
            .into());
        }
        validate_output_contract(key, &template["output_contract"])?;
    }
    let specialist_tags: Vec<_> = policy::SPECIALISTS.iter().map(|(tag, _)| *tag).collect();
    let specialists = &value["specialists"];
    if !object_names(specialists, &specialist_tags) {
        return Err(ContractError::new("specialists", "ENGINE_INVARIANT_CHANGED").into());
    }
    for (tag, node) in policy::SPECIALISTS {
        let expected = Value::object([
            ("node_key", (*node).into()),
            ("role", (*node).into()),
            ("stages", Value::list(["design".into(), "delivery".into()])),
            ("mandatory", true.into()),
        ]);
        if !specialists[tag].py_eq(&expected) {
            return Err(ContractError::new(
                &format!("specialists.{tag}"),
                "ENGINE_INVARIANT_CHANGED",
            )
            .into());
        }
    }
    Ok(())
}
pub fn validate_limits(value: &Value) -> ValidationResult<()> {
    if value.as_object().is_none() {
        return Err(ContractError::new("limits", "INVALID_OBJECT").into());
    }
    contracts::require_keys(
        value,
        &[
            "design_revisions",
            "delivery_repairs",
            "inspection",
            "manifest_bytes",
            "artifact_bytes",
        ],
        &[
            "design_revisions",
            "delivery_repairs",
            "inspection",
            "manifest_bytes",
            "artifact_bytes",
            "branch_lease_seconds",
        ],
        "limits",
    )?;
    if let Some(lease) = value.get("branch_lease_seconds") {
        if !integer_in(lease, 30, 3600, false) {
            return Err(ContractError::new("limits.branch_lease_seconds", "INVALID_LEASE").into());
        }
    }
    for (key, ceiling) in [("design_revisions", 2), ("delivery_repairs", 3)] {
        if !integer_in(&value[key], 0, ceiling, false) {
            return Err(
                ContractError::new(&format!("limits.{key}"), "LIMIT_MAY_NOT_INCREASE").into(),
            );
        }
    }
    let inspection = &value["inspection"];
    if inspection.as_object().is_none() {
        return Err(ContractError::new("limits.inspection", "INVALID_OBJECT").into());
    }
    contracts::require_keys(
        inspection,
        &["file_reads", "discovery_commands"],
        &["file_reads", "discovery_commands"],
        "limits.inspection",
    )?;
    for (key, ceiling) in [("file_reads", 12), ("discovery_commands", 8)] {
        if !integer_in(&inspection[key], 0, ceiling, false) {
            return Err(ContractError::new(
                &format!("limits.inspection.{key}"),
                "LIMIT_MAY_NOT_INCREASE",
            )
            .into());
        }
    }
    for (key, ceiling) in [
        ("manifest_bytes", 256 * 1024),
        ("artifact_bytes", 4 * 1024 * 1024),
    ] {
        if !integer_in(&value[key], 1, ceiling, true) {
            return Err(
                ContractError::new(&format!("limits.{key}"), "LIMIT_MAY_NOT_INCREASE").into(),
            );
        }
    }
    Ok(())
}
pub fn validate_artifact_kinds(value: &Value) -> ValidationResult<()> {
    let names: Vec<_> = policy::ARTIFACT_KINDS
        .iter()
        .map(|(name, _, _)| *name)
        .collect();
    if !object_names(value, &names) {
        return Err(ContractError::new("artifact_kinds", "ENGINE_ARTIFACT_POLICY_CHANGED").into());
    }
    for (kind, ceiling, allowed) in policy::ARTIFACT_KINDS {
        let field = format!("artifact_kinds.{kind}");
        let config = &value[kind];
        if config.as_object().is_none() {
            return Err(ContractError::new(&field, "INVALID_OBJECT").into());
        }
        contracts::require_keys(
            config,
            &["extensions", "max_bytes"],
            &["extensions", "max_bytes"],
            &field,
        )?;
        let extensions = config["extensions"]
            .as_list()
            .ok_or_else(|| ContractError::new(&field, "INVALID_EXTENSIONS"))?;
        if extensions.is_empty() {
            return Err(ContractError::new(&field, "INVALID_EXTENSIONS").into());
        }
        if super::errors::set_values(extensions)?.len() != extensions.len()
            || extensions.iter().any(|item| {
                item.as_string().is_none_or(|extension| {
                    !extension.starts_with(".") || *extension != unicode::lower(extension)
                })
            })
        {
            return Err(ContractError::new(&field, "INVALID_EXTENSIONS").into());
        }
        if extensions
            .iter()
            .any(|extension| !allowed.iter().any(|name| extension.is_text(name)))
        {
            return Err(ContractError::new(&field, "ENGINE_ARTIFACT_TYPE_EXCEEDED").into());
        }
        if !integer_in(&config["max_bytes"], 1, *ceiling as i64, true) {
            return Err(ContractError::new(&field, "LIMIT_MAY_NOT_INCREASE").into());
        }
    }
    Ok(())
}
pub fn validate_checks(value: &Value, version: i64) -> ValidationResult<Vec<String>> {
    let checks = value
        .as_object()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| ContractError::new("required_checks", "ENGINE_COMMAND_SET_CHANGED"))?;
    if version == 1 && value.get("repo-check").is_none() {
        return Err(ContractError::new("required_checks", "ENGINE_COMMAND_SET_CHANGED").into());
    }
    let mut command_ids = Vec::new();
    for (id, check) in checks {
        let identity = contracts::opaque(&Value::String(id.clone()), "required_checks")?;
        let field = format!("required_checks.{identity}");
        if check.as_object().is_none() {
            return Err(ContractError::new(&field, "INVALID_OBJECT").into());
        }
        contracts::require_keys(
            check,
            &["command_id", "mandatory"],
            &["command_id", "mandatory", "argv", "timeout_seconds"],
            &field,
        )?;
        let command = contracts::opaque(&check["command_id"], "required_checks.command_id")?;
        if version == 1 && identity == "repo-check" && command != "npm-run-check" {
            return Err(ContractError::new(&field, "ENGINE_COMMAND_SET_CHANGED").into());
        }
        if command_ids.contains(&command) || !matches!(check["mandatory"], Value::Bool(true)) {
            return Err(ContractError::new("required_checks", "INVALID_OR_DUPLICATE").into());
        }
        if version >= 2 && check.get("argv").is_none() {
            return Err(ContractError::new(&field, "MISSING_FIELD").into());
        }
        if let Some(argv) = check.get("argv") {
            if argv.as_list().is_none_or(|items| {
                items.is_empty()
                    || items.len() > 32
                    || items.iter().any(|item| {
                        item.as_string()
                            .is_none_or(|text| text.is_empty() || text.len() > 1024)
                    })
            }) {
                return Err(ContractError::new(&format!("{field}.argv"), "INVALID_COMMAND").into());
            }
        }
        if version >= 2 && check.get("timeout_seconds").is_none() {
            return Err(ContractError::new(&field, "MISSING_FIELD").into());
        }
        if check
            .get("timeout_seconds")
            .is_some_and(|timeout| !integer_in(timeout, 1, 3600, false))
        {
            return Err(
                ContractError::new(&format!("{field}.timeout_seconds"), "INVALID_TIMEOUT").into(),
            );
        }
        command_ids.push(command);
    }
    Ok(command_ids)
}
pub fn validate_denials_and_delegation(value: &mut Value) -> ValidationResult<()> {
    let denials = value["denied_patterns"]
        .as_list()
        .ok_or_else(|| ContractError::new("denied_patterns", "ENGINE_DENIAL_REMOVED"))?;
    let unique = super::errors::set_values(denials)?;
    if unique.len() != denials.len()
        || !policy::DENIALS
            .iter()
            .all(|name| unique.iter().any(|item| item.is_text(name)))
    {
        return Err(ContractError::new("denied_patterns", "ENGINE_DENIAL_REMOVED").into());
    }
    if denials.iter().any(|item| {
        item.as_string()
            .is_none_or(|pattern| pattern.is_empty() || pattern.len() > 256)
    }) {
        return Err(ContractError::new("denied_patterns", "INVALID_PATTERN").into());
    }
    if let Some(config) =
        super::reviewer_delegation::validate_policy_config(&value["reviewer_delegation"], true)?
    {
        value.set("reviewer_delegation", config);
    }
    Ok(())
}

pub fn filesystem_target(value: &Value, denied: &Value) -> ValidationResult<PyString> {
    let target = contracts::filesystem_capability_target(value, &[])?;
    if target.starts_with("repo:") {
        for pattern in super::errors::iterable_values(denied)? {
            let pattern = pattern
                .as_string()
                .ok_or_else(|| attribute_error(&pattern, "lower"))?;
            contracts::filesystem_capability_target(value, std::slice::from_ref(pattern))?;
        }
    }
    Ok(target)
}
pub fn location_key(value: &Value, windows: bool) -> ValidationResult<PyString> {
    let mut text = value
        .as_string()
        .ok_or_else(|| attribute_error(value, "rstrip"))?
        .clone();
    while text.ends_with("/") {
        text.0.pop();
    }
    Ok(unicode::normcase(&text, windows))
}
pub fn under_configured_root(
    target: &Value,
    roots: &Value,
    windows: bool,
) -> ValidationResult<bool> {
    let text = target
        .as_string()
        .ok_or_else(|| attribute_error(target, "startswith"))?;
    if !text.starts_with("repo:") && !text.starts_with("runtime:") {
        return Ok(false);
    }
    let target_key = location_key(&Value::String(text.split_once(":").unwrap().1), windows)?;
    for root in super::errors::iterable_values(roots)? {
        let mut root_key = location_key(&root, windows)?;
        if target_key == root_key {
            return Ok(true);
        }
        root_key.0.push(if windows { 92 } else { 47 });
        if root.as_string().unwrap().ends_with("/") && target_key.0.starts_with(&root_key.0) {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn roots_overlap(left: &Value, right: &Value, windows: bool) -> ValidationResult<bool> {
    let left_key = location_key(left, windows)?;
    let right_key = location_key(right, windows)?;
    let separator = if windows { 92 } else { 47 };
    let mut left_prefix = left_key.clone();
    left_prefix.0.push(separator);
    let mut right_prefix = right_key.clone();
    right_prefix.0.push(separator);
    Ok(left_key == right_key
        || (left.as_string().unwrap().ends_with("/") && right_key.0.starts_with(&left_prefix.0))
        || (right.as_string().unwrap().ends_with("/") && left_key.0.starts_with(&right_prefix.0)))
}
pub fn has_location_duplicates(paths: &Value, windows: bool) -> ValidationResult<bool> {
    let mut keys = Vec::new();
    for path in super::errors::iterable_values(paths)? {
        keys.push(location_key(&path, windows)?);
    }
    Ok(keys
        .iter()
        .enumerate()
        .any(|(index, key)| keys[..index].contains(key)))
}
fn combine_roots(left: &Value, right: &Value) -> ValidationResult<Value> {
    use super::errors::python_type;
    use num_traits::ToPrimitive;
    match (left, right) {
        (Value::List(a), Value::List(b)) => {
            let mut values = a.clone();
            values.extend(b.clone());
            return Ok(Value::List(values));
        }
        (Value::String(a), Value::String(b)) => {
            let mut value = a.clone();
            value.0.extend(&b.0);
            return Ok(Value::String(value));
        }
        _ => (),
    }
    let integer = |value: &Value| match value {
        Value::Bool(value) => Some(BigInt::from(u8::from(*value))),
        Value::Integer(value) => Some(value.clone()),
        _ => None,
    };
    if let (Some(a), Some(b)) = (integer(left), integer(right)) {
        return Ok(Value::Integer(a + b));
    }
    let numeric = |value: &Value| {
        matches!(
            value,
            Value::Bool(_) | Value::Integer(_) | Value::Float(_) | Value::ConstantNaN
        )
    };
    if numeric(left) && numeric(right) {
        let floating = |value: &Value| -> ValidationResult<f64> {
            match value {
                Value::Float(value) => Ok(*value),
                Value::ConstantNaN => Ok(f64::NAN),
                _ => integer(value)
                    .unwrap()
                    .to_f64()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| {
                        ValidationFailure::python(
                            "OverflowError",
                            "int too large to convert to float",
                        )
                    }),
            }
        };
        return Ok(Value::Float(floating(left)? + floating(right)?));
    }
    let message = match left {
        Value::List(_) => format!(
            "can only concatenate list (not \"{}\") to list",
            python_type(right)
        ),
        Value::String(_) => format!(
            "can only concatenate str (not \"{}\") to str",
            python_type(right)
        ),
        _ => format!(
            "unsupported operand type(s) for +: '{}' and '{}'",
            python_type(left),
            python_type(right)
        ),
    };
    Err(ValidationFailure::python("TypeError", message))
}
pub fn role_capability_allowed(
    value: &Value,
    role: &Value,
    capability: &Value,
    windows: bool,
) -> ValidationResult<bool> {
    use super::errors::{container_equal, iterable_set, require_hashable, set_contains};
    let effect = string_item(capability, "effect")?;
    let action = string_item(capability, "action")?;
    let target = string_item(capability, "target_ref")?;
    require_hashable(role)?;
    let role_name = role.text().unwrap_or_default();
    let ceiling = policy::role_ceiling(&role_name);
    let version = string_item(value, "schema_version")?;
    if version.py_eq(&1_i64.into()) {
        for item in [effect, action, target] {
            require_hashable(item)?;
        }
        return Ok(ceiling.iter().any(|outer| {
            container_equal(effect, &outer["effect"])
                && container_equal(action, &outer["action"])
                && container_equal(target, &outer["target_ref"])
        }));
    }
    require_hashable(effect)?;
    require_hashable(action)?;
    if !ceiling.iter().any(|outer| {
        container_equal(effect, &outer["effect"]) && container_equal(action, &outer["action"])
    }) {
        return Ok(false);
    }
    if effect.is_text("command") {
        let checks = string_item(value, "required_checks")?;
        let checks = checks
            .as_object()
            .ok_or_else(|| attribute_error(checks, "values"))?;
        let mut targets = Vec::new();
        for (_, check) in checks {
            let command = string_item(check, "command_id")?;
            require_hashable(command)?;
            targets.push(command.clone());
        }
        let targets = iterable_set(&Value::List(targets))?;
        require_hashable(target)?;
        return Ok(set_contains(&targets, target));
    }
    if effect.as_string().unwrap().starts_with("filesystem") {
        let target_text = target
            .as_string()
            .ok_or_else(|| attribute_error(target, "startswith"))?;
        if version.py_eq(&3_i64.into()) && target_text.starts_with("runtime:") {
            return under_configured_root(
                target,
                string_item(string_item(value, "artifact_roots")?, "runtime")?,
                windows,
            );
        }
        if !target_text.starts_with("repo:") {
            return Ok(false);
        }
        if effect.is_text("filesystem_write") {
            if role_name == "senior_engineer" {
                let implementation = string_item(value, "implementation_roots")?;
                let extra = if version.py_eq(&2_i64.into()) {
                    string_item(string_item(value, "artifact_roots")?, "repo")?.clone()
                } else {
                    Value::List(vec![])
                };
                return under_configured_root(
                    target,
                    &combine_roots(implementation, &extra)?,
                    windows,
                );
            }
            return if version.py_eq(&2_i64.into()) && role_name == "tech_lead" {
                under_configured_root(
                    target,
                    string_item(string_item(value, "artifact_roots")?, "repo")?,
                    windows,
                )
            } else {
                Ok(false)
            };
        }
        return Ok(true);
    }
    for item in [effect, action, target] {
        require_hashable(item)?;
    }
    Ok(ceiling.iter().any(|outer| {
        container_equal(effect, &outer["effect"])
            && container_equal(action, &outer["action"])
            && container_equal(target, &outer["target_ref"])
    }))
}

pub fn branch_lease_seconds(value: &Value) -> ValidationResult<BigInt> {
    let policy = value
        .as_object()
        .ok_or_else(|| attribute_error(value, "get"))?;
    let limits = policy
        .iter()
        .find(|(key, _)| key.equals("limits"))
        .map(|(_, value)| value);
    let Some(limits) = limits else {
        return Ok(900.into());
    };
    let limits = limits
        .as_object()
        .ok_or_else(|| attribute_error(limits, "get"))?;
    let Some((_, lease)) = limits
        .iter()
        .find(|(key, _)| key.equals("branch_lease_seconds"))
    else {
        return Ok(900.into());
    };
    integer::coerce(lease).map_err(Into::into)
}
