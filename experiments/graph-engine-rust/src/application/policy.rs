use super::{
    error::{EngineError, EngineResult, PolicySnapshotSource, SnapshotJsonTraceSite},
    snapshots::{self, FileSnapshotPort, Snapshot, SnapshotFailure},
};
use crate::{
    compat::{unicode, PyString, Value},
    domain::{contracts::ContractError, ids, policy, policy_validation},
};
use std::path::{Path, PathBuf};

pub const RUNTIME_DIRECTORY: &str = "software-engineering-graph";
pub const POLICY_MAXIMUM: usize = 256 * 1024;

pub trait PolicyFilePort: FileSnapshotPort {
    fn absolute(&self, path: &Path) -> EngineResult<PathBuf>;
    fn resolve(&self, path: &Path, strict: bool) -> EngineResult<PathBuf>;
    fn from_string(&self, text: &PyString) -> EngineResult<PathBuf>;
    fn spelling(&self, path: &Path) -> PyString;
    fn same_path(&self, left: &Path, right: &Path) -> bool;
    fn under(&self, candidate: &Path, root: &Path) -> bool;
    fn exists(&self, path: &Path, follow_links: bool) -> EngineResult<bool>;
    fn is_directory(&self, path: &Path) -> EngineResult<bool>;
    fn is_file(&self, path: &Path) -> EngineResult<bool>;
    fn is_symlink(&self, path: &Path) -> EngineResult<bool>;
    fn is_reparse(&self, path: &Path) -> EngineResult<bool>;
    fn ensure_safe(&self, path: &Path, stop: Option<&Path>) -> EngineResult<()>;
    fn entries(&self, directory: &Path) -> EngineResult<Vec<PathBuf>>;
    fn make_directory(&self, path: &Path, parents: bool, mode: u32) -> EngineResult<()>;
    fn chmod(&self, path: &Path, mode: u32) -> EngineResult<()>;
    fn write_new_sync(&self, path: &Path, bytes: &[u8], mode: u32) -> EngineResult<()>;
    fn same_file(&self, left: &Path, right: &Path) -> EngineResult<bool>;
    fn environment(&self, key: &str) -> Option<PyString>;
    fn windows(&self) -> bool;
}

fn snapshot_error(error: SnapshotFailure) -> EngineError {
    match error {
        SnapshotFailure::Contract(error) => error.into(),
        SnapshotFailure::Io(error) => error.into(),
        SnapshotFailure::Json(error) => error.into(),
    }
}
pub fn path_from_value(files: &dyn PolicyFilePort, value: &Value) -> EngineResult<PathBuf> {
    let text = value.as_string().ok_or_else(|| {
        super::error::EngineError::python(
            "TypeError",
            format!(
                "argument should be a str or an os.PathLike object where __fspath__ returns a str, not '{}'",
                value.python_type()
            ),
        )
    })?;
    files.from_string(text)
}

