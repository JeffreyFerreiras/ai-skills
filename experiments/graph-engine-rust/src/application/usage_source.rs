use super::error::EngineResult;
use crate::{
    compat::Value,
    domain::usage::{self, Limits, Source},
};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct UsageBytes {
    pub data: Vec<u8>,
    pub diagnostics: Vec<String>,
}

pub trait UsageSourcePort {
    fn read_prefix(&self, path: &Path, limits: Limits) -> EngineResult<UsageBytes>;
}

pub fn read_source(
    files: &dyn UsageSourcePort,
    path: &Path,
    limits: Limits,
) -> EngineResult<Source> {
    let sampled = files.read_prefix(path, limits)?;
    Ok(usage::parse_source(
        sampled.data,
        &sampled.diagnostics,
        usage::known_models(),
        limits,
    ))
}

pub fn checkpoint(files: &dyn UsageSourcePort, path: &Path) -> EngineResult<Value> {
    Ok(read_source(files, path, Limits::default())?.usable_checkpoint()?)
}
