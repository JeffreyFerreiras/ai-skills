use super::{filesystem, snapshot_file};
use crate::{
    application::{
        error::{EngineError, EngineResult},
        usage_source::{UsageBytes, UsageSourcePort},
    },
    domain::usage::Limits,
};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

#[cfg(windows)]
type UsageFile = snapshot_file::SnapshotFile;
#[cfg(unix)]
type UsageFile = fs::File;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageReadStage {
    SafeComponents,
    BeforeOpen,
    Opened,
    BeforeRead,
    BeforeVerify,
    BeforeFinalStat,
    FinalIdentity,
    BeforeClose,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UsageFault {
    pub unsafe_component: bool,
    pub opened_identity_changed: bool,
    pub opened_nonregular: bool,
    pub final_identity_changed: bool,
}

pub trait UsageReadObserver {
    fn observe(
        &self,
        stage: UsageReadStage,
        path: &Path,
        requested_flags: i32,
    ) -> EngineResult<UsageFault>;
}
impl<F> UsageReadObserver for F
where
    F: Fn(UsageReadStage, &Path, i32) -> EngineResult<UsageFault>,
{
    fn observe(
        &self,
        stage: UsageReadStage,
        path: &Path,
        requested_flags: i32,
    ) -> EngineResult<UsageFault> {
        self(stage, path, requested_flags)
    }
}

pub struct NativeUsageSource;
impl UsageSourcePort for NativeUsageSource {
    fn read_prefix(&self, path: &Path, limits: Limits) -> EngineResult<UsageBytes> {
        read_prefix_observed(path, limits, &|_, _: &Path, _| Ok(UsageFault::default()))
    }
}

pub fn requested_open_flags(nonblocking: i32) -> i32 {
    #[cfg(windows)]
    {
        0x8000 | nonblocking
    }
    #[cfg(unix)]
    {
        libc::O_RDONLY | libc::O_NOFOLLOW | nonblocking
    }
}

fn safe_components(path: &Path) -> EngineResult<()> {
    for component in path.ancestors() {
        let component = if component.as_os_str().is_empty() {
            Path::new(".")
        } else {
            component
        };
        let info = fs::symlink_metadata(component)?;
        if info.file_type().is_symlink() || filesystem::reparse(&info) {
            return Err(EngineError::state("USAGE_SOURCE_UNSAFE"));
        }
    }
    Ok(())
}

fn checked_components(
    path: &Path,
    observer: &dyn UsageReadObserver,
    flags: i32,
) -> EngineResult<()> {
    if observer
        .observe(UsageReadStage::SafeComponents, path, flags)?
        .unsafe_component
    {
        return Err(EngineError::state("USAGE_SOURCE_UNSAFE"));
    }
    safe_components(path)
}

struct BufferedUsageFile {
    file: UsageFile,
    buffer: Vec<u8>,
    position: usize,
    buffer_start: u64,
    raw_position: u64,
    capacity: usize,
}
impl BufferedUsageFile {
    fn new(file: UsageFile) -> io::Result<Self> {
        #[cfg(windows)]
        let capacity = 8192;
        #[cfg(unix)]
        let capacity = {
            use std::os::unix::fs::MetadataExt;
            usize::try_from(file.metadata()?.blksize())
                .unwrap_or(8 * 1024 * 1024)
                .clamp(8192, 8 * 1024 * 1024)
        };
        Ok(Self {
            file,
            buffer: Vec::new(),
            position: 0,
            buffer_start: 0,
            raw_position: 0,
            capacity,
        })
    }

    fn read(&mut self, amount: usize) -> io::Result<Vec<u8>> {
        let mut result = Vec::with_capacity(amount);
        while result.len() < amount {
            let remaining = amount - result.len();
            if self.position < self.buffer.len() {
                let length = remaining.min(self.buffer.len() - self.position);
                result.extend_from_slice(&self.buffer[self.position..self.position + length]);
                self.position += length;
            } else if remaining >= self.capacity {
                self.buffer.clear();
                self.position = 0;
                let length = remaining / self.capacity * self.capacity;
                let start = result.len();
                result.resize(start + length, 0);
                let count = self.file.read(&mut result[start..])?;
                result.truncate(start + count);
                self.raw_position += count as u64;
                if count == 0 {
                    break;
                }
            } else {
                self.buffer.resize(self.capacity, 0);
                self.buffer_start = self.raw_position;
                let count = self.file.read(&mut self.buffer)?;
                self.buffer.truncate(count);
                self.position = 0;
                self.raw_position += count as u64;
                if count == 0 {
                    break;
                }
            }
        }
        Ok(result)
    }

    fn rewind(&mut self) -> io::Result<()> {
        if self.position < self.buffer.len() && self.buffer_start == 0 {
            self.position = 0;
        } else {
            self.file.seek(SeekFrom::Start(0))?;
            self.raw_position = 0;
            self.buffer.clear();
            self.position = 0;
        }
        Ok(())
    }

    fn close(self) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.file.close()
        }
        #[cfg(unix)]
        {
            use std::os::fd::IntoRawFd;
            let descriptor = self.file.into_raw_fd();
            if unsafe { libc::close(descriptor) } == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }
}

