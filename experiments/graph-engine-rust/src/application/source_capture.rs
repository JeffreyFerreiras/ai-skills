use super::{
    error::{EngineError, EngineResult},
    process::{ProcessOutcome, ProcessPort, ProcessRequest},
};
use crate::{
    compat::{self, PyString, Value},
    domain::{errors, ids, source_paths},
};
use std::{collections::BTreeMap, path::Path, time::Duration};

pub const MAX_PATHS: usize = 10000;
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: i128 = 128 * 1024 * 1024;
pub const MAX_ENUMERATION_BYTES: usize = 2 * 1024 * 1024;
pub const SOURCE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct SourceMetadata {
    pub regular: bool,
    pub symlink: bool,
    pub size: u64,
    pub mode: u32,
}
pub trait SourceFilePort {
    fn normalize_target(&self, relative: &PyString) -> EngineResult<PyString>;
    fn metadata(&self, relative: &PyString) -> EngineResult<Option<SourceMetadata>>;
    fn snapshot(&self, relative: &PyString, maximum: usize) -> EngineResult<(usize, String)>;
    fn elapsed(&self) -> Duration;
    fn windows(&self) -> bool;
}

pub trait SourceFileFactory {
    fn create<'a>(
        &'a self,
        repository: &'a Path,
        policy: &'a Value,
        files: &'a dyn super::policy::PolicyFilePort,
    ) -> Box<dyn SourceFilePort + 'a>;
}

pub fn source_roots(files: &dyn SourceFilePort, capabilities: &Value) -> EngineResult<Value> {
    let mut roots = std::collections::BTreeSet::new();
    for capability in errors::iterable_values(capabilities)? {
        if !errors::string_item(&capability, "effect")?.is_text("filesystem_read")
            || !errors::string_item(&capability, "action")?.is_text("read")
        {
            continue;
        }
        let target = errors::string_item(&capability, "target_ref")?;
        let target = match target {
            Value::String(text) => text,
            Value::Bytes(_) => {
                return Err(EngineError::python(
                    "TypeError",
                    "startswith first arg must be bytes or a tuple of bytes, not str",
                ))
            }
            _ => return Err(errors::attribute_error(target, "startswith").into()),
        };
        if target.starts_with("repo:") {
            roots.insert(files.normalize_target(&PyString(target.0[5..].to_vec()))?);
        }
    }
    Ok(Value::List(roots.into_iter().map(Value::String).collect()))
}

type Entries = BTreeMap<PyString, Vec<String>>;
fn unverifiable<T>() -> EngineResult<T> {
    Err(EngineError::state("SOURCE_UNVERIFIABLE"))
}

pub fn bounded_git(
    process: &dyn ProcessPort,
    files: &dyn SourceFilePort,
    repository: &Path,
    arguments: &[PyString],
    deadline: Duration,
    budget: &mut usize,
) -> EngineResult<Vec<u8>> {
    let Some(remaining) = deadline
        .checked_sub(files.elapsed())
        .filter(|value| !value.is_zero())
    else {
        return unverifiable();
    };
    let mut command = vec![PyString::from("git")];
    command.extend_from_slice(arguments);
    match process.capture(&ProcessRequest {
        arguments: &command,
        directory: repository,
        timeout: remaining,
        stdout_limit: Some(*budget),
        capture_stderr: false,
    }) {
        Ok(ProcessOutcome::Completed {
            exit_code: 0,
            stdout,
            ..
        }) => {
            let Some(remaining) = budget.checked_sub(stdout.len()) else {
                return unverifiable();
            };
            *budget = remaining;
            Ok(stdout)
        }
        Ok(_) | Err(EngineError::Io(_)) => unverifiable(),
        Err(error) => Err(error),
    }
}

fn string_arguments(arguments: &[&str], selectors: &[PyString]) -> Vec<PyString> {
    let mut result = arguments
        .iter()
        .map(|value| PyString::from(*value))
        .collect::<Vec<_>>();
    result.extend_from_slice(selectors);
    result
}
fn decoded_path(bytes: &[u8]) -> EngineResult<PyString> {
    match std::str::from_utf8(bytes) {
        Ok(value) => Ok(value.into()),
        Err(_) => unverifiable(),
    }
}

