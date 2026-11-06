use super::{repr, unicode, PyString, Value};
use std::cmp::Ordering;

#[derive(Debug)]
pub struct DateTimeError {
    pub kind: &'static str,
    pub message: PyString,
}

fn error(kind: &'static str, message: impl Into<PyString>) -> DateTimeError {
    DateTimeError {
        kind,
        message: message.into(),
    }
}

#[derive(Clone, Debug)]
pub struct DateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub microsecond: u32,
    pub offset_microseconds: Option<i64>,
}

impl DateTime {
    pub fn epoch_microseconds(&self) -> i64 {
        (ordinal(self.year, self.month, self.day) - 719162) * 86400000000
            + i64::from(self.hour * 3600 + self.minute * 60 + self.second) * 1000000
            + i64::from(self.microsecond)
            - self.offset_microseconds.unwrap_or(0)
    }

    pub fn compare(&self, other: &Self) -> Result<Ordering, DateTimeError> {
        if self.offset_microseconds.is_some() != other.offset_microseconds.is_some() {
            return Err(error(
                "TypeError",
                "can't compare offset-naive and offset-aware datetimes",
            ));
        }
        Ok(self.epoch_microseconds().cmp(&other.epoch_microseconds()))
    }

    pub fn isoformat(&self) -> String {
        let mut result = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        );
        if self.microsecond != 0 {
            result.push_str(&format!(".{:06}", self.microsecond));
        }
        if let Some(offset) = self.offset_microseconds {
            let magnitude = offset.abs();
            let seconds = magnitude / 1000000;
            result.push_str(&format!(
                "{}{:02}:{:02}",
                if offset < 0 { '-' } else { '+' },
                seconds / 3600,
                seconds / 60 % 60
            ));
            if seconds % 60 != 0 || magnitude % 1000000 != 0 {
                result.push_str(&format!(":{:02}", seconds % 60));
                if magnitude % 1000000 != 0 {
                    result.push_str(&format!(".{:06}", magnitude % 1000000));
                }
            }
        }
        result
    }
}

