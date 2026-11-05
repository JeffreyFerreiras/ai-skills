use crate::compat::PyString;
use std::{
    ffi::OsStr,
    io::{self, Write},
};

pub fn native_string(
    value: &PyString,
) -> crate::application::error::EngineResult<std::ffi::OsString> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let mut units = Vec::new();
        for &point in &value.0 {
            if point <= 0xffff {
                units.push(point as u16);
            } else {
                let scalar = point - 0x10000;
                units.push((0xd800 + (scalar >> 10)) as u16);
                units.push((0xdc00 + (scalar & 0x3ff)) as u16);
            }
        }
        Ok(std::ffi::OsString::from_wide(&units))
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let mut bytes = Vec::new();
        for &point in &value.0 {
            if (0xdc80..=0xdcff).contains(&point) {
                bytes.push((point - 0xdc00) as u8);
            } else if let Some(character) = char::from_u32(point) {
                let mut buffer = [0u8; 4];
                bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            } else {
                return Err(crate::compat::encode_utf8(value).unwrap_err().into());
            }
        }
        Ok(std::ffi::OsString::from_vec(bytes))
    }
}

#[cfg(windows)]
pub fn python_string(value: &OsStr) -> PyString {
    use std::os::windows::ffi::OsStrExt;
    PyString(
        char::decode_utf16(value.encode_wide())
            .map(|point| {
                point.map_or_else(|error| u32::from(error.unpaired_surrogate()), u32::from)
            })
            .collect(),
    )
}
#[cfg(unix)]
pub fn python_string(value: &OsStr) -> PyString {
    use std::os::unix::ffi::OsStrExt;
    let mut bytes = value.as_bytes();
    let mut result = Vec::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                result.extend(text.chars().map(u32::from));
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                result.extend(
                    std::str::from_utf8(&bytes[..valid])
                        .unwrap()
                        .chars()
                        .map(u32::from),
                );
                bytes = &bytes[valid..];
                let invalid = error.error_len().unwrap_or(bytes.len());
                result.extend(
                    bytes[..invalid]
                        .iter()
                        .map(|byte| 0xdc00 + u32::from(*byte)),
                );
                bytes = &bytes[invalid..];
            }
        }
    }
    PyString(result)
}
pub fn arguments() -> Vec<PyString> {
    std::env::args_os()
        .skip(1)
        .map(|value| python_string(&value))
        .collect()
}
pub fn terminal_columns() -> usize {
    std::env::var_os("COLUMNS")
        .map(|value| columns_value(&python_string(&value)))
        .unwrap_or_else(fallback_columns)
}
pub fn columns_value(value: &PyString) -> usize {
    use num_traits::ToPrimitive;
    if let Ok(width) = crate::compat::integer::coerce(&crate::compat::Value::String(value.clone()))
    {
        if width > 0_i64.into() {
            // Wider Python integers cannot wrap any representable help text.
            return width.to_usize().unwrap_or(usize::MAX);
        }
    }
    fallback_columns()
}
fn fallback_columns() -> usize {
    console_columns().filter(|width| *width > 0).unwrap_or(80)
}
#[cfg(windows)]
fn console_columns() -> Option<usize> {
    use windows_sys::Win32::System::Console::{
        GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO, STD_OUTPUT_HANDLE,
    };
    let mut information = unsafe { std::mem::zeroed::<CONSOLE_SCREEN_BUFFER_INFO>() };
    if unsafe { GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut information) } == 0
    {
        return None;
    }
    usize::try_from(
        i32::from(information.srWindow.Right) - i32::from(information.srWindow.Left) + 1,
    )
    .ok()
}
#[cfg(unix)]
fn console_columns() -> Option<usize> {
    let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
    if unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut size) } == 0 {
        Some(usize::from(size.ws_col))
    } else {
        None
    }
}
pub fn write_text(
    output: &mut dyn Write,
    text: &PyString,
    stream: super::text_stream::Stream,
) -> io::Result<()> {
    output.write_all(&super::text_stream::encode(text, stream)?)
}
pub fn write_bytes(output: &mut dyn Write, bytes: &[u8]) -> io::Result<()> {
    write_text(
        output,
        &PyString(bytes.iter().copied().map(u32::from).collect()),
        super::text_stream::Stream::Stdout,
    )
}
