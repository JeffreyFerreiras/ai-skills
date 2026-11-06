use super::{
    contracts::{self, ContractError},
    errors::{self, ValidationFailure, ValidationResult},
    ids, policy,
};
use crate::compat::{self, PyString, Value};
use num_bigint::BigInt;
use num_traits::Zero;

#[derive(Clone, Debug)]
pub struct VerifiedArtifact {
    pub reference: Value,
    pub kind: Value,
    pub sha256: Value,
    pub size_bytes: Value,
    pub source_type: Value,
    pub source_path: Value,
    pub content_json: Value,
    pub device: Value,
    pub inode: Value,
}
impl VerifiedArtifact {
    pub fn as_input(&self) -> Value {
        Value::object([
            ("kind", self.kind.clone()),
            ("ref", self.reference.clone()),
            ("sha256", self.sha256.clone()),
            ("size_bytes", self.size_bytes.clone()),
        ])
    }
    pub fn record(&self) -> Value {
        Value::object([
            ("ref", self.reference.clone()),
            ("kind", self.kind.clone()),
            ("sha256", self.sha256.clone()),
            ("size_bytes", self.size_bytes.clone()),
            ("source_type", self.source_type.clone()),
            ("source_path", self.source_path.clone()),
            ("content_json", self.content_json.clone()),
            ("device", self.device.clone()),
            ("inode", self.inode.clone()),
        ])
    }
    pub fn from_record(record: &Value) -> ValidationResult<Self> {
        Ok(Self {
            reference: item(record, "ref")?.clone(),
            kind: item(record, "kind")?.clone(),
            sha256: item(record, "sha256")?.clone(),
            size_bytes: item(record, "size_bytes")?.clone(),
            source_type: item(record, "source_type")?.clone(),
            source_path: item(record, "source_path")?.clone(),
            content_json: item(record, "content_json")?.clone(),
            device: item(record, "device")?.clone(),
            inode: item(record, "inode")?.clone(),
        })
    }
    pub fn stored_values(&self, run_id: &str) -> Vec<Value> {
        vec![
            self.reference.clone(),
            run_id.into(),
            self.kind.clone(),
            self.sha256.clone(),
            self.size_bytes.clone(),
            self.source_type.clone(),
            self.source_path.clone(),
            self.content_json.clone(),
            self.device.clone(),
            self.inode.clone(),
            1_i64.into(),
        ]
    }
}
pub fn item<'a>(value: &'a Value, key: &str) -> ValidationResult<&'a Value> {
    errors::string_item(value, key)
}
pub fn kind_policy<'a>(kind: &Value, record: &'a Value) -> ValidationResult<&'a Value> {
    let kinds = item(record, "artifact_kinds")?;
    let Some(entries) = kinds.as_object() else {
        return Err(errors::attribute_error(kinds, "get"));
    };
    errors::require_hashable(kind)?;
    let config = entries
        .iter()
        .find(|(key, _)| Value::String(key.clone()).py_eq(kind))
        .map(|(_, value)| value);
    match config {
        Some(value) if value.as_object().is_some() => Ok(value),
        _ => Err(ContractError::new("artifact_kind", "UNKNOWN_ARTIFACT_KIND").into()),
    }
}
pub fn artifact_size_limit(kind: &Value, record: &Value) -> ValidationResult<BigInt> {
    let kind_config = kind_policy(kind, record)?;
    let engine = kind
        .text()
        .and_then(|name| policy::artifact_maximum(&name))
        .ok_or_else(|| ContractError::new("artifact_kind", "UNKNOWN_ARTIFACT_KIND"))?;
    let kind_maximum = compat::integer::coerce(item(kind_config, "max_bytes")?)?;
    let policy_maximum = compat::integer::coerce(item(item(record, "limits")?, "artifact_bytes")?)?;
    Ok([
        kind_maximum,
        policy_maximum,
        BigInt::from(engine),
        BigInt::from(4 * 1024 * 1024),
    ]
    .into_iter()
    .min()
    .unwrap())
}
pub fn enforce_artifact_size(
    kind: &Value,
    size_bytes: &Value,
    record: &Value,
) -> ValidationResult<()> {
    let Value::Integer(size) = size_bytes else {
        return Err(ContractError::new("artifact_ref", "INVALID_SIZE").into());
    };
    if size < &BigInt::zero() {
        return Err(ContractError::new("artifact_ref", "INVALID_SIZE").into());
    }
    if size > &artifact_size_limit(kind, record)? {
        return Err(ContractError::new("artifact_ref", "FILE_TOO_LARGE").into());
    }
    Ok(())
}
fn posix_parts(text: &PyString) -> (u8, Vec<Vec<u32>>) {
    let leading = text.0.iter().take_while(|&&point| point == 47).count();
    let anchor = if leading == 2 {
        2
    } else {
        u8::from(leading > 0)
    };
    let parts = text
        .0
        .split(|&point| point == 47)
        .filter(|part| !part.is_empty() && *part != [46])
        .map(<[u32]>::to_vec)
        .collect();
    (anchor, parts)
}
pub fn within_roots(relative: &PyString, roots: &Value) -> ValidationResult<bool> {
    let (anchor, parts) = posix_parts(relative);
    for configured in errors::iterable_values(roots)? {
        let Some(mut root) = configured.as_string().cloned() else {
            return Err(errors::attribute_error(&configured, "rstrip"));
        };
        while root.0.last() == Some(&47) {
            root.0.pop();
        }
        let (root_anchor, root_parts) = posix_parts(&root);
        if anchor == root_anchor && parts.starts_with(&root_parts) {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn policy_path_allowed(relative: &Value, record: &Value) -> ValidationResult<()> {
    contracts::lexical_relative(relative, "artifact_ref")?;
    let mut normalized = relative.as_string().unwrap().clone();
    for point in &mut normalized.0 {
        if *point == 92 {
            *point = 47;
        }
    }
    for pattern in errors::iterable_values(item(record, "denied_patterns")?)? {
        let normalized = compat::unicode::lower(&normalized);
        let Value::String(pattern_text) = &pattern else {
            return Err(errors::attribute_error(&pattern, "lower"));
        };
        let pattern = compat::unicode::lower(pattern_text);
        if compat::glob::fnmatchcase(&normalized, &pattern)
            || compat::glob::posix_path_match(&normalized, &pattern)
                .map_err(|_| ValidationFailure::python("ValueError", "empty pattern"))?
        {
            return Err(ContractError::new("artifact_ref", "SENSITIVE_PATH").into());
        }
    }
    Ok(())
}
pub fn canonical_ledger_artifact(
    identifier: &PyString,
    kind: &Value,
    content: &Value,
) -> Result<VerifiedArtifact, compat::JsonError> {
    let serialized = ids::canonical_bytes(content)?;
    let digest = ids::sha256_bytes(&serialized);
    let mut reference = PyString::from("ledger:");
    reference.0.extend(&identifier.0);
    reference
        .0
        .extend(format!("#sha256={digest}").chars().map(u32::from));
    Ok(VerifiedArtifact {
        reference: Value::String(reference),
        kind: kind.clone(),
        sha256: digest.into(),
        size_bytes: Value::Integer(serialized.len().into()),
        source_type: "ledger".into(),
        source_path: Value::Null,
        content_json: String::from_utf8(serialized).unwrap().into(),
        device: Value::Null,
        inode: Value::Null,
    })
}
