use super::{
    contracts::{self, ContractError},
    errors::{self, ValidationFailure},
    hosts,
};
use crate::compat::{JsonError, PyString, Value};
use num_bigint::BigInt;

pub const LIMIT_KEYS: &[&str] = &[
    "children",
    "concurrency",
    "commands",
    "time_seconds",
    "output_tokens",
    "file_reads",
];
pub const ELIGIBLE_PARENTS: &[&str] = &[
    "tech_lead",
    "software_architect",
    "senior_engineer",
    "code_reviewer",
    "test_engineer",
    "security_reviewer",
];
pub const HOST_CAPABILITIES: &[&str] = &[
    "fresh_model_effort_selection",
    "filesystem_confinement",
    "tool_confinement",
    "command_confinement",
];
pub const CONFINEMENT_CAPABILITIES: &[&str] = &[
    "filesystem_confinement",
    "tool_confinement",
    "command_confinement",
];
pub const TERMINAL_STATES: &[&str] = &["succeeded", "failed", "blocked", "cancelled", "replaced"];
pub const MAX_RECORD_BYTES: usize = 256 * 1024;
#[derive(Debug)]
pub struct HelperRegisterError {
    pub code: String,
}
impl HelperRegisterError {
    pub fn new(code: &str) -> Self {
        Self { code: code.into() }
    }
}
#[derive(Debug)]
pub enum HelperFailure {
    Validation(ValidationFailure),
    Register(HelperRegisterError),
    Serialization(JsonError),
}
impl From<ContractError> for HelperFailure {
    fn from(error: ContractError) -> Self {
        Self::Validation(error.into())
    }
}
impl From<ValidationFailure> for HelperFailure {
    fn from(error: ValidationFailure) -> Self {
        Self::Validation(error)
    }
}
impl From<HelperRegisterError> for HelperFailure {
    fn from(error: HelperRegisterError) -> Self {
        Self::Register(error)
    }
}
impl From<JsonError> for HelperFailure {
    fn from(error: JsonError) -> Self {
        Self::Serialization(error)
    }
}
pub type HelperResult<T> = Result<T, HelperFailure>;
pub fn item<'a>(record: &'a Value, key: &str) -> HelperResult<&'a Value> {
    Ok(errors::string_item(record, key)?)
}
fn invalid(field: &str, code: &str) -> HelperFailure {
    ContractError::new(field, code).into()
}
fn object(record: &Value, field: &str) -> HelperResult<()> {
    if record.as_object().is_none() {
        Err(invalid(field, "INVALID_OBJECT"))
    } else {
        Ok(())
    }
}
fn keys(record: &Value, fields: &[&str], field: &str) -> HelperResult<()> {
    Ok(contracts::require_keys(record, fields, fields, field)?)
}
pub fn integer(value: &Value, field: &str, minimum: i64, maximum: i64) -> HelperResult<BigInt> {
    let Value::Integer(value) = value else {
        return Err(invalid(field, "INVALID_LIMIT"));
    };
    if value < &BigInt::from(minimum) || value > &BigInt::from(maximum) {
        return Err(invalid(field, "INVALID_LIMIT"));
    }
    Ok(value.clone())
}
pub fn unique_ids(value: &Value, field: &str, allow_empty: bool) -> HelperResult<Vec<String>> {
    let Some(values) = value
        .as_list()
        .filter(|values| allow_empty || !values.is_empty())
    else {
        return Err(invalid(field, "INVALID_LIST"));
    };
    let values = values
        .iter()
        .map(|value| contracts::opaque(value, field))
        .collect::<Result<Vec<_>, _>>()?;
    let unique = values.iter().collect::<std::collections::BTreeSet<_>>();
    if values.len() != unique.len() {
        return Err(invalid(field, "DUPLICATE_VALUE"));
    }
    Ok(values)
}
pub fn scope_refs(value: &Value, field: &str) -> HelperResult<Vec<PyString>> {
    let Some(values) = value.as_list().filter(|values| !values.is_empty()) else {
        return Err(invalid(field, "INVALID_LIST"));
    };
    let values = values
        .iter()
        .map(|value| contracts::validate_ref(value, field, false))
        .collect::<Result<Vec<_>, _>>()?;
    let unique = values.iter().collect::<std::collections::BTreeSet<_>>();
    if values.len() != unique.len() {
        return Err(invalid(field, "DUPLICATE_VALUE"));
    }
    if values.iter().any(|value| {
        !["repo:", "runtime:", "profile:software-engineering-graph/"]
            .iter()
            .any(|prefix| value.starts_with(prefix))
    }) {
        return Err(invalid(field, "FILESYSTEM_REF_REQUIRED"));
    }
    Ok(values)
}
pub fn limits(value: &Value, field: &str) -> HelperResult<Value> {
    object(value, field)?;
    keys(value, LIMIT_KEYS, field)?;
    let mut result = Value::object([]);
    for key in LIMIT_KEYS {
        result.set(
            key,
            Value::Integer(integer(
                item(value, key)?,
                &format!("{field}.{key}"),
                0,
                i32::MAX.into(),
            )?),
        );
    }
    if result["children"].as_i64().unwrap() < 1 || result["concurrency"].as_i64().unwrap() < 1 {
        return Err(invalid(field, "INVALID_LIMIT"));
    }
    Ok(result)
}
pub fn commands(value: &Value, field: &str) -> HelperResult<Value> {
    let Some(values) = value.as_list().filter(|values| values.len() <= 32) else {
        return Err(invalid(field, "INVALID_LIST"));
    };
    let mut result = Vec::new();
    let mut identifiers = std::collections::BTreeSet::new();
    for value in values {
        object(value, field)?;
        keys(value, &["command_id", "argv", "timeout_seconds"], field)?;
        let identifier =
            contracts::opaque(item(value, "command_id")?, &format!("{field}.command_id"))?;
        let arguments = item(value, "argv")?;
        if !arguments.as_list().is_some_and(|values| {
            !values.is_empty()
                && values.len() <= 32
                && values.iter().all(|value| {
                    value
                        .as_string()
                        .is_some_and(|text| !text.0.is_empty() && text.0.len() <= 1024)
                })
        }) {
            return Err(invalid(&format!("{field}.argv"), "INVALID_COMMAND"));
        }
        let timeout = integer(
            item(value, "timeout_seconds")?,
            &format!("{field}.timeout_seconds"),
            1,
            3600,
        )?;
        if !identifiers.insert(identifier.clone()) {
            return Err(invalid(field, "DUPLICATE_VALUE"));
        }
        result.push(Value::object([
            ("command_id", identifier.into()),
            ("argv", arguments.clone()),
            ("timeout_seconds", Value::Integer(timeout)),
        ]));
    }
    Ok(Value::List(result))
}
pub fn parent_capabilities(value: &Value, field: &str) -> HelperResult<Value> {
    let Some(values) = value.as_list().filter(|values| !values.is_empty()) else {
        return Err(invalid(field, "INVALID_LIST"));
    };
    let mut result: Vec<Value> = Vec::new();
    for value in values {
        object(value, field)?;
        keys(value, &["effect", "action", "target_ref"], field)?;
        let effect = item(value, "effect")?;
        let action = item(value, "action")?;
        errors::require_hashable(effect)?;
        errors::require_hashable(action)?;
        if !(effect.is_text("filesystem_read") && action.is_text("read")
            || effect.is_text("command") && action.is_text("run"))
        {
            return Err(invalid(field, "PARENT_CAPABILITY_FORBIDDEN"));
        }
        let target = if effect.is_text("filesystem_read") {
            Value::String(contracts::validate_ref(
                item(value, "target_ref")?,
                &format!("{field}.target_ref"),
                false,
            )?)
        } else {
            contracts::opaque(item(value, "target_ref")?, &format!("{field}.target_ref"))?.into()
        };
        let record = Value::object([
            ("effect", effect.clone()),
            ("action", action.clone()),
            ("target_ref", target),
        ]);
        if result.iter().any(|previous| previous.container_eq(&record)) {
            return Err(invalid(field, "DUPLICATE_VALUE"));
        }
        result.push(record);
    }
    result.sort_by_key(|record| {
        ["effect", "action", "target_ref"].map(|key| record[key].as_string().unwrap().clone())
    });
    Ok(Value::List(result))
}
pub fn ref_within(scope: &PyString, ceiling: &PyString) -> bool {
    scope == ceiling || ceiling.ends_with("/") && scope.0.starts_with(&ceiling.0)
}
pub fn trusted_observation_source(source: &PyString) -> bool {
    let lowered = crate::compat::unicode::lower(source);
    ![
        "caller",
        "prompt",
        "self-report",
        "self_report",
        "agent-report",
        "agent_report",
    ]
    .iter()
    .any(|marker| {
        lowered
            .0
            .windows(marker.len())
            .any(|part| part == marker.chars().map(u32::from).collect::<Vec<_>>())
    })
}
pub fn validate_test_mode(
    value: &Value,
    path_is_absolute: &dyn Fn(&PyString) -> bool,
) -> HelperResult<Value> {
    object(value, "allowance.test_mode")?;
    keys(
        value,
        &[
            "disposable_repository",
            "acknowledge_unenforced_isolation",
            "external_effects",
        ],
        "allowance.test_mode",
    )?;
    let repository = contracts::bounded_string(
        item(value, "disposable_repository")?,
        "test_mode.disposable_repository",
        4096,
    )?;
    if !path_is_absolute(&repository) {
        return Err(invalid(
            "test_mode.disposable_repository",
            "ABSOLUTE_PATH_REQUIRED",
        ));
    }
    if !item(value, "acknowledge_unenforced_isolation")?.is_true()
        || !item(value, "external_effects")?.is_text("mocked")
    {
        return Err(invalid(
            "allowance.test_mode",
            "TEST_ACKNOWLEDGMENT_REQUIRED",
        ));
    }
    Ok(value.clone())
}
pub fn validate_allowance(
    value: &Value,
    run_id: &Value,
    host: Option<&Value>,
    approved_parent: Option<&Value>,
    historical: bool,
    path_is_absolute: &dyn Fn(&PyString) -> bool,
) -> HelperResult<Value> {
    object(value, "allowance")?;
    let null = Value::Null;
    let version = value.get("schema_version").unwrap_or(&null);
    if !matches!(version, Value::Integer(_))
        || ![1_i64, 2]
            .iter()
            .any(|number| version.py_eq(&(*number).into()))
    {
        return Err(invalid("allowance.schema_version", "UNSUPPORTED_SCHEMA"));
    }
    let test_mode = version.py_eq(&2_i64.into());
    let mut fields = vec![
        "schema_version",
        "allowance_id",
        "run_id",
        "assignments",
        "shared_limits",
        "resources",
    ];
    if test_mode {
        fields.push("test_mode");
    }
    keys(value, &fields, "allowance")?;
    let mut result = Value::object([
        ("schema_version", version.clone()),
        (
            "allowance_id",
            contracts::opaque(item(value, "allowance_id")?, "allowance.allowance_id")?.into(),
        ),
        (
            "run_id",
            contracts::opaque(item(value, "run_id")?, "allowance.run_id")?.into(),
        ),
        (
            "shared_limits",
            limits(item(value, "shared_limits")?, "allowance.shared_limits")?,
        ),
    ]);
    if test_mode {
        result.set(
            "test_mode",
            validate_test_mode(item(value, "test_mode")?, path_is_absolute)?,
        );
    }
    if !result["run_id"].py_eq(run_id) {
        return Err(invalid("allowance", "PLAN_BINDING_MISMATCH"));
    }
    let Some(resources) = item(value, "resources")?
        .as_list()
        .filter(|values| !values.is_empty() && values.len() <= 64)
    else {
        return Err(invalid("allowance.resources", "INVALID_LIST"));
    };
    let mut resource_rows = Vec::new();
    let mut resource_keys = std::collections::BTreeSet::new();
    for resource in resources {
        object(resource, "allowance.resources")?;
        keys(resource, &["key", "capacity"], "allowance.resources")?;
        let key = contracts::opaque(item(resource, "key")?, "allowance.resources.key")?;
        let capacity = integer(
            item(resource, "capacity")?,
            "allowance.resources.capacity",
            1,
            1024,
        )?;
        if !resource_keys.insert(key.clone()) {
            return Err(invalid("allowance.resources", "DUPLICATE_VALUE"));
        }
        resource_rows.push(Value::object([
            ("key", key.into()),
            ("capacity", Value::Integer(capacity)),
        ]));
    }
    resource_rows.sort_by_key(|row| row["key"].text().unwrap());
    result.set("resources", Value::List(resource_rows));
    let Some(assignments) = item(value, "assignments")?
        .as_list()
        .filter(|values| !values.is_empty() && values.len() <= 32)
    else {
        return Err(invalid("allowance.assignments", "INVALID_LIST"));
    };
    let mut assignment_rows = Vec::new();
    let mut identifiers = std::collections::BTreeSet::new();
    for assignment in assignments {
        object(assignment, "allowance.assignments")?;
        keys(
            assignment,
            &[
                "assignment_id",
                "parent_role",
                "helper_role",
                "contract_revision",
                "model",
                "reasoning_effort",
                "parent_capabilities",
                "scope_refs",
                "commands",
                "checkpoint_policy",
                "resource_keys",
                "required_host_capabilities",
                "limits",
            ],
            "allowance.assignments",
        )?;
        let identifier = contracts::opaque(
            item(assignment, "assignment_id")?,
            "allowance.assignment_id",
        )?;
        let parent = contracts::opaque(item(assignment, "parent_role")?, "allowance.parent_role")?;
        let role = contracts::opaque(item(assignment, "helper_role")?, "allowance.helper_role")?;
        if identifiers.contains(&identifier) {
            return Err(invalid("allowance.assignments", "DUPLICATE_VALUE"));
        }
        if !ELIGIBLE_PARENTS.contains(&parent.as_str())
            || !["evidence_scout", "validation_executor"].contains(&role.as_str())
            || role == "validation_executor"
                && !["senior_engineer", "test_engineer"].contains(&parent.as_str())
        {
            return Err(invalid("allowance.assignments", "HELPER_ROLE_FORBIDDEN"));
        }
        if !item(assignment, "contract_revision")?.py_eq(&1_i64.into()) {
            return Err(invalid("allowance.contract_revision", "UNSUPPORTED_SCHEMA"));
        }
        let model =
            contracts::bounded_string(item(assignment, "model")?, "allowance.model", 128)?.clone();
        let effort = contracts::opaque(
            item(assignment, "reasoning_effort")?,
            "allowance.reasoning_effort",
        )?;
        if model.equals(hosts::MUSE_MODEL) {
            return Err(invalid(
                "allowance.assignments",
                "HELPER_RUNTIME_UNSUPPORTED",
            ));
        }
        if model.starts_with(hosts::RETIRED_MODEL_PREFIX) && !historical {
            return Err(invalid("allowance.assignments", "MODEL_RETIRED"));
        }
        if let Some(host) = host {
            errors::require_hashable(host)?;
            let host = host.text().unwrap_or_default();
            let model_text = model.utf8().unwrap_or_default();
            let revision = if historical {
                match host.as_str() {
                    "codex" | "codex-astra" => Some(7),
                    "cursor" | "claude" => Some(3),
                    _ => None,
                }
            } else {
                None
            };
            let admitted = hosts::selected_dispatch_model(&host, &model_text, &effort, revision)
                .or_else(|error| {
                    if historical {
                        hosts::selected_dispatch_model(&host, &model_text, &effort, None)
                    } else {
                        Err(error)
                    }
                });
            if admitted.is_err() {
                return Err(invalid(
                    "allowance.assignments",
                    "HELPER_ASSIGNMENT_MISMATCH",
                ));
            }
        }
        let commands = commands(item(assignment, "commands")?, "allowance.commands")?;
        if role == "evidence_scout" && commands.truthy() {
            return Err(invalid("allowance.commands", "COMMAND_FORBIDDEN"));
        }
        if role == "validation_executor" && !commands.truthy() {
            return Err(invalid("allowance.commands", "COMMAND_REQUIRED"));
        }
        let mut required = unique_ids(
            item(assignment, "required_host_capabilities")?,
            "allowance.required_host_capabilities",
            false,
        )?;
        let mut mandatory = vec![
            "fresh_model_effort_selection",
            "filesystem_confinement",
            "tool_confinement",
        ];
        if role == "validation_executor" {
            mandatory.push("command_confinement");
        }
        if required
            .iter()
            .any(|key| !HOST_CAPABILITIES.contains(&key.as_str()))
            || mandatory
                .iter()
                .any(|key| !required.iter().any(|value| value == key))
        {
            return Err(invalid(
                "allowance.required_host_capabilities",
                "HOST_CAPABILITY_REQUIRED",
            ));
        }
        let mut resource_ids = unique_ids(
            item(assignment, "resource_keys")?,
            "allowance.resource_keys",
            false,
        )?;
        if resource_ids.iter().any(|key| !resource_keys.contains(key)) {
            return Err(invalid("allowance.resource_keys", "UNKNOWN_RESOURCE"));
        }
        let limits = limits(item(assignment, "limits")?, "allowance.assignment_limits")?;
        if LIMIT_KEYS.iter().any(|key| {
            limits[key].as_i64().unwrap() > result["shared_limits"][key].as_i64().unwrap()
        }) {
            return Err(invalid(
                "allowance.assignment_limits",
                "LIMIT_MAY_NOT_INCREASE",
            ));
        }
        if !item(assignment, "checkpoint_policy")?.is_text("observed_repository_state") {
            return Err(invalid("allowance.checkpoint_policy", "UNKNOWN_VALUE"));
        }
        let capabilities = parent_capabilities(
            item(assignment, "parent_capabilities")?,
            "allowance.parent_capabilities",
        )?;
        if let Some(approved) = approved_parent {
            let Some(entries) = approved.as_object() else {
                return Err(errors::attribute_error(approved, "get").into());
            };
            let approved = entries
                .iter()
                .find(|(key, _)| key.equals(&parent))
                .map(|(_, value)| value);
            let approved_values = if let Some(approved) = approved {
                errors::iterable_values(approved)?
            } else {
                Vec::new()
            };
            let mut ceilings = Vec::new();
            for capability in approved_values {
                let effect = item(&capability, "effect")?;
                errors::require_hashable(effect)?;
                if effect.is_text("filesystem_read") || effect.is_text("command") {
                    let action = item(&capability, "action")?;
                    let target = item(&capability, "target_ref")?;
                    errors::require_hashable(effect)?;
                    errors::require_hashable(action)?;
                    errors::require_hashable(target)?;
                    ceilings.push(Value::object([
                        ("effect", effect.clone()),
                        ("action", action.clone()),
                        ("target_ref", target.clone()),
                    ]));
                }
            }
            if capabilities.as_list().unwrap().iter().any(|capability| {
                !ceilings
                    .iter()
                    .any(|ceiling| ceiling.container_eq(capability))
            }) {
                return Err(invalid(
                    "allowance.parent_capabilities",
                    "PARENT_AUTHORITY_EXCEEDED",
                ));
            }
        }
        let scopes = scope_refs(item(assignment, "scope_refs")?, "allowance.scope_refs")?;
        let ceilings = capabilities.as_list().unwrap();
        if scopes.iter().any(|scope| {
            !ceilings.iter().any(|capability| {
                capability["effect"].is_text("filesystem_read")
                    && ref_within(scope, capability["target_ref"].as_string().unwrap())
            })
        }) {
            return Err(invalid("allowance.scope_refs", "PARENT_AUTHORITY_EXCEEDED"));
        }
        if commands.as_list().unwrap().iter().any(|command| {
            !ceilings.iter().any(|capability| {
                capability["effect"].is_text("command")
                    && capability["target_ref"].py_eq(&command["command_id"])
            })
        }) {
            return Err(invalid("allowance.commands", "PARENT_AUTHORITY_EXCEEDED"));
        }
        resource_ids.sort();
        required.sort();
        identifiers.insert(identifier.clone());
        assignment_rows.push(Value::object([
            ("assignment_id", identifier.into()),
            ("parent_role", parent.into()),
            ("helper_role", role.into()),
            ("contract_revision", 1_i64.into()),
            ("model", Value::String(model)),
            ("reasoning_effort", effort.into()),
            ("parent_capabilities", capabilities),
            (
                "scope_refs",
                Value::list(scopes.into_iter().map(Value::String)),
            ),
            ("commands", commands),
            ("checkpoint_policy", "observed_repository_state".into()),
            (
                "resource_keys",
                Value::list(resource_ids.into_iter().map(Value::from)),
            ),
            (
                "required_host_capabilities",
                Value::list(required.into_iter().map(Value::from)),
            ),
            ("limits", limits),
        ]));
    }
    assignment_rows.sort_by_key(|row| row["assignment_id"].text().unwrap());
    result.set("assignments", Value::List(assignment_rows));
    Ok(result)
}
fn observation_status(value: &Value, field: &str) -> HelperResult<()> {
    errors::require_hashable(value)?;
    if !["verified", "unverified", "unavailable"]
        .iter()
        .any(|status| value.is_text(status))
    {
        return Err(invalid(field, "UNKNOWN_VALUE"));
    }
    Ok(())
}
pub fn validate_host_observation(value: &Value) -> HelperResult<Value> {
    object(value, "host_observation")?;
    keys(
        value,
        &[
            "schema_version",
            "host_id",
            "observed_at",
            "source",
            "uncertainty",
            "capabilities",
            "supported_assignments",
        ],
        "host_observation",
    )?;
    if !item(value, "schema_version")?.py_eq(&1_i64.into()) {
        return Err(invalid(
            "host_observation.schema_version",
            "UNSUPPORTED_SCHEMA",
        ));
    }
    let capabilities = item(value, "capabilities")?;
    let Some(entries) = capabilities
        .as_object()
        .filter(|entries| !entries.is_empty())
    else {
        return Err(invalid("host_observation.capabilities", "INVALID_OBJECT"));
    };
    if entries
        .iter()
        .any(|(key, _)| !HOST_CAPABILITIES.iter().any(|name| key.equals(name)))
    {
        return Err(invalid("host_observation.capabilities", "UNKNOWN_FIELD"));
    }
    let mut normalized = Value::object([]);
    for (key, capability) in entries {
        object(capability, "host_observation.capabilities")?;
        let key = key.utf8().unwrap();
        keys(capability, &["status", "source", "uncertainty"], &key)?;
        observation_status(
            item(capability, "status")?,
            "host_observation.capabilities.status",
        )?;
        normalized.set(
            &key,
            Value::object([
                ("status", item(capability, "status")?.clone()),
                (
                    "source",
                    Value::String(
                        contracts::bounded_string(
                            item(capability, "source")?,
                            &format!("{key}.source"),
                            256,
                        )?
                        .clone(),
                    ),
                ),
                (
                    "uncertainty",
                    Value::String(
                        contracts::bounded_string(
                            item(capability, "uncertainty")?,
                            &format!("{key}.uncertainty"),
                            1024,
                        )?
                        .clone(),
                    ),
                ),
            ]),
        );
    }
    let Some(assignments) = item(value, "supported_assignments")?
        .as_list()
        .filter(|values| !values.is_empty() && values.len() <= 32)
    else {
        return Err(invalid(
            "host_observation.supported_assignments",
            "INVALID_LIST",
        ));
    };
    let mut normalized_assignments = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for assignment in assignments {
        object(assignment, "host_observation.supported_assignments")?;
        keys(
            assignment,
            &[
                "model",
                "reasoning_effort",
                "status",
                "source",
                "uncertainty",
            ],
            "host_observation.supported_assignments",
        )?;
        let model =
            contracts::bounded_string(item(assignment, "model")?, "host_observation.model", 128)?
                .clone();
        let effort = contracts::opaque(
            item(assignment, "reasoning_effort")?,
            "host_observation.reasoning_effort",
        )?;
        observation_status(
            item(assignment, "status")?,
            "host_observation.assignment.status",
        )?;
        if !seen.insert((model.clone(), effort.clone())) {
            return Err(invalid(
                "host_observation.supported_assignments",
                "DUPLICATE_VALUE",
            ));
        }
        normalized_assignments.push(Value::object([
            ("model", Value::String(model)),
            ("reasoning_effort", effort.into()),
            ("status", item(assignment, "status")?.clone()),
            (
                "source",
                Value::String(
                    contracts::bounded_string(
                        item(assignment, "source")?,
                        "host_observation.assignment.source",
                        256,
                    )?
                    .clone(),
                ),
            ),
            (
                "uncertainty",
                Value::String(
                    contracts::bounded_string(
                        item(assignment, "uncertainty")?,
                        "host_observation.assignment.uncertainty",
                        1024,
                    )?
                    .clone(),
                ),
            ),
        ]));
    }
    normalized_assignments.sort_by_key(|record| {
        ["model", "reasoning_effort"].map(|key| record[key].as_string().unwrap().clone())
    });
    Ok(Value::object([
        ("schema_version", 1_i64.into()),
        (
            "host_id",
            contracts::opaque(item(value, "host_id")?, "host_observation.host_id")?.into(),
        ),
        (
            "observed_at",
            Value::String(
                contracts::bounded_string(
                    item(value, "observed_at")?,
                    "host_observation.observed_at",
                    64,
                )?
                .clone(),
            ),
        ),
        (
            "source",
            Value::String(
                contracts::bounded_string(item(value, "source")?, "host_observation.source", 256)?
                    .clone(),
            ),
        ),
        (
            "uncertainty",
            Value::String(
                contracts::bounded_string(
                    item(value, "uncertainty")?,
                    "host_observation.uncertainty",
                    1024,
                )?
                .clone(),
            ),
        ),
        ("capabilities", normalized),
        ("supported_assignments", Value::List(normalized_assignments)),
    ]))
}