struct Enumeration<'a> {
    process: &'a dyn ProcessPort,
    files: &'a dyn SourceFilePort,
    repository: &'a Path,
    roots: &'a [PyString],
    selectors: &'a [PyString],
    known_inputs: &'a Value,
    deadline: Duration,
}
impl Enumeration<'_> {
    fn read(&self, budget: &mut usize) -> EngineResult<Entries> {
        let index = bounded_git(
            self.process,
            self.files,
            self.repository,
            &string_arguments(&["ls-files", "--stage", "-z", "--"], self.selectors),
            self.deadline,
            budget,
        )?;
        let untracked = bounded_git(
            self.process,
            self.files,
            self.repository,
            &string_arguments(
                &["ls-files", "--others", "--exclude-standard", "-z", "--"],
                self.selectors,
            ),
            self.deadline,
            budget,
        )?;
        let mut entries = Entries::new();
        for item in index
            .split(|byte| *byte == 0)
            .filter(|item| !item.is_empty())
        {
            let Some(tab) = item.iter().position(|byte| *byte == 9) else {
                return unverifiable();
            };
            let (metadata, path) = (&item[..tab], &item[tab + 1..]);
            if metadata.starts_with(b"160000 ") || !metadata.is_ascii() {
                return unverifiable();
            }
            entries
                .entry(decoded_path(path)?)
                .or_default()
                .push(String::from_utf8(metadata.to_vec()).unwrap());
        }
        for item in untracked
            .split(|byte| *byte == 0)
            .filter(|item| !item.is_empty())
        {
            entries.entry(decoded_path(item)?).or_default();
        }
        for root in self.roots {
            if !root.ends_with("/") {
                entries.entry(root.clone()).or_default();
            }
        }
        if self.known_inputs.truthy() {
            let ignored = bounded_git(
                self.process,
                self.files,
                self.repository,
                &string_arguments(
                    &[
                        "ls-files",
                        "--others",
                        "--ignored",
                        "--exclude-standard",
                        "-z",
                        "--",
                    ],
                    self.selectors,
                ),
                self.deadline,
                budget,
            )?;
            let ignored = ignored
                .split(|byte| *byte == 0)
                .filter(|item| !item.is_empty())
                .map(decoded_path)
                .collect::<EngineResult<Vec<_>>>()?;
            let mut known_inputs = Vec::new();
            for path in errors::iterable_values(self.known_inputs)? {
                let path = match &path {
                    Value::String(path) => path.clone(),
                    Value::Bytes(_) => {
                        return Err(EngineError::python(
                            "TypeError",
                            "endswith first arg must be bytes or a tuple of bytes, not str",
                        ));
                    }
                    value => return Err(errors::attribute_error(value, "endswith").into()),
                };
                if !path.ends_with("/") && !entries.contains_key(&path) {
                    return unverifiable();
                }
                known_inputs.push(path);
            }
            for path in &ignored {
                if !entries.contains_key(path)
                    && known_inputs
                        .iter()
                        .any(|root| source_paths::contains(root, path, self.files.windows()))
                {
                    return unverifiable();
                }
            }
        }
        if entries.len() > MAX_PATHS {
            return unverifiable();
        }
        Ok(entries)
    }
}

pub fn capture_source(
    repository: &Path,
    roots: &[PyString],
    outputs: &[Value],
    known_inputs: &[PyString],
    files: &dyn SourceFilePort,
    process: &dyn ProcessPort,
) -> EngineResult<Value> {
    capture_source_values(
        repository,
        &Value::list(roots.iter().cloned().map(Value::String)),
        &Value::List(outputs.to_vec()),
        &Value::list(known_inputs.iter().cloned().map(Value::String)),
        files,
        process,
    )
}

