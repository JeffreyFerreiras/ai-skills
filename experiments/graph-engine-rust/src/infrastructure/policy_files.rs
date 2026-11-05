use super::filesystem::{self, NativeFileSnapshots};
use crate::{
    application::{
        error::{EngineError, EngineResult},
        policy::PolicyFilePort,
        snapshots::{FileSnapshotPort, Snapshot, SnapshotFailure},
    },
    compat::{unicode, PyString},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, Metadata, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct NativePolicyFiles {
    pub environment_override: Option<BTreeMap<String, PyString>>,
}
fn failure(error: SnapshotFailure) -> EngineError {
    match error {
        SnapshotFailure::Contract(error) => error.into(),
        SnapshotFailure::Io(error) => error.into(),
        SnapshotFailure::Json(error) => error.into(),
    }
}
fn missing(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}
fn metadata(path: &Path, follow: bool) -> EngineResult<Option<Metadata>> {
    let result = if follow {
        fs::metadata(path)
    } else {
        fs::symlink_metadata(path)
    };
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if missing(&error) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
impl FileSnapshotPort for NativePolicyFiles {
    fn snapshot(
        &self,
        path: &Path,
        roots: &[PathBuf],
        maximum: usize,
    ) -> Result<Snapshot, SnapshotFailure> {
        NativeFileSnapshots.snapshot(path, roots, maximum)
    }
}
impl PolicyFilePort for NativePolicyFiles {
    fn absolute(&self, path: &Path) -> EngineResult<PathBuf> {
        Ok(filesystem::python_path(&if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        }))
    }
    fn resolve(&self, path: &Path, strict: bool) -> EngineResult<PathBuf> {
        filesystem::resolve_path(path, strict).map_err(Into::into)
    }
    fn from_string(&self, text: &PyString) -> EngineResult<PathBuf> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            let mut units = Vec::new();
            for &point in &text.0 {
                if point <= 0xffff {
                    units.push(point as u16);
                } else {
                    let character = char::from_u32(point).ok_or_else(|| {
                        EngineError::python("ValueError", "invalid Unicode codepoint")
                    })?;
                    units.extend(character.encode_utf16(&mut [0; 2]).iter().copied());
                }
            }
            Ok(PathBuf::from(OsString::from_wide(&units)))
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let mut bytes = Vec::new();
            for (index, &point) in text.0.iter().enumerate() {
                if (0xdc80..=0xdcff).contains(&point) {
                    bytes.push((point - 0xdc00) as u8);
                } else if let Some(character) = char::from_u32(point) {
                    bytes.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                } else {
                    return Err(EngineError::python("UnicodeEncodeError", format!("'utf-8' codec can't encode character '\\u{point:04x}' in position {index}: surrogates not allowed")));
                }
            }
            Ok(PathBuf::from(OsString::from_vec(bytes)))
        }
    }
    fn spelling(&self, path: &Path) -> PyString {
        filesystem::os_string(path.as_os_str())
    }
    fn same_path(&self, left: &Path, right: &Path) -> bool {
        unicode::normcase(
            &self.spelling(&filesystem::python_path(left)),
            cfg!(windows),
        ) == unicode::normcase(
            &self.spelling(&filesystem::python_path(right)),
            cfg!(windows),
        )
    }
    fn under(&self, candidate: &Path, root: &Path) -> bool {
        let mut candidate = candidate.components();
        root.components().all(|outer| {
            candidate.next().is_some_and(|inner| {
                unicode::normcase(&filesystem::os_string(inner.as_os_str()), cfg!(windows))
                    == unicode::normcase(&filesystem::os_string(outer.as_os_str()), cfg!(windows))
            })
        })
    }
    fn exists(&self, path: &Path, follow: bool) -> EngineResult<bool> {
        Ok(metadata(path, follow)?.is_some())
    }
    fn is_directory(&self, path: &Path) -> EngineResult<bool> {
        Ok(metadata(path, true)?.is_some_and(|value| value.is_dir()))
    }
    fn is_file(&self, path: &Path) -> EngineResult<bool> {
        Ok(metadata(path, true)?.is_some_and(|value| value.is_file()))
    }
    fn is_symlink(&self, path: &Path) -> EngineResult<bool> {
        Ok(metadata(path, false)?.is_some_and(|value| value.file_type().is_symlink()))
    }
    fn is_reparse(&self, path: &Path) -> EngineResult<bool> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            Ok(fs::symlink_metadata(path)?.file_attributes() & 0x400 != 0)
        }
        #[cfg(unix)]
        {
            let _ = path;
            Ok(false)
        }
    }
    fn ensure_safe(&self, path: &Path, stop: Option<&Path>) -> EngineResult<()> {
        filesystem::ensure_safe_components(path, stop).map_err(failure)
    }
    fn entries(&self, directory: &Path) -> EngineResult<Vec<PathBuf>> {
        Ok(fs::read_dir(directory)?
            .map(|entry| entry.map(|value| value.path()))
            .collect::<io::Result<Vec<_>>>()?)
    }
    fn make_directory(&self, path: &Path, parents: bool, mode: u32) -> EngineResult<()> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(parents);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(mode);
        }
        #[cfg(windows)]
        {
            let _ = mode;
        }
        match builder.create(path) {
            Ok(()) => Ok(()),
            Err(error)
                if error.kind() == io::ErrorKind::AlreadyExists && self.is_directory(path)? =>
            {
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
    fn chmod(&self, path: &Path, mode: u32) -> EngineResult<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        }
        #[cfg(windows)]
        {
            let _ = (path, mode);
        }
        Ok(())
    }
    fn write_new_sync(&self, path: &Path, bytes: &[u8], mode: u32) -> EngineResult<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(mode);
        }
        #[cfg(windows)]
        {
            let _ = mode;
        }
        let mut file = match options.open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    }
    fn same_file(&self, left: &Path, right: &Path) -> EngineResult<bool> {
        Ok(filesystem::identity(left)? == filesystem::identity(right)?)
    }
    fn environment(&self, key: &str) -> Option<PyString> {
        match &self.environment_override {
            Some(values) => values.get(key).cloned(),
            None => std::env::var_os(key).map(|value| filesystem::os_string(&value)),
        }
    }
    fn windows(&self) -> bool {
        cfg!(windows)
    }
}