fn sample(
    path: &Path,
    before: &filesystem::FileState,
    handle: &mut BufferedUsageFile,
    limits: Limits,
    observer: &dyn UsageReadObserver,
    flags: i32,
) -> EngineResult<UsageBytes> {
    let fault = observer.observe(UsageReadStage::Opened, path, flags)?;
    let opened = filesystem::file_state(&handle.file)?;
    if !opened.regular
        || fault.opened_nonregular
        || opened.identity != before.identity
        || fault.opened_identity_changed
    {
        return Ok(failure("source_changed"));
    }
    if fault.unsafe_component {
        return Ok(failure("source_unsafe"));
    }
    checked_components(path, observer, flags)?;
    observer.observe(UsageReadStage::BeforeRead, path, flags)?;
    let mut remaining = before.size as usize;
    let mut data = Vec::with_capacity(remaining);
    while remaining != 0 {
        let chunk = handle.read(remaining.min(limits.chunk_bytes))?;
        if chunk.is_empty() {
            return Ok(failure("source_changed"));
        }
        remaining -= chunk.len();
        data.extend(chunk);
    }
    observer.observe(UsageReadStage::BeforeVerify, path, flags)?;
    handle.rewind()?;
    let mut position = 0;
    while position < data.len() {
        let chunk = handle.read(limits.chunk_bytes.min(data.len() - position))?;
        if chunk.is_empty() || chunk != data[position..position + chunk.len()] {
            return Ok(failure("source_changed"));
        }
        position += chunk.len();
    }
    observer.observe(UsageReadStage::BeforeFinalStat, path, flags)?;
    let after = filesystem::usage_path_state(path)?;
    checked_components(path, observer, flags)?;
    if after.identity != opened.identity || after.size < data.len() as u64 {
        return Ok(failure("source_changed"));
    }
    let fault = observer.observe(UsageReadStage::FinalIdentity, path, flags)?;
    if filesystem::file_state(&handle.file)?.identity != opened.identity
        || fault.final_identity_changed
    {
        return Ok(failure("source_changed"));
    }
    Ok(UsageBytes {
        data,
        diagnostics: Vec::new(),
    })
}

fn failure(code: &str) -> UsageBytes {
    UsageBytes {
        data: Vec::new(),
        diagnostics: vec![code.into()],
    }
}

fn absolute_source(path: &Path) -> io::Result<std::path::PathBuf> {
    let path = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };
    #[cfg(windows)]
    {
        Ok(filesystem::python_path(&std::path::absolute(path)?))
    }
    #[cfg(unix)]
    {
        use std::{
            os::unix::ffi::OsStrExt,
            path::{Component, PathBuf},
        };
        let absolute = std::path::absolute(path)?;
        let double_root = absolute.as_os_str().as_bytes().starts_with(b"//")
            && !absolute.as_os_str().as_bytes().starts_with(b"///");
        let mut normalized = PathBuf::new();
        for component in absolute.components() {
            match component {
                Component::CurDir => (),
                Component::ParentDir => {
                    normalized.pop();
                }
                component => normalized.push(component.as_os_str()),
            }
        }
        Ok(if double_root {
            Path::new("//").join(normalized.strip_prefix("/").unwrap_or(&normalized))
        } else {
            normalized
        })
    }
}

pub fn read_prefix_observed(
    path: &Path,
    limits: Limits,
    observer: &dyn UsageReadObserver,
) -> EngineResult<UsageBytes> {
    #[cfg(windows)]
    let nonblocking = 0;
    #[cfg(unix)]
    let nonblocking = libc::O_NONBLOCK;
    read_prefix_with_open(path, limits, observer, nonblocking, |path, _| {
        snapshot_file::open(path)
    })
}

fn read_prefix_with_open(
    path: &Path,
    limits: Limits,
    observer: &dyn UsageReadObserver,
    nonblocking: i32,
    open: impl FnOnce(&Path, i32) -> io::Result<UsageFile>,
) -> EngineResult<UsageBytes> {
    let result = (|| {
        let path = absolute_source(path)?;
        checked_components(&path, observer, 0)?;
        let before = filesystem::usage_path_state(&path)?;
        if !before.regular {
            return Ok(failure("source_unsafe"));
        }
        if before.size > limits.max_bytes as u64 {
            return Ok(failure("source_limit"));
        }
        let flags = requested_open_flags(nonblocking);
        let fault = observer.observe(UsageReadStage::BeforeOpen, &path, flags)?;
        if fault.unsafe_component {
            return Ok(failure("source_unsafe"));
        }
        let file = open(&path, flags)?;
        let mut handle = BufferedUsageFile::new(file)?;
        let result = sample(&path, &before, &mut handle, limits, observer, flags);
        let close_observation = observer.observe(UsageReadStage::BeforeClose, &path, flags);
        handle.close()?;
        close_observation?;
        result
    })();
    match result {
        Err(EngineError::State(_)) => Ok(failure("source_unsafe")),
        Err(EngineError::Io(_)) => Ok(failure("source_unavailable")),
        Err(EngineError::Python {
            kind: "ValueError" | "OverflowError",
            ..
        }) => Ok(failure("source_unavailable")),
        result => result,
    }
}

#[cfg(test)]
#[path = "usage_files_tests.rs"]
mod tests;
