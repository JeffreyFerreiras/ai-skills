use graph_engine::{
    application::snapshots::{self, FileSnapshotPort, SnapshotFailure},
    compat::{self, SerializationProfile, Value},
    domain::{
        constraints, contracts, execution, hosts, ids, merge_readiness, planner, policy,
        policy_validation,
    },
    infrastructure::filesystem,
};
use std::io::{self, BufRead, Write};
#[path = "../suite/artifact_parity.rs"]
mod artifact_parity;
#[path = "../suite/helper_parity.rs"]
mod helper_parity;
#[path = "../suite/sorting_parity.rs"]
mod sorting_parity;
#[path = "../suite/store_parity.rs"]
mod store_parity;

fn engine_response(
    result: graph_engine::application::error::EngineResult<Value>,
) -> Result<Value, Value> {
    use graph_engine::application::error::EngineError;
    result.map_err(|error| match error {
        EngineError::Contract(error) => Value::object([
            ("kind", "ContractError".into()),
            ("field", Value::String(error.field)),
            ("code", error.code.into()),
        ]),
        EngineError::State(error) => {
            Value::object([("kind", "StateError".into()), ("code", error.code.into())])
        }
        EngineError::Helper(error) => Value::object([
            ("kind", "HelperRegisterError".into()),
            ("code", error.code.into()),
        ]),
        EngineError::Python { kind, message }
        | EngineError::TracedGraphctl { kind, message, .. } => {
            Value::object([("kind", kind.into()), ("message", Value::String(message))])
        }
        EngineError::Json(error) | EngineError::TracedGraphctlJson { error, .. } => {
            Value::object([
                ("kind", error.kind.into()),
                ("message", error.message.into()),
            ])
        }
        EngineError::Io(error) => {
            let kind = match error.kind() {
                io::ErrorKind::NotFound => "FileNotFoundError",
                io::ErrorKind::PermissionDenied => "PermissionError",
                io::ErrorKind::AlreadyExists => "FileExistsError",
                io::ErrorKind::NotADirectory => "NotADirectoryError",
                io::ErrorKind::IsADirectory => "IsADirectoryError",
                _ => "OSError",
            };
            Value::object([("kind", kind.into()), ("message", error.to_string().into())])
        }
        EngineError::Storage { kind, message, .. } => {
            Value::object([("kind", kind.into()), ("message", message.into())])
        }
        EngineError::Injected(message) => {
            Value::object([("kind", "Injected".into()), ("message", message.into())])
        }
        EngineError::ProcessDeath(message) => {
            Value::object([("kind", "ProcessDeath".into()), ("message", message.into())])
        }
    })
}

fn scalar_descriptor(value: &Value) -> Value {
    match value {
        Value::Bytes(bytes) => Value::object([
            ("type", "bytes".into()),
            (
                "hex",
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
                    .into(),
            ),
        ]),
        Value::Object(items) => Value::object([
            ("type", "dict".into()),
            (
                "items",
                Value::list(items.iter().map(|(key, value)| {
                    Value::list([Value::String(key.clone()), scalar_descriptor(value)])
                })),
            ),
        ]),
        _ => Value::object([
            ("type", value.python_type().into()),
            ("value", value.clone()),
        ]),
    }
}

fn fixture_value(parameter: &Value) -> Value {
    if parameter["type"].is_text("bytes") || parameter["type"].is_text("bytearray") {
        let hex = parameter["hex"].text().unwrap();
        Value::Bytes(
            (0..hex.len())
                .step_by(2)
                .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
                .collect(),
        )
    } else {
        parameter["value"].clone()
    }
}