pub fn capture_source_values(
    repository: &Path,
    roots: &Value,
    outputs: &Value,
    known_inputs: &Value,
    files: &dyn SourceFilePort,
    process: &dyn ProcessPort,
) -> EngineResult<Value> {
    let deadline = files.elapsed() + SOURCE_TIMEOUT;
    let mut budget = MAX_ENUMERATION_BYTES;
    if !roots.truthy() {
        return unverifiable();
    }
    let result = (|| {
        let mut normalized = errors::iterable_values(roots)?
            .iter()
            .map(|root| {
                let reference = crate::domain::python_operations::add(&"repo:".into(), root)?;
                let Value::String(reference) = &reference else {
                    unreachable!("string concatenation returns a string")
                };
                files.normalize_target(&PyString(reference.0[5..].to_vec()))
            })
            .collect::<EngineResult<Vec<_>>>()?;
        normalized.sort();
        normalized.dedup();
        let selectors = normalized
            .iter()
            .map(|root| {
                let mut selector = PyString::from(":(literal)");
                let end = root
                    .0
                    .iter()
                    .rposition(|point| *point != 47)
                    .map_or(0, |index| index + 1);
                selector.0.extend_from_slice(&root.0[..end]);
                selector
            })
            .collect::<Vec<_>>();
        let head = bounded_git(
            process,
            files,
            repository,
            &string_arguments(&["rev-parse", "HEAD"], &[]),
            deadline,
            &mut budget,
        )?;
        if !head.is_ascii() {
            return unverifiable();
        }
        let whitespace = |point: &u8| matches!(point, 9..=13 | 28..=32);
        let first = head
            .iter()
            .position(|byte| !whitespace(byte))
            .unwrap_or(head.len());
        let last = head
            .iter()
            .rposition(|byte| !whitespace(byte))
            .map_or(first, |index| index + 1);
        let head = String::from_utf8(head[first..last].to_vec()).unwrap();
        let enumeration = Enumeration {
            process,
            files,
            repository,
            roots: &normalized,
            selectors: &selectors,
            known_inputs,
            deadline,
        };
        let entries = enumeration.read(&mut budget)?;
        let excluded = errors::iterable_values(outputs)?
            .iter()
            .map(|output| {
                let path = errors::string_item(output, "path")?;
                errors::require_hashable(path)?;
                Ok(path.clone())
            })
            .collect::<EngineResult<Vec<_>>>()?;
        let mut records = Vec::new();
        let mut total = 0i128;
        for (relative, index) in &entries {
            if files.elapsed() > deadline
                || !normalized
                    .iter()
                    .any(|root| source_paths::contains(root, relative, files.windows()))
            {
                return unverifiable();
            }
            files.normalize_target(relative)?;
            let excluded = excluded
                .iter()
                .any(|path| path.py_eq(&Value::String(relative.clone())));
            let index = Value::list(index.iter().cloned().map(Value::from));
            let Some(metadata) = files.metadata(relative)? else {
                if !excluded {
                    records.push(Value::object([
                        ("path", Value::String(relative.clone())),
                        ("index", index),
                        ("exists", false.into()),
                    ]));
                }
                continue;
            };
            if !metadata.regular || metadata.symlink || metadata.size > MAX_FILE_BYTES {
                return unverifiable();
            }
            if excluded {
                continue;
            }
            total += i128::from(metadata.size);
            if total > MAX_TOTAL_BYTES {
                return unverifiable();
            }
            let (size, digest) = files.snapshot(relative, MAX_FILE_BYTES as usize)?;
            total += size as i128 - i128::from(metadata.size);
            if total > MAX_TOTAL_BYTES {
                return unverifiable();
            }
            records.push(Value::object([
                ("path", Value::String(relative.clone())),
                ("index", index),
                ("exists", true.into()),
                ("mode", i64::from(metadata.mode & 0o7777).into()),
                ("sha256", digest.into()),
            ]));
        }
        if entries != enumeration.read(&mut budget)? || files.elapsed() > deadline {
            return unverifiable();
        }
        let count = records.len();
        let digest = ids::sha256_bytes(&ids::canonical_bytes(&Value::object([
            ("head", head.into()),
            ("paths", Value::List(records)),
        ]))?);
        Ok(Value::object([
            ("sha256", digest.into()),
            (
                "roots",
                Value::list(normalized.into_iter().map(Value::String)),
            ),
            ("path_count", (count as i64).into()),
        ]))
    })();
    match result {
        Err(EngineError::Contract(_) | EngineError::Io(_)) => unverifiable(),
        Err(EngineError::Python {
            kind:
                "ValueError"
                | "UnicodeError"
                | "UnicodeEncodeError"
                | "UnicodeDecodeError"
                | "UnicodeTranslateError",
            ..
        }) => unverifiable(),
        Err(EngineError::Json(error))
            if matches!(
                error.kind,
                "ValueError"
                    | "UnicodeEncodeError"
                    | "UnicodeDecodeError"
                    | "UnicodeTranslateError"
            ) =>
        {
            unverifiable()
        }
        result => result,
    }
}

#[derive(Default)]
pub struct StatusSourceCache {
    successful: BTreeMap<String, Value>,
}
impl StatusSourceCache {
    pub fn capture(
        &mut self,
        roots: &[PyString],
        outputs: &[Value],
        capture: impl FnOnce() -> EngineResult<Value>,
    ) -> EngineResult<Value> {
        self.capture_values(
            &Value::list(roots.iter().cloned().map(Value::String)),
            &Value::List(outputs.to_vec()),
            capture,
        )
    }
    pub fn capture_values(
        &mut self,
        roots: &Value,
        outputs: &Value,
        capture: impl FnOnce() -> EngineResult<Value>,
    ) -> EngineResult<Value> {
        let key = ids::sha256_bytes(&compat::dumps(
            &Value::object([
                ("roots", Value::List(errors::iterable_values(roots)?)),
                ("outputs", Value::List(errors::iterable_values(outputs)?)),
            ]),
            compat::SerializationProfile::CANONICAL,
        )?);
        if let Some(snapshot) = self
            .successful
            .get(&key)
            .filter(|value| !matches!(value, Value::Null))
        {
            return Ok(snapshot.clone());
        }
        let snapshot = capture()?;
        self.successful.insert(key, snapshot.clone());
        Ok(snapshot)
    }
}
