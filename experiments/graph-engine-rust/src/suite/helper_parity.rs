use graph_engine::{
    application::{error::EngineResult, policy::PolicyFilePort},
    compat::Value,
    domain::helper_register as helper,
    infrastructure::policy_files::NativePolicyFiles,
};

pub fn dispatch(request: &Value) -> EngineResult<Value> {
    let value = &request["value"];
    let field = "f";
    let files = NativePolicyFiles::default();
    match request["op"].text().as_deref() {
        Some("helper_plan") => {
            return graph_engine::application::helper_validation::validate_plan(
                value,
                &request["run_id"],
            )
        }
        Some("helper_context") => {
            return graph_engine::application::helper_validation::validate_context(value, &files)
        }
        Some("helper_request") => {
            return graph_engine::application::helper_validation::validate_request(value)
        }
        Some("helper_amounts") => {
            return graph_engine::application::helper_validation::required_amounts(value)
        }
        Some("helper_settlement") => {
            return graph_engine::application::helper_validation::validate_settlement(value)
        }
        Some("helper_filesystem_identity") => {
            return graph_engine::application::helper_validation::filesystem_identity(
                value, "f", &files,
            )
        }
        Some("helper_record") => {
            return graph_engine::application::helper_validation::validate_record(value, &files)
        }
        Some("helper_accounting") => {
            return graph_engine::application::helper_validation::check_accounting(
                value,
                &request["allowance"],
            )
            .map(|_| Value::Null)
        }
        _ => (),
    }
    let absolute = |text: &graph_engine::compat::PyString| {
        #[cfg(windows)]
        {
            files.from_string(text).is_ok_and(|path| path.is_absolute())
        }
        #[cfg(unix)]
        {
            text.starts_with("/")
        }
    };
    let result = match request["op"].text().as_deref() {
        Some("helper_allowance") => helper::validate_allowance(
            value,
            &request["run_id"],
            request
                .get("host")
                .filter(|value| !matches!(value, Value::Null)),
            request
                .get("approved")
                .filter(|value| !matches!(value, Value::Null)),
            request["historical"].is_true(),
            &absolute,
        ),
        Some("helper_host_observation") => helper::validate_host_observation(value),
        Some("helper_limits") => helper::limits(value, field),
        Some("helper_commands") => helper::commands(value, field),
        Some("helper_parent_capabilities") => helper::parent_capabilities(value, field),
        Some("helper_scope_refs") => helper::scope_refs(value, field)
            .map(|values| Value::list(values.into_iter().map(Value::String))),
        Some("helper_unique_ids") => {
            helper::unique_ids(value, field, request["allow_empty"].is_true())
                .map(|values| Value::list(values.into_iter().map(Value::from)))
        }
        Some("helper_integer") => helper::integer(
            value,
            field,
            request["minimum"].as_i64().unwrap(),
            request["maximum"].as_i64().unwrap(),
        )
        .map(Value::Integer),
        Some("helper_test_mode") => helper::validate_test_mode(value, &absolute),
        _ => unreachable!(),
    };
    result.map_err(Into::into)
}
