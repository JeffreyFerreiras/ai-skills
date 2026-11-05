use super::filesystem;
use crate::{
    application::{
        error::{EngineError, EngineResult},
        persistent_store::{StateFileInfo, StatePlatform},
        snapshots::SnapshotFailure,
    },
    compat::{self, PyString, Value},
    domain::ids,
};
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub struct NativeStatePlatform;

#[cfg(windows)]
fn locality_with_probe(
    path: &Path,
    drive_type: impl FnOnce(&[u16]) -> u32,
) -> EngineResult<String> {
    use std::os::windows::ffi::OsStrExt;

    let resolved = filesystem::resolve_path(path, true)?;
    let text = NativeStatePlatform.spelling(&resolved);
    if text.starts_with("\\\\") {
        return Err(EngineError::state("NON_LOCAL_FILESYSTEM"));
    }
    let anchor: PathBuf = resolved.components().take(2).collect();
    let wide = anchor
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    if drive_type(&wide) != 3 {
        return Err(EngineError::state("NON_LOCAL_FILESYSTEM"));
    }
    let mut anchor = NativeStatePlatform.spelling(&anchor);
    while anchor.0.last() == Some(&92) {
        anchor.0.pop();
    }
    for point in &mut anchor.0 {
        if (97..=122).contains(point) {
            *point -= 32;
        }
    }
    let anchor = Value::String(anchor).text().unwrap();
    Ok(format!("windows_fixed_drive:{anchor}"))
}

#[cfg(unix)]
fn locality_with_probe(
    path: &Path,
    filesystem_type: impl FnOnce(&Path) -> Option<String>,
) -> EngineResult<String> {
    use std::os::unix::fs::MetadataExt;

    let resolved = filesystem::resolve_path(path, true)?;
    let fs_type = filesystem_type(&resolved);
    if fs_type.as_ref().is_some_and(|value| {
        [
            "9p",
            "afs",
            "ceph",
            "cifs",
            "davfs",
            "gcsfuse",
            "glusterfs",
            "nfs",
            "nfs4",
            "smb3",
            "sshfs",
        ]
        .contains(&value.as_str())
            || value.starts_with("fuse")
    }) {
        return Err(EngineError::state("NON_LOCAL_FILESYSTEM"));
    }
    Ok(format!(
        "posix_local:{}:{}",
        fs_type.as_deref().unwrap_or("lock_probe"),
        fs::metadata(resolved)?.dev()
    ))
}

