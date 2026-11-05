use crate::{
    compat::{self, JsonError, Value},
    domain::contracts::{self, ContractError},
};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u128,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub path: PathBuf,
    pub data: Vec<u8>,
    pub digest: String,
    pub size: usize,
    pub parsed: Option<Value>,
    pub identity: FileIdentity,
}
#[derive(Debug)]
pub enum SnapshotFailure {
    Contract(ContractError),
    Io(io::Error),
    Json(JsonError),
}
impl From<ContractError> for SnapshotFailure {
    fn from(value: ContractError) -> Self {
        Self::Contract(value)
    }
}
impl From<io::Error> for SnapshotFailure {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub trait FileSnapshotPort {
    fn snapshot(
        &self,
        path: &Path,
        roots: &[PathBuf],
        maximum: usize,
    ) -> Result<Snapshot, SnapshotFailure>;
}
pub trait IntegerFileSnapshotPort {
    fn snapshot_integer(
        &self,
        path: &Path,
        roots: &[PathBuf],
        maximum: &num_bigint::BigInt,
    ) -> Result<Snapshot, SnapshotFailure>;
}
pub fn safe_json_snapshot<F: FileSnapshotPort + ?Sized>(
    files: &F,
    path: &Path,
    roots: &[PathBuf],
    maximum: usize,
) -> Result<Snapshot, SnapshotFailure> {
    finish_json_snapshot(files.snapshot(path, roots, maximum)?)
}

pub fn safe_json_snapshot_integer(
    files: &dyn IntegerFileSnapshotPort,
    path: &Path,
    roots: &[PathBuf],
    maximum: &num_bigint::BigInt,
) -> Result<Snapshot, SnapshotFailure> {
    finish_json_snapshot(files.snapshot_integer(path, roots, maximum)?)
}

fn finish_json_snapshot(mut snapshot: Snapshot) -> Result<Snapshot, SnapshotFailure> {
    let parsed = match compat::loads(&snapshot.data) {
        Ok(value) => value,
        Err(error) if ["UnicodeDecodeError", "JSONDecodeError"].contains(&error.kind) => {
            return Err(ContractError::new("path", "INVALID_JSON").into())
        }
        Err(error) => return Err(SnapshotFailure::Json(error)),
    };
    contracts::reject_sensitive(&parsed, "root", 0)?;
    snapshot.parsed = Some(parsed);
    Ok(snapshot)
}
