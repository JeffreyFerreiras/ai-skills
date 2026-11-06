use crate::compat::{repr::string_repr, PyString};
use std::{fmt, io, path::Path};

#[derive(Clone, Copy, Debug)]
pub enum OsApi {
    Native,
}
#[derive(Debug)]
pub struct OsFailure {
    pub raw: io::Error,
    pub errno: Option<i32>,
    pub winerror: Option<i32>,
    pub filename: Option<PyString>,
    pub filename2: Option<PyString>,
    message: String,
}
pub(crate) fn errno_message(errno: i32, kind: io::ErrorKind, description: &str) -> io::Error {
    io::Error::new(
        kind,
        OsFailure {
            raw: io::Error::from_raw_os_error(errno),
            errno: Some(errno),
            winerror: None,
            filename: None,
            filename2: None,
            message: format!("[Errno {errno}] {description}"),
        },
    )
}
impl fmt::Display for OsFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for OsFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.raw)
    }
}

pub fn with_path(
    error: io::Error,
    api: OsApi,
    filename: Option<&Path>,
    filename2: Option<&Path>,
) -> io::Error {
    if error.get_ref().is_some_and(|inner| inner.is::<OsFailure>()) {
        return error;
    }
    let Some(code) = error.raw_os_error() else {
        return error;
    };
    let kind = error.kind();
    let filename = filename.map(|path| super::filesystem::os_string(path.as_os_str()));
    let filename2 = filename2.map(|path| super::filesystem::os_string(path.as_os_str()));
    #[cfg(windows)]
    let (errno, winerror, description) = {
        let _ = api;
        let message = error.to_string();
        let suffix = format!(" (os error {code})");
        let message = message
            .strip_suffix(&suffix)
            .unwrap_or(&message)
            .trim_end_matches(['.', ' ', '\r', '\n'])
            .to_owned();
        (None, Some(code), message)
    };
    #[cfg(unix)]
    let (errno, winerror, description) = {
        let _ = api;
        let message = error.to_string();
        let suffix = format!(" (os error {code})");
        (
            Some(code),
            None,
            message.strip_suffix(&suffix).unwrap_or(&message).to_owned(),
        )
    };
    let mut message = if let Some(winerror) = winerror {
        format!("[WinError {winerror}] {description}")
    } else {
        format!("[Errno {}] {description}", errno.unwrap())
    };
    if let Some(filename) = &filename {
        message.push_str(": ");
        message.push_str(&string_repr(filename).utf8().unwrap());
    }
    if let Some(filename) = &filename2 {
        message.push_str(" -> ");
        message.push_str(&string_repr(filename).utf8().unwrap());
    }
    io::Error::new(
        kind,
        OsFailure {
            raw: error,
            errno,
            winerror,
            filename,
            filename2,
            message,
        },
    )
}
#[cfg(windows)]
pub fn from_crt_errno(errno: i32, filename: Option<&Path>) -> io::Error {
    unsafe extern "C" {
        fn strerror(code: i32) -> *const std::ffi::c_char;
    }
    let description = unsafe { std::ffi::CStr::from_ptr(strerror(errno)) }.to_string_lossy();
    let kind = match errno {
        2 => io::ErrorKind::NotFound,
        1 | 13 => io::ErrorKind::PermissionDenied,
        17 => io::ErrorKind::AlreadyExists,
        20 => io::ErrorKind::NotADirectory,
        21 => io::ErrorKind::IsADirectory,
        4 => io::ErrorKind::Interrupted,
        32 => io::ErrorKind::BrokenPipe,
        _ => io::ErrorKind::Other,
    };
    let filename = filename.map(|path| super::filesystem::os_string(path.as_os_str()));
    let mut message = format!("[Errno {errno}] {description}");
    if let Some(filename) = &filename {
        message.push_str(": ");
        message.push_str(&string_repr(filename).utf8().unwrap());
    }
    io::Error::new(
        kind,
        OsFailure {
            raw: io::Error::from_raw_os_error(errno),
            errno: Some(errno),
            winerror: None,
            filename,
            filename2: None,
            message,
        },
    )
}
