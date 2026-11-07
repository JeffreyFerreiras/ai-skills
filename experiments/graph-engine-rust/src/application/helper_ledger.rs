use super::{
    error::{EngineError, EngineResult},
    helper_repository::{extend, test_mode_metadata, HelperRegisterPersistence},
    helper_validation as validation,
    policy::PolicyFilePort,
    snapshots::SnapshotFailure,
};
use crate::{
    compat::Value,
    domain::{contracts::ContractError, helper_register as helper, ids},
};
use num_bigint::BigInt;
use std::path::Path;

fn contract(field: &str, code: &str) -> EngineError {
    ContractError::new(field, code).into()
}
fn failure(code: &str) -> EngineError {
    helper::HelperRegisterError::new(code).into()
}
fn snapshot_error(error: SnapshotFailure) -> EngineError {
    match error {
        SnapshotFailure::Contract(error) => error.into(),
        SnapshotFailure::Io(error) => error.into(),
        SnapshotFailure::Json(error) => error.into(),
    }
}
pub struct HelperReservationLedger<'a> {
    pub repository: &'a dyn HelperRegisterPersistence,
    pub files: &'a dyn PolicyFilePort,
}
struct Preflight {
    request: Value,
    amounts: Value,
    missing: Vec<String>,
}
impl HelperReservationLedger<'_> {
    fn verify_checkpoint(&self, record: &Value, request: &Value) -> EngineResult<()> {
        let checkpoint = request["checkpoint_ref"].as_string().unwrap();
        if !checkpoint.starts_with("repo:") && !checkpoint.starts_with("runtime:") {
            return Err(contract(
                "request.checkpoint_ref",
                "FILESYSTEM_REF_REQUIRED",
            ));
        }
        let colon = checkpoint.0.iter().position(|point| *point == 58).unwrap();
        let prefix = crate::compat::PyString(checkpoint.0[..colon].to_vec());
        let body = &checkpoint.0[colon + 1..];
        let marker = crate::compat::PyString::from("#sha256=");
        let Some(index) = body
            .windows(marker.0.len())
            .position(|part| part == marker.0)
        else {
            return Err(contract(
                "request.checkpoint_ref",
                "CONTENT_DIGEST_REQUIRED",
            ));
        };
        let relative = crate::domain::contracts::lexical_relative(
            &Value::String(crate::compat::PyString(body[..index].to_vec())),
            "request.checkpoint_ref",
        )?;
        let digest = crate::compat::PyString(body[index + marker.0.len()..].to_vec());
        let mut approved = false;
        let mut prefix_with_colon = prefix.clone();
        prefix_with_colon.0.push(58);
        for scope in request["scope_refs"].as_list().unwrap() {
            let scope = scope.as_string().unwrap();
            if !scope.0.starts_with(&prefix_with_colon.0) {
                continue;
            }
            let mut scope = crate::compat::PyString(scope.0[prefix_with_colon.0.len()..].to_vec());
            while scope.ends_with("/") {
                scope.0.pop();
            }
            let mut nested = scope.clone();
            nested.0.push(47);
            approved = relative == scope || relative.0.starts_with(&nested.0);
            if approved {
                break;
            }
        }
        if !approved {
            return Err(contract("request.checkpoint_ref", "SCOPE_EXCEEDED"));
        }
        let root = if prefix.equals("repo") {
            &record["repository_identity"]["path"]
        } else {
            &record["context"]["state_root_identity"]["path"]
        };
        let root = self.files.from_string(root.as_string().unwrap())?;
        let snapshot = self
            .files
            .snapshot(
                &root.join(self.files.from_string(&relative)?),
                &[root],
                helper::MAX_RECORD_BYTES,
            )
            .map_err(snapshot_error)?;
        if !digest.equals(&snapshot.digest) {
            return Err(contract("request.checkpoint_ref", "INPUT_DIGEST_MISMATCH"));
        }
        Ok(())
    }
    fn preflight_record(&self, record: &Value, value: &Value) -> EngineResult<Preflight> {
        let request = validation::validate_request(value)?;
        let allowance = &record["allowance"];
        let assignment = validation::assignment_for(allowance, &request["assignment_id"])?;
        if assignment["model"]
            .as_string()
            .unwrap()
            .starts_with(crate::domain::hosts::RETIRED_MODEL_PREFIX)
        {
            return Err(contract("request.model", "MODEL_RETIRED"));
        }
        if [
            "parent_role",
            "helper_role",
            "contract_revision",
            "model",
            "reasoning_effort",
        ]
        .iter()
        .any(|key| !request[key].py_eq(&assignment[key]))
        {
            return Err(contract("request", "HELPER_ASSIGNMENT_MISMATCH"));
        }
        if request["scope_refs"]
            .as_list()
            .unwrap()
            .iter()
            .any(|scope| {
                !assignment["scope_refs"]
                    .as_list()
                    .unwrap()
                    .iter()
                    .any(|approved| approved.py_eq(scope))
            })
        {
            return Err(contract("request.scope_refs", "SCOPE_EXCEEDED"));
        }
        let approved = assignment["commands"]
            .as_list()
            .unwrap()
            .iter()
            .map(ids::canonical_bytes)
            .collect::<Result<Vec<_>, _>>()?;
        for command in request["commands"].as_list().unwrap() {
            if !approved.contains(&ids::canonical_bytes(command)?) {
                return Err(contract("request.commands", "COMMAND_NOT_APPROVED"));
            }
        }
        if request["helper_role"].is_text("validation_executor") && !request["commands"].truthy() {
            return Err(contract("request.commands", "COMMAND_REQUIRED"));
        }
        if request["helper_role"].is_text("evidence_scout") && request["commands"].truthy() {
            return Err(contract("request.commands", "COMMAND_FORBIDDEN"));
        }
        if !request["resource_keys"].container_eq(&assignment["resource_keys"]) {
            return Err(contract("request.resource_keys", "RESOURCE_EXCEEDED"));
        }
        self.verify_checkpoint(record, &request)?;
        let amounts = validation::required_amounts(&request)?;
        if helper::LIMIT_KEYS.iter().any(|key| {
            amounts[key].strict_integer().unwrap()
                > assignment["limits"][key].strict_integer().unwrap()
        }) {
            return Err(contract("request.budgets", "ASSIGNMENT_BUDGET_EXCEEDED"));
        }
        let mut missing = Vec::new();
        let observed = record["host_observation"]["supported_assignments"]
            .as_list()
            .unwrap()
            .iter()
            .find(|row| {
                row["model"].py_eq(&assignment["model"])
                    && row["reasoning_effort"].py_eq(&assignment["reasoning_effort"])
            });
        if observed.is_none()
            || !observed.unwrap()["status"].is_text("verified")
            || !helper::trusted_observation_source(observed.unwrap()["source"].as_string().unwrap())
        {
            missing.push("model_effort_assignment".into());
        }
        for capability in assignment["required_host_capabilities"].as_list().unwrap() {
            let capability = capability.text().unwrap();
            if allowance.get("test_mode").is_some()
                && helper::CONFINEMENT_CAPABILITIES.contains(&capability.as_str())
            {
                continue;
            }
            let observation = record["host_observation"]["capabilities"].get(&capability);
            if observation.is_none()
                || !observation.unwrap()["status"].is_text("verified")
                || !helper::trusted_observation_source(
                    observation.unwrap()["source"].as_string().unwrap(),
                )
            {
                missing.push(capability);
            }
        }
        missing.sort();
        Ok(Preflight {
            request,
            amounts,
            missing,
        })
    }
    pub fn preflight(&self, path: &Path, context: &Value, value: &Value) -> EngineResult<Value> {
        let (record, _) = self.repository.read_bound(path, context)?;
        let prepared = self.preflight_record(&record, value)?;
        let mut result = if prepared.missing.is_empty() {
            Value::object([
                ("ok", true.into()),
                ("code", "PREFLIGHT_READY".into()),
                ("request_id", prepared.request["request_id"].clone()),
                ("reservation", prepared.amounts),
            ])
        } else {
            Value::object([
                ("ok", false.into()),
                ("code", "BLOCKED_UNSUPPORTED".into()),
                ("request_id", prepared.request["request_id"].clone()),
                (
                    "missing_host_capabilities",
                    Value::list(prepared.missing.into_iter().map(Value::from)),
                ),
            ])
        };
        extend(&mut result, &test_mode_metadata(&record)?);
        Ok(result)
    }
    pub fn reserve(&self, path: &Path, context: &Value, value: &Value) -> EngineResult<Value> {
        self.repository
            .transaction(path, context, &mut |transaction| {
                let normalized = validation::validate_request(value)?;
                let digest = ids::sha256_bytes(&ids::canonical_bytes(&normalized)?);
                let identifier = normalized["request_id"].text().unwrap();
                let record = transaction.record();
                if let Some(existing) = record["reservations"]
                    .get(&identifier)
                    .filter(|value| !matches!(value, Value::Null))
                {
                    if !existing["request_digest"].is_text(&digest) {
                        return Err(failure("RESERVATION_CONFLICT"));
                    }
                    let mut result = Value::object([
                        ("ok", true.into()),
                        ("code", "REPLAYED".into()),
                        ("request_id", normalized["request_id"].clone()),
                        ("request_digest", digest.into()),
                        ("status", existing["status"].clone()),
                        ("new_dispatch_authorized", false.into()),
                    ]);
                    extend(&mut result, &test_mode_metadata(record)?);
                    return Ok(result);
                }
                let prepared = self.preflight_record(record, &normalized)?;
                if !prepared.missing.is_empty() {
                    let mut result = Value::object([
                        ("ok", false.into()),
                        ("code", "BLOCKED_UNSUPPORTED".into()),
                        ("request_id", normalized["request_id"].clone()),
                        (
                            "missing_host_capabilities",
                            Value::list(prepared.missing.into_iter().map(Value::from)),
                        ),
                    ]);
                    extend(&mut result, &test_mode_metadata(record)?);
                    return Ok(result);
                }
                let assignment_id = normalized["assignment_id"].text().unwrap();
                let assignment =
                    validation::assignment_for(&record["allowance"], &normalized["assignment_id"])?;
                let amounts = prepared.amounts;
                let mut assigned = record["assignment_usage"][&assignment_id].clone();
                for key in helper::LIMIT_KEYS {
                    let active = if *key == "concurrency" {
                        &record["active_concurrency"]
                    } else {
                        &record["usage"][key]
                    };
                    let amount = amounts[key].strict_integer().unwrap();
                    if active.strict_integer().unwrap() + amount
                        > *record["allowance"]["shared_limits"][key]
                            .strict_integer()
                            .unwrap()
                    {
                        return Err(failure("SHARED_LIMIT_EXCEEDED"));
                    }
                    if assigned[key].strict_integer().unwrap() + amount
                        > *assignment["limits"][key].strict_integer().unwrap()
                    {
                        return Err(failure("ASSIGNMENT_LIMIT_EXCEEDED"));
                    }
                }
                for key in normalized["resource_keys"].as_list().unwrap() {
                    let capacity = record["allowance"]["resources"]
                        .as_list()
                        .unwrap()
                        .iter()
                        .find(|resource| resource["key"].py_eq(key))
                        .unwrap()["capacity"]
                        .strict_integer()
                        .unwrap();
                    if record["active_resources"][&key.text().unwrap()]
                        .strict_integer()
                        .unwrap()
                        + BigInt::from(1)
                        > *capacity
                    {
                        return Err(failure("RESOURCE_CAPACITY_EXCEEDED"));
                    }
                }
                let mut usage = record["usage"].clone();
                let mut resources = record["active_resources"].clone();
                let mut assignments = record["assignment_usage"].clone();
                let mut reservations = record["reservations"].clone();
                for key in helper::LIMIT_KEYS {
                    let amount = amounts[key].strict_integer().unwrap();
                    if *key != "concurrency" {
                        add(&mut usage, key, amount);
                    }
                    add(&mut assigned, key, amount);
                }
                let active =
                    record["active_concurrency"].strict_integer().unwrap() + BigInt::from(1);
                for key in normalized["resource_keys"].as_list().unwrap() {
                    add(&mut resources, &key.text().unwrap(), &1_i64.into());
                }
                assignments.set(&assignment_id, assigned);
                reservations.set(
                    &identifier,
                    Value::object([
                        ("request_digest", digest.clone().into()),
                        ("assignment_id", normalized["assignment_id"].clone()),
                        ("status", "active".into()),
                        ("reservation", amounts),
                        ("resource_keys", normalized["resource_keys"].clone()),
                        ("settlement", Value::Null),
                    ]),
                );
                let record = transaction.record_mut();
                record.set("usage", usage);
                record.set("assignment_usage", assignments);
                record.set("active_concurrency", Value::Integer(active));
                record.set("active_resources", resources);
                record.set("reservations", reservations);
                transaction.commit()?;
                let mut result = Value::object([
                    ("ok", true.into()),
                    ("code", "RESERVED".into()),
                    ("request_id", normalized["request_id"].clone()),
                    ("request_digest", digest.into()),
                    ("status", "active".into()),
                    ("new_dispatch_authorized", true.into()),
                ]);
                extend(&mut result, &test_mode_metadata(transaction.record())?);
                Ok(result)
            })
    }
    pub fn settle(&self, path: &Path, context: &Value, value: &Value) -> EngineResult<Value> {
        let settlement = validation::validate_settlement(value)?;
        let digest = ids::sha256_bytes(&ids::canonical_bytes(&settlement)?);
        self.repository
            .transaction(path, context, &mut |transaction| {
                let record = transaction.record();
                let identifier = settlement["request_id"].text().unwrap();
                let Some(reservation) = record["reservations"]
                    .get(&identifier)
                    .filter(|value| !matches!(value, Value::Null))
                else {
                    return Err(failure("RESERVATION_NOT_FOUND"));
                };
                if !reservation["request_digest"].py_eq(&settlement["request_digest"]) {
                    return Err(failure("RESERVATION_NOT_FOUND"));
                }
                if !matches!(reservation["settlement"], Value::Null) {
                    if !reservation["settlement_digest"].is_text(&digest) {
                        return Err(failure("SETTLEMENT_CONFLICT"));
                    }
                    let mut result = Value::object([
                        ("ok", true.into()),
                        ("code", "REPLAYED".into()),
                        ("request_id", settlement["request_id"].clone()),
                        ("status", reservation["status"].clone()),
                    ]);
                    extend(&mut result, &test_mode_metadata(record)?);
                    return Ok(result);
                }
                if !reservation["status"].is_text("active") {
                    return Err(failure("SETTLEMENT_CONFLICT"));
                }
                let mut reservation = reservation.clone();
                reservation.set("status", settlement["terminal_state"].clone());
                reservation.set("settlement", settlement.clone());
                reservation.set("settlement_digest", digest.clone().into());
                let active =
                    record["active_concurrency"].strict_integer().unwrap() - BigInt::from(1);
                let assignment_id = reservation["assignment_id"].text().unwrap();
                let mut assigned = record["assignment_usage"][&assignment_id].clone();
                add(
                    &mut assigned,
                    "concurrency",
                    &-reservation["reservation"]["concurrency"]
                        .strict_integer()
                        .unwrap(),
                );
                let mut assignments = record["assignment_usage"].clone();
                assignments.set(&assignment_id, assigned);
                let mut resources = record["active_resources"].clone();
                for key in reservation["resource_keys"].as_list().unwrap() {
                    add(&mut resources, &key.text().unwrap(), &(-1_i64).into());
                }
                let mut reservations = record["reservations"].clone();
                reservations.set(&identifier, reservation);
                let record = transaction.record_mut();
                record.set("active_concurrency", Value::Integer(active));
                record.set("assignment_usage", assignments);
                record.set("active_resources", resources);
                record.set("reservations", reservations);
                transaction.commit()?;
                let mut result = Value::object([
                    ("ok", true.into()),
                    ("code", "SETTLED".into()),
                    ("request_id", settlement["request_id"].clone()),
                    ("status", settlement["terminal_state"].clone()),
                ]);
                extend(&mut result, &test_mode_metadata(transaction.record())?);
                Ok(result)
            })
    }
    pub fn status(&self, path: &Path, context: &Value) -> EngineResult<Value> {
        let (record, context) = self.repository.read_bound(path, context)?;
        let mut reservations = record["reservations"]
            .as_object()
            .unwrap()
            .iter()
            .collect::<Vec<_>>();
        reservations.sort_by(|left, right| left.0.cmp(&right.0));
        let mut result = Value::object([
            ("ok", true.into()),
            ("code", "STATUS".into()),
            ("context", context),
            ("usage", record["usage"].clone()),
            ("active_concurrency", record["active_concurrency"].clone()),
            ("active_resources", record["active_resources"].clone()),
            (
                "reservations",
                Value::Object(
                    reservations
                        .into_iter()
                        .map(|(key, value)| {
                            (
                                key.clone(),
                                Value::object([
                                    ("status", value["status"].clone()),
                                    ("assignment_id", value["assignment_id"].clone()),
                                ]),
                            )
                        })
                        .collect(),
                ),
            ),
            (
                "evidence_notice",
                "trusted-caller evidence; not proof of host confinement or authenticity".into(),
            ),
        ]);
        extend(&mut result, &test_mode_metadata(&record)?);
        Ok(result)
    }
}
fn add(record: &mut Value, key: &str, amount: &BigInt) {
    let value = record[key].strict_integer().unwrap() + amount;
    record.set(key, Value::Integer(value));
}
