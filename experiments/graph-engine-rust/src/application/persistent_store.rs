use super::{
    error::{EngineError, EngineResult},
    state::{FaultHook, GraphSession},
};
use crate::{
    compat::{self, PyString, SerializationProfile, Value},
    domain::{contracts::ContractError, ids},
};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct StateFileInfo {
    pub device: u64,
    pub inode: u128,
    pub size: u64,
    pub regular: bool,
    pub directory: bool,
    pub symlink: bool,
    pub owner: Option<u32>,
    pub mode: u32,
}

pub trait StatePlatform {
    fn windows(&self) -> bool;
    fn spelling(&self, path: &Path) -> PyString;
    fn absolute(&self, path: &Path) -> EngineResult<PathBuf>;
    fn resolve(&self, path: &Path, strict: bool) -> EngineResult<PathBuf>;
    fn ensure_safe(&self, path: &Path, stop: Option<&Path>) -> EngineResult<()>;
    fn info(&self, path: &Path, follow: bool) -> EngineResult<StateFileInfo>;
    fn exists(&self, path: &Path) -> bool;
    fn create_directory(&self, path: &Path, parents: bool) -> EngineResult<()>;
    fn set_mode(&self, path: &Path, mode: u32) -> EngineResult<()>;
    fn effective_user(&self) -> Option<u32>;
    fn entries(&self, path: &Path) -> EngineResult<Vec<PathBuf>>;
    fn read_text(&self, path: &Path) -> EngineResult<PyString>;
    fn write_synced(&self, path: &Path, bytes: &[u8], exclusive: bool) -> EngineResult<()>;
    fn remove_file(&self, path: &Path) -> EngineResult<()>;
    fn replace(&self, source: &Path, target: &Path) -> EngineResult<()>;
    fn flush_file(&self, path: &Path) -> EngineResult<()>;
    fn directory_sync(&self, directory: &Path, acknowledged: bool) -> EngineResult<String>;
    fn locality(&self, path: &Path) -> EngineResult<String>;
    fn host_identity(&self) -> EngineResult<String>;
    fn process_id(&self) -> u32;
    fn process_alive(&self, process_id: &Value) -> EngineResult<bool>;
    fn temporary_name(&self) -> EngineResult<String>;
    fn utc_now(&self) -> EngineResult<String>;
}

