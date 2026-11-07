use crate::compat::PyString;
use std::io::{self, IsTerminal};

#[derive(Clone, Copy)]
pub enum Stream {
    Stdout,
    Stderr,
}
#[derive(Clone, Copy)]
enum Encoding {
    Utf8(bool),
    Ascii,
    Latin1,
    Windows1252,
    Utf16 { big: bool, bom: bool },
    Utf32 { big: bool, bom: bool },
    Windows(u32),
}

fn encoding(stream: Stream) -> io::Result<Encoding> {
    #[cfg(windows)]
    {
        let console = match stream {
            Stream::Stdout => std::io::stdout().is_terminal(),
            Stream::Stderr => std::io::stderr().is_terminal(),
        };
        if console && std::env::var_os("PYTHONLEGACYWINDOWSSTDIO").is_none() {
            return Ok(Encoding::Utf8(false));
        }
    }
    #[cfg(unix)]
    let _ = stream;
    if let Ok(value) = std::env::var("PYTHONIOENCODING") {
        let label = value.split(':').next().unwrap();
        if !label.is_empty() {
            let normalized = label
                .to_ascii_lowercase()
                .replace('-', "_")
                .replace(' ', "_");
            return match normalized.as_str() {
                "utf8" | "utf_8" | "u8" | "cp65001" => Ok(Encoding::Utf8(false)),
                "utf_8_sig" => Ok(Encoding::Utf8(true)),
                "ascii" | "us_ascii" | "646" => Ok(Encoding::Ascii),
                "latin1" | "latin_1" | "iso8859_1" | "iso_8859_1" | "l1" => Ok(Encoding::Latin1),
                "cp1252" | "windows_1252" => Ok(Encoding::Windows1252),
                "utf16" | "utf_16" => Ok(Encoding::Utf16 {
                    big: cfg!(target_endian = "big"),
                    bom: true,
                }),
                "utf_16_le" | "utf16le" => Ok(Encoding::Utf16 {
                    big: false,
                    bom: false,
                }),
                "utf_16_be" | "utf16be" => Ok(Encoding::Utf16 {
                    big: true,
                    bom: false,
                }),
                "utf32" | "utf_32" => Ok(Encoding::Utf32 {
                    big: cfg!(target_endian = "big"),
                    bom: true,
                }),
                "utf_32_le" | "utf32le" => Ok(Encoding::Utf32 {
                    big: false,
                    bom: false,
                }),
                "utf_32_be" | "utf32be" => Ok(Encoding::Utf32 {
                    big: true,
                    bom: false,
                }),
                _ => Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("unknown encoding: {label}"),
                )),
            };
        }
    }
    if std::env::var("PYTHONUTF8").is_ok_and(|value| value == "1") {
        return Ok(Encoding::Utf8(false));
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetACP() -> u32;
        }
        Ok(match unsafe { GetACP() } {
            1252 => Encoding::Windows1252,
            65001 => Encoding::Utf8(false),
            code => Encoding::Windows(code),
        })
    }
    #[cfg(unix)]
    {
        Ok(Encoding::Utf8(false))
    }
}

fn windows_1252(point: u32) -> Option<u8> {
    if point < 0x80 || (0xa0..=0xff).contains(&point) {
        return Some(point as u8);
    }
    const EXTENDED: &[(u32, u8)] = &[
        (0x20ac, 0x80),
        (0x201a, 0x82),
        (0x0192, 0x83),
        (0x201e, 0x84),
        (0x2026, 0x85),
        (0x2020, 0x86),
        (0x2021, 0x87),
        (0x02c6, 0x88),
        (0x2030, 0x89),
        (0x0160, 0x8a),
        (0x2039, 0x8b),
        (0x0152, 0x8c),
        (0x017d, 0x8e),
        (0x2018, 0x91),
        (0x2019, 0x92),
        (0x201c, 0x93),
        (0x201d, 0x94),
        (0x2022, 0x95),
        (0x2013, 0x96),
        (0x2014, 0x97),
        (0x02dc, 0x98),
        (0x2122, 0x99),
        (0x0161, 0x9a),
        (0x203a, 0x9b),
        (0x0153, 0x9c),
        (0x017e, 0x9e),
        (0x0178, 0x9f),
    ];
    EXTENDED
        .iter()
        .find_map(|(character, byte)| (*character == point).then_some(*byte))
}