fn snapshot_error(error: SnapshotFailure) -> EngineError {
    match error {
        SnapshotFailure::Contract(error) => error.into(),
        SnapshotFailure::Io(error) => error.into(),
        SnapshotFailure::Json(error) => error.into(),
    }
}
fn actor() -> EngineResult<PyString> {
    for key in ["LOGNAME", "USER", "LNAME", "USERNAME"] {
        if let Some(value) = std::env::var_os(key).filter(|value| !value.is_empty()) {
            return Ok(filesystem::os_string(&value));
        }
    }
    #[cfg(unix)]
    {
        use std::ffi::CStr;
        let entry = unsafe { libc::getpwuid(libc::getuid()) };
        if !entry.is_null() {
            return Ok(filesystem::os_string(std::ffi::OsStr::new(
                &unsafe { CStr::from_ptr((*entry).pw_name) }
                    .to_string_lossy()
                    .into_owned(),
            )));
        }
    }
    Err(EngineError::python(
        "ModuleNotFoundError",
        "No module named 'pwd'",
    ))
}
pub fn current_actor() -> EngineResult<PyString> {
    let value = actor()?;
    Ok(if value.0.is_empty() {
        "unknown-user".into()
    } else {
        value
    })
}
fn machine_identity() -> EngineResult<(String, String, String)> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{
            ComputerNameDnsHostname, GetComputerNameExW,
        };
        let mut size = 0;
        unsafe {
            GetComputerNameExW(ComputerNameDnsHostname, std::ptr::null_mut(), &mut size);
        }
        let mut buffer = vec![0; size as usize];
        if unsafe { GetComputerNameExW(ComputerNameDnsHostname, buffer.as_mut_ptr(), &mut size) }
            == 0
        {
            return Err(io::Error::last_os_error().into());
        }
        let node = String::from_utf16(&buffer[..size as usize])
            .map_err(|_| EngineError::python("UnicodeEncodeError", "surrogates not allowed"))?;
        let machine = std::env::var("PROCESSOR_ARCHITEW6432")
            .or_else(|_| std::env::var("PROCESSOR_ARCHITECTURE"))
            .unwrap_or_default();
        Ok(("Windows".into(), node, machine))
    }
    #[cfg(unix)]
    {
        use std::{ffi::CStr, mem::MaybeUninit};
        let mut value = MaybeUninit::<libc::utsname>::zeroed();
        if unsafe { libc::uname(value.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error().into());
        }
        let value = unsafe { value.assume_init() };
        let text = |field: &[libc::c_char]| {
            unsafe { CStr::from_ptr(field.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        };
        Ok((
            text(&value.sysname),
            text(&value.nodename),
            text(&value.machine),
        ))
    }
}
impl StatePlatform for NativeStatePlatform {
    fn windows(&self) -> bool {
        cfg!(windows)
    }
    fn spelling(&self, path: &Path) -> PyString {
        filesystem::os_string(path.as_os_str())
    }
    fn absolute(&self, path: &Path) -> EngineResult<PathBuf> {
        Ok(filesystem::python_path(&if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        }))
    }
    fn resolve(&self, path: &Path, strict: bool) -> EngineResult<PathBuf> {
        filesystem::resolve_python_path(path, strict)
    }
    fn ensure_safe(&self, path: &Path, stop: Option<&Path>) -> EngineResult<()> {
        filesystem::ensure_safe_components(path, stop).map_err(snapshot_error)
    }
    fn info(&self, path: &Path, follow: bool) -> EngineResult<StateFileInfo> {
        let metadata = if follow {
            fs::metadata(path)?
        } else {
            fs::symlink_metadata(path)?
        };
        let identity = filesystem::identity(path)?;
        #[cfg(unix)]
        let (owner, mode) = {
            use std::os::unix::fs::MetadataExt;
            (Some(metadata.uid()), metadata.mode() & 0o7777)
        };
        #[cfg(windows)]
        let (owner, mode) = (None, 0);
        Ok(StateFileInfo {
            device: identity.device,
            inode: identity.inode,
            size: metadata.len(),
            regular: metadata.is_file(),
            directory: metadata.is_dir(),
            symlink: fs::symlink_metadata(path)?.file_type().is_symlink(),
            owner,
            mode,
        })
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn create_directory(&self, path: &Path, parents: bool) -> EngineResult<()> {
        if parents {
            fs::create_dir_all(path)?;
        } else {
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let mut builder = builder;
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(path)?;
        }
        Ok(())
    }
    fn set_mode(&self, path: &Path, mode: u32) -> EngineResult<()> {
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
    fn effective_user(&self) -> Option<u32> {
        #[cfg(unix)]
        {
            Some(unsafe { libc::geteuid() })
        }
        #[cfg(windows)]
        {
            None
        }
    }
    fn entries(&self, path: &Path) -> EngineResult<Vec<PathBuf>> {
        fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()).map_err(Into::into))
            .collect()
    }
    fn read_text(&self, path: &Path) -> EngineResult<PyString> {
        let bytes = fs::read(path)?;
        // json.loads(bytes) permits surrogatepass; text-mode UTF-8 input does not.
        let text = std::str::from_utf8(&bytes).map_err(|error| {
            let start = error.valid_up_to();
            let end = start + error.error_len().unwrap_or(bytes.len() - start);
            let reason = if error.error_len().is_none() {
                "unexpected end of data"
            } else if bytes[start] >= 0xc2 && bytes[start] <= 0xf4 {
                "invalid continuation byte"
            } else {
                "invalid start byte"
            };
            let location = if end == start + 1 {
                format!("byte 0x{:02x} in position {start}", bytes[start])
            } else {
                format!("bytes in position {start}-{}", end - 1)
            };
            EngineError::python(
                "UnicodeDecodeError",
                format!("'utf-8' codec can't decode {location}: {reason}"),
            )
        })?;
        Ok(text.replace("\r\n", "\n").replace('\r', "\n").into())
    }
    fn write_synced(&self, path: &Path, bytes: &[u8], exclusive: bool) -> EngineResult<()> {
        let mut options = OpenOptions::new();
        options.write(true);
        if exclusive {
            options.create_new(true);
        } else {
            options.create(true).truncate(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        if exclusive {
            file.write(bytes)?;
        } else {
            file.write_all(bytes)?;
        }
        file.sync_all()?;
        Ok(())
    }
    fn remove_file(&self, path: &Path) -> EngineResult<()> {
        fs::remove_file(path)?;
        Ok(())
    }
    fn replace(&self, source: &Path, target: &Path) -> EngineResult<()> {
        fs::rename(source, target)?;
        Ok(())
    }
    fn flush_file(&self, path: &Path) -> EngineResult<()> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?
            .sync_all()?;
        Ok(())
    }
    fn directory_sync(&self, directory: &Path, acknowledged: bool) -> EngineResult<String> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_DIRECTORY);
        }
        let file = match options.open(directory) {
            Ok(file) => file,
            Err(error) => {
                #[cfg(windows)]
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied
                        | io::ErrorKind::InvalidInput
                        | io::ErrorKind::Unsupported
                ) || matches!(error.raw_os_error(), Some(1 | 5 | 50 | 87))
                {
                    return if acknowledged {
                        Ok("directory_fsync_unavailable_windows".into())
                    } else {
                        Err(EngineError::state("DEGRADED_DURABILITY_ACK_REQUIRED"))
                    };
                }
                return Err(EngineError::state("DIRECTORY_SYNC_FAILED"));
            }
        };
        if let Err(error) = file.sync_all() {
            #[cfg(unix)]
            let unsupported = matches!(error.raw_os_error(), Some(libc::EINVAL | libc::ENOTSUP));
            #[cfg(windows)]
            let unsupported = matches!(error.raw_os_error(), Some(50 | 87));
            if unsupported {
                return if acknowledged {
                    Ok("directory_fsync_unsupported".into())
                } else {
                    Err(EngineError::state("DEGRADED_DURABILITY_ACK_REQUIRED"))
                };
            }
            return Err(EngineError::state("DIRECTORY_SYNC_FAILED"));
        }
        Ok("verified".into())
    }
    fn locality(&self, path: &Path) -> EngineResult<String> {
        #[cfg(windows)]
        return locality_with_probe(path, |anchor| unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(anchor.as_ptr())
        });
        #[cfg(unix)]
        locality_with_probe(path, linux_filesystem_type)
    }
    fn host_identity(&self) -> EngineResult<String> {
        let (system, node, machine) = machine_identity()?;
        let mut value = PyString::from(format!("{system}\0{node}\0{machine}\0"));
        value.0.extend(actor()?.0);
        let mut bytes = b"graph-host-v1\0".to_vec();
        bytes.extend(compat::encode_utf8(&value)?);
        Ok(ids::sha256_bytes(&bytes))
    }
    fn process_id(&self) -> u32 {
        std::process::id()
    }
    fn process_alive(&self, value: &Value) -> EngineResult<bool> {
        let number = match value {
            Value::Integer(value) => value.clone(),
            Value::Bool(value) => BigInt::from(i32::from(*value)),
            Value::Float(value) => {
                if *value <= 0.0 {
                    return Ok(false);
                }
                if *value == f64::from(std::process::id()) {
                    return Ok(true);
                }
                return Err(EngineError::python(
                    "ArgumentError",
                    "argument 3: TypeError: Don't know how to convert parameter 3",
                ));
            }
            Value::ConstantNaN => {
                return Err(EngineError::python(
                    "ArgumentError",
                    "argument 3: TypeError: Don't know how to convert parameter 3",
                ))
            }
            _ => {
                return Err(EngineError::python(
                    "TypeError",
                    format!(
                        "'<=' not supported between instances of '{}' and 'int'",
                        value.python_type()
                    ),
                ))
            }
        };
        if number <= BigInt::zero() {
            return Ok(false);
        }
        if number == BigInt::from(std::process::id()) {
            return Ok(true);
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::OpenProcess};
            let pid = number.to_u32().ok_or_else(|| {
                EngineError::python(
                    "ArgumentError",
                    "argument 3: OverflowError: int too long to convert",
                )
            })?;
            let handle = unsafe { OpenProcess(0x1000, 0, pid) };
            if handle.is_null() {
                Ok(false)
            } else {
                unsafe {
                    CloseHandle(handle);
                }
                Ok(true)
            }
        }
        #[cfg(unix)]
        {
            let pid = number.to_i32().ok_or_else(|| {
                EngineError::python("OverflowError", "Python int too large to convert to C long")
            })?;
            if unsafe { libc::kill(pid, 0) } == 0 {
                Ok(true)
            } else {
                Ok(io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
            }
        }
    }
    fn temporary_name(&self) -> EngineResult<String> {
        let mut random = [0; 8];
        getrandom::getrandom(&mut random).map_err(|error| io::Error::other(error.to_string()))?;
        Ok(random
            .into_iter()
            .map(|byte| b"abcdefghijklmnopqrstuvwxyz0123456789_"[(byte as usize) % 37] as char)
            .collect())
    }
    fn utc_now(&self) -> EngineResult<String> {
        Ok(utc_timestamp(time::OffsetDateTime::now_utc()))
    }
}

pub fn utc_timestamp(now: time::OffsetDateTime) -> String {
    crate::domain::python_datetime::format_utc(now)
}
#[cfg(unix)]
fn linux_filesystem_type(path: &Path) -> Option<String> {
    let lines = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let mut best = String::new();
    let mut fs_type = None;
    for line in lines.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let fields = before.split_whitespace().collect::<Vec<_>>();
        let Some(kind) = after.split_whitespace().next() else {
            continue;
        };
        if fields.len() < 5 {
            continue;
        }
        let mount = fields[4].replace("\\040", " ").replace("\\134", "\\");
        if path.starts_with(&mount) && mount.len() >= best.len() {
            best = mount;
            fs_type = Some(kind.to_string());
        }
    }
    fs_type
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_locality_probe_rejects_known_network_filesystems() {
        let root = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let result = locality_with_probe(root.path(), |_| 4);
        #[cfg(unix)]
        let result = locality_with_probe(root.path(), |_| Some("nfs".into()));
        assert!(
            matches!(result, Err(EngineError::State(error)) if error.code == "NON_LOCAL_FILESYSTEM")
        );
    }
}