pub struct FinalState<'a> {
    pub run_id: &'a str,
    pub operation_id: &'a str,
    pub journal: &'a str,
    pub durability: &'a str,
    pub detail: &'a str,
    pub locality: &'a str,
    pub device: u64,
    pub inode: u128,
}
pub trait PersistentSession:
    GraphSession
    + super::usage_store::UsageSession
    + super::graph_ledger::GraphLedgerReadPort
    + super::graph_writes::GraphLedgerWritePort
    + super::artifacts::ArtifactLedger
{
    fn first_run(&self) -> EngineResult<Option<Value>>;
    fn incomplete_rows(&self) -> EngineResult<(Vec<Value>, Vec<Value>)>;
    fn integrity(&self) -> EngineResult<Value>;
    fn journal(&self, enable_wal: bool) -> EngineResult<Value>;
    fn set_full_synchronous(&self) -> EngineResult<()>;
    fn synchronous(&self) -> EngineResult<Value>;
    fn begin_initial(&self) -> EngineResult<()>;
    fn insert_initial_run(&self, values: &[Value]) -> EngineResult<()>;
    fn insert_initial_plan(&self, values: &[Value]) -> EngineResult<()>;
    fn insert_artifact(&self, values: &[Value]) -> EngineResult<()>;
    fn insert_budget(&self, values: &[Value]) -> EngineResult<()>;
    fn finalize_initial(&self, state: &FinalState<'_>) -> EngineResult<()>;
    fn update_initial_response(
        &self,
        run_id: &str,
        operation_id: &str,
        response_json: &[u8],
    ) -> EngineResult<()>;
    fn insert_node(&self, values: &[Value]) -> EngineResult<()>;
    fn insert_join_row(&self, values: &[Value]) -> EngineResult<()>;
    fn insert_join_member(&self, values: &[Value]) -> EngineResult<()>;
}
pub trait StateDatabase {
    fn create(&self, path: &Path) -> EngineResult<Box<dyn PersistentSession>>;
    fn connect(&self, path: &Path) -> EngineResult<Box<dyn PersistentSession>>;
    fn locking_probe(&self, path: &Path) -> EngineResult<()>;
    fn version(&self) -> &str;
}

pub struct StateStore<'a> {
    pub state_root: PathBuf,
    pub platform: &'a dyn StatePlatform,
    pub database: &'a dyn StateDatabase,
    pub fault: &'a dyn FaultHook,
}
pub struct InitializeRequest<'a> {
    pub repository: &'a Path,
    pub policy: &'a Value,
    pub policy_digest: &'a str,
    pub task: &'a Value,
    pub task_path: &'a Path,
    pub task_digest: &'a str,
    pub run_id: &'a str,
    pub operation_id: &'a str,
    pub request_digest: &'a str,
    pub acknowledge_permissions: bool,
    pub acknowledge_durability: bool,
    pub bootstrap: &'a Value,
    pub execution_plan: &'a Value,
    pub task_ref: &'a Value,
    pub initial_artifacts: &'a [Value],
}
impl<'a> StateStore<'a> {
    pub fn new(
        codex_home: Option<&Path>,
        state_root: Option<&Path>,
        skill: &Path,
        files: &dyn super::policy::PolicyFilePort,
        platform: &'a dyn StatePlatform,
        database: &'a dyn StateDatabase,
        fault: &'a dyn FaultHook,
    ) -> EngineResult<Self> {
        if codex_home.is_some() && state_root.is_some() {
            return Err(EngineError::state("STATE_ROOT_CONFLICT"));
        }
        let explicit = state_root.or(codex_home).map(|path| {
            if path.as_os_str().is_empty() {
                Path::new(".")
            } else {
                path
            }
        });
        let root = super::policy::resolve_state_root(files, explicit, skill)?;
        Self::at_root(root, platform, database, fault)
    }
    pub fn at_root(
        root: PathBuf,
        platform: &'a dyn StatePlatform,
        database: &'a dyn StateDatabase,
        fault: &'a dyn FaultHook,
    ) -> EngineResult<Self> {
        if root.as_os_str().is_empty() {
            return Err(EngineError::state("STATE_ROOT_INVALID"));
        }
        if !root.is_absolute() {
            return Err(EngineError::state("STATE_ROOT_MUST_BE_ABSOLUTE"));
        }
        Ok(Self {
            state_root: platform.absolute(&root)?,
            platform,
            database,
            fault,
        })
    }
    pub fn run_root(&self, repository_id: &str, run_id: &str) -> PathBuf {
        self.state_root
            .join("graph-runs")
            .join(ids::repository_digest(repository_id))
            .join(run_id)
    }
    pub fn inbox_root(&self, repository_id: &str, run_id: &str) -> PathBuf {
        self.state_root
            .join("graph-inbox")
            .join(ids::repository_digest(repository_id))
            .join(run_id)
    }
    pub fn db_path(&self, repository_id: &str, run_id: &str) -> PathBuf {
        self.run_root(repository_id, run_id).join("state.sqlite3")
    }
    pub fn mkdir_private(&self, path: &Path) -> EngineResult<()> {
        let files = self.platform;
        files.ensure_safe(&self.state_root, None)?;
        files.create_directory(&self.state_root, true)?;
        let relative = path
            .strip_prefix(&self.state_root)
            .map_err(|_| EngineError::python("ValueError", "path is not in the subpath"))?;
        let mut current = self.state_root.clone();
        for part in relative.components() {
            current.push(part);
            if files.exists(&current) {
                files.ensure_safe(&current, Some(&self.state_root))?;
            } else {
                files.create_directory(&current, false)?;
            }
            if !files.windows() {
                files.set_mode(&current, 0o700)?;
                self.verify_permissions(&files.info(&current, true)?, 0o700, false)?;
            }
        }
        Ok(())
    }
    fn verify_permissions(&self, info: &StateFileInfo, mode: u32, mask: bool) -> EngineResult<()> {
        if !self.platform.windows()
            && (info.owner != self.platform.effective_user()
                || if mask {
                    info.mode & mode != 0
                } else {
                    info.mode & 0o7777 != mode
                })
        {
            return Err(EngineError::state("PERMISSION_VERIFICATION_FAILED"));
        }
        Ok(())
    }
    fn private_directory(&self, path: &Path) -> EngineResult<StateFileInfo> {
        self.platform.ensure_safe(path, Some(&self.state_root))?;
        let info = self.platform.info(path, false).map_err(|error| {
            if matches!(error, EngineError::Io(_)) {
                EngineError::state("STATE_DIRECTORY_INVALID")
            } else {
                error
            }
        })?;
        if !info.directory || info.symlink {
            return Err(EngineError::state("STATE_DIRECTORY_INVALID"));
        }
        self.verify_permissions(&info, 0o700, false)?;
        Ok(info)
    }
    pub fn verify_state_paths(
        &self,
        repository_id: &str,
        run_id: &str,
        expected: Option<&str>,
    ) -> EngineResult<String> {
        self.platform.ensure_safe(&self.state_root, None)?;
        let locality = self.platform.locality(&self.state_root)?;
        if expected.is_some_and(|value| value != locality) {
            return Err(EngineError::state("FILESYSTEM_IDENTITY_CHANGED"));
        }
        let run = self.private_directory(&self.run_root(repository_id, run_id))?;
        let inbox = self.inbox_root(repository_id, run_id);
        let inbox_info = self.private_directory(&inbox)?;
        if run.device != inbox_info.device {
            return Err(EngineError::state("STATE_DIRECTORY_INVALID"));
        }
        for entry in self.platform.entries(&inbox)? {
            self.platform.ensure_safe(&entry, Some(&inbox))?;
            let info = self.platform.info(&entry, false)?;
            if !info.regular || info.symlink {
                return Err(EngineError::state("INBOX_ENTRY_INVALID"));
            }
            self.verify_permissions(&info, 0o077, true)?;
        }
        Ok(locality)
    }
    pub fn verify_inbox_manifest(
        &self,
        repository_id: &str,
        run_id: &str,
        path: &Path,
    ) -> EngineResult<()> {
        self.verify_state_paths(repository_id, run_id, None)?;
        let inbox = self
            .platform
            .absolute(&self.inbox_root(repository_id, run_id))?;
        let candidate = self.platform.absolute(path)?;
        let equal = candidate.parent().is_some_and(|parent| {
            crate::compat::unicode::normcase(
                &self.platform.spelling(parent),
                self.platform.windows(),
            ) == crate::compat::unicode::normcase(
                &self.platform.spelling(&inbox),
                self.platform.windows(),
            )
        });
        if !equal {
            return Err(ContractError::new("path", "OUTSIDE_FIXED_INBOX").into());
        }
        self.platform.ensure_safe(&candidate, Some(&inbox))?;
        let info = self.platform.info(&candidate, false).map_err(|error| {
            if matches!(error, EngineError::Io(_)) {
                EngineError::from(ContractError::new("path", "NOT_REGULAR_FILE"))
            } else {
                error
            }
        })?;
        if !info.regular || info.symlink {
            return Err(ContractError::new("path", "NOT_REGULAR_FILE").into());
        }
        self.verify_permissions(&info, 0o077, true)
    }
    pub fn repository_identity(
        &self,
        repository: &Path,
    ) -> EngineResult<(StateFileInfo, PyString)> {
        let resolved = self.platform.resolve(repository, true)?;
        self.platform.ensure_safe(&resolved, None)?;
        let info = self.platform.info(&resolved, true)?;
        if !info.directory {
            return Err(ContractError::new("repo", "NOT_DIRECTORY").into());
        }
        self.platform.locality(&resolved)?;
        Ok((info, self.platform.spelling(&resolved)))
    }
    pub fn directory_sync(&self, directory: &Path, acknowledged: bool) -> EngineResult<String> {
        self.fault.observe("directory_sync")?;
        self.platform.directory_sync(directory, acknowledged)
    }
    pub fn write_incomplete_marker(
        &self,
        marker: &Path,
        payload: &Value,
        acknowledged: bool,
    ) -> EngineResult<String> {
        self.platform
            .write_synced(marker, &ids::canonical_bytes(payload)?, false)?;
        if !self.platform.windows() {
            self.platform.set_mode(marker, 0o600)?;
        }
        self.directory_sync(marker.parent().unwrap(), acknowledged)
    }
    pub fn incomplete_database_request_digest(
        &self,
        path: &Path,
        run_id: &str,
    ) -> EngineResult<Value> {
        let result = (|| {
            let connection = self.database.connect(path)?;
            if !connection.integrity()?.is_text("ok") {
                return Err(EngineError::state("INCOMPLETE_INIT_CORRUPT"));
            }
            let (runs, operations) = connection.incomplete_rows()?;
            if runs.len() != 1
                || !runs[0]["run_id"].is_text(run_id)
                || !runs[0]["authoritative"].py_eq(&Value::from(0_i64))
                || operations.len() != 1
                || !operations[0]["run_id"].is_text(run_id)
            {
                return Err(EngineError::state("INCOMPLETE_INIT_CORRUPT"));
            }
            let digest = &operations[0]["request_digest"];
            let length = match digest {
                Value::String(value) => value.0.len(),
                Value::Bytes(value) => value.len(),
                _ => {
                    return Err(EngineError::python(
                        "TypeError",
                        format!("object of type '{}' has no len()", digest.python_type()),
                    ))
                }
            };
            if length != 64 {
                return Err(EngineError::state("INCOMPLETE_INIT_CORRUPT"));
            }
            Ok(digest.clone())
        })();
        result.map_err(|error| {
            if database_error(&error) {
                EngineError::state("INCOMPLETE_INIT_CORRUPT")
            } else {
                error
            }
        })
    }
    pub fn open_run(
        &self,
        repository_id: &str,
        run_id: &str,
    ) -> EngineResult<Box<dyn PersistentSession>> {
        let path = self.db_path(repository_id, run_id);
        let locality = self.verify_state_paths(repository_id, run_id, None)?;
        self.platform.ensure_safe(&path, Some(&self.state_root))?;
        let info = match self.platform.info(&path, true) {
            Ok(info) if info.regular && !info.symlink => info,
            Ok(_) => return Err(EngineError::state("RUN_NOT_FOUND")),
            Err(EngineError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                return Err(EngineError::state("RUN_NOT_FOUND"))
            }
            Err(error) => return Err(error),
        };
        if info.size > 64 * 1024 * 1024 {
            return Err(EngineError::state("DATABASE_FILE_INVALID"));
        }
        self.verify_permissions(&info, 0o600, false)?;
        let result = (|| {
            let connection = self.database.connect(&path)?;
            let run = connection
                .first_run()?
                .ok_or_else(|| EngineError::state("NON_AUTHORITATIVE_STATE"))?;
            if !run["authoritative"].py_eq(&Value::from(1_i64)) {
                return Err(EngineError::state("NON_AUTHORITATIVE_STATE"));
            }
            if !run["database_device"].py_eq(&integer(info.device))
                || !run["database_inode"].py_eq(&integer(info.inode))
            {
                return Err(EngineError::state("DATABASE_IDENTITY_FAILED"));
            }
            if !run["local_filesystem"].is_text(&locality) {
                return Err(EngineError::state("FILESYSTEM_IDENTITY_CHANGED"));
            }
            if !connection.integrity()?.is_text("ok") {
                return Err(EngineError::state("INTEGRITY_CHECK_FAILED"));
            }
            let journal = lower(&connection.journal(false)?)?;
            if !matches!(journal.as_str(), "wal" | "delete")
                || !run["journal_mode"].is_text(&journal)
                || !connection.synchronous()?.py_eq(&Value::from(2_i64))
                || !run["synchronous"].is_text("FULL")
            {
                return Err(EngineError::state("DURABILITY_STATE_INVALID"));
            }
            if !(run["durability"].is_text("verified") || run["durability"].is_text("degraded"))
                || (run["durability"].is_text("degraded")
                    && !run["degraded_durability_ack"].truthy())
            {
                return Err(EngineError::state("DURABILITY_STATE_INVALID"));
            }
            if run["permission_verification"].is_text("DEGRADED_PERMISSION_VERIFICATION")
                && !run["degraded_permissions_ack"].truthy()
            {
                return Err(EngineError::state("PERMISSION_VERIFICATION_FAILED"));
            }
            Ok(connection)
        })();
        result.map_err(|error| {
            if database_error(&error) {
                EngineError::state("DATABASE_STATE_INVALID")
            } else {
                error
            }
        })
    }
    pub fn with_run<T>(
        &self,
        repository_id: &str,
        run_id: &str,
        action: impl FnOnce(&dyn PersistentSession) -> EngineResult<T>,
    ) -> EngineResult<T> {
        let connection = self.open_run(repository_id, run_id)?;
        action(&*connection).map_err(|error| {
            if database_error(&error) {
                EngineError::state("DATABASE_STATE_INVALID")
            } else {
                error
            }
        })
    }
    pub fn initialize(&self, request: &InitializeRequest<'_>) -> EngineResult<Value> {
        let repository_id = text(required(request.policy, "repository_id")?)?;
        let run_root = self.run_root(&repository_id, request.run_id);
        let inbox = self.inbox_root(&repository_id, request.run_id);
        self.mkdir_private(&run_root)?;
        self.mkdir_private(&inbox)?;
        let locality = self.verify_state_paths(&repository_id, request.run_id, None)?;
        let permission = if self.platform.windows() {
            if !request.acknowledge_permissions {
                return Err(EngineError::state("DEGRADED_PERMISSION_ACK_REQUIRED"));
            }
            "DEGRADED_PERMISSION_VERIFICATION"
        } else {
            "verified"
        };
        let final_path = run_root.join("state.sqlite3");
        let suffix = PyString::from(".incomplete.sqlite3.marker");
        let mut markers = self
            .platform
            .entries(&run_root)?
            .into_iter()
            .filter(|path| {
                crate::compat::unicode::normcase(
                    &self.platform.spelling(path),
                    self.platform.windows(),
                )
                .0
                .ends_with(&suffix.0)
            })
            .collect::<Vec<_>>();
        markers.sort_by_key(|path| {
            crate::compat::unicode::normcase(&self.platform.spelling(path), self.platform.windows())
                .0
        });
        let mut marker_digests = Vec::new();
        let mut marker_corrupt = false;
        for marker in &markers {
            let incomplete = match self
                .platform
                .read_text(marker)
                .and_then(|text| compat::loads_text(&text).map_err(Into::into))
            {
                Ok(value) => value,
                Err(EngineError::Io(_) | EngineError::Json(_)) => {
                    marker_corrupt = true;
                    continue;
                }
                Err(error) => return Err(error),
            };
            let digest = mapping_get(&incomplete, "request_digest")?;
            if !matches!(digest, Some(Value::String(text)) if text.0.len() == 64) {
                marker_corrupt = true;
                continue;
            }
            marker_digests.push(digest.unwrap().clone());
        }
        if self.platform.exists(&final_path) {
            let existing = self.with_run(&repository_id, request.run_id, |connection| {
                if let Some(operation) =
                    connection.operation(request.run_id, request.operation_id)?
                {
                    if !operation.request_digest.is_text(request.request_digest) {
                        return Err(EngineError::state("OPERATION_CONFLICT"));
                    }
                    let mut result = compat::loads_value(&operation.response_json)?;
                    assign(&mut result, "code", "REPLAYED".into())?;
                    return Ok(result);
                }
                Err(EngineError::state("RUN_ALREADY_EXISTS"))
            });
            match existing {
                Err(error) if state_code(&error) == Some("NON_AUTHORITATIVE_STATE") => {
                    let digest =
                        self.incomplete_database_request_digest(&final_path, request.run_id)?;
                    if !digest.is_text(request.request_digest) {
                        return Err(EngineError::state("INCOMPLETE_INIT_CONFLICT"));
                    }
                    if marker_digests.iter().any(|item| !item.py_eq(&digest)) {
                        return Err(EngineError::state("INCOMPLETE_INIT_CORRUPT"));
                    }
                    self.platform
                        .replace(&final_path, &self.failed_path(&run_root)?)?;
                }
                other => return other,
            }
        } else if marker_corrupt {
            return Err(EngineError::state("INCOMPLETE_INIT_CORRUPT"));
        } else if marker_digests
            .iter()
            .any(|item| !item.is_text(request.request_digest))
        {
            return Err(EngineError::state("INCOMPLETE_INIT_CONFLICT"));
        }
        let (repository_info, repository_path) = self.repository_identity(request.repository)?;
        let lock_path = run_root.join(".init.lock");
        let lock = object([
            ("schema_version", 1_i64.into()),
            ("host_identity", self.platform.host_identity()?.into()),
            ("process_id", integer(self.platform.process_id())),
            ("request_digest", request.request_digest.into()),
        ]);
        for attempt in 0..2 {
            match self
                .platform
                .write_synced(&lock_path, &ids::canonical_bytes(&lock)?, true)
            {
                Ok(()) => break,
                Err(EngineError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if attempt != 0 {
                        return Err(EngineError::state("INITIALIZATION_CONFLICT"));
                    }
                    let existing = match self
                        .platform
                        .read_text(&lock_path)
                        .and_then(|text| compat::loads_text(&text).map_err(Into::into))
                    {
                        Ok(value) => value,
                        Err(EngineError::Io(_) | EngineError::Json(_)) => {
                            return Err(EngineError::state("INITIALIZATION_CONFLICT"))
                        }
                        Err(error) => return Err(error),
                    };
                    let null = Value::Null;
                    let host = mapping_get(&existing, "host_identity")?.unwrap_or(&null);
                    if !host.is_text(&self.platform.host_identity()?)
                        || self.platform.process_alive(
                            mapping_get(&existing, "process_id")?.unwrap_or(&Value::from(-1_i64)),
                        )?
                    {
                        return Err(EngineError::state("INITIALIZATION_CONFLICT"));
                    }
                    self.platform.remove_file(&lock_path).map_err(|error| {
                        if matches!(error, EngineError::Io(_)) {
                            EngineError::state("INITIALIZATION_CONFLICT")
                        } else {
                            error
                        }
                    })?;
                }
                Err(error) => return Err(error),
            }
        }
        let temporary = run_root.join(format!(
            ".state.{}.incomplete.sqlite3",
            self.platform.temporary_name()?
        ));
        let marker = temporary.with_extension("sqlite3.marker");
        let result = self.initialize_locked(
            request,
            &repository_id,
            &run_root,
            &inbox,
            &final_path,
            &temporary,
            &marker,
            &markers,
            &locality,
            permission,
            &repository_info,
            &repository_path,
        );
        let _ = self.platform.remove_file(&lock_path);
        result
    }
    fn failed_path(&self, root: &Path) -> EngineResult<PathBuf> {
        Ok(root.join(format!(
            ".state.failed.{}.sqlite3",
            self.platform.temporary_name()?
        )))
    }
    #[allow(clippy::too_many_arguments)]
    fn initialize_locked(
        &self,
        request: &InitializeRequest<'_>,
        repository_id: &str,
        run_root: &Path,
        inbox: &Path,
        final_path: &Path,
        temporary: &Path,
        marker: &Path,
        old_markers: &[PathBuf],
        locality: &str,
        permission: &str,
        repository_info: &StateFileInfo,
        repository_path: &PyString,
    ) -> EngineResult<Value> {
        let result = (|| {
            let marker_sync = self.write_incomplete_marker(
                marker,
                &marker_payload(request, "INITIALIZING"),
                request.acknowledge_durability,
            )?;
            let connection = self.database.create(temporary)?;
            connection.begin_initial()?;
            self.insert_initial_rows(
                &*connection,
                request,
                repository_id,
                permission,
                repository_info,
                repository_path,
            )?;
            let mut response = object([
                ("schema_version", 1_i64.into()),
                ("ok", true.into()),
                ("code", "INITIALIZED".into()),
                ("run_id", request.run_id.into()),
                ("state_revision", 1_i64.into()),
                ("status", "initialized".into()),
                (
                    "branch",
                    compat::loads_value(required(request.bootstrap, "envelope_json")?)?,
                ),
                ("execution_plan", request.execution_plan.clone()),
                (
                    "execution_plan_digest",
                    required(request.execution_plan, "plan_digest")?.clone(),
                ),
                ("execution_plan_status", "pending".into()),
                ("approval_required", true.into()),
                ("inbox", Value::String(self.platform.spelling(inbox))),
                ("permission_verification", permission.into()),
            ]);
            connection.insert_operation(
                request.run_id,
                request.operation_id,
                request.request_digest,
                &json_bytes(&response)?,
                &1_i64.into(),
            )?;
            connection.insert_event(
                request.run_id,
                &1_i64.into(),
                "init",
                request.operation_id,
                b"{}",
            )?;
            connection.commit()?;
            drop(connection);
            self.fault.observe("locking_probe")?;
            self.database.locking_probe(temporary)?;
            if !self.platform.windows() {
                self.platform.set_mode(temporary, 0o600)?;
                self.verify_permissions(&self.platform.info(temporary, true)?, 0o600, false)?;
            }
            self.fault.observe("database_file_flush")?;
            self.platform.flush_file(temporary)?;
            let first_sync = self.directory_sync(run_root, request.acknowledge_durability)?;
            self.fault.observe("replace")?;
            self.platform.replace(temporary, final_path)?;
            self.fault.observe("post_replace")?;
            let second_sync = self.directory_sync(run_root, request.acknowledge_durability)?;
            self.fault.observe("reopen")?;
            let verified = self.database.connect(final_path)?;
            self.fault.observe("identity_verification")?;
            if !self
                .platform
                .info(final_path, true)
                .map(|info| info.regular)
                .unwrap_or(false)
            {
                return Err(EngineError::state("DATABASE_IDENTITY_FAILED"));
            }
            let database_info = self.platform.info(final_path, true)?;
            self.fault.observe("integrity_check")?;
            if !verified.integrity()?.is_text("ok") {
                return Err(EngineError::state("INTEGRITY_CHECK_FAILED"));
            }
            let journal = lower(&verified.journal(true)?)?;
            if !matches!(journal.as_str(), "wal" | "delete") {
                return Err(EngineError::state("UNSUPPORTED_JOURNAL_MODE"));
            }
            verified.set_full_synchronous()?;
            if !verified.synchronous()?.py_eq(&Value::from(2_i64)) {
                return Err(EngineError::state("SYNCHRONOUS_NOT_FULL"));
            }
            let detail = [&marker_sync, &first_sync, &second_sync]
                .into_iter()
                .find(|value| value.as_str() != "verified")
                .map(String::as_str)
                .unwrap_or("verified");
            let durability = if detail != "verified" {
                "degraded"
            } else {
                "verified"
            };
            verified.finalize_initial(&FinalState {
                run_id: request.run_id,
                operation_id: request.operation_id,
                journal: &journal,
                durability,
                detail,
                locality,
                device: database_info.device,
                inode: database_info.inode,
            })?;
            response.set("durability", durability.into());
            response.set("durability_detail", detail.into());
            verified.update_initial_response(
                request.run_id,
                request.operation_id,
                &json_bytes(&response)?,
            )?;
            verified.commit()?;
            drop(verified);
            for old in old_markers {
                let _ = self.platform.remove_file(old);
            }
            let _ = self.platform.remove_file(marker);
            Ok(response)
        })();
        if let Err(error) = &result {
            if !matches!(error, EngineError::ProcessDeath(_)) {
                let failure = state_code(error).unwrap_or("INIT_FAILURE");
                match self.write_incomplete_marker(
                    marker,
                    &marker_payload(request, failure),
                    request.acknowledge_durability,
                ) {
                    Ok(_) | Err(EngineError::Io(_) | EngineError::State(_)) => (),
                    Err(error) => return Err(error),
                }
                if self.platform.exists(final_path) {
                    if let Ok(failed) = self.failed_path(run_root) {
                        let _ = self.platform.replace(final_path, &failed);
                    }
                }
            }
        }
        result
    }
    fn insert_initial_rows(
        &self,
        connection: &dyn PersistentSession,
        request: &InitializeRequest<'_>,
        repository_id: &str,
        permission: &str,
        info: &StateFileInfo,
        repository_path: &PyString,
    ) -> EngineResult<()> {
        let compact = |value: &Value| -> EngineResult<Value> {
            Ok(
                String::from_utf8(compat::dumps(value, SerializationProfile::CLI)?)
                    .unwrap()
                    .into(),
            )
        };
        let task_json = compact(request.task)?;
        let policy_json = compact(request.policy)?;
        let run = vec![
            request.run_id.into(),
            repository_id.into(),
            ids::repository_digest(repository_id).into(),
            Value::String(repository_path.clone()),
            integer(info.device),
            integer(info.inode),
            "initialized".into(),
            1_i64.into(),
            "2.8.0".into(),
            6_i64.into(),
            request.policy_digest.into(),
            policy_json,
            request.task_digest.into(),
            request.task_ref.clone(),
            Value::String(self.platform.spelling(request.task_path)),
            task_json,
            required(request.task, "request_mode")?.clone(),
            required(request.task, "minimum_route")?.clone(),
            (if request.acknowledge_durability && self.platform.windows() {
                "degraded"
            } else {
                "verified"
            })
            .into(),
            "pending_final_verification".into(),
            permission.into(),
            i64::from(request.acknowledge_permissions).into(),
            i64::from(request.acknowledge_durability).into(),
            "delete".into(),
            "FULL".into(),
            self.database.version().into(),
            "pending".into(),
            self.platform.host_identity()?.into(),
            Value::Null,
            Value::Null,
            0_i64.into(),
            self.platform.utc_now()?.into(),
            Value::Null,
        ];
        connection.insert_initial_run(&run)?;
        let plan = vec![
            request.run_id.into(),
            required(request.execution_plan, "size")?.clone(),
            compact(request.execution_plan)?,
            required(request.execution_plan, "plan_digest")?.clone(),
            "pending".into(),
        ];
        connection.insert_initial_plan(&plan)?;
        for artifact in request.initial_artifacts {
            let values = vec![
                required(artifact, "ref")?.clone(),
                request.run_id.into(),
                required(artifact, "kind")?.clone(),
                required(artifact, "sha256")?.clone(),
                required(artifact, "size_bytes")?.clone(),
                required(artifact, "source_type")?.clone(),
                artifact.get("source_path").cloned().unwrap_or(Value::Null),
                artifact.get("content_json").cloned().unwrap_or(Value::Null),
                artifact.get("device").cloned().unwrap_or(Value::Null),
                artifact.get("inode").cloned().unwrap_or(Value::Null),
                1_i64.into(),
            ];
            connection.insert_artifact(&values)?;
        }
        connection.insert_node(&node_values(request.run_id, request.bootstrap)?)?;
        let budgets = [
            (
                "design_revisions",
                required(required(request.policy, "limits")?, "design_revisions")?,
            ),
            (
                "delivery_repairs",
                required(required(request.policy, "limits")?, "delivery_repairs")?,
            ),
            (
                "file_reads",
                required(required(request.task, "inspection_budget")?, "file_reads")?,
            ),
            (
                "discovery_commands",
                required(
                    required(request.task, "inspection_budget")?,
                    "discovery_commands",
                )?,
            ),
        ];
        for (key, value) in budgets {
            connection.insert_budget(&[request.run_id.into(), key.into(), value.clone()])?;
        }
        Ok(())
    }
}
fn integer(value: impl Into<num_bigint::BigInt>) -> Value {
    Value::Integer(value.into())
}
fn database_error(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::Storage {
            kind: "DatabaseError"
                | "OperationalError"
                | "IntegrityError"
                | "ProgrammingError"
                | "InternalError"
                | "NotSupportedError",
            ..
        }
    )
}
fn lower(value: &Value) -> EngineResult<String> {
    let Value::String(text) = value else {
        return Err(EngineError::python(
            "AttributeError",
            format!("'{}' object has no attribute 'lower'", value.python_type()),
        ));
    };
    Value::String(crate::compat::unicode::lower(text))
        .text()
        .ok_or_else(|| EngineError::python("UnicodeEncodeError", "surrogates not allowed"))
}
pub fn node_values(run_id: &str, row: &Value) -> EngineResult<Vec<Value>> {
    let mut values = Vec::new();
    for key in [
        "branch_id",
        "run_id",
        "node_instance_id",
        "node_key",
        "role",
        "stage",
        "generation",
        "mandatory",
        "specialist_tag",
        "status",
        "retry_count",
        "max_retries",
        "envelope_json",
        "parent_branch_id",
        "request_slot_id",
        "assignment_id",
        "ordinal",
        "depth",
    ] {
        let value = if key == "run_id" {
            run_id.into()
        } else if key == "mandatory" {
            Value::Integer(crate::compat::integer::coerce(required(row, key)?)?)
        } else if [
            "specialist_tag",
            "parent_branch_id",
            "request_slot_id",
            "assignment_id",
            "ordinal",
        ]
        .contains(&key)
        {
            row.get(key).cloned().unwrap_or(Value::Null)
        } else if key == "depth" {
            row.get(key).cloned().unwrap_or_else(|| 0_i64.into())
        } else {
            required(row, key)?.clone()
        };
        values.push(value);
    }
    Ok(values)
}
pub fn insert_join(
    connection: &dyn PersistentSession,
    run_id: &str,
    row: &Value,
    members: &[Value],
) -> EngineResult<()> {
    let values = vec![
        required(row, "join_id")?.clone(),
        run_id.into(),
        required(row, "join_key")?.clone(),
        required(row, "kind")?.clone(),
        required(row, "stage")?.clone(),
        required(row, "generation")?.clone(),
        "open".into(),
    ];
    connection.insert_join_row(&values)?;
    for member in members {
        connection.insert_join_member(&[
            required(row, "join_id")?.clone(),
            required(member, "branch_id")?.clone(),
            Value::Integer(crate::compat::integer::coerce(required(
                member,
                "mandatory",
            )?)?),
        ])?;
    }
    Ok(())
}
fn required<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| EngineError::python("KeyError", format!("'{key}'")))
}
fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn json_bytes(value: &Value) -> EngineResult<Vec<u8>> {
    Ok(compat::dumps(value, SerializationProfile::SORTED_STORED)?)
}
fn state_code(error: &EngineError) -> Option<&str> {
    if let EngineError::State(error) = error {
        Some(&error.code)
    } else {
        None
    }
}
fn mapping_get<'a>(value: &'a Value, key: &str) -> EngineResult<Option<&'a Value>> {
    if value.as_object().is_none() {
        Err(EngineError::python(
            "AttributeError",
            format!("'{}' object has no attribute 'get'", value.python_type()),
        ))
    } else {
        Ok(value.get(key))
    }
}
fn text(value: &Value) -> EngineResult<String> {
    value.text().ok_or_else(|| {
        EngineError::python(
            "TypeError",
            format!("expected str, got {}", value.python_type()),
        )
    })
}
fn assign(value: &mut Value, key: &str, item: Value) -> EngineResult<()> {
    if value.as_object().is_none() {
        return Err(EngineError::python(
            "TypeError",
            if matches!(value, Value::List(_)) {
                "list indices must be integers or slices, not str".to_string()
            } else {
                format!(
                    "'{}' object does not support item assignment",
                    value.python_type()
                )
            },
        ));
    }
    value.set(key, item);
    Ok(())
}
fn marker_payload(request: &InitializeRequest<'_>, failure: &str) -> Value {
    object([
        ("schema_version", 1_i64.into()),
        ("request_digest", request.request_digest.into()),
        ("operation_id", request.operation_id.into()),
        ("failure_code", failure.into()),
    ])
}
