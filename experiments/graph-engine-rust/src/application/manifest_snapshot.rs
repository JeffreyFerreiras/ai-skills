use super::{
    error::{EngineError, EngineResult, ManifestJsonTraceSite},
    policy,
    semantic_state::SemanticContext,
    snapshots::{self, Snapshot, SnapshotFailure},
};
use crate::{
    compat::{integer, Value},
    domain::errors,
};
use num_bigint::BigInt;

pub fn inbox_snapshot(
    context: &SemanticContext<'_>,
    stored_policy: &Value,
    run_id: &str,
    path: &Value,
    maximum: BigInt,
    trace_site: ManifestJsonTraceSite,
) -> EngineResult<Snapshot> {
    let repository = errors::string_item(stored_policy, "repository_id")?
        .text()
        .expect("validated repository ID");
    let inbox = context.store.inbox_root(&repository, run_id);
    let path = policy::path_from_value(context.files, path)?;
    context
        .store
        .verify_inbox_manifest(&repository, run_id, &path)?;
    let limits = errors::string_item(stored_policy, "limits")?;
    let limit = integer::coerce(errors::string_item(limits, "manifest_bytes")?)?;
    snapshots::safe_json_snapshot_integer(context.snapshots, &path, &[inbox], &maximum.min(limit))
        .map_err(|failure| match failure {
            SnapshotFailure::Contract(error) => EngineError::from(error),
            SnapshotFailure::Io(error) => EngineError::from(error),
            SnapshotFailure::Json(error) => EngineError::manifest_json(error, trace_site),
        })
}
