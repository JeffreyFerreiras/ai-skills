use super::{
    error::{EngineError, EngineResult},
    policy::{self, PolicyFilePort},
    snapshots::{IntegerFileSnapshotPort, SnapshotFailure},
};
use crate::{
    compat::{self, PyString, Value},
    domain::{
        contracts::{self, ContractError},
        errors,
        evidence::{self, VerifiedArtifact},
        ids,
    },
};
use std::path::{Path, PathBuf};

pub trait ArtifactLedger {
    fn immutable_artifact(&self, reference: &Value, digest: &Value) -> EngineResult<Option<Value>>;
    fn artifact_by_reference(&self, reference: &Value) -> EngineResult<Option<Value>>;
    fn insert_artifact_values(&self, values: &[Value]) -> EngineResult<()>;
}
pub struct ArtifactResolver<'a> {
    pub files: &'a dyn PolicyFilePort,
    pub snapshots: &'a dyn IntegerFileSnapshotPort,
    pub repository: &'a Path,
    pub skill: &'a Path,
    pub installation: &'a Path,
    pub policy: &'a Value,
    pub runtime: Option<&'a Path>,
    pub ledger: Option<&'a dyn ArtifactLedger>,
}
fn item<'a>(record: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(evidence::item(record, key)?)
}
fn snapshot_error(error: SnapshotFailure) -> EngineError {
    match error {
        SnapshotFailure::Contract(error) => error.into(),
        SnapshotFailure::Io(error) => error.into(),
        SnapshotFailure::Json(error) => error.into(),
    }
}
fn split_reference(reference: &PyString) -> (&[u32], &[u32]) {
    let index = reference.0.iter().position(|&point| point == 58).unwrap();
    (&reference.0[..index], &reference.0[index + 1..])
}
impl ArtifactResolver<'_> {
    fn reference_path(
        &self,
        prefix: &[u32],
        identity: &PyString,
    ) -> EngineResult<(PathBuf, Vec<PathBuf>, PyString)> {
        let repo = prefix == [114, 101, 112, 111];
        let runtime = prefix == [114, 117, 110, 116, 105, 109, 101];
        let profile = prefix == [112, 114, 111, 102, 105, 108, 101];
        let profile_prefix = PyString::from("software-engineering-graph/");
        let version_three = || -> EngineResult<bool> {
            Ok(item(self.policy, "schema_version")?.py_eq(&3_i64.into()))
        };
        let (relative, root, permitted, normalized_prefix) = if repo {
            let relative =
                contracts::lexical_relative(&Value::String(identity.clone()), "artifact_ref")?;
            evidence::policy_path_allowed(&Value::String(relative.clone()), self.policy)?;
            let permitted = if version_three()? {
                item(self.policy, "implementation_roots")?
            } else {
                item(item(self.policy, "artifact_roots")?, "repo")?
            };
            if !evidence::within_roots(&relative, permitted)? {
                return Err(ContractError::new("artifact_ref", "OUTSIDE_ALLOWED_ROOT").into());
            }
            (
                relative,
                self.repository.to_path_buf(),
                permitted,
                PyString::from("repo:"),
            )
        } else if runtime && version_three()? {
            let relative =
                contracts::lexical_relative(&Value::String(identity.clone()), "artifact_ref")?;
            evidence::policy_path_allowed(&Value::String(relative.clone()), self.policy)?;
            let permitted = item(item(self.policy, "artifact_roots")?, "runtime")?;
            if !evidence::within_roots(&relative, permitted)? {
                return Err(ContractError::new("artifact_ref", "OUTSIDE_ALLOWED_ROOT").into());
            }
            let root = if let Some(root) = self.runtime {
                root.to_path_buf()
            } else {
                policy::resolve_state_root(self.files, None, self.installation)?
            };
            (relative, root, permitted, PyString::from("runtime:"))
        } else if profile && identity.0.starts_with(&profile_prefix.0) {
            let relative = contracts::lexical_relative(
                &Value::String(PyString(identity.0[profile_prefix.0.len()..].to_vec())),
                "artifact_ref",
            )?;
            evidence::policy_path_allowed(&Value::String(relative.clone()), self.policy)?;
            let permitted = item(item(self.policy, "artifact_roots")?, "profile")?;
            if !evidence::within_roots(&relative, permitted)? {
                return Err(ContractError::new("artifact_ref", "OUTSIDE_ALLOWED_ROOT").into());
            }
            (
                relative,
                self.skill.to_path_buf(),
                permitted,
                PyString::from("profile:software-engineering-graph/"),
            )
        } else {
            return Err(ContractError::new("artifact_ref", "AUTHORITATIVE_REF_REQUIRED").into());
        };
        let path = root.join(self.files.from_string(&relative)?);
        let mut roots = Vec::new();
        for configured in errors::iterable_values(permitted)? {
            let Some(configured) = configured.as_string() else {
                return Err(EngineError::python(
                    "TypeError",
                    format!(
                        "unsupported operand type(s) for /: '{}' and '{}'",
                        if self.files.windows() {
                            "WindowsPath"
                        } else {
                            "PosixPath"
                        },
                        configured.python_type()
                    ),
                ));
            };
            roots.push(root.join(self.files.from_string(configured)?));
        }
        let mut normalized = normalized_prefix;
        normalized.0.extend(relative.0);
        Ok((path, roots, normalized))
    }
    fn check_extension(&self, path: &Path, kind_policy: &Value) -> EngineResult<()> {
        let suffix = path
            .extension()
            .filter(|value| !value.is_empty())
            .map(|extension| {
                let mut suffix = PyString::from(".");
                suffix.0.extend(self.files.spelling(Path::new(extension)).0);
                compat::unicode::lower(&suffix)
            })
            .unwrap_or_else(|| PyString::from(""));
        let extensions = errors::iterable_set(item(kind_policy, "extensions")?)?;
        if !errors::set_contains(&extensions, &Value::String(suffix)) {
            return Err(ContractError::new("artifact_ref", "ARTIFACT_TYPE_MISMATCH").into());
        }
        Ok(())
    }
    pub fn resolve_reference(
        &self,
        reference: &Value,
        separate_digest: &Value,
        kind: &Value,
    ) -> EngineResult<VerifiedArtifact> {
        contracts::validate_ref(reference, "artifact_ref", true)?;
        let expected = contracts::digest(separate_digest, "artifact_sha256")?;
        let (prefix, body) = split_reference(reference.as_string().unwrap());
        let marker = PyString::from("#sha256=");
        let position = body
            .windows(marker.0.len())
            .position(|part| part == marker.0);
        let Some(position) = position else {
            return Err(ContractError::new("artifact_ref", "DIGEST_DISAGREEMENT").into());
        };
        if body[position + marker.0.len()..] != expected.chars().map(u32::from).collect::<Vec<_>>()
        {
            return Err(ContractError::new("artifact_ref", "DIGEST_DISAGREEMENT").into());
        }
        let kind_policy = evidence::kind_policy(kind, self.policy)?;
        let maximum = evidence::artifact_size_limit(kind, self.policy)?;
        if prefix == [108, 101, 100, 103, 101, 114] {
            let ledger = self
                .ledger
                .ok_or_else(|| ContractError::new("artifact_ref", "LEDGER_CONTEXT_REQUIRED"))?;
            let row = ledger
                .immutable_artifact(reference, &expected.clone().into())?
                .filter(|row| row["kind"].py_eq(kind))
                .ok_or_else(|| ContractError::new("artifact_ref", "LEDGER_ARTIFACT_NOT_FOUND"))?;
            evidence::enforce_artifact_size(
                item(&row, "kind")?,
                item(&row, "size_bytes")?,
                self.policy,
            )?;
            return Ok(VerifiedArtifact::from_record(&row)?);
        }
        let (path, roots, _) = self.reference_path(prefix, &PyString(body[..position].to_vec()))?;
        self.check_extension(&path, kind_policy)?;
        let snapshot = self
            .snapshots
            .snapshot_integer(&path, &roots, &maximum)
            .map_err(snapshot_error)?;
        if snapshot.digest != expected {
            return Err(ContractError::new("artifact_ref", "INPUT_DIGEST_MISMATCH").into());
        }
        Ok(VerifiedArtifact {
            reference: reference.clone(),
            kind: kind.clone(),
            sha256: expected.into(),
            size_bytes: Value::Integer(snapshot.size.into()),
            source_type: Value::String(PyString(prefix.to_vec())),
            source_path: Value::String(self.files.spelling(&snapshot.path)),
            content_json: Value::Null,
            device: Value::Integer(snapshot.identity.device.into()),
            inode: Value::Integer(snapshot.identity.inode.into()),
        })
    }
    pub fn resolve_unhashed_reference(
        &self,
        reference: &Value,
        kind: &Value,
    ) -> EngineResult<VerifiedArtifact> {
        contracts::validate_ref(reference, "artifact_ref", false)?;
        let (prefix, body) = split_reference(reference.as_string().unwrap());
        if body.contains(&35) {
            return Err(ContractError::new("artifact_ref", "UNEXPECTED_DIGEST").into());
        }
        let kind_policy = evidence::kind_policy(kind, self.policy)?;
        let maximum = evidence::artifact_size_limit(kind, self.policy)?;
        let (path, roots, mut normalized) =
            self.reference_path(prefix, &PyString(body.to_vec()))?;
        self.check_extension(&path, kind_policy)?;
        let snapshot = self
            .snapshots
            .snapshot_integer(&path, &roots, &maximum)
            .map_err(snapshot_error)?;
        normalized.0.extend(
            format!("#sha256={}", snapshot.digest)
                .chars()
                .map(u32::from),
        );
        Ok(VerifiedArtifact {
            reference: Value::String(normalized),
            kind: kind.clone(),
            sha256: snapshot.digest.into(),
            size_bytes: Value::Integer(snapshot.size.into()),
            source_type: Value::String(PyString(prefix.to_vec())),
            source_path: Value::String(self.files.spelling(&snapshot.path)),
            content_json: Value::Null,
            device: Value::Integer(snapshot.identity.device.into()),
            inode: Value::Integer(snapshot.identity.inode.into()),
        })
    }
    pub fn reverify_artifact(&self, artifact: &Value) -> EngineResult<()> {
        evidence::enforce_artifact_size(
            item(artifact, "kind")?,
            item(artifact, "size_bytes")?,
            self.policy,
        )?;
        if item(artifact, "source_type")?.is_text("ledger") {
            let content = item(artifact, "content_json")?;
            if matches!(content, Value::Null) {
                return Err(ContractError::new("artifact_ref", "LEDGER_CONTENT_MISSING").into());
            }
            let value = match compat::loads_value(content) {
                Ok(value) => value,
                Err(error) if error.kind == "JSONDecodeError" => {
                    return Err(ContractError::new("artifact_ref", "LEDGER_CONTENT_INVALID").into())
                }
                Err(error) => return Err(error.into()),
            };
            let canonical = ids::canonical_bytes(&value)?;
            if !item(artifact, "sha256")?.is_text(&ids::sha256_bytes(&canonical))
                || !item(artifact, "size_bytes")?.py_eq(&Value::Integer(canonical.len().into()))
            {
                return Err(ContractError::new("artifact_ref", "LEDGER_DIGEST_MISMATCH").into());
            }
            return Ok(());
        }
        let verified = self.resolve_reference(
            item(artifact, "ref")?,
            item(artifact, "sha256")?,
            item(artifact, "kind")?,
        )?;
        if !verified.device.py_eq(item(artifact, "device")?)
            || !verified.inode.py_eq(item(artifact, "inode")?)
        {
            return Err(ContractError::new("artifact_ref", "ARTIFACT_IDENTITY_MISMATCH").into());
        }
        Ok(())
    }
}
pub fn persist_artifact(
    ledger: &dyn ArtifactLedger,
    run_id: &str,
    artifact: &VerifiedArtifact,
) -> EngineResult<()> {
    let existing = ledger.artifact_by_reference(&artifact.reference)?;
    let values = artifact.stored_values(run_id);
    let Some(existing) = existing else {
        return ledger.insert_artifact_values(&values);
    };
    let mut comparison = Vec::new();
    for key in [
        "run_id",
        "kind",
        "sha256",
        "size_bytes",
        "source_type",
        "source_path",
        "content_json",
        "device",
        "inode",
        "immutable",
    ] {
        comparison.push(item(&existing, key)?.clone());
    }
    if !Value::List(comparison).container_eq(&Value::List(values[1..].to_vec())) {
        return Err(ContractError::new("artifact_ref", "ARTIFACT_CONFLICT").into());
    }
    Ok(())
}
