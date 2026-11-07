use super::errors::{attribute_error, ValidationFailure, ValidationResult};
use crate::compat::{PyString, Value};

pub enum LeaseExpiry {
    Expired,
    Naive,
    Instant(i128),
}

fn byte(text: &[u8], index: usize) -> u8 {
    text.get(index).copied().unwrap_or(0)
}
fn digit(value: u8) -> bool {
    value.is_ascii_digit()
}

fn digits(text: &[u8], position: &mut usize, count: usize) -> Option<u32> {
    let mut value = 0;
    for _ in 0..count {
        let next = byte(text, *position);
        if !digit(next) {
            return None;
        }
        value = value * 10 + u32::from(next - b'0');
        *position += 1;
    }
    Some(value)
}

fn separator(text: &[u8]) -> Option<usize> {
    let length = text.len();
    if length == 7 {
        return Some(7);
    }
    if byte(text, 4) == b'-' {
        if byte(text, 5) == b'W' {
            if length < 8 {
                return None;
            }
            if length > 8 && byte(text, 8) == b'-' {
                if length == 9 {
                    return None;
                }
                return Some(if length > 10 && digit(byte(text, 10)) {
                    8
                } else {
                    10
                });
            }
            Some(8)
        } else {
            Some(10)
        }
    } else if byte(text, 4) == b'W' {
        let mut index = 7;
        while index < length && digit(byte(text, index)) {
            index += 1;
        }
        Some(if index < 9 {
            index
        } else if index % 2 == 0 {
            7
        } else {
            8
        })
    } else {
        Some(8)
    }
}

fn date(text: &[u8], length: usize) -> Option<time::Date> {
    let mut position = 0;
    let year = digits(text, &mut position, 4)? as i32;
    let separated = byte(text, position) == b'-';
    if separated {
        position += 1;
    }
    if byte(text, position) == b'W' {
        position += 1;
        let week = digits(text, &mut position, 2)? as u8;
        let day = if position < length {
            if separated {
                if byte(text, position) != b'-' {
                    return None;
                }
                position += 1;
            }
            digits(text, &mut position, 1)?
        } else {
            1
        };
        let weekday = match day {
            1 => time::Weekday::Monday,
            2 => time::Weekday::Tuesday,
            3 => time::Weekday::Wednesday,
            4 => time::Weekday::Thursday,
            5 => time::Weekday::Friday,
            6 => time::Weekday::Saturday,
            7 => time::Weekday::Sunday,
            _ => return None,
        };
        if !(1..=9999).contains(&year) {
            return None;
        }
        return time::Date::from_iso_week_date(year, week, weekday)
            .ok()
            .filter(|date| (1..=9999).contains(&date.year()));
    }
    let month = digits(text, &mut position, 2)? as u8;
    if separated {
        if byte(text, position) != b'-' {
            return None;
        }
        position += 1;
    }
    let day = digits(text, &mut position, 2)? as u8;
    if !(1..=9999).contains(&year) {
        return None;
    }
    time::Date::from_calendar_date(year, month.try_into().ok()?, day).ok()
}

fn hms(text: &[u8], start: usize, end: usize) -> Option<([u32; 4], bool)> {
    let mut values = [0; 4];
    let mut position = start;
    let mut separated = true;
    for index in 0..3 {
        values[index] = digits(text, &mut position, 2)?;
        let next = byte(text, position);
        position += 1;
        if index == 0 {
            separated = next == b':';
        }
        if position >= end {
            return Some((values, next != 0));
        }
        if separated && next == b':' {
            continue;
        }
        if next == b'.' || next == b',' {
            break;
        }
        if !separated {
            position -= 1;
        } else {
            return None;
        }
    }
    let count = (end - position).min(6);
    if count == 0 {
        return None;
    }
    values[3] = digits(text, &mut position, count)? * 10_u32.pow((6 - count) as u32);
    while digit(byte(text, position)) {
        position += 1;
    }
    Some((values, byte(text, position) != 0))
}

