use super::{
    os_error::{self, OsApi},
    policy_files::NativePolicyFiles,
    python_temporary,
    state_platform::NativeStatePlatform,
    temporary_files,
};
use crate::{
    application::{
        error::{EngineError, EngineResult},
        helper_repository::HelperFileTransactions,
        persistent_store::StatePlatform,
        policy::PolicyFilePort,
        state::FaultHook,
    },
    compat::Value,
    domain::helper_register::HelperRegisterError,
};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

pub struct NativeHelperFileTransactions;
fn unsafe_lock() -> EngineError {
    HelperRegisterError::new("REGISTER_LOCK_UNSAFE").into()
}

impl HelperFileTransactions for NativeHelperFileTransactions {
    fn temporary_root(&self) -> EngineResult<PathBuf> {
        python_temporary::temporary_root()
    }

    fn atomic_write(&self, path: &Path, payload: &[u8], fault: &dyn FaultHook) -> EngineResult<()> {
        let parent = path.parent().unwrap();
        let (file, temporary) = python_temporary::create_file(parent, ".register.", ".tmp")?;
        let mut descriptor = Some(file);
        let outcome = (|| {
            let file = descriptor.as_mut().unwrap();
            #[cfg(unix)]
            {
                use std::os::fd::AsRawFd;
                if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
                    return Err(os_error::with_path(
                        io::Error::last_os_error(),
                        OsApi::Native,
                        None,
                        None,
                    )
                    .into());
                }
            }
            let mut offset = 0;
            while offset < payload.len() {
                let written = file.write(&payload[offset..])?;
                if written == 0 {
                    return Err(io::Error::other("short register write").into());
                }
                offset += written;
            }
            file.sync_all()?;
            fault.observe("before_replace")?;
            temporary_files::close(descriptor.take().unwrap())?;
            std::fs::rename(&temporary, path).map_err(|error| {
                os_error::with_path(error, OsApi::Native, Some(&temporary), Some(path))
            })?;
            #[cfg(unix)]
            {
                let directory = std::fs::File::open(parent).map_err(|error| {
                    os_error::with_path(error, OsApi::Native, Some(parent), None)
                })?;
                let synchronized = directory
                    .sync_all()
                    .map_err(|error| os_error::with_path(error, OsApi::Native, None, None));
                temporary_files::close(directory)?;
                synchronized?;
            }
            Ok(())
        })();
        if let Some(file) = descriptor {
            temporary_files::close(file)?;
        }
        outcome
    }

    fn exclusive_lock(
        &self,
        path: &Path,
        action: &mut dyn FnMut() -> EngineResult<Value>,
    ) -> EngineResult<Value> {
        let files = NativePolicyFiles::default();
        if files.exists(path, true)? {
            files.ensure_safe(path, path.parent())?;
            let info = NativeStatePlatform.info(path, false)?;
            if !info.regular || info.symlink {
                return Err(unsafe_lock());
            }
        }
        let mut descriptor = temporary_files::open_lock(path)?;
        let outcome = (|| {
            let metadata = descriptor
                .metadata()
                .map_err(|error| os_error::with_path(error, OsApi::Native, None, None))?;
            if !metadata.is_file() {
                return Err(unsafe_lock());
            }
            #[cfg(windows)]
            {
                let opened = descriptor
                    .metadata()
                    .map_err(|error| os_error::with_path(error, OsApi::Native, None, None))?;
                if opened.len() == 0 {
                    descriptor.write_all(&[0])?;
                    descriptor.sync_all()?;
                }
            }
            lock(&mut descriptor, false)?;
            action()
        })();
        if let Err(error) = lock(&mut descriptor, true) {
            // The source closes only after a successful unlock, including failure cleanup.
            std::mem::forget(descriptor);
            return Err(error.into());
        }
        temporary_files::close(descriptor)?;
        outcome
    }
}

#[cfg(windows)]
fn lock(file: &mut temporary_files::File, unlock: bool) -> io::Result<()> {
    file.lock_byte(unlock)
}
#[cfg(unix)]
fn lock(file: &mut temporary_files::File, unlock: bool) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    loop {
        if unsafe {
            libc::flock(
                file.as_raw_fd(),
                if unlock { libc::LOCK_UN } else { libc::LOCK_EX },
            )
        } == 0
        {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(os_error::with_path(error, OsApi::Native, None, None));
        }
    }
}
