use super::{PyString, Value};
use num_bigint::BigInt;
use std::{fmt, str::FromStr};

pub const JSON_CONTAINER_LIMIT: usize = 2997;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonError {
    pub kind: &'static str,
    pub message: String,
    pub position: usize,
}
impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.message, self.position)
    }
}
impl std::error::Error for JsonError {}
fn error(message: &str, position: usize) -> JsonError {
    JsonError {
        kind: "JSONDecodeError",
        message: message.into(),
        position,
    }
}

pub fn loads(bytes: &[u8]) -> Result<Value, JsonError> {
    let text = std::str::from_utf8(bytes).map_err(|e| JsonError {
        kind: "UnicodeDecodeError",
        message: "invalid UTF-8".into(),
        position: e.valid_up_to(),
    })?;
    loads_text(&text.into())
}
pub fn loads_text(text: &PyString) -> Result<Value, JsonError> {
    let chars = text.0.clone();
    if chars.first() == Some(&0xfeff) {
        return Err(error("Unexpected UTF-8 BOM (decode using utf-8-sig)", 0));
    }
    let mut reader = Reader { chars, index: 0 };
    reader.space();
    let value = reader.document()?;
    reader.space();
    if reader.index != reader.chars.len() {
        return Err(error("Extra data", reader.index));
    }
    Ok(value)
}