fn time_and_offset(text: &[u8], start: usize) -> Option<([u32; 4], Option<i64>)> {
    let end = text.len();
    let offset_start = (start..end)
        .find(|index| matches!(byte(text, *index), b'Z' | b'+' | b'-'))
        .unwrap_or(end);
    let (time, extra) = hms(text, start, offset_start)?;
    if offset_start == end {
        return (!extra).then_some((time, None));
    }
    if byte(text, offset_start) == b'Z' {
        return (byte(text, offset_start + 1) == 0).then_some((time, Some(0)));
    }
    let sign = if byte(text, offset_start) == b'-' {
        -1
    } else {
        1
    };
    let (offset, extra) = hms(text, offset_start + 1, end)?;
    if extra {
        return None;
    }
    let seconds =
        sign * (i64::from(offset[0]) * 3600 + i64::from(offset[1]) * 60 + i64::from(offset[2]));
    // CPython returns its UTC singleton when whole offset seconds are zero, before considering fractional offset.
    let microseconds = if seconds == 0 {
        0
    } else {
        seconds * 1_000_000 + sign * i64::from(offset[3])
    };
    if !(-86_400_000_000_i64..86_400_000_000_i64).contains(&microseconds)
        || microseconds == -86_400_000_000
    {
        return None;
    }
    Some((time, Some(microseconds)))
}

fn encoded(value: &PyString) -> Option<Vec<u8>> {
    let mut sanitized = value.clone();
    if sanitized.len() < 7 {
        return None;
    }
    for index in [7, 8, 10] {
        if index > sanitized.len() {
            break;
        }
        if sanitized
            .0
            .get(index)
            .is_some_and(|point| (0xd800..=0xdfff).contains(point))
        {
            sanitized.0[index] = u32::from(b'T');
            break;
        }
    }
    let mut bytes = Vec::new();
    for point in &sanitized.0 {
        let mut buffer = [0; 4];
        bytes.extend_from_slice(char::from_u32(*point)?.encode_utf8(&mut buffer).as_bytes());
    }
    Some(bytes)
}

fn parsed(value: &PyString) -> Option<LeaseExpiry> {
    let text = encoded(value)?;
    let separator = separator(&text)?;
    let date = date(&text, separator)?;
    let (clock, offset) = if text.len() > separator {
        let first = byte(&text, separator);
        let width = if first & 0x80 == 0 {
            1
        } else if first & 0xf0 == 0xe0 {
            3
        } else if first & 0xf0 == 0xf0 {
            4
        } else {
            2
        };
        time_and_offset(&text, separator + width)?
    } else {
        ([0; 4], None)
    };
    let local = date
        .with_hms_micro(
            clock[0].try_into().ok()?,
            clock[1].try_into().ok()?,
            clock[2].try_into().ok()?,
            clock[3],
        )
        .ok()?
        .assume_utc();
    Some(match offset {
        None => LeaseExpiry::Naive,
        Some(offset) => LeaseExpiry::Instant(
            local.unix_timestamp_nanos().div_euclid(1_000) - i128::from(offset),
        ),
    })
}

pub fn prepare_lease_expiry(value: &Value) -> ValidationResult<LeaseExpiry> {
    if !value.truthy() {
        return Ok(LeaseExpiry::Expired);
    }
    let text = match value {
        Value::String(text) => text,
        Value::Bytes(_) => {
            return Err(ValidationFailure::python(
                "TypeError",
                "a bytes-like object is required, not 'str'",
            ))
        }
        _ => return Err(attribute_error(value, "replace")),
    };
    let mut replaced = PyString::default();
    for point in &text.0 {
        if *point == u32::from(b'Z') {
            replaced.0.extend("+00:00".chars().map(u32::from));
        } else {
            replaced.0.push(*point);
        }
    }
    Ok(parsed(&replaced).unwrap_or(LeaseExpiry::Expired))
}
