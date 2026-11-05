use num_bigint::BigInt;
use num_traits::{FromPrimitive, ToPrimitive, Zero};
use std::{fmt, ops::Index};

#[derive(Clone, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PyString(pub Vec<u32>);

impl PyString {
    pub fn utf8(&self) -> Result<String, usize> {
        self.0
            .iter()
            .enumerate()
            .map(|(index, &point)| char::from_u32(point).ok_or(index))
            .collect()
    }
    pub fn equals(&self, text: &str) -> bool {
        self.0.iter().copied().eq(text.chars().map(u32::from))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.0
            .starts_with(&prefix.chars().map(u32::from).collect::<Vec<_>>())
    }
    pub fn ends_with(&self, suffix: &str) -> bool {
        self.0
            .ends_with(&suffix.chars().map(u32::from).collect::<Vec<_>>())
    }
    pub fn contains(&self, text: &str) -> bool {
        self.find(text).is_some()
    }
    pub fn find(&self, text: &str) -> Option<usize> {
        let points: Vec<_> = text.chars().map(u32::from).collect();
        if points.is_empty() {
            return Some(0);
        }
        self.0
            .windows(points.len())
            .position(|window| window == points)
    }
    pub fn split_once(&self, text: &str) -> Option<(Self, Self)> {
        let index = self.find(text)?;
        Some((
            Self(self.0[..index].to_vec()),
            Self(self.0[index + text.chars().count()..].to_vec()),
        ))
    }
}
impl From<&str> for PyString {
    fn from(s: &str) -> Self {
        Self(s.chars().map(u32::from).collect())
    }
}
impl From<String> for PyString {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
impl fmt::Debug for PyString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PyString").field(&self.0).finish()
    }
}

#[derive(Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(BigInt),
    Float(f64),
    ConstantNaN,
    String(PyString),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Object(Vec<(PyString, Value)>),
}

impl Clone for Value {
    fn clone(&self) -> Self {
        enum Task<'a> {
            Visit(&'a Value),
            List(usize),
            Tuple(usize),
            Object(Vec<PyString>),
        }
        let mut tasks = vec![Task::Visit(self)];
        let mut values = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Visit(value) => match value {
                    Value::Null => values.push(Value::Null),
                    Value::Bool(v) => values.push(Value::Bool(*v)),
                    Value::Integer(v) => values.push(Value::Integer(v.clone())),
                    Value::Float(v) => values.push(Value::Float(*v)),
                    Value::ConstantNaN => values.push(Value::ConstantNaN),
                    Value::String(v) => values.push(Value::String(v.clone())),
                    Value::Bytes(v) => values.push(Value::Bytes(v.clone())),
                    Value::List(items) => {
                        tasks.push(Task::List(items.len()));
                        for item in items.iter().rev() {
                            tasks.push(Task::Visit(item));
                        }
                    }
                    Value::Tuple(items) => {
                        tasks.push(Task::Tuple(items.len()));
                        for item in items.iter().rev() {
                            tasks.push(Task::Visit(item));
                        }
                    }
                    Value::Object(items) => {
                        tasks.push(Task::Object(
                            items.iter().map(|(key, _)| key.clone()).collect(),
                        ));
                        for (_, item) in items.iter().rev() {
                            tasks.push(Task::Visit(item));
                        }
                    }
                },
                Task::List(length) => {
                    let items = values.split_off(values.len() - length);
                    values.push(Value::List(items));
                }
                Task::Tuple(length) => {
                    let items = values.split_off(values.len() - length);
                    values.push(Value::Tuple(items));
                }
                Task::Object(keys) => {
                    let items = values.split_off(values.len() - keys.len());
                    values.push(Value::Object(keys.into_iter().zip(items).collect()));
                }
            }
        }
        values.pop().unwrap()
    }
}
impl Drop for Value {
    fn drop(&mut self) {
        fn children(value: &mut Value, pending: &mut Vec<Value>) {
            match value {
                Value::List(items) | Value::Tuple(items) => pending.append(items),
                Value::Object(items) => pending.extend(items.drain(..).map(|(_, item)| item)),
                _ => (),
            }
        }
        let mut pending = Vec::new();
        children(self, &mut pending);
        while let Some(mut value) = pending.pop() {
            children(&mut value, &mut pending);
        }
    }
}

pub static NULL: Value = Value::Null;

