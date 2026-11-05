use super::{
    json::{self, JsonError},
    PyString, Value,
};

fn decode_error(encoding: &str, data: &[u8], start: usize, end: usize, reason: &str) -> JsonError {
    let detail = if end == start + 1 {
        format!("byte 0x{:02x} in position {start}", data[start])
    } else {
        format!("bytes in position {start}-{}", end - 1)
    };
    JsonError {
        kind: "UnicodeDecodeError",
        message: format!("'{encoding}' codec can't decode {detail}: {reason}"),
        position: start,
    }
}
fn utf8(data: &[u8]) -> Result<PyString, JsonError> {
    let mut points = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let first = data[offset];
        let (length, mut point) = match first {
            0..=0x7f => (1, u32::from(first)),
            0xc2..=0xdf => (2, u32::from(first & 31)),
            0xe0..=0xef => (3, u32::from(first & 15)),
            0xf0..=0xf4 => (4, u32::from(first & 7)),
            _ => {
                return Err(decode_error(
                    "utf-8",
                    data,
                    offset,
                    offset + 1,
                    "invalid start byte",
                ))
            }
        };
        for index in 1..length {
            if offset + index == data.len() {
                return Err(decode_error(
                    "utf-8",
                    data,
                    offset,
                    data.len(),
                    "unexpected end of data",
                ));
            }
            let byte = data[offset + index];
            let valid = (0x80..=0xbf).contains(&byte)
                && !(index == 1
                    && ((first == 0xe0 && byte < 0xa0)
                        || (first == 0xf0 && byte < 0x90)
                        || (first == 0xf4 && byte > 0x8f)));
            if !valid {
                return Err(decode_error(
                    "utf-8",
                    data,
                    offset,
                    offset + index,
                    "invalid continuation byte",
                ));
            }
            point = (point << 6) | u32::from(byte & 63);
        }
        points.push(point);
        offset += length;
    }
    Ok(PyString(points))
}
fn utf16(data: &[u8], little: bool, skip: usize) -> Result<PyString, JsonError> {
    let encoding = if little { "utf-16-le" } else { "utf-16-be" };
    let unit = |index| {
        if little {
            u16::from_le_bytes([data[index], data[index + 1]])
        } else {
            u16::from_be_bytes([data[index], data[index + 1]])
        }
    };
    let mut points = Vec::new();
    let mut offset = skip;
    while offset < data.len() {
        if offset + 1 == data.len() {
            return Err(decode_error(
                encoding,
                data,
                offset,
                data.len(),
                "truncated data",
            ));
        }
        let first = unit(offset);
        offset += 2;
        if (0xd800..=0xdbff).contains(&first) && offset + 1 < data.len() {
            let second = unit(offset);
            if (0xdc00..=0xdfff).contains(&second) {
                points.push(
                    0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00),
                );
                offset += 2;
                continue;
            }
        }
        points.push(u32::from(first));
    }
    Ok(PyString(points))
}
fn utf32(data: &[u8], little: bool, skip: usize) -> Result<PyString, JsonError> {
    let encoding = if little { "utf-32-le" } else { "utf-32-be" };
    let mut points = Vec::new();
    let mut offset = skip;
    while offset < data.len() {
        if data.len() - offset < 4 {
            return Err(decode_error(
                encoding,
                data,
                offset,
                data.len(),
                "truncated data",
            ));
        }
        let bytes = data[offset..offset + 4].try_into().unwrap();
        let point = if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        };
        if point > 0x10ffff {
            return Err(decode_error(
                encoding,
                data,
                offset,
                offset + 4,
                "code point not in range(0x110000)",
            ));
        }
        points.push(point);
        offset += 4;
    }
    Ok(PyString(points))
}
fn decode(data: &[u8]) -> Result<PyString, JsonError> {
    if data.starts_with(&[0xff, 0xfe, 0, 0]) {
        return utf32(data, true, 4);
    }
    if data.starts_with(&[0, 0, 0xfe, 0xff]) {
        return utf32(data, false, 4);
    }
    if data.starts_with(&[0xff, 0xfe]) {
        return utf16(data, true, 2);
    }
    if data.starts_with(&[0xfe, 0xff]) {
        return utf16(data, false, 2);
    }
    if data.starts_with(&[0xef, 0xbb, 0xbf]) {
        return utf8(&data[3..]);
    }
    if data.len() >= 4 {
        if data[0] == 0 {
            return if data[1] == 0 {
                utf32(data, false, 0)
            } else {
                utf16(data, false, 0)
            };
        }
        if data[1] == 0 {
            return if data[2] != 0 || data[3] != 0 {
                utf16(data, true, 0)
            } else {
                utf32(data, true, 0)
            };
        }
    } else if data.len() == 2 {
        if data[0] == 0 {
            return utf16(data, false, 0);
        }
        if data[1] == 0 {
            return utf16(data, true, 0);
        }
    }
    utf8(data)
}
fn decoded(text: &PyString, original_string: bool) -> Result<Value, JsonError> {
    let result = if text.0.first() == Some(&0xfeff) {
        Err(JsonError {
            kind: "JSONDecodeError",
            message: if original_string {
                "Unexpected UTF-8 BOM (decode using utf-8-sig)"
            } else {
                "Expecting value"
            }
            .into(),
            position: 0,
        })
    } else {
        json::loads_text(text)
    };
    result.map_err(|mut error| {
        if error.kind == "JSONDecodeError" {
            let prefix = &text.0[..error.position.min(text.len())];
            let line = prefix.iter().filter(|&&point| point == 10).count() + 1;
            let column = prefix.len()
                - prefix
                    .iter()
                    .rposition(|&point| point == 10)
                    .map_or(0, |index| index + 1)
                + 1;
            error.message = format!(
                "{}: line {line} column {column} (char {})",
                error.message, error.position
            );
        }
        error
    })
}
pub fn loads_value(value: &Value) -> Result<Value, JsonError> {
    match value {
        Value::String(text) => decoded(text, true),
        Value::Bytes(bytes) => decoded(&decode(bytes)?, false),
        _ => Err(JsonError {
            kind: "TypeError",
            message: format!(
                "the JSON object must be str, bytes or bytearray, not {}",
                value.python_type()
            ),
            position: 0,
        }),
    }
}
