use crate::{
    application::{
        error::{EngineError, EngineResult},
        policy::{self, PolicyFilePort},
        snapshots::SnapshotFailure,
        source_capture::{SourceFilePort, SourceMetadata},
    },
    compat::{PyString, Value},
    domain::errors,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct NativeSourceFiles<'a> {
    pub repository: &'a Path,
    pub policy: &'a Value,
    pub files: &'a dyn PolicyFilePort,
    started: Instant,
}

pub struct NativeSourceFileFactory;

impl crate::application::source_capture::SourceFileFactory for NativeSourceFileFactory {
    fn create<'a>(
        &'a self,
        repository: &'a Path,
        policy: &'a Value,
        files: &'a dyn PolicyFilePort,
    ) -> Box<dyn SourceFilePort + 'a> {
        Box::new(NativeSourceFiles::new(repository, policy, files))
    }
}
impl<'a> NativeSourceFiles<'a> {
    pub fn new(repository: &'a Path, policy: &'a Value, files: &'a dyn PolicyFilePort) -> Self {
        Self {
            repository,
            policy,
            files,
            started: Instant::now(),
        }
    }
    fn path(&self, relative: &PyString) -> EngineResult<PathBuf> {
        Ok(self.repository.join(self.files.from_string(relative)?))
    }
}

fn mode(path: &Path, metadata: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        metadata.mode() & 0o7777
    }
    #[cfg(windows)]
    {
        let mut mode = if metadata.permissions().readonly() {
            0o444
        } else {
            0o666
        };
        if metadata.is_dir()
            || path.extension().is_some_and(|extension| {
                ["exe", "bat", "cmd", "com"]
                    .iter()
                    .any(|expected| extension.eq_ignore_ascii_case(expected))
            })
        {
            mode |= 0o111;
        }
        mode
    }
}

impl SourceFilePort for NativeSourceFiles<'_> {
    fn normalize_target(&self, relative: &PyString) -> EngineResult<PyString> {
        let mut reference = PyString::from("repo:");
        reference.0.extend_from_slice(&relative.0);
        let denied = errors::iterable_values(errors::string_item(self.policy, "denied_patterns")?)?;
        let normalized = policy::repository_target(
            self.files,
            self.repository,
            &Value::String(reference),
            &denied,
        )?;
        Ok(PyString(normalized.0[5..].to_vec()))
    }
    fn metadata(&self, relative: &PyString) -> EngineResult<Option<SourceMetadata>> {
        let path = self.path(relative)?;
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let regular = metadata.is_file();
        let symlink = if regular {
            self.files.is_symlink(&path)?
        } else {
            metadata.file_type().is_symlink()
        };
        Ok(Some(SourceMetadata {
            regular,
            symlink,
            size: metadata.len(),
            mode: mode(&path, &metadata),
        }))
    }
    fn snapshot(&self, relative: &PyString, maximum: usize) -> EngineResult<(usize, String)> {
        let snapshot = self
            .files
            .snapshot(
                &self.path(relative)?,
                &[self.repository.to_path_buf()],
                maximum,
            )
            .map_err(|error| match error {
                SnapshotFailure::Contract(error) => EngineError::from(error),
                SnapshotFailure::Io(error) => error.into(),
                SnapshotFailure::Json(error) => error.into(),
            })?;
        Ok((snapshot.size, snapshot.digest))
    }
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
    fn windows(&self) -> bool {
        self.files.windows()
    }
}