fn dispatch(request: &Value) -> Result<Value, Value> {
    if [
        "sort_trace",
        "private_sorted_json",
        "private_subset",
        "tuple_operation",
    ]
    .iter()
    .any(|name| request["op"].is_text(name))
    {
        return sorting_parity::dispatch(request);
    }
    let contract = |result: Result<Value, contracts::ContractError>| {
        result.map_err(|e| {
            Value::object([
                ("kind", "ContractError".into()),
                ("field", Value::String(e.field)),
                ("code", e.code.into()),
            ])
        })
    };
    let serialization = |result: Result<Vec<u8>, compat::JsonError>| {
        result
            .map(|bytes| Value::from(String::from_utf8(bytes).unwrap()))
            .map_err(|e| {
                Value::object([
                    ("kind", e.kind.into()),
                    ("message", e.message.into()),
                    ("position", e.position.into()),
                ])
            })
    };
    let value = &request["value"];
    let domain = |result: Result<Value, String>| {
        result.map_err(|message| {
            Value::object([("kind", "ValueError".into()), ("message", message.into())])
        })
    };
    let constraint = |result: Result<Value, constraints::ConstraintFailure>| {
        result.map_err(|error| match error {
            constraints::ConstraintFailure::Contract(e) => Value::object([
                ("kind", "ContractError".into()),
                ("field", Value::String(e.field)),
                ("code", e.code.into()),
            ]),
            constraints::ConstraintFailure::Python { kind, message } => {
                Value::object([("kind", kind.into()), ("message", Value::String(message))])
            }
        })
    };
    match request["op"].text().as_deref() {
        Some("state_store" | "sqlite_uri") => engine_response(store_parity::dispatch(request)),
        Some(
            "helper_allowance"
            | "helper_host_observation"
            | "helper_limits"
            | "helper_commands"
            | "helper_parent_capabilities"
            | "helper_scope_refs"
            | "helper_unique_ids"
            | "helper_integer"
            | "helper_test_mode"
            | "helper_plan"
            | "helper_context"
            | "helper_request"
            | "helper_amounts"
            | "helper_settlement"
            | "helper_filesystem_identity"
            | "helper_record"
            | "helper_accounting",
        ) => engine_response(helper_parity::dispatch(request)),
        Some("repr_string") => {
            let described = request.get("value_points").map(|points| {
                compat::PyString(
                    points
                        .as_list()
                        .unwrap()
                        .iter()
                        .map(|value| value.as_i64().unwrap() as u32)
                        .collect(),
                )
            });
            Ok(Value::String(compat::repr::string_repr(
                described
                    .as_ref()
                    .unwrap_or_else(|| value.as_string().unwrap()),
            )))
        }
        Some(
            "artifact_resolve" | "artifact_unhashed" | "artifact_reverify" | "artifact_persist",
        ) => engine_response(artifact_parity::dispatch(request)),
        Some("artifact_kind_policy") => engine_response(
            graph_engine::domain::evidence::kind_policy(&request["kind"], &request["policy"])
                .cloned()
                .map_err(Into::into),
        ),
        Some("artifact_size_limit") => engine_response(
            graph_engine::domain::evidence::artifact_size_limit(
                &request["kind"],
                &request["policy"],
            )
            .map(Value::Integer)
            .map_err(Into::into),
        ),
        Some("artifact_enforce_size") => engine_response(
            graph_engine::domain::evidence::enforce_artifact_size(
                &request["kind"],
                value,
                &request["policy"],
            )
            .map(|_| Value::Null)
            .map_err(Into::into),
        ),
        Some("artifact_within_roots") => engine_response(
            graph_engine::domain::evidence::within_roots(
                value.as_string().unwrap(),
                &request["roots"],
            )
            .map(Value::from)
            .map_err(Into::into),
        ),
        Some("artifact_path_allowed") => engine_response(
            graph_engine::domain::evidence::policy_path_allowed(value, &request["policy"])
                .map(|_| Value::Null)
                .map_err(Into::into),
        ),
        Some("ledger_artifact") => engine_response(
            graph_engine::domain::evidence::canonical_ledger_artifact(
                request["identifier"].as_string().unwrap(),
                &request["kind"],
                value,
            )
            .map(|artifact| artifact.record())
            .map_err(Into::into),
        ),
        Some("loads_value") => engine_response(
            compat::loads_value(&fixture_value(&request["parameter"]))
                .and_then(|value| compat::dumps(&value, SerializationProfile::CLI))
                .map(|bytes| String::from_utf8(bytes).unwrap().into())
                .map_err(Into::into),
        ),
        Some("python_int") => engine_response(
            compat::integer::coerce(&fixture_value(&request["parameter"]))
                .map(Value::Integer)
                .map_err(Into::into),
        ),
        Some("bytes_profile") => {
            let mut value = Value::Bytes(vec![0, 255]);
            for _ in 0..request["depth"].as_i64().unwrap().max(0) {
                value = if request["container"].is_text("object") {
                    Value::object([("value", value)])
                } else {
                    Value::list([value])
                };
            }
            let cloned = value.clone();
            assert!(value.py_eq(&cloned));
            drop(cloned);
            engine_response(
                compat::dumps(&value, SerializationProfile::CANONICAL)
                    .map(|bytes| String::from_utf8(bytes).unwrap().into())
                    .map_err(Into::into),
            )
        }
        Some("sqlite_rows") => {
            let operation = (|| -> graph_engine::application::error::EngineResult<Value> {
                let session = graph_engine::infrastructure::sqlite::SqliteSession::in_memory()?;
                let mut parameters = Vec::new();
                for parameter in request["parameters"].as_list().unwrap() {
                    parameters.push(fixture_value(parameter));
                }
                let rows = session.query_values(&request["sql"].text().unwrap(), &parameters)?;
                let mut described = Vec::new();
                for row in rows {
                    let mut result = Value::object([("result", scalar_descriptor(&row))]);
                    match compat::dumps(&row, SerializationProfile::CLI) {
                        Ok(bytes) => result.set("json", String::from_utf8(bytes).unwrap().into()),
                        Err(error) => result.set(
                            "json_error",
                            Value::object([
                                ("kind", error.kind.into()),
                                ("message", error.message.into()),
                            ]),
                        ),
                    }
                    described.push(result);
                }
                Ok(Value::List(described))
            })();
            engine_response(operation)
        }
        Some("sqlite_version") => Ok(graph_engine::infrastructure::sqlite::sqlite_version().into()),
        Some(
            "policy_default_native"
            | "policy_load_native"
            | "state_root_native"
            | "policy_task_paths"
            | "policy_case_identity",
        ) => {
            use graph_engine::{
                application::policy::{self, PolicyFilePort},
                infrastructure::policy_files::NativePolicyFiles,
            };
            let files = NativePolicyFiles {
                environment_override: Some(
                    request["environment"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(key, value)| {
                            (key.utf8().unwrap(), value.as_string().unwrap().clone())
                        })
                        .collect(),
                ),
            };
            let operation = (|| -> graph_engine::application::error::EngineResult<Value> {
                let repo = files.from_string(request["repo"].as_string().unwrap())?;
                let skill = files.from_string(request["skill"].as_string().unwrap())?;
                let runtime = request
                    .get("runtime")
                    .filter(|value| !matches!(value, Value::Null))
                    .map(|value| files.from_string(value.as_string().unwrap()))
                    .transpose()?;
                if request["op"].is_text("policy_task_paths") {
                    return policy::validate_task_capability_paths(
                        &files,
                        &repo,
                        &request["task"],
                        &request["policy"],
                        runtime.as_deref(),
                        request
                            .get("version")
                            .filter(|value| !matches!(value, Value::Null)),
                    )
                    .map(|_| Value::Null);
                }
                if request["op"].is_text("policy_case_identity") {
                    return policy::prove_capability_case_identity(
                        &files,
                        &repo,
                        runtime.as_deref(),
                        value,
                        &request["outer"],
                    )
                    .map(|_| Value::Null);
                }
                if request["op"].is_text("policy_default_native") {
                    return policy::default_profile_policy(&files, &repo);
                }
                if request["op"].is_text("state_root_native") {
                    return Ok(Value::String(files.spelling(&policy::resolve_state_root(
                        &files,
                        runtime.as_deref(),
                        &skill,
                    )?)));
                }
                let (value, snapshot) =
                    policy::load_policy(&files, &repo, &skill, runtime.as_deref())?;
                let raw: String = snapshot
                    .data
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                Ok(Value::object([
                    ("value", value),
                    (
                        "snapshot",
                        Value::object([
                            ("path", Value::String(files.spelling(&snapshot.path))),
                            ("digest", snapshot.digest.into()),
                            ("size", snapshot.size.into()),
                            ("data", raw.into()),
                            (
                                "identity",
                                Value::list([
                                    Value::Integer(snapshot.identity.device.into()),
                                    Value::Integer(snapshot.identity.inode.into()),
                                ]),
                            ),
                            ("parsed", snapshot.parsed.unwrap()),
                        ]),
                    ),
                ]))
            })();
            engine_response(operation)
        }
        Some("require_keys") => constraint(
            contracts::require_python_keys(value, &request["required"], &request["allowed"], "f")
                .map(|_| Value::Null),
        ),
        Some("unicode_decimal_inventory") => Ok(Value::list((0..0x110000).filter_map(|point| {
            compat::unicode::decimal(point)
                .map(|digit| Value::list([Value::from(point as i64), Value::from(digit as i64)]))
        }))),
        Some("policy_version_tuple") => contract(
            policy_validation::version_tuple(value)
                .map(|parts| Value::list(parts.into_iter().map(Value::Integer))),
        ),
        Some("policy_engine_compatible") => constraint(
            policy_validation::engine_version_compatible(value, &request["policy"])
                .map(Value::from),
        ),
        Some("policy_output_contract") => constraint(
            policy_validation::validate_output_contract(&request["node"].text().unwrap(), value)
                .map(|_| Value::Null),
        ),
        Some("policy_capability_shape") => {
            let commands: Vec<_> = request["commands"]
                .as_list()
                .unwrap()
                .iter()
                .map(|item| item.text().unwrap())
                .collect();
            constraint(
                policy_validation::validate_capability_shape(
                    value,
                    &request["role"].text().unwrap(),
                    &commands,
                )
                .map(|(effect, action, target)| Value::list([effect, action, target])),
            )
        }
        Some("policy_repository_spelling") => constraint(
            policy_validation::repository_target_spelling(
                value,
                request["denied"].as_list().unwrap(),
            )
            .map(Value::String),
        ),
        Some("policy_runtime_target") => constraint(
            policy_validation::runtime_target(
                value,
                request["roots"].as_list().unwrap(),
                request["windows"].is_true(),
            )
            .map(Value::String),
        ),
        Some("canonical") => serialization(ids::canonical_bytes(value)),
        Some("cli_json") => serialization(compat::dumps(value, SerializationProfile::CLI)),
        Some("stored_json") => serialization(compat::dumps(value, SerializationProfile::STORED)),
        Some("sorted_stored_json") => {
            serialization(compat::dumps(value, SerializationProfile::SORTED_STORED))
        }
        Some("parse") => {
            let text = value
                .as_string()
                .ok_or_else(|| Value::from("STRING_REQUIRED"))?;
            match compat::loads_text(text) {
                Ok(parsed) => serialization(compat::dumps(&parsed, SerializationProfile::CLI)),
                Err(e) => Err(Value::object([
                    ("kind", e.kind.into()),
                    ("message", e.message.into()),
                    ("position", e.position.into()),
                ])),
            }
        }
        Some("lower" | "casefold" | "normcase") => {
            let text = value
                .as_string()
                .ok_or_else(|| Value::from("STRING_REQUIRED"))?;
            Ok(Value::String(match request["op"].text().as_deref() {
                Some("lower") => compat::unicode::lower(text),
                Some("casefold") => compat::unicode::casefold(text),
                _ => compat::unicode::normcase(text, request["windows"].is_true()),
            }))
        }
        Some("unicode_property") => Ok(Value::from(compat::unicode::property(
            request["point"].as_i64().unwrap_or(-1) as u32,
            request["bit"].as_i64().unwrap_or(0) as u8,
        ))),
        Some("py_eq") => Ok(Value::from(value.py_eq(&request["other"]))),
        Some("repository_digest") => Ok(ids::repository_digest(&value.text().unwrap()).into()),
        Some("stable_id") => serialization(
            ids::stable_id(
                &request["run_id"].text().unwrap(),
                &request["policy_digest"].text().unwrap(),
                &request["entity_type"].text().unwrap(),
                &request["template_key"].text().unwrap(),
                request["generation"].as_i64().unwrap(),
                request["specialist_tag"].text().as_deref(),
            )
            .map(|s| s.into_bytes()),
        ),
        Some("opaque") => contract(contracts::opaque(value, "f").map(Value::from)),
        Some("digest") => contract(contracts::digest(value, "f").map(Value::from)),
        Some("bounded_string") => contract(
            contracts::bounded_string(
                value,
                "f",
                request["maximum"].as_i64().unwrap_or(4096) as usize,
            )
            .map(|s| Value::String(s.clone())),
        ),
        Some("reject_sensitive") => {
            contract(contracts::reject_sensitive(value, "root", 0).map(|_| Value::Null))
        }
        Some("lexical_relative") => {
            contract(contracts::lexical_relative(value, "ref").map(Value::String))
        }
        Some("validate_ref") => contract(
            contracts::validate_ref(value, "f", request["content_required"].is_true())
                .map(Value::String),
        ),
        Some("merge_readiness") => Ok(merge_readiness::assess_merge_readiness(value)),
        Some("glob") => {
            let text = value.as_string().unwrap();
            let pattern = request["pattern"].as_string().unwrap();
            if request["posix_path"].is_true() {
                domain(
                    compat::glob::posix_path_match(text, pattern)
                        .map(Value::from)
                        .map_err(|_| "empty pattern".into()),
                )
            } else {
                Ok(Value::from(compat::glob::fnmatchcase(text, pattern)))
            }
        }
        Some("filesystem_capability_target") => constraint(
            policy_validation::filesystem_target(value, &request["denied"]).map(Value::String),
        ),
        Some("capability_within") => constraint(
            contracts::capability_within(value, &request["ceiling"], request["windows"].is_true())
                .map(Value::from),
        ),
        Some("policy_role_allowed") => constraint(
            policy_validation::role_capability_allowed(
                value,
                &request["role"],
                &request["capability"],
                request["windows"].is_true(),
            )
            .map(Value::from),
        ),
        Some("policy_location_key") => constraint(
            policy_validation::location_key(value, request["windows"].is_true()).map(Value::String),
        ),
        Some("branch_lease_seconds") => {
            constraint(policy_validation::branch_lease_seconds(value).map(Value::Integer))
        }
        Some("policy_under_root") => constraint(
            policy_validation::under_configured_root(
                value,
                &request["roots"],
                request["windows"].is_true(),
            )
            .map(Value::from),
        ),
        Some("policy_roots_overlap") => constraint(
            policy_validation::roots_overlap(
                value,
                &request["other"],
                request["windows"].is_true(),
            )
            .map(Value::from),
        ),
        Some("policy_duplicates") => constraint(
            policy_validation::has_location_duplicates(value, request["windows"].is_true())
                .map(Value::from),
        ),
        Some("projected_capability_within") => constraint(
            contracts::projected_capability_within(
                value,
                &request["ceiling"],
                request
                    .get("version")
                    .filter(|value| !matches!(value, Value::Null)),
                request["windows"].is_true(),
            )
            .map_err(|error| match error {
                contracts::ProjectionError::Validation(error) => error,
                contracts::ProjectionError::InvalidVersion => {
                    graph_engine::domain::errors::ValidationFailure::python(
                        "ValueError",
                        "CAPABILITY_PROJECTION_VERSION_INVALID",
                    )
                }
            })
            .map(Value::from),
        ),
        Some("policy_default_data") => Ok(policy::default_profile_policy(
            &request["repository_id"].text().unwrap(),
            request["roots"].as_list().unwrap(),
        )),
        Some("fanout_ordering") => domain(
            planner::validate_fanout_ordering(
                value.as_list().unwrap(),
                request["dependencies"].as_list().unwrap(),
                request["case_sensitive"].is_true(),
            )
            .map(Value::List),
        ),
        Some("sqlite_schema") => graph_engine::infrastructure::sqlite::schema_contract_snapshot()
            .map_err(|e| {
                Value::object([
                    ("kind", "SQLiteError".into()),
                    ("message", e.to_string().into()),
                ])
            }),
        Some("hosts") => {
            let host = request["host"].text().unwrap();
            let revision = request["revision"].as_i64();
            let result = match request["function"].text().as_deref() {
                Some("model_options") => hosts::model_options(&host, revision).map(|options| {
                    Value::Object(
                        options
                            .into_iter()
                            .map(|(model, efforts)| {
                                (
                                    model.into(),
                                    Value::list(efforts.into_iter().map(Value::from)),
                                )
                            })
                            .collect(),
                    )
                }),
                Some("recommended_assignment") => hosts::recommended_assignment(
                    &host,
                    &request["workload"].text().unwrap(),
                    revision,
                )
                .map(|(a, b)| Value::list([a.into(), b.into()])),
                Some("selected_dispatch_model") => hosts::selected_dispatch_model(
                    &host,
                    &request["model"].text().unwrap(),
                    &request["effort"].text().unwrap(),
                    revision,
                )
                .map(Value::from),
                Some("resolve_assignment") => hosts::resolve_assignment(
                    &host,
                    &request["class"].text().unwrap(),
                    &request["effort"].text().unwrap(),
                )
                .map(|(a, b)| Value::list([a.into(), b.into()])),
                Some("dispatch_weight_for") => Ok(hosts::dispatch_weight_for(
                    &request["model"].text().unwrap(),
                    &request["effort"].text().unwrap(),
                    request["historical"].is_true(),
                )
                .map(Value::from)
                .unwrap_or(Value::Null)),
                Some("supported_dispatch_weights") => Ok(Value::list(
                    hosts::supported_dispatch_weights().into_iter().map(
                        |((model, effort), weight)| {
                            Value::list([model.into(), effort.into(), weight.into()])
                        },
                    ),
                )),
                _ => Err("UNKNOWN_HOST_FUNCTION".into()),
            };
            domain(result)
        }
        Some("execution") => {
            let host = request["host"]
                .text()
                .unwrap_or_else(|| hosts::DEFAULT_HOST.into());
            let size = request["size"].text();
            let run = request["run_id"].text().unwrap_or_else(|| "RUN-1".into());
            let result = match request["function"].text().as_deref() {
                Some("build") => {
                    execution::build_execution_plan(&run, value, size.as_deref(), &host)
                }
                Some("reconstruct") => execution::reconstruct_execution_plan(
                    &run,
                    value,
                    &request["stored"],
                    size.as_deref(),
                ),
                Some("validate_model_assignment") => execution::validate_model_assignment(
                    &request["node"].text().unwrap(),
                    &request["model"].text().unwrap(),
                    &request["effort"].text().unwrap(),
                    &host,
                )
                .map(|_| Value::Null),
                Some("validate_new_plan_assignment") => execution::validate_new_plan_assignment(
                    &request["node"].text().unwrap(),
                    &request["model"].text().unwrap(),
                    &request["effort"].text().unwrap(),
                    &host,
                )
                .map(|_| Value::Null),
                Some("validate_model_overrides") => execution::validate_model_overrides(value),
                Some("assignment_for") => {
                    execution::assignment_for(value, &request["node"].text().unwrap()).cloned()
                }
                _ => Err("UNKNOWN_EXECUTION_FUNCTION".into()),
            };
            domain(result)
        }
        Some("constraints_validate") => constraint(constraints::validate_bundle(value)),
        Some("constraints_select") => {
            let normalize = |text: &compat::PyString| {
                if request["windows"].is_true() {
                    compat::unicode::normcase(text, true)
                } else {
                    text.clone()
                }
            };
            constraint(constraints::select_constraints(
                value,
                &request["context"],
                constraints::Selection {
                    activated: request["activated"].is_true(),
                    supported: request["supported"].as_list().unwrap_or(&[]),
                    authority_refs: request["authority_refs"].as_list().unwrap_or(&[]),
                    normalize: &normalize,
                },
            ))
        }
        Some("snapshot" | "json_snapshot") => {
            use base64::Engine;
            let path = std::path::PathBuf::from(request["path"].text().unwrap());
            let roots: Vec<_> = request["roots"]
                .as_list()
                .unwrap()
                .iter()
                .map(|v| std::path::PathBuf::from(v.text().unwrap()))
                .collect();
            let maximum = request["maximum"].as_i64().unwrap() as usize;
            let result = if request["op"].is_text("json_snapshot") {
                snapshots::safe_json_snapshot(
                    &filesystem::NativeFileSnapshots,
                    &path,
                    &roots,
                    maximum,
                )
            } else {
                filesystem::NativeFileSnapshots.snapshot(&path, &roots, maximum)
            };
            match result {
                Ok(s) => Ok(Value::object([
                    (
                        "path",
                        Value::String(filesystem::os_string(s.path.as_os_str())),
                    ),
                    (
                        "data",
                        base64::engine::general_purpose::STANDARD
                            .encode(s.data)
                            .into(),
                    ),
                    ("digest", s.digest.into()),
                    ("size", s.size.into()),
                    ("parsed", s.parsed.unwrap_or(Value::Null)),
                    ("device", Value::Integer(s.identity.device.into())),
                    ("inode", Value::Integer(s.identity.inode.into())),
                ])),
                Err(SnapshotFailure::Contract(e)) => contract(Err(e)),
                Err(SnapshotFailure::Json(e)) => Err(Value::object([
                    ("kind", e.kind.into()),
                    ("message", e.message.into()),
                    ("position", e.position.into()),
                ])),
                Err(SnapshotFailure::Io(e)) => Err(Value::object([(
                    "kind",
                    match e.kind() {
                        std::io::ErrorKind::NotFound => "FileNotFoundError",
                        std::io::ErrorKind::PermissionDenied => "PermissionError",
                        _ => "OSError",
                    }
                    .into(),
                )])),
            }
        }
        _ => Err(Value::from("UNKNOWN_PARITY_OPERATION")),
    }
}
fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().split(b'\n') {
        let line = line.expect("stdin read failure");
        if line.is_empty() {
            continue;
        }
        let response = match compat::loads(&line) {
            Ok(request) => match dispatch(&request) {
                Ok(value) => Value::object([("ok", true.into()), ("result", value)]),
                Err(value) => Value::object([("ok", false.into()), ("error", value)]),
            },
            Err(e) => Value::object([
                ("ok", false.into()),
                (
                    "error",
                    Value::object([
                        ("kind", e.kind.into()),
                        ("message", e.message.into()),
                        ("position", e.position.into()),
                    ]),
                ),
            ]),
        };
        let bytes = compat::dumps(&response, SerializationProfile::CLI)
            .expect("private response serialization failed");
        stdout.write_all(&bytes).unwrap();
        stdout.write_all(b"\n").unwrap();
        stdout.flush().unwrap();
    }
}
