#[cfg(unix)]
use super::os_error::{self, OsApi};
use std::{io, path::Path};

#[cfg(windows)]
pub(crate) type File = super::snapshot_file::SnapshotFile;
#[cfg(unix)]
pub(crate) type File = std::fs::File;

#[cfg(windows)]
pub(crate) fn create_new(path: &Path) -> io::Result<File> {
    super::snapshot_file::open_flags(path, 2 | 0x100 | 0x400)
}
#[cfg(unix)]
pub(crate) fn create_new(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| os_error::with_path(error, OsApi::Native, Some(path), None))
}

#[cfg(windows)]
pub(crate) fn open_lock(path: &Path) -> io::Result<File> {
    super::snapshot_file::open_flags(path, 2 | 0x100)
}
#[cfg(unix)]
pub(crate) fn open_lock(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| os_error::with_path(error, OsApi::Native, Some(path), None))
}

#[cfg(windows)]
pub(crate) fn close(file: File) -> io::Result<()> {
    file.close()
}
#[cfg(unix)]
pub(crate) fn close(file: File) -> io::Result<()> {
    use std::os::fd::IntoRawFd;
    if unsafe { libc::close(file.into_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(os_error::with_path(
            io::Error::last_os_error(),
            OsApi::Native,
            None,
            None,
        ))
    }
}

pub(crate) fn writable_directory(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    #[cfg(windows)]
    {
        true
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
    }
}
