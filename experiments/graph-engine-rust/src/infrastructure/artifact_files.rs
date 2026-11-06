use super::{filesystem::NativeFileSnapshots, policy_files::NativePolicyFiles};
use crate::application::snapshots::{IntegerFileSnapshotPort, Snapshot, SnapshotFailure};
use num_bigint::BigInt;
use std::path::{Path, PathBuf};

impl IntegerFileSnapshotPort for NativePolicyFiles {
    fn snapshot_integer(
        &self,
        path: &Path,
        roots: &[PathBuf],
        maximum: &BigInt,
    ) -> Result<Snapshot, SnapshotFailure> {
        NativeFileSnapshots.snapshot_integer(path, roots, maximum)
    }
}