struct Reader {
    chars: Vec<u32>,
    index: usize,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum FrameState {
    First,
    Value,
    Delimiter,
    KeyAfterComma,
}
enum Frame {
    List {
        items: Vec<Value>,
        state: FrameState,
    },
    Object {
        items: Vec<(PyString, Value)>,
        key: Option<PyString>,
        state: FrameState,
    },
}
impl Frame {
    fn value(self) -> Value {
        match self {
            Self::List { items, .. } => Value::List(items),
            Self::Object { items, .. } => Value::Object(items),
        }
    }
}
impl Reader {
    fn peek(&self) -> Option<u32> {
        self.chars.get(self.index).copied()
    }
    fn space(&mut self) {
        while matches!(self.peek(), Some(9 | 10 | 13 | 32)) {
            self.index += 1;
        }
    }
    fn take(&mut self, point: u32) -> bool {
        if self.peek() == Some(point) {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn literal(&mut self, text: &str) -> bool {
        let points: Vec<_> = text.chars().map(u32::from).collect();
        if self.chars.get(self.index..self.index + points.len()) == Some(points.as_slice()) {
            self.index += points.len();
            true
        } else {
            false
        }
    }
    fn document(&mut self) -> Result<Value, JsonError> {
        let mut frames: Vec<Frame> = Vec::new();
        let mut completed = None;
        loop {
            if let Some(value) = completed.take() {
                match frames.last_mut() {
                    Some(Frame::List { items, state }) => {
                        items.push(value);
                        *state = FrameState::Delimiter;
                    }
                    Some(Frame::Object { items, key, state }) => {
                        let key = key.take().unwrap();
                        if let Some((_, existing)) =
                            items.iter_mut().find(|(candidate, _)| candidate == &key)
                        {
                            *existing = value;
                        } else {
                            items.push((key, value));
                        }
                        *state = FrameState::Delimiter;
                    }
                    None => return Ok(value),
                }
                continue;
            }
            self.space();
            match frames.last_mut() {
                Some(Frame::List { state, .. }) if *state == FrameState::Delimiter => {
                    if self.take(93) {
                        completed = Some(frames.pop().unwrap().value());
                        continue;
                    }
                    if !self.take(44) {
                        return Err(error("Expecting ',' delimiter", self.index));
                    }
                    *state = FrameState::Value;
                    self.space();
                }
                Some(Frame::Object { state, .. }) if *state == FrameState::Delimiter => {
                    if self.take(125) {
                        completed = Some(frames.pop().unwrap().value());
                        continue;
                    }
                    if !self.take(44) {
                        return Err(error("Expecting ',' delimiter", self.index));
                    }
                    *state = FrameState::KeyAfterComma;
                    self.space();
                }
                _ => (),
            }
            match frames.last_mut() {
                Some(Frame::List { state, .. }) if *state == FrameState::First => {
                    if self.take(93) {
                        completed = Some(frames.pop().unwrap().value());
                        continue;
                    }
                    *state = FrameState::Value;
                }
                Some(Frame::Object { key, state, .. })
                    if *state == FrameState::First || *state == FrameState::KeyAfterComma =>
                {
                    if *state == FrameState::First && self.take(125) {
                        completed = Some(frames.pop().unwrap().value());
                        continue;
                    }
                    if self.peek() != Some(34) {
                        return Err(error(
                            "Expecting property name enclosed in double quotes",
                            self.index,
                        ));
                    }
                    *key = Some(self.string()?);
                    self.space();
                    if !self.take(58) {
                        return Err(error("Expecting ':' delimiter", self.index));
                    }
                    self.space();
                    *state = FrameState::Value;
                }
                _ => (),
            }
            if frames.len() >= JSON_CONTAINER_LIMIT && matches!(self.peek(), Some(91 | 123)) {
                return Err(JsonError{kind:"RecursionError",message:format!("maximum recursion depth exceeded while decoding a JSON {} from a unicode string",if self.peek()==Some(123){"object"}else{"array"}),position:0});
            }
            completed = match self.peek() {
                Some(91) => {
                    self.index += 1;
                    frames.push(Frame::List {
                        items: Vec::new(),
                        state: FrameState::First,
                    });
                    None
                }
                Some(123) => {
                    self.index += 1;
                    frames.push(Frame::Object {
                        items: Vec::new(),
                        key: None,
                        state: FrameState::First,
                    });
                    None
                }
                Some(34) => Some(Value::String(self.string()?)),
                Some(110) if self.literal("null") => Some(Value::Null),
                Some(116) if self.literal("true") => Some(Value::Bool(true)),
                Some(102) if self.literal("false") => Some(Value::Bool(false)),
                Some(78) if self.literal("NaN") => Some(Value::ConstantNaN),
                Some(73) if self.literal("Infinity") => Some(Value::Float(f64::INFINITY)),
                Some(45) if self.literal("-Infinity") => Some(Value::Float(f64::NEG_INFINITY)),
                Some(45 | 48..=57) => Some(self.number()?),
                _ => return Err(error("Expecting value", self.index)),
            };
        }
    }
    fn hex4(&mut self) -> Result<u32, JsonError> {
        let start = self.index;
        let mut point = 0;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(48..=57) => self.peek().unwrap() - 48,
                Some(65..=70) => self.peek().unwrap() - 55,
                Some(97..=102) => self.peek().unwrap() - 87,
                _ => return Err(error("Invalid \\uXXXX escape", start.saturating_sub(1))),
            };
            point = (point << 4) | digit;
            self.index += 1;
        }
        Ok(point)
    }
    fn string(&mut self) -> Result<PyString, JsonError> {
        let start = self.index;
        self.index += 1;
        let mut result = Vec::new();
        while let Some(point) = self.peek() {
            self.index += 1;
            match point {
                34 => return Ok(PyString(result)),
                0..=31 => return Err(error("Invalid control character at", self.index - 1)),
                92 => {
                    let escaped = self
                        .peek()
                        .ok_or_else(|| error("Unterminated string starting at", start))?;
                    self.index += 1;
                    match escaped {
                        34 | 47 | 92 => result.push(escaped),
                        98 => result.push(8),
                        102 => result.push(12),
                        110 => result.push(10),
                        114 => result.push(13),
                        116 => result.push(9),
                        117 => {
                            let mut decoded = self.hex4()?;
                            if (0xd800..=0xdbff).contains(&decoded)
                                && self.chars.get(self.index..self.index + 2) == Some(&[92, 117])
                            {
                                let saved = self.index;
                                self.index += 2;
                                match self.hex4() {
                                    Ok(low) if (0xdc00..=0xdfff).contains(&low) => {
                                        decoded =
                                            0x10000 + ((decoded - 0xd800) << 10) + (low - 0xdc00)
                                    }
                                    _ => self.index = saved,
                                }
                            }
                            result.push(decoded);
                        }
                        _ => return Err(error("Invalid \\escape", self.index - 2)),
                    }
                }
                _ => result.push(point),
            }
        }
        Err(error("Unterminated string starting at", start))
    }
    fn number(&mut self) -> Result<Value, JsonError> {
        let start = self.index;
        self.take(45);
        if !self.take(48) {
            if !matches!(self.peek(), Some(49..=57)) {
                self.index = start;
                return Err(error("Expecting value", start));
            }
            while matches!(self.peek(), Some(48..=57)) {
                self.index += 1;
            }
        }
        let mut floating = false;
        if self.peek() == Some(46) && matches!(self.chars.get(self.index + 1), Some(48..=57)) {
            floating = true;
            self.index += 1;
            while matches!(self.peek(), Some(48..=57)) {
                self.index += 1;
            }
        }
        if matches!(self.peek(), Some(69 | 101)) {
            let saved = self.index;
            self.index += 1;
            if matches!(self.peek(), Some(43 | 45)) {
                self.index += 1;
            }
            if matches!(self.peek(), Some(48..=57)) {
                floating = true;
                while matches!(self.peek(), Some(48..=57)) {
                    self.index += 1;
                }
            } else {
                self.index = saved;
            }
        }
        let token: String = self.chars[start..self.index]
            .iter()
            .map(|&point| char::from_u32(point).unwrap())
            .collect();
        if floating {
            Ok(Value::Float(
                token.parse().map_err(|_| error("Expecting value", start))?,
            ))
        } else {
            let digits = token.len() - usize::from(token.starts_with('-'));
            if digits > 4300 {
                return Err(JsonError{kind:"ValueError",message:format!("Exceeds the limit (4300 digits) for integer string conversion: value has {digits} digits; use sys.set_int_max_str_digits() to increase the limit"),position:start});
            }
            Ok(Value::Integer(
                BigInt::from_str(&token).map_err(|_| error("Expecting value", start))?,
            ))
        }
    }
}

#[derive(Clone, Copy)]
pub struct SerializationProfile {
    pub ensure_ascii: bool,
    pub sorted: bool,
    pub spaced: bool,
}
impl SerializationProfile {
    pub const CANONICAL: Self = Self {
        ensure_ascii: false,
        sorted: true,
        spaced: false,
    };
    pub const CLI: Self = Self {
        ensure_ascii: true,
        sorted: true,
        spaced: false,
    };
    pub const STORED: Self = Self {
        ensure_ascii: true,
        sorted: false,
        spaced: true,
    };
    pub const SORTED_STORED: Self = Self {
        ensure_ascii: true,
        sorted: true,
        spaced: true,
    };
}
pub fn dumps(value: &Value, profile: SerializationProfile) -> Result<Vec<u8>, JsonError> {
    let mut output = JsonText(Vec::new());
    serialize(value, profile, &mut output)?;
    output.into_bytes()
}
pub fn dumps_text(value: &Value, profile: SerializationProfile) -> Result<PyString, JsonError> {
    let mut output = JsonText(Vec::new());
    serialize(value, profile, &mut output)?;
    Ok(PyString(output.0))
}
pub fn encode_utf8(text: &PyString) -> Result<Vec<u8>, JsonError> {
    JsonText(text.0.clone()).into_bytes()
}
struct JsonText(Vec<u32>);
impl JsonText {
    fn push(&mut self, character: char) {
        self.0.push(u32::from(character));
    }
    fn push_point(&mut self, point: u32) {
        self.0.push(point);
    }
    fn push_str(&mut self, text: &str) {
        self.0.extend(text.chars().map(u32::from));
    }
    fn into_bytes(self) -> Result<Vec<u8>, JsonError> {
        let mut bytes = Vec::new();
        for (index, &point) in self.0.iter().enumerate() {
            let character = char::from_u32(point).ok_or_else(|| {
                let length = self.0[index..]
                    .iter()
                    .take_while(|&&p| (0xd800..=0xdfff).contains(&p))
                    .count();
                let detail = if length == 1 {
                    format!("character '\\u{point:04x}' in position {index}")
                } else {
                    format!("characters in position {index}-{}", index + length - 1)
                };
                JsonError {
                    kind: "UnicodeEncodeError",
                    message: format!("'utf-8' codec can't encode {detail}: surrogates not allowed"),
                    position: index,
                }
            })?;
            let mut buffer = [0; 4];
            bytes.extend(character.encode_utf8(&mut buffer).as_bytes());
        }
        Ok(bytes)
    }
}
impl fmt::Write for JsonText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.push_str(text);
        Ok(())
    }
}
enum WriteTask<'a> {
    Value(&'a Value, usize),
    Quoted(&'a PyString),
    Text(&'static str),
}
fn serialize(
    value: &Value,
    profile: SerializationProfile,
    out: &mut JsonText,
) -> Result<(), JsonError> {
    let comma = if profile.spaced { ", " } else { "," };
    let colon = if profile.spaced { ": " } else { ":" };
    let mut tasks = vec![WriteTask::Value(value, 0)];
    while let Some(task) = tasks.pop() {
        match task {
            WriteTask::Text(text) => out.push_str(text),
            WriteTask::Quoted(text) => quoted(text, profile.ensure_ascii, out)?,
            WriteTask::Value(value, depth) => {
                if depth >= JSON_CONTAINER_LIMIT
                    && matches!(value, Value::List(_) | Value::Tuple(_) | Value::Object(_))
                {
                    return Err(JsonError {
                        kind: "RecursionError",
                        message: "maximum recursion depth exceeded while encoding a JSON object"
                            .into(),
                        position: 0,
                    });
                }
                match value {
                    Value::Null => out.push_str("null"),
                    Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
                    Value::Integer(i) => {
                        let text = i.to_string();
                        let digits = text.len() - usize::from(text.starts_with('-'));
                        if digits > 4300 {
                            return Err(JsonError{kind:"ValueError",message:"Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit".into(),position:0});
                        }
                        out.push_str(&text);
                    }
                    Value::Float(f) => out.push_str(&float_repr(*f)),
                    Value::ConstantNaN => out.push_str("NaN"),
                    Value::String(text) => quoted(text, profile.ensure_ascii, out)?,
                    Value::Bytes(_) => {
                        // CPython calls its Python default handler for bytes, consuming
                        // more recursion budget than an ordinary JSON scalar.
                        let (kind, message) = if depth == JSON_CONTAINER_LIMIT - 2 {
                            (
                                "RecursionError",
                                "maximum recursion depth exceeded while calling a Python object",
                            )
                        } else if depth >= JSON_CONTAINER_LIMIT - 1 {
                            ("RecursionError", "maximum recursion depth exceeded")
                        } else {
                            ("TypeError", "Object of type bytes is not JSON serializable")
                        };
                        return Err(JsonError {
                            kind,
                            message: message.into(),
                            position: 0,
                        });
                    }
                    Value::List(items) | Value::Tuple(items) => {
                        out.push('[');
                        tasks.push(WriteTask::Text("]"));
                        for (index, item) in items.iter().enumerate().rev() {
                            tasks.push(WriteTask::Value(item, depth + 1));
                            if index > 0 {
                                tasks.push(WriteTask::Text(comma));
                            }
                        }
                    }
                    Value::Object(items) => {
                        let mut ordered: Vec<_> = items.iter().collect();
                        if profile.sorted {
                            ordered.sort_by(|(a, _), (b, _)| a.cmp(b));
                        }
                        out.push('{');
                        tasks.push(WriteTask::Text("}"));
                        for (index, (key, value)) in ordered.into_iter().enumerate().rev() {
                            tasks.push(WriteTask::Value(value, depth + 1));
                            tasks.push(WriteTask::Text(colon));
                            tasks.push(WriteTask::Quoted(key));
                            if index > 0 {
                                tasks.push(WriteTask::Text(comma));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
fn quoted(text: &PyString, ascii: bool, out: &mut JsonText) -> Result<(), JsonError> {
    use std::fmt::Write;
    out.push('"');
    for &point in &text.0 {
        match point {
            34 => out.push_str("\\\""),
            92 => out.push_str("\\\\"),
            8 => out.push_str("\\b"),
            12 => out.push_str("\\f"),
            10 => out.push_str("\\n"),
            13 => out.push_str("\\r"),
            9 => out.push_str("\\t"),
            0..=31 => {
                write!(out, "\\u{point:04x}").unwrap();
            }
            32..=126 => out.push(char::from_u32(point).unwrap()),
            127..=0xffff if ascii => {
                write!(out, "\\u{point:04x}").unwrap();
            }
            0x10000..=0x10ffff if ascii => {
                let adjusted = point - 0x10000;
                write!(
                    out,
                    "\\u{:04x}\\u{:04x}",
                    0xd800 + (adjusted >> 10),
                    0xdc00 + (adjusted & 0x3ff)
                )
                .unwrap();
            }
            _ => out.push_point(point),
        }
    }
    out.push('"');
    Ok(())
}

pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == f64::INFINITY {
        return "Infinity".into();
    }
    if value == f64::NEG_INFINITY {
        return "-Infinity".into();
    }
    if value == 0. {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .into();
    }
    let negative = value.is_sign_negative();
    let mut buffer = ryu::Buffer::new();
    let raw = buffer.format_finite(value.abs());
    let (coefficient, exponent) = raw
        .split_once('e')
        .map(|(c, e)| (c, e.parse::<i32>().unwrap()))
        .unwrap_or((raw, 0));
    let decimal = coefficient.find('.').unwrap_or(coefficient.len()) as i32;
    let mut digits: String = coefficient.chars().filter(|c| *c != '.').collect();
    let leading = digits.bytes().take_while(|b| *b == b'0').count();
    digits.drain(..leading);
    let scientific = exponent + decimal - leading as i32 - 1;
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    let mut result = if negative {
        "-".to_owned()
    } else {
        String::new()
    };
    if !(-4..16).contains(&scientific) {
        result.push(digits.as_bytes()[0] as char);
        if digits.len() > 1 {
            result.push('.');
            result.push_str(&digits[1..]);
        }
        result.push('e');
        result.push(if scientific >= 0 { '+' } else { '-' });
        result.push_str(&format!("{:02}", scientific.abs()));
    } else {
        let split = scientific + 1;
        if split <= 0 {
            result.push_str("0.");
            result.push_str(&"0".repeat((-split) as usize));
            result.push_str(&digits);
        } else if split as usize >= digits.len() {
            result.push_str(&digits);
            result.push_str(&"0".repeat(split as usize - digits.len()));
            result.push_str(".0");
        } else {
            result.push_str(&digits[..split as usize]);
            result.push('.');
            result.push_str(&digits[split as usize..]);
        }
    }
    result
}