fn leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
fn month_days(year: i32, month: u32) -> u32 {
    match month {
        2 => {
            if leap(year) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
fn ordinal(year: i32, month: u32, day: u32) -> i64 {
    let previous = i64::from(year - 1);
    let months = (1..month)
        .map(|m| i64::from(month_days(year, m)))
        .sum::<i64>();
    previous * 365 + previous / 4 - previous / 100 + previous / 400 + months + i64::from(day) - 1
}
fn calendar_date(mut days: i64) -> Option<(i32, u32, u32)> {
    if !(0..=3652058).contains(&days) {
        return None;
    }
    let mut year = (days / 366 + 1) as i32;
    days -= ordinal(year, 1, 1);
    while days >= if leap(year) { 366 } else { 365 } {
        days -= if leap(year) { 366 } else { 365 };
        year += 1;
    }
    let mut month = 1;
    while days >= i64::from(month_days(year, month)) {
        days -= i64::from(month_days(year, month));
        month += 1;
    }
    Some((year, month, days as u32 + 1))
}
fn digits(points: &[u32], start: usize, count: usize, unicode_digits: bool) -> Option<u32> {
    points
        .get(start..start + count)?
        .iter()
        .try_fold(0, |value, &point| {
            let digit = if unicode_digits {
                unicode::decimal(point)
            } else if (48..=57).contains(&point) {
                Some(point - 48)
            } else {
                None
            }?;
            Some(value * 10 + digit)
        })
}
fn date(points: &[u32]) -> Option<(i32, u32, u32, usize)> {
    let year = digits(points, 0, 4, false)? as i32;
    let separated = points.get(4) == Some(&45);
    let cursor = if separated { 5 } else { 4 };
    if points.get(cursor) == Some(&87) {
        let week = digits(points, cursor + 1, 2, false)?;
        let mut end = cursor + 3;
        let mut weekday = 1;
        if separated && points.get(end) == Some(&45) {
            weekday = digits(points, end + 1, 1, false)?;
            end += 2;
        } else if !separated
            && points
                .get(end)
                .is_some_and(|point| (48..=57).contains(point))
        {
            weekday = digits(points, end, 1, false)?;
            end += 1;
        }
        if !(1..=9999).contains(&year) || !(1..=7).contains(&weekday) || !(1..=53).contains(&week) {
            return None;
        }
        let january_fourth = ordinal(year, 1, 4);
        let monday = january_fourth - january_fourth % 7;
        if week == 53 {
            let january_first_weekday = ordinal(year, 1, 1) % 7;
            if january_first_weekday != 3 && !(january_first_weekday == 2 && leap(year)) {
                return None;
            }
        }
        let (year, month, day) = calendar_date(monday + i64::from((week - 1) * 7 + weekday - 1))?;
        Some((year, month, day, end))
    } else {
        let month = digits(points, cursor, 2, false)?;
        let day_start = cursor + if separated { 3 } else { 2 };
        if separated && points.get(cursor + 2) != Some(&45) {
            return None;
        }
        Some((
            year,
            month,
            digits(points, day_start, 2, false)?,
            day_start + 2,
        ))
    }
}

fn clock(points: &[u32], allow_empty_fraction: bool) -> Option<(u32, u32, u32, u32)> {
    let hour = digits(points, 0, 2, false)?;
    let (mut minute, mut second, mut fraction) = (0, 0, 0);
    let mut cursor = 2;
    let colon = points.get(cursor) == Some(&58);
    for part in [&mut minute, &mut second] {
        if cursor == points.len() || matches!(points[cursor], 46 | 44) {
            break;
        }
        if colon {
            if points.get(cursor) != Some(&58) {
                return None;
            }
            cursor += 1;
        }
        *part = digits(points, cursor, 2, false)?;
        cursor += 2;
    }
    if cursor < points.len() {
        if !matches!(points[cursor], 46 | 44) {
            return None;
        }
        cursor += 1;
        if cursor == points.len() && !allow_empty_fraction {
            return None;
        }
        let count = points.len() - cursor;
        for (index, &point) in points[cursor..].iter().enumerate() {
            if !(48..=57).contains(&point) {
                return None;
            }
            if index < 6 {
                fraction = fraction * 10 + point - 48;
            }
        }
        for _ in count..6 {
            fraction *= 10;
        }
    }
    Some((hour, minute, second, fraction))
}

fn invalid_iso(text: &PyString) -> DateTimeError {
    let mut message = PyString::from("Invalid isoformat string: ");
    message.0.extend(repr::string_repr(text).0);
    error("ValueError", message)
}
fn offset_error(offset: i64) -> DateTimeError {
    let days = offset.div_euclid(86400000000);
    let remainder = offset.rem_euclid(86400000000);
    let mut fields = Vec::new();
    if days != 0 {
        fields.push(format!("days={days}"));
    }
    if remainder / 1000000 != 0 {
        fields.push(format!("seconds={}", remainder / 1000000));
    }
    if remainder % 1000000 != 0 {
        fields.push(format!("microseconds={}", remainder % 1000000));
    }
    if fields.is_empty() {
        fields.push("0".into());
    }
    error("ValueError", format!("offset must be a timedelta strictly between -timedelta(hours=24) and timedelta(hours=24), not datetime.timedelta({}).", fields.join(", ")))
}

pub fn fromisoformat(value: &Value) -> Result<DateTime, DateTimeError> {
    let Value::String(text) = value else {
        return Err(error("TypeError", "fromisoformat: argument must be str"));
    };
    let (year, month, day, end) = date(&text.0).ok_or_else(|| invalid_iso(text))?;
    let (mut hour, mut minute, mut second, mut microsecond) = (0, 0, 0, 0);
    let mut offset_microseconds = None;
    if end < text.len() {
        let time = text.0.get(end + 1..).ok_or_else(|| invalid_iso(text))?;
        let zone = time.iter().position(|point| matches!(point, 43 | 45 | 90));
        let clock_end = zone.unwrap_or(time.len());
        (hour, minute, second, microsecond) =
            clock(&time[..clock_end], zone.is_some()).ok_or_else(|| invalid_iso(text))?;
        if let Some(zone) = zone {
            if time[zone] == 90 {
                if zone + 1 != time.len() {
                    return Err(invalid_iso(text));
                }
                offset_microseconds = Some(0);
            } else {
                let (hours, minutes, seconds, micros) =
                    clock(&time[zone + 1..], false).ok_or_else(|| invalid_iso(text))?;
                let total_seconds = i64::from(hours * 3600 + minutes * 60 + seconds);
                let offset = if total_seconds == 0 {
                    0
                } else {
                    (total_seconds * 1000000 + i64::from(micros))
                        * if time[zone] == 45 { -1 } else { 1 }
                };
                if offset.abs() >= 86400000000 {
                    return Err(offset_error(offset));
                }
                offset_microseconds = Some(offset);
            }
        }
    }
    if !(1..=9999).contains(&year) {
        return Err(error("ValueError", format!("year {year} is out of range")));
    }
    if !(1..=12).contains(&month) {
        return Err(error("ValueError", "month must be in 1..12"));
    }
    if day < 1 || day > month_days(year, month) {
        return Err(error("ValueError", "day is out of range for month"));
    }
    if hour > 23 {
        return Err(error("ValueError", "hour must be in 0..23"));
    }
    if minute > 59 {
        return Err(error("ValueError", "minute must be in 0..59"));
    }
    if second > 59 {
        return Err(error("ValueError", "second must be in 0..59"));
    }
    Ok(DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
        offset_microseconds,
    })
}

pub fn receipt_datetime(value: &Value) -> Result<DateTime, DateTimeError> {
    let Value::String(text) = value else {
        return Err(error(
            "AttributeError",
            format!(
                "'{}' object has no attribute 'replace'",
                value.python_type()
            ),
        ));
    };
    let mut normalized = Vec::new();
    for &point in &text.0 {
        if point == 90 {
            normalized.extend("+00:00".chars().map(u32::from));
        } else {
            normalized.push(point);
        }
    }
    fromisoformat(&Value::String(PyString(normalized)))
}

pub fn strict_utc_microseconds(value: &Value) -> Option<i64> {
    let Value::String(text) = value else {
        return None;
    };
    let points = &text.0;
    if points.len() < 20 || points.last() != Some(&90) {
        return None;
    }
    for (index, point) in [(4, 45), (7, 45), (10, 84), (13, 58), (16, 58)] {
        if points.get(index) != Some(&point) {
            return None;
        }
    }
    let year = digits(points, 0, 4, true)? as i32;
    let month = digits(points, 5, 2, false)?;
    let day = digits(points, 8, 2, false)?;
    let hour = digits(points, 11, 2, false)?;
    let minute = digits(points, 14, 2, false)?;
    let second = digits(points, 17, 2, false)?;
    let mut microsecond = 0;
    if points.len() != 20 {
        let count = points.len().checked_sub(21)?;
        if points.get(19) != Some(&46) || !(1..=6).contains(&count) {
            return None;
        }
        microsecond = digits(points, 20, count, true)? * 10u32.pow((6 - count) as u32);
    }
    if !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day == 0
        || day > month_days(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some(
        DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            microsecond,
            offset_microseconds: Some(0),
        }
        .epoch_microseconds(),
    )
}