fn character_bytes(point: u32, encoding: Encoding) -> io::Result<Option<Vec<u8>>> {
    let Some(character) = char::from_u32(point) else {
        return Ok(None);
    };
    Ok(match encoding {
        Encoding::Utf8(_) => {
            let mut bytes = [0; 4];
            Some(character.encode_utf8(&mut bytes).as_bytes().to_vec())
        }
        Encoding::Ascii => (point < 0x80).then(|| vec![point as u8]),
        Encoding::Latin1 => (point < 0x100).then(|| vec![point as u8]),
        Encoding::Windows1252 => windows_1252(point).map(|byte| vec![byte]),
        Encoding::Utf16 { big, .. } => {
            let mut units = [0; 2];
            Some(
                character
                    .encode_utf16(&mut units)
                    .iter()
                    .flat_map(|unit| {
                        if big {
                            unit.to_be_bytes()
                        } else {
                            unit.to_le_bytes()
                        }
                    })
                    .collect(),
            )
        }
        Encoding::Utf32 { big, .. } => Some(
            if big {
                point.to_be_bytes()
            } else {
                point.to_le_bytes()
            }
            .to_vec(),
        ),
        Encoding::Windows(code) => windows_character(character, code)?,
    })
}

#[cfg(windows)]
fn windows_character(character: char, code: u32) -> io::Result<Option<Vec<u8>>> {
    #[link(name = "kernel32")]
    extern "system" {
        fn WideCharToMultiByte(
            code: u32,
            flags: u32,
            source: *const u16,
            source_length: i32,
            target: *mut u8,
            target_length: i32,
            default_character: *const u8,
            used_default: *mut i32,
        ) -> i32;
    }
    let mut units = [0; 2];
    let units = character.encode_utf16(&mut units);
    let mut bytes = [0; 8];
    let mut replaced = 0;
    let length = unsafe {
        WideCharToMultiByte(
            code,
            0x400,
            units.as_ptr(),
            units.len() as i32,
            bytes.as_mut_ptr(),
            bytes.len() as i32,
            std::ptr::null(),
            &mut replaced,
        )
    };
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((replaced == 0).then(|| bytes[..length as usize].to_vec()))
}
#[cfg(unix)]
fn windows_character(_character: char, _code: u32) -> io::Result<Option<Vec<u8>>> {
    unreachable!()
}

pub fn encode(text: &PyString, stream: Stream) -> io::Result<Vec<u8>> {
    let encoding = encoding(stream)?;
    let mut result = match encoding {
        Encoding::Utf8(true) => vec![0xef, 0xbb, 0xbf],
        Encoding::Utf16 { big, bom: true } => {
            if big {
                vec![0xfe, 0xff]
            } else {
                vec![0xff, 0xfe]
            }
        }
        Encoding::Utf32 { big, bom: true } => {
            if big {
                vec![0, 0, 0xfe, 0xff]
            } else {
                vec![0xff, 0xfe, 0, 0]
            }
        }
        _ => Vec::new(),
    };
    for point in &text.0 {
        #[cfg(windows)]
        if *point == 10 {
            result.extend(character_bytes(13, encoding)?.unwrap());
        }
        if let Some(bytes) = character_bytes(*point, encoding)? {
            result.extend(bytes);
        } else {
            let escaped = if *point < 0x100 {
                format!("\\x{point:02x}")
            } else if *point < 0x10000 {
                format!("\\u{point:04x}")
            } else {
                format!("\\U{point:08x}")
            };
            for byte in escaped.bytes() {
                result.extend(character_bytes(u32::from(byte), encoding)?.unwrap());
            }
        }
    }
    Ok(result)
}
