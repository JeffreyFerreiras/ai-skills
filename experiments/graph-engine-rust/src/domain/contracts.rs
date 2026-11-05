use super::errors::{ValidationFailure, ValidationResult};
use crate::compat::{glob, unicode, PyString, Value};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractError {
    pub field: PyString,
    pub code: String,
}
impl ContractError {
    pub fn new(field: &str, code: &str) -> Self {
        Self {
            field: field.into(),
            code: code.into(),
        }
    }
}
impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}",
            self.field
                .utf8()
                .unwrap_or_else(|_| format!("{:?}", self.field)),
            self.code
        )
    }
}
impl std::error::Error for ContractError {}
pub type CResult<T> = Result<T, ContractError>;
pub fn require_python_keys(
    value: &Value,
    required: &Value,
    allowed: &Value,
    field: &str,
) -> ValidationResult<()> {
    use super::errors::{iterable_set, set_contains};
    let required = iterable_set(required)?;
    let allowed = iterable_set(allowed)?;
    let actual_for_missing = iterable_set(value)?;
    let missing = required
        .iter()
        .any(|key| !set_contains(&actual_for_missing, key));
    let actual_for_unknown = iterable_set(value)?;
    let unknown = actual_for_unknown
        .iter()
        .any(|key| !set_contains(&allowed, key));
    if missing {
        return Err(ContractError::new(field, "MISSING_FIELD").into());
    }
    if unknown {
        return Err(ContractError::new(field, "UNKNOWN_FIELD").into());
    }
    Ok(())
}
pub fn require_keys(
    value: &Value,
    required: &[&str],
    allowed: &[&str],
    field: &str,
) -> CResult<()> {
    let items = value
        .as_object()
        .ok_or_else(|| ContractError::new(field, "INVALID_OBJECT"))?;
    if required.iter().any(|key| value.get(key).is_none()) {
        return Err(ContractError::new(field, "MISSING_FIELD"));
    }
    if items
        .iter()
        .any(|(key, _)| !allowed.iter().any(|text| key.equals(text)))
    {
        return Err(ContractError::new(field, "UNKNOWN_FIELD"));
    }
    Ok(())
}
pub fn opaque(value: &Value, field: &str) -> CResult<String> {
    let text = value
        .text()
        .ok_or_else(|| ContractError::new(field, "INVALID_ID"))?;
    let bytes = text.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 64
        || !bytes[0].is_ascii_alphanumeric()
        || !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(b))
    {
        return Err(ContractError::new(field, "INVALID_ID"));
    }
    Ok(text)
}
pub fn digest(value: &Value, field: &str) -> CResult<String> {
    let text = value
        .text()
        .ok_or_else(|| ContractError::new(field, "INVALID_DIGEST"))?;
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ContractError::new(field, "INVALID_DIGEST"));
    }
    Ok(text)
}
fn control(point: u32) -> bool {
    point <= 31 || point == 127
}
fn latin_ignorecase(point: u32) -> bool {
    matches!(point,65..=90|97..=122|0x130|0x131|0x17f|0x212a)
}
fn credential(text: &PyString) -> bool {
    let s = &text.0;
    for start in 0..s.len() {
        if s[start..].starts_with(&"-----BEGIN ".chars().map(u32::from).collect::<Vec<_>>()) {
            let mut index = start + 11;
            while index < s.len() && (matches!(s[index], 65..=90) || s[index] == 32) {
                index += 1;
            }
            for end in start + 11..=index {
                if s[end..].starts_with(
                    &"PRIVATE KEY-----"
                        .chars()
                        .map(u32::from)
                        .collect::<Vec<_>>(),
                ) {
                    return true;
                }
            }
        }
        if start + 6 < s.len()
            && s[start..start + 6]
                .iter()
                .zip(b"bearer")
                .all(|(&p, &b)| p == u32::from(b) || p == u32::from(b.to_ascii_uppercase()))
        {
            let mut index = start + 6;
            if unicode::whitespace(s[index]) {
                while index < s.len() && unicode::whitespace(s[index]) {
                    index += 1;
                }
                if index < s.len()
                    && (latin_ignorecase(s[index])
                        || matches!(s[index], 48..=57 | 46 | 95 | 126 | 43 | 47 | 61 | 45))
                {
                    return true;
                }
            }
        }
        if s[start..].starts_with(&[101, 121, 74]) {
            let b64 = |p: u32| matches!(p,65..=90|97..=122|48..=57|95|45);
            let mut index = start + 3;
            let first = index;
            while index < s.len() && b64(s[index]) {
                index += 1;
            }
            if index == first || s.get(index) != Some(&46) {
                continue;
            }
            index += 1;
            if !s[index..].starts_with(&[101, 121, 74]) {
                continue;
            }
            index += 3;
            let second = index;
            while index < s.len() && b64(s[index]) {
                index += 1;
            }
            if index == second || s.get(index) != Some(&46) {
                continue;
            }
            index += 1;
            if index < s.len() && b64(s[index]) {
                return true;
            }
        }
    }
    false
}
pub fn bounded_string<'a>(value: &'a Value, field: &str, maximum: usize) -> CResult<&'a PyString> {
    let s = value
        .as_string()
        .ok_or_else(|| ContractError::new(field, "INVALID_STRING"))?;
    if s.is_empty() || s.len() > maximum {
        return Err(ContractError::new(field, "INVALID_STRING"));
    }
    if s.0.iter().copied().any(control) {
        return Err(ContractError::new(field, "CONTROL_CHARACTER"));
    }
    if credential(s) {
        return Err(ContractError::new(field, "CREDENTIAL_VALUE"));
    }
    Ok(s)
}
pub fn reject_sensitive(value: &Value, field: &str, depth: usize) -> CResult<()> {
    reject_sensitive_at(value, &field.into(), depth)
}
fn reject_sensitive_at(value: &Value, field: &PyString, depth: usize) -> CResult<()> {
    let failure = |code: &str| ContractError {
        field: field.clone(),
        code: code.into(),
    };
    if depth > 24 {
        return Err(failure("MAX_DEPTH"));
    }
    const SECRETS: &[&str] = &[
        "token",
        "password",
        "secret",
        "authorization",
        "cookie",
        "private_key",
        "api_key",
        "access_key",
        "client_secret",
    ];
    match value {
        Value::Object(items) => {
            if items.len() > 4096 {
                return Err(failure("MAX_ITEMS"));
            }
            for (key, item) in items {
                if key.0.iter().copied().any(control) {
                    return Err(failure("INVALID_KEY"));
                }
                let normalized = unicode::lower(key);
                let normalized = PyString(
                    normalized
                        .0
                        .into_iter()
                        .map(|p| if p == 45 { 95 } else { p })
                        .collect(),
                );
                let secret = SECRETS.iter().any(|text| normalized.equals(text))
                    || normalized.0.split(|&p| p == 95).any(|part| {
                        SECRETS
                            .iter()
                            .any(|text| part.iter().copied().eq(text.chars().map(u32::from)))
                    });
                let mut child = field.clone();
                child.0.push(46);
                child.0.extend(&key.0);
                if secret {
                    return Err(ContractError {
                        field: child,
                        code: "SECRET_FIELD".into(),
                    });
                }
                reject_sensitive_at(item, &child, depth + 1)?;
            }
        }
        Value::List(items) => {
            if items.len() > 4096 {
                return Err(failure("MAX_ITEMS"));
            }
            for (index, item) in items.iter().enumerate() {
                let mut child = field.clone();
                child.0.extend(format!("[{index}]").chars().map(u32::from));
                reject_sensitive_at(item, &child, depth + 1)?;
            }
        }
        Value::String(_) => {
            if let Err(error) = bounded_string(value, "", 4096) {
                return Err(failure(&error.code));
            }
        }
        _ => (),
    }
    Ok(())
}
pub fn lexical_relative(value: &Value, field: &str) -> CResult<PyString> {
    let s = bounded_string(value, field, 1024)?;
    let mut text = PyString(s.0.iter().map(|&p| if p == 92 { 47 } else { p }).collect());
    let directory = text.ends_with("/");
    while text.0.last() == Some(&47) {
        text.0.pop();
    }
    if text.starts_with("/")
        || (text
            .0
            .first()
            .is_some_and(|&p| matches!(p,65..=90|97..=122))
            && text.0.get(1) == Some(&58))
    {
        return Err(ContractError::new(field, "ABSOLUTE_PATH"));
    }
    let parts: Vec<_> = text
        .0
        .split(|&p| p == 47)
        .map(|p| PyString(p.to_vec()))
        .collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || part.equals(".") || part.equals(".."))
    {
        return Err(ContractError::new(field, "PATH_TRAVERSAL"));
    }
    let lowered: Vec<_> = parts.iter().map(unicode::lower).collect();
    if lowered.iter().any(|part| {
        [
            ".git",
            ".hg",
            ".svn",
            ".ssh",
            "secret",
            "secrets",
            "credential",
            "credentials",
            "keys",
            "graph-runs",
            "graph-inbox",
        ]
        .iter()
        .any(|s| part.equals(s))
    }) {
        return Err(ContractError::new(field, "SENSITIVE_PATH"));
    }
    let name = lowered.last().unwrap();
    let suffix = name
        .0
        .iter()
        .rposition(|&p| p == 46)
        .filter(|&i| i > 0 && i + 1 < name.len())
        .map(|i| PyString(name.0[i..].to_vec()))
        .unwrap_or_default();
    if [".env", "id_rsa", "id_ed25519"]
        .iter()
        .any(|s| name.equals(s))
        || name.starts_with(".env.")
        || [".pem", ".key", ".pfx", ".p12"]
            .iter()
            .any(|s| suffix.equals(s))
    {
        return Err(ContractError::new(field, "SENSITIVE_PATH"));
    }
    if directory {
        text.0.push(47);
    }
    Ok(text)
}
pub fn validate_ref(value: &Value, field: &str, content_required: bool) -> CResult<PyString> {
    let text = bounded_string(value, field, 1200)?.clone();
    let lower = unicode::lower(&text);
    if text.contains("://")
        || ["http:", "https:", "file:"]
            .iter()
            .any(|p| lower.starts_with(p))
    {
        return Err(ContractError::new(field, "URL_FORBIDDEN"));
    }
    let body = if text.starts_with("repo:") {
        Some((PyString(text.0[5..].to_vec()), 0))
    } else if text.starts_with("runtime:") {
        Some((PyString(text.0[8..].to_vec()), 1))
    } else if text.starts_with("profile:software-engineering-graph/") {
        Some((
            PyString(text.0["profile:software-engineering-graph/".len()..].to_vec()),
            2,
        ))
    } else {
        None
    };
    if let Some((body, kind)) = body {
        let (path, hash) = body
            .split_once("#sha256=")
            .map(|(p, h)| (p, Some(h)))
            .unwrap_or((body, None));
        lexical_relative(&Value::String(path.clone()), field)?;
        if kind == 1 && !path.starts_with("artifacts/") {
            return Err(ContractError::new(field, "RUNTIME_ROOT_FORBIDDEN"));
        }
        if kind == 2 && !path.starts_with("references/") {
            return Err(ContractError::new(field, "PROFILE_ROOT_FORBIDDEN"));
        }
        if content_required && hash.is_none() {
            return Err(ContractError::new(field, "CONTENT_DIGEST_REQUIRED"));
        }
        if let Some(hash) = hash {
            digest(&Value::String(hash), field)?;
        }
    } else if text.starts_with("ledger:") {
        let body = PyString(text.0[7..].to_vec());
        let (identity, hash) = body
            .split_once("#sha256=")
            .map(|(p, h)| (p, Some(h)))
            .unwrap_or((body, None));
        opaque(&Value::String(identity), field)?;
        if content_required && hash.is_none() {
            return Err(ContractError::new(field, "CONTENT_DIGEST_REQUIRED"));
        }
        if let Some(hash) = hash {
            digest(&Value::String(hash), field)?;
        }
    } else if text.starts_with("thread:") || text.starts_with("authority:") {
        let identity = text.split_once(":").unwrap().1;
        opaque(&Value::String(identity.clone()), field)?;
        if content_required || identity.contains("#") || identity.contains("?") {
            return Err(ContractError::new(field, "AUDIT_REF_NOT_CONTENT"));
        }
    } else {
        return Err(ContractError::new(field, "UNSUPPORTED_REFERENCE"));
    }
    Ok(text)
}
pub fn filesystem_capability_target(
    value: &Value,
    denied: &[PyString],
) -> ValidationResult<PyString> {
    let target = validate_ref(value, "capability.target_ref", false)?;
    let (prefix, relative) = if target.starts_with("repo:") || target.starts_with("runtime:") {
        target.split_once(":").unwrap()
    } else if target.starts_with("profile:software-engineering-graph/") {
        (PyString::from("profile"), PyString(target.0[8..].to_vec()))
    } else {
        return Err(ContractError::new("capability.target_ref", "FILESYSTEM_REF_REQUIRED").into());
    };
    let normalized = lexical_relative(&Value::String(relative.clone()), "capability.target_ref")?;
    let mut parts = relative.0.split(|&p| p == 47).collect::<Vec<_>>();
    if parts.last() == Some(&&[][..]) {
        parts.pop();
    }
    if normalized != relative
        || relative
            .0
            .iter()
            .any(|&p| matches!(p, 42 | 63 | 91 | 93 | 123 | 125 | 35 | 58))
        || parts
            .iter()
            .any(|part| part.last().is_some_and(|&p| p == 46 || p == 32))
    {
        return Err(
            ContractError::new("capability.target_ref", "NON_LITERAL_CAPABILITY_PATH").into(),
        );
    }
    if prefix.equals("repo") {
        let normalized = unicode::lower(&relative);
        if [".codex/", ".agents/", ".claude/", ".cursor/"]
            .iter()
            .any(|p| normalized.starts_with(p))
        {
            return Err(ContractError::new(
                "capability.target_ref",
                "PROFILE_OR_POLICY_ROOT_FORBIDDEN",
            )
            .into());
        }
        let mut prefixed = PyString::from("/");
        prefixed.0.extend(&normalized.0);
        for pattern in denied {
            let pattern = unicode::lower(pattern);
            if glob::fnmatchcase(&normalized, &pattern)
                || glob::fnmatchcase(&prefixed, &pattern)
                || glob::posix_path_match(&normalized, &pattern)
                    .map_err(|_| ValidationFailure::python("ValueError", "empty pattern"))?
            {
                return Err(ContractError::new("capability.target_ref", "SENSITIVE_PATH").into());
            }
        }
    }
    Ok(target)
}
pub fn capability_within(
    candidate: &Value,
    ceiling: &Value,
    windows: bool,
) -> ValidationResult<bool> {
    use super::errors::{attribute_error, container_equal, string_item};
    let effect = string_item(candidate, "effect")?;
    let action = string_item(candidate, "action")?;
    let outer_effect = string_item(ceiling, "effect")?;
    let outer_action = string_item(ceiling, "action")?;
    if !container_equal(effect, outer_effect) || !container_equal(action, outer_action) {
        return Ok(false);
    }
    let target = string_item(candidate, "target_ref")?;
    let outer = string_item(ceiling, "target_ref")?;
    let effect = effect
        .as_string()
        .ok_or_else(|| attribute_error(effect, "startswith"))?;
    if !effect.starts_with("filesystem") {
        return Ok(target.py_eq(outer));
    }
    let target = filesystem_capability_target(target, &[])?;
    let outer = filesystem_capability_target(outer, &[])?;
    let target_path = unicode::normcase(&target, windows);
    let outer_path = unicode::normcase(&outer, windows);
    Ok(target_path == outer_path
        || ((outer.starts_with("repo:") || outer.starts_with("runtime:"))
            && outer.ends_with("/")
            && target_path.0.starts_with(&outer_path.0)))
}
#[derive(Debug)]
pub enum ProjectionError {
    Validation(ValidationFailure),
    InvalidVersion,
}
pub fn projected_capability_within(
    candidate: &Value,
    ceiling: &Value,
    version: Option<&Value>,
    windows: bool,
) -> Result<bool, ProjectionError> {
    match version {
        None => super::errors::dictionaries_equal(candidate, ceiling)
            .map_err(ProjectionError::Validation),
        Some(value) if value.as_i64() == Some(1) => {
            capability_within(candidate, ceiling, windows).map_err(ProjectionError::Validation)
        }
        _ => Err(ProjectionError::InvalidVersion),
    }
}
