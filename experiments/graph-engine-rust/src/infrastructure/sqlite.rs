use crate::application::{
    error::{EngineError, EngineResult},
    state::{GraphSession, OperationRecord, RunPatch, RunRecord},
};
use crate::compat::Value;
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use rusqlite::{types::ValueRef, Connection, OptionalExtension};
use std::path::Path;

pub const SCHEMA: &str = include_str!("state_schema.sql");
pub fn connection_uri(path: &Path) -> EngineResult<String> {
    let resolved = crate::infrastructure::filesystem::resolve_python_path(path, false)?;
    let (prefix, bytes) = {
        #[cfg(windows)]
        {
            use std::path::{Component, Prefix};
            let mut text = crate::infrastructure::filesystem::os_string(resolved.as_os_str());
            for point in &mut text.0 {
                if *point == 92 {
                    *point = 47;
                }
            }
            let (prefix, start) = match resolved.components().next() {
                Some(Component::Prefix(prefix)) => match prefix.kind() {
                    Prefix::Disk(drive) => (format!("file:///{}:", drive as char), 2),
                    _ => ("file:".into(), 0),
                },
                _ => ("file://".into(), 0),
            };
            let mut bytes = Vec::new();
            for &point in &text.0[start..] {
                if point < 0x80 {
                    bytes.push(point as u8);
                } else if point < 0x800 {
                    bytes.extend([(0xc0 | (point >> 6)) as u8, (0x80 | (point & 63)) as u8]);
                } else if point < 0x10000 {
                    bytes.extend([
                        (0xe0 | (point >> 12)) as u8,
                        (0x80 | ((point >> 6) & 63)) as u8,
                        (0x80 | (point & 63)) as u8,
                    ]);
                } else {
                    bytes.extend([
                        (0xf0 | (point >> 18)) as u8,
                        (0x80 | ((point >> 12) & 63)) as u8,
                        (0x80 | ((point >> 6) & 63)) as u8,
                        (0x80 | (point & 63)) as u8,
                    ]);
                }
            }
            (prefix, bytes)
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            (
                "file://".to_string(),
                resolved.as_os_str().as_bytes().to_vec(),
            )
        }
    };
    let mut uri = prefix;
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri.push_str("?mode=rw");
    Ok(uri)
}

pub fn schema_contract_snapshot() -> rusqlite::Result<Value> {
    let connection = rusqlite::Connection::open_in_memory()?;
    connection.execute_batch(SCHEMA)?;
    let mut statement = connection
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name")?;
    let rows = statement
        .query_map([], |row| {
            Ok(Value::list([
                row.get::<_, String>(0)?.into(),
                row.get::<_, String>(1)?.into(),
                row.get::<_, String>(2)?.into(),
                row.get::<_, Option<String>>(3)?
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            ]))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Value::List(rows))
}
pub fn sqlite_version() -> &'static str {
    rusqlite::version()
}