impl Value {
    pub fn object<const N: usize>(items: [(&str, Value); N]) -> Self {
        Self::Object(
            items
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }
    pub fn string(s: impl Into<PyString>) -> Self {
        Self::String(s.into())
    }
    pub fn list(items: impl IntoIterator<Item = Value>) -> Self {
        Self::List(items.into_iter().collect())
    }
    pub fn tuple(items: impl IntoIterator<Item = Value>) -> Self {
        Self::Tuple(items.into_iter().collect())
    }
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(items) => items
                .iter()
                .find(|(k, _)| k.equals(key))
                .map(|(_, value)| value),
            _ => None,
        }
    }
    pub fn set(&mut self, key: &str, value: Self) {
        if let Self::Object(items) = self {
            if let Some((_, item)) = items.iter_mut().find(|(k, _)| k.equals(key)) {
                *item = value;
            } else {
                items.push((key.into(), value));
            }
        }
    }
    pub fn remove(&mut self, key: &str) -> Option<Self> {
        if let Self::Object(items) = self {
            let index = items.iter().position(|(k, _)| k.equals(key))?;
            Some(items.remove(index).1)
        } else {
            None
        }
    }
    pub fn as_string(&self) -> Option<&PyString> {
        if let Self::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    pub fn text(&self) -> Option<String> {
        self.as_string()?.utf8().ok()
    }
    pub fn is_text(&self, text: &str) -> bool {
        matches!(self, Self::String(s) if s.equals(text))
    }
    pub fn as_list(&self) -> Option<&[Self]> {
        if let Self::List(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_object(&self) -> Option<&[(PyString, Self)]> {
        if let Self::Object(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn strict_integer(&self) -> Option<&BigInt> {
        if let Self::Integer(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        self.strict_integer()?.to_i64()
    }
    pub fn is_true(&self) -> bool {
        matches!(self, Self::Bool(true))
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::Integer(i) => !i.is_zero(),
            Self::Float(f) => *f != 0.,
            Self::ConstantNaN => true,
            Self::String(s) => !s.is_empty(),
            Self::Bytes(v) => !v.is_empty(),
            Self::List(v) | Self::Tuple(v) => !v.is_empty(),
            Self::Object(v) => !v.is_empty(),
        }
    }
    pub fn py_eq(&self, rhs: &Self) -> bool {
        let mut pending = vec![(self, rhs, false)];
        while let Some((left, right, identity)) = pending.pop() {
            if identity
                && (std::ptr::eq(left, right)
                    || matches!((left, right), (Self::ConstantNaN, Self::ConstantNaN)))
            {
                continue;
            }
            let equal = match (left, right) {
                (Self::Null, Self::Null) => true,
                (Self::Bool(a), Self::Bool(b)) => a == b,
                (Self::Integer(a), Self::Integer(b)) => a == b,
                (Self::Float(a), Self::Float(b)) => a == b,
                (Self::Bool(a), Self::Integer(b)) | (Self::Integer(b), Self::Bool(a)) => {
                    b == &BigInt::from(u8::from(*a))
                }
                (Self::Bool(a), Self::Float(b)) | (Self::Float(b), Self::Bool(a)) => {
                    *b == f64::from(u8::from(*a))
                }
                (Self::Integer(a), Self::Float(b)) | (Self::Float(b), Self::Integer(a)) => {
                    b.is_finite()
                        && b.fract() == 0.
                        && BigInt::from_f64(*b).is_some_and(|i| &i == a)
                }
                (Self::String(a), Self::String(b)) => a == b,
                (Self::Bytes(a), Self::Bytes(b)) => a == b,
                (Self::List(a), Self::List(b)) | (Self::Tuple(a), Self::Tuple(b)) => {
                    if a.len() != b.len() {
                        return false;
                    }
                    pending.extend(a.iter().zip(b).map(|(x, y)| (x, y, true)));
                    true
                }
                (Self::Object(a), Self::Object(b)) => {
                    if a.len() != b.len() {
                        return false;
                    }
                    for (key, value) in a {
                        let Some((_, other)) = b.iter().find(|(candidate, _)| candidate == key)
                        else {
                            return false;
                        };
                        pending.push((value, other, true));
                    }
                    true
                }
                _ => false,
            };
            if !equal {
                return false;
            }
        }
        true
    }
    pub fn container_eq(&self, rhs: &Self) -> bool {
        std::ptr::eq(self, rhs)
            || matches!((self, rhs), (Self::ConstantNaN, Self::ConstantNaN))
            || self.py_eq(rhs)
    }
    pub fn hashable(&self) -> bool {
        let mut pending = vec![self];
        while let Some(value) = pending.pop() {
            match value {
                Self::List(_) | Self::Object(_) => return false,
                Self::Tuple(items) => pending.extend(items.iter()),
                _ => (),
            }
        }
        true
    }
    pub fn python_type(&self) -> &'static str {
        match self {
            Self::Null => "NoneType",
            Self::Bool(_) => "bool",
            Self::Integer(_) => "int",
            Self::Float(_) | Self::ConstantNaN => "float",
            Self::String(_) => "str",
            Self::Bytes(_) => "bytes",
            Self::List(_) => "list",
            Self::Tuple(_) => "tuple",
            Self::Object(_) => "dict",
        }
    }
}
impl Index<&str> for Value {
    type Output = Value;
    fn index(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::string(v)
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::string(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Integer(v.into())
    }
}
impl From<usize> for Value {
    fn from(v: usize) -> Self {
        Self::Integer(v.into())
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