fn snapshot(
    files: &dyn PolicyFilePort,
    path: &Path,
    root: &Path,
    source: PolicySnapshotSource,
) -> EngineResult<Snapshot> {
    snapshots::safe_json_snapshot(files, path, &[root.to_path_buf()], POLICY_MAXIMUM).map_err(
        |error| snapshot_error(error).with_snapshot_trace(SnapshotJsonTraceSite::Policy(source)),
    )
}
fn absolute_root(files: &dyn PolicyFilePort, path: &Path) -> EngineResult<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(EngineError::state("STATE_ROOT_INVALID"));
    }
    if !path.is_absolute() {
        return Err(EngineError::state("STATE_ROOT_MUST_BE_ABSOLUTE"));
    }
    files.absolute(path)
}
pub fn resolve_state_root(
    files: &dyn PolicyFilePort,
    explicit: Option<&Path>,
    skill: &Path,
) -> EngineResult<PathBuf> {
    if let Some(path) = explicit {
        return absolute_root(files, path);
    }
    for (key, suffix) in [
        ("SOFTWARE_ENGINEERING_GRAPH_HOME", false),
        ("SOFTWARE_ENGINEERING_GRAPH_STATE_HOME", false),
        ("XDG_STATE_HOME", true),
    ] {
        if let Some(value) = files.environment(key) {
            let path = absolute_root(files, &files.from_string(&value)?)?;
            return Ok(if suffix {
                path.join(RUNTIME_DIRECTORY)
            } else {
                path
            });
        }
    }
    let installed = files.resolve(skill, false)?;
    let parent = installed
        .parent()
        .ok_or_else(|| EngineError::python("ValueError", "installed skill has no parent"))?;
    let profile = if parent
        .file_name()
        .is_some_and(|name| files.spelling(Path::new(name)).equals("skills"))
    {
        parent
            .parent()
            .ok_or_else(|| EngineError::python("ValueError", "profile has no parent"))?
    } else {
        parent
    };
    let name = profile
        .file_name()
        .ok_or_else(|| EngineError::python("ValueError", "profile has an empty name"))?;
    let mut local_name = files.spelling(Path::new(name));
    local_name.0.extend(".local".chars().map(u32::from));
    Ok(profile
        .with_file_name(files.from_string(&local_name)?)
        .join(RUNTIME_DIRECTORY))
}