pub fn sqlite_failure(error: rusqlite::Error) -> EngineError {
    let (kind, code) = match &error {
        rusqlite::Error::SqliteFailure(code, _) => {
            let kind = match code.code {
                rusqlite::ErrorCode::ConstraintViolation => "IntegrityError",
                rusqlite::ErrorCode::DatabaseBusy
                | rusqlite::ErrorCode::DatabaseLocked
                | rusqlite::ErrorCode::ReadOnly
                | rusqlite::ErrorCode::OperationInterrupted
                | rusqlite::ErrorCode::SystemIoFailure
                | rusqlite::ErrorCode::DiskFull
                | rusqlite::ErrorCode::CannotOpen
                | rusqlite::ErrorCode::FileLockingProtocolFailed
                | rusqlite::ErrorCode::SchemaChanged
                | rusqlite::ErrorCode::Unknown => "OperationalError",
                _ => "DatabaseError",
            };
            (kind, Some(code.extended_code))
        }
        _ => ("DatabaseError", None),
    };
    EngineError::Storage {
        kind,
        message: error.to_string(),
        code,
    }
}
fn integer(value: &BigInt) -> EngineResult<i64> {
    value.to_i64().ok_or_else(|| {
        EngineError::python(
            "OverflowError",
            "Python int too large to convert to SQLite INTEGER",
        )
    })
}
pub(crate) fn row_value(row: &rusqlite::Row<'_>) -> EngineResult<Value> {
    let names = row.as_ref().column_names();
    let mut values = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let value = match row.get_ref(index).map_err(sqlite_failure)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(value) => value.into(),
            ValueRef::Real(value) => Value::Float(value),
            ValueRef::Blob(bytes) => Value::Bytes(bytes.to_vec()),
            ValueRef::Text(bytes) => {
                let text = std::str::from_utf8(bytes).map_err(|_| {
                    let end = bytes
                        .iter()
                        .position(|&byte| byte == 0)
                        .unwrap_or(bytes.len());
                    EngineError::Storage {
                        kind: "OperationalError",
                        message: format!(
                            "Could not decode to UTF-8 column '{name}' with text '{}'",
                            String::from_utf8_lossy(&bytes[..end])
                        ),
                        code: None,
                    }
                })?;
                Value::from(text)
            }
        };
        values.push(value);
    }
    let mut fields: Vec<(crate::compat::PyString, Value)> = Vec::new();
    for name in &names {
        if fields.iter().any(|(key, _)| key.equals(name)) {
            continue;
        }
        let first = names
            .iter()
            .position(|candidate| {
                candidate == name
                    || (candidate.is_ascii()
                        && name.is_ascii()
                        && candidate.eq_ignore_ascii_case(name))
            })
            .unwrap();
        fields.push(((*name).into(), values[first].clone()));
    }
    Ok(Value::Object(fields))
}
fn parameter(value: &Value, index: usize) -> EngineResult<rusqlite::types::Value> {
    use rusqlite::types::Value as SqlValue;
    Ok(match value {
        Value::Null => SqlValue::Null,
        Value::Bool(value) => SqlValue::Integer(i64::from(*value)),
        Value::Integer(value) => SqlValue::Integer(integer(value)?),
        Value::Float(value) => SqlValue::Real(*value),
        Value::ConstantNaN => SqlValue::Real(f64::NAN),
        Value::String(text) => {
            SqlValue::Text(String::from_utf8(crate::compat::encode_utf8(text)?).unwrap())
        }
        Value::Bytes(bytes) => SqlValue::Blob(bytes.clone()),
        _ => {
            return Err(EngineError::Storage {
                kind: "ProgrammingError",
                message: format!(
                    "Error binding parameter {}: type '{}' is not supported",
                    index + 1,
                    value.python_type()
                ),
                code: None,
            })
        }
    })
}
pub(crate) fn parameters(
    values: &[Value],
    expected: usize,
) -> EngineResult<Vec<rusqlite::types::Value>> {
    if values.len() != expected {
        return Err(EngineError::Storage{kind:"ProgrammingError",message:format!("Incorrect number of bindings supplied. The current statement uses {expected}, and there are {} supplied.",values.len()),code:None});
    }
    values
        .iter()
        .enumerate()
        .map(|(index, value)| parameter(value, index))
        .collect()
}

