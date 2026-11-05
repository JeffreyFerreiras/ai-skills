use super::ids;
use crate::compat::{JsonError, Value};

pub fn request_slot_id(fields: [&Value; 8]) -> Result<String, JsonError> {
    let mut identity = vec![6i64.into()];
    identity.extend(fields.into_iter().cloned());
    let hash = ids::sha256_bytes(&ids::canonical_bytes(&Value::List(identity))?);
    Ok(format!("g2-{}", &hash[..24]))
}

pub fn delegated_identity(
    slot_id: &Value,
    assignment_id: &Value,
    ordinal: &Value,
    entity: &Value,
) -> Result<String, JsonError> {
    let identity = Value::List(vec![
        6i64.into(),
        entity.clone(),
        slot_id.clone(),
        assignment_id.clone(),
        ordinal.clone(),
    ]);
    let hash = ids::sha256_bytes(&ids::canonical_bytes(&identity)?);
    Ok(format!("g2-{}", &hash[..24]))
}
