use crate::{
    application::snapshots::{FileIdentity, FileSnapshotPort, Snapshot, SnapshotFailure},
    compat::{unicode, PyString, Value},
    domain::{
        contracts::{self, ContractError},
        ids,
    },
};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};

pub struct NativeFileSnapshots;
fn ordinary_resolved(path: PathBuf, preserve_verbatim: bool) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        if !preserve_verbatim {
            let mut parts = path.components();
            let replacement = match parts.next() {
                Some(Component::Prefix(prefix)) => match prefix.kind() {
                    Prefix::VerbatimDisk(drive) => {
                        Some(PathBuf::from(format!("{}:\\", drive as char)))
                    }
                    Prefix::VerbatimUNC(server, share) => {
                        Some(Path::new("\\\\").join(server).join(share))
                    }
                    _ => None,
                },
                _ => None,
            };
            if let Some(mut result) = replacement {
                for part in parts {
                    if part != Component::RootDir {
                        result.push(part.as_os_str());
                    }
                }
                return python_path(&result);
            }
        }
    }
    #[cfg(unix)]
    {
        let _ = preserve_verbatim;
    }
    python_path(&path)
}
pub fn resolve_path(path: &Path, strict: bool) -> io::Result<PathBuf> {
    let full = absolute(path)?;
    let preserve_verbatim = os_string(full.as_os_str()).starts_with("\\\\?\\");
    let mut current = full;
    let mut tail = Vec::new();
    loop {
        match fs::canonicalize(&current) {
            Ok(resolved) => {
                let mut resolved = ordinary_resolved(resolved, preserve_verbatim);
                for part in tail.iter().rev() {
                    resolved.push(part);
                }
                return Ok(python_path(&resolved));
            }
            Err(error)
                if !strict
                    && matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) =>
            {
                let Some(name) = current.file_name().map(std::ffi::OsString::from) else {
                    return Err(error);
                };
                tail.push(name);
                if !current.pop() {
                    return Err(error);
                }
            }
            Err(error) => return Err(error),
        }
    }
}
pub fn resolve_python_path(
    path: &Path,
    strict: bool,
) -> crate::application::error::EngineResult<PathBuf> {
    if os_string(path.as_os_str()).0.contains(&0) {
        return Err(crate::application::error::EngineError::python(
            "ValueError",
            "stat: embedded null character in path",
        ));
    }
    Ok(resolve_path(path, strict)?)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotStage {
    BeforeOpen,
    AfterOpen,
    AfterRead,
    BeforeFinalPath,
}

pub fn os_string(value: &std::ffi::OsStr) -> PyString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        PyString(
            std::char::decode_utf16(value.encode_wide())
                .map(|r| {
                    r.map(u32::from)
                        .unwrap_or_else(|e| u32::from(e.unpaired_surrogate()))
                })
                .collect(),
        )
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bytes = value.as_bytes();
        let mut points = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            match std::str::from_utf8(&bytes[offset..]) {
                Ok(text) => {
                    points.extend(text.chars().map(u32::from));
                    break;
                }
                Err(error) => {
                    let end = offset + error.valid_up_to();
                    points.extend(
                        std::str::from_utf8(&bytes[offset..end])
                            .unwrap()
                            .chars()
                            .map(u32::from),
                    );
                    offset = end;
                    let length = error.error_len().unwrap_or(bytes.len() - offset);
                    points.extend(
                        bytes[offset..offset + length]
                            .iter()
                            .map(|&b| 0xdc00 + u32::from(b)),
                    );
                    offset += length;
                }
            }
        }
        PyString(points)
    }
}
fn path_eq(left: &Path, right: &Path) -> bool {
    unicode::normcase(&os_string(left.as_os_str()), cfg!(windows))
        == unicode::normcase(&os_string(right.as_os_str()), cfg!(windows))
}
pub fn python_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        path.components().collect()
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let normalized: PathBuf = path.components().collect();
        let bytes = path.as_os_str().as_bytes();
        if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
            Path::new("//").join(normalized.strip_prefix("/").unwrap_or(&normalized))
        } else {
            normalized
        }
    }
}
pub(crate) fn absolute(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(python_path(path))
    } else {
        Ok(python_path(&std::env::current_dir()?.join(path)))
    }
}
fn relative(candidate: &Path, root: &Path) -> Option<PyString> {
    let left: Vec<_> = candidate.components().collect();
    let right: Vec<_> = root.components().collect();
    if right.len() > left.len()
        || !left.iter().zip(&right).all(|(a, b)| {
            unicode::normcase(&os_string(a.as_os_str()), cfg!(windows))
                == unicode::normcase(&os_string(b.as_os_str()), cfg!(windows))
        })
    {
        return None;
    }
    let mut result = Vec::new();
    for component in &left[right.len()..] {
        if !result.is_empty() {
            result.push(47);
        }
        result.extend(os_string(component.as_os_str()).0);
    }
    Some(PyString(result))
}
pub(crate) fn reparse(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(unix)]
    {
        let _ = metadata;
        false
    }
}
pub fn ensure_safe_components(path: &Path, stop: Option<&Path>) -> Result<(), SnapshotFailure> {
    let mut components = Vec::new();
    for component in path.ancestors() {
        components.push(component);
        if stop.is_some_and(|root| path_eq(root, component)) {
            break;
        }
    }
    for component in components.into_iter().rev() {
        match fs::symlink_metadata(component) {
            Ok(metadata) if metadata.file_type().is_symlink() || reparse(&metadata) => {
                return Err(ContractError::new("path", "LINK_OR_REPARSE_POINT").into())
            }
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => {
                return Err(super::os_error::with_path(
                    error,
                    super::os_error::OsApi::Native,
                    Some(component),
                    None,
                )
                .into())
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileState {
    pub(crate) identity: FileIdentity,
    pub(crate) size: u64,
    pub(crate) modified: i128,
    pub(crate) regular: bool,
    pub(crate) reparse: bool,
    pub(crate) symlink: bool,
}
pub(crate) fn file_state(file: &File) -> io::Result<FileState> {
    let metadata = file.metadata().map_err(|error| {
        super::os_error::with_path(error, super::os_error::OsApi::Native, None, None)
    })?;
    #[cfg(windows)]
    {
        use std::{
            mem::MaybeUninit,
            os::windows::{fs::MetadataExt, io::AsRawHandle},
        };
        use windows_sys::Win32::Storage::FileSystem::{
            FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO,
        };
        let mut identity = MaybeUninit::<FILE_ID_INFO>::zeroed();
        let success = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle() as _,
                FileIdInfo,
                identity.as_mut_ptr() as _,
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        };
        if success == 0 {
            return Err(super::os_error::with_path(
                io::Error::last_os_error(),
                super::os_error::OsApi::Native,
                None,
                None,
            ));
        }
        let identity = unsafe { identity.assume_init() };
        Ok(FileState {
            identity: FileIdentity {
                device: identity.VolumeSerialNumber,
                inode: u128::from_le_bytes(identity.FileId.Identifier),
            },
            size: metadata.len(),
            modified: (i128::from(metadata.last_write_time()) - 116444736000000000) * 100,
            regular: metadata.is_file(),
            reparse: reparse(&metadata),
            symlink: metadata.file_type().is_symlink(),
        })
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(FileState {
            identity: FileIdentity {
                device: metadata.dev(),
                inode: u128::from(metadata.ino()),
            },
            size: metadata.len(),
            modified: i128::from(metadata.mtime()) * 1_000_000_000
                + i128::from(metadata.mtime_nsec()),
            regular: metadata.is_file(),
            reparse: false,
            symlink: metadata.file_type().is_symlink(),
        })
    }
}
fn path_state(path: &Path) -> io::Result<FileState> {
    path_state_mode(path, false)
}
pub(crate) fn usage_path_state(path: &Path) -> io::Result<FileState> {
    path_state_mode(path, true)
}
fn path_state_mode(path: &Path, follow: bool) -> io::Result<FileState> {
    let metadata = if follow {
        fs::metadata(path)
    } else {
        fs::symlink_metadata(path)
    }
    .map_err(|error| {
        super::os_error::with_path(error, super::os_error::OsApi::Native, Some(path), None)
    })?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        let file = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(
                FILE_FLAG_BACKUP_SEMANTICS
                    | if follow {
                        0
                    } else {
                        FILE_FLAG_OPEN_REPARSE_POINT
                    },
            )
            .open(path)
            .map_err(|error| {
                super::os_error::with_path(error, super::os_error::OsApi::Native, Some(path), None)
            })?;
        let mut state = file_state(&file)?;
        state.regular = metadata.is_file();
        state.reparse = reparse(&metadata);
        state.symlink = metadata.file_type().is_symlink();
        Ok(state)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(FileState {
            identity: FileIdentity {
                device: metadata.dev(),
                inode: u128::from(metadata.ino()),
            },
            size: metadata.len(),
            modified: i128::from(metadata.mtime()) * 1_000_000_000
                + i128::from(metadata.mtime_nsec()),
            regular: metadata.is_file(),
            reparse: false,
            symlink: metadata.file_type().is_symlink(),
        })
    }
}
pub fn identity(path: &Path) -> io::Result<FileIdentity> {
    Ok(path_state(path)?.identity)
}

impl FileSnapshotPort for NativeFileSnapshots {
    fn snapshot(
        &self,
        path: &Path,
        roots: &[PathBuf],
        maximum: usize,
    ) -> Result<Snapshot, SnapshotFailure> {
        snapshot_observed(path, roots, maximum, &|_| {})
    }
}
#[doc(hidden)]
pub fn snapshot_observed(
    path: &Path,
    roots: &[PathBuf],
    maximum: usize,
    observe: &dyn Fn(SnapshotStage),
) -> Result<Snapshot, SnapshotFailure> {
    snapshot_integer_observed(path, roots, &num_bigint::BigInt::from(maximum), observe)
}
impl crate::application::snapshots::IntegerFileSnapshotPort for NativeFileSnapshots {
    fn snapshot_integer(
        &self,
        path: &Path,
        roots: &[PathBuf],
        maximum: &num_bigint::BigInt,
    ) -> Result<Snapshot, SnapshotFailure> {
        snapshot_integer_observed(path, roots, maximum, &|_| {})
    }
}
#[doc(hidden)]
pub fn snapshot_integer_observed(
    path: &Path,
    roots: &[PathBuf],
    maximum: &num_bigint::BigInt,
    observe: &dyn Fn(SnapshotStage),
) -> Result<Snapshot, SnapshotFailure> {
    snapshot_integer_checked(path, roots, maximum, observe, false)
}
#[doc(hidden)]
pub fn snapshot_integer_final_identity_fault(
    path: &Path,
    roots: &[PathBuf],
    maximum: &num_bigint::BigInt,
) -> Result<Snapshot, SnapshotFailure> {
    snapshot_integer_checked(path, roots, maximum, &|_| {}, true)
}
fn snapshot_integer_checked(
    path: &Path,
    roots: &[PathBuf],
    maximum: &num_bigint::BigInt,
    observe: &dyn Fn(SnapshotStage),
    force_final_identity_mismatch: bool,
) -> Result<Snapshot, SnapshotFailure> {
    use num_bigint::BigInt;
    use num_traits::ToPrimitive;
    let candidate = absolute(path)?;
    let mut allowed = false;
    for root in roots {
        let root = absolute(root)?;
        if let Some(path) = relative(&candidate, &root) {
            if !path_eq(&candidate, &root) {
                contracts::lexical_relative(&Value::String(path), "path")?;
            }
            ensure_safe_components(&candidate, Some(&root))?;
            allowed = true;
            break;
        }
    }
    if !allowed {
        return Err(ContractError::new("path", "OUTSIDE_ALLOWED_ROOT").into());
    }
    let pre = path_state(&candidate)?;
    if !pre.regular || pre.symlink || pre.reparse {
        return Err(ContractError::new("path", "NOT_REGULAR_FILE").into());
    }
    if BigInt::from(pre.size) > *maximum {
        return Err(ContractError::new("path", "FILE_TOO_LARGE").into());
    }
    observe(SnapshotStage::BeforeOpen);
    let mut file = super::snapshot_file::open(&candidate)?;
    let opened = file_state(&file)?;
    if !opened.regular || BigInt::from(opened.size) > *maximum {
        return Err(ContractError::new("path", "NOT_REGULAR_FILE").into());
    }
    if pre.identity != opened.identity {
        return Err(ContractError::new("path", "FILE_CHANGED").into());
    }
    observe(SnapshotStage::AfterOpen);
    let mut data = Vec::new();
    loop {
        let remaining = maximum + BigInt::from(1) - BigInt::from(data.len());
        let length = remaining.min(BigInt::from(65536)).to_usize().unwrap();
        let mut chunk = vec![0; length];
        let length = file.read(&mut chunk)?;
        if length == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..length]);
        if BigInt::from(data.len()) > *maximum {
            return Err(ContractError::new("path", "FILE_TOO_LARGE").into());
        }
    }
    observe(SnapshotStage::AfterRead);
    let post = file_state(&file)?;
    if (opened.identity, opened.size, opened.modified)
        != (post.identity.clone(), post.size, post.modified)
    {
        return Err(ContractError::new("path", "FILE_CHANGED").into());
    }
    observe(SnapshotStage::BeforeFinalPath);
    let final_path = path_state(&candidate)?;
    if final_path.symlink
        || final_path.reparse
        || final_path.identity != post.identity
        || force_final_identity_mismatch
    {
        return Err(ContractError::new("path", "FILE_CHANGED").into());
    }
    let size = data.len();
    let digest = ids::sha256_bytes(&data);
    Ok(Snapshot {
        path: candidate,
        data,
        digest,
        size,
        parsed: None,
        identity: pre.identity,
    })
}
