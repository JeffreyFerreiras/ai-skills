#[cfg(unix)]
pub fn open(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| {
            super::os_error::with_path(error, super::os_error::OsApi::Native, Some(path), None)
        })
}

#[cfg(windows)]
mod windows {
    use std::{
        fs::File,
        io::{self, Read, Seek, SeekFrom, Write},
        mem::ManuallyDrop,
        ops::Deref,
        os::windows::{ffi::OsStrExt, io::FromRawHandle},
        path::Path,
    };
    unsafe extern "C" {
        fn _wopen(path: *const u16, flags: i32, ...) -> i32;
        fn _read(descriptor: i32, buffer: *mut std::ffi::c_void, length: u32) -> i32;
        fn _write(descriptor: i32, buffer: *const std::ffi::c_void, length: u32) -> i32;
        fn _commit(descriptor: i32) -> i32;
        fn _lseeki64(descriptor: i32, offset: i64, origin: i32) -> i64;
        fn _locking(descriptor: i32, mode: i32, length: i32) -> i32;
        fn _close(descriptor: i32) -> i32;
        fn _get_osfhandle(descriptor: i32) -> isize;
        fn _errno() -> *mut i32;
    }
    pub struct SnapshotFile {
        descriptor: i32,
        borrowed_file: ManuallyDrop<File>,
    }
    impl Deref for SnapshotFile {
        type Target = File;
        fn deref(&self) -> &File {
            &self.borrowed_file
        }
    }
    impl Read for SnapshotFile {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = u32::try_from(buffer.len()).expect("snapshot chunks fit CRT read");
            let result = unsafe { _read(self.descriptor, buffer.as_mut_ptr().cast(), count) };
            if result < 0 {
                let errno = unsafe { *_errno() };
                Err(super::super::os_error::from_crt_errno(errno, None))
            } else {
                Ok(result as usize)
            }
        }
    }
    impl Write for SnapshotFile {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            let count = u32::try_from(buffer.len()).expect("bounded writes fit CRT");
            let result = unsafe { _write(self.descriptor, buffer.as_ptr().cast(), count) };
            if result < 0 {
                Err(super::super::os_error::from_crt_errno(
                    unsafe { *_errno() },
                    None,
                ))
            } else {
                Ok(result as usize)
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Seek for SnapshotFile {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            let (offset, origin) = match position {
                SeekFrom::Start(offset) => (
                    i64::try_from(offset).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "seek offset overflow")
                    })?,
                    0,
                ),
                SeekFrom::Current(offset) => (offset, 1),
                SeekFrom::End(offset) => (offset, 2),
            };
            let result = unsafe { _lseeki64(self.descriptor, offset, origin) };
            if result < 0 {
                Err(super::super::os_error::from_crt_errno(
                    unsafe { *_errno() },
                    None,
                ))
            } else {
                Ok(result as u64)
            }
        }
    }
    impl SnapshotFile {
        pub fn sync_all(&self) -> io::Result<()> {
            if unsafe { _commit(self.descriptor) } != 0 {
                Err(super::super::os_error::from_crt_errno(
                    unsafe { *_errno() },
                    None,
                ))
            } else {
                Ok(())
            }
        }
        pub fn lock_byte(&mut self, unlock: bool) -> io::Result<()> {
            self.seek(SeekFrom::Start(0))?;
            if unsafe { _locking(self.descriptor, if unlock { 0 } else { 1 }, 1) } != 0 {
                Err(super::super::os_error::from_crt_errno(
                    unsafe { *_errno() },
                    None,
                ))
            } else {
                Ok(())
            }
        }
        pub fn close(mut self) -> io::Result<()> {
            let descriptor = std::mem::replace(&mut self.descriptor, -1);
            if unsafe { _close(descriptor) } != 0 {
                Err(super::super::os_error::from_crt_errno(
                    unsafe { *_errno() },
                    None,
                ))
            } else {
                Ok(())
            }
        }
    }
    impl Drop for SnapshotFile {
        fn drop(&mut self) {
            if self.descriptor >= 0 {
                unsafe {
                    _close(self.descriptor);
                }
            }
        }
    }
    pub fn open_flags(path: &Path, flags: i32) -> io::Result<SnapshotFile> {
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>();
        let descriptor = unsafe { _wopen(wide.as_ptr(), flags | 0x8000 | 0x0080, 0o600_i32) };
        if descriptor < 0 {
            let errno = unsafe { *_errno() };
            return Err(super::super::os_error::from_crt_errno(errno, Some(path)));
        }
        let handle = unsafe { _get_osfhandle(descriptor) };
        if handle == -1 {
            let errno = unsafe { *_errno() };
            unsafe {
                _close(descriptor);
            }
            return Err(super::super::os_error::from_crt_errno(errno, None));
        }
        Ok(SnapshotFile {
            descriptor,
            borrowed_file: ManuallyDrop::new(unsafe { File::from_raw_handle(handle as _) }),
        })
    }
    pub fn open(path: &Path) -> io::Result<SnapshotFile> {
        open_flags(path, 0)
    }
}
#[cfg(windows)]
pub use windows::open;
#[cfg(windows)]
pub(crate) use windows::{open_flags, SnapshotFile};