pub struct SqliteSession {
    connection: Connection,
}
impl SqliteSession {
    pub fn create(path: &Path) -> EngineResult<Self> {
        let connection = Connection::open(path).map_err(sqlite_failure)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sqlite_failure)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
            )
            .map_err(sqlite_failure)?;
        connection.execute_batch(SCHEMA).map_err(sqlite_failure)?;
        Ok(Self { connection })
    }
    pub fn in_memory() -> EngineResult<Self> {
        let connection = Connection::open_in_memory().map_err(sqlite_failure)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sqlite_failure)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
            .map_err(sqlite_failure)?;
        connection.execute_batch(SCHEMA).map_err(sqlite_failure)?;
        Ok(Self { connection })
    }
    pub fn from_connection(connection: Connection) -> Self {
        Self { connection }
    }
    pub fn connect_existing(path: &Path) -> EngineResult<Self> {
        let path = connection_uri(path)?;
        let connection = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(sqlite_failure)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sqlite_failure)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
            .map_err(sqlite_failure)?;
        Ok(Self { connection })
    }
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    pub fn query_values(&self, sql: &str, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        let mut statement = self.connection.prepare(sql).map_err(sqlite_failure)?;
        let parameters = self::parameters(parameters, statement.parameter_count())?;
        let mut rows = statement
            .query(rusqlite::params_from_iter(parameters.iter()))
            .map_err(sqlite_failure)?;
        let mut values = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite_failure)? {
            values.push(row_value(row)?);
        }
        Ok(values)
    }
    pub fn execute_values(&self, sql: &str, parameters: &[Value]) -> EngineResult<usize> {
        let mut statement = self.connection.prepare(sql).map_err(sqlite_failure)?;
        let parameters = self::parameters(parameters, statement.parameter_count())?;
        statement
            .execute(rusqlite::params_from_iter(parameters.iter()))
            .map_err(sqlite_failure)
    }
}
impl GraphSession for SqliteSession {
    fn begin_immediate(&self) -> EngineResult<()> {
        self.connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|error| {
                let mapped = sqlite_failure(error);
                if matches!(
                    mapped,
                    EngineError::Storage {
                        kind: "OperationalError",
                        ..
                    }
                ) {
                    EngineError::state("TRANSACTION_CONFLICT")
                } else {
                    mapped
                }
            })
    }
    fn commit(&self) -> EngineResult<()> {
        self.connection
            .execute_batch("COMMIT")
            .map_err(sqlite_failure)
    }
    fn rollback(&self) -> EngineResult<()> {
        if self.connection.is_autocommit() {
            Ok(())
        } else {
            self.connection
                .execute_batch("ROLLBACK")
                .map_err(sqlite_failure)
        }
    }
    fn operation(&self, run: &str, operation: &str) -> EngineResult<Option<OperationRecord>> {
        Ok(self
            .query_values(
                "SELECT * FROM operations WHERE run_id=? AND operation_id=?",
                &[run.into(), operation.into()],
            )?
            .into_iter()
            .next()
            .map(|row| OperationRecord {
                request_digest: row["request_digest"].clone(),
                response_json: row["response_json"].clone(),
                resulting_revision: row["resulting_revision"].clone(),
            }))
    }
    fn run(&self, run: &str) -> EngineResult<Option<RunRecord>> {
        Ok(self
            .query_values("SELECT * FROM runs WHERE run_id=?", &[run.into()])?
            .into_iter()
            .next()
            .map(|fields| RunRecord { fields }))
    }
    fn update_revision(&self, run: &str, revision: &BigInt) -> EngineResult<()> {
        self.connection
            .execute(
                "UPDATE runs SET state_revision=? WHERE run_id=?",
                rusqlite::params![integer(revision)?, run],
            )
            .map_err(sqlite_failure)?;
        Ok(())
    }
    fn insert_operation(
        &self,
        run: &str,
        operation: &str,
        request_digest: &str,
        response_json: &[u8],
        revision: &BigInt,
    ) -> EngineResult<()> {
        let response = std::str::from_utf8(response_json)
            .map_err(|_| EngineError::python("UnicodeEncodeError", "surrogates not allowed"))?;
        self.connection
            .execute(
                "INSERT INTO operations VALUES(?,?,?,?,?)",
                rusqlite::params![run, operation, request_digest, response, integer(revision)?],
            )
            .map_err(sqlite_failure)?;
        Ok(())
    }
    fn insert_event(
        &self,
        run: &str,
        revision: &BigInt,
        command: &str,
        operation: &str,
        detail_json: &[u8],
    ) -> EngineResult<()> {
        let detail = std::str::from_utf8(detail_json)
            .map_err(|_| EngineError::python("UnicodeEncodeError", "surrogates not allowed"))?;
        self.connection.execute("INSERT INTO events(run_id,revision,event_type,source_id,detail_json) VALUES(?,?,?,?,?)",rusqlite::params![run,integer(revision)?,command,operation,detail]).map_err(sqlite_failure)?;
        Ok(())
    }
    fn patch_run(&self, run: &str, patch: &RunPatch) -> EngineResult<()> {
        match patch {
            RunPatch::BlockedReason(reason) => {
                self.connection
                    .execute(
                        "UPDATE runs SET blocked_reason=? WHERE run_id=?",
                        rusqlite::params![reason, run],
                    )
                    .map_err(sqlite_failure)?;
            }
            RunPatch::Status(status) => {
                self.connection
                    .execute(
                        "UPDATE runs SET status=? WHERE run_id=?",
                        [status.as_str(), run],
                    )
                    .map_err(sqlite_failure)?;
            }
        }
        Ok(())
    }
    fn event_exists(&self, run: &str, operation: &str) -> EngineResult<bool> {
        self.connection
            .query_row(
                "SELECT 1 FROM events WHERE run_id=? AND source_id=?",
                [run, operation],
                |_| Ok(true),
            )
            .optional()
            .map(|value| value.unwrap_or(false))
            .map_err(sqlite_failure)
    }
}
