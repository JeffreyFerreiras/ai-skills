use crate::compat::{dumps, JsonError, SerializationProfile, Value};
use sha2::{Digest, Sha256};

pub fn canonical_bytes(value: &Value) -> Result<Vec<u8>, JsonError> {
    dumps(value, SerializationProfile::CANONICAL)
}
pub fn sha256_bytes(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
pub fn repository_digest(repository_id: &str) -> String {
    let mut bytes = b"graph-repository-v1\0".to_vec();
    bytes.extend(repository_id.as_bytes());
    sha256_bytes(&bytes)
}
pub fn stable_id(
    run_id: &str,
    policy_digest: &str,
    entity_type: &str,
    template_key: &str,
    generation: i64,
    specialist_tag: Option<&str>,
) -> Result<String, JsonError> {
    let identity = Value::list([
        Value::from(5_i64),
        run_id.into(),
        policy_digest.into(),
        entity_type.into(),
        template_key.into(),
        generation.into(),
        specialist_tag.unwrap_or("").into(),
    ]);
    Ok(format!(
        "g2-{}",
        &sha256_bytes(&canonical_bytes(&identity)?)[..24]
    ))
}

pub fn stable_id_values(
    run_id: &Value,
    policy_digest: &Value,
    entity_type: &str,
    template_key: &str,
    generation: &Value,
    specialist_tag: Option<&str>,
) -> Result<String, JsonError> {
    stable_id_with_tag_value(
        run_id,
        policy_digest,
        entity_type,
        template_key,
        generation,
        &Value::from(specialist_tag.unwrap_or("")),
    )
}

pub fn stable_id_with_tag_value(
    run_id: &Value,
    policy_digest: &Value,
    entity_type: &str,
    template_key: &str,
    generation: &Value,
    specialist_tag: &Value,
) -> Result<String, JsonError> {
    stable_id_with_key_value(
        run_id,
        policy_digest,
        entity_type,
        &Value::from(template_key),
        generation,
        specialist_tag,
    )
}

pub fn stable_id_with_key_value(
    run_id: &Value,
    policy_digest: &Value,
    entity_type: &str,
    template_key: &Value,
    generation: &Value,
    specialist_tag: &Value,
) -> Result<String, JsonError> {
    let identity = Value::list([
        Value::from(5_i64),
        run_id.clone(),
        policy_digest.clone(),
        entity_type.into(),
        template_key.clone(),
        generation.clone(),
        if specialist_tag.truthy() {
            specialist_tag.clone()
        } else {
            "".into()
        },
    ]);
    Ok(format!(
        "g2-{}",
        &sha256_bytes(&canonical_bytes(&identity)?)[..24]
    ))
}
