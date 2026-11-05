use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        persistent_store::{InitializeRequest, StatePlatform, StateStore},
        policy::PolicyFilePort,
    },
    compat::Value,
    infrastructure::{
        policy_files::NativePolicyFiles, sqlite, state_database::NativeStateDatabase,
        state_platform::NativeStatePlatform,
    },
};
use std::{cell::RefCell, path::PathBuf};

pub fn dispatch(request: &Value) -> EngineResult<Value> {
    let files = NativePolicyFiles {
        environment_override: Some(
            request["environment"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.utf8().unwrap(), value.as_string().unwrap().clone()))
                .collect(),
        ),
    };
    let path = |key: &str| -> EngineResult<PathBuf> {
        files.from_string(request[key].as_string().unwrap())
    };
    if request["op"].is_text("sqlite_uri") {
        return Ok(sqlite::connection_uri(&path("path")?)?.into());
    }
    let platform = NativeStatePlatform;
    if request["action"].is_text("host") {
        return Ok(platform.host_identity()?.into());
    }
    if request["action"].is_text("actor") {
        return Ok(Value::String(
            graph_engine::infrastructure::state_platform::current_actor()?,
        ));
    }
    if request["action"].is_text("process") {
        return Ok(platform.process_alive(&request["process_id"])?.into());
    }
    let optional = |key: &str| -> EngineResult<Option<PathBuf>> {
        request
            .get(key)
            .filter(|value| !matches!(value, Value::Null))
            .map(|value| files.from_string(value.as_string().unwrap()))
            .transpose()
    };
    let home = optional("codex_home")?;
    let root = optional("state_root")?;
    let skill = path("skill")?;
    let database = NativeStateDatabase;
    let points = RefCell::new(Vec::new());
    let hook = |point: &str| {
        points.borrow_mut().push(point.to_owned());
        if request["fault"].is_text(point) {
            return Err(if request["process_death"].is_true() {
                EngineError::ProcessDeath(point.into())
            } else {
                EngineError::Injected(point.into())
            });
        }
        Ok(())
    };
    let store = StateStore::new(
        home.as_deref(),
        root.as_deref(),
        &skill,
        &files,
        &platform,
        &database,
        &hook,
    )?;
    let action = request["action"].text().unwrap();
    if action == "roots" {
        return Ok(Value::object([
            (
                "state_root",
                Value::String(platform.spelling(&store.state_root)),
            ),
            (
                "run_root",
                Value::String(platform.spelling(&store.run_root("fixture-repository", "RUN-1"))),
            ),
            (
                "inbox_root",
                Value::String(platform.spelling(&store.inbox_root("fixture-repository", "RUN-1"))),
            ),
            (
                "database",
                Value::String(platform.spelling(&store.db_path("fixture-repository", "RUN-1"))),
            ),
        ]));
    }
    if action == "repository" {
        let (info, path) = store.repository_identity(&path("repo")?)?;
        return Ok(Value::list([
            Value::Integer(info.device.into()),
            Value::Integer(info.inode.into()),
            Value::String(path),
        ]));
    }
    if action == "locality" {
        return Ok(platform.locality(&path("path")?)?.into());
    }
    if action == "incomplete_digest" {
        return store.incomplete_database_request_digest(&path("path")?, "RUN-1");
    }
    if action == "verify_inbox" {
        store.verify_inbox_manifest("fixture-repository", "RUN-1", &path("path")?)?;
        return Ok(Value::Null);
    }
    if action == "open" {
        return store.with_run("fixture-repository", "RUN-1", |connection| {
            Ok(connection.first_run()?.unwrap())
        });
    }
    let repo = path("repo")?;
    let task_path = path("task_path")?;
    let task_ref = Value::from("task-ref");
    let value = InitializeRequest {
        repository: &repo,
        policy: &request["policy"],
        policy_digest: &request["policy_digest"].text().unwrap(),
        task: &request["task"],
        task_path: &task_path,
        task_digest: &request["task_digest"].text().unwrap(),
        run_id: "RUN-1",
        operation_id: &request["operation_id"].text().unwrap(),
        request_digest: &request["request_digest"].text().unwrap(),
        acknowledge_permissions: request["ack_permissions"].is_true(),
        acknowledge_durability: request["ack_durability"].is_true(),
        bootstrap: &request["bootstrap"],
        execution_plan: &request["execution_plan"],
        task_ref: &task_ref,
        initial_artifacts: request["initial_artifacts"].as_list().unwrap(),
    };
    store.initialize(&value)
}