pub fn installed_codex_home(files: &dyn PolicyFilePort, source: &Path) -> EngineResult<PathBuf> {
    let resolved = files.resolve(source, false)?;
    resolved
        .ancestors()
        .skip(1)
        .find(|path| {
            path.file_name()
                .is_some_and(|name| files.spelling(Path::new(name)).equals(".codex"))
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| EngineError::state("CODEX_PROFILE_ROOT_NOT_FOUND"))
}

fn identity_bytes(files: &dyn PolicyFilePort, repository: &Path) -> EngineResult<Vec<u8>> {
    let path = files.resolve(repository, true)?;
    let mut text = files.spelling(&path);
    if files.windows() {
        text = unicode::casefold(&text);
    }
    match text.utf8() {
        Ok(value) => Ok(value.into_bytes()),
        Err(first) => {
            let mut last = first + 1;
            while last < text.len() && (0xd800..=0xdfff).contains(&text.0[last]) {
                last += 1;
            }
            let message = if last == first + 1 {
                format!("'utf-8' codec can't encode character '\\u{:04x}' in position {first}: surrogates not allowed", text.0[first])
            } else {
                format!("'utf-8' codec can't encode characters in position {first}-{}: surrogates not allowed", last - 1)
            };
            Err(EngineError::python("UnicodeEncodeError", message))
        }
    }
}
pub fn profile_policy_path(
    files: &dyn PolicyFilePort,
    repository: &Path,
    runtime: &Path,
) -> EngineResult<PathBuf> {
    let digest = ids::sha256_bytes(&identity_bytes(files, repository)?);
    Ok(runtime
        .join("policies")
        .join(format!("repo-{}.json", &digest[..24])))
}
fn relative_path(
    files: &dyn PolicyFilePort,
    root: &Path,
    text: &PyString,
) -> EngineResult<PathBuf> {
    let mut relative = text.clone();
    while relative.ends_with("/") {
        relative.0.pop();
    }
    Ok(root.join(files.from_string(&relative)?))
}
pub fn repository_target(
    files: &dyn PolicyFilePort,
    repository: &Path,
    target: &Value,
    denied: &[Value],
) -> EngineResult<PyString> {
    let normalized = policy_validation::repository_target_spelling(target, denied)?;
    let root = files.resolve(repository, true)?;
    let candidate = relative_path(files, repository, &PyString(normalized.0[5..].to_vec()))?;
    files.ensure_safe(&candidate, Some(&root))?;
    if !files.under(&files.resolve(&candidate, false)?, &root) {
        return Err(ContractError::new("capability.target_ref", "OUTSIDE_REPOSITORY").into());
    }
    Ok(normalized)
}
pub fn default_profile_policy(
    files: &dyn PolicyFilePort,
    repository: &Path,
) -> EngineResult<Value> {
    let repository_id = format!(
        "profile-{}",
        &ids::sha256_bytes(&identity_bytes(files, repository)?)[..24]
    );
    let mut entries = files.entries(repository)?;
    entries.sort_by_key(|entry| {
        unicode::lower(&files.spelling(Path::new(entry.file_name().unwrap())))
    });
    let mut roots = Vec::new();
    let denials: Vec<_> = policy::DENIALS
        .iter()
        .map(|&pattern| Value::from(pattern))
        .collect();
    for entry in entries {
        let name = files.spelling(Path::new(entry.file_name().unwrap()));
        if name.starts_with(".")
            || name.equals("AGENTS.md")
            || unicode::normcase(&name, files.windows())
                == unicode::normcase(
                    &PyString::from("engineering-graph-artifacts"),
                    files.windows(),
                )
            || files.is_symlink(&entry)?
            || (files.windows() && files.is_reparse(&entry)?)
        {
            continue;
        }
        let mut relative = name;
        if files.is_directory(&entry)? {
            relative.0.push(47);
        }
        let mut target = PyString::from("repo:");
        target.0.extend(&relative.0);
        match crate::domain::contracts::lexical_relative(
            &Value::String(relative.clone()),
            "implementation_roots",
        )
        .map_err(EngineError::from)
        .and_then(|_| repository_target(files, repository, &Value::String(target), &denials))
        {
            Ok(_) => (),
            Err(EngineError::Contract(_)) => continue,
            Err(error) => return Err(error),
        }
        if files.is_directory(&entry)? || files.is_file(&entry)? {
            roots.push(Value::String(relative));
        }
        if roots.len() == 128 {
            break;
        }
    }
    Ok(policy::default_profile_policy(&repository_id, &roots))
}
pub fn ensure_profile_policy(
    files: &dyn PolicyFilePort,
    repository: &Path,
    runtime: &Path,
    seed: Option<&[u8]>,
) -> EngineResult<PathBuf> {
    let path = profile_policy_path(files, repository, runtime)?;
    files.ensure_safe(runtime, None)?;
    files.make_directory(runtime, true, 0o700)?;
    files.make_directory(path.parent().unwrap(), false, 0o700)?;
    files.ensure_safe(path.parent().unwrap(), None)?;
    if !files.windows() {
        files.chmod(runtime, 0o700)?;
        files.chmod(path.parent().unwrap(), 0o700)?;
    }
    if !files.exists(&path, false)? {
        let generated;
        let bytes = if let Some(bytes) = seed {
            bytes
        } else {
            generated = ids::canonical_bytes(&default_profile_policy(files, repository)?)?;
            &generated
        };
        files.write_new_sync(&path, bytes, 0o600)?;
    }
    Ok(path)
}
pub fn policy_snapshot(
    files: &dyn PolicyFilePort,
    repository: &Path,
    skill: &Path,
    runtime: Option<&Path>,
) -> EngineResult<(PathBuf, PathBuf, Snapshot)> {
    policy_snapshot_traced(files, repository, skill, runtime)
        .map_err(EngineError::without_graphctl_trace)
}

fn policy_snapshot_traced(
    files: &dyn PolicyFilePort,
    repository: &Path,
    skill: &Path,
    runtime: Option<&Path>,
) -> EngineResult<(PathBuf, PathBuf, Snapshot)> {
    let root = files.absolute(repository)?;
    let installed = files.resolve(skill, true)?;
    let home = resolve_state_root(files, runtime, &installed)?;
    let path = profile_policy_path(files, &root, &home)?;
    let legacy = profile_policy_path(files, &root, &installed)?;
    let selected = if !files.exists(&path, false)?
        && (files.same_path(&home, &installed.join("state"))
            || files.exists(&home.join("graph-runs"), true)?)
        && files.exists(&legacy, true)?
    {
        snapshot(files, &legacy, &installed, PolicySnapshotSource::Legacy)?
    } else {
        let default_home = match resolve_state_root(files, None, &installed) {
            Ok(value) => Some(value),
            Err(EngineError::State(error)) if runtime.is_some() => {
                let _ = error;
                None
            }
            Err(error) => return Err(error),
        };
        let mut seed = None;
        if let Some(default) = default_home {
            if !files.same_path(&home, &default) && !files.exists(&path, false)? {
                let source = profile_policy_path(files, &root, &default)?;
                if files.exists(&source, true)? {
                    seed =
                        Some(snapshot(files, &source, &default, PolicySnapshotSource::Seed)?.data);
                }
            }
        }
        let selected_path = ensure_profile_policy(files, &root, &home, seed.as_deref())?;
        snapshot(files, &selected_path, &home, PolicySnapshotSource::Runtime)?
    };
    Ok((root, home, selected))
}

fn configured_denials(value: &Value) -> Vec<Value> {
    value["denied_patterns"]
        .as_list()
        .map(<[Value]>::to_vec)
        .unwrap_or_else(|| {
            policy::DENIALS
                .iter()
                .map(|&pattern| Value::from(pattern))
                .collect()
        })
}
fn unique_paths(value: &Value, field: &str, allow_empty: bool) -> EngineResult<Vec<Value>> {
    let paths = value
        .as_list()
        .ok_or_else(|| ContractError::new(field, "INVALID_OR_DUPLICATE"))?;
    if (!allow_empty && paths.is_empty())
        || crate::domain::errors::set_values(paths)?.len() != paths.len()
    {
        return Err(ContractError::new(field, "INVALID_OR_DUPLICATE").into());
    }
    Ok(paths.to_vec())
}
fn validate_roots(
    files: &dyn PolicyFilePort,
    repository: &Path,
    value: &mut Value,
    version: i64,
) -> EngineResult<()> {
    use crate::domain::contracts;
    let mut roots = value["artifact_roots"].clone();
    if roots.as_object().is_none() {
        return Err(ContractError::new("artifact_roots", "INVALID_OBJECT").into());
    }
    let kinds = if version == 3 {
        &["repo", "profile", "runtime"][..]
    } else {
        &["repo", "profile"][..]
    };
    contracts::require_keys(&roots, kinds, kinds, "artifact_roots")?;
    if version == 3 && !roots["repo"].py_eq(&Value::List(vec![])) {
        return Err(ContractError::new("artifact_roots.repo", "RUNTIME_ARTIFACTS_REQUIRED").into());
    }
    let artifact_root = format!("artifacts/{}/", value["repository_id"].text().unwrap());
    if version == 3 && !roots["runtime"].py_eq(&Value::list([artifact_root.clone().into()])) {
        return Err(ContractError::new("artifact_roots.runtime", "RUNTIME_ROOT_FORBIDDEN").into());
    }
    for &kind in kinds {
        let field = format!("artifact_roots.{kind}");
        let configured = unique_paths(&roots[kind], &field, kind == "repo" && version == 3)?;
        let mut normalized = Vec::new();
        for path in &configured {
            let relative = contracts::lexical_relative(path, &field)?;
            if !relative.ends_with("/") {
                return Err(ContractError::new(&field, "DIRECTORY_ROOT_REQUIRED").into());
            }
            if kind == "profile" && !relative.starts_with("references/") {
                return Err(
                    ContractError::new("artifact_roots.profile", "PROFILE_ROOT_FORBIDDEN").into(),
                );
            }
            if kind == "runtime" && !relative.starts_with(&artifact_root) {
                return Err(
                    ContractError::new("artifact_roots.runtime", "RUNTIME_ROOT_FORBIDDEN").into(),
                );
            }
            let legacy_root = if kind == "repo" {
                "docs/"
            } else {
                "references/"
            };
            if version == 1 && !relative.starts_with(legacy_root) {
                return Err(ContractError::new(&field, "ENGINE_ROOT_EXCEEDED").into());
            }
            if kind == "repo" && version >= 2 {
                let mut target = PyString::from("repo:");
                target.0.extend(&relative.0);
                repository_target(
                    files,
                    repository,
                    &Value::String(target),
                    &configured_denials(value),
                )?;
                let candidate = relative_path(files, repository, &relative)?;
                if files.exists(&candidate, true)? && !files.is_directory(&candidate)? {
                    return Err(ContractError::new(
                        "artifact_roots.repo",
                        "DIRECTORY_ROOT_REQUIRED",
                    )
                    .into());
                }
            }
            normalized.push(Value::String(relative));
        }
        if version >= 2 {
            if policy::has_location_duplicates(&normalized, files.windows()) {
                return Err(ContractError::new(&field, "INVALID_OR_DUPLICATE").into());
            }
            roots.set(kind, Value::List(normalized));
        }
    }
    value.set("artifact_roots", roots.clone());
    if version >= 2 {
        let configured =
            unique_paths(&value["implementation_roots"], "implementation_roots", true)?;
        let mut normalized = Vec::new();
        for path in configured {
            let relative = contracts::lexical_relative(&path, "implementation_roots")?;
            if !relative.ends_with("/") && relative.contains("/") {
                return Err(ContractError::new(
                    "implementation_roots",
                    "DIRECTORY_OR_ROOT_FILE_REQUIRED",
                )
                .into());
            }
            let mut target = PyString::from("repo:");
            target.0.extend(&relative.0);
            repository_target(
                files,
                repository,
                &Value::String(target),
                &configured_denials(value),
            )?;
            let candidate = relative_path(files, repository, &relative)?;
            if relative.ends_with("/")
                && files.exists(&candidate, true)?
                && !files.is_directory(&candidate)?
            {
                return Err(
                    ContractError::new("implementation_roots", "DIRECTORY_ROOT_REQUIRED").into(),
                );
            }
            if !relative.ends_with("/")
                && files.exists(&candidate, true)?
                && !files.is_file(&candidate)?
            {
                return Err(
                    ContractError::new("implementation_roots", "ROOT_FILE_REQUIRED").into(),
                );
            }
            normalized.push(Value::String(relative));
        }
        if policy::has_location_duplicates(&normalized, files.windows()) {
            return Err(ContractError::new("implementation_roots", "INVALID_OR_DUPLICATE").into());
        }
        for implementation in &normalized {
            if roots["repo"].as_list().unwrap().iter().any(|artifact| {
                policy::roots_overlap(
                    implementation.as_string().unwrap(),
                    artifact.as_string().unwrap(),
                    files.windows(),
                )
            }) {
                return Err(ContractError::new(
                    "implementation_roots",
                    "ARTIFACT_IMPLEMENTATION_OVERLAP",
                )
                .into());
            }
        }
        value.set("implementation_roots", Value::List(normalized));
    }
    Ok(())
}
fn validate_capabilities(
    files: &dyn PolicyFilePort,
    repository: &Path,
    value: &mut Value,
    version: i64,
    commands: &[String],
) -> EngineResult<()> {
    use crate::domain::errors;
    let mut capabilities = value["role_capabilities"].clone();
    if capabilities.as_object().is_none_or(|items| {
        items.len() != policy::ROLES.len()
            || policy::ROLES
                .iter()
                .any(|role| capabilities.get(role).is_none())
    }) {
        return Err(ContractError::new("role_capabilities", "ENGINE_AUTHORITY_CHANGED").into());
    }
    for (role, configured) in capabilities.as_object().unwrap().to_vec() {
        let role = role.utf8().unwrap();
        let field = format!("role_capabilities.{role}");
        let rows = configured
            .as_list()
            .ok_or_else(|| ContractError::new(&field, "INVALID_LIST"))?;
        let mut normalized = Vec::new();
        let mut tuples = Vec::new();
        for original in rows {
            let mut item = original.clone();
            let (effect, action, mut target) =
                policy_validation::validate_capability_shape(&item, &role, commands)?;
            if version >= 2 && effect.as_string().unwrap().starts_with("filesystem") {
                let target_text = target.as_string().unwrap();
                let normalized_target = if target_text.starts_with("runtime:") && version == 3 {
                    policy_validation::runtime_target(
                        &target,
                        value["artifact_roots"]["runtime"].as_list().unwrap(),
                        files.windows(),
                    )?
                } else {
                    repository_target(files, repository, &target, &configured_denials(value))?
                };
                target = Value::String(normalized_target);
                item.set("target_ref", target.clone());
            }
            errors::require_hashable(&action)?;
            if !policy_validation::role_capability_allowed(
                value,
                &Value::from(role.as_str()),
                &item,
                files.windows(),
            )? {
                return Err(ContractError::new(&field, "ENGINE_AUTHORITY_EXCEEDED").into());
            }
            tuples.push(Value::list([effect, action, target]));
            normalized.push(item);
        }
        if tuples
            .iter()
            .enumerate()
            .any(|(index, item)| tuples[..index].iter().any(|previous| item.py_eq(previous)))
        {
            return Err(ContractError::new(&field, "DUPLICATE_VALUE").into());
        }
        capabilities.set(&role, Value::List(normalized));
    }
    value.set("role_capabilities", capabilities);
    Ok(())
}
pub fn validate_policy_record(
    files: &dyn PolicyFilePort,
    repository: &Path,
    value: &mut Value,
) -> EngineResult<()> {
    let version = policy_validation::validate_header(value)?;
    validate_roots(files, repository, value, version)?;
    policy_validation::validate_topology(value)?;
    policy_validation::validate_limits(&value["limits"])?;
    policy_validation::validate_artifact_kinds(&value["artifact_kinds"])?;
    let commands = policy_validation::validate_checks(&value["required_checks"], version)?;
    validate_capabilities(files, repository, value, version, &commands)?;
    policy_validation::validate_denials_and_delegation(value)?;
    Ok(())
}
pub fn load_policy(
    files: &dyn PolicyFilePort,
    repository: &Path,
    skill: &Path,
    runtime: Option<&Path>,
) -> EngineResult<(Value, Snapshot)> {
    load_policy_traced(files, repository, skill, runtime)
        .map_err(EngineError::without_graphctl_trace)
}

pub(super) fn load_policy_traced(
    files: &dyn PolicyFilePort,
    repository: &Path,
    skill: &Path,
    runtime: Option<&Path>,
) -> EngineResult<(Value, Snapshot)> {
    let (root, _, mut selected) = policy_snapshot_traced(files, repository, skill, runtime)?;
    let mut value = selected.parsed.take().unwrap();
    validate_policy_record(files, &root, &mut value)?;
    selected.parsed = Some(value.clone());
    Ok((value, selected))
}

fn joined_parts(
    files: &dyn PolicyFilePort,
    root: &Path,
    parts: &[&[u32]],
) -> EngineResult<PathBuf> {
    let mut path = root.to_path_buf();
    for part in parts {
        path.push(files.from_string(&PyString(part.to_vec()))?);
    }
    Ok(path)
}
pub fn prove_capability_case_identity(
    files: &dyn PolicyFilePort,
    repository: &Path,
    runtime: Option<&Path>,
    target: &Value,
    outer: &Value,
) -> EngineResult<()> {
    use crate::domain::errors::attribute_error;
    if target.py_eq(outer) {
        return Ok(());
    }
    let outer_text = outer
        .as_string()
        .ok_or_else(|| attribute_error(outer, "endswith"))?;
    if outer_text.ends_with("/") {
        let target_text = target
            .as_string()
            .ok_or_else(|| attribute_error(target, "startswith"))?;
        if target_text.0.starts_with(&outer_text.0) {
            return Ok(());
        }
    }
    let target_text = target
        .as_string()
        .ok_or_else(|| attribute_error(target, "split"))?;
    let split = |text: &PyString| {
        text.split_once(":").ok_or_else(|| {
            EngineError::python(
                "ValueError",
                "not enough values to unpack (expected 2, got 1)",
            )
        })
    };
    let (scheme, mut relative) = split(target_text)?;
    let (outer_scheme, mut outer_relative) = split(outer_text)?;
    let unproven =
        || ContractError::new("capability.target_ref", "CAPABILITY_CASE_IDENTITY_UNPROVEN");
    let root = if scheme.equals("repo") {
        Some(repository)
    } else if scheme.equals("runtime") {
        runtime
    } else {
        None
    };
    let Some(root) = root.filter(|_| scheme == outer_scheme) else {
        return Err(unproven().into());
    };
    while relative.ends_with("/") {
        relative.0.pop();
    }
    while outer_relative.ends_with("/") {
        outer_relative.0.pop();
    }
    let outer_parts: Vec<_> = outer_relative.0.split(|&point| point == 47).collect();
    let candidate_parts: Vec<_> = relative
        .0
        .split(|&point| point == 47)
        .take(outer_parts.len())
        .collect();
    let candidate = joined_parts(files, root, &candidate_parts)?;
    let ceiling = joined_parts(files, root, &outer_parts)?;
    let proof = (|| -> EngineResult<bool> {
        files.ensure_safe(&candidate, Some(root))?;
        files.ensure_safe(&ceiling, Some(root))?;
        files.same_file(&candidate, &ceiling)
    })();
    let equivalent = match proof {
        Ok(value) => value,
        Err(EngineError::Io(_) | EngineError::Contract(_)) => false,
        Err(EngineError::Python { kind, .. })
            if [
                "ValueError",
                "OSError",
                "FileNotFoundError",
                "PermissionError",
                "NotADirectoryError",
                "IsADirectoryError",
            ]
            .contains(&kind) =>
        {
            false
        }
        Err(error) => return Err(error),
    };
    if !equivalent {
        return Err(unproven().into());
    }
    Ok(())
}
pub fn validate_task_capability_paths(
    files: &dyn PolicyFilePort,
    repository: &Path,
    task: &Value,
    value: &Value,
    runtime: Option<&Path>,
    projection: Option<&Value>,
) -> EngineResult<()> {
    use crate::domain::{
        contracts::{self, ProjectionError},
        errors::{attribute_error, iterable_values, string_item},
    };
    let capabilities = string_item(string_item(task, "authority")?, "capabilities")?;
    for capability in iterable_values(capabilities)? {
        let effect = string_item(&capability, "effect")?;
        let effect = effect
            .as_string()
            .ok_or_else(|| attribute_error(effect, "startswith"))?;
        if !effect.starts_with("filesystem") {
            continue;
        }
        let raw_target = string_item(&capability, "target_ref")?;
        let denied = string_item(value, "denied_patterns")?;
        let target = policy_validation::filesystem_target(raw_target, denied)?;
        if target.starts_with("repo:") {
            repository_target(
                files,
                repository,
                &Value::String(target.clone()),
                &iterable_values(denied)?,
            )?;
        } else if target.starts_with("runtime:") {
            if let Some(runtime) = runtime {
                let candidate = relative_path(files, runtime, &PyString(target.0[8..].to_vec()))?;
                files.ensure_safe(&candidate, Some(runtime))?;
                if !files.under(
                    &files.resolve(&candidate, false)?,
                    &files.resolve(runtime, false)?,
                ) {
                    return Err(ContractError::new(
                        "capability.target_ref",
                        "OUTSIDE_ALLOWED_ROOT",
                    )
                    .into());
                }
            }
        }
        let roles = string_item(value, "role_capabilities")?;
        let roles = roles
            .as_object()
            .ok_or_else(|| attribute_error(roles, "values"))?;
        for (_, ceilings) in roles {
            for ceiling in iterable_values(ceilings)? {
                let within = contracts::projected_capability_within(
                    &capability,
                    &ceiling,
                    projection,
                    files.windows(),
                )
                .map_err(|error| match error {
                    ProjectionError::Validation(error) => EngineError::from(error),
                    ProjectionError::InvalidVersion => {
                        EngineError::python("ValueError", "CAPABILITY_PROJECTION_VERSION_INVALID")
                    }
                })?;
                if within {
                    prove_capability_case_identity(
                        files,
                        repository,
                        runtime,
                        &Value::String(target.clone()),
                        string_item(&ceiling, "target_ref")?,
                    )?;
                }
            }
        }
    }
    Ok(())
}
