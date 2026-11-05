use super::{
    error::{EngineError, EngineResult},
    policy::PolicyFilePort,
};
use crate::{
    compat::Value,
    domain::{
        contracts::{self, ContractError},
        errors, helper_register as helper, hosts, ids,
    },
};

pub fn item<'a>(record: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(record, key)?)
}
fn invalid(field: &str, code: &str) -> EngineError {
    ContractError::new(field, code).into()
}
fn object(value: &Value, field: &str) -> EngineResult<()> {
    if value.as_object().is_none() {
        Err(invalid(field, "INVALID_OBJECT"))
    } else {
        Ok(())
    }
}
fn keys(value: &Value, fields: &[&str], field: &str) -> EngineResult<()> {
    Ok(contracts::require_keys(value, fields, fields, field)?)
}
pub fn validate_plan(value: &Value, run_id: &Value) -> EngineResult<Value> {
    if value.as_object().is_none() || !value["schema_version"].py_eq(&3_i64.into()) {
        return Err(invalid("execution_plan", "HELPER_PLAN_V3_REQUIRED"));
    }
    if !value["run_id"].py_eq(run_id) {
        return Err(invalid("execution_plan", "PLAN_BINDING_MISMATCH"));
    }
    if !hosts::HOSTS.iter().any(|host| value["host"].is_text(host)) {
        return Err(invalid("execution_plan.host", "HOST_UNSUPPORTED"));
    }
    let digest = contracts::digest(&value["plan_digest"], "execution_plan.plan_digest")?;
    let mut unsigned = value.clone();
    unsigned.remove("plan_digest");
    if ids::sha256_bytes(&ids::canonical_bytes(&unsigned)?) != digest {
        return Err(invalid(
            "execution_plan.plan_digest",
            "INPUT_DIGEST_MISMATCH",
        ));
    }
    let attachment = &value["helper_allowance"];
    if attachment.as_object().is_none() {
        return Err(invalid("execution_plan.helper_allowance", "MISSING_FIELD"));
    }
    keys(
        attachment,
        &["ref", "sha256"],
        "execution_plan.helper_allowance",
    )?;
    let reference = contracts::validate_ref(
        item(attachment, "ref")?,
        "execution_plan.helper_allowance.ref",
        false,
    )?;
    if !(reference.starts_with("repo:") || reference.starts_with("runtime:"))
        || reference.0.contains(&35)
        || !reference.ends_with(".json")
    {
        return Err(invalid(
            "execution_plan.helper_allowance.ref",
            "FILESYSTEM_REF_REQUIRED",
        ));
    }
    contracts::digest(
        item(attachment, "sha256")?,
        "execution_plan.helper_allowance.sha256",
    )?;
    Ok(value.clone())
}
pub fn filesystem_identity(
    value: &Value,
    field: &str,
    files: &dyn PolicyFilePort,
) -> EngineResult<Value> {
    object(value, field)?;
    keys(value, &["device", "inode", "path"], field)?;
    let path =
        contracts::bounded_string(item(value, "path")?, &format!("{field}.path"), 4096)?.clone();
    if !files.from_string(&path)?.is_absolute() {
        return Err(invalid(&format!("{field}.path"), "ABSOLUTE_PATH_REQUIRED"));
    }
    Ok(Value::object([
        (
            "device",
            Value::Integer(helper::integer(
                item(value, "device")?,
                &format!("{field}.device"),
                0,
                i64::MAX,
            )?),
        ),
        (
            "inode",
            Value::Integer(helper::integer(
                item(value, "inode")?,
                &format!("{field}.inode"),
                0,
                i64::MAX,
            )?),
        ),
        ("path", Value::String(path)),
    ]))
}
pub fn register_key(
    identity: &Value,
    repository_digest: &Value,
    run_id: &Value,
) -> EngineResult<String> {
    Ok(ids::sha256_bytes(&ids::canonical_bytes(&Value::object([
        ("state_root_identity", identity.clone()),
        ("repository_digest", repository_digest.clone()),
        ("run_id", run_id.clone()),
    ]))?))
}
pub fn validate_context(value: &Value, files: &dyn PolicyFilePort) -> EngineResult<Value> {
    object(value, "context")?;
    keys(
        value,
        &[
            "schema_version",
            "register_id",
            "register_path",
            "state_root_identity",
            "repository_digest",
            "run_id",
            "plan_digest",
            "allowance_ref",
            "allowance_digest",
            "allowance_payload_digest",
            "host_observation_digest",
            "host_observation_payload_digest",
        ],
        "context",
    )?;
    if !item(value, "schema_version")?.py_eq(&1_i64.into()) {
        return Err(invalid("context.schema_version", "UNSUPPORTED_SCHEMA"));
    }
    let register_path =
        contracts::bounded_string(item(value, "register_path")?, "context.register_path", 4096)?
            .clone();
    if !files.from_string(&register_path)?.is_absolute() {
        return Err(invalid("context.register_path", "ABSOLUTE_PATH_REQUIRED"));
    }
    let result = Value::object([
        ("schema_version", 1_i64.into()),
        (
            "register_id",
            contracts::digest(item(value, "register_id")?, "context.register_id")?.into(),
        ),
        ("register_path", Value::String(register_path)),
        (
            "state_root_identity",
            filesystem_identity(
                item(value, "state_root_identity")?,
                "context.state_root_identity",
                files,
            )?,
        ),
        (
            "repository_digest",
            contracts::digest(
                item(value, "repository_digest")?,
                "context.repository_digest",
            )?
            .into(),
        ),
        (
            "run_id",
            contracts::opaque(item(value, "run_id")?, "context.run_id")?.into(),
        ),
        (
            "plan_digest",
            contracts::digest(item(value, "plan_digest")?, "context.plan_digest")?.into(),
        ),
        (
            "allowance_ref",
            Value::String(contracts::validate_ref(
                item(value, "allowance_ref")?,
                "context.allowance_ref",
                false,
            )?),
        ),
        (
            "allowance_digest",
            contracts::digest(item(value, "allowance_digest")?, "context.allowance_digest")?.into(),
        ),
        (
            "allowance_payload_digest",
            contracts::digest(
                item(value, "allowance_payload_digest")?,
                "context.allowance_payload_digest",
            )?
            .into(),
        ),
        (
            "host_observation_digest",
            contracts::digest(
                item(value, "host_observation_digest")?,
                "context.host_observation_digest",
            )?
            .into(),
        ),
        (
            "host_observation_payload_digest",
            contracts::digest(
                item(value, "host_observation_payload_digest")?,
                "context.host_observation_payload_digest",
            )?
            .into(),
        ),
    ]);
    if !result["register_id"].is_text(&register_key(
        &result["state_root_identity"],
        &result["repository_digest"],
        &result["run_id"],
    )?) {
        return Err(invalid("context.register_id", "REGISTER_BINDING_MISMATCH"));
    }
    let expected = files
        .from_string(result["state_root_identity"]["path"].as_string().unwrap())?
        .join("helper-registers")
        .join(result["register_id"].text().unwrap())
        .join("register.json");
    if !files.same_path(
        &files.from_string(result["register_path"].as_string().unwrap())?,
        &expected,
    ) {
        return Err(invalid(
            "context.register_path",
            "REGISTER_BINDING_MISMATCH",
        ));
    }
    Ok(result)
}
pub fn validate_request(value: &Value) -> EngineResult<Value> {
    object(value, "request")?;
    keys(
        value,
        &[
            "schema_version",
            "request_id",
            "assignment_id",
            "parent_role",
            "helper_role",
            "contract_revision",
            "model",
            "reasoning_effort",
            "scope_refs",
            "commands",
            "checkpoint_ref",
            "resource_keys",
            "budgets",
        ],
        "request",
    )?;
    let budgets = item(value, "budgets")?;
    object(budgets, "request.budgets")?;
    keys(
        budgets,
        &["time_seconds", "output_tokens", "file_reads"],
        "request.budgets",
    )?;
    if !item(value, "schema_version")?.py_eq(&1_i64.into()) {
        return Err(invalid("request.schema_version", "UNSUPPORTED_SCHEMA"));
    }
    let identifier = contracts::opaque(item(value, "request_id")?, "request.request_id")?;
    let assignment = contracts::opaque(item(value, "assignment_id")?, "request.assignment_id")?;
    let parent = contracts::opaque(item(value, "parent_role")?, "request.parent_role")?;
    let role = contracts::opaque(item(value, "helper_role")?, "request.helper_role")?;
    let revision = helper::integer(
        item(value, "contract_revision")?,
        "request.contract_revision",
        1,
        i32::MAX.into(),
    )?;
    let model = contracts::bounded_string(item(value, "model")?, "request.model", 128)?.clone();
    let effort = contracts::opaque(item(value, "reasoning_effort")?, "request.reasoning_effort")?;
    let scopes = helper::scope_refs(item(value, "scope_refs")?, "request.scope_refs")?;
    let commands = helper::commands(item(value, "commands")?, "request.commands")?;
    let checkpoint = contracts::validate_ref(
        item(value, "checkpoint_ref")?,
        "request.checkpoint_ref",
        true,
    )?;
    let mut resources =
        helper::unique_ids(item(value, "resource_keys")?, "request.resource_keys", true)?;
    resources.sort();
    let mut normalized_budgets = Value::object([]);
    for key in ["time_seconds", "output_tokens", "file_reads"] {
        normalized_budgets.set(
            key,
            Value::Integer(helper::integer(
                item(budgets, key)?,
                &format!("request.budgets.{key}"),
                0,
                i32::MAX.into(),
            )?),
        );
    }
    Ok(Value::object([
        ("schema_version", 1_i64.into()),
        ("request_id", identifier.into()),
        ("assignment_id", assignment.into()),
        ("parent_role", parent.into()),
        ("helper_role", role.into()),
        ("contract_revision", Value::Integer(revision)),
        ("model", Value::String(model)),
        ("reasoning_effort", effort.into()),
        (
            "scope_refs",
            Value::list(scopes.into_iter().map(Value::String)),
        ),
        ("commands", commands),
        ("checkpoint_ref", Value::String(checkpoint)),
        (
            "resource_keys",
            Value::list(resources.into_iter().map(Value::from)),
        ),
        ("budgets", normalized_budgets),
    ]))
}
pub fn assignment_for<'a>(allowance: &'a Value, identifier: &Value) -> EngineResult<&'a Value> {
    let assignments = item(allowance, "assignments")?;
    let Some(assignments) = assignments.as_list() else {
        for assignment in errors::iterable_values(assignments)? {
            item(&assignment, "assignment_id")?;
        }
        return Err(invalid("request.assignment_id", "ASSIGNMENT_NOT_APPROVED"));
    };
    for assignment in assignments {
        if item(assignment, "assignment_id")?.py_eq(identifier) {
            return Ok(assignment);
        }
    }
    Err(invalid("request.assignment_id", "ASSIGNMENT_NOT_APPROVED"))
}
pub fn required_amounts(request: &Value) -> EngineResult<Value> {
    let commands = item(request, "commands")?;
    let length = match commands {
        Value::List(values) => values.len(),
        Value::Object(values) => values.len(),
        Value::String(value) => value.0.len(),
        Value::Bytes(values) => values.len(),
        _ => {
            return Err(EngineError::python(
                "TypeError",
                format!("object of type '{}' has no len()", commands.python_type()),
            ))
        }
    };
    Ok(Value::object([
        ("children", 1_i64.into()),
        ("concurrency", 1_i64.into()),
        ("commands", Value::Integer(length.into())),
        (
            "time_seconds",
            item(item(request, "budgets")?, "time_seconds")?.clone(),
        ),
        (
            "output_tokens",
            item(item(request, "budgets")?, "output_tokens")?.clone(),
        ),
        (
            "file_reads",
            item(item(request, "budgets")?, "file_reads")?.clone(),
        ),
    ]))
}
pub fn validate_settlement(value: &Value) -> EngineResult<Value> {
    object(value, "settlement")?;
    keys(
        value,
        &[
            "schema_version",
            "request_id",
            "request_digest",
            "terminal_state",
            "evidence_refs",
            "uncertainty",
            "session_identity_sha256",
            "usage_ref",
        ],
        "settlement",
    )?;
    let state = item(value, "terminal_state")?;
    if !item(value, "schema_version")?.py_eq(&1_i64.into()) {
        return Err(invalid("settlement", "INVALID_TERMINAL_STATE"));
    }
    errors::require_hashable(state)?;
    if !helper::TERMINAL_STATES
        .iter()
        .any(|name| state.is_text(name))
    {
        return Err(invalid("settlement", "INVALID_TERMINAL_STATE"));
    }
    let Some(evidence) = item(value, "evidence_refs")?
        .as_list()
        .filter(|values| values.len() <= 64)
    else {
        return Err(invalid("settlement.evidence_refs", "INVALID_LIST"));
    };
    let mut evidence = evidence
        .iter()
        .map(|value| contracts::validate_ref(value, "settlement.evidence_refs", false))
        .collect::<Result<Vec<_>, _>>()?;
    evidence.sort();
    if evidence.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(invalid("settlement.evidence_refs", "DUPLICATE_VALUE"));
    }
    Ok(Value::object([
        ("schema_version", 1_i64.into()),
        (
            "request_id",
            contracts::opaque(item(value, "request_id")?, "settlement.request_id")?.into(),
        ),
        (
            "request_digest",
            contracts::digest(item(value, "request_digest")?, "settlement.request_digest")?.into(),
        ),
        ("terminal_state", state.clone()),
        (
            "evidence_refs",
            Value::list(evidence.into_iter().map(Value::String)),
        ),
        (
            "uncertainty",
            Value::String(
                contracts::bounded_string(
                    item(value, "uncertainty")?,
                    "settlement.uncertainty",
                    2048,
                )?
                .clone(),
            ),
        ),
        (
            "session_identity_sha256",
            contracts::digest(
                item(value, "session_identity_sha256")?,
                "settlement.session_identity_sha256",
            )?
            .into(),
        ),
        (
            "usage_ref",
            if matches!(item(value, "usage_ref")?, Value::Null) {
                Value::Null
            } else {
                Value::String(contracts::validate_ref(
                    item(value, "usage_ref")?,
                    "settlement.usage_ref",
                    false,
                )?)
            },
        ),
    ]))
}
fn zero_limits() -> Value {
    Value::Object(
        helper::LIMIT_KEYS
            .iter()
            .map(|key| ((*key).into(), 0_i64.into()))
            .collect(),
    )
}
fn increment(record: &mut Value, key: &str, amount: &num_bigint::BigInt) {
    let current = record[key].strict_integer().unwrap().clone();
    record.set(key, Value::Integer(current + amount));
}
pub fn check_accounting(value: &Value, allowance: &Value) -> EngineResult<()> {
    let resources = item(allowance, "resources")?.as_list().unwrap();
    let assignments = item(allowance, "assignments")?.as_list().unwrap();
    let mut usage = zero_limits();
    let mut active = 0_i64;
    let mut active_resources = Value::Object(
        resources
            .iter()
            .map(|record| (record["key"].as_string().unwrap().clone(), 0_i64.into()))
            .collect(),
    );
    let mut assignment_usage = Value::Object(
        assignments
            .iter()
            .map(|record| {
                (
                    record["assignment_id"].as_string().unwrap().clone(),
                    zero_limits(),
                )
            })
            .collect(),
    );
    let reservations = item(value, "reservations")?;
    let Some(reservations) = reservations.as_object() else {
        return Err(errors::attribute_error(reservations, "items").into());
    };
    for (identifier, reservation) in reservations {
        contracts::opaque(&Value::String(identifier.clone()), "register.request_id")?;
        object(reservation, "register.reservations")?;
        contracts::require_keys(
            reservation,
            &[
                "request_digest",
                "assignment_id",
                "status",
                "reservation",
                "resource_keys",
                "settlement",
            ],
            &[
                "request_digest",
                "assignment_id",
                "status",
                "reservation",
                "resource_keys",
                "settlement",
                "settlement_digest",
            ],
            "register.reservations",
        )?;
        contracts::digest(
            item(reservation, "request_digest")?,
            "register.request_digest",
        )?;
        let assignment = contracts::opaque(
            item(reservation, "assignment_id")?,
            "register.assignment_id",
        )?;
        if !assignments
            .iter()
            .any(|record| record["assignment_id"].is_text(&assignment))
        {
            return Err(invalid("register.assignment_id", "ASSIGNMENT_NOT_APPROVED"));
        }
        let amounts = item(reservation, "reservation")?;
        object(amounts, "register.reservation")?;
        keys(amounts, helper::LIMIT_KEYS, "register.reservation")?;
        let mut assigned = assignment_usage[&assignment].clone();
        for key in helper::LIMIT_KEYS {
            let amount = helper::integer(
                item(amounts, key)?,
                &format!("register.reservation.{key}"),
                0,
                i32::MAX.into(),
            )?;
            if *key != "concurrency" {
                increment(&mut usage, key, &amount);
            }
            increment(&mut assigned, key, &amount);
        }
        let keys = helper::unique_ids(
            item(reservation, "resource_keys")?,
            "register.resource_keys",
            true,
        )?;
        if keys
            .iter()
            .any(|key| !resources.iter().any(|record| record["key"].is_text(key)))
        {
            return Err(invalid("register.resource_keys", "UNKNOWN_RESOURCE"));
        }
        if item(reservation, "status")?.is_text("active") {
            if !matches!(item(reservation, "settlement")?, Value::Null)
                || reservation.get("settlement_digest").is_some()
            {
                return Err(invalid("register.reservations", "SETTLEMENT_CONFLICT"));
            }
            active += 1;
            for key in keys {
                increment(&mut active_resources, &key, &1_i64.into());
            }
        } else {
            let status = item(reservation, "status")?;
            errors::require_hashable(status)?;
            if !helper::TERMINAL_STATES
                .iter()
                .any(|name| status.is_text(name))
                || matches!(item(reservation, "settlement")?, Value::Null)
            {
                return Err(invalid("register.reservations", "INVALID_TERMINAL_STATE"));
            }
            contracts::digest(
                reservation.get("settlement_digest").unwrap_or(&Value::Null),
                "register.settlement_digest",
            )?;
            increment(
                &mut assigned,
                "concurrency",
                &-amounts["concurrency"].strict_integer().unwrap(),
            );
        }
        assignment_usage.set(&assignment, assigned);
    }
    if !usage.container_eq(item(value, "usage")?)
        || !Value::from(active).py_eq(item(value, "active_concurrency")?)
        || !active_resources.container_eq(item(value, "active_resources")?)
        || !assignment_usage.container_eq(item(value, "assignment_usage")?)
    {
        return Err(invalid("register", "REGISTER_ACCOUNTING_MISMATCH"));
    }
    Ok(())
}
pub fn repository_key(identity: &Value) -> EngineResult<String> {
    let mut text = crate::compat::PyString::from(format!(
        "{}\0{}\0",
        identity["device"].strict_integer().unwrap(),
        identity["inode"].strict_integer().unwrap()
    ));
    text.0
        .extend(identity["path"].as_string().unwrap().0.iter().copied());
    let mut bytes = b"graph-repository-v1\0".to_vec();
    bytes.extend(crate::compat::encode_utf8(&text)?);
    Ok(ids::sha256_bytes(&bytes))
}
pub fn validate_record(value: &Value, files: &dyn PolicyFilePort) -> EngineResult<Value> {
    let invalid_record = || EngineError::from(helper::HelperRegisterError::new("REGISTER_INVALID"));
    if value.as_object().is_none() {
        return Err(invalid_record());
    }
    let result = (|| {
        keys(
            value,
            &[
                "schema_version",
                "context",
                "host",
                "repository_identity",
                "allowance_ref",
                "allowance",
                "host_observation",
                "usage",
                "active_concurrency",
                "active_resources",
                "assignment_usage",
                "reservations",
            ],
            "register",
        )?;
        if !item(value, "schema_version")?.py_eq(&1_i64.into()) {
            return Err(invalid("register.schema_version", "UNSUPPORTED_SCHEMA"));
        }
        let context = validate_context(item(value, "context")?, files)?;
        if !item(value, "context")?.container_eq(&context) {
            return Err(invalid("register.context", "REGISTER_BINDING_MISMATCH"));
        }
        let identity = filesystem_identity(
            item(value, "repository_identity")?,
            "repository_identity",
            files,
        )?;
        if !item(value, "repository_identity")?.container_eq(&identity)
            || !context["repository_digest"].is_text(&repository_key(&identity)?)
        {
            return Err(invalid("repository_identity", "REGISTER_BINDING_MISMATCH"));
        }
        let allowance_ref =
            contracts::validate_ref(item(value, "allowance_ref")?, "allowance_ref", false)?;
        if !Value::String(allowance_ref).py_eq(&context["allowance_ref"]) {
            return Err(invalid("allowance_ref", "REGISTER_BINDING_MISMATCH"));
        }
        let host = item(value, "host")?;
        if !hosts::HOSTS.iter().any(|name| host.is_text(name)) {
            return Err(invalid("register.host", "HOST_UNSUPPORTED"));
        }
        let absolute = |text: &crate::compat::PyString| {
            files.from_string(text).is_ok_and(|path| path.is_absolute())
        };
        let allowance = helper::validate_allowance(
            item(value, "allowance")?,
            &context["run_id"],
            Some(host),
            None,
            true,
            &absolute,
        )?;
        let observation = helper::validate_host_observation(item(value, "host_observation")?)?;
        if !item(value, "allowance")?.container_eq(&allowance)
            || !item(value, "host_observation")?.container_eq(&observation)
        {
            return Err(invalid("register", "REGISTER_BINDING_MISMATCH"));
        }
        if !context["allowance_payload_digest"]
            .is_text(&ids::sha256_bytes(&ids::canonical_bytes(&allowance)?))
        {
            return Err(invalid("register.allowance", "REGISTER_BINDING_MISMATCH"));
        }
        if !context["host_observation_payload_digest"]
            .is_text(&ids::sha256_bytes(&ids::canonical_bytes(&observation)?))
        {
            return Err(invalid(
                "register.host_observation",
                "REGISTER_BINDING_MISMATCH",
            ));
        }
        let usage = item(value, "usage")?;
        keys(usage, helper::LIMIT_KEYS, "register.usage")?;
        for key in helper::LIMIT_KEYS {
            helper::integer(
                item(usage, key)?,
                &format!("register.usage.{key}"),
                0,
                i32::MAX.into(),
            )?;
        }
        helper::integer(
            item(value, "active_concurrency")?,
            "register.active_concurrency",
            0,
            i32::MAX.into(),
        )?;
        let resources = allowance["resources"].as_list().unwrap();
        let resource_keys = resources
            .iter()
            .map(|row| row["key"].text().unwrap())
            .collect::<Vec<_>>();
        let resource_names = resource_keys.iter().map(String::as_str).collect::<Vec<_>>();
        let active_resources = item(value, "active_resources")?;
        keys(
            active_resources,
            &resource_names,
            "register.active_resources",
        )?;
        for (key, amount) in active_resources.as_object().unwrap() {
            let key = key.utf8().unwrap();
            let amount = helper::integer(
                amount,
                &format!("register.active_resources.{key}"),
                0,
                i32::MAX.into(),
            )?;
            let capacity = resources
                .iter()
                .find(|row| row["key"].is_text(&key))
                .unwrap()["capacity"]
                .strict_integer()
                .unwrap();
            if &amount > capacity {
                return Err(invalid(
                    "register.active_resources",
                    "RESOURCE_CAPACITY_EXCEEDED",
                ));
            }
        }
        let assignments = allowance["assignments"].as_list().unwrap();
        let assignment_keys = assignments
            .iter()
            .map(|row| row["assignment_id"].text().unwrap())
            .collect::<Vec<_>>();
        let assignment_names = assignment_keys
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let assignment_usage = item(value, "assignment_usage")?;
        keys(
            assignment_usage,
            &assignment_names,
            "register.assignment_usage",
        )?;
        for (_, amounts) in assignment_usage.as_object().unwrap() {
            keys(amounts, helper::LIMIT_KEYS, "register.assignment_usage")?;
            for key in helper::LIMIT_KEYS {
                helper::integer(
                    item(amounts, key)?,
                    &format!("register.assignment_usage.{key}"),
                    0,
                    i32::MAX.into(),
                )?;
            }
        }
        if !item(value, "reservations")?
            .as_object()
            .is_some_and(|entries| entries.len() <= 4096)
        {
            return Err(invalid("register.reservations", "INVALID_OBJECT"));
        }
        check_accounting(value, &allowance)?;
        Ok(value.clone())
    })();
    result.map_err(|error| match &error {
        EngineError::Contract(_) => invalid_record(),
        EngineError::Python {
            kind:
                "AttributeError" | "KeyError" | "TypeError" | "ValueError" | "UnicodeEncodeError"
                | "UnicodeDecodeError",
            ..
        } => invalid_record(),
        EngineError::Json(error)
            if matches!(
                error.kind,
                "ValueError"
                    | "TypeError"
                    | "UnicodeEncodeError"
                    | "UnicodeDecodeError"
                    | "JSONDecodeError"
            ) =>
        {
            invalid_record()
        }
        _ => error,
    })
}
