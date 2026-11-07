use super::{
    error::{EngineError, EngineResult},
    helper_validation as validation,
    persistent_store::StatePlatform,
    policy::PolicyFilePort,
    snapshots::{self, SnapshotFailure},
    state::FaultHook,
};
use crate::{
    compat::{PyString, Value},
    domain::{
        contracts::{self, ContractError},
        helper_register as helper, ids,
    },
};
use std::path::{Path, PathBuf};

pub trait HelperFileTransactions {
    fn temporary_root(&self) -> EngineResult<PathBuf>;
    fn atomic_write(&self, path: &Path, payload: &[u8], fault: &dyn FaultHook) -> EngineResult<()>;
    fn exclusive_lock(
        &self,
        path: &Path,
        action: &mut dyn FnMut() -> EngineResult<Value>,
    ) -> EngineResult<Value>;
}
pub trait RegisterTransaction {
    fn record(&self) -> &Value;
    fn record_mut(&mut self) -> &mut Value;
    fn commit(&mut self) -> EngineResult<()>;
}
pub trait HelperRegisterPersistence {
    fn read_bound(&self, register_path: &Path, context: &Value) -> EngineResult<(Value, Value)>;
    fn transaction(
        &self,
        register_path: &Path,
        context: &Value,
        action: &mut dyn FnMut(&mut dyn RegisterTransaction) -> EngineResult<Value>,
    ) -> EngineResult<Value>;
}
pub struct HelperRegisterRepository<'a> {
    pub files: &'a dyn PolicyFilePort,
    pub platform: &'a dyn StatePlatform,
    pub transactions: &'a dyn HelperFileTransactions,
    pub installation: &'a Path,
    pub fault: &'a dyn FaultHook,
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
fn contract(field: &str, code: &str) -> EngineError {
    ContractError::new(field, code).into()
}
impl HelperRegisterRepository<'_> {
    fn private_directory(&self, path: &Path) -> EngineResult<()> {
        self.files.make_directory(path, true, 0o700)?;
        self.files.ensure_safe(path, None)?;
        if !self.files.windows() {
            self.files.chmod(path, 0o700)?;
            if self.platform.info(path, true)?.mode & 0o7777 != 0o700 {
                return Err(failure("PERMISSION_VERIFICATION_FAILED"));
            }
        }
        Ok(())
    }
    fn identity(&self, path: &Path) -> EngineResult<Value> {
        let resolved = self.files.resolve(path, true)?;
        self.files.ensure_safe(&resolved, None)?;
        let info = self.platform.info(&resolved, true)?;
        if !info.directory {
            return Err(contract("repo", "NOT_DIRECTORY"));
        }
        self.platform.locality(&resolved)?;
        Ok(Value::object([
            ("device", Value::Integer(info.device.into())),
            ("inode", Value::Integer(info.inode.into())),
            ("path", Value::String(self.files.spelling(&resolved))),
        ]))
    }
    fn read(&self, path: &Path) -> EngineResult<Value> {
        let snapshot = snapshots::safe_json_snapshot(
            self.files,
            path,
            &[path.parent().unwrap().to_path_buf()],
            helper::MAX_RECORD_BYTES,
        )
        .map_err(snapshot_error)?;
        validation::validate_record(&snapshot.parsed.unwrap(), self.files)
    }
    fn write(&self, path: &Path, value: &Value) -> EngineResult<()> {
        let payload = ids::canonical_bytes(value)?;
        if payload.len() > helper::MAX_RECORD_BYTES {
            return Err(failure("REGISTER_TOO_LARGE"));
        }
        self.transactions.atomic_write(path, &payload, self.fault)
    }
    fn verify_test_repository(&self, allowance: &Value, repository: &Path) -> EngineResult<()> {
        let Some(test_mode) = allowance.get("test_mode") else {
            return Ok(());
        };
        let declared = self
            .files
            .from_string(test_mode["disposable_repository"].as_string().unwrap())?;
        let temporary = self
            .files
            .resolve(&self.transactions.temporary_root()?, false)?;
        let installation = self.installation;
        let ancestor = |root: &Path, path: &Path| root != path && self.files.under(path, root);
        if !self.files.same_path(&declared, repository)
            || !ancestor(&temporary, repository)
            || self.files.same_path(installation, repository)
            || ancestor(installation, repository)
            || ancestor(repository, installation)
            || self.files.is_file(&repository.join(".git"))?
        {
            return Err(failure("TEST_REPOSITORY_REQUIRED"));
        }
        Ok(())
    }
    pub fn initialize(
        &self,
        state_root: &Path,
        repository: &Path,
        run_id: &Value,
        plan_path: &Path,
        allowance_path: &Path,
        observation_path: &Path,
    ) -> EngineResult<Value> {
        if !state_root.is_absolute() {
            return Err(failure("STATE_ROOT_MUST_BE_ABSOLUTE"));
        }
        let run = contracts::opaque(run_id, "run_id")?;
        let root = self.files.absolute(state_root)?;
        self.private_directory(&root)?;
        self.platform.locality(&root)?;
        let root = self.files.resolve(&root, true)?;
        let info = self.platform.info(&root, true)?;
        let identity = Value::object([
            ("device", Value::Integer(info.device.into())),
            ("inode", Value::Integer(info.inode.into())),
            ("path", Value::String(self.files.spelling(&root))),
        ]);
        let repository = self.files.resolve(repository, true)?;
        let repository_identity = self.identity(&repository)?;
        let repository_key = validation::repository_key(&repository_identity)?;
        let plan_snapshot = snapshots::safe_json_snapshot(
            self.files,
            plan_path,
            &[plan_path.parent().unwrap().to_path_buf()],
            helper::MAX_RECORD_BYTES,
        )
        .map_err(snapshot_error)?;
        let plan = validation::validate_plan(&plan_snapshot.parsed.unwrap(), &run.clone().into())?;
        let attachment = &plan["helper_allowance"];
        let (expected, allowance_root) =
            self.allowance_location(attachment["ref"].as_string().unwrap(), &repository, &root)?;
        if !self.files.same_path(
            &self.files.resolve(allowance_path, true)?,
            &self.files.resolve(&expected, true)?,
        ) {
            return Err(contract("allowance", "ALLOWANCE_REF_MISMATCH"));
        }
        let allowance_snapshot = snapshots::safe_json_snapshot(
            self.files,
            allowance_path,
            &[allowance_root],
            helper::MAX_RECORD_BYTES,
        )
        .map_err(snapshot_error)?;
        if !attachment["sha256"].is_text(&allowance_snapshot.digest) {
            return Err(contract("allowance", "INPUT_DIGEST_MISMATCH"));
        }
        let absolute = |text: &PyString| {
            self.files
                .from_string(text)
                .is_ok_and(|path| path.is_absolute())
        };
        let allowance = helper::validate_allowance(
            &allowance_snapshot.parsed.unwrap(),
            &run.clone().into(),
            Some(&plan["host"]),
            None,
            false,
            &absolute,
        )?;
        self.verify_test_repository(&allowance, &repository)?;
        let observation_snapshot = snapshots::safe_json_snapshot(
            self.files,
            observation_path,
            &[observation_path.parent().unwrap().to_path_buf()],
            helper::MAX_RECORD_BYTES,
        )
        .map_err(snapshot_error)?;
        let observation = helper::validate_host_observation(&observation_snapshot.parsed.unwrap())?;
        let register_id = validation::register_key(
            &identity,
            &repository_key.clone().into(),
            &run.clone().into(),
        )?;
        self.private_directory(&root.join("helper-registers"))?;
        let directory = root.join("helper-registers").join(&register_id);
        self.private_directory(&directory)?;
        let path = directory.join("register.json");
        let lock = directory.join("register.lock");
        let context = Value::object([
            ("schema_version", 1_i64.into()),
            ("register_id", register_id.into()),
            ("register_path", Value::String(self.files.spelling(&path))),
            ("state_root_identity", identity),
            ("repository_digest", repository_key.into()),
            ("run_id", run.into()),
            ("plan_digest", plan["plan_digest"].clone()),
            ("allowance_ref", attachment["ref"].clone()),
            ("allowance_digest", allowance_snapshot.digest.into()),
            (
                "allowance_payload_digest",
                ids::sha256_bytes(&ids::canonical_bytes(&allowance)?).into(),
            ),
            (
                "host_observation_digest",
                observation_snapshot.digest.into(),
            ),
            (
                "host_observation_payload_digest",
                ids::sha256_bytes(&ids::canonical_bytes(&observation)?).into(),
            ),
        ]);
        let zero = || {
            Value::Object(
                helper::LIMIT_KEYS
                    .iter()
                    .map(|key| ((*key).into(), 0_i64.into()))
                    .collect(),
            )
        };
        let record = Value::object([
            ("schema_version", 1_i64.into()),
            ("context", context.clone()),
            ("host", plan["host"].clone()),
            ("repository_identity", repository_identity),
            ("allowance_ref", attachment["ref"].clone()),
            ("allowance", allowance.clone()),
            ("host_observation", observation),
            ("usage", zero()),
            ("active_concurrency", 0_i64.into()),
            (
                "active_resources",
                Value::Object(
                    allowance["resources"]
                        .as_list()
                        .unwrap()
                        .iter()
                        .map(|row| (row["key"].as_string().unwrap().clone(), 0_i64.into()))
                        .collect(),
                ),
            ),
            (
                "assignment_usage",
                Value::Object(
                    allowance["assignments"]
                        .as_list()
                        .unwrap()
                        .iter()
                        .map(|row| (row["assignment_id"].as_string().unwrap().clone(), zero()))
                        .collect(),
                ),
            ),
            ("reservations", Value::object([])),
        ]);
        let code = self.transactions.exclusive_lock(&lock, &mut || {
            if self.files.exists(&path, true)? {
                let existing = self.read(&path)?;
                if [
                    "context",
                    "host",
                    "repository_identity",
                    "allowance_ref",
                    "allowance",
                    "host_observation",
                ]
                .iter()
                .any(|key| !existing[key].container_eq(&record[key]))
                {
                    return Err(failure("INITIALIZATION_CONFLICT"));
                }
                Ok("REPLAYED".into())
            } else {
                self.write(&path, &record)?;
                Ok("INITIALIZED".into())
            }
        })?;
        let mut result = Value::object([
            ("ok", true.into()),
            ("code", code),
            ("register_path", Value::String(self.files.spelling(&path))),
            ("context", context),
        ]);
        extend(&mut result, &test_mode_metadata(&record)?);
        Ok(result)
    }
    pub fn allowance_location(
        &self,
        reference: &PyString,
        repository: &Path,
        runtime: &Path,
    ) -> EngineResult<(PathBuf, PathBuf)> {
        let Some(index) = reference.0.iter().position(|point| *point == 58) else {
            return Err(EngineError::python(
                "ValueError",
                "not enough values to unpack (expected 2, got 1)",
            ));
        };
        let prefix = PyString(reference.0[..index].to_vec());
        if !prefix.equals("repo") && !prefix.equals("runtime") {
            return Err(contract("allowance_ref", "FILESYSTEM_REF_REQUIRED"));
        }
        let root = if prefix.equals("repo") {
            repository
        } else {
            runtime
        };
        let relative = contracts::lexical_relative(
            &Value::String(PyString(reference.0[index + 1..].to_vec())),
            "allowance_ref",
        )?;
        Ok((
            root.join(self.files.from_string(&relative)?),
            root.to_path_buf(),
        ))
    }
    fn bound_path(&self, path: &Path, value: &Value) -> EngineResult<(PathBuf, Value)> {
        let context = validation::validate_context(value, self.files)?;
        if self.files.is_symlink(path)? {
            return Err(failure("REGISTER_BINDING_MISMATCH"));
        }
        let path = self.files.resolve(path, true)?;
        if !self.files.same_path(
            &path,
            &self
                .files
                .from_string(context["register_path"].as_string().unwrap())?,
        ) {
            return Err(failure("REGISTER_BINDING_MISMATCH"));
        }
        let identity = &context["state_root_identity"];
        let current = self
            .identity(
                &self
                    .files
                    .from_string(identity["path"].as_string().unwrap())?,
            )
            .map_err(binding_error)?;
        if !current.container_eq(identity) {
            return Err(failure("REGISTER_BINDING_MISMATCH"));
        }
        self.files.ensure_safe(
            &path,
            Some(
                &self
                    .files
                    .from_string(current["path"].as_string().unwrap())?,
            ),
        )?;
        Ok((path, context))
    }
    fn bound_record(&self, path: &Path, value: &Value) -> EngineResult<(Value, Value)> {
        let (path, context) = self.bound_path(path, value)?;
        let record = self.read(&path)?;
        if !record["context"].container_eq(&context) {
            return Err(failure("REGISTER_BINDING_MISMATCH"));
        }
        let identity = &record["repository_identity"];
        let current = self
            .identity(
                &self
                    .files
                    .from_string(identity["path"].as_string().unwrap())?,
            )
            .map_err(binding_error)?;
        if !current.container_eq(identity) {
            return Err(failure("REGISTER_BINDING_MISMATCH"));
        }
        let repository = self
            .files
            .from_string(identity["path"].as_string().unwrap())?;
        let (allowance_path, allowance_root) = self.allowance_location(
            context["allowance_ref"].as_string().unwrap(),
            &repository,
            &self
                .files
                .from_string(context["state_root_identity"]["path"].as_string().unwrap())?,
        )?;
        let checked = (|| {
            let snapshot = snapshots::safe_json_snapshot(
                self.files,
                &allowance_path,
                &[allowance_root],
                helper::MAX_RECORD_BYTES,
            )
            .map_err(snapshot_error)?;
            let absolute = |text: &PyString| {
                self.files
                    .from_string(text)
                    .is_ok_and(|path| path.is_absolute())
            };
            let allowance = helper::validate_allowance(
                &snapshot.parsed.unwrap(),
                &context["run_id"],
                Some(&record["host"]),
                None,
                true,
                &absolute,
            )?;
            Ok::<_, EngineError>((snapshot.digest, allowance))
        })()
        .map_err(binding_error)?;
        if !context["allowance_digest"].is_text(&checked.0)
            || !checked.1.container_eq(&record["allowance"])
        {
            return Err(failure("REGISTER_BINDING_MISMATCH"));
        }
        self.verify_test_repository(&checked.1, &repository)?;
        Ok((record, context))
    }
}
fn binding_error(error: EngineError) -> EngineError {
    if matches!(error, EngineError::Contract(_) | EngineError::Io(_)) {
        failure("REGISTER_BINDING_MISMATCH")
    } else {
        error
    }
}
struct FileRegisterTransaction<'a> {
    record: Value,
    writer: &'a dyn Fn(&Value) -> EngineResult<()>,
}
impl RegisterTransaction for FileRegisterTransaction<'_> {
    fn record(&self) -> &Value {
        &self.record
    }
    fn record_mut(&mut self) -> &mut Value {
        &mut self.record
    }
    fn commit(&mut self) -> EngineResult<()> {
        (self.writer)(&self.record)
    }
}
impl HelperRegisterPersistence for HelperRegisterRepository<'_> {
    fn read_bound(&self, path: &Path, context: &Value) -> EngineResult<(Value, Value)> {
        self.bound_record(path, context)
    }
    fn transaction(
        &self,
        path: &Path,
        value: &Value,
        action: &mut dyn FnMut(&mut dyn RegisterTransaction) -> EngineResult<Value>,
    ) -> EngineResult<Value> {
        let (path, context) = self.bound_path(path, value)?;
        self.transactions
            .exclusive_lock(&path.parent().unwrap().join("register.lock"), &mut || {
                let (record, _) = self.bound_record(&path, &context)?;
                let writer = |record: &Value| self.write(&path, record);
                action(&mut FileRegisterTransaction {
                    record,
                    writer: &writer,
                })
            })
    }
}
pub fn extend(target: &mut Value, fields: &Value) {
    for (key, value) in fields.as_object().unwrap() {
        target.set(&key.utf8().unwrap(), value.clone());
    }
}
pub fn test_mode_metadata(record: &Value) -> EngineResult<Value> {
    if record["allowance"].get("test_mode").is_none() {
        return Ok(Value::object([]));
    }
    let observations = &record["host_observation"]["capabilities"];
    let mut required = std::collections::BTreeSet::new();
    for assignment in record["allowance"]["assignments"].as_list().unwrap() {
        for capability in assignment["required_host_capabilities"].as_list().unwrap() {
            required.insert(capability.text().unwrap());
        }
    }
    let mut unenforced = Vec::new();
    for capability in required {
        if helper::CONFINEMENT_CAPABILITIES.contains(&capability.as_str()) {
            let observation = observations.get(&capability);
            if observation.is_none()
                || !observation.unwrap()["status"].is_text("verified")
                || !helper::trusted_observation_source(
                    observation.unwrap()["source"].as_string().unwrap(),
                )
            {
                unenforced.push(capability.into());
            }
        }
    }
    Ok(Value::object([
        ("execution_mode", "cooperative_test".into()),
        ("production_evidence", false.into()),
        ("unenforced_capabilities", Value::List(unenforced)),
        (
            "isolation_notice",
            "Disposable evaluation only; instructions and checks do not enforce isolation.".into(),
        ),
    ]))
}
