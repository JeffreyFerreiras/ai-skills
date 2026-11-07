use graph_engine::{
    application::{
        error::EngineResult,
        persistent_store::{StateFileInfo, StatePlatform},
    },
    compat::{PyString, Value},
    infrastructure::state_platform::NativeStatePlatform,
};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct StatePlatformOverrides {
    pub(super) directory_sync: Option<fn(bool) -> String>,
    pub(super) locality: Option<fn() -> EngineResult<String>>,
}

impl StatePlatform for StatePlatformOverrides {
    fn windows(&self) -> bool {
        NativeStatePlatform.windows()
    }
    fn spelling(&self, path: &Path) -> PyString {
        NativeStatePlatform.spelling(path)
    }
    fn absolute(&self, path: &Path) -> EngineResult<PathBuf> {
        NativeStatePlatform.absolute(path)
    }
    fn resolve(&self, path: &Path, strict: bool) -> EngineResult<PathBuf> {
        NativeStatePlatform.resolve(path, strict)
    }
    fn ensure_safe(&self, path: &Path, stop: Option<&Path>) -> EngineResult<()> {
        NativeStatePlatform.ensure_safe(path, stop)
    }
    fn info(&self, path: &Path, follow: bool) -> EngineResult<StateFileInfo> {
        NativeStatePlatform.info(path, follow)
    }
    fn exists(&self, path: &Path) -> bool {
        NativeStatePlatform.exists(path)
    }
    fn create_directory(&self, path: &Path, parents: bool) -> EngineResult<()> {
        NativeStatePlatform.create_directory(path, parents)
    }
    fn set_mode(&self, path: &Path, mode: u32) -> EngineResult<()> {
        NativeStatePlatform.set_mode(path, mode)
    }
    fn effective_user(&self) -> Option<u32> {
        NativeStatePlatform.effective_user()
    }
    fn entries(&self, path: &Path) -> EngineResult<Vec<PathBuf>> {
        NativeStatePlatform.entries(path)
    }
    fn read_text(&self, path: &Path) -> EngineResult<PyString> {
        NativeStatePlatform.read_text(path)
    }
    fn write_synced(&self, path: &Path, bytes: &[u8], exclusive: bool) -> EngineResult<()> {
        NativeStatePlatform.write_synced(path, bytes, exclusive)
    }
    fn remove_file(&self, path: &Path) -> EngineResult<()> {
        NativeStatePlatform.remove_file(path)
    }
    fn replace(&self, source: &Path, target: &Path) -> EngineResult<()> {
        NativeStatePlatform.replace(source, target)
    }
    fn flush_file(&self, path: &Path) -> EngineResult<()> {
        NativeStatePlatform.flush_file(path)
    }
    fn directory_sync(&self, directory: &Path, acknowledged: bool) -> EngineResult<String> {
        match self.directory_sync {
            Some(sync) => Ok(sync(acknowledged)),
            None => NativeStatePlatform.directory_sync(directory, acknowledged),
        }
    }
    fn locality(&self, path: &Path) -> EngineResult<String> {
        match self.locality {
            Some(probe) => probe(),
            None => NativeStatePlatform.locality(path),
        }
    }
    fn host_identity(&self) -> EngineResult<String> {
        NativeStatePlatform.host_identity()
    }
    fn process_id(&self) -> u32 {
        NativeStatePlatform.process_id()
    }
    fn process_alive(&self, process_id: &Value) -> EngineResult<bool> {
        NativeStatePlatform.process_alive(process_id)
    }
    fn temporary_name(&self) -> EngineResult<String> {
        NativeStatePlatform.temporary_name()
    }
    fn utc_now(&self) -> EngineResult<String> {
        NativeStatePlatform.utc_now()
    }
}
